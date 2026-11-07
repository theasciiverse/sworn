use fealty::court::LocalCourt;
use fealty::rng::Rng;
use fealty::tui::play::Popup;
use fealty::tui::{self, Screen, Ui};
use fealty::Faction;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;

fn ui(seed: u64, nobles: usize) -> Ui {
    let g = tui::new_local_game("David", Faction::ALL[seed as usize % 4], nobles, seed).unwrap();
    Ui::with_court(Box::new(LocalCourt::new(g, 0, None)))
}

fn press(ui: &mut Ui, code: KeyCode) {
    ui.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn screen_text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    let a = buf.area;
    (0..a.height).map(|y| (0..a.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>() + "\n").collect()
}

const KEYS: &[KeyCode] = &[
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Enter,
    KeyCode::Tab,
    KeyCode::BackTab,
    KeyCode::Esc,
    KeyCode::Backspace,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Char('r'),
    KeyCode::Char('m'),
    KeyCode::Char('w'),
    KeyCode::Char('a'),
    KeyCode::Char('b'),
    KeyCode::Char('f'),
    KeyCode::Char('x'),
    KeyCode::Char('j'),
    KeyCode::Char('k'),
    KeyCode::Char('n'),
    KeyCode::Char('e'),
    KeyCode::Char('y'),
    KeyCode::Char(' '),
    KeyCode::Char('1'),
    KeyCode::Char('2'),
    KeyCode::Char('3'),
    KeyCode::Char('4'),
    KeyCode::Char('5'),
    KeyCode::Char('?'),
    KeyCode::Char('R'),
    KeyCode::Char('Z'),
    KeyCode::Char('ö'),
];

/// Random keys and clicks for a whole game, drawn at several sizes: nothing
/// may panic, and the game must move on.
#[test]
fn random_play_never_panics() {
    for seed in 0..6u64 {
        let mut rng = Rng::new(seed);
        let mut ui = ui(seed, 2 + seed as usize);
        let sizes = [(120u16, 40u16), (80, 24), (200, 60), (30, 10), (12, 5)];
        let mut terms: Vec<Terminal<TestBackend>> = sizes.iter().map(|&(w, h)| Terminal::new(TestBackend::new(w, h)).unwrap()).collect();
        for step in 0..3000 {
            if let Screen::Play(p) = &ui.screen {
                // Never leave or quit, so the whole budget goes into play.
                if matches!(p.popup, Some(Popup::ConfirmQuit)) {
                    press(&mut ui, KeyCode::Char('n'));
                }
            } else {
                break;
            }
            if rng.chance(3) {
                let (w, h) = sizes[0];
                let m = MouseEvent {
                    kind: *rng.pick(&[MouseEventKind::Down(MouseButton::Left), MouseEventKind::ScrollDown, MouseEventKind::ScrollUp]),
                    column: rng.below(w as u32) as u16,
                    row: rng.below(h as u32) as u16,
                    modifiers: KeyModifiers::NONE,
                };
                ui.event(Event::Mouse(m));
            } else {
                let k = *rng.pick(KEYS);
                if k == KeyCode::Char('q') || k == KeyCode::Esc && matches!(&ui.screen, Screen::Play(p) if p.popup.is_none()) {
                    continue;
                }
                press(&mut ui, k);
            }
            let t = &mut terms[step % sizes.len()];
            t.draw(|f| ui.draw(f)).unwrap();
        }
        if let Screen::Play(p) = &ui.screen {
            assert!(p.view().season > 0, "seed {seed}: random play never ended a season");
        }
    }
}

#[test]
fn a_season_through_the_keyboard() {
    let mut ui = ui(3, 6);
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    for k in [KeyCode::Char('r'), KeyCode::End, KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter] {
        t.draw(|f| ui.draw(f)).unwrap();
        press(&mut ui, k);
    }
    let Screen::Play(p) = &ui.screen else { panic!("not playing") };
    assert_eq!(p.draft.len(), 2, "a raise and a march");
    press(&mut ui, KeyCode::Char('e'));
    press(&mut ui, KeyCode::Char('y'));
    t.draw(|f| ui.draw(f)).unwrap();
    let text = screen_text(&t);
    assert!(text.contains("Spring 800 is over"), "{text}");
    assert!(text.contains("raised"), "{text}");
    let Screen::Play(p) = &ui.screen else { panic!("not playing") };
    assert_eq!(p.view().season, 1);
    assert!(p.draft.is_empty());
}

#[test]
fn the_end_screen_draws_charts() {
    let mut g = tui::new_local_game("David", Faction::Danes, 4, 5).unwrap();
    g.nobles[0].ai = true;
    while g.winner.is_none() {
        g.play_ai();
        g.resolve();
    }
    g.nobles[0].ai = false;
    let mut ui = Ui::with_court(Box::new(LocalCourt::new(g, 0, None)));
    let mut t = Terminal::new(TestBackend::new(100, 36)).unwrap();
    for _ in 0..4 {
        t.draw(|f| ui.draw(f)).unwrap();
        let text = screen_text(&t);
        assert!(text.contains("rules the") || text.contains("Long live the King") || text.contains("The realm"), "{text}");
        assert!(text.contains('⣀') || text.contains('⠁') || text.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)), "no chart:\n{text}");
        press(&mut ui, KeyCode::Tab);
    }
}
