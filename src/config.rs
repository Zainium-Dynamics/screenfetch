use serde::Deserialize;
use std::{fs, path::PathBuf};

/// `~/.config/config.toml`; every key is optional, CLI flags win.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub logo: Option<String>,
    pub distro: Option<String>,
    pub protocol: Option<String>,
    pub rows: Option<u32>,
    /// Fallback renderer glyphs: auto, quadrant (2x2) or octant (2x4, needs a font with Unicode 16 octants).
    pub glyphs: Option<String>,
    /// Info lines to show, in order (keys: os, kernel, uptime, shell, cpu, memory).
    pub show: Option<Vec<String>>,
    /// Label colour: black, red, green, yellow, blue, magenta, cyan, white.
    pub color: Option<String>,
}

pub fn path() -> PathBuf {
    crate::info::config_home().join("screenfetch/.config/config.toml")
}

pub fn load() -> Result<Config, String> {
    let p = path();
    match fs::read_to_string(&p) {
        Ok(s) => toml::from_str(&s).map_err(|e| format!("{}: {e}", p.display())),
        Err(_) => Ok(Config::default()),
    }
}

pub fn ansi_color(name: &str) -> Option<u8> {
    ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
        .iter()
        .position(|c| *c == name)
        .map(|i| 30 + i as u8)
}
