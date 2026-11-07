//! The whole state of one game.

use crate::chronicle::{Event, EventKind, Visibility};
use crate::diplomacy::{Letter, Offer};
use crate::faction::Faction;
use crate::map::Map;
use crate::orders::Order;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::{NobleId, OfferId, ProvinceId};
use serde::{Deserialize, Serialize};

pub const MIN_NOBLES: usize = 2;
pub const MAX_NOBLES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Title {
    Lord,
    Count,
    Duke,
    King,
}

impl Title {
    pub fn rank(self) -> i32 {
        self as i32
    }

    pub fn name(self) -> &'static str {
        match self {
            Title::Lord => "Lord",
            Title::Count => "Count",
            Title::Duke => "Duke",
            Title::King => "King",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Noble {
    pub id: NobleId,
    pub name: String,
    pub faction: Faction,
    pub gold: u32,
    pub prestige: i32,
    /// Highest title reached; titles are never taken away.
    pub title: Title,
    pub liege: Option<NobleId>,
    pub alive: bool,
    /// Played by the computer when no one sits in this seat.
    pub ai: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewNoble {
    pub name: String,
    pub faction: Faction,
    pub ai: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub seed: u64,
    pub nobles: Vec<NewNoble>,
    pub rules: Rules,
}

/// One noble at the end of one season, for the charts after the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub provinces: u32,
    pub power: u32,
    pub troops: u32,
    pub gold: u32,
    pub prestige: i32,
    pub title: Title,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub rules: Rules,
    pub map: Map,
    pub nobles: Vec<Noble>,
    /// Seasons since the start; see `season_name`.
    pub season: u32,
    pub letters: Vec<Letter>,
    /// Offers still open.
    pub offers: Vec<Offer>,
    pub(crate) next_offer: OfferId,
    pub(crate) next_letter: u32,
    /// This season's orders, per noble; `None` until submitted.
    pub orders: Vec<Option<Vec<Order>>>,
    pub chronicle: Vec<Event>,
    /// Per season, per noble.
    pub history: Vec<Vec<Stats>>,
    pub winner: Option<NobleId>,
    pub rng: Rng,
}

impl Game {
    pub fn new(setup: Setup) -> Result<Game, String> {
        let n = setup.nobles.len();
        if !(MIN_NOBLES..=MAX_NOBLES).contains(&n) {
            return Err(format!("a game needs {MIN_NOBLES} to {MAX_NOBLES} nobles"));
        }
        let mut rng = Rng::new(setup.seed);
        let mut seats: Vec<Faction> = setup.nobles.iter().map(|n| n.faction).collect();
        seats.sort();
        seats.dedup();
        let (mut map, starts) = Map::generate(&mut rng, n, &seats);
        for (i, &s) in starts.iter().enumerate() {
            let p = &mut map.provinces[s as usize];
            p.owner = Some(i as NobleId);
            p.troops = setup.rules.start_troops;
        }
        Game::with_map(setup.rules, map, setup.nobles, rng)
    }

    /// A game on a ready-made map; provinces already owned keep their owners.
    pub fn with_map(rules: Rules, map: Map, nobles: Vec<NewNoble>, rng: Rng) -> Result<Game, String> {
        if nobles.len() > MAX_NOBLES {
            return Err(format!("at most {MAX_NOBLES} nobles"));
        }
        let nobles: Vec<Noble> = nobles
            .into_iter()
            .enumerate()
            .map(|(i, n)| Noble {
                id: i as NobleId,
                name: n.name,
                faction: n.faction,
                gold: rules.start_gold,
                prestige: 0,
                title: Title::Lord,
                liege: None,
                alive: true,
                ai: n.ai,
            })
            .collect();
        let count = nobles.len();
        let mut g = Game {
            rules,
            map,
            nobles,
            season: 0,
            letters: vec![],
            offers: vec![],
            next_offer: 1,
            next_letter: 1,
            orders: vec![None; count],
            chronicle: vec![],
            history: vec![],
            winner: None,
            rng,
        };
        g.history.push(g.stats());
        Ok(g)
    }

    pub fn noble(&self, id: NobleId) -> Option<&Noble> {
        self.nobles.get(id as usize)
    }

    pub fn alive(&self) -> impl Iterator<Item = &Noble> {
        self.nobles.iter().filter(|n| n.alive)
    }

    pub fn provinces_of(&self, n: NobleId) -> impl Iterator<Item = ProvinceId> + '_ {
        self.map.provinces.iter().filter(move |p| p.owner == Some(n)).map(|p| p.id)
    }

    pub fn troops_of(&self, n: NobleId) -> u32 {
        self.map.provinces.iter().filter(|p| p.owner == Some(n)).map(|p| p.troops).sum()
    }

    pub fn vassals_of(&self, n: NobleId) -> impl Iterator<Item = NobleId> + '_ {
        self.nobles.iter().filter(move |v| v.alive && v.liege == Some(n)).map(|v| v.id)
    }

    /// Whether `a` stands somewhere above `b` in the chain of oaths.
    pub fn is_above(&self, a: NobleId, b: NobleId) -> bool {
        let mut cur = self.nobles[b as usize].liege;
        let mut steps = 0;
        while let Some(l) = cur {
            if l == a {
                return true;
            }
            steps += 1;
            if steps > self.nobles.len() {
                return false;
            }
            cur = self.nobles[l as usize].liege;
        }
        false
    }

    /// Provinces held directly and through vassals, at any depth.
    pub fn power(&self, n: NobleId) -> u32 {
        let own = self.provinces_of(n).count() as u32;
        own + self.vassals_of(n).map(|v| self.power(v)).sum::<u32>()
    }

    /// Whether `n` can see what is in province `p`: their own provinces and
    /// their neighbours, and those of their liege and direct vassals.
    pub fn sees(&self, n: NobleId, p: ProvinceId) -> bool {
        let me = &self.nobles[n as usize];
        let friend = |o: Option<NobleId>| {
            o.is_some_and(|o| o == n || me.liege == Some(o) || self.nobles[o as usize].liege == Some(n))
        };
        let prov = &self.map.provinces[p as usize];
        friend(prov.owner) || prov.neighbours.iter().any(|&q| self.map.provinces[q as usize].owner == Some(n))
    }

    /// Who could see province `p` this moment.
    pub(crate) fn watchers(&self, p: ProvinceId) -> Vec<NobleId> {
        self.alive().filter(|n| self.sees(n.id, p)).map(|n| n.id).collect()
    }

    pub fn submit(&mut self, n: NobleId, orders: Vec<Order>) -> Result<(), String> {
        if self.winner.is_some() {
            return Err("the game is over".into());
        }
        if !self.noble(n).is_some_and(|x| x.alive) {
            return Err("that noble is out of the game".into());
        }
        self.orders[n as usize] = Some(orders);
        Ok(())
    }

    /// Takes back orders before the season resolves.
    pub fn unsubmit(&mut self, n: NobleId) {
        if let Some(o) = self.orders.get_mut(n as usize) {
            *o = None;
        }
    }

    /// Whether every noble played by a person has given orders.
    pub fn people_ready(&self) -> bool {
        self.alive().filter(|n| !n.ai).all(|n| self.orders[n.id as usize].is_some())
    }

    /// Living nobles whose orders are not in yet.
    pub fn waiting_on(&self) -> Vec<NobleId> {
        self.alive().filter(|n| self.orders[n.id as usize].is_none()).map(|n| n.id).collect()
    }

    /// Lets every computer noble write its letters and give its orders.
    pub fn play_ai(&mut self) {
        self.ai_write();
        self.ai_orders();
    }

    /// Computer nobles write their letters; call at the start of a season.
    pub fn ai_write(&mut self) {
        for id in self.ai_ids() {
            let turn = crate::ai::plan(&self.view(id));
            self.send_drafts(id, turn.letters);
        }
    }

    /// Computer nobles answer offers and give their orders; call once the
    /// players have had their say.
    pub fn ai_orders(&mut self) {
        for id in self.ai_ids() {
            if self.orders[id as usize].is_some() {
                continue;
            }
            let turn = crate::ai::plan(&self.view(id));
            self.send_drafts(id, turn.replies);
            let _ = self.submit(id, turn.orders);
        }
    }

    fn ai_ids(&self) -> Vec<NobleId> {
        self.alive().filter(|n| n.ai).map(|n| n.id).collect()
    }

    fn send_drafts(&mut self, id: NobleId, drafts: Vec<crate::ai::Draft>) {
        for l in drafts {
            if let Err(e) = self.send_letter(id, &l.to, &l.body, l.offer) {
                log::debug!("ai {id} letter refused: {e}");
            }
        }
    }

    pub(crate) fn record(&mut self, to: Visibility, kind: EventKind) {
        log::debug!("{} {:?} {:?}", crate::season_name(self.season), to, kind);
        self.chronicle.push(Event { season: self.season, to, kind });
    }

    pub fn stats(&self) -> Vec<Stats> {
        self.nobles
            .iter()
            .map(|n| Stats {
                provinces: self.provinces_of(n.id).count() as u32,
                power: if n.alive { self.power(n.id) } else { 0 },
                troops: self.troops_of(n.id),
                gold: n.gold,
                prestige: n.prestige,
                title: n.title,
            })
            .collect()
    }

    /// Final ranking: the crowned noble first, then by score.
    pub fn standings(&self) -> Vec<NobleId> {
        let mut ids: Vec<NobleId> = self.nobles.iter().map(|n| n.id).collect();
        ids.sort_by_key(|&id| {
            let n = &self.nobles[id as usize];
            (std::cmp::Reverse(self.winner == Some(id)), std::cmp::Reverse(n.alive), std::cmp::Reverse(self.score(id)), id)
        });
        ids
    }

    pub fn score(&self, n: NobleId) -> i64 {
        let noble = &self.nobles[n as usize];
        if !noble.alive {
            return i64::MIN / 2 + noble.prestige as i64;
        }
        self.power(n) as i64 * 10 + noble.prestige as i64 + noble.title.rank() as i64 * 5
    }
}
