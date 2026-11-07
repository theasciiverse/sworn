//! The terminal interface: title, setup, and the game screen.

pub mod draw;
pub mod lobby;
pub mod map;
pub mod over;
pub mod play;
pub mod title;

use crate::court::{autosave_path, hosted_path, Court, LocalCourt};
use crate::netcourt::{HostLobby, JoinLobby};
use lobby::{Browse, BrowseEvent, Lobby, LobbyEvent};
use crate::faction::Faction;
use crate::game::{Game, NewNoble, Setup as GameSetup};
use crate::rules::Rules;
use crate::save;
use play::Play;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};
use std::io;
use std::time::Duration;
use title::{Choice, Setup, SetupResult, Title};

pub enum Screen {
    Title(Title),
    Setup(Setup),
    Browse(Browse),
    Lobby(Lobby),
    Play(Play),
}

fn title() -> Title {
    Title::new(autosave_path().exists(), hosted_path().exists())
}

pub struct Ui {
    pub screen: Screen,
    pub quit: bool,
    pub player_name: String,
}

/// The player's name from the environment, for the setup screen.
pub fn default_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let mut c = user.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).take(12).collect(),
        None => "Lord".into(),
    }
}

/// A new single-player game: you are noble 0, computer nobles the rest.
pub fn new_local_game(name: &str, faction: Faction, count: usize, seed: u64) -> Result<Game, String> {
    let mut nobles = vec![NewNoble { name: name.into(), faction, ai: false }];
    let mut others = crate::netcourt::COMPUTER_NAMES.iter().filter(|n| !n.eq_ignore_ascii_case(name));
    for i in 1..count {
        let f = Faction::ALL[(faction as usize + i) % 4];
        nobles.push(NewNoble { name: others.next().copied().unwrap_or("Nobody").into(), faction: f, ai: true });
    }
    Game::new(GameSetup { seed, nobles, rules: Rules::default() })
}

pub fn seed_from_clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

impl Ui {
    pub fn new() -> Ui {
        Ui { screen: Screen::Title(title()), quit: false, player_name: default_name() }
    }

    pub fn hosting() -> Ui {
        Ui { screen: Screen::Setup(Setup::new(default_name(), true)), quit: false, player_name: default_name() }
    }

    pub fn joining(addr: Option<iroh::EndpointAddr>) -> Ui {
        let name = default_name();
        let screen = match addr {
            Some(a) => Screen::Lobby(Lobby::Join(Box::new(JoinLobby::new(a, &name)))),
            None => Screen::Browse(Browse::new(name.clone())),
        };
        Ui { screen, quit: false, player_name: name }
    }

    pub fn with_court(court: Box<dyn Court>) -> Ui {
        Ui { screen: Screen::Play(Play::new(court)), quit: false, player_name: default_name() }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        match &mut self.screen {
            Screen::Title(t) => t.draw(f),
            Screen::Setup(s) => s.draw(f),
            Screen::Browse(b) => b.draw(f),
            Screen::Lobby(l) => l.draw(f),
            Screen::Play(p) => draw::draw(f, p),
        }
    }

    pub fn tick(&mut self) {
        match &mut self.screen {
            Screen::Play(p) => {
                p.tick();
                if p.leave {
                    self.screen = Screen::Title(title());
                }
            }
            Screen::Lobby(l) => {
                if let Some(LobbyEvent::Ready) = l.tick() {
                    self.lobby_event(LobbyEvent::Ready);
                }
            }
            _ => {}
        }
    }

    fn lobby_event(&mut self, e: LobbyEvent) {
        let Screen::Lobby(l) = std::mem::replace(&mut self.screen, Screen::Title(title())) else { return };
        match (e, l) {
            (LobbyEvent::Leave, _) => {}
            (LobbyEvent::Start, Lobby::Host(h)) => match h.start(seed_from_clock()) {
                Ok(court) => self.screen = Screen::Play(Play::new(Box::new(court))),
                Err(e) => {
                    let mut t = title();
                    t.note = e;
                    self.screen = Screen::Title(t);
                }
            },
            (LobbyEvent::Ready, Lobby::Join(j)) => match j.into_court() {
                Some(court) => self.screen = Screen::Play(Play::new(Box::new(court))),
                None => self.screen = Screen::Title(title()),
            },
            (_, l) => self.screen = Screen::Lobby(l),
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                log::debug!("key {:?} {:?}", k.code, k.modifiers);
                match &mut self.screen {
                    Screen::Title(t) => {
                        if let Some(c) = t.key(k) {
                            self.choose(c);
                        }
                    }
                    Screen::Setup(s) => match s.key(k) {
                        Some(SetupResult::Back) => self.screen = Screen::Title(title()),
                        Some(SetupResult::Start) if s.hosting => {
                            let (name, faction, count) = (s.name.trim().to_string(), s.faction(), s.nobles);
                            self.player_name = name.clone();
                            let h = HostLobby::new(&name, faction, count, hosted_path());
                            self.screen = Screen::Lobby(Lobby::Host(Box::new(h)));
                        }
                        Some(SetupResult::Start) => {
                            let (name, faction, count) = (s.name.trim().to_string(), s.faction(), s.nobles);
                            self.player_name = name.clone();
                            match new_local_game(&name, faction, count, seed_from_clock()) {
                                Ok(g) => {
                                    log::info!("new game: {count} nobles, {} provinces", g.map.provinces.len());
                                    let court = LocalCourt::new(g, 0, Some(autosave_path()));
                                    self.screen = Screen::Play(Play::new(Box::new(court)));
                                }
                                Err(e) => {
                                    let mut t = title();
                                    t.note = e;
                                    self.screen = Screen::Title(t);
                                }
                            }
                        }
                        None => {}
                    },
                    Screen::Browse(b) => match b.key(k) {
                        Some(BrowseEvent::Back) => self.screen = Screen::Title(title()),
                        Some(BrowseEvent::Join(addr, name)) => {
                            self.player_name = name.clone();
                            self.screen = Screen::Lobby(Lobby::Join(Box::new(JoinLobby::new(addr, &name))));
                        }
                        None => {}
                    },
                    Screen::Lobby(l) => {
                        if let Some(e) = l.key(k) {
                            self.lobby_event(e);
                        }
                    }
                    Screen::Play(p) => p.key(k),
                }
            }
            Event::Mouse(m) => {
                if let Screen::Play(p) = &mut self.screen {
                    p.mouse(m);
                }
            }
            Event::Resize(w, h) => log::info!("resize {w}x{h}"),
            _ => {}
        }
        self.tick();
    }

    fn choose(&mut self, c: Choice) {
        match c {
            Choice::Quit => self.quit = true,
            Choice::New => self.screen = Screen::Setup(Setup::new(self.player_name.clone(), false)),
            Choice::Continue => match save::load(&autosave_path()) {
                Ok(g) => {
                    let me = g.nobles.iter().find(|n| !n.ai).map_or(0, |n| n.id);
                    log::info!("continuing a game at season {}", g.season);
                    self.screen = Screen::Play(Play::new(Box::new(LocalCourt::new(g, me, Some(autosave_path())))));
                }
                Err(e) => {
                    if let Screen::Title(t) = &mut self.screen {
                        t.note = format!("Could not continue: {e}");
                    }
                }
            },
            Choice::Host => self.screen = Screen::Setup(Setup::new(self.player_name.clone(), true)),
            Choice::Join => self.screen = Screen::Browse(Browse::new(self.player_name.clone())),
            Choice::Resume => match save::load(&hosted_path()) {
                Ok(g) => {
                    log::info!("resuming a hosted game at season {}", g.season);
                    self.screen = Screen::Lobby(Lobby::Host(Box::new(HostLobby::resume(g, hosted_path()))));
                }
                Err(e) => {
                    if let Screen::Title(t) = &mut self.screen {
                        t.note = format!("Could not resume: {e}");
                    }
                }
            },
        }
    }
}

impl Default for Ui {
    fn default() -> Ui {
        Ui::new()
    }
}

/// Runs until the player quits. Wakes at least four times a second so news
/// from the network shows up without a key press.
pub fn run(terminal: &mut DefaultTerminal, ui: &mut Ui) -> io::Result<()> {
    while !ui.quit {
        terminal.draw(|f| ui.draw(f))?;
        if event::poll(Duration::from_millis(250))? {
            ui.event(event::read()?);
            while event::poll(Duration::ZERO)? {
                ui.event(event::read()?);
            }
        } else {
            ui.tick();
        }
    }
    Ok(())
}
