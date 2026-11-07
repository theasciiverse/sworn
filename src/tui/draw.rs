//! Drawing the game screen: the map on the left, a tabbed sidebar on the
//! right, the status line at the bottom, and whatever popup is open.

use super::map::{self, noble_color, owner_color, MapMarks};
use super::play::{Field, Play, Popup, Tab};
use crate::chronicle::Visibility;
use crate::diplomacy::{Offer, Term};
use crate::orders::Order;
use crate::view::View;
use crate::{is_winter, season_name, NobleId};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

const SIDEBAR: u16 = 46;
const DIM: Style = Style::new().fg(Color::Indexed(245));
const KEY: Style = Style::new().fg(Color::Indexed(220));

fn key(k: &str) -> Span<'static> {
    Span::styled(k.to_string(), KEY)
}

fn name_span(v: &View, n: NobleId) -> Span<'static> {
    Span::styled(v.noble(n).name.clone(), Style::new().fg(noble_color(n)).add_modifier(Modifier::BOLD))
}

pub fn terms_text(v: &View, ts: &[Term]) -> String {
    if ts.is_empty() {
        return "nothing".into();
    }
    ts.iter()
        .map(|t| match t {
            Term::Gold(g) => format!("{g} gold"),
            Term::Province(p) => v.province(*p).name.clone(),
            Term::Fealty => "fealty".into(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn offer_text(v: &View, o: &Offer) -> String {
    let from = &v.noble(o.from).name;
    let to = &v.noble(o.to).name;
    let mut s = format!("{from} gives {}; {to} gives {}.", terms_text(v, &o.give), terms_text(v, &o.want));
    if o.give.contains(&Term::Fealty) {
        s.push_str(&format!(" {from} would swear to {to}."));
    }
    if o.want.contains(&Term::Fealty) {
        s.push_str(&format!(" {to} would swear to {from}."));
    }
    s
}

pub fn order_text(v: &View, o: &Order) -> String {
    let p = |id| v.province(id).name.clone();
    match o {
        Order::Raise { at, count } => format!("Raise {count} at {}", p(*at)),
        Order::March { from, to, count, on_behalf: None } => {
            let verb = if v.province(*to).owner == Some(v.me) { "Move" } else { "Attack" };
            format!("{verb}: {count} from {} to {}", p(*from), p(*to))
        }
        Order::March { from, to, count, on_behalf: Some(a) } => {
            format!("For {}: {count} from {} to {}", v.noble(*a).name, p(*from), p(*to))
        }
        Order::Accept(id) => match v.offers.iter().find(|x| x.id == *id) {
            Some(x) => format!("Accept {}'s offer", v.noble(x.from).name),
            None => "Accept an offer (no longer open)".into(),
        },
        Order::Renounce => "Renounce your oath".into(),
        Order::Release(n) => format!("Release {} from their oath", v.noble(*n).name),
    }
}

pub fn draw(f: &mut Frame, play: &mut Play) {
    let area = f.area();
    let [main, status] = Layout::vertical([Constraint::Min(5), Constraint::Length(2)]).areas(area);
    let side_w = SIDEBAR.min(main.width / 2);
    let [map_area, side] = Layout::horizontal([Constraint::Min(10), Constraint::Length(side_w)]).areas(main);
    play.sidebar = side;

    let v = play.court.view();
    let title = format!(" The Realm · {} ", season_name(v.season));
    let block = Block::bordered().border_type(BorderType::Rounded).title(title).border_style(DIM);
    let inner = block.inner(map_area);
    f.render_widget(block, map_area);

    let marches = play.marches();
    let (targets, target) = match &play.popup {
        Some(Popup::March { dests, di, .. }) => (dests.clone(), dests.get(*di).copied()),
        _ => (vec![], None),
    };
    let marks = MapMarks { sel: play.sel, targets: &targets, target, marches: &marches };
    let mut hits = std::mem::take(&mut play.hits);
    map::draw(f.buffer_mut(), inner, play.court.view(), &marks, &mut hits);
    play.hits = hits;

    draw_sidebar(f, side, play);
    draw_status(f, status, play);
    draw_popup(f, map_area, play);
}

fn draw_sidebar(f: &mut Frame, area: Rect, play: &Play) {
    let mut titles: Vec<Span> = vec![];
    for (i, t) in Tab::ALL.iter().enumerate() {
        let style = if *t == play.tab { Style::new().fg(Color::Black).bg(Color::Indexed(220)) } else { DIM };
        titles.push(Span::styled(format!("{}{}", i + 1, &t.name()[..3.min(t.name().len())]), style));
        titles.push(Span::raw(" "));
    }
    let block = Block::bordered().border_type(BorderType::Rounded).title(Line::from(titles)).border_style(DIM);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let sel = play.list_sel[play.tab as usize];
    let (text, scroll) = match play.tab {
        Tab::Province => (province_panel(play), 0),
        Tab::Orders => orders_panel(play, sel),
        Tab::Letters => letters_panel(play, sel),
        Tab::Nobles => nobles_panel(play, sel),
        Tab::Chronicle => chronicle_panel(play, sel, inner.height),
    };
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }).scroll((scroll, 0)), inner);
}

fn province_panel(play: &Play) -> Text<'static> {
    let v = play.court.view();
    let p = v.province(play.sel);
    let mut lines: Vec<Line> = vec![];
    let color = owner_color(p.owner);
    lines.push(Line::from(vec![
        Span::styled(format!("{} {}", map::glyph(p.kind), p.name), Style::new().fg(color).bold()),
        Span::styled(format!("  {}", p.kind.name()), DIM),
    ]));
    if let crate::map::Kind::Seat(fac) = p.kind {
        lines.push(Line::styled(format!("Whoever would be King of the {} must hold it.", fac.name()), DIM));
    }
    match p.owner {
        Some(o) => {
            let n = v.noble(o);
            lines.push(Line::from(vec![
                Span::raw("Held by "),
                name_span(v, o),
                Span::styled(format!(", {} of the {}", n.title.name(), n.faction.name()), DIM),
            ]));
        }
        None => lines.push(Line::styled("Unclaimed; local levies hold it.", DIM)),
    }
    let troops = match p.troops {
        Some(t) => t.to_string(),
        None => "unknown".into(),
    };
    lines.push(Line::raw(format!(
        "Troops {troops} · income {} · walls {}%",
        p.kind.income(),
        p.kind.defence_pct()
    )));
    if p.owner == Some(v.me) {
        let (r, m) = (play.raised_at(p.id), play.marching_from(p.id));
        if r > 0 || m > 0 {
            lines.push(Line::styled(format!("Planned: +{r} raised, {m} marching out, {} stay", play.free_troops(p.id)), KEY));
        }
        lines.push(Line::from(vec![key("r"), Span::raw(" raise  "), key("m"), Span::raw(" march  "), key("n"), Span::raw(" next of yours")]));
    } else if p.owner.is_some() {
        lines.push(Line::from(vec![key("w"), Span::raw(" write to them")]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Roads to", DIM));
    for &q in &p.neighbours {
        let qp = v.province(q);
        let who = qp.owner.map_or_else(|| "unclaimed".to_string(), |o| v.noble(o).name.clone());
        let t = qp.troops.map_or_else(|| "?".into(), |t| t.to_string());
        lines.push(Line::from(vec![
            Span::styled(format!(" {} {:<11}", map::glyph(qp.kind), qp.name), Style::new().fg(owner_color(qp.owner))),
            Span::styled(format!("{t:>3}  {who}"), DIM),
        ]));
    }
    Text::from(lines)
}

fn orders_panel(play: &Play, sel: usize) -> (Text<'static>, u16) {
    let v = play.court.view();
    let mut lines: Vec<Line> = vec![];
    let spent = play.planned_gold();
    lines.push(Line::raw(format!("Gold {} · committed {spent} · left {}", v.gold, v.gold.saturating_sub(spent))));
    if is_winter(v.season) && !v.rules.winter_marches {
        lines.push(Line::styled("Winter: snow closes the roads, marches will not go.", KEY));
    }
    if v.submitted {
        lines.push(Line::styled("Sent. Waiting for the season to turn.", KEY));
    }
    lines.push(Line::raw(""));
    if play.draft.is_empty() {
        lines.push(Line::styled("No orders yet. Select a province of yours", DIM));
        lines.push(Line::styled("and press r to raise or m to march.", DIM));
    }
    for (i, o) in play.draft.iter().enumerate() {
        let style = if i == sel { Style::new().reversed() } else { Style::new() };
        lines.push(Line::styled(order_text(v, o), style));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![key("j/k"), Span::raw(" choose  "), key("x"), Span::raw(" remove  "), key("e"), Span::raw(" end season")]));
    (Text::from(lines), (sel as u16 + 4).saturating_sub(20))
}

fn letters_panel(play: &Play, sel: usize) -> (Text<'static>, u16) {
    let v = play.court.view();
    let letters = play.letters();
    let mut lines: Vec<Line> = vec![];
    lines.push(Line::from(vec![key("Enter"), Span::raw(" read "), key("a"), Span::raw(" accept "), key("r"), Span::raw(" reply "), key("w"), Span::raw(" write")]));
    if letters.is_empty() {
        lines.push(Line::styled("No letters yet.", DIM));
    }
    for (i, l) in letters.iter().enumerate() {
        let incoming = l.from != v.me;
        let who = if incoming { name_span(v, l.from) } else { Span::styled(format!("to {}", recipients(v, &l.to)), DIM) };
        let mut mark = "  ".to_string();
        if let Some(id) = l.offer {
            let open = v.offers.iter().any(|o| o.id == id);
            mark = if play.accepted(id) {
                "✓ ".into()
            } else if open {
                "⚖ ".into()
            } else {
                "· ".into()
            };
        }
        let first: String = l.body.lines().next().unwrap_or("").chars().take(60).collect();
        let head = Line::from(vec![
            Span::styled(mark, KEY),
            Span::styled(format!("{} ", short_season(l.season)), DIM),
            who,
        ]);
        let body = Line::styled(format!("   {first}"), DIM);
        if i == sel {
            lines.push(head.reversed());
            lines.push(body.reversed());
        } else {
            lines.push(head);
            lines.push(body);
        }
    }
    (Text::from(lines), (sel as u16 * 2).saturating_sub(16))
}

fn recipients(v: &View, to: &[NobleId]) -> String {
    let all_others = v.nobles.iter().filter(|n| n.alive && n.id != v.me).count();
    if to.len() > 1 && to.len() == all_others {
        return "all".into();
    }
    to.iter().map(|&n| v.noble(n).name.clone()).collect::<Vec<_>>().join(", ")
}

fn short_season(s: u32) -> String {
    let q = ["Sp", "Su", "Au", "Wi"][(s % 4) as usize];
    format!("{q}{}", crate::FIRST_YEAR + s / 4)
}

fn nobles_panel(play: &Play, sel: usize) -> (Text<'static>, u16) {
    let v = play.court.view();
    let total = v.provinces.len() as u32;
    let mut lines: Vec<Line> = vec![];
    for (i, n) in v.nobles.iter().enumerate() {
        let mut head = vec![
            Span::styled("■ ", Style::new().fg(noble_color(n.id))),
            name_span(v, n.id),
            Span::styled(format!("  {} of the {}", n.title.name(), n.faction.name()), DIM),
        ];
        if n.id == v.me {
            head.push(Span::styled(" (you)", KEY));
        }
        if !n.alive {
            head.push(Span::styled(" — fallen", DIM));
        }
        let mut detail = format!("   power {}/{total} · prestige {} · {} held", n.power, n.prestige, n.provinces);
        if let Some(l) = n.liege {
            detail.push_str(&format!(" · sworn to {}", v.noble(l).name));
        }
        if n.ai {
            detail.push_str(" · computer");
        }
        let (h, d) = (Line::from(head), Line::styled(detail, DIM));
        if i == sel {
            lines.push(h.reversed());
            lines.push(d.reversed());
        } else {
            lines.push(h);
            lines.push(d);
        }
    }
    lines.push(Line::raw(""));
    let me = v.noble(v.me);
    let r = &v.rules;
    lines.push(Line::styled("The crown of your people needs:", DIM));
    lines.push(Line::styled(
        format!(
            " no liege, your seat, power {}+, prestige {}+,",
            (r.king_share_pct * total).div_ceil(100),
            r.king_prestige
        ),
        DIM,
    ));
    lines.push(Line::styled(format!(" and every {} noble sworn under you.", me.faction.name()), DIM));
    lines.push(Line::styled(format!("You: {}", me.faction.blurb()), DIM));
    (Text::from(lines), (sel as u16 * 2).saturating_sub(16))
}

fn chronicle_panel(play: &Play, sel: usize, height: u16) -> (Text<'static>, u16) {
    let v = play.court.view();
    let mut lines: Vec<Line> = vec![];
    let mut last = u32::MAX;
    for e in &v.events {
        if e.season != last {
            last = e.season;
            lines.push(Line::styled(season_name(e.season), Style::new().bold()));
        }
        let style = if e.to == Visibility::Public { Style::new() } else { DIM };
        lines.push(Line::styled(format!(" {}", e.describe(v)), style));
    }
    if lines.is_empty() {
        lines.push(Line::styled("Nothing has happened yet.", DIM));
    }
    // j/k scrolls back from the newest.
    let back = sel.min(lines.len()) as u16;
    let n = lines.len() as u16;
    let scroll = n.saturating_sub(height.saturating_mul(2)).saturating_sub(back);
    (Text::from(lines), scroll)
}

fn draw_status(f: &mut Frame, area: Rect, play: &Play) {
    let v = play.court.view();
    let me = v.noble(v.me);
    let spent = play.planned_gold();
    let top = Line::from(vec![
        name_span(v, v.me),
        Span::styled(format!(" {} · ", me.title.name()), DIM),
        Span::raw(format!("{} gold", v.gold.saturating_sub(spent))),
        Span::styled(if spent > 0 { format!(" ({spent} committed)") } else { String::new() }, DIM),
        Span::raw(format!(" · power {} · prestige {} · {} orders", me.power, me.prestige, play.draft.len())),
        Span::styled(if v.submitted { " · sent" } else { "" }, KEY),
    ]);
    let mut top = top;
    let others: Vec<String> = v.waiting.iter().filter(|&&n| n != v.me).map(|&n| v.noble(n).name.clone()).collect();
    if v.submitted && !others.is_empty() {
        top.push_span(Span::styled(format!(" · waiting for {}", others.join(", ")), DIM));
    }
    let bottom = Line::from(vec![Span::styled(play.status.clone(), DIM)]);
    f.render_widget(Paragraph::new(vec![top, bottom]), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn bottom_of(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.bottom() - h, w, h)
}

fn boxed(f: &mut Frame, r: Rect, title: &str, text: Text, scroll: u16) {
    f.render_widget(Clear, r);
    let block = Block::bordered().border_type(BorderType::Rounded).title(format!(" {title} ")).border_style(KEY);
    f.render_widget(Paragraph::new(text).block(block).wrap(Wrap { trim: false }).scroll((scroll, 0)), r);
}

fn draw_popup(f: &mut Frame, map_area: Rect, play: &Play) {
    let full = f.area();
    let v = play.court.view();
    let Some(p) = &play.popup else { return };
    match p {
        Popup::Raise { at, count } => {
            let cost = play.raise_cost();
            let text = Text::from(vec![
                Line::raw(format!("Raise troops at {}", v.province(*at).name)),
                Line::from(vec![Span::raw("  ◀ "), Span::styled(format!("{count}"), KEY.bold()), Span::raw(" ▶")]),
                Line::styled(format!("Costs {} gold; at most {} more here.", count * cost, play.raise_room(*at)), DIM),
                Line::from(vec![key("←→"), Span::raw(" count  "), key("Enter"), Span::raw(" order  "), key("Esc"), Span::raw(" cancel")]),
            ]);
            boxed(f, bottom_of(map_area, 48, 6), "Raise", text, 0);
        }
        Popup::March { from, dests, di, count, ally } => {
            let to = v.province(dests[*di]);
            let what = match (ally, to.owner) {
                (Some(a), _) => vec![Span::raw("to fight for "), name_span(v, *a)],
                (None, Some(o)) if o == v.me => vec![Span::raw("to reinforce your own province")],
                (None, Some(o)) => vec![Span::styled("to attack ", Style::new().fg(Color::Indexed(203))), name_span(v, o)],
                (None, None) => vec![Span::raw("to take it from the local levies")],
            };
            let seen = to.troops.map_or_else(|| "unknown troops".into(), |t| format!("{t} troops"));
            let text = Text::from(vec![
                Line::from(vec![
                    Span::raw(format!("From {} to ", v.province(*from).name)),
                    Span::styled(to.name.clone(), Style::new().fg(owner_color(to.owner)).bold()),
                    Span::styled(format!(" ({seen}, walls {}%)", to.kind.defence_pct()), DIM),
                ]),
                Line::from(
                    [Span::raw("  ◀ "), Span::styled(format!("{count}"), KEY.bold()), Span::raw(format!(" ▶ of {}  ", play.free_troops(*from)))]
                        .into_iter()
                        .chain(what)
                        .collect::<Vec<_>>(),
                ),
                Line::from(vec![
                    key("↑↓"),
                    Span::raw(" where  "),
                    key("←→"),
                    Span::raw(" how many  "),
                    key("b"),
                    Span::raw(" on behalf of  "),
                    key("Enter"),
                    Span::raw(" order"),
                ]),
            ]);
            boxed(f, bottom_of(map_area, 64, 5), "March", text, 0);
        }
        Popup::Compose(c) => draw_compose(f, full, play, c),
        Popup::Read { letter, scroll } => {
            let Some(l) = v.letters.iter().find(|l| l.id == *letter) else { return };
            let mut lines = vec![
                Line::from(vec![Span::styled("From ", DIM), name_span(v, l.from), Span::styled(format!(", {}", season_name(l.season)), DIM)]),
                Line::from(vec![Span::styled("To ", DIM), Span::raw(recipients(v, &l.to))]),
                Line::raw(""),
            ];
            lines.extend(l.body.lines().map(|s| Line::raw(s.to_string())));
            if let Some(id) = l.offer {
                lines.push(Line::raw(""));
                match v.offers.iter().find(|o| o.id == id) {
                    Some(o) => {
                        lines.push(Line::styled(format!("⚖ Offer: {}", offer_text(v, o)), KEY));
                        if o.to == v.me {
                            let state = if play.accepted(id) { "You have accepted (press a to withdraw)." } else { "Press a to accept." };
                            lines.push(Line::styled(state, DIM));
                        }
                    }
                    None => lines.push(Line::styled("The offer it carried is closed.", DIM)),
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![key("r"), Span::raw(" reply  "), key("R"), Span::raw(" reply to all  "), key("any key"), Span::raw(" close")]));
            let h = (lines.len() as u16 + 4).clamp(8, 24);
            boxed(f, centered(full, 70, h), "Letter", Text::from(lines), *scroll);
        }
        Popup::Report { season, scroll } => {
            let mut lines: Vec<Line> = vec![];
            for e in v.events.iter().filter(|e| e.season == *season) {
                let style = if e.to == Visibility::Public { Style::new() } else { DIM };
                lines.push(Line::styled(format!("• {}", e.describe(v)), style));
            }
            if lines.is_empty() {
                lines.push(Line::styled("A quiet season.", DIM));
            }
            let fresh = v.letters.iter().filter(|l| l.season == v.season && l.from != v.me).count();
            if fresh > 0 {
                lines.push(Line::raw(""));
                lines.push(Line::styled(format!("{fresh} new letter{} waiting (tab 3).", if fresh == 1 { "" } else { "s" }), KEY));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled("Any key to continue.", DIM));
            let h = (lines.len() as u16 + 2).clamp(6, 26);
            boxed(f, centered(full, 80, h), &format!("{} is over", season_name(*season)), Text::from(lines), *scroll);
        }
        Popup::ConfirmEnd => {
            let mut lines = vec![Line::raw(format!("Send {} order{} and end {}?", play.draft.len(), if play.draft.len() == 1 { "" } else { "s" }, season_name(v.season)))];
            if play.draft.is_empty() {
                lines.push(Line::styled("You have given no orders: your troops will sit still.", DIM));
            }
            lines.push(Line::from(vec![key("y"), Span::raw(" yes  "), key("any key"), Span::raw(" not yet")]));
            boxed(f, centered(full, 56, 5), "End season", Text::from(lines), 0);
        }
        Popup::ConfirmQuit => {
            let text = Text::from(vec![
                Line::raw("Leave this game? It is saved, and can be continued."),
                Line::from(vec![key("y"), Span::raw(" leave  "), key("any key"), Span::raw(" stay")]),
            ]);
            boxed(f, centered(full, 56, 4), "Leave", text, 0);
        }
        Popup::Help => boxed(f, centered(full, 72, 26), "How to play", help_text(), 0),
        Popup::Over { metric } => super::over::draw(f, full, v, *metric),
    }
}

fn draw_compose(f: &mut Frame, full: Rect, play: &Play, c: &super::play::Compose) {
    let v = play.court.view();
    let field = c.field();
    let label = |fl: Field, s: &str| {
        if fl == field {
            Span::styled(format!("▸ {s}"), KEY.bold())
        } else {
            Span::styled(format!("  {s}"), DIM)
        }
    };
    let mut lines: Vec<Line> = vec![];
    let others = play.others();
    let mut to = vec![label(Field::To, "To: ")];
    for (i, &n) in others.iter().enumerate() {
        let on = c.to.contains(&n);
        let mut s = Style::new().fg(noble_color(n));
        if on {
            s = s.bold();
        } else {
            s = s.add_modifier(Modifier::DIM);
        }
        if field == Field::To && i == c.cursor % others.len().max(1) {
            s = s.reversed();
        }
        to.push(Span::styled(format!("{}{}", if on { "✓" } else { " " }, v.noble(n).name), s));
        to.push(Span::raw(" "));
    }
    lines.push(Line::from(to));
    lines.push(Line::raw(""));
    lines.push(Line::from(label(Field::Body, "Letter:")));
    let mut body: Vec<Line> = c.body.split('\n').map(|s| Line::raw(format!("    {s}"))).collect();
    if field == Field::Body {
        if let Some(last) = body.last_mut() {
            last.push_span(Span::styled("▏", KEY));
        }
    }
    let shown = body.len().saturating_sub(8);
    lines.extend(body.into_iter().skip(shown));
    lines.push(Line::raw(""));

    let mine: Vec<_> = v.mine().map(|p| p.id).collect();
    let theirs: Vec<_> = match c.to.as_slice() {
        [one] => v.provinces.iter().filter(|p| p.owner == Some(*one)).map(|p| p.id).collect(),
        _ => vec![],
    };
    for (fl, title, gold, provs, fealty, pool, cursor, swear) in [
        (Field::Give, "You give:", c.gold_give, &c.prov_give, c.fealty_give, &mine, c.give_cursor, "you swear fealty to them"),
        (Field::Want, "You ask:", c.gold_want, &c.prov_want, c.fealty_want, &theirs, c.want_cursor, "they swear fealty to you"),
    ] {
        let mut parts: Vec<String> = vec![];
        if gold > 0 {
            parts.push(format!("{gold} gold"));
        }
        parts.extend(provs.iter().map(|&p| v.province(p).name.clone()));
        if fealty {
            parts.push(swear.into());
        }
        let what = if parts.is_empty() { "nothing".into() } else { parts.join(", ") };
        lines.push(Line::from(vec![label(fl, title), Span::raw(format!(" {what}"))]));
        if fl == field {
            let pick = if pool.is_empty() {
                if fl == Field::Want { "(choose exactly one recipient to ask for land)".into() } else { String::new() }
            } else {
                let p = pool[cursor % pool.len()];
                format!("province: {} {}", v.province(p).name, if provs.contains(&p) { "(in)" } else { "" })
            };
            lines.push(Line::from(vec![
                Span::raw("    "),
                key("←→"),
                Span::raw(" gold "),
                key("↑↓"),
                Span::raw(" province "),
                key("Space"),
                Span::raw(" add/remove "),
                key("f"),
                Span::raw(" fealty"),
            ]));
            lines.push(Line::styled(format!("    {pick}"), DIM));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![label(Field::Send, "Send"), Span::styled(if c.has_offer() { "  (with a binding offer)" } else { "" }, DIM)]));
    if let Some(e) = &c.error {
        lines.push(Line::styled(e.clone(), Style::new().fg(Color::Indexed(203))));
    }
    lines.push(Line::from(vec![key("Tab"), Span::raw(" next field  "), key("Esc"), Span::raw(" discard")]));
    let r = centered(full, 78, lines.len() as u16 + 2);
    boxed(f, r, "Write a letter", Text::from(lines), 0);
}

fn help_text() -> Text<'static> {
    let row = |k: &str, s: &str| Line::from(vec![Span::styled(format!("{k:>10}  "), KEY), Span::raw(s.to_string())]);
    Text::from(vec![
        Line::raw("Rise from lesser noble to king of your people. Every season, all"),
        Line::raw("orders are secret and take effect at once. Letters can promise"),
        Line::raw("anything; only offers bind, and only if both can still pay."),
        Line::raw(""),
        row("arrows", "move around the map (or click)"),
        row("n / N", "next / previous province of yours"),
        row("r", "raise troops here"),
        row("m", "march from here; b fights on someone's behalf"),
        row("w", "write a letter, with an offer if you like"),
        row("Tab, 1-5", "sidebar: province, orders, letters, nobles, chronicle"),
        row("j / k", "move in the sidebar list"),
        row("a", "accept the selected letter's offer"),
        row("x", "remove the selected order"),
        row("e", "end the season    u  take your orders back"),
        row("s", "save    q  leave"),
        Line::raw(""),
        Line::raw("Battles: the strongest side wins and keeps troops in proportion"),
        Line::raw("to its margin; a tie leaves both bloodied. Walls multiply the"),
        Line::raw("defenders. Marching on your liege or vassal breaks your oath"),
        Line::raw("before the whole realm. No marching in winter."),
        Line::raw(""),
        Line::styled("Any key to close.", DIM),
    ])
}
