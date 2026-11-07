//! Wire messages between a Fealty host and the players who join it.
//!
//! The host holds the only `Game`. Players send what they would do (letters,
//! orders); the host answers with their `View` whenever it changes, and never
//! anything more, so the fog of war holds across the network too.
//!
//! Encoding is postcard (serde) inside u32-length-prefixed frames, as in
//! Tetrix and Panzer.

use crate::diplomacy::Term;
use crate::faction::Faction;
use crate::orders::Order;
use crate::view::View;
use crate::NobleId;
use serde::{Deserialize, Serialize};

/// Bumped on any incompatible change; the host rejects mismatched players.
pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub name: String,
    pub faction: Faction,
    pub host: bool,
    /// Still to be claimed by a returning player (a game continued from a save).
    pub empty: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    /// A player's first message.
    Hello { name: String, version: u32 },
    Reject { reason: String },
    /// Host: who sits at the table before the game starts, and how many seats.
    /// `timer` is the seconds each season allows for orders, 0 for no limit.
    Lobby { seats: Vec<Seat>, size: u8, resumed: bool, timer: u32 },
    /// Player: the people they want to lead.
    Pick(Faction),
    /// Host: the game has begun (or resumed) and this is your seat.
    Seated { me: NobleId },
    /// Host: your view of the game, after anything that changed it.
    View(Box<View>),
    /// Player: a letter, perhaps with an offer.
    Letter { to: Vec<NobleId>, body: String, offer: Option<(Vec<Term>, Vec<Term>)> },
    Submit(Vec<Order>),
    Unsubmit,
    /// Host: something the player asked for was refused, or news for the status line.
    Info(String),
    /// Host: seconds left for this season's orders; None means no limit.
    Deadline(Option<u32>),
    /// Keeps a quiet connection from timing out.
    Ping,
}

pub fn encode(m: &Msg) -> Vec<u8> {
    postcard::to_allocvec(m).expect("postcard encoding of an owned message cannot fail")
}

pub fn decode(b: &[u8]) -> Option<Msg> {
    postcard::from_bytes(b).ok()
}
