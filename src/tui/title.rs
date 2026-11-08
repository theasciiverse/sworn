//! The title menu and the new-game setup.

use crate::faction::Faction;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

const BANNER: &[&str] = &[
    " ___ __      __  ___   ___  _  _ ",
    "/ __|\\ \\    / / / _ \\ | _ \\| \\| |",
    "\\__ \\ \\ \\/\\/ / | (_) ||   /| .` |",
    "|___/  \\_/\\_/   \\___/ |_|_\\|_|\\_|",
];
const GOLD: Color = Color::Indexed(220);
const DIM: Style = Style::new().fg(Color::Indexed(245));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Continue,
    Resume,
    New,
    Host,
    Join,
    Quit,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Choice::Continue => "Continue against the computer",
            Choice::Resume => "Resume the game you host",
            Choice::New => "New game against the computer",
            Choice::Host => "Host a game for others",
            Choice::Join => "Join a game",
            Choice::Quit => "Quit",
        }
    }
}

pub struct Title {
    pub items: Vec<Choice>,
    pub sel: usize,
    pub note: String,
}

impl Title {
    pub fn new(can_continue: bool, can_resume: bool) -> Title {
        let mut items = vec![];
        if can_continue {
            items.push(Choice::Continue);
        }
        if can_resume {
            items.push(Choice::Resume);
        }
        items.extend([Choice::New, Choice::Host, Choice::Join, Choice::Quit]);
        Title { items, sel: 0, note: String::new() }
    }

    pub fn key(&mut self, k: KeyEvent) -> Option<Choice> {
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = (self.sel + self.items.len() - 1) % self.items.len(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.sel = (self.sel + 1) % self.items.len(),
            KeyCode::Enter | KeyCode::Char(' ') => return Some(self.items[self.sel]),
            KeyCode::Char('q') | KeyCode::Esc => return Some(Choice::Quit),
            _ => {}
        }
        None
    }

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let mut lines: Vec<Line> = BANNER.iter().map(|l| Line::styled(*l, Style::new().fg(GOLD).bold())).collect();
        lines.push(Line::styled("multiplayer medieval politics for the terminal", DIM));
        lines.push(Line::raw(""));
        for (i, c) in self.items.iter().enumerate() {
            if i == self.sel {
                lines.push(Line::styled(format!("▸ {}", c.label()), Style::new().fg(GOLD).bold()));
            } else {
                lines.push(Line::raw(format!("  {}", c.label())));
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(self.note.clone(), DIM));
        let h = lines.len() as u16;
        let r = Rect::new(area.x, area.y + area.height.saturating_sub(h) / 2, area.width, h.min(area.height));
        f.render_widget(Paragraph::new(lines).centered(), r);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Name,
    Faction,
    Nobles,
    Start,
}

const ROWS: [Row; 4] = [Row::Name, Row::Faction, Row::Nobles, Row::Start];

pub struct Setup {
    pub name: String,
    pub faction: usize,
    pub nobles: usize,
    /// Hosting: other players fill seats first, computer nobles the rest.
    pub hosting: bool,
    row: usize,
}

pub enum SetupResult {
    Start,
    Back,
}

impl Setup {
    pub fn new(name: String, hosting: bool) -> Setup {
        Setup { name, faction: 0, nobles: 5, hosting, row: 0 }
    }

    pub fn faction(&self) -> Faction {
        Faction::ALL[self.faction % 4]
    }

    pub fn key(&mut self, k: KeyEvent) -> Option<SetupResult> {
        let row = ROWS[self.row];
        match k.code {
            KeyCode::Esc => return Some(SetupResult::Back),
            KeyCode::Up => self.row = (self.row + ROWS.len() - 1) % ROWS.len(),
            KeyCode::Down | KeyCode::Tab => self.row = (self.row + 1) % ROWS.len(),
            KeyCode::Enter if row == Row::Start || !self.name.trim().is_empty() && row != Row::Name => {
                if !self.name.trim().is_empty() {
                    return Some(SetupResult::Start);
                }
            }
            KeyCode::Enter => self.row = (self.row + 1) % ROWS.len(),
            KeyCode::Left | KeyCode::Right => {
                let d: i32 = if k.code == KeyCode::Left { -1 } else { 1 };
                match row {
                    Row::Faction => self.faction = (self.faction as i32 + d).rem_euclid(4) as usize,
                    Row::Nobles => self.nobles = (self.nobles as i32 + d).clamp(2, 8) as usize,
                    _ => {}
                }
            }
            KeyCode::Backspace if row == Row::Name => {
                self.name.pop();
            }
            KeyCode::Char(c) if row == Row::Name => {
                if self.name.chars().count() < 12 && (c.is_alphanumeric() || c == ' ' || c == '-' || c == '\'') {
                    self.name.push(c);
                }
            }
            _ => {}
        }
        None
    }

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let w = 64.min(area.width);
        let r = Rect::new(area.x + (area.width - w) / 2, area.y + area.height.saturating_sub(16) / 2, w, 16.min(area.height));
        let title = if self.hosting { " Host a game " } else { " A new game " };
        let block = Block::bordered().border_type(BorderType::Rounded).title(title).border_style(Style::new().fg(GOLD));
        let row = |i: usize, label: &str, value: Vec<Span<'static>>| {
            let mut spans = vec![if self.row == i {
                Span::styled(format!("▸ {label:<14}"), Style::new().fg(GOLD).bold())
            } else {
                Span::styled(format!("  {label:<14}"), DIM)
            }];
            spans.extend(value);
            Line::from(spans)
        };
        let fac = self.faction();
        let cursor = if self.row == 0 { "▏" } else { "" };
        let table = if self.hosting { "Seats" } else { "Nobles" };
        let mut lines = vec![
            row(0, "Your name", vec![Span::raw(self.name.clone()), Span::styled(cursor, Style::new().fg(GOLD))]),
            Line::raw(""),
            row(1, "Your people", vec![Span::raw(format!("◀ {} ▶", fac.name()))]),
            Line::styled(format!("                  {}", fac.blurb()), DIM),
            Line::raw(""),
            row(2, table, vec![Span::raw(format!("◀ {} ▶", self.nobles))]),
        ];
        if self.hosting {
            lines.push(Line::styled("                  Seats no one takes go to computer nobles.", DIM));
        } else {
            lines.push(Line::styled(format!("                  You and {} computer nobles.", self.nobles - 1), DIM));
        }
        lines.push(Line::raw(""));
        lines.push(row(3, "Begin", vec![]));
        lines.push(Line::raw(""));
        lines.push(Line::styled("  ↑↓ choose · ←→ change · Enter begin · Esc back", DIM));
        f.render_widget(Paragraph::new(Text::from(lines)).block(block), r);
    }
}
