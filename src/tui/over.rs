//! After the game: who stood where, and how the realm changed hands, as
//! charts drawn in the terminal.

use super::map::noble_color;
use crate::game::Stats;
use crate::view::View;
use crate::{season_name, NobleId};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Chart, Clear, Dataset, GraphType, Paragraph};
use ratatui::Frame;

type Metric = (&'static str, fn(&Stats) -> f64);

pub const METRICS: [Metric; 4] = [
    ("Power", |s| s.power as f64),
    ("Prestige", |s| s.prestige as f64),
    ("Troops", |s| s.troops as f64),
    ("Gold", |s| s.gold as f64),
];

/// Nobles from first to last: the winner, then the living by power and
/// prestige, then the fallen by how long they lasted.
pub fn ranking(v: &View) -> Vec<NobleId> {
    let lasted = |n: NobleId| v.history.iter().take_while(|h| h.get(n as usize).is_some_and(|s| s.provinces > 0)).count();
    let mut ids: Vec<NobleId> = v.nobles.iter().map(|n| n.id).collect();
    ids.sort_by_key(|&id| {
        let n = v.noble(id);
        (
            std::cmp::Reverse(v.winner == Some(id)),
            std::cmp::Reverse(n.alive),
            std::cmp::Reverse(n.power),
            std::cmp::Reverse(n.prestige),
            std::cmp::Reverse(lasted(id)),
            id,
        )
    });
    ids
}

pub fn draw(f: &mut Frame, area: Rect, v: &View, metric: usize) {
    let r = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), area.height.saturating_sub(2));
    f.render_widget(Clear, r);
    let title = match v.winner {
        Some(w) if v.winner == Some(v.me) => format!(" Long live the King! You rule the {} ", v.noble(w).faction.name()),
        Some(w) => format!(" {} rules the {} ", v.noble(w).name, v.noble(w).faction.name()),
        None => " The realm ".into(),
    };
    let block = Block::bordered().border_type(BorderType::Rounded).title(title.bold()).border_style(Style::new().fg(Color::Indexed(220)));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let rows = v.nobles.len() as u16 + 3;
    let [table, chart, help] = Layout::vertical([Constraint::Length(rows), Constraint::Min(6), Constraint::Length(1)]).areas(inner);

    let mut lines = vec![Line::styled(
        format!("{:>3}  {:<10} {:<10} {:<6} {:>5} {:>8} {:>6} {:>5}", "", "Noble", "People", "Title", "Power", "Prestige", "Troops", "Gold"),
        Style::new().add_modifier(Modifier::DIM),
    )];
    let last = v.history.last();
    for (rank, id) in ranking(v).into_iter().enumerate() {
        let n = v.noble(id);
        let s = last.and_then(|h| h.get(id as usize));
        let mut style = Style::new().fg(noble_color(id));
        if id == v.me {
            style = style.bold();
        }
        let fate = if v.winner == Some(id) {
            "  crowned"
        } else if !n.alive {
            "  fallen"
        } else if n.liege.is_some() {
            "  vassal"
        } else {
            ""
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{:>3}. {:<10} {:<10} {:<6} {:>5} {:>8} {:>6} {:>5}",
                    rank + 1,
                    n.name,
                    n.faction.name(),
                    n.title.name(),
                    n.power,
                    n.prestige,
                    s.map_or(0, |s| s.troops),
                    s.map_or(0, |s| s.gold)
                ),
                style,
            ),
            Span::styled(fate, Style::new().add_modifier(Modifier::DIM)),
        ]));
    }
    lines.push(Line::styled(format!("{} seasons, to {}.", v.history.len().saturating_sub(1), season_name(v.season.saturating_sub(1))), Style::new().add_modifier(Modifier::DIM)));
    f.render_widget(Paragraph::new(lines), table);

    let (name, get) = METRICS[metric % METRICS.len()];
    let series: Vec<(NobleId, Vec<(f64, f64)>)> = v
        .nobles
        .iter()
        .map(|n| (n.id, v.history.iter().enumerate().filter_map(|(t, h)| h.get(n.id as usize).map(|s| (t as f64, get(s)))).collect()))
        .collect();
    let max_y = series.iter().flat_map(|(_, s)| s.iter().map(|p| p.1)).fold(1.0f64, f64::max);
    let min_y = series.iter().flat_map(|(_, s)| s.iter().map(|p| p.1)).fold(0.0f64, f64::min);
    let max_x = (v.history.len().max(2) - 1) as f64;
    let datasets: Vec<Dataset> = series
        .iter()
        .map(|(id, pts)| {
            Dataset::default()
                .name(v.noble(*id).name.clone())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(noble_color(*id)))
                .data(pts)
        })
        .collect();
    let dim = Style::new().add_modifier(Modifier::DIM);
    let years = |t: f64| format!("{}", crate::FIRST_YEAR + t as u32 / 4);
    let chart_w = Chart::new(datasets)
        .block(Block::new().title(Line::from(vec![Span::styled(format!(" {name} "), Style::new().bold())])))
        .x_axis(Axis::default().style(dim).bounds([0.0, max_x]).labels(vec![years(0.0), years(max_x / 2.0), years(max_x)]))
        .y_axis(
            Axis::default()
                .style(dim)
                .bounds([min_y, max_y])
                .labels(vec![format!("{min_y:.0}"), format!("{:.0}", (min_y + max_y) / 2.0), format!("{max_y:.0}")]),
        );
    f.render_widget(chart_w, chart);
    let tabs: Vec<Span> = METRICS
        .iter()
        .enumerate()
        .map(|(i, (n, _))| if i == metric % METRICS.len() { Span::styled(format!(" {n} "), Style::new().fg(Color::Black).bg(Color::Indexed(220))) } else { Span::styled(format!(" {n} "), dim) })
        .chain([Span::styled("   Tab chart · Enter leave", dim)])
        .collect();
    f.render_widget(Paragraph::new(Line::from(tabs)), help);
}
