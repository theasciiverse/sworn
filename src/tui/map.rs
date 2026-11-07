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

const FLASH_TAKEN: Color = Color::Indexed(196);
const FLASH_HELD: Color = Color::Indexed(231);
const FLASH_ROAD: Color = Color::Indexed(203);

/// Where each province's label sits in `area`, and how many name characters fit.
pub fn layout(area: Rect, v: &View) -> (Vec<(u16, u16)>, usize) {
    let n = v.provinces.len().max(1);
    let cols = ((n as f64 * 1.6).sqrt().ceil() as u16).max(1);
    let name_len = (area.width / cols).saturating_sub(3).clamp(3, 10) as usize;
    let label_w = name_len as u16 + 2;
    let w = area.width.saturating_sub(label_w).max(1) as u32;
    let h = area.height.saturating_sub(2).max(1) as u32;
    // Stretch the realm to fill the panel.
    let (x0, x1) = v.provinces.iter().fold((u16::MAX, 0), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (y0, y1) = v.provinces.iter().fold((u16::MAX, 0), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let (sx, sy) = ((x1.saturating_sub(x0)).max(1) as u32, (y1.saturating_sub(y0)).max(1) as u32);
    let pos = v
        .provinces
        .iter()
        .map(|p| {
            let x = (p.x - x0.min(p.x)) as u32 * w.saturating_sub(1) / sx;
            let y = (p.y - y0.min(p.y)) as u32 * h.saturating_sub(1) / sy;
            (area.x + x as u16, area.y + y as u16)
        })
        .collect();
    (pos, name_len)
}

pub fn draw(buf: &mut Buffer, area: Rect, v: &View, marks: &MapMarks, hits: &mut Vec<(Rect, ProvinceId)>) {
    hits.clear();
    if area.width < 10 || area.height < 4 {
        return;
    }
    let (pos, name_len) = layout(area, v);
    // Each name takes what room it has before the next label to its right.
    let lens: Vec<usize> = v
        .provinces
        .iter()
        .map(|p| {
            let (x, y) = pos[p.id as usize];
            let room = pos
                .iter()
                .filter(|&&(qx, qy)| qx > x && (qy == y || qy + 1 == y))
                .map(|&(qx, _)| (qx - x) as usize)
                .min()
                .unwrap_or((area.right() - x) as usize);
            let room = room.min((area.right() - x) as usize).saturating_sub(2);
            p.name.chars().count().min(room.max(name_len.min(3))).min(12)
        })
        .collect();
    let mut taken: HashSet<(u16, u16)> = HashSet::new();
    for (i, &(x, y)) in pos.iter().enumerate() {
        for dx in 0..lens[i] as u16 + 1 {
            taken.insert((x + dx, y));
        }
        for dx in 0..5 {
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
                let style = Style::new().fg(if taken { FLASH_TAKEN } else { FLASH_HELD }).add_modifier(Modifier::BOLD);
                put(buf, x, y, '✶', style, true);
            }
            _ => put(buf, x, y, glyph(p.kind), Style::new().fg(color), true),
        }
        let name: String = p.name.chars().take(lens[p.id as usize]).collect();
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
