mod config;
mod info;
mod logo;
mod render;
mod term;

use render::{Logo, Protocol};

const HELP: &str = "screenfetch - system info with an SVG logo drawn right in the terminal

USAGE: screenfetch [OPTIONS]
  --logo FILE      render this SVG
  --distro ID      look up <share>/ID.pixmap/ID_logo.svg (default: from /etc/os-release)
  --protocol P     auto | kitty | iterm2 | sixel | blocks
  --rows N         logo height in terminal rows (default 20)
  --list           print logo search paths and embedded logos
  -h, --help
Config: ~/.config/screenfetch/config.toml (see README)
Env: SCREENFETCH_SHARE_DIR overrides the share dir searched at runtime.";

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
            "--list" => list = true,
            "-h" | "--help" => return println!("{HELP}"),
            other => die(&format!("unknown option {other}")),
        }
    }

    let os = info::os_release();
    let mut ids: Vec<String> = distro.into_iter().collect();
    if ids.is_empty() {
        ids.push(os.id.clone());
        ids.extend(os.id_like.iter().cloned());
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
    let cell = probe.cell.unwrap_or((10, 20));
    let cols = (rows * cell.1 + cell.0 / 2) / cell.0;
    let art = render::logo(&svg, proto, cols, rows, cell).unwrap_or_else(|e| die(&e));

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
