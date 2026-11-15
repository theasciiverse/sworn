//! A player's link to the game. The interface talks only to a `Court` and
//! only ever sees a `View`, so it works the same whether the game runs here
//! (`LocalCourt`, against computer nobles) or on another player's machine.

use crate::diplomacy::Term;
use crate::game::Game;
use crate::orders::Order;
use crate::save;
use crate::view::View;
use crate::NobleId;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A season resolved; the view now holds what happened in it.
    Resolved { season: u32 },
    /// Something to show in the status line.
    Info(String),
    /// The link to the game is gone.
    Lost(String),
}

pub type Offer = Option<(Vec<Term>, Vec<Term>)>;

pub trait Court {
    fn view(&self) -> &View;
    fn send_letter(&mut self, to: &[NobleId], body: &str, offer: Offer) -> Result<(), String>;
    /// Locks in this season's orders.
    fn submit(&mut self, orders: Vec<Order>) -> Result<(), String>;
    /// Takes back submitted orders while the season is still open.
    fn unsubmit(&mut self) -> Result<(), String> {
        Err("orders cannot be taken back here".into())
    }
    fn poll(&mut self) -> Vec<Notice>;
    /// Saves the game, if this side holds it. Returns where.
    fn save(&mut self) -> Result<String, String> {
        Err("only the host holds the game".into())
    }
    /// When this season's orders are due, if the table has a timer.
    fn deadline(&self) -> Option<std::time::Instant> {
        None
    }
    /// Who else is at the table, for the status line.
    fn describe(&self) -> String;
}

/// One player against computer nobles, on this machine.
pub struct LocalCourt {
    game: Game,
    me: NobleId,
    view: View,
    path: Option<PathBuf>,
    notices: Vec<Notice>,
}

impl LocalCourt {
    pub fn new(mut game: Game, me: NobleId, path: Option<PathBuf>) -> LocalCourt {
        if game.season == 0 && game.letters.is_empty() {
            game.ai_write();
        }
        let view = game.view(me);
        let mut c = LocalCourt { game, me, view, path, notices: vec![] };
        c.autosave();
        c
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    fn refresh(&mut self) {
        self.view = self.game.view(self.me);
    }

    fn autosave(&mut self) {
        if let Some(p) = &self.path {
            if let Err(e) = save::save(&self.game, p) {
                log::warn!("autosave to {} failed: {e}", p.display());
                self.notices.push(Notice::Info(format!("could not save: {e}")));
            }
        }
    }
}

impl Court for LocalCourt {
    fn view(&self) -> &View {
        &self.view
    }

    fn send_letter(&mut self, to: &[NobleId], body: &str, offer: Offer) -> Result<(), String> {
        self.game.send_letter(self.me, to, body, offer)?;
        self.refresh();
        Ok(())
    }

    fn submit(&mut self, orders: Vec<Order>) -> Result<(), String> {
        self.game.submit(self.me, orders)?;
        self.game.ai_orders();
        let season = self.game.season;
        self.game.resolve();
        if self.game.winner.is_none() {
            self.game.ai_write();
        }
        self.autosave();
        self.refresh();
        self.notices.push(Notice::Resolved { season });
        Ok(())
    }

    fn poll(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    fn save(&mut self) -> Result<String, String> {
        let Some(p) = &self.path else { return Err("no save file for this game".into()) };
        save::save(&self.game, p).map_err(|e| e.to_string())?;
        Ok(p.display().to_string())
    }

    fn describe(&self) -> String {
        let ai = self.game.alive().filter(|n| n.ai).count();
        let level = match self.game.rules.ai_income_pct {
            p if p < 100 => " (gentle)",
            p if p > 100 => " (hard)",
            _ => "",
        };
        format!("against {ai} computer noble{}{level}", if ai == 1 { "" } else { "s" })
    }
}

/// Where saves live: `$XDG_STATE_HOME/sworn`, else `~/.local/state/sworn`
/// (`%LOCALAPPDATA%\sworn` on Windows). Unit tests get a folder of their own
/// in the temp directory, never a player's.
pub fn state_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("sworn-unit-{}", std::process::id()));
    }
    state_home().join("sworn")
}

fn state_home() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_absolute());
    var("XDG_STATE_HOME")
        .or_else(|| if cfg!(windows) { var("LOCALAPPDATA") } else { None })
        .or_else(|| var("HOME").map(|h| h.join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn autosave_path() -> PathBuf {
    state_dir().join("autosave.sworn")
}

/// The save of the game this machine hosts for others.
pub fn hosted_path() -> PathBuf {
    state_dir().join("hosted.sworn")
}
