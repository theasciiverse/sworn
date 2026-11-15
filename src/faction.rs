//! The four peoples. Light bonuses for now: enough to make a choice mean
//! something, not enough to decide a game.

use crate::map::Kind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Faction {
    Saxons,
    Danes,
    Bretons,
    Moravians,
}

impl Faction {
    pub const ALL: [Faction; 4] = [Faction::Saxons, Faction::Danes, Faction::Bretons, Faction::Moravians];

    pub fn name(self) -> &'static str {
        match self {
            Faction::Saxons => "Saxons",
            Faction::Danes => "Danes",
            Faction::Bretons => "Bretons",
            Faction::Moravians => "Moravians",
        }
    }

    /// One of the people: "every other Saxon".
    pub fn one(self) -> &'static str {
        match self {
            Faction::Saxons => "Saxon",
            Faction::Danes => "Dane",
            Faction::Bretons => "Breton",
            Faction::Moravians => "Moravian",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Faction::Saxons => "The fyrd: troops cost a third less to keep.",
            Faction::Danes => "Raiders: 10% stronger when attacking.",
            Faction::Bretons => "Castle-builders: half again as strong defending; fortresses pay 3 more.",
            Faction::Moravians => "Traders: every town pays three more gold.",
        }
    }

    pub fn attack_pct(self) -> u64 {
        if self == Faction::Danes {
            110
        } else {
            100
        }
    }

    pub fn defence_pct(self) -> u64 {
        if self == Faction::Bretons {
            150
        } else {
            100
        }
    }

    pub fn raise_cost(self, base: u32) -> u32 {
        base
    }

    /// Troops per gold of upkeep.
    pub fn upkeep_per(self, base: u32) -> u32 {
        if self == Faction::Saxons {
            base * 3 / 2
        } else {
            base
        }
    }

    pub fn income_bonus(self, kind: Kind) -> u32 {
        match (self, kind) {
            (Faction::Moravians, Kind::Town) => 3,
            (Faction::Bretons, Kind::Fortress) => 3,
            _ => 0,
        }
    }
}
