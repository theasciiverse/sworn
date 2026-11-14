//! Renders the game screen offscreen and prints it, for checking layout
//! without a terminal: cargo run --example screens -- [WIDTH HEIGHT] [keys] [nobles] [seed]
use sworn::court::LocalCourt;
use sworn::tui::{self, Ui};
use sworn::Faction;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let w = args.first().and_then(|a| a.parse().ok()).unwrap_or(120);
    let h = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(40);
    let keys = args.get(2).cloned().unwrap_or_default();
    let nobles = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(6);
    let seed = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(3);
    let g = tui::new_local_game("David", Faction::Saxons, nobles, seed).unwrap();
    let mut ui = Ui::with_court(Box::new(LocalCourt::new(g, 0, None)));
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    // keys: one char each; '<' '>' '^' 'v' arrows, '\n' as '|', 'T' tab, 'E' esc
    for c in keys.chars() {
        let code = match c {
            '<' => KeyCode::Left,
            '>' => KeyCode::Right,
            '^' => KeyCode::Up,
            'v' => KeyCode::Down,
            '|' => KeyCode::Enter,
            'T' => KeyCode::Tab,
            'E' => KeyCode::Esc,
            c => KeyCode::Char(c),
        };
        t.draw(|f| ui.draw(f)).unwrap();
        ui.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }
    t.draw(|f| ui.draw(f)).unwrap();
    let buf = t.backend().buffer();
    for y in 0..h {
        let line: String = (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect();
        println!("{}", line.trim_end());
    }
}
