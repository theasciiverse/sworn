//! What happened, and who knows about it.

use crate::diplomacy::{Offer, Term};
use crate::game::{Game, Title};
use crate::view::View;
use crate::{NobleId, ProvinceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private(Vec<NobleId>),
}

impl Visibility {
    pub fn includes(&self, n: NobleId) -> bool {
        match self {
            Visibility::Public => true,
            Visibility::Private(v) => v.contains(&n),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Held,
    Taken(NobleId),
    /// No side was strongest: everyone bled and nothing changed hands.
    Standoff,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    pub noble: NobleId,
    pub from: ProvinceId,
    pub count: u32,
    pub on_behalf: NobleId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Deal { offer: Offer },
    DealFailed { offer: Offer, reason: String },
    Sworn { vassal: NobleId, liege: NobleId },
    /// Marched against their own liege or vassal.
    OathBroken { breaker: NobleId, victim: NobleId, at: ProvinceId },
    Renounced { vassal: NobleId, liege: NobleId },
    Released { liege: NobleId, vassal: NobleId },
    Raised { noble: NobleId, at: ProvinceId, count: u32 },
    Moved { noble: NobleId, from: ProvinceId, to: ProvinceId, count: u32 },
    /// Came to an ally's province to help hold it; no one attacked.
    StoodGuard { noble: NobleId, at: ProvinceId, count: u32 },
    Battle { at: ProvinceId, defender: Option<NobleId>, garrison: u32, marched: Vec<Contribution>, outcome: Outcome },
    /// Troops came home to find home taken.
    Scattered { noble: NobleId, home: ProvinceId, count: u32 },
    Deserted { noble: NobleId, count: u32 },
    Snowbound { noble: NobleId },
    Eliminated { noble: NobleId },
    Titled { noble: NobleId, title: Title },
    GameOver { winner: NobleId, crowned: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub season: u32,
    pub to: Visibility,
    pub kind: EventKind,
}

/// Names for describing events, from the whole game or from one noble's view.
pub trait Names {
    fn noble_name(&self, n: NobleId) -> String;
    fn province_name(&self, p: ProvinceId) -> String;
    fn faction_name(&self, n: NobleId) -> &'static str;
}

impl Names for Game {
    fn noble_name(&self, n: NobleId) -> String {
        self.noble(n).map_or_else(|| format!("noble {n}"), |n| n.name.clone())
    }
    fn province_name(&self, p: ProvinceId) -> String {
        self.map.get(p).map_or_else(|| format!("province {p}"), |p| p.name.clone())
    }
    fn faction_name(&self, n: NobleId) -> &'static str {
        self.noble(n).map_or("", |n| n.faction.name())
    }
}

impl Names for View {
    fn noble_name(&self, n: NobleId) -> String {
        self.nobles.get(n as usize).map_or_else(|| format!("noble {n}"), |n| n.name.clone())
    }
    fn province_name(&self, p: ProvinceId) -> String {
        self.provinces.get(p as usize).map_or_else(|| format!("province {p}"), |p| p.name.clone())
    }
    fn faction_name(&self, n: NobleId) -> &'static str {
        self.nobles.get(n as usize).map_or("", |n| n.faction.name())
    }
}

impl Event {
    pub fn describe(&self, g: &impl Names) -> String {
        let who = |n: NobleId| g.noble_name(n);
        let at = |p: ProvinceId| g.province_name(p);
        let terms = |ts: &[Term]| -> String {
            if ts.is_empty() {
                return "nothing".into();
            }
            ts.iter()
                .map(|t| match t {
                    Term::Gold(n) => format!("{n} gold"),
                    Term::Province(p) => at(*p),
                    Term::Fealty => "an oath of fealty".into(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        match &self.kind {
            EventKind::Deal { offer: o } => {
                format!("{} and {} struck a deal: {} for {}.", who(o.from), who(o.to), terms(&o.give), terms(&o.want))
            }
            EventKind::DealFailed { offer: o, reason } => {
                format!("The deal between {} and {} fell through: {reason}.", who(o.from), who(o.to))
            }
            EventKind::Sworn { vassal, liege } => format!("{} swore fealty to {}.", who(*vassal), who(*liege)),
            EventKind::OathBroken { breaker, victim, at: p } => {
                format!("Oathbreaker! {} marched on {} at {}.", who(*breaker), who(*victim), at(*p))
            }
            EventKind::Renounced { vassal, liege } => format!("{} renounced their oath to {}.", who(*vassal), who(*liege)),
            EventKind::Released { liege, vassal } => format!("{} released {} from their oath.", who(*liege), who(*vassal)),
            EventKind::Raised { noble, at: p, count } => format!("{} raised {count} troops at {}.", who(*noble), at(*p)),
            EventKind::Moved { noble, from, to, count } => {
                format!("{} moved {count} troops from {} to {}.", who(*noble), at(*from), at(*to))
            }
            EventKind::StoodGuard { noble, at: p, count } => {
                format!("{} sent {count} troops to guard {}; no one came.", who(*noble), at(*p))
            }
            EventKind::Battle { at: p, defender, garrison, marched, outcome } => {
                let def = defender.map_or_else(|| "local levies".into(), who);
                let mut sides: Vec<String> = vec![];
                for c in marched {
                    if c.noble == c.on_behalf {
                        sides.push(format!("{} with {}", who(c.noble), c.count));
                    } else {
                        sides.push(format!("{} with {} for {}", who(c.noble), c.count, who(c.on_behalf)));
                    }
                }
                let end = match outcome {
                    Outcome::Held => format!("{def} held."),
                    Outcome::Taken(w) => format!("{} took it.", who(*w)),
                    Outcome::Standoff => "Neither side prevailed.".into(),
                };
                format!("Battle at {} ({def} with {garrison}; {}). {end}", at(*p), sides.join("; "))
            }
            EventKind::Scattered { noble, home, count } => {
                format!("{count} of {}'s troops came home to {} and found it lost; they scattered.", who(*noble), at(*home))
            }
            EventKind::Deserted { noble, count } => format!("Unpaid, {count} of {}'s troops deserted.", who(*noble)),
            EventKind::Snowbound { noble } => format!("Snow closed the roads; {}'s marches did not go.", who(*noble)),
            EventKind::Eliminated { noble } => format!("{} lost their last province and is gone from the realm.", who(*noble)),
            EventKind::Titled { noble, title } => format!("{} is now {}.", who(*noble), title.name()),
            EventKind::GameOver { winner, crowned: true } => {
                let f = g.faction_name(*winner);
                format!("{} is crowned King of the {f}!", who(*winner))
            }
            EventKind::GameOver { winner, crowned: false } => {
                format!("The years ran out. {} stands highest in the realm.", who(*winner))
            }
        }
    }
}
