//! What one noble is allowed to know. The host sends each player their view
//! and nothing else; the computer nobles plan from views too, so they cannot
//! cheat either.
//!
//! The political map is public: everyone knows who holds what. Troop counts
//! are not: you see them only in your own provinces, their neighbours, and
//! the provinces of your liege and direct vassals. Anything else you know is
//! what someone wrote you, and they may have lied.

use crate::chronicle::Event;
use crate::diplomacy::{Letter, Offer};
use crate::faction::Faction;
use crate::game::{Game, Stats, Title};
use crate::map::Kind;
use crate::rules::Rules;
use crate::{NobleId, ProvinceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProvinceView {
    pub id: ProvinceId,
    pub name: String,
    pub kind: Kind,
    pub x: u16,
    pub y: u16,
    pub neighbours: Vec<ProvinceId>,
    pub owner: Option<NobleId>,
    pub troops: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NobleView {
    pub id: NobleId,
    pub name: String,
    pub faction: Faction,
    pub title: Title,
    pub prestige: i32,
    pub liege: Option<NobleId>,
    pub alive: bool,
    pub ai: bool,
    pub provinces: u32,
    pub power: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub me: NobleId,
    pub season: u32,
    pub rules: Rules,
    pub gold: u32,
    pub provinces: Vec<ProvinceView>,
    pub nobles: Vec<NobleView>,
    /// Letters to or from me.
    pub letters: Vec<Letter>,
    /// Open offers to or from me.
    pub offers: Vec<Offer>,
    /// Chronicle entries I know of, from `since` on.
    pub events: Vec<Event>,
    pub since: usize,
    pub submitted: bool,
    /// Players (not computer nobles) whose orders are not in yet.
    pub waiting: Vec<NobleId>,
    pub winner: Option<NobleId>,
    /// Every noble's numbers season by season; private until the game ends.
    pub history: Vec<Vec<Stats>>,
}

impl Game {
    pub fn view(&self, me: NobleId) -> View {
        self.view_since(me, 0)
    }

    /// A view with only the chronicle entries from index `since` on, so a
    /// host can send each season's news without resending the whole history.
    pub fn view_since(&self, me: NobleId, since: usize) -> View {
        let provinces = self
            .map
            .provinces
            .iter()
            .map(|p| ProvinceView {
                id: p.id,
                name: p.name.clone(),
                kind: p.kind,
                x: p.x,
                y: p.y,
                neighbours: p.neighbours.clone(),
                owner: p.owner,
                troops: self.sees(me, p.id).then_some(p.troops),
            })
            .collect();
        let nobles = self
            .nobles
            .iter()
            .map(|n| NobleView {
                id: n.id,
                name: n.name.clone(),
                faction: n.faction,
                title: n.title,
                prestige: n.prestige,
                liege: n.liege,
                alive: n.alive,
                ai: n.ai,
                provinces: self.provinces_of(n.id).count() as u32,
                power: if n.alive { self.power(n.id) } else { 0 },
            })
            .collect();
        View {
            me,
            season: self.season,
            rules: self.rules.clone(),
            gold: self.nobles[me as usize].gold,
            provinces,
            nobles,
            letters: self.letters.iter().filter(|l| l.from == me || l.to.contains(&me)).cloned().collect(),
            offers: self.offers.iter().filter(|o| o.from == me || o.to == me).cloned().collect(),
            events: self.chronicle.iter().skip(since).filter(|e| e.to.includes(me)).cloned().collect(),
            since,
            submitted: self.orders[me as usize].is_some(),
            waiting: self.alive().filter(|n| !n.ai && self.orders[n.id as usize].is_none()).map(|n| n.id).collect(),
            winner: self.winner,
            history: if self.winner.is_some() { self.history.clone() } else { vec![] },
        }
    }
}

impl View {
    pub fn province(&self, id: ProvinceId) -> &ProvinceView {
        &self.provinces[id as usize]
    }

    pub fn noble(&self, id: NobleId) -> &NobleView {
        &self.nobles[id as usize]
    }

    /// Me, my liege, or my direct vassals.
    pub fn friendly(&self, n: NobleId) -> bool {
        n == self.me || self.noble(self.me).liege == Some(n) || self.noble(n).liege == Some(self.me)
    }

    pub fn mine(&self) -> impl Iterator<Item = &ProvinceView> {
        self.provinces.iter().filter(move |p| p.owner == Some(self.me))
    }
}
