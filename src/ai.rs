//! Computer nobles. They fill empty seats and let one player test the game
//! alone. Plain greed with a little prudence: take what is weakly held, keep
//! the borders manned, kneel when hopelessly outmatched, take good deals, and
//! throw off a liege who has become the weaker of the two.
//!
//! They plan from a `View`, so they know no more than a player would.

use crate::chronicle::{EventKind, Outcome};
use crate::diplomacy::Term;
use crate::orders::Order;
use crate::rng::Rng;
use crate::view::View;
use crate::{is_winter, NobleId, ProvinceId};
use std::collections::{BTreeMap, VecDeque};

pub struct Draft {
    pub to: Vec<NobleId>,
    pub body: String,
    pub offer: Option<(Vec<Term>, Vec<Term>)>,
}

pub struct AiTurn {
    pub orders: Vec<Order>,
    /// Written at the start of a season, so others can answer in time.
    pub letters: Vec<Draft>,
    /// Answers to offers, written when giving orders.
    pub replies: Vec<Draft>,
}

/// Guess at an unseen garrison.
const UNSEEN: u32 = 6;
/// Gold paid within the last year that buys a computer noble's peace.
const TRUCE_PRICE: u32 = 4;
/// A noble with this share of the realm is everyone's target.
const LEADER_PCT: u32 = 25;

fn gold_in(ts: &[Term]) -> u32 {
    ts.iter().map(|t| if let Term::Gold(g) = t { *g } else { 0 }).sum()
}

/// Gold each noble has paid me in deals over the last four seasons.
fn paid_me(v: &View) -> BTreeMap<NobleId, u32> {
    let mut paid = BTreeMap::new();
    for e in v.events.iter().filter(|e| e.season + 4 >= v.season) {
        if let EventKind::Deal { offer: o } = &e.kind {
            if o.to == v.me {
                *paid.entry(o.from).or_default() += gold_in(&o.give);
            } else if o.from == v.me {
                *paid.entry(o.to).or_default() += gold_in(&o.want);
            }
        }
    }
    paid
}

/// Gold I have paid each noble in deals over the last four seasons: peace
/// bought is peace kept, or the plea was a lie.
fn i_paid(v: &View) -> BTreeMap<NobleId, u32> {
    let mut paid = BTreeMap::new();
    for e in v.events.iter().filter(|e| e.season + 4 >= v.season) {
        if let EventKind::Deal { offer: o } = &e.kind {
            if o.from == v.me {
                *paid.entry(o.to).or_default() += gold_in(&o.give);
            } else if o.to == v.me {
                *paid.entry(o.from).or_default() += gold_in(&o.want);
            }
        }
    }
    paid
}

/// Letters written at the start of a season: pleas, demands, warnings.
fn write_letters(v: &View, mine: &[ProvinceId], paid: &BTreeMap<NobleId, u32>, gold: u32, turn: &mut AiTurn) {
    let me = v.noble(v.me);
    let offered_to = |n: NobleId| v.offers.iter().any(|o| o.from == v.me && o.to == n);
    let neighbours: Vec<NobleId> = {
        let mut ns: Vec<NobleId> = mine
            .iter()
            .flat_map(|&p| v.province(p).neighbours.iter().filter_map(|&q| v.province(q).owner))
            .filter(|&o| o != v.me && v.noble(o).alive)
            .collect();
        ns.sort_unstable();
        ns.dedup();
        ns
    };
    let hurt_by: Vec<NobleId> = v
        .events
        .iter()
        .filter(|e| e.season + 1 >= v.season)
        .filter_map(|e| match &e.kind {
            EventKind::Battle { defender: Some(d), outcome: Outcome::Taken(w), .. } if *d == v.me => Some(*w),
            _ => None,
        })
        .collect();

    // First word to a player whose lands have come to touch mine: the
    // strong ask a year's tribute for peace, the rest say they want none.
    let written = |n: NobleId| v.letters.iter().any(|l| (l.from == v.me && l.to.contains(&n)) || (l.from == n && l.to.contains(&v.me)));
    if let Some(&n) = neighbours.iter().find(|&&n| !v.noble(n).ai && !written(n) && !offered_to(n)) {
        let them = v.noble(n);
        let at = mine
            .iter()
            .find(|&&p| v.province(p).neighbours.iter().any(|&q| v.province(q).owner == Some(n)))
            .map_or_else(String::new, |&p| format!(" at {}", v.province(p).name));
        let (body, offer) = if me.power >= them.power * 2 && me.liege.is_none() {
            (
                format!(
                    "{}, our lands meet now{at}. Send me {TRUCE_PRICE} gold and my men will find other work this year. Keep your purse shut and they may come for it instead.",
                    them.name
                ),
                Some((vec![], vec![Term::Gold(TRUCE_PRICE)])),
            )
        } else if them.power >= me.power * 2 {
            (
                format!("{}, our lands meet now{at}. I am a small lord and want no quarrel with you. Look elsewhere, and so shall I.", them.name),
                None,
            )
        } else {
            (
                format!(
                    "{}, our lands meet now{at}. Keep your border manned and we shall get on well enough; an empty hold tempts any lord. {TRUCE_PRICE} gold would buy my peace for a year.",
                    them.name
                ),
                None,
            )
        };
        turn.letters.push(Draft { to: vec![n], body, offer });
        return;
    }

    // Plead with whoever took land from me, if I cannot hope to win it back.
    if let Some(&foe) = hurt_by.iter().find(|&&f| v.noble(f).power >= me.power * 2 && !offered_to(f) && me.liege != Some(f)) {
        let price = (gold / 2).clamp(0, 12);
        if price >= TRUCE_PRICE && paid.get(&foe).is_none() {
            turn.letters.push(Draft {
                to: vec![foe],
                body: format!("{}, take this and leave my lands in peace. I have no quarrel with you.", v.noble(foe).name),
                offer: Some((vec![Term::Gold(price)], vec![])),
            });
            return;
        }
    }

    // Demand fealty of the weak, my own people first: the crown needs them.
    if me.liege.is_none() && v.season.is_multiple_of(2) {
        let mut weak: Vec<NobleId> = neighbours
            .iter()
            .copied()
            .filter(|&n| {
                let o = v.noble(n);
                o.liege.is_none() && !offered_to(n) && (o.power * 3 <= me.power || o.faction == me.faction && o.power * 2 <= me.power)
            })
            .collect();
        weak.sort_by_key(|&n| (v.noble(n).faction != me.faction, v.noble(n).power));
        if let Some(&n) = weak.first() {
            let sweetener = if gold >= 12 { 6 } else { 0 };
            let give = if sweetener > 0 { vec![Term::Gold(sweetener)] } else { vec![] };
            let kin = if v.noble(n).faction == me.faction { format!(" we are both {}; ", me.faction.name()) } else { " ".into() };
            turn.letters.push(Draft {
                to: vec![n],
                body: format!("{},{}kneel to me and keep your lands. Refuse, and I will take them.", v.noble(n).name, kin),
                offer: Some((give, vec![Term::Fealty])),
            });
            return;
        }
    }

    // Once a year the strongest rival warns everyone else about the leader.
    let total = v.provinces.len() as u32;
    let alive: Vec<_> = v.nobles.iter().filter(|n| n.alive).collect();
    let leader = alive.iter().max_by_key(|n| n.power).filter(|n| n.power * 100 >= total * LEADER_PCT && n.id != v.me);
    let runner_up = alive.iter().filter(|n| Some(n.id) != leader.map(|l| l.id)).max_by_key(|n| (n.power, std::cmp::Reverse(n.id))).map(|n| n.id);
    if let (Some(l), true) = (leader, runner_up == Some(v.me) && v.season.is_multiple_of(4)) {
        let to: Vec<NobleId> = alive.iter().map(|n| n.id).filter(|&n| n != v.me && n != l.id && v.noble(n).liege != Some(l.id)).collect();
        if !to.is_empty() {
            turn.letters.push(Draft {
                to,
                body: format!(
                    "{} holds {} of the realm's {total} provinces and means to be king. Strike at {} this year, or bow to {} forever.",
                    l.name, l.power, l.name, l.name
                ),
                offer: None,
            });
        }
    }
}

pub fn plan(v: &View) -> AiTurn {
    let mut turn = AiTurn { orders: vec![], letters: vec![], replies: vec![] };
    if v.winner.is_some() || !v.noble(v.me).alive {
        return turn;
    }
    let mut rng = Rng::new(0xFEA1_7700 ^ ((v.season as u64) << 8) ^ v.me as u64);
    let me = v.noble(v.me);
    let mut gold = v.gold;

    // Deals. Anyone whose oath I take this season is a friend already.
    let mut new_vassals: Vec<NobleId> = vec![];
    // Whoever I kneel to this season is my liege before anyone marches.
    let mut new_liege: Option<NobleId> = None;
    // Land I am handing over this season: neither mine to use nor to take.
    let mut selling: Vec<ProvinceId> = vec![];
    for o in v.offers.iter().filter(|o| o.to == v.me) {
        // What a term is worth to me: received when `mine` is false, given
        // up when true (then it is a cost, subtracted below).
        let worth = |t: &Term, mine: bool| -> i64 {
            match *t {
                Term::Gold(g) => g as i64,
                Term::Province(p) => 8 + 3 * v.province(p).kind.income() as i64,
                Term::Fealty if !mine => 12,
                // Kneeling costs little when hopelessly outmatched, else never.
                Term::Fealty => {
                    let them = v.noble(o.from);
                    // Kin kneel sooner, but not to whoever is nearest the crown:
                    // that oath may be the last thing it lacks.
                    let total = v.provinces.len() as u32;
                    let near_crown = them.power * 100 >= total * LEADER_PCT;
                    let kin = them.faction == me.faction && me.power * 2 <= them.power && !near_crown;
                    if kin || me.power * 3 <= them.power {
                        4
                    } else {
                        1000
                    }
                }
            }
        };
        let cost: u32 = o.want.iter().map(|t| if let Term::Gold(g) = t { *g } else { 0 }).sum();
        let value: i64 = o.give.iter().map(|t| worth(t, false)).sum::<i64>() - o.want.iter().map(|t| worth(t, true)).sum::<i64>();
        let yes = value > 0 && cost <= gold;
        if yes {
            gold -= cost;
            turn.orders.push(Order::Accept(o.id));
            if o.give.contains(&Term::Fealty) {
                new_vassals.push(o.from);
            }
            if o.want.contains(&Term::Fealty) && me.liege.is_none() && new_liege.is_none() {
                new_liege = Some(o.from);
            }
            selling.extend(o.want.iter().filter_map(|t| if let Term::Province(p) = t { Some(*p) } else { None }));
        }
        if !v.noble(o.from).ai {
            let body = if yes {
                "Agreed. Let it be done this season."
            } else if cost > gold {
                "I could not pay that even if I wished to."
            } else {
                "No. Come back with something worth my while."
            };
            turn.replies.push(Draft { to: vec![o.from], body: body.into(), offer: None });
        }
    }

    // A liege who has fallen behind is no longer worth the tribute.
    if let Some(l) = me.liege {
        if me.power > 2 * v.noble(l).power || crown_but_for_oath(v) {
            turn.orders.push(Order::Renounce);
        }
    }
    let liege = if turn.orders.contains(&Order::Renounce) { None } else { me.liege.or(new_liege) };
    let paid = paid_me(v);
    let bought = i_paid(v);
    // Peace is not for sale to whoever is closest to the crown.
    let total = v.provinces.len() as u32;
    let leading = |n: NobleId| v.noble(n).power * 100 >= total * LEADER_PCT && v.nobles.iter().all(|o| o.id == n || o.power < v.noble(n).power);
    let truce = |n: NobleId| {
        (paid.get(&n).copied().unwrap_or(0) >= TRUCE_PRICE && !leading(n)) || bought.get(&n).copied().unwrap_or(0) >= TRUCE_PRICE
    };
    // Whoever I have just asked to kneel gets the season to answer.
    let asked = |n: NobleId| v.offers.iter().any(|o| o.from == v.me && o.to == n && o.season == v.season && o.want.contains(&Term::Fealty));
    let friendly = |n: NobleId| {
        n == v.me || liege == Some(n) || v.noble(n).liege == Some(v.me) || new_vassals.contains(&n) || truce(n) || asked(n)
    };
    let hostile = |p: ProvinceId| !selling.contains(&p) && v.province(p).owner.is_none_or(|o| !friendly(o));

    let mine: Vec<ProvinceId> = v.mine().map(|p| p.id).filter(|p| !selling.contains(p)).collect();
    let threat = |p: ProvinceId| -> u32 {
        v.province(p)
            .neighbours
            .iter()
            .filter(|&&q| v.province(q).owner.is_some_and(|o| !friendly(o)))
            .map(|&q| v.province(q).troops.unwrap_or(UNSEEN))
            .max()
            .unwrap_or(0)
    };
    let border: Vec<ProvinceId> = mine.iter().copied().filter(|&p| v.province(p).neighbours.iter().any(|&q| hostile(q))).collect();

    // Kneel if hopelessly outmatched by a neighbour, once a year.
    if liege.is_none() && me.power <= 1 && v.season.is_multiple_of(4) && !v.offers.iter().any(|o| o.from == v.me) {
        let strongest = mine
            .iter()
            .flat_map(|&p| v.province(p).neighbours.iter().filter_map(|&q| v.province(q).owner))
            .filter(|&o| o != v.me)
            .max_by_key(|&o| v.noble(o).power);
        if let Some(lord) = strongest.filter(|&o| v.noble(o).power >= 4) {
            turn.letters.push(Draft {
                to: vec![lord],
                body: format!("My lord {}, I would hold my lands under your banner.", v.noble(lord).name),
                offer: Some((vec![Term::Fealty], vec![])),
            });
        }
    }

    write_letters(v, &mine, &paid, gold, &mut turn);
    // Nor do I march on anyone I am offering gold for peace this season.
    // (Letters go out before orders, so the offer may already be open.)
    let pleading: Vec<NobleId> = turn
        .letters
        .iter()
        .filter(|d| d.offer.as_ref().is_some_and(|(give, want)| gold_in(give) > 0 && want.is_empty()))
        .flat_map(|d| d.to.iter().copied())
        .chain(v.offers.iter().filter(|o| o.from == v.me && gold_in(&o.give) > 0 && o.want.is_empty()).map(|o| o.to))
        .collect();
    let hostile = |p: ProvinceId| hostile(p) && v.province(p).owner.is_none_or(|o| !pleading.contains(&o));

    // What each conquest within reach needs, cheapest and richest first.
    let mut targets: Vec<(u32, ProvinceId)> = vec![];
    for p in v.provinces.iter().filter(|p| hostile(p.id)) {
        if !p.neighbours.iter().any(|q| mine.contains(q)) {
            continue;
        }
        let fac = p.owner.map_or(100, |o| v.noble(o).faction.defence_pct());
        let def = p.troops.unwrap_or(UNSEEN) as u64 * p.kind.defence_pct() * fac / 10_000;
        // Local levies never reinforce, so they need less of a margin.
        let margin = if p.owner.is_none() { def * 5 / 4 } else { def * 3 / 2 };
        let need = ((margin + 1) * 100).div_ceil(me.faction.attack_pct()) as u32;
        targets.push((need, p.id));
    }
    // Everyone leans on whoever is closest to the crown.
    let leader = v.nobles.iter().filter(|n| n.alive && n.id != v.me).max_by_key(|n| n.power).filter(|n| n.power * 100 >= total * LEADER_PCT).map(|n| n.id);
    // My people's crown seat is worth saving up for.
    let urgency = |need: u32, id: ProvinceId| {
        if v.province(id).kind == crate::map::Kind::Seat(me.faction) {
            need / 2
        } else if leader.is_some() && v.province(id).owner == leader {
            need * 2 / 3
        } else {
            need
        }
    };
    let mut keyed: Vec<_> =
        targets.into_iter().map(|(need, id)| ((urgency(need, id), std::cmp::Reverse(v.province(id).kind.income()), rng.below(100)), (need, id))).collect();
    keyed.sort_by_key(|k| k.0);
    let targets: Vec<(u32, ProvinceId)> = keyed.into_iter().map(|k| k.1).collect();

    // Raise, keeping a purse for upkeep: first what threatened borders need
    // to hold, then what the cheapest conquests lack (mustered where they
    // will march from), then the rest along the borders.
    let troops = |p: ProvinceId| v.province(p).troops.unwrap_or(0);
    let keep = |p: ProvinceId| if border.contains(&p) { (threat(p) / 2).max(1) } else { 0 };
    let my_troops: u32 = mine.iter().map(|&p| troops(p)).sum();
    let cost = me.faction.raise_cost(v.rules.raise_cost).max(1);
    let reserve = my_troops / me.faction.upkeep_per(v.rules.troops_per_upkeep).max(1) * 2 + 2;
    let mut budget = gold.saturating_sub(reserve) / cost;
    let cap = |p: ProvinceId| v.rules.raise_cap * v.province(p).kind.raise_multiplier();
    let mut raised: BTreeMap<ProvinceId, u32> = BTreeMap::new();
    let raise = |raised: &mut BTreeMap<ProvinceId, u32>, budget: &mut u32, p: ProvinceId, n: u32| {
        let r = raised.entry(p).or_default();
        let k = n.min(cap(p).saturating_sub(*r)).min(*budget);
        *r += k;
        *budget -= k;
    };
    let mut by_threat: Vec<ProvinceId> = border.clone();
    by_threat.sort_by_key(|&p| std::cmp::Reverse(threat(p)));
    for &p in &by_threat {
        raise(&mut raised, &mut budget, p, keep(p).saturating_sub(troops(p)));
    }
    let mut free: BTreeMap<ProvinceId, u32> = mine.iter().map(|&p| (p, (troops(p) + raised.get(&p).copied().unwrap_or(0)).saturating_sub(keep(p)))).collect();
    let mut mustered = 0;
    for &(need, t) in &targets {
        if mustered >= 3 || budget == 0 {
            break;
        }
        let mut sources: Vec<ProvinceId> = v.province(t).neighbours.iter().copied().filter(|q| mine.contains(q)).collect();
        let have: u32 = sources.iter().map(|q| free[q]).sum();
        let room: u32 = sources.iter().map(|&q| cap(q).saturating_sub(raised.get(&q).copied().unwrap_or(0))).sum::<u32>().min(budget);
        if have + room < need.max(1) {
            continue;
        }
        let mut short = need.max(1).saturating_sub(have);
        sources.sort_by_key(|&q| std::cmp::Reverse(cap(q).saturating_sub(raised.get(&q).copied().unwrap_or(0))));
        for &q in &sources {
            let before = raised.get(&q).copied().unwrap_or(0);
            raise(&mut raised, &mut budget, q, short);
            short -= raised[&q] - before;
        }
        for q in sources {
            free.insert(q, 0);
        }
        mustered += 1;
    }
    // Gold in the chest wins nothing: what the borders cannot take is
    // raised inside, to march out to them.
    let mut rest = by_threat.clone();
    rest.extend(mine.iter().copied().filter(|p| !border.contains(p)));
    while budget > 0 && !rest.is_empty() {
        let before = budget;
        for &p in &rest {
            raise(&mut raised, &mut budget, p, 1);
        }
        if budget == before {
            break;
        }
    }
    raised.retain(|_, n| *n > 0);
    for (&at, &count) in &raised {
        turn.orders.push(Order::Raise { at, count });
    }

    if is_winter(v.season) && !v.rules.winter_marches {
        return turn;
    }

    // March.
    let mut spare: BTreeMap<ProvinceId, u32> = BTreeMap::new();
    for &p in &mine {
        let have = v.province(p).troops.unwrap_or(0) + raised.get(&p).copied().unwrap_or(0);
        spare.insert(p, have.saturating_sub(keep(p)));
    }
    answer_calls(v, liege, &mine, &mut spare, &mut turn);

    let mut attacks = 0;
    for (need, t) in targets {
        if attacks >= 3 {
            break;
        }
        let mut sources: Vec<ProvinceId> = v.province(t).neighbours.iter().copied().filter(|q| spare.get(q).copied().unwrap_or(0) > 0).collect();
        sources.sort_by_key(|q| std::cmp::Reverse(spare[q]));
        let total: u32 = sources.iter().map(|q| spare[q]).sum();
        if total < need.max(1) {
            continue;
        }
        let mut left = need.max(1);
        for q in sources {
            let c = spare[&q].min(left);
            if c == 0 {
                break;
            }
            *spare.get_mut(&q).unwrap() -= c;
            left -= c;
            turn.orders.push(Order::March { from: q, to: t, count: c, on_behalf: None });
        }
        attacks += 1;
    }

    // Send idle troops from the interior one step toward the nearest border.
    if !border.is_empty() {
        let dist = border_distance(v, &mine, &border);
        for &p in &mine {
            let s = spare.get(&p).copied().unwrap_or(0);
            if s == 0 || border.contains(&p) {
                continue;
            }
            let step = v.province(p).neighbours.iter().copied().filter(|q| mine.contains(q)).min_by_key(|q| dist.get(q).copied().unwrap_or(u32::MAX));
            if let Some(q) = step.filter(|q| dist.get(q) < dist.get(&p)) {
                turn.orders.push(Order::March { from: p, to: q, count: s, on_behalf: None });
            }
        }
    }
    turn
}

/// A liege's call to arms: a letter this season that names a province and
/// says what to do there ("march on Tavton", "help me take Osmoor"). A
/// vassal that can reach it sends what it can spare to fight there for its
/// liege, and says so; one grown bigger than its liege does not answer.
fn answer_calls(v: &View, liege: Option<NobleId>, mine: &[ProvinceId], spare: &mut BTreeMap<ProvinceId, u32>, turn: &mut AiTurn) {
    let Some(l) = liege else { return };
    if v.noble(v.me).power > v.noble(l).power {
        return;
    }
    let words = |text: &str| -> Vec<String> { text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect() };
    for letter in v.letters.iter().filter(|x| x.from == l && x.season == v.season && x.to.contains(&v.me)) {
        let said = words(&letter.body);
        const CALLS: [&str; 9] = ["march", "attack", "take", "strike", "seize", "send", "help", "join", "fight"];
        if !said.iter().any(|w| CALLS.contains(&w.as_str())) {
            continue;
        }
        let named = v.provinces.iter().find(|p| {
            said.contains(&p.name.to_lowercase()) && p.owner.is_none_or(|o| o != l && o != v.me && v.noble(o).liege != Some(l))
        });
        let Some(target) = named else { continue };
        let mut sent = 0;
        for &q in target.neighbours.iter().filter(|q| mine.contains(q)) {
            let c = spare.get(&q).copied().unwrap_or(0);
            if c > 0 {
                spare.insert(q, 0);
                sent += c;
                turn.orders.push(Order::March { from: q, to: target.id, count: c, on_behalf: Some(l) });
            }
        }
        let body = if sent > 0 {
            format!("My lord, {sent} of my men march on {} for you this season.", target.name)
        } else {
            format!("My lord, I have no men within reach of {}.", target.name)
        };
        if !v.noble(l).ai {
            turn.replies.push(Draft { to: vec![l], body, offer: None });
        }
    }
}

/// Whether the oath I swore is all that stands between me and the crown of
/// my people. Then the oath goes, whatever the liege thinks of it.
fn crown_but_for_oath(v: &View) -> bool {
    let me = v.noble(v.me);
    let r = &v.rules;
    let total = v.provinces.len() as u32;
    let seat = v.provinces.iter().any(|p| p.kind == crate::map::Kind::Seat(me.faction) && p.owner == Some(v.me));
    let kin_sworn = v.nobles.iter().filter(|n| n.alive && n.id != v.me && n.faction == me.faction).all(|n| v.is_above(v.me, n.id));
    seat && kin_sworn && me.power * 100 >= r.king_share_pct * total && me.prestige - r.renounce_penalty >= r.king_prestige
}

/// Hops from each of my provinces to my nearest border, through my own land.
fn border_distance(v: &View, mine: &[ProvinceId], border: &[ProvinceId]) -> BTreeMap<ProvinceId, u32> {
    let mut d: BTreeMap<ProvinceId, u32> = border.iter().map(|&b| (b, 0)).collect();
    let mut q: VecDeque<ProvinceId> = border.iter().copied().collect();
    while let Some(p) = q.pop_front() {
        for &n in &v.province(p).neighbours {
            if mine.contains(&n) && !d.contains_key(&n) {
                d.insert(n, d[&p] + 1);
                q.push_back(n);
            }
        }
    }
    d
}
