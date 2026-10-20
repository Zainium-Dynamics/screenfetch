use serde::Deserialize;
use std::{env, fs, path::PathBuf};

/// `~/.config/screenfetch/config.toml`; every key is optional, CLI flags win.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub logo: Option<String>,
    pub distro: Option<String>,
    pub protocol: Option<String>,
    pub rows: Option<u32>,
    /// Info lines to show, in order (keys: os, kernel, uptime, shell, cpu, memory).
    pub show: Option<Vec<String>>,
    /// Label colour: black, red, green, yellow, blue, magenta, cyan, white.
    pub color: Option<String>,
}

pub fn path() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default();
    base.join("screenfetch/config.toml")
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
