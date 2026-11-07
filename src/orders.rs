//! What a noble commands in a season. Orders are secret until the season
//! resolves, and everyone's resolve at once.

use crate::{NobleId, OfferId, ProvinceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    /// Pay gold for new troops in a province you hold.
    Raise { at: ProvinceId, count: u32 },
    /// Send troops along a road. `on_behalf: None` marches for yourself: into
    /// your own province it reinforces, into anyone else's it attacks. With
    /// `Some(ally)` the troops fight on the ally's side (helping defend the
    /// ally's province, or helping the ally take someone else's) and come
    /// home afterwards. Promising this and not doing it is the game.
    March { from: ProvinceId, to: ProvinceId, count: u32, on_behalf: Option<NobleId> },
    /// Take up an offer someone sent you. It executes before any marching,
    /// provided both sides can still honour it.
    Accept(OfferId),
    /// Break your oath to your liege openly. Cheaper than betrayal by arms.
    Renounce,
    /// Free a vassal from their oath, at no cost to either.
    Release(NobleId),
}
