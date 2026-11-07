//! Plays many computer games and reports who wins:
//! cargo run --release --example balance -- [GAMES]
use fealty::{Faction, Game, NewNoble, Rules, Setup};
use std::collections::BTreeMap;

fn main() {
    let games: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(400);
    let mut wins: BTreeMap<&str, u32> = BTreeMap::new();
    let mut seats: BTreeMap<&str, u32> = BTreeMap::new();
    let (mut crowned, mut seasons, mut oaths, mut broken, mut deals) = (0, 0, 0, 0, 0);
    for seed in 0..games {
        let count = 3 + (seed % 6) as usize;
        // Rotate who sits where so faction and start position are independent.
        let nobles: Vec<NewNoble> = (0..count)
            .map(|i| NewNoble { name: format!("N{i}"), faction: Faction::ALL[(i + seed as usize / 6) % 4], ai: true })
            .collect();
        for n in &nobles {
            *seats.entry(n.faction.name()).or_default() += 1;
        }
        let mut g = Game::new(Setup { seed, nobles, rules: Rules::default() }).unwrap();
        while g.winner.is_none() {
            g.play_ai();
            g.resolve();
        }
        let w = g.winner.unwrap();
        *wins.entry(g.nobles[w as usize].faction.name()).or_default() += 1;
        seasons += g.season;
        for e in &g.chronicle {
            match &e.kind {
                fealty::EventKind::GameOver { crowned: true, .. } => crowned += 1,
                fealty::EventKind::Sworn { .. } => oaths += 1,
                fealty::EventKind::OathBroken { .. } | fealty::EventKind::Renounced { .. } => broken += 1,
                fealty::EventKind::Deal { .. } => deals += 1,
                _ => {}
            }
        }
    }
    println!("{games} games, {crowned} crowned, {:.1} years on average", seasons as f64 / games as f64 / 4.0);
    println!("per game: {:.2} oaths sworn, {:.2} broken, {:.2} deals", oaths as f64 / games as f64, broken as f64 / games as f64, deals as f64 / games as f64);
    for (f, s) in &seats {
        let w = wins.get(f).copied().unwrap_or(0);
        println!("{f:<10} won {w:>4} of {games}  ({:.1}% of wins; {:.1}% of seats)", 100.0 * w as f64 / games as f64, 100.0 * *s as f64 / seats.values().sum::<u32>() as f64);
    }
}
