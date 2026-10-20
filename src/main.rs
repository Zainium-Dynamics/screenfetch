mod config;
mod info;
mod logo;
mod octants;
mod render;
mod term;

use render::{Logo, Protocol};

const HELP: &str = "screenfetch - system info with an SVG logo drawn right in the terminal

USAGE: screenfetch [OPTIONS]
  --logo FILE      render this SVG or PNG
  --distro ID      look up <share>/ID.pixmap/ID_logo.svg (default: zainium; `auto` = from /etc/os-release)
  --protocol P     auto | kitty | iterm2 | sixel | blocks
  --glyphs G       auto | quadrant | octant: block glyph set for the fallback renderer
  --rows N         logo height in terminal rows (default 20)
  --probe          show what the terminal reported (sixel support, cell size)
  --list           print logo search paths and embedded logos
  -h, --help
Config: ~/.config/screenfetch/config.toml (see README)
Env: SCREENFETCH_SHARE_DIR overrides the share dir searched at runtime.
     SCREENFETCH_BG=rrggbb sets the background used to smooth logo edges (auto-detected normally).";

const KEYS: &[&str] = &[
    "os", "release", "host", "kernel", "uptime", "shell", "display", "de", "wm", "theme", "icons", "font", "cursor",
    "terminal", "termfont", "cpu", "gpu", "memory", "swap", "disk", "ip", "battery", "locale",
];

fn main() {
    let cfg = config::load().unwrap_or_else(|e| die(&e));
    let probe = term::probe();
    let mut args = std::env::args().skip(1);
    let mut proto = match cfg.protocol.as_deref() {
        None | Some("auto") => Protocol::detect(probe.sixel),
        Some(p) => Protocol::parse(p).unwrap_or_else(|| die("unknown protocol in config")),
    };
    let mut glyphs: Option<String> = cfg.glyphs.clone();
    let (mut file, mut distro, mut rows, mut list) = (cfg.logo.clone(), cfg.distro.clone(), cfg.rows.unwrap_or(20), false);
    while let Some(a) = args.next() {
        let mut val = |n: &str| args.next().unwrap_or_else(|| die(&format!("{n} needs a value")));
        match a.as_str() {
            "--logo" => file = Some(val("--logo")),
            "--distro" => distro = Some(val("--distro")),
            "--protocol" => {
                let p = val("--protocol");
                if p != "auto" {
                    proto = Protocol::parse(&p).unwrap_or_else(|| die("unknown protocol"));
                }
            }
            "--rows" => rows = val("--rows").parse().unwrap_or_else(|_| die("--rows needs a number")),
            "--glyphs" => glyphs = Some(val("--glyphs")),
            "--list" => list = true,
            "--probe" => {
                println!("sixel: {}\ncell px: {:?}\nprotocol: {proto:?}", probe.sixel, probe.cell);
                return;
            }
            "-h" | "--help" => return println!("{HELP}"),
            other => die(&format!("unknown option {other}")),
        }
    }

    let os = info::os_release();
    // the built-in distro (zainium) unless asked otherwise; `auto` follows /etc/os-release
    let mut ids: Vec<String> = Vec::new();
    match distro.as_deref() {
        None => ids.push(logo::DEFAULT_DISTRO.into()),
        Some("auto") => {
            ids.push(os.id.clone());
            ids.extend(os.id_like.iter().cloned());
        }
        Some(d) => ids.push(d.into()),
    }
    if list {
        for id in &ids {
            for p in logo::candidate_paths(id) {
                println!("{}", p.display());
            }
        }
        for (id, _) in logo::EMBEDDED {
            println!("embedded: {id}");
        }
        return;
    }

    let svg = logo::resolve(file.as_deref(), &ids).unwrap_or_else(|e| die(&e));
    // keep the logo square on screen whatever the cell shape is
    let cell = probe.cell.unwrap_or((9, 20));
    // terminal background for edge blending; SCREENFETCH_BG=rrggbb overrides the probe
    let bg = std::env::var("SCREENFETCH_BG")
        .ok()
        .and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .or(probe.bg);
    // 2x4 octant cells look best, but only if the terminal font has the glyphs
    let octants = match glyphs.as_deref() {
        Some("octant") => true,
        Some("quadrant") => false,
        None | Some("auto") => render::font_has_octants(&info::terminal_font()),
        Some(_) => die("--glyphs: auto | quadrant | octant"),
    };
    let cols = (rows * cell.1 + cell.0 / 2) / cell.0;
    let art = render::logo(&svg, proto, cols, rows, cell, bg, octants).unwrap_or_else(|e| die(&e));

    let (title, rows_info) = info::collect(&os);
    let col = cfg.color.as_deref().map_or(36, |c| config::ansi_color(c).unwrap_or_else(|| die("unknown colour in config")));
    let mut text = vec![format!("\x1b[1;{col}m{title}\x1b[0m"), "-".repeat(title.len())];
    match &cfg.show {
        Some(keys) => {
            for k in keys {
                if !KEYS.contains(&k.as_str()) {
                    die(&format!("unknown info key {k:?} in config"));
                }
                for (_, l, v) in rows_info.iter().filter(|r| r.0 == k) {
                    text.push(format!("\x1b[1;{col}m{l}\x1b[0m: {v}"));
                }
            }
        }
        None => text.extend(rows_info.iter().map(|(_, l, v)| format!("\x1b[1;{col}m{l}\x1b[0m: {v}"))),
    }

    // never let a line wrap: it would shear the logo (long mount paths, narrow windows)
    if let Some((w, _)) = probe.size {
        let avail = (w as usize).saturating_sub(cols as usize + 2);
        text.iter_mut().for_each(|l| *l = clip(l, avail));
    }
    let height = (rows as usize).max(text.len());
    let mut out = String::new();
    match art {
        Logo::Lines(lines) => {
            for i in 0..height {
                out += lines.get(i).map(String::as_str).unwrap_or(&" ".repeat(cols as usize));
                out += &format!("  {}\n", text.get(i).map(String::as_str).unwrap_or(""));
            }
        }
        Logo::Graphic(seq) => {
            // reserve space, jump back up, draw the image without moving the cursor
            out += &"\n".repeat(height);
            out += &format!("\x1b[{height}A{seq}");
            for i in 0..height {
                out += &format!("\x1b[{}C{}\n", cols + 2, text.get(i).map(String::as_str).unwrap_or(""));
            }
        }
    }
    print!("{out}");
}

fn die(msg: &str) -> ! {
    eprintln!("screenfetch: {msg}");
    std::process::exit(1);
}

/// Cut `s` to `max` visible characters (ANSI colour codes don't count), ending with `…`.
fn clip(s: &str, max: usize) -> String {
    let visible = |s: &str| {
        let (mut n, mut esc) = (0, false);
        for c in s.chars() {
            match (esc, c) {
                (_, '\x1b') => esc = true,
                (true, 'm') => esc = false,
                (false, _) => n += 1,
                _ => {}
            }
        }
        n
    };
    if visible(s) <= max {
        return s.to_string();
    }
    let (mut out, mut n, mut esc) = (String::new(), 0, false);
    for c in s.chars() {
        match (esc, c) {
            (_, '\x1b') => esc = true,
            (true, 'm') => esc = false,
            (false, _) => {
                if n + 1 >= max {
                    break;
                }
                n += 1;
            }
            _ => {}
        }
        out.push(c);
    }
    out + "…\x1b[0m"
}
