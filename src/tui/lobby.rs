//! Finding a game, and waiting at the table before it begins.

use crate::faction::Faction;
use crate::lan;
use crate::netcourt::{timer_name, HostLobby, JoinLobby};
use crate::proto::Seat;
use iroh::{EndpointAddr, EndpointId};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use ratatui::Frame;

const GOLD: Color = Color::Indexed(220);
const DIM: Style = Style::new().fg(Color::Indexed(245));

fn panel(f: &mut Frame, title: &str, lines: Vec<Line>) {
    let area = f.area();
    let w = 76.min(area.width);
    // rows as wrapped, so a long line (a host's id) does not push the last off
    let inner = w.saturating_sub(2).max(1) as usize;
    let rows: usize = lines.iter().map(|l| l.width().max(1).div_ceil(inner)).sum();
    let h = (rows as u16 + 2).min(area.height);
    let r = Rect::new(area.x + (area.width - w) / 2, area.y + area.height.saturating_sub(h) / 2, w, h);
    let block = Block::bordered().border_type(BorderType::Rounded).title(format!(" {title} ")).border_style(Style::new().fg(GOLD));
    f.render_widget(Paragraph::new(Text::from(lines)).block(block).wrap(Wrap { trim: false }), r);
}

fn key(k: &str) -> Span<'static> {
    Span::styled(k.to_string(), Style::new().fg(GOLD))
}

fn seat_lines(seats: &[Seat], size: u8, resumed: bool) -> Vec<Line<'static>> {
    let mut lines = vec![];
    for s in seats {
        let who = if s.host { " (host)" } else if s.empty { " (waiting to be reclaimed)" } else { "" };
        let style = if s.empty { DIM } else { Style::new().bold() };
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<12}", s.name), style),
            Span::styled(format!(" {:<10}", s.faction.name()), DIM),
            Span::styled(who.to_string(), DIM),
        ]));
    }
    if !resumed {
        for _ in seats.len()..size as usize {
            lines.push(Line::styled("  (open seat: a computer noble if no one comes)", DIM));
        }
    }
    lines
}

pub enum LobbyEvent {
    Start,
    Ready,
    Leave,
}

pub enum Lobby {
    Host(Box<HostLobby>),
    Join(Box<JoinLobby>),
}

impl Lobby {
    /// Polls the network; returns `Ready` when a joiner has been seated.
    pub fn tick(&mut self) -> Option<LobbyEvent> {
        match self {
            Lobby::Host(h) => {
                h.poll();
                None
            }
            Lobby::Join(j) => j.poll().then_some(LobbyEvent::Ready),
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Option<LobbyEvent> {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Some(LobbyEvent::Leave),
            KeyCode::Left | KeyCode::Right => {
                let d: i32 = if k.code == KeyCode::Left { 3 } else { 1 };
                match self {
                    Lobby::Host(h) => {
                        let cur = h.seats().first().map_or(Faction::Saxons, |s| s.faction);
                        h.set_faction(Faction::ALL[(cur as i32 + d).rem_euclid(4) as usize]);
                    }
                    Lobby::Join(j) => {
                        if !j.resumed {
                            let f = Faction::ALL[(j.faction as i32 + d).rem_euclid(4) as usize];
                            j.pick(f);
                        }
                    }
                }
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                if let Lobby::Host(h) = self {
                    h.cycle_timer(k.code == KeyCode::Char('t'));
                }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Lobby::Host(h) = self {
                    if !h.resumed() {
                        h.cycle_years(k.code == KeyCode::Char('y'));
                    }
                }
            }
            KeyCode::Enter => {
                if let Lobby::Host(h) = self {
                    if h.id.is_some() {
                        return Some(LobbyEvent::Start);
                    }
                }
            }
            _ => {}
        }
        None
    }

    pub fn draw(&self, f: &mut Frame) {
        match self {
            Lobby::Host(h) => {
                let mut lines = vec![Line::raw(h.status.clone())];
                if let Some(e) = &h.failed {
                    lines.push(Line::styled(format!("Network trouble: {e}"), Style::new().fg(Color::Indexed(203))));
                }
                lines.push(Line::raw(""));
                if let Some(id) = h.id {
                    lines.push(Line::styled("Over the internet, players join with:", DIM));
                    lines.push(Line::styled(format!("  sworn --join {id}"), Style::new().fg(GOLD)));
                    lines.push(Line::raw(""));
                }
                lines.push(Line::styled(if h.resumed() { "Seats (players rejoin under their old names):" } else { "At the table:" }, DIM));
                lines.extend(seat_lines(&h.seats(), h.size as u8, h.resumed()));
                lines.push(Line::raw(""));
                let mut orders = vec![Span::styled("Orders: ", DIM), Span::styled(timer_name(h.timer), Style::new().fg(GOLD))];
                if !h.resumed() {
                    orders.extend([Span::styled("   Length: ", DIM), Span::styled(format!("{} years", h.years), Style::new().fg(GOLD))]);
                }
                lines.push(Line::from(orders));
                if h.timer.is_some() {
                    lines.push(Line::styled("When time runs out, anyone who has not sent orders holds.", DIM));
                }
                lines.push(Line::raw(""));
                let mut help = vec![key("Enter"), Span::raw(" begin  "), key("t"), Span::raw(" timer  ")];
                if !h.resumed() {
                    help.extend([key("y"), Span::raw(" length  "), key("←→"), Span::raw(" your people  ")]);
                }
                help.extend([key("Esc"), Span::raw(" close the table")]);
                lines.push(Line::from(help));
                panel(f, if h.resumed() { "Resuming a hosted game" } else { "Hosting" }, lines);
            }
            Lobby::Join(j) => {
                let mut lines = vec![];
                if let Some(e) = &j.failed {
                    lines.push(Line::styled(format!("Could not join: {e}"), Style::new().fg(Color::Indexed(203))));
                } else if !j.connected {
                    lines.push(Line::raw("Riding to the host…"));
                } else if j.seats.is_empty() {
                    lines.push(Line::raw("Connected. Waiting for the host to seat you…"));
                } else {
                    lines.push(Line::raw("Seated. Waiting for the host to begin."));
                    lines.push(Line::raw(""));
                    lines.extend(seat_lines(&j.seats, j.size, j.resumed));
                    lines.push(Line::raw(""));
                    let t = (j.timer > 0).then(|| std::time::Duration::from_secs(j.timer as u64));
                    lines.push(Line::from(vec![Span::styled("Orders: ", DIM), Span::raw(timer_name(t))]));
                    if !j.resumed {
                        lines.push(Line::raw(""));
                        lines.push(Line::from(vec![Span::raw("Your people: "), Span::styled(format!("◀ {} ▶", j.faction.name()), Style::new().fg(GOLD)), Span::styled(format!("  {}", j.faction.blurb()), DIM)]));
                    }
                }
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![key("Esc"), Span::raw(" leave")]));
                panel(f, "Joining", lines);
            }
        }
    }
}

pub struct Browse {
    listener: Result<lan::Listener, String>,
    pub sel: usize,
    /// Typing a host's id instead of picking from the LAN.
    pub typing: Option<String>,
    pub name: String,
    editing_name: bool,
    pub error: Option<String>,
}

pub enum BrowseEvent {
    Join(EndpointAddr, String),
    Back,
}

impl Browse {
    pub fn new(name: String) -> Browse {
        let listener = lan::Listener::start(lan::PORT).map_err(|e| e.to_string());
        Browse { listener, sel: 0, typing: None, name, editing_name: false, error: None }
    }

    fn hosts(&self) -> Vec<lan::Found> {
        self.listener.as_ref().map(|l| l.hosts()).unwrap_or_default()
    }

    pub fn key(&mut self, k: KeyEvent) -> Option<BrowseEvent> {
        if self.editing_name {
            match k.code {
                KeyCode::Enter | KeyCode::Tab | KeyCode::Esc => self.editing_name = false,
                KeyCode::Backspace => {
                    self.name.pop();
                }
                KeyCode::Char(c) if self.name.chars().count() < 12 && (c.is_alphanumeric() || " -'".contains(c)) => self.name.push(c),
                _ => {}
            }
            return None;
        }
        if let Some(t) = &mut self.typing {
            match k.code {
                KeyCode::Esc => self.typing = None,
                KeyCode::Backspace => {
                    t.pop();
                }
                KeyCode::Char(c) if c.is_ascii_alphanumeric() && t.len() < 80 => t.push(c),
                KeyCode::Enter => match t.trim().parse::<EndpointId>() {
                    Ok(id) => return self.join(EndpointAddr::new(id)),
                    Err(e) => self.error = Some(format!("not a host id: {e}")),
                },
                _ => {}
            }
            return None;
        }
        let hosts = self.hosts();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Some(BrowseEvent::Back),
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1).min(hosts.len().saturating_sub(1)),
            KeyCode::Tab | KeyCode::Char('n') => self.editing_name = true,
            KeyCode::Char('i') => {
                self.typing = Some(String::new());
                self.error = None;
            }
            KeyCode::Enter => {
                if let Some(h) = hosts.get(self.sel) {
                    return self.join(h.addr());
                }
            }
            _ => {}
        }
        None
    }

    fn join(&mut self, addr: EndpointAddr) -> Option<BrowseEvent> {
        if self.name.trim().is_empty() {
            self.error = Some("Give yourself a name first (Tab).".into());
            self.editing_name = true;
            return None;
        }
        Some(BrowseEvent::Join(addr, self.name.trim().to_string()))
    }

    pub fn draw(&self, f: &mut Frame) {
        let mut lines = vec![Line::from(vec![
            Span::styled("Your name: ", DIM),
            Span::styled(self.name.clone(), Style::new().bold()),
            Span::styled(if self.editing_name { "▏" } else { "" }, Style::new().fg(GOLD)),
        ])];
        lines.push(Line::raw(""));
        let hosts = self.hosts();
        match &self.listener {
            Err(e) => lines.push(Line::styled(format!("Cannot listen on the LAN: {e}"), DIM)),
            Ok(_) if hosts.is_empty() => lines.push(Line::styled("Looking for games on the local network…", DIM)),
            Ok(_) => lines.push(Line::styled("Games on the local network:", DIM)),
        }
        for (i, h) in hosts.iter().enumerate() {
            let state = if h.beacon.in_game { "in progress (rejoin by name)" } else { "gathering" };
            let text = format!("{}'s table · {} player{} · {state}", h.beacon.name, h.beacon.players, if h.beacon.players == 1 { "" } else { "s" });
            lines.push(if i == self.sel { Line::styled(format!("▸ {text}"), Style::new().fg(GOLD).bold()) } else { Line::raw(format!("  {text}")) });
        }
        lines.push(Line::raw(""));
        if let Some(t) = &self.typing {
            lines.push(Line::from(vec![Span::styled("Host id: ", DIM), Span::raw(t.clone()), Span::styled("▏", Style::new().fg(GOLD))]));
        }
        if let Some(e) = &self.error {
            lines.push(Line::styled(e.clone(), Style::new().fg(Color::Indexed(203))));
        }
        lines.push(Line::from(vec![key("Enter"), Span::raw(" join  "), key("i"), Span::raw(" type a host id  "), key("Tab"), Span::raw(" your name  "), key("Esc"), Span::raw(" back")]));
        panel(f, "Join a game", lines);
    }
}
