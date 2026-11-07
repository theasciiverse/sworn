//! A real game over iroh on this machine: a host and two players who find it
//! by LAN beacon. Slow-ish (a few seconds) and needs UDP on localhost.

use fealty::court::{Court, Notice};
use fealty::lan;
use fealty::netcourt::{HostCourt, HostLobby, JoinLobby, RemoteCourt};
use fealty::{Faction, Order};
use std::time::{Duration, Instant};

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fealty-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("hosted.fealty")
}

fn until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("timed out waiting for {what}");
}

fn find_host(listener: &lan::Listener, name: &str) -> fealty::lan::Found {
    find(listener, |f| f.beacon.name == name)
}

/// By id: a host restarted under the same name is a different endpoint.
fn find_id(listener: &lan::Listener, id: iroh::EndpointId) -> fealty::lan::Found {
    find(listener, |f| f.beacon.id == id)
}

fn find(listener: &lan::Listener, want: impl Fn(&fealty::lan::Found) -> bool) -> fealty::lan::Found {
    let mut found = None;
    until("the LAN beacon", 15, || {
        found = listener.hosts().into_iter().find(&want);
        found.is_some()
    });
    found.unwrap()
}

fn join(addr: iroh::EndpointAddr, name: &str, host: &mut HostLobby) -> JoinLobby {
    let mut j = JoinLobby::new(addr, name);
    until(&format!("{name} to join"), 20, || {
        host.poll();
        j.poll();
        assert!(j.failed.is_none(), "{name}: {:?}", j.failed);
        j.seats.iter().any(|s| s.name == name)
    });
    j
}

fn seat(mut j: JoinLobby, host: &mut HostCourt) -> RemoteCourt {
    until("seating", 20, || {
        host.poll();
        j.poll()
    });
    j.into_court().unwrap()
}

#[test]
fn a_season_over_the_network() {
    let listener = lan::Listener::start(lan::PORT).expect("LAN listener");
    let path = scratch("season");
    let mut lobby = HostLobby::new("Hosta", Faction::Saxons, 4, path.clone());
    until("the host endpoint", 20, || {
        lobby.poll();
        assert!(lobby.failed.is_none(), "{:?}", lobby.failed);
        lobby.id.is_some()
    });
    let found = find_host(&listener, "Hosta");
    let mut ann = join(found.addr(), "Ann", &mut lobby);
    ann.pick(Faction::Danes);
    let bo = join(found.addr(), "Bo", &mut lobby);
    until("Ann's pick", 10, || {
        lobby.poll();
        ann.poll();
        lobby.seats().iter().any(|s| s.name == "Ann" && s.faction == Faction::Danes)
    });
    assert_eq!(lobby.players(), 3);

    let mut host = lobby.start(42).unwrap();
    let mut ann = seat(ann, &mut host);
    let mut bo = seat(bo, &mut host);
    assert_eq!(host.game().nobles.len(), 4);
    assert!(host.game().nobles[3].ai, "the empty seat went to the computer");
    let (a, b) = (ann.view().me, bo.view().me);
    assert_eq!(host.game().nobles[a as usize].faction, Faction::Danes);
    // Fog of war crosses the network: Ann sees her own troops, not Bo's.
    let bo_home = host.game().provinces_of(b).next().unwrap();
    assert_eq!(ann.view().province(bo_home).troops, None);

    // A letter from Ann to Bo reaches Bo and nobody else.
    ann.send_letter(&[b], "Shall we carve up the Saxon?", None).unwrap();
    until("the letter", 10, || {
        host.poll();
        ann.poll();
        bo.poll();
        bo.view().letters.iter().any(|l| l.from == a)
    });
    assert!(host.view().letters.is_empty(), "the host cannot read it");

    // Orders: the season waits for every person, then resolves for all.
    let home = ann.view().mine().next().unwrap().id;
    ann.submit(vec![Order::Raise { at: home, count: 2 }]).unwrap();
    host.submit(vec![]).unwrap();
    until("Ann's orders at the host", 10, || {
        host.poll();
        ann.poll();
        host.game().orders[a as usize].is_some()
    });
    assert_eq!(host.game().season, 0, "still waiting for Bo");
    assert_eq!(host.view().waiting, vec![b]);
    bo.submit(vec![]).unwrap();
    let mut resolved = (false, false);
    until("the season to turn", 10, || {
        host.poll();
        resolved.0 |= ann.poll().iter().any(|n| matches!(n, Notice::Resolved { season: 0 }));
        resolved.1 |= bo.poll().iter().any(|n| matches!(n, Notice::Resolved { season: 0 }));
        resolved.0 && resolved.1
    });
    assert_eq!(ann.view().season, 1);
    assert!(ann.view().events.iter().any(|e| e.describe(ann.view()).contains("raised 2")));
    assert!(!bo.view().events.iter().any(|e| e.describe(bo.view()).contains("raised 2")), "Bo did not see it");
    assert!(path.exists(), "the host saved");

    // Bo drops: a steward takes the seat, and the season need not wait.
    drop(bo);
    until("Bo's seat to pass to a steward", 15, || {
        host.poll();
        ann.poll();
        host.game().nobles[b as usize].ai
    });
    ann.submit(vec![]).unwrap();
    host.submit(vec![]).unwrap();
    until("a season without Bo", 10, || {
        host.poll();
        ann.poll();
        host.game().season == 2
    });

    // Bo comes back under the same name and has his seat again.
    let mut bo = JoinLobby::new(find_host(&listener, "Hosta").addr(), "bo");
    until("Bo's return", 20, || {
        host.poll();
        assert!(bo.failed.is_none(), "{:?}", bo.failed);
        bo.poll()
    });
    let bo = bo.into_court().unwrap();
    assert_eq!(bo.view().me, b);
    assert!(!host.game().nobles[b as usize].ai);

    // A stranger cannot take a seat once the game has begun.
    let mut stranger = JoinLobby::new(find_host(&listener, "Hosta").addr(), "Zed");
    until("the stranger's rejection", 20, || {
        host.poll();
        stranger.poll();
        stranger.failed.is_some()
    });
    assert!(stranger.failed.unwrap().contains("no seat"));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_host_drops_and_resumes_from_the_save() {
    let listener = lan::Listener::start(lan::PORT).expect("LAN listener");
    let path = scratch("resume");
    let mut lobby = HostLobby::new("Hostb", Faction::Bretons, 3, path.clone());
    until("the host endpoint", 20, || {
        lobby.poll();
        lobby.id.is_some()
    });
    let ann = join(find_host(&listener, "Hostb").addr(), "Ann", &mut lobby);
    let mut host = lobby.start(7).unwrap();
    let mut ann = seat(ann, &mut host);
    let a = ann.view().me;
    ann.submit(vec![]).unwrap();
    host.submit(vec![]).unwrap();
    until("a season", 10, || {
        host.poll();
        ann.poll();
        ann.view().season == 1
    });
    drop(host);
    let mut lost = false;
    until("Ann to notice", 15, || {
        lost |= ann.poll().iter().any(|n| matches!(n, Notice::Lost(_)));
        lost
    });

    let saved = fealty::save::load(&path).unwrap();
    assert_eq!(saved.season, 1);
    let mut lobby = HostLobby::resume(saved, path.clone());
    until("the host endpoint", 20, || {
        lobby.poll();
        lobby.id.is_some()
    });
    assert!(lobby.seats().iter().any(|s| s.name == "Ann" && s.empty));
    let here = find_id(&listener, lobby.id.unwrap());
    let mut wrong = JoinLobby::new(here.addr(), "Zed");
    until("a stranger turned away", 20, || {
        lobby.poll();
        wrong.poll();
        wrong.failed.is_some()
    });
    let ann = join(here.addr(), "Ann", &mut lobby);
    assert!(lobby.seats().iter().any(|s| s.name == "Ann" && !s.empty));
    let mut host = lobby.start(0).unwrap();
    let ann = seat(ann, &mut host);
    assert_eq!(ann.view().me, a);
    assert_eq!(ann.view().season, 1);
    assert!(host.game().nobles.iter().filter(|n| n.id != 0 && n.id != a).all(|n| n.ai));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn hosting_and_joining_through_the_screens() {
    use fealty::tui::{Screen, Ui};
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    let dir = std::env::temp_dir().join(format!("fealty-test-{}-screens", std::process::id()));
    std::env::set_var("XDG_DATA_HOME", &dir);
    let press = |ui: &mut Ui, c: KeyCode| ui.event(Event::Key(KeyEvent::new(c, KeyModifiers::NONE)));
    let text = |t: &Terminal<TestBackend>| {
        let b = t.backend().buffer();
        (0..b.area.height).map(|y| (0..b.area.width).map(|x| b[(x, y)].symbol().to_string()).collect::<String>() + "\n").collect::<String>()
    };
    let mut t = Terminal::new(TestBackend::new(100, 34)).unwrap();

    let mut host = Ui::hosting();
    if let Screen::Setup(s) = &mut host.screen {
        s.name = "Hostc".into();
    }
    press(&mut host, KeyCode::Down);
    press(&mut host, KeyCode::Down);
    press(&mut host, KeyCode::Left); // 4 seats
    press(&mut host, KeyCode::Down);
    press(&mut host, KeyCode::Enter);
    assert!(matches!(host.screen, Screen::Lobby(_)));
    until("the host to open", 20, || {
        host.tick();
        t.draw(|f| host.draw(f)).unwrap();
        text(&t).contains("fealty --join")
    });

    let mut guest = Ui::joining(None);
    if let Screen::Browse(b) = &mut guest.screen {
        b.name = "Guest".into();
    }
    until("the guest to see the table", 15, || {
        host.tick();
        guest.tick();
        t.draw(|f| guest.draw(f)).unwrap();
        text(&t).contains("Hostc's table")
    });
    // Other tests may be hosting at the same time: pick Hostc's table.
    for _ in 0..8 {
        t.draw(|f| guest.draw(f)).unwrap();
        if text(&t).contains("▸ Hostc's table") {
            break;
        }
        press(&mut guest, KeyCode::Down);
    }
    press(&mut guest, KeyCode::Enter);
    until("the guest to sit down", 20, || {
        host.tick();
        guest.tick();
        t.draw(|f| host.draw(f)).unwrap();
        text(&t).contains("Guest")
    });
    press(&mut host, KeyCode::Char('t'));
    press(&mut host, KeyCode::Char('t'));
    t.draw(|f| host.draw(f)).unwrap();
    assert!(text(&t).contains("2 min a season"), "{}", text(&t));
    until("the guest to see the timer", 10, || {
        host.tick();
        guest.tick();
        t.draw(|f| guest.draw(f)).unwrap();
        text(&t).contains("2 min a season")
    });
    press(&mut host, KeyCode::Enter);
    assert!(matches!(host.screen, Screen::Play(_)));
    t.draw(|f| host.draw(f)).unwrap();
    assert!(text(&t).contains(" left"), "the countdown shows: {}", text(&t));
    until("the guest's game screen", 20, || {
        host.tick();
        guest.tick();
        matches!(guest.screen, Screen::Play(_))
    });
    t.draw(|f| guest.draw(f)).unwrap();
    let s = text(&t);
    assert!(s.contains("The Realm") && s.contains("Guest"), "{s}");
    // The guest ends the season; the host still owes orders.
    press(&mut guest, KeyCode::Char('e'));
    press(&mut guest, KeyCode::Char('y'));
    until("the host to see the guest is ready", 10, || {
        host.tick();
        guest.tick();
        matches!(&host.screen, Screen::Play(p) if p.view().waiting == vec![0])
    });
    press(&mut host, KeyCode::Char('e'));
    press(&mut host, KeyCode::Char('y'));
    until("the season report on the guest's screen", 10, || {
        host.tick();
        guest.tick();
        t.draw(|f| guest.draw(f)).unwrap();
        text(&t).contains("Spring 800 is over")
    });
    assert!(dir.join("fealty/hosted.fealty").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_season_timer_runs_out_on_a_slow_player() {
    let listener = lan::Listener::start(lan::PORT).expect("LAN listener");
    let path = scratch("timer");
    let mut lobby = HostLobby::new("Hostt", Faction::Saxons, 2, path.clone());
    until("the host endpoint", 20, || {
        lobby.poll();
        lobby.id.is_some()
    });
    let mut ann = join(find_host(&listener, "Hostt").addr(), "Ann", &mut lobby);
    lobby.set_timer(Some(Duration::from_secs(2)));
    until("the timer to reach Ann", 10, || {
        lobby.poll();
        ann.poll();
        ann.timer == 2
    });

    let mut host = lobby.start(7).unwrap();
    let mut ann = seat(ann, &mut host);
    let a = ann.view().me;
    assert!(ann.deadline().is_some(), "Ann sees the clock");
    assert!(host.deadline().is_some());
    host.submit(vec![]).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    host.poll();
    assert_eq!(host.game().season, 0, "time is not up yet");

    // Ann never sends orders; the clock turns the season without her.
    let (mut resolved, mut told) = (false, false);
    until("the timer to turn the season", 10, || {
        host.poll();
        for n in ann.poll() {
            resolved |= matches!(n, Notice::Resolved { season: 0 });
            told |= matches!(&n, Notice::Info(s) if s.contains("Time ran out") && s.contains("Ann"));
        }
        resolved && told
    });
    assert_eq!(host.game().season, 1);
    assert!(!host.game().nobles[a as usize].ai, "Ann keeps her seat");
    let left = host.deadline().unwrap().saturating_duration_since(Instant::now());
    assert!(left > Duration::from_millis(1000), "a fresh clock for the new season");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
