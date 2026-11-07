//! Fealty's rules core: the realm, the nobles, their orders and letters, and
//! the resolution of a season. Pure and deterministic: no screen, no network,
//! no clock. The same `Game` and the same orders always give the same result,
//! which is what lets any player who holds a save take over as host.

pub mod ai;
pub mod chronicle;
pub mod court;
pub mod diplomacy;
pub mod faction;
pub mod game;
pub mod lan;
pub mod map;
pub mod net;
pub mod netcourt;
pub mod orders;
pub mod proto;
mod resolve;
pub mod rng;
pub mod rules;
pub mod save;
pub mod tui;
pub mod view;

pub type NobleId = u8;
pub type ProvinceId = u16;
pub type OfferId = u32;

pub use chronicle::{Event, EventKind, Outcome, Visibility};
pub use diplomacy::{Letter, Offer, Term};
pub use faction::Faction;
pub use game::{Game, NewNoble, Noble, Setup, Stats, Title};
pub use map::{Kind, Map, Province};
pub use orders::Order;
pub use rules::Rules;
pub use view::View;

/// Year the chronicle starts in. Seasons run Spring, Summer, Autumn, Winter.
pub const FIRST_YEAR: u32 = 800;

pub fn season_name(season: u32) -> String {
    let q = ["Spring", "Summer", "Autumn", "Winter"][(season % 4) as usize];
    format!("{q} {}", FIRST_YEAR + season / 4)
}

pub fn is_winter(season: u32) -> bool {
    season % 4 == 3
}
