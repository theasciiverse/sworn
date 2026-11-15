//! Every tunable number in one place. Saved with the game, so a game keeps
//! the rules it started with even after the defaults change.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    pub start_gold: u32,
    pub start_troops: u32,
    /// Gold per troop raised, before faction discounts.
    pub raise_cost: u32,
    /// Troops a hold can raise in one season (towns and seats twice this).
    pub raise_cap: u32,
    /// One gold of upkeep per this many troops, every season.
    pub troops_per_upkeep: u32,
    /// Share of a vassal's income that goes to the liege.
    pub tribute_pct: u32,
    /// Whether armies may march in winter. Off: winter is for letters.
    pub winter_marches: bool,

    pub count_power: u32,
    pub count_prestige: i32,
    pub duke_power: u32,
    pub duke_prestige: i32,
    /// The crown needs this share of all provinces, held directly or by vassals.
    pub king_share_pct: u32,
    pub king_prestige: i32,

    pub capture_prestige: i32,
    pub neutral_capture_prestige: i32,
    /// Gained by a liege when a vassal swears.
    pub vassal_prestige: i32,
    /// Lost for marching against your own liege or vassal.
    pub betrayal_penalty: i32,
    /// Lost for openly renouncing an oath.
    pub renounce_penalty: i32,

    /// Seasons an offer stays open: the season it was sent and the next.
    pub offer_lifetime: u32,
    /// After this many seasons the highest score wins.
    pub max_seasons: u32,
    /// Computer nobles' income, percent of what a player's lands would pay:
    /// the difficulty of a game against them.
    pub ai_income_pct: u32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            start_gold: 10,
            start_troops: 6,
            raise_cost: 2,
            raise_cap: 4,
            troops_per_upkeep: 4,
            tribute_pct: 25,
            winter_marches: false,
            count_power: 3,
            count_prestige: 4,
            duke_power: 6,
            duke_prestige: 12,
            king_share_pct: 40,
            king_prestige: 20,
            capture_prestige: 2,
            neutral_capture_prestige: 1,
            vassal_prestige: 3,
            betrayal_penalty: 10,
            renounce_penalty: 4,
            offer_lifetime: 2,
            max_seasons: 30 * 4,
            ai_income_pct: 100,
        }
    }
}
