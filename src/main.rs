//! Fealty: the terminal game, or with --simulate a headless game between
//! computer nobles that prints the chronicle.

use fealty::court::LocalCourt;
use fealty::tui::{self, Ui};
use fealty::{save, season_name, Faction, Game, NewNoble, Rules, Setup, Visibility};
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use std::io;
use std::path::PathBuf;

const NAMES: &[&str] = &["Aldric", "Brynja", "Cedric", "Dagny", "Eadric", "Frida", "Godwin", "Hild"];

fn main() -> io::Result<()> {
    // Session logging: RUST_LOG=debug fealty 2>fealty.log
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "fealty [--load FILE | --host | --join [ID]]\n\
             fealty --simulate [--seed N] [--nobles N] [--all]\n\n\
             Multiplayer medieval politics for the terminal (working title).\n\
             Rise from lesser noble to king of your people. In game, ? shows the keys.\n\
             --load FILE  continue a saved game (saves go to {})\n\
             --host       host a game for others on the LAN or the internet\n\
             --join       look for games on the local network\n\
             --join ID    join a host by its id (works across the internet)\n\
             --simulate   computer nobles play a whole game; prints the chronicle\n\
             --all        with --simulate, print private events too",
            fealty::court::data_dir().display()
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--simulate") {
        simulate(&args);
        return Ok(());
    }
    let mut ui = match args.iter().position(|a| a == "--load").and_then(|i| args.get(i + 1)) {
        Some(path) => {
            let path = PathBuf::from(path);
            let g = match save::load(&path) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            };
            let me = g.nobles.iter().find(|n| !n.ai).map_or(0, |n| n.id);
            Ui::with_court(Box::new(LocalCourt::new(g, me, Some(path))))
        }
        None if args.iter().any(|a| a == "--host") => Ui::hosting(),
        None if args.iter().any(|a| a == "--join") => {
            let id = args.iter().position(|a| a == "--join").and_then(|i| args.get(i + 1)).filter(|v| !v.starts_with("--"));
            match id.map(|v| v.parse::<iroh::EndpointId>()) {
                Some(Ok(id)) => Ui::joining(Some(iroh::EndpointAddr::new(id))),
                Some(Err(e)) => {
                    eprintln!("not a host id: {e}");
                    std::process::exit(2);
                }
                None => Ui::joining(None),
            }
        }
        None => Ui::new(),
    };
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    log::info!("start");
    let result = tui::run(&mut terminal, &mut ui);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn simulate(args: &[String]) {
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<u64>().ok());
    let seed = value("--seed").unwrap_or(1);
    let count = value("--nobles").unwrap_or(6).clamp(2, 8) as usize;
    let all = args.iter().any(|a| a == "--all");
    let nobles = (0..count)
        .map(|i| NewNoble { name: NAMES[i].into(), faction: Faction::ALL[i % 4], ai: true })
        .collect();
    let mut g = match Game::new(Setup { seed, nobles, rules: Rules::default() }) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    println!("{} provinces, {} nobles, seed {seed}\n", g.map.provinces.len(), count);
    while g.winner.is_none() {
        g.play_ai();
        let season = g.season;
        let first = g.resolve();
        let lines: Vec<String> =
            g.chronicle[first..].iter().filter(|e| all || e.to == Visibility::Public).map(|e| e.describe(&g)).collect();
        if !lines.is_empty() {
            println!("{}", season_name(season));
            for l in lines {
                println!("  {l}");
            }
        }
    }
    println!("\nStandings after {} seasons:", g.season);
    for (rank, id) in g.standings().into_iter().enumerate() {
        let n = &g.nobles[id as usize];
        let s = g.history.last().map(|h| h[id as usize].clone()).unwrap();
        println!(
            "{:>2}. {:<8} {:<9} {:<5} power {:>2}  prestige {:>3}  gold {:>3}  troops {:>3}{}",
            rank + 1,
            n.name,
            n.faction.name(),
            n.title.name(),
            s.power,
            n.prestige,
            n.gold,
            s.troops,
            if n.alive { "" } else { "  (fallen)" }
        );
    }
}
