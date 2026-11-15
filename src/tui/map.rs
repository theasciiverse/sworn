//! The realm drawn as text: provinces as a glyph and a name with the troops
//! underneath, roads as dotted lines, planned marches as bright ones.

use crate::map::Kind;
use crate::view::View;
use crate::{NobleId, ProvinceId};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use std::collections::HashSet;

const PALETTE: [u8; 8] = [196, 39, 46, 226, 201, 51, 208, 141];
pub const NEUTRAL: Color = Color::Indexed(244);
pub const ROAD: Color = Color::Indexed(239);

pub fn noble_color(n: NobleId) -> Color {
    Color::Indexed(PALETTE[n as usize % PALETTE.len()])
}

pub fn owner_color(o: Option<NobleId>) -> Color {
    o.map_or(NEUTRAL, noble_color)
}

pub fn glyph(k: Kind) -> char {
    match k {
        Kind::Hold => '■',
        Kind::Town => '◆',
        Kind::Fortress => '▲',
        Kind::Seat(_) => '♛',
    }
}

/// What happened somewhere last season, flashed on the map once the report closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flash {
    /// A battle at a province, and whether it changed hands.
    Battle { at: ProvinceId, taken: bool },
    /// Columns met on the road between two provinces.
    Clash(ProvinceId, ProvinceId),
}

pub struct MapMarks<'a> {
    pub sel: ProvinceId,
    /// Provinces to point out (where a march can go).
    pub targets: &'a [ProvinceId],
    pub target: Option<ProvinceId>,
    /// Planned marches, drawn over the roads.
    pub marches: &'a [(ProvinceId, ProvinceId)],
    /// Last season's fighting, and whether this frame is the bright half of the blink.
    pub flashes: &'a [Flash],
    pub flash_on: bool,
}

/// Behind a province that changed hands: no noble's colour, since red is
/// the first noble's (often the player's own).
const FLASH_TAKEN: Color = Color::Indexed(124);
const FLASH_HELD: Color = Color::Indexed(231);
const FLASH_ROAD: Color = Color::Indexed(203);

/// Where each province's label sits in `area`, and how many name characters fit.
fn scaled(area: Rect, v: &View) -> Vec<(u16, u16)> {
    let n = v.provinces.len().max(1);
    let cols = ((n as f64 * 1.6).sqrt().ceil() as u16).max(1);
    let name_len = (area.width / cols).saturating_sub(3).clamp(3, 10) as usize;
    let label_w = name_len as u16 + 2;
    let w = area.width.saturating_sub(label_w).max(1) as u32;
    let h = area.height.saturating_sub(1).max(1) as u32;
    // Stretch the realm to fill the panel.
    let (x0, x1) = v.provinces.iter().fold((u16::MAX, 0), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (y0, y1) = v.provinces.iter().fold((u16::MAX, 0), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let (sx, sy) = ((x1.saturating_sub(x0)).max(1) as u32, (y1.saturating_sub(y0)).max(1) as u32);
    v.provinces
        .iter()
        .map(|p| {
            let x = (p.x - x0.min(p.x)) as u32 * w.saturating_sub(1) / sx;
            let y = (p.y - y0.min(p.y)) as u32 * h.saturating_sub(1) / sy;
            (area.x + x as u16, area.y + y as u16)
        })
        .collect()
}

/// Cells kept for a province's troop count under its name.
const TROOPS_W: u16 = 3;

/// How many name characters fit at `at` before the next glyph or troop
/// count to the right, or `None` if the glyph or troops would sit on
/// another province's.
fn room(area: Rect, at: (u16, u16), me: usize, pos: &[(u16, u16)]) -> Option<usize> {
    let (x, y) = at;
    let mut right = area.right().saturating_sub(1);
    for (j, &(qx, qy)) in pos.iter().enumerate() {
        if j == me {
            continue;
        }
        // Fixed footprints: the glyph, and the troop count under the name.
        let dx = qx as i32 - x as i32;
        let w = TROOPS_W as i32;
        let clash = (qy == y && dx.abs() < w)
            || (qy + 1 == y && (-w..0).contains(&dx))
            || (qy == y + 1 && (1..=w).contains(&dx));
        if clash {
            return None;
        }
        // What stops the name: a glyph on its row, or a troop count there.
        if qy == y && qx > x {
            right = right.min(qx.saturating_sub(1));
        }
        if qy + 1 == y && qx + 1 > x {
            right = right.min(qx);
        }
    }
    Some(right.saturating_sub(x + 1) as usize)
}

/// Each province's place and how much of its name shows: the scaled
/// position, nudged a cell or two where that lets more names show whole.
pub fn layout(area: Rect, v: &View) -> (Vec<(u16, u16)>, Vec<usize>) {
    type Key = (Rect, Vec<(u16, u16, usize)>);
    thread_local! {
        static LAST: std::cell::RefCell<Option<(Key, (Vec<(u16, u16)>, Vec<usize>))>> = const { std::cell::RefCell::new(None) };
    }
    let key: Key = (area, v.provinces.iter().map(|p| (p.x, p.y, p.name.len())).collect());
    if let Some(hit) = LAST.with_borrow(|l| l.as_ref().filter(|(k, _)| *k == key).map(|(_, out)| out.clone())) {
        return hit;
    }
    let out = place(area, v);
    LAST.with_borrow_mut(|l| *l = Some((key, out.clone())));
    out
}

fn place(area: Rect, v: &View) -> (Vec<(u16, u16)>, Vec<usize>) {
    let mut pos = scaled(area, v);
    let lens: Vec<usize> = v.provinces.iter().map(|p| p.name.chars().count()).collect();
    let (top, bottom) = (area.y, area.bottom().saturating_sub(2).max(area.y));
    // The first four letters tell a name apart, so they count the most.
    let worth = |pos: &[(u16, u16)], i: usize| -> i64 {
        let Some(r) = room(area, pos[i], i, pos) else { return -1000 };
        let s = r.min(lens[i]) as i64;
        12 * s.min(4) + 3 * (s - 4).max(0) + if r >= lens[i] { 6 } else { 0 }
    };
    let home = pos.clone();
    let mut moves: Vec<(i32, i32)> = vec![(0, 0)];
    for dy in -2i32..=2 {
        for dx in -3i32..=2 {
            if (dx, dy) != (0, 0) && (dy.abs() < 2 || dx.abs() < 2) {
                moves.push((dx, dy));
            }
        }
    }
    for _ in 0..4 {
        let mut changed = false;
        for i in 0..pos.len() {
            let mut best = (i64::MIN, pos[i]);
            for &(dx, dy) in &moves {
                let dy = if dy.abs() == 2 && area.height < 12 { 0 } else { dy };
                let (hx, hy) = home[i];
                let x = hx as i32 + dx;
                let y = hy as i32 + dy;
                if x < area.x as i32 || x >= area.right() as i32 - 1 || y < top as i32 || y > bottom as i32 {
                    continue;
                }
                pos[i] = (x as u16, y as u16);
                // Score this label and those it can crowd from any of its places.
                let (hx, hy) = (hx as i32, hy as i32);
                let near = |j: usize| {
                    let (jx, jy) = (pos[j].0 as i32, pos[j].1 as i32);
                    (jy - hy).abs() <= 5 && jx >= hx - 20 && jx <= hx + 18
                };
                let mut score: i64 = (0..pos.len()).filter(|&j| near(j)).map(|j| worth(&pos, j)).sum();
                score -= 2 * (dx.abs() + dy.abs()) as i64;
                if score > best.0 {
                    best = (score, pos[i]);
                }
            }
            changed |= pos[i] != best.1;
            pos[i] = best.1;
        }
        if !changed {
            break;
        }
    }
    let shown = (0..pos.len()).map(|i| room(area, pos[i], i, &pos).unwrap_or(0).min(lens[i])).collect();
    (pos, shown)
}

/// A name cut to `len` characters, keeping the first letter of its ending
/// ("Eskburg" in four is "Eskb", "Thornhithe" in five "Thorh") so that
/// names sharing a beginning still differ.
pub fn short_name(name: &str, len: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= len {
        return name.to_string();
    }
    if len >= 2 {
        if let Some(sfx) = SUFFIXES_FOR_SHORT.iter().find(|s| name.ends_with(*s) && name.len() > s.len()) {
            let stem = chars.len() - sfx.chars().count();
            if stem >= len {
                let mut out: String = chars[..len - 1].iter().collect();
                out.push(chars[stem]);
                return out;
            }
        }
    }
    chars[..len].iter().collect()
}

const SUFFIXES_FOR_SHORT: &[&str] = crate::map::SUFFIXES;

pub fn draw(buf: &mut Buffer, area: Rect, v: &View, marks: &MapMarks, hits: &mut Vec<(Rect, ProvinceId)>) {
    hits.clear();
    if area.width < 10 || area.height < 4 {
        return;
    }
    let (pos, lens) = layout(area, v);
    let mut taken: HashSet<(u16, u16)> = HashSet::new();
    for (i, &(x, y)) in pos.iter().enumerate() {
        for dx in 0..lens[i] as u16 + 1 {
            taken.insert((x + dx, y));
        }
        for dx in 0..TROOPS_W + 1 {
            taken.insert((x + dx, y + 1));
        }
    }
    let inside = |x: u16, y: u16| x >= area.x && y >= area.y && x < area.right() && y < area.bottom();
    let put = |buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style, over: bool| {
        if inside(x, y) && (over || !taken.contains(&(x, y))) {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(ch).set_style(style);
            }
        }
    };

    for p in &v.provinces {
        for &q in &p.neighbours {
            if q > p.id {
                line(pos[p.id as usize], pos[q as usize], |x, y| put(buf, x, y, '·', Style::new().fg(ROAD), false));
            }
        }
    }
    for &(a, b) in marks.marches {
        let style = Style::new().fg(Color::Indexed(220)).add_modifier(Modifier::BOLD);
        line(pos[a as usize], pos[b as usize], |x, y| put(buf, x, y, '•', style, false));
    }

    if marks.flash_on {
        for f in marks.flashes {
            let &Flash::Clash(a, b) = f else { continue };
            let style = Style::new().fg(FLASH_ROAD).add_modifier(Modifier::BOLD);
            let mut cells = vec![];
            line(pos[a as usize], pos[b as usize], |x, y| cells.push((x, y)));
            for &(x, y) in &cells {
                put(buf, x, y, '•', style, false);
            }
            let (pa, pb) = (pos[a as usize], pos[b as usize]);
            let mid = cells.get(cells.len() / 2).copied().unwrap_or(((pa.0 + pb.0) / 2 + 1, (pa.1 + pb.1) / 2));
            put(buf, mid.0, mid.1, '×', style, false);
        }
    }

    for p in &v.provinces {
        let (x, y) = pos[p.id as usize];
        let color = owner_color(p.owner);
        let mut name_style = Style::new().fg(color);
        if p.owner == Some(v.me) {
            name_style = name_style.add_modifier(Modifier::BOLD);
        }
        if marks.targets.contains(&p.id) {
            name_style = name_style.add_modifier(Modifier::UNDERLINED);
        }
        if marks.target == Some(p.id) {
            name_style = Style::new().fg(Color::Black).bg(Color::Indexed(220)).add_modifier(Modifier::BOLD);
        }
        if marks.sel == p.id {
            name_style = Style::new().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD);
        }
        let battle = marks.flashes.iter().find_map(|f| match *f {
            Flash::Battle { at, taken } if at == p.id => Some(taken),
            _ => None,
        });
        match battle {
            Some(taken) if marks.flash_on => {
                let style = if taken {
                    Style::new().fg(FLASH_HELD).bg(FLASH_TAKEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(FLASH_HELD).add_modifier(Modifier::BOLD)
                };
                put(buf, x, y, '✶', style, true);
            }
            _ => put(buf, x, y, glyph(p.kind), Style::new().fg(color), true),
        }
        let name = short_name(&p.name, lens[p.id as usize]);
        for (i, ch) in name.chars().enumerate() {
            put(buf, x + 1 + i as u16, y, ch, name_style, true);
        }
        let troops = p.troops.map_or_else(|| "?".to_string(), |t| t.to_string());
        let ts = Style::new().fg(if p.troops.is_some() { color } else { NEUTRAL }).add_modifier(Modifier::DIM);
        for (i, ch) in troops.chars().enumerate() {
            put(buf, x + 1 + i as u16, y + 1, ch, ts, true);
        }
        let w = (name.chars().count() as u16 + 1).min(area.right().saturating_sub(x));
        hits.push((Rect::new(x, y, w, 2).intersection(area), p.id));
    }
}

/// Bresenham, skipping both ends' label cells.
fn line((x0, y0): (u16, u16), (x1, y1): (u16, u16), mut plot: impl FnMut(u16, u16)) {
    let (mut x, mut y) = (x0 as i32 + 1, y0 as i32);
    let (x1, y1) = (x1 as i32 + 1, y1 as i32);
    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut steps = 0;
    loop {
        if steps > 0 && (x, y) != (x1, y1) && (steps % 2 == 0 || dy != 0) {
            plot(x as u16, y as u16);
        }
        if (x, y) == (x1, y1) || steps > 2000 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        steps += 1;
    }
}

/// The province nearest to `from` in a direction (dx, dy each -1, 0 or 1).
pub fn step(v: &View, from: ProvinceId, dx: i32, dy: i32) -> Option<ProvinceId> {
    let a = v.province(from);
    v.provinces
        .iter()
        .filter(|p| p.id != from)
        .filter_map(|p| {
            let (ex, ey) = (p.x as i32 - a.x as i32, p.y as i32 - a.y as i32);
            let along = ex * dx + ey * dy;
            if along <= 0 {
                return None;
            }
            let across = (ex * dy - ey * dx).abs();
            Some((along + 2 * across, p.id))
        })
        .min()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Faction;

    #[test]
    fn short_names_keep_their_ending_letter() {
        assert_eq!(short_name("Eskburg", 9), "Eskburg");
        assert_eq!(short_name("Eskburg", 5), "Eskbu");
        assert_eq!(short_name("Eskburg", 3), "Esb");
        assert_eq!(short_name("Thornhithe", 5), "Thorh");
        assert_eq!(short_name("Osby", 2), "Ob");
        assert_eq!(short_name("Nowhere", 3), "Now");
    }

    /// Glyphs and troop counts never overlap, and at 80x24 nearly every
    /// name shows whole and none drops below three letters.
    #[test]
    fn labels_fit_at_small_sizes() {
        for nobles in [2, 4, 6, 8] {
            for seed in 0..12 {
                let g = crate::tui::new_local_game("David", Faction::Saxons, nobles, seed).unwrap();
                let v = g.view(0);
                for (w, h) in [(48, 20), (58, 26), (100, 40)] {
                    let area = Rect::new(1, 1, w, h);
                    let (pos, lens) = layout(area, &v);
                    let mut whole = 0;
                    for (i, p) in v.provinces.iter().enumerate() {
                        assert!(room(area, pos[i], i, &pos).is_some(), "{nobles} nobles seed {seed} {w}x{h}: {} overlaps", p.name);
                        assert!(lens[i] >= 3, "{nobles}/{seed} {w}x{h}: {} shows {}", p.name, lens[i]);
                        whole += (lens[i] == p.name.chars().count()) as usize;
                    }
                    assert!(whole * 10 >= v.provinces.len() * 7, "{nobles}/{seed} {w}x{h}: {whole} of {} whole", v.provinces.len());
                }
            }
        }
    }
}
