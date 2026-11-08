//! Save files: the whole game, so whoever holds the latest save can host it.

use crate::game::Game;
use std::path::Path;

const MAGIC: &[u8; 8] = b"SWORN\0\0\0";
/// Saves from before the game was renamed from Fealty.
const OLD_MAGIC: &[u8; 8] = b"FEALTY\0\0";
/// Bumped on any incompatible change to `Game`.
pub const FORMAT: u16 = 1;

pub fn to_bytes(g: &Game) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend(postcard::to_allocvec(g).expect("postcard encoding of an owned game cannot fail"));
    out
}

pub fn from_bytes(b: &[u8]) -> Result<Game, String> {
    if b.len() < 10 || (&b[..8] != MAGIC && &b[..8] != OLD_MAGIC) {
        return Err("not a Sworn save".into());
    }
    let format = u16::from_le_bytes([b[8], b[9]]);
    if format != FORMAT {
        return Err(format!("save format {format}, this build reads {FORMAT}"));
    }
    postcard::from_bytes(&b[10..]).map_err(|e| format!("damaged save: {e}"))
}

pub fn save(g: &Game, path: &Path) -> std::io::Result<()> {
    // Write beside and rename, so a crash never leaves half a save.
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, to_bytes(g))?;
    std::fs::rename(tmp, path)
}

pub fn load(path: &Path) -> Result<Game, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    from_bytes(&b)
}
