use base64::{Engine, engine::general_purpose::STANDARD as B64};
use std::{env, fs, path::PathBuf};

include!(concat!(env!("OUT_DIR"), "/logos.rs"));

pub const DEFAULT_DISTRO: &str = env!("SCREENFETCH_DEFAULT_DISTRO");
const BUILD_SHARE_DIR: &str = env!("SCREENFETCH_BUILD_SHARE_DIR");

/// `<share>/<id>.pixmap/<id>_logo.svg` (or `.png`) under every share dir we know of.
pub fn candidate_paths(id: &str) -> Vec<PathBuf> {
    candidate_paths_ext(id, "svg")
}

fn candidate_paths_ext(id: &str, ext: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(d) = env::var("SCREENFETCH_SHARE_DIR") {
        dirs.push(d);
    }
    dirs.push(BUILD_SHARE_DIR.into());
    dirs.extend(crate::info::SHARE_DIRS.iter().map(|d| d.to_string()).filter(|d| d != BUILD_SHARE_DIR));
    dirs.into_iter()
        .map(|d| PathBuf::from(d).join(format!("{id}.pixmap")).join(format!("{id}_logo.{ext}")))
        .collect()
}

fn embedded(id: &str) -> Option<Vec<u8>> {
    EMBEDDED.iter().find(|(k, _)| *k == id).map(|(_, b)| b.to_vec())
}

/// Resolve SVG bytes. `ids` is tried in order (e.g. ID, then ID_LIKE entries);
/// on-disk files win over embedded ones so a distro can override at runtime.
pub fn resolve(explicit: Option<&str>, ids: &[String]) -> Result<Vec<u8>, String> {
    if let Some(p) = explicit {
        return fs::read(p).map(into_svg).map_err(|e| format!("{p}: {e}"));
    }
    for id in ids {
        for p in candidate_paths(id).into_iter().chain(candidate_paths_ext(id, "png")) {
            if let Ok(b) = fs::read(&p) {
                return Ok(into_svg(b));
            }
        }
        if let Some(b) = embedded(id) {
            return Ok(b);
        }
    }
    embedded(DEFAULT_DISTRO).ok_or_else(|| format!("no logo for default distro {DEFAULT_DISTRO:?}"))
}

/// A PNG logo is wrapped in a tiny SVG so it goes through the same renderer.
fn into_svg(bytes: Vec<u8>) -> Vec<u8> {
    const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(MAGIC) || bytes.len() < 24 {
        return bytes;
    }
    let dim = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    let (w, h) = (dim(16), dim(20));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><image width=\"{w}\" height=\"{h}\" xlink:href=\"data:image/png;base64,{}\"/></svg>",
        B64.encode(&bytes)
    )
    .into_bytes()
}
