use fealty::chronicle::EventKind;
use fealty::rng::Rng;
use fealty::{save, Faction, Game, Kind, Map, NewNoble, Order, Outcome, Rules, Setup, Term, Title, Visibility};

// Ann(0) and Bo(1) are Saxons, Cy(2) Moravian: no combat bonuses, so the
// arithmetic below is plain troop counts.
//
//   A(0) - B(1) - C(2) - D(3) - G(5, Saxon seat)
//          |
//          F(4, fortress)
fn game() -> Game {
    let map = Map::from_parts(
        &[
            ("A", Kind::Hold, 0, 0),
            ("B", Kind::Hold, 100, 0),
            ("C", Kind::Hold, 200, 0),
            ("D", Kind::Hold, 300, 0),
            ("F", Kind::Fortress, 100, 100),
            ("G", Kind::Seat(Faction::Saxons), 400, 0),
        ],
        &[(0, 1), (1, 2), (2, 3), (1, 4), (3, 5)],
    );
    let nobles = [("Ann", Faction::Saxons), ("Bo", Faction::Saxons), ("Cy", Faction::Moravians)]
        .into_iter()
        .map(|(n, f)| NewNoble { name: n.into(), faction: f, ai: false })
        .collect();
    let mut g = Game::with_map(Rules::default(), map, nobles, Rng::new(1)).unwrap();
    own(&mut g, 0, 0, 10);
    own(&mut g, 1, 1, 4);
    own(&mut g, 2, 2, 5);
    own(&mut g, 3, 2, 1);
    own(&mut g, 4, 1, 5);
    g
}

fn own(g: &mut Game, p: u16, n: u8, troops: u32) {
    g.map.provinces[p as usize].owner = Some(n);
    g.map.provinces[p as usize].troops = troops;
}

fn march(from: u16, to: u16, count: u32) -> Order {
    Order::March { from, to, count, on_behalf: None }
}

fn outcome_at(g: &Game, first: usize, at: u16) -> Outcome {
    g.chronicle[first..]
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Battle { at: a, outcome, .. } if *a == at => Some(*outcome),
            _ => None,
        })
        .expect("a battle")
}

#[test]
fn the_stronger_side_takes_and_keeps_its_margin() {
    let mut g = game();
    g.submit(0, vec![march(0, 1, 10)]).unwrap();
    let first = g.resolve();
    assert_eq!(outcome_at(&g, first, 1), Outcome::Taken(0));
    let b = &g.map.provinces[1];
    assert_eq!(b.owner, Some(0));
    assert_eq!(b.troops, 6, "10 against 4 leaves 6");
    assert_eq!(g.map.provinces[0].troops, 0);
    assert_eq!(g.nobles[0].prestige, g.rules.capture_prestige);
}

#[test]
fn a_tie_is_a_standoff_that_bleeds_both() {
    let mut g = game();
    g.submit(0, vec![march(0, 1, 4)]).unwrap();
    let first = g.resolve();
    assert_eq!(outcome_at(&g, first, 1), Outcome::Standoff);
    assert_eq!(g.map.provinces[1].owner, Some(1));
    assert_eq!(g.map.provinces[1].troops, 2);
    assert_eq!(g.map.provinces[0].troops, 6 + 2, "half the attackers come home");
}

#[test]
fn fortress_walls_double_the_garrison() {
    let mut g = game();
    own(&mut g, 1, 0, 9);
    g.submit(0, vec![march(1, 4, 9)]).unwrap();
    let first = g.resolve();
    // 5 behind fortress walls count as 10 against 9.
    assert_eq!(outcome_at(&g, first, 4), Outcome::Held);
    assert_eq!(g.map.provinces[4].troops, 1);
    assert_eq!(g.map.provinces[1].troops, 4);
}

#[test]
fn support_fights_for_the_ally_then_goes_home() {
    let mut g = game();
    g.submit(0, vec![march(0, 1, 2)]).unwrap();
    g.submit(2, vec![Order::March { from: 2, to: 1, count: 3, on_behalf: Some(0) }]).unwrap();
    let first = g.resolve();
    assert_eq!(outcome_at(&g, first, 1), Outcome::Taken(0), "2 + 3 beat 4");
    assert_eq!(g.map.provinces[1].owner, Some(0));
    assert_eq!(g.map.provinces[1].troops, 1, "ceil(2 * 1/5)");
    assert_eq!(g.map.provinces[2].troops, 2 + 1, "Cy's survivors are back in C");
}

#[test]
fn support_promised_and_withheld_loses_the_battle() {
    let mut g = game();
    // Cy promised in a letter, then marched for himself elsewhere.
    g.send_letter(2, &[0], "Attack B in spring; my 3 will march with you.", None).unwrap();
    g.submit(0, vec![march(0, 1, 2)]).unwrap();
    g.submit(2, vec![]).unwrap();
    let first = g.resolve();
    assert_eq!(outcome_at(&g, first, 1), Outcome::Held);
}

#[test]
fn marching_on_your_liege_breaks_the_oath_publicly() {
    let mut g = game();
    g.nobles[1].liege = Some(0);
    g.submit(1, vec![march(1, 0, 4)]).unwrap();
    let first = g.resolve();
    assert_eq!(g.nobles[1].liege, None);
    assert!(g.nobles[1].prestige <= -g.rules.betrayal_penalty + 1);
    assert!(g.chronicle[first..]
        .iter()
        .any(|e| e.to == Visibility::Public && matches!(e.kind, EventKind::OathBroken { breaker: 1, victim: 0, .. })));
}

#[test]
fn defending_your_liege_is_not_betrayal() {
    let mut g = game();
    g.nobles[1].liege = Some(0);
    g.submit(1, vec![Order::March { from: 1, to: 0, count: 2, on_behalf: Some(0) }]).unwrap();
    g.resolve();
    assert_eq!(g.nobles[1].liege, Some(0));
    assert_eq!(g.map.provinces[1].troops, 4, "nobody attacked, the guard came home");
}

#[test]
fn deals_execute_and_the_same_gold_cannot_be_promised_twice() {
    let mut g = game();
    let start = g.nobles[0].gold;
    g.send_letter(0, &[1], "Five gold for B.", Some((vec![Term::Gold(start - 2)], vec![Term::Province(1)]))).unwrap();
    g.send_letter(0, &[2], "And for you, the same coin.", Some((vec![Term::Gold(start - 2)], vec![Term::Province(3)])))
        .unwrap();
    let (first_offer, second_offer) = (g.offers[0].id, g.offers[1].id);
    g.submit(1, vec![Order::Accept(first_offer)]).unwrap();
    g.submit(2, vec![Order::Accept(second_offer)]).unwrap();
    let first = g.resolve();
    assert_eq!(g.map.provinces[1].owner, Some(0), "Bo sold B");
    assert_eq!(g.map.provinces[3].owner, Some(2), "Cy's deal fell through");
    assert!(g.chronicle[first..].iter().any(|e| matches!(e.kind, EventKind::DealFailed { .. })));
    assert!(g.offers.is_empty());
}

#[test]
fn fealty_by_deal_and_no_swearing_to_your_own_vassal() {
    let mut g = game();
    g.send_letter(1, &[0], "I kneel.", Some((vec![Term::Fealty], vec![]))).unwrap();
    let id = g.offers[0].id;
    g.submit(0, vec![Order::Accept(id)]).unwrap();
    g.resolve();
    assert_eq!(g.nobles[1].liege, Some(0));
    assert!(g.send_letter(0, &[1], "Now I kneel to you.", Some((vec![Term::Fealty], vec![]))).is_err());
}

#[test]
fn offers_expire_after_the_next_season() {
    let mut g = game();
    g.send_letter(1, &[0], "I kneel.", Some((vec![Term::Fealty], vec![]))).unwrap();
    g.resolve();
    assert_eq!(g.offers.len(), 1, "still open the season after");
    g.resolve();
    assert!(g.offers.is_empty());
}

#[test]
fn snow_closes_the_roads_in_winter() {
    let mut g = game();
    g.season = 3;
    g.submit(0, vec![march(0, 1, 10)]).unwrap();
    let first = g.resolve();
    assert_eq!(g.map.provinces[1].owner, Some(1));
    assert!(g.chronicle[first..].iter().any(|e| matches!(e.kind, EventKind::Snowbound { noble: 0 })));
}

#[test]
fn losing_every_province_ends_a_noble_and_frees_their_vassals() {
    let mut g = game();
    own(&mut g, 4, 0, 0); // F to Ann, so Bo holds only B
    g.nobles[2].liege = Some(1);
    g.submit(0, vec![march(0, 1, 10)]).unwrap();
    g.resolve();
    assert!(!g.nobles[1].alive);
    assert_eq!(g.nobles[2].liege, None);
    assert!(g.submit(1, vec![]).is_err());
}

#[test]
fn raising_costs_gold_and_respects_the_cap() {
    let mut g = game();
    g.submit(0, vec![Order::Raise { at: 0, count: 50 }]).unwrap();
    g.resolve();
    // Saxons pay 1 a troop; a hold raises at most raise_cap a season.
    assert_eq!(g.map.provinces[0].troops, 10 + g.rules.raise_cap);
}

#[test]
fn troops_are_hidden_beyond_your_borders() {
    let g = game();
    let v = g.view(0);
    assert_eq!(v.province(1).troops, Some(4), "neighbour");
    assert_eq!(v.province(2).troops, None, "two roads away");
    assert_eq!(v.province(2).owner, Some(2), "but who holds it is public");
}

#[test]
fn private_letters_stay_private() {
    let mut g = game();
    g.send_letter(0, &[1], "Just between us.", None).unwrap();
    assert_eq!(g.view(1).letters.len(), 1);
    assert!(g.view(2).letters.is_empty());
}

#[test]
fn the_crown_needs_the_seat_the_share_and_the_people() {
    let mut g = game();
    for p in 0..6 {
        own(&mut g, p, 0, 3);
    }
    g.nobles[0].prestige = g.rules.king_prestige;
    g.nobles[2].alive = false;
    // Bo is a Saxon too and still free: no crown yet.
    own(&mut g, 4, 1, 3);
    g.resolve();
    assert_eq!(g.winner, None);
    g.nobles[1].liege = Some(0);
    g.resolve();
    assert_eq!(g.winner, Some(0));
    assert_eq!(g.nobles[0].title, Title::King);
}

#[test]
fn saves_round_trip() {
    let nobles = (0..4).map(|i| NewNoble { name: format!("N{i}"), faction: Faction::ALL[i], ai: true }).collect();
    let mut g = Game::new(Setup { seed: 9, nobles, rules: Rules::default() }).unwrap();
    for _ in 0..12 {
        g.play_ai();
        g.resolve();
    }
    let back = save::from_bytes(&save::to_bytes(&g)).unwrap();
    assert_eq!(back, g);
    assert!(save::from_bytes(b"not a save").is_err());
}

/// Whole computer games across many seeds and table sizes: they must end,
/// and the realm must stay consistent every season.
#[test]
fn computer_games_finish_and_stay_consistent() {
    let mut crowned = 0;
    let mut games = 0;
    for seed in 0..25 {
        for count in 2..=8 {
            let nobles =
                (0..count).map(|i| NewNoble { name: format!("N{i}"), faction: Faction::ALL[i % 4], ai: true }).collect();
            let mut g = Game::new(Setup { seed, nobles, rules: Rules::default() }).unwrap();
            let replay = g.clone();
            while g.winner.is_none() {
                g.play_ai();
                g.resolve();
                assert!(g.season <= g.rules.max_seasons);
                for n in &g.nobles {
                    if n.alive {
                        assert!(g.provinces_of(n.id).next().is_some(), "alive without land");
                        if let Some(l) = n.liege {
                            assert!(g.nobles[l as usize].alive);
                            assert!(!g.is_above(n.id, l), "a cycle of oaths");
                        }
                    } else {
                        assert!(g.provinces_of(n.id).next().is_none());
                        assert_eq!(n.liege, None);
                    }
                }
            }
            games += 1;
            if g.chronicle.iter().any(|e| matches!(e.kind, EventKind::GameOver { crowned: true, .. })) {
                crowned += 1;
            }
            // Same seed, same orders, same game: what lets anyone rehost.
            let mut again = replay;
            while again.winner.is_none() {
                again.play_ai();
                again.resolve();
            }
            assert_eq!(again, g, "seed {seed} with {count} nobles is not deterministic");
        }
    }
    eprintln!("{crowned} of {games} computer games ended in a coronation");
    assert!(crowned * 2 > games, "the crown should usually be reachable");
}

#[test]
fn a_computer_noble_will_not_kneel_to_an_equal_for_pocket_money() {
    let mut g = game();
    g.nobles[1].ai = true;
    g.send_letter(0, &[1], "Kneel.", Some((vec![Term::Gold(5)], vec![Term::Fealty]))).unwrap();
    g.send_letter(0, &[1], "Or sell me B.", Some((vec![Term::Gold(1)], vec![Term::Province(1)]))).unwrap();
    g.ai_orders();
    g.submit(0, vec![]).unwrap();
    g.submit(2, vec![]).unwrap();
    g.resolve();
    assert_eq!(g.nobles[1].liege, None);
    assert_eq!(g.map.provinces[1].owner, Some(1));
    assert!(g.letters.iter().any(|l| l.from == 1 && l.body.starts_with("No")), "it answers");
}

#[test]
fn a_computer_noble_takes_a_good_price() {
    let mut g = game();
    g.nobles[1].ai = true;
    g.nobles[0].gold = 40;
    g.send_letter(0, &[1], "A fair price for B.", Some((vec![Term::Gold(30)], vec![Term::Province(1)]))).unwrap();
    g.ai_orders();
    g.submit(0, vec![]).unwrap();
    g.submit(2, vec![]).unwrap();
    g.resolve();
    assert_eq!(g.map.provinces[1].owner, Some(0));
}

fn bo_attacks_ann(gift: bool) -> bool {
    let mut g = game();
    g.nobles[1].ai = true;
    own(&mut g, 0, 0, 1); // Ann's A is nearly empty
    own(&mut g, 1, 1, 20); // Bo's B is strong
    if gift {
        g.send_letter(0, &[1], "For your friendship.", Some((vec![Term::Gold(5)], vec![]))).unwrap();
        g.ai_orders();
        g.submit(0, vec![]).unwrap();
        g.submit(2, vec![]).unwrap();
        g.resolve();
        assert!(g.chronicle.iter().any(|e| matches!(e.kind, EventKind::Deal { .. })), "Bo took the gold");
        g.map.provinces[0].troops = 1;
    }
    let turn = fealty::ai::plan(&g.view(1));
    turn.orders.iter().any(|o| matches!(o, Order::March { to: 0, .. }))
}

#[test]
fn gold_buys_a_computer_nobles_peace() {
    assert!(bo_attacks_ann(false), "without a gift, the weak hold is taken");
    assert!(!bo_attacks_ann(true), "after a gift, Bo leaves Ann alone");
}

fn clash(g: &Game, first: usize) -> Option<Option<u8>> {
    g.chronicle[first..].iter().find_map(|e| match &e.kind {
        EventKind::Clash { winner, .. } => Some(*winner),
        _ => None,
    })
}

#[test]
fn head_on_marches_meet_on_the_road() {
    let mut g = game();
    g.submit(0, vec![march(0, 1, 10)]).unwrap();
    g.submit(1, vec![march(1, 0, 4)]).unwrap();
    let first = g.resolve();
    assert_eq!(clash(&g, first), Some(Some(0)), "10 beat 4 in the field");
    assert_eq!(g.map.provinces[0].owner, Some(0), "Bo's column never reached A");
    assert_eq!(g.map.provinces[1].owner, Some(0));
    assert_eq!(g.map.provinces[1].troops, 6, "ceil(10 * 6/10) marched on into an empty B");
    let scattered = g.chronicle[first..].iter().any(|e| matches!(e.kind, EventKind::Scattered { noble: 1, home: 1, count: 2 }));
    assert!(scattered, "Bo's beaten 2 fell back to a B already lost");
}

#[test]
fn an_even_clash_on_the_road_sends_both_home() {
    let mut g = game();
    g.submit(0, vec![march(0, 1, 4)]).unwrap();
    g.submit(1, vec![march(1, 0, 4)]).unwrap();
    let first = g.resolve();
    assert_eq!(clash(&g, first), Some(None));
    assert_eq!((g.map.provinces[0].owner, g.map.provinces[0].troops), (Some(0), 6 + 2));
    assert_eq!((g.map.provinces[1].owner, g.map.provinces[1].troops), (Some(1), 2));
}

#[test]
fn allies_marching_to_guard_each_other_pass_on_the_road() {
    let mut g = game();
    g.submit(0, vec![Order::March { from: 0, to: 1, count: 3, on_behalf: Some(1) }]).unwrap();
    g.submit(1, vec![Order::March { from: 1, to: 0, count: 2, on_behalf: Some(0) }]).unwrap();
    let first = g.resolve();
    assert_eq!(clash(&g, first), None);
    assert_eq!(g.map.provinces[0].troops, 10);
    assert_eq!(g.map.provinces[1].troops, 4);
}

#[test]
fn your_own_columns_swap_places_freely() {
    let mut g = game();
    own(&mut g, 1, 0, 4);
    g.submit(0, vec![march(0, 1, 10), march(1, 0, 4)]).unwrap();
    let first = g.resolve();
    assert_eq!(clash(&g, first), None);
    assert_eq!(g.map.provinces[0].troops, 4);
    assert_eq!(g.map.provinces[1].troops, 10);
}
