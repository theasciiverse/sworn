//! Letters and the offers they carry. A letter can say anything and promise
//! anything; only an offer binds, and only once accepted. Offers are always
//! between two nobles, so a deal can be kept secret from everyone else.

use crate::chronicle::{EventKind, Visibility};
use crate::game::Game;
use crate::{NobleId, OfferId, ProvinceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    Gold(u32),
    Province(ProvinceId),
    /// The side giving this swears fealty to the other.
    Fealty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub id: OfferId,
    pub from: NobleId,
    pub to: NobleId,
    /// What `from` hands to `to`.
    pub give: Vec<Term>,
    /// What `to` hands to `from` in return.
    pub want: Vec<Term>,
    /// Season it was sent in.
    pub season: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Letter {
    pub id: u32,
    pub from: NobleId,
    /// Everyone it went to. Only they (and the sender) can read it.
    pub to: Vec<NobleId>,
    pub season: u32,
    pub body: String,
    pub offer: Option<OfferId>,
}

pub const MAX_LETTER: usize = 4000;

impl Game {
    /// Sends a letter now, during the orders phase. An offer can go only to
    /// one noble. Returns the letter's id.
    pub fn send_letter(
        &mut self,
        from: NobleId,
        to: &[NobleId],
        body: &str,
        offer: Option<(Vec<Term>, Vec<Term>)>,
    ) -> Result<u32, String> {
        if self.winner.is_some() {
            return Err("the game is over".into());
        }
        if !self.noble(from).is_some_and(|n| n.alive) {
            return Err("only nobles still in the game can write".into());
        }
        let mut to: Vec<NobleId> = to.iter().copied().filter(|&t| t != from).collect();
        to.sort_unstable();
        to.dedup();
        if to.is_empty() {
            return Err("a letter needs someone to go to".into());
        }
        if let Some(&bad) = to.iter().find(|&&t| !self.noble(t).is_some_and(|n| n.alive)) {
            return Err(format!("noble {bad} is not in the game"));
        }
        if body.len() > MAX_LETTER {
            return Err("the letter is too long".into());
        }
        let offer_id = match offer {
            None => None,
            Some((give, want)) => {
                if to.len() != 1 {
                    return Err("an offer can go to one noble only".into());
                }
                if give.is_empty() && want.is_empty() {
                    return Err("an offer must offer or ask something".into());
                }
                let o = Offer { id: self.next_offer, from, to: to[0], give, want, season: self.season };
                self.check_offer(&o)?;
                self.next_offer += 1;
                let id = o.id;
                self.offers.push(o);
                Some(id)
            }
        };
        let id = self.next_letter;
        self.next_letter += 1;
        self.letters.push(Letter { id, from, to, season: self.season, body: body.into(), offer: offer_id });
        log::debug!("letter {id} from {from} offer {offer_id:?}");
        Ok(id)
    }

    /// Whether both sides could honour the offer right now.
    pub fn check_offer(&self, o: &Offer) -> Result<(), String> {
        let (Some(a), Some(b)) = (self.noble(o.from), self.noble(o.to)) else {
            return Err("no such noble".into());
        };
        if !a.alive || !b.alive {
            return Err("one side is no longer in the game".into());
        }
        let gold = |ts: &[Term]| ts.iter().map(|t| if let Term::Gold(g) = t { *g } else { 0 }).sum::<u32>();
        if gold(&o.give) > a.gold {
            return Err(format!("{} cannot pay {} gold", a.name, gold(&o.give)));
        }
        if gold(&o.want) > b.gold {
            return Err(format!("{} cannot pay {} gold", b.name, gold(&o.want)));
        }
        for (ts, who) in [(&o.give, o.from), (&o.want, o.to)] {
            for t in ts {
                if let Term::Province(p) = t {
                    if self.map.get(*p).and_then(|p| p.owner) != Some(who) {
                        return Err(format!("{} does not hold that province", self.nobles[who as usize].name));
                    }
                }
            }
        }
        let swears_give = o.give.contains(&Term::Fealty);
        let swears_want = o.want.contains(&Term::Fealty);
        if swears_give && swears_want {
            return Err("both sides cannot swear to each other".into());
        }
        if swears_give {
            self.can_swear(o.from, o.to)?;
        }
        if swears_want {
            self.can_swear(o.to, o.from)?;
        }
        Ok(())
    }

    pub fn can_swear(&self, vassal: NobleId, liege: NobleId) -> Result<(), String> {
        if self.nobles[vassal as usize].liege.is_some() {
            return Err(format!("{} is already sworn to another", self.nobles[vassal as usize].name));
        }
        if self.is_above(vassal, liege) {
            return Err("a lord cannot swear to their own vassal".into());
        }
        Ok(())
    }

    /// Carries out an accepted offer, or records why it fell through.
    pub(crate) fn execute_offer(&mut self, o: &Offer) {
        let both = Visibility::Private(vec![o.from, o.to]);
        if let Err(reason) = self.check_offer(o) {
            self.record(both, EventKind::DealFailed { offer: o.clone(), reason });
            return;
        }
        for (ts, giver, taker) in [(&o.give, o.from, o.to), (&o.want, o.to, o.from)] {
            for t in ts {
                match *t {
                    Term::Gold(g) => {
                        self.nobles[giver as usize].gold -= g;
                        self.nobles[taker as usize].gold += g;
                    }
                    // The garrison changes allegiance with the walls.
                    Term::Province(p) => self.map.provinces[p as usize].owner = Some(taker),
                    Term::Fealty => {}
                }
            }
        }
        self.record(both, EventKind::Deal { offer: o.clone() });
        if o.give.contains(&Term::Fealty) {
            self.swear(o.from, o.to);
        }
        if o.want.contains(&Term::Fealty) {
            self.swear(o.to, o.from);
        }
    }

    fn swear(&mut self, vassal: NobleId, liege: NobleId) {
        self.nobles[vassal as usize].liege = Some(liege);
        self.nobles[liege as usize].prestige += self.rules.vassal_prestige;
        self.record(Visibility::Public, EventKind::Sworn { vassal, liege });
    }
}
