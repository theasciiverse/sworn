//! The realm as a graph of provinces joined by roads. Positions are in map
//! units (0..1000 on both axes) for the renderer to scale to the terminal.

use crate::faction::Faction;
use crate::rng::Rng;
use crate::{NobleId, ProvinceId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Hold,
    Town,
    Fortress,
    /// The crown seat of a people: whoever would be their king must hold it.
    Seat(Faction),
}

impl Kind {
    pub fn income(self) -> u32 {
        match self {
            Kind::Hold => 2,
            Kind::Town => 4,
            Kind::Fortress => 1,
            Kind::Seat(_) => 5,
        }
    }

    /// Defender strength multiplier, percent.
    pub fn defence_pct(self) -> u64 {
        match self {
            Kind::Hold | Kind::Town => 100,
            Kind::Fortress => 200,
            Kind::Seat(_) => 150,
        }
    }

    /// Troops sitting in it before anyone claims it.
    pub fn neutral_garrison(self) -> u32 {
        match self {
            Kind::Hold => 2,
            Kind::Town => 4,
            Kind::Fortress => 5,
            Kind::Seat(_) => 9,
        }
    }

    pub fn raise_multiplier(self) -> u32 {
        match self {
            Kind::Town | Kind::Seat(_) => 2,
            _ => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Hold => "hold",
            Kind::Town => "town",
            Kind::Fortress => "fortress",
            Kind::Seat(_) => "crown seat",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Province {
    pub id: ProvinceId,
    pub name: String,
    pub kind: Kind,
    pub x: u16,
    pub y: u16,
    pub neighbours: Vec<ProvinceId>,
    /// `None`: unclaimed, its garrison is local levies loyal to no one.
    pub owner: Option<NobleId>,
    pub troops: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Map {
    pub provinces: Vec<Province>,
}

const PREFIXES: &[&str] = &[
    "Ash", "Bram", "Cal", "Dun", "Eld", "Fen", "Gar", "Hal", "Ivr", "Kel", "Lin", "Mor", "Nor", "Os", "Ral", "Sel",
    "Thorn", "Ul", "Ver", "Wyn", "Ard", "Brey", "Cor", "Drav", "Esk", "Grim", "Hart", "Lod", "Mar", "Ruth", "Sten",
    "Tav", "Wend", "Yar",
];
const SUFFIXES: &[&str] = &[
    "ford", "holm", "by", "wick", "stead", "mere", "ton", "burg", "dale", "croft", "garth", "ley", "hithe", "march",
    "combe", "thorp", "moor", "hall",
];

impl Map {
    pub fn get(&self, id: ProvinceId) -> Option<&Province> {
        self.provinces.get(id as usize)
    }

    pub fn adjacent(&self, a: ProvinceId, b: ProvinceId) -> bool {
        self.get(a).is_some_and(|p| p.neighbours.contains(&b))
    }

    pub fn seat_of(&self, f: Faction) -> Option<ProvinceId> {
        self.provinces.iter().find(|p| p.kind == Kind::Seat(f)).map(|p| p.id)
    }

    /// Road hops from `from` to every province (`u32::MAX` if unreachable).
    pub fn distances(&self, from: ProvinceId) -> Vec<u32> {
        let mut d = vec![u32::MAX; self.provinces.len()];
        let mut q = VecDeque::from([from]);
        d[from as usize] = 0;
        while let Some(p) = q.pop_front() {
            for &n in &self.provinces[p as usize].neighbours {
                if d[n as usize] == u32::MAX {
                    d[n as usize] = d[p as usize] + 1;
                    q.push_back(n);
                }
            }
        }
        d
    }

    pub fn connected(&self) -> bool {
        self.provinces.is_empty() || self.distances(0).iter().all(|&d| d != u32::MAX)
    }

    /// Builds a map from positions, kinds and roads; for tests and scenarios.
    pub fn from_parts(parts: &[(&str, Kind, u16, u16)], roads: &[(ProvinceId, ProvinceId)]) -> Map {
        let mut provinces: Vec<Province> = parts
            .iter()
            .enumerate()
            .map(|(i, &(name, kind, x, y))| Province {
                id: i as ProvinceId,
                name: name.into(),
                kind,
                x,
                y,
                neighbours: vec![],
                owner: None,
                troops: kind.neutral_garrison(),
            })
            .collect();
        for &(a, b) in roads {
            provinces[a as usize].neighbours.push(b);
            provinces[b as usize].neighbours.push(a);
        }
        for p in &mut provinces {
            p.neighbours.sort_unstable();
            p.neighbours.dedup();
        }
        Map { provinces }
    }

    /// A jittered grid with some diagonals and some roads cut, about five
    /// provinces per noble. Returns the map and one start province per noble,
    /// spread as far apart as the roads allow.
    pub fn generate(rng: &mut Rng, nobles: usize, seats: &[Faction]) -> (Map, Vec<ProvinceId>) {
        let want = nobles * 5 + seats.len() + 2;
        let cols = ((want as f64 * 1.6).sqrt().ceil() as usize).max(2);
        let rows = want.div_ceil(cols).max(2);
        let n = cols * rows;
        let at = |c: usize, r: usize| (r * cols + c) as ProvinceId;

        let mut names = BTreeSet::new();
        let mut provinces = Vec::with_capacity(n);
        for r in 0..rows {
            for c in 0..cols {
                let jx = rng.below(41) as f64 / 100.0 - 0.2;
                let jy = rng.below(41) as f64 / 100.0 - 0.2;
                let x = ((c as f64 + 0.5 + jx) / cols as f64 * 1000.0) as u16;
                let y = ((r as f64 + 0.5 + jy) / rows as f64 * 1000.0) as u16;
                let name = loop {
                    let s = format!("{}{}", rng.pick(PREFIXES), rng.pick(SUFFIXES));
                    if names.insert(s.clone()) {
                        break s;
                    }
                };
                provinces.push(Province { id: at(c, r), name, kind: Kind::Hold, x, y, neighbours: vec![], owner: None, troops: 0 });
            }
        }
        let mut roads = BTreeSet::new();
        for r in 0..rows {
            for c in 0..cols {
                if c + 1 < cols {
                    roads.insert((at(c, r), at(c + 1, r)));
                }
                if r + 1 < rows {
                    roads.insert((at(c, r), at(c, r + 1)));
                }
                if c + 1 < cols && r + 1 < rows && rng.chance(45) {
                    // one diagonal per square at most, so roads never cross
                    if rng.chance(50) {
                        roads.insert((at(c, r), at(c + 1, r + 1)));
                    } else {
                        roads.insert((at(c + 1, r), at(c, r + 1)));
                    }
                }
            }
        }
        let mut map = Map { provinces };
        let link = |map: &mut Map, roads: &BTreeSet<(ProvinceId, ProvinceId)>| {
            for p in &mut map.provinces {
                p.neighbours.clear();
            }
            for &(a, b) in roads {
                map.provinces[a as usize].neighbours.push(b);
                map.provinces[b as usize].neighbours.push(a);
            }
            for p in &mut map.provinces {
                p.neighbours.sort_unstable();
            }
        };
        link(&mut map, &roads);

        // Cut some roads, so the realm has chokepoints, but keep it whole.
        let mut candidates: Vec<_> = roads.iter().copied().collect();
        rng.shuffle(&mut candidates);
        for (a, b) in candidates {
            if !rng.chance(22) {
                continue;
            }
            if map.provinces[a as usize].neighbours.len() <= 2 || map.provinces[b as usize].neighbours.len() <= 2 {
                continue;
            }
            roads.remove(&(a, b));
            link(&mut map, &roads);
            if !map.connected() {
                roads.insert((a, b));
                link(&mut map, &roads);
            }
        }

        let starts = map.spread(rng, nobles, &[], &BTreeSet::new());
        let near_start: BTreeSet<ProvinceId> =
            starts.iter().flat_map(|&s| map.provinces[s as usize].neighbours.iter().copied().chain([s])).collect();
        let seat_ids = map.spread(rng, seats.len(), &starts, &near_start);
        for (&id, &f) in seat_ids.iter().zip(seats) {
            map.provinces[id as usize].kind = Kind::Seat(f);
        }
        let mut rest: Vec<ProvinceId> =
            (0..n as ProvinceId).filter(|id| !starts.contains(id) && !seat_ids.contains(id)).collect();
        rng.shuffle(&mut rest);
        let towns = rest.len() / 5;
        let forts = rest.len() * 3 / 20;
        for (i, &id) in rest.iter().enumerate() {
            map.provinces[id as usize].kind = if i < towns {
                Kind::Town
            } else if i < towns + forts {
                Kind::Fortress
            } else {
                Kind::Hold
            };
        }
        for p in &mut map.provinces {
            p.troops = p.kind.neutral_garrison();
        }
        (map, starts)
    }

    /// Farthest-point sampling by road distance: `count` provinces, each as
    /// far as possible from those already chosen and from `away_from`.
    fn spread(&self, rng: &mut Rng, count: usize, away_from: &[ProvinceId], excluded: &BTreeSet<ProvinceId>) -> Vec<ProvinceId> {
        let n = self.provinces.len();
        let mut chosen: Vec<ProvinceId> = vec![];
        let mut best_dist = vec![u32::MAX; n];
        for &a in away_from {
            for (i, d) in self.distances(a).into_iter().enumerate() {
                best_dist[i] = best_dist[i].min(d);
            }
        }
        for _ in 0..count {
            let pool: Vec<ProvinceId> =
                (0..n as ProvinceId).filter(|id| !chosen.contains(id) && !away_from.contains(id)).collect();
            let open: Vec<ProvinceId> = pool.iter().copied().filter(|id| !excluded.contains(id)).collect();
            let pool = if open.is_empty() { pool } else { open };
            let far = pool.iter().map(|&id| best_dist[id as usize]).max().unwrap_or(0);
            let best: Vec<ProvinceId> = pool.into_iter().filter(|&id| best_dist[id as usize] == far).collect();
            let pick = *rng.pick(&best);
            chosen.push(pick);
            for (i, d) in self.distances(pick).into_iter().enumerate() {
                best_dist[i] = best_dist[i].min(d);
            }
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_maps_are_whole_and_repeatable() {
        for seed in 0..40 {
            for nobles in 2..=8 {
                let seats = &Faction::ALL[..nobles.min(4)];
                let (a, sa) = Map::generate(&mut Rng::new(seed), nobles, seats);
                let (b, sb) = Map::generate(&mut Rng::new(seed), nobles, seats);
                assert_eq!(a, b);
                assert_eq!(sa, sb);
                assert!(a.connected(), "seed {seed} nobles {nobles}");
                assert_eq!(sa.len(), nobles);
                let distinct: BTreeSet<_> = sa.iter().collect();
                assert_eq!(distinct.len(), nobles);
                for f in seats {
                    assert!(a.seat_of(*f).is_some());
                }
                for p in &a.provinces {
                    for &q in &p.neighbours {
                        assert!(a.adjacent(q, p.id));
                    }
                }
            }
        }
    }
}
