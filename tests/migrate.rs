//! Saves from when the game was called Fealty. Its own test binary, because
//! it sets XDG_DATA_HOME for the whole process.

use sworn::court::{autosave_path, data_dir, hosted_path, migrate_old_saves};
use sworn::{save, Faction, Game, NewNoble, Rules, Setup};

fn game() -> Game {
    let nobles = ["Ann", "Bo"].iter().map(|n| NewNoble { name: n.to_string(), faction: Faction::Danes, ai: true }).collect();
    Game::new(Setup { seed: 9, nobles, rules: Rules::default() }).unwrap()
}

#[test]
fn fealty_saves_move_over_and_still_load() {
    let home = std::env::temp_dir().join(format!("sworn-test-{}-migrate", std::process::id()));
    let old = home.join("fealty");
    std::fs::create_dir_all(&old).unwrap();
    // A save as Fealty wrote it: the same format behind the old magic.
    let mut bytes = save::to_bytes(&game());
    bytes[..8].copy_from_slice(b"FEALTY\0\0");
    std::fs::write(old.join("hosted.fealty"), &bytes).unwrap();
    std::fs::write(old.join("autosave.fealty"), &bytes).unwrap();
    std::env::set_var("XDG_DATA_HOME", &home);

    migrate_old_saves();
    assert_eq!(data_dir(), home.join("sworn"));
    assert!(!old.exists());
    let g = save::load(&hosted_path()).expect("an old save loads");
    assert_eq!(g.nobles[1].name, "Bo");
    assert!(save::load(&autosave_path()).is_ok());

    // Once moved, never again: a new fealty/ folder is left alone.
    std::fs::create_dir_all(&old).unwrap();
    migrate_old_saves();
    assert!(old.exists());
    assert!(save::from_bytes(b"NOTASAVE\0\0rest").is_err());
    let _ = std::fs::remove_dir_all(&home);
}
