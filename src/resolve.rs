//! Resolving a season. Every noble's orders take effect together, in this
//! order: oaths renounced and released, accepted deals, troops raised,
//! marches and battles, eliminations, income and upkeep, prestige, titles.
//!
//! Battles are deterministic: the strongest side wins and keeps troops in
//! proportion to its margin. Plans can be made, and lies can be calculated.

use crate::chronicle::{Contribution, EventKind, Outcome, Visibility};
use crate::game::{Game, Title};
use crate::map::Kind;
use crate::orders::Order;
use crate::{is_winter, NobleId, ProvinceId};
use std::collections::{BTreeMap, BTreeSet};

impl Game {
    /// Resolves the season with the orders submitted (missing orders count as
    /// none) and moves on to the next. Returns the index of the first new
    /// chronicle entry.
    pub fn resolve(&mut self) -> usize {
        let first = self.chronicle.len();
        if self.winner.is_some() {
            return first;
        }
        let orders: Vec<Vec<Order>> = self.orders.iter_mut().map(|o| o.take().unwrap_or_default()).collect();
        self.resolve_oaths(&orders);
        self.resolve_deals(&orders);
        self.resolve_raises(&orders);
        if is_winter(self.season) && !self.rules.winter_marches {
            for (n, os) in orders.iter().enumerate() {
                if os.iter().any(|o| matches!(o, Order::March { .. })) {
                    let n = n as NobleId;
                    self.record(Visibility::Private(vec![n]), EventKind::Snowbound { noble: n });
                }
            }
        } else {
            self.resolve_marches(&orders);
        }
        self.eliminate();
        self.economy();
        if is_winter(self.season) {
            // The winter court: standing is rewarded once a year.
            for n in self.nobles.iter_mut().filter(|n| n.alive) {
                n.prestige += 1 + n.title.rank();
            }
        }
        self.titles_and_crown();
        self.history.push(self.stats());
        let (season, life) = (self.season, self.rules.offer_lifetime.max(1));
        self.offers.retain(|o| o.season + life > season + 1);
        self.season += 1;
        if self.winner.is_none() && self.season >= self.rules.max_seasons {
            if let Some(&w) = self.standings().first() {
                self.winner = Some(w);
                self.record(Visibility::Public, EventKind::GameOver { winner: w, crowned: false });
            }
        }
        first
    }

    fn resolve_oaths(&mut self, orders: &[Vec<Order>]) {
        for (n, os) in orders.iter().enumerate() {
            let n = n as NobleId;
            for o in os {
                match *o {
                    Order::Release(v) => {
                        if self.noble(v).is_some_and(|x| x.alive && x.liege == Some(n)) {
                            self.nobles[v as usize].liege = None;
                            self.record(Visibility::Public, EventKind::Released { liege: n, vassal: v });
                        }
                    }
                    Order::Renounce => {
                        if let Some(l) = self.nobles[n as usize].liege {
                            self.nobles[n as usize].liege = None;
                            self.nobles[n as usize].prestige -= self.rules.renounce_penalty;
                            self.record(Visibility::Public, EventKind::Renounced { vassal: n, liege: l });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn resolve_deals(&mut self, orders: &[Vec<Order>]) {
        // In offer order, so whoever made an offer first is honoured first.
        let mut accepted: Vec<u32> = vec![];
        for (n, os) in orders.iter().enumerate() {
            for o in os {
                if let Order::Accept(id) = *o {
                    if self.offers.iter().any(|x| x.id == id && x.to as usize == n) {
                        accepted.push(id);
                    }
                }
            }
        }
        accepted.sort_unstable();
        accepted.dedup();
        for id in accepted {
            if let Some(i) = self.offers.iter().position(|x| x.id == id) {
                let offer = self.offers.remove(i);
                self.execute_offer(&offer);
            }
        }
    }

    fn resolve_raises(&mut self, orders: &[Vec<Order>]) {
        let mut raised: BTreeMap<ProvinceId, u32> = BTreeMap::new();
        for (n, os) in orders.iter().enumerate() {
            let n = n as NobleId;
            for o in os {
                let Order::Raise { at, count } = *o else { continue };
                let Some(p) = self.map.get(at) else { continue };
                if p.owner != Some(n) {
                    continue;
                }
                let cap = self.rules.raise_cap * p.kind.raise_multiplier();
                let done = raised.entry(at).or_default();
                let cost = self.nobles[n as usize].faction.raise_cost(self.rules.raise_cost).max(1);
                let c = count.min(cap.saturating_sub(*done)).min(self.nobles[n as usize].gold / cost);
                if c == 0 {
                    continue;
                }
                *done += c;
                self.nobles[n as usize].gold -= c * cost;
                self.map.provinces[at as usize].troops += c;
                self.record(Visibility::Private(vec![n]), EventKind::Raised { noble: n, at, count: c });
            }
        }
    }

    /// Fighting strength in hundredths of hundredths of a troop.
    fn strength(&self, noble: Option<NobleId>, troops: u32, defending: bool, kind: Kind) -> u64 {
        let faction = noble.map(|n| self.nobles[n as usize].faction);
        let fac = match faction {
            Some(f) if defending => f.defence_pct(),
            Some(f) => f.attack_pct(),
            None => 100,
        };
        let walls = if defending { kind.defence_pct() } else { 100 };
        troops as u64 * fac * walls
    }

    fn resolve_marches(&mut self, orders: &[Vec<Order>]) {
        let np = self.map.provinces.len();
        let mut avail: Vec<u32> = self.map.provinces.iter().map(|p| p.troops).collect();
        let mut marches: Vec<(Contribution, ProvinceId)> = vec![];
        for (n, os) in orders.iter().enumerate() {
            let n = n as NobleId;
            if !self.nobles[n as usize].alive {
                continue;
            }
            for o in os {
                let Order::March { from, to, count, on_behalf } = *o else { continue };
                if from as usize >= np || to as usize >= np || from == to {
                    continue;
                }
                if self.map.provinces[from as usize].owner != Some(n) || !self.map.adjacent(from, to) {
                    continue;
                }
                let c = count.min(avail[from as usize]);
                if c == 0 {
                    continue;
                }
                let ally = on_behalf.filter(|&b| self.noble(b).is_some_and(|x| x.alive)).unwrap_or(n);
                avail[from as usize] -= c;
                marches.push((Contribution { noble: n, from, count: c, on_behalf: ally }, to));
            }
        }
        for (c, _) in &marches {
            self.map.provinces[c.from as usize].troops -= c.count;
        }

        // Oaths broken by arms: marching on your liege or your vassal, for
        // anyone but them. Judged by intent, whatever the battle's outcome.
        let mut broken: BTreeMap<(NobleId, NobleId), ProvinceId> = BTreeMap::new();
        for (c, to) in &marches {
            let Some(o) = self.map.provinces[*to as usize].owner else { continue };
            if c.on_behalf == o || o == c.noble {
                continue;
            }
            let me = &self.nobles[c.noble as usize];
            if me.liege == Some(o) || self.nobles[o as usize].liege == Some(c.noble) {
                broken.entry((c.noble, o)).or_insert(*to);
            }
        }
        for ((breaker, victim), at) in broken {
            let (b, v) = (breaker as usize, victim as usize);
            if self.nobles[b].liege == Some(victim) {
                self.nobles[b].liege = None;
            } else if self.nobles[v].liege == Some(breaker) {
                self.nobles[v].liege = None;
            } else {
                continue; // the same pair, already broken this season
            }
            self.nobles[b].prestige -= self.rules.betrayal_penalty;
            self.record(Visibility::Public, EventKind::OathBroken { breaker, victim, at });
        }

        let mut by_dest: BTreeMap<ProvinceId, Vec<Contribution>> = BTreeMap::new();
        for (c, to) in marches {
            by_dest.entry(to).or_default().push(c);
        }
        let mut returns: Vec<(NobleId, ProvinceId, u32)> = vec![];
        for (dest, cs) in by_dest {
            self.battle(dest, cs, &mut returns);
        }
        for (n, home, count) in returns {
            if count == 0 {
                continue;
            }
            if self.map.provinces[home as usize].owner == Some(n) {
                self.map.provinces[home as usize].troops += count;
            } else {
                self.record(Visibility::Private(vec![n]), EventKind::Scattered { noble: n, home, count });
            }
        }
    }

    fn battle(&mut self, dest: ProvinceId, cs: Vec<Contribution>, returns: &mut Vec<(NobleId, ProvinceId, u32)>) {
        let d = dest as usize;
        let (owner, kind, garrison) = {
            let p = &self.map.provinces[d];
            (p.owner, p.kind, p.troops)
        };
        let defends = |c: &Contribution| Some(c.on_behalf) == owner;

        if cs.iter().all(defends) {
            for c in cs {
                if Some(c.noble) == owner {
                    self.map.provinces[d].troops += c.count;
                    let to = Visibility::Private(vec![c.noble]);
                    self.record(to, EventKind::Moved { noble: c.noble, from: c.from, to: dest, count: c.count });
                } else {
                    returns.push((c.noble, c.from, c.count));
                    let to = Visibility::Private(vec![c.noble, c.on_behalf]);
                    self.record(to, EventKind::StoodGuard { noble: c.noble, at: dest, count: c.count });
                }
            }
            return;
        }

        let mut sides: BTreeMap<Option<NobleId>, u64> = BTreeMap::new();
        sides.insert(owner, self.strength(owner, garrison, true, kind));
        for c in &cs {
            let s = self.strength(Some(c.noble), c.count, defends(c), kind);
            *sides.entry(Some(c.on_behalf)).or_default() += s;
        }
        let mut ranked: Vec<(Option<NobleId>, u64)> = sides.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        let top = ranked[0].1;
        let second = ranked.get(1).map_or(0, |s| s.1);

        let mut witnesses: BTreeSet<NobleId> = self.watchers(dest).into_iter().collect();
        witnesses.extend(owner);
        witnesses.extend(cs.iter().flat_map(|c| [c.noble, c.on_behalf]));

        let outcome;
        if top > second {
            let winner = ranked[0].0;
            // Survivors in proportion to the margin, rounded up.
            let survive = |n: u32| ((n as u64 * (top - second)).div_ceil(top)) as u32;
            let mut held = 0;
            if winner == owner {
                held = survive(garrison);
                outcome = Outcome::Held;
            } else {
                let w = winner.expect("only the levies fight for no one, and they only defend");
                let gain = if owner.is_some() { self.rules.capture_prestige } else { self.rules.neutral_capture_prestige };
                self.nobles[w as usize].prestige += gain;
                self.map.provinces[d].owner = winner;
                outcome = Outcome::Taken(w);
            }
            for c in &cs {
                if Some(c.on_behalf) == winner {
                    let s = survive(c.count);
                    if Some(c.noble) == winner {
                        held += s;
                    } else {
                        returns.push((c.noble, c.from, s));
                    }
                } else if defends(c) {
                    // defenders who lost the walls die with them
                } else {
                    returns.push((c.noble, c.from, c.count / 2));
                }
            }
            self.map.provinces[d].troops = held;
        } else {
            outcome = Outcome::Standoff;
            let mut held = garrison - garrison / 2;
            for c in &cs {
                if Some(c.noble) == owner && defends(c) {
                    held += c.count - c.count / 2;
                } else {
                    returns.push((c.noble, c.from, c.count / 2));
                }
            }
            self.map.provinces[d].troops = held;
        }
        let to = Visibility::Private(witnesses.into_iter().collect());
        self.record(to, EventKind::Battle { at: dest, defender: owner, garrison, marched: cs, outcome });
    }

    fn eliminate(&mut self) {
        let gone: Vec<NobleId> =
            self.alive().filter(|n| self.provinces_of(n.id).next().is_none()).map(|n| n.id).collect();
        for n in gone {
            self.nobles[n as usize].alive = false;
            self.nobles[n as usize].liege = None;
            for v in self.nobles.iter_mut().filter(|v| v.liege == Some(n)) {
                v.liege = None;
            }
            self.offers.retain(|o| o.from != n && o.to != n);
            self.record(Visibility::Public, EventKind::Eliminated { noble: n });
        }
    }

    fn economy(&mut self) {
        let mut gross = vec![0u32; self.nobles.len()];
        for p in &self.map.provinces {
            if let Some(o) = p.owner {
                gross[o as usize] += p.kind.income() + self.nobles[o as usize].faction.income_bonus(p.kind);
            }
        }
        for (n, g) in self.nobles.iter_mut().zip(&gross).filter(|(n, _)| n.alive) {
            n.gold += g;
        }
        for (i, g) in gross.iter().enumerate() {
            if let (true, Some(l)) = (self.nobles[i].alive, self.nobles[i].liege) {
                let tribute = g * self.rules.tribute_pct / 100;
                self.nobles[i].gold -= tribute;
                self.nobles[l as usize].gold += tribute;
            }
        }
        for i in 0..self.nobles.len() {
            if !self.nobles[i].alive {
                continue;
            }
            let per = self.nobles[i].faction.upkeep_per(self.rules.troops_per_upkeep).max(1);
            let n = i as NobleId;
            let upkeep = self.troops_of(n) / per;
            if self.nobles[i].gold >= upkeep {
                self.nobles[i].gold -= upkeep;
                continue;
            }
            let short = upkeep - self.nobles[i].gold;
            self.nobles[i].gold = 0;
            let mut desert = short * per;
            let total = desert;
            while desert > 0 {
                let Some(p) = self.map.provinces.iter().filter(|p| p.owner == Some(n) && p.troops > 0).max_by_key(|p| p.troops).map(|p| p.id)
                else {
                    break;
                };
                self.map.provinces[p as usize].troops -= 1;
                desert -= 1;
            }
            self.record(Visibility::Private(vec![n]), EventKind::Deserted { noble: n, count: total - desert });
        }
    }

    fn earned_title(&self, n: NobleId) -> Title {
        let noble = &self.nobles[n as usize];
        let power = self.power(n);
        let r = &self.rules;
        if self.can_be_crowned(n) {
            Title::King
        } else if power >= r.duke_power && noble.prestige >= r.duke_prestige {
            Title::Duke
        } else if power >= r.count_power && noble.prestige >= r.count_prestige {
            Title::Count
        } else {
            Title::Lord
        }
    }

    /// King of your people: answer to no one, hold their crown seat, command
    /// enough of the realm, and have every other noble of your people sworn
    /// to you, directly or through your vassals.
    pub fn can_be_crowned(&self, n: NobleId) -> bool {
        let noble = &self.nobles[n as usize];
        if !noble.alive || noble.liege.is_some() || noble.prestige < self.rules.king_prestige {
            return false;
        }
        let Some(seat) = self.map.seat_of(noble.faction) else { return false };
        if self.map.provinces[seat as usize].owner != Some(n) {
            return false;
        }
        let total = self.map.provinces.len() as u32;
        if self.power(n) * 100 < self.rules.king_share_pct * total {
            return false;
        }
        self.alive().filter(|o| o.faction == noble.faction && o.id != n).all(|o| self.is_above(n, o.id))
    }

    fn titles_and_crown(&mut self) {
        let mut crowned: Vec<NobleId> = vec![];
        for i in 0..self.nobles.len() {
            let n = i as NobleId;
            if !self.nobles[i].alive {
                continue;
            }
            let t = self.earned_title(n);
            if t == Title::King {
                crowned.push(n);
            }
            if t > self.nobles[i].title {
                self.nobles[i].title = t;
                if t != Title::King {
                    self.record(Visibility::Public, EventKind::Titled { noble: n, title: t });
                }
            }
        }
        if let Some(&w) = crowned.iter().max_by_key(|&&n| (self.power(n), self.nobles[n as usize].prestige, std::cmp::Reverse(n))) {
            // Two crowns in one season: the greater realm takes it; the other
            // keeps the title but not the game.
            self.winner = Some(w);
            self.record(Visibility::Public, EventKind::GameOver { winner: w, crowned: true });
        }
    }
}
