use std::{env, fs, path::PathBuf};

include!(concat!(env!("OUT_DIR"), "/logos.rs"));

pub const DEFAULT_DISTRO: &str = env!("SCREENFETCH_DEFAULT_DISTRO");
const BUILD_SHARE_DIR: &str = env!("SCREENFETCH_BUILD_SHARE_DIR");

/// `<share>/<id>.pixmap/<id>_logo.svg` under every share dir we know of.
pub fn candidate_paths(id: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(d) = env::var("SCREENFETCH_SHARE_DIR") {
        dirs.push(d);
    }
    dirs.push(BUILD_SHARE_DIR.into());
    dirs.push("/usr/local/share".into());
    dirs.push("/usr/share".into());
    dirs.into_iter()
        .map(|d| PathBuf::from(d).join(format!("{id}.pixmap")).join(format!("{id}_logo.svg")))
        .collect()
}

fn embedded(id: &str) -> Option<Vec<u8>> {
    EMBEDDED.iter().find(|(k, _)| *k == id).map(|(_, b)| b.to_vec())
}

/// Resolve SVG bytes. `ids` is tried in order (e.g. ID, then ID_LIKE entries);
/// on-disk files win over embedded ones so a distro can override at runtime.
pub fn resolve(explicit: Option<&str>, ids: &[String]) -> Result<Vec<u8>, String> {
    if let Some(p) = explicit {
        return fs::read(p).map_err(|e| format!("{p}: {e}"));
    }
    for id in ids {
        for p in candidate_paths(id) {
            if let Ok(b) = fs::read(&p) {
                return Ok(b);
            }
        }
        if let Some(b) = embedded(id) {
            return Ok(b);
        }
    }
    embedded(DEFAULT_DISTRO).ok_or_else(|| format!("no logo for default distro {DEFAULT_DISTRO:?}"))
}
