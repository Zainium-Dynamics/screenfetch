use std::{env, fs};

pub struct OsRelease {
    pub id: String,
    pub id_like: Vec<String>,
    pub pretty: String,
}

pub fn os_release() -> OsRelease {
    let text = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    OsRelease {
        id: get("ID"),
        id_like: get("ID_LIKE").split_whitespace().map(String::from).collect(),
        pretty: get("PRETTY_NAME"),
    }
}

fn file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn meminfo_kb(key: &str) -> u64 {
    file("/proc/meminfo")
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

fn cmd(prog: &str, args: &[&str]) -> String {
    std::process::Command::new(prog)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn gsetting(key: &str) -> String {
    cmd("gsettings", &["get", "org.gnome.desktop.interface", key]).trim_matches('\'').to_string()
}

fn first_line(path: &str) -> String {
    file(path)
}

fn host() -> String {
    let name = first_line("/sys/class/dmi/id/product_name");
    let ver = first_line("/sys/class/dmi/id/product_version");
    let junk = |s: &str| s.is_empty() || s.contains("O.E.M") || s == "None";
    match (junk(&name), junk(&ver)) {
        (true, _) => String::new(),
        (false, true) => name,
        _ => format!("{name} {ver}"),
    }
}

fn terminal() -> String {
    if let Ok(t) = env::var("TERM_PROGRAM") {
        return t;
    }
    // walk up the parent chain until something that is not a shell
    let mut pid = std::process::id();
    for _ in 0..8 {
        let stat = file(&format!("/proc/{pid}/stat"));
        let Some(ppid) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)?.parse::<u32>().ok()) else {
            break;
        };
        let name = file(&format!("/proc/{ppid}/comm"));
        if !["bash", "zsh", "fish", "sh", "screenfetch", "fastfetch"].contains(&name.as_str()) && !name.is_empty() {
            return name.trim_end_matches("-server").trim_end_matches("-agent").to_string();
        }
        pid = ppid;
    }
    env::var("TERM").unwrap_or_default()
}

fn display() -> String {
    let Ok(dir) = fs::read_dir("/sys/class/drm") else { return String::new() };
    let mut out = Vec::new();
    for e in dir.flatten() {
        let p = e.path();
        if file(&format!("{}/status", p.display())) == "connected" {
            let mode = file(&format!("{}/modes", p.display())).lines().next().unwrap_or("").to_string();
            if !mode.is_empty() {
                out.push(mode);
            }
        }
    }
    out.join(", ")
}

fn de_wm() -> (String, String) {
    let de = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let wm = match de.to_lowercase().as_str() {
        d if d.contains("gnome") => "Mutter",
        d if d.contains("kde") => "KWin",
        d if d.contains("cosmic") => "cosmic-comp",
        d if d.contains("xfce") => "Xfwm4",
        d if d.contains("sway") => "sway",
        d if d.contains("hypr") => "Hyprland",
        _ => "",
    };
    let wm = match (wm.is_empty(), session.is_empty()) {
        (true, _) => String::new(),
        (false, true) => wm.to_string(),
        _ => format!("{wm} ({})", capitalize(&session)),
    };
    (de, wm)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn gpu() -> String {
    let card = (0..8).map(|n| format!("/sys/class/drm/card{n}/device")).find(|d| fs::metadata(format!("{d}/vendor")).is_ok());
    let Some(card) = card else { return String::new() };
    let dev = |f: &str| file(&format!("{card}/{f}")).trim_start_matches("0x").to_string();
    let (v, d) = (dev("vendor"), dev("device"));
    if v.is_empty() {
        return String::new();
    }
    for ids in ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"] {
        let Ok(t) = fs::read_to_string(ids) else { continue };
        let mut vendor = None;
        for l in t.lines() {
            if let Some(name) = l.strip_prefix(&v).and_then(|r| r.strip_prefix("  ")) {
                vendor = Some(name.to_string());
            } else if vendor.is_some() {
                if let Some(name) = l.strip_prefix('\t').and_then(|r| r.strip_prefix(&d)).and_then(|r| r.strip_prefix("  ")) {
                    let name = name.rsplit_once('[').map_or(name, |(_, n)| n.trim_end_matches(']'));
                    let vn = vendor.unwrap();
                    let vn = vn.split_whitespace().next().unwrap_or("");
                    return format!("{vn} {name}");
                }
                if !l.starts_with('\t') && !l.starts_with('#') {
                    vendor = None;
                }
            }
        }
    }
    format!("{v}:{d}")
}

fn gib(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / (1u64 << 30) as f64)
}

fn disk_usage(path: &str) -> String {
    let out = cmd("df", &["-B1", "--output=size,used,fstype", path]);
    let f: Vec<&str> = out.lines().nth(1).unwrap_or("").split_whitespace().collect();
    match f[..] {
        [size, used, fs] => {
            let (s, u): (u64, u64) = (size.parse().unwrap_or(0), used.parse().unwrap_or(0));
            let ext = if path.starts_with("/run/media/") || path.starts_with("/media/") { " [External]" } else { "" };
            format!("{} / {} ({}%) - {fs}{ext}", gib(u), gib(s), u * 100 / s.max(1))
        }
        _ => String::new(),
    }
}

/// `/` plus user-visible mounts (USB drives and friends).
fn disks() -> Vec<(String, String)> {
    let mut paths = vec!["/".to_string()];
    for l in file("/proc/mounts").lines() {
        let Some(mp) = l.split_whitespace().nth(1) else { continue };
        let mp = mp.replace("\\040", " ");
        if ["/run/media/", "/media/", "/mnt/"].iter().any(|p| mp.starts_with(p)) && !paths.contains(&mp) {
            paths.push(mp);
        }
    }
    paths.into_iter().map(|p| (format!("Disk ({p})"), disk_usage(&p))).collect()
}

fn terminal_font() -> String {
    let gs = |schema: &str, key: &str| cmd("gsettings", &["get", schema, key]).trim_matches('\'').to_string();
    match terminal().to_lowercase().as_str() {
        t if t.contains("ptyxis") => {
            let own = gs("org.gnome.Ptyxis", "font-name");
            let sys = gs("org.gnome.Ptyxis", "use-system-font") != "false";
            if sys || own.is_empty() { gs("org.gnome.desktop.interface", "monospace-font-name") } else { own }
        }
        "kitty" => {
            let home = env::var("HOME").unwrap_or_default();
            file(&format!("{home}/.config/kitty/kitty.conf"))
                .lines()
                .find_map(|l| l.strip_prefix("font_family").map(|v| v.trim().to_string()))
                .unwrap_or_default()
        }
        _ => gs("org.gnome.desktop.interface", "monospace-font-name"),
    }
}

fn local_ip() -> String {
    for l in cmd("ip", &["-4", "-o", "addr", "show", "scope", "global"]).lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if let (Some(ifc), Some(addr)) = (f.get(1), f.get(3)) {
            return format!("{ifc}: {addr}");
        }
    }
    String::new()
}

fn battery() -> String {
    let Ok(dir) = fs::read_dir("/sys/class/power_supply") else { return String::new() };
    for e in dir.flatten() {
        let p = e.path().display().to_string();
        if file(&format!("{p}/type")) == "Battery" {
            return format!("{}% [{}]", file(&format!("{p}/capacity")), file(&format!("{p}/status")));
        }
    }
    String::new()
}

fn cpu() -> String {
    let info = file("/proc/cpuinfo");
    let name = info
        .lines()
        .find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default();
    let name = name.replace("(R)", "").replace("(TM)", "").replace(" CPU", "");
    let cores = info.lines().filter(|l| l.starts_with("processor")).count();
    let mhz = file("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq").parse::<f64>().unwrap_or(0.0) / 1e6;
    let name = name.split(" @ ").next().unwrap_or("").trim().to_string();
    if mhz > 0.0 { format!("{name} ({cores}) @ {mhz:.2} GHz") } else { format!("{name} ({cores})") }
}

/// (key, label, value) rows plus the `user@host` title. Rows with no value are dropped.
pub fn collect(os: &OsRelease) -> (String, Vec<(&'static str, String, String)>) {
    let title = format!("{}@{}", env::var("USER").unwrap_or_else(|_| "user".into()), file("/proc/sys/kernel/hostname"));
    let up = file("/proc/uptime").split('.').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let uptime = match (up / 86400, up % 86400 / 3600, up % 3600 / 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, m) => format!("{d}d {h}h {m}m"),
    };
    let (total, avail) = (meminfo_kb("MemTotal"), meminfo_kb("MemAvailable"));
    let (stotal, sfree) = (meminfo_kb("SwapTotal"), meminfo_kb("SwapFree"));
    let pct = |used: u64, tot: u64| used * 100 / tot.max(1);
    let gb = |kb: u64| gib(kb * 1024);
    let shell = env::var("SHELL").unwrap_or_default();
    let (de, wm) = de_wm();
    let release = file("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("VERSION=").map(|v| v.trim_matches('"').to_string()))
        .unwrap_or_default();
    let rows: Vec<(&'static str, &str, String)> = vec![
        ("os", "OS", format!("{} {}", os.pretty, std::env::consts::ARCH)),
        ("release", "Release", release),
        ("host", "Host", host()),
        ("kernel", "Kernel", format!("Linux {}", file("/proc/sys/kernel/osrelease"))),
        ("uptime", "Uptime", uptime),
        ("shell", "Shell", shell.rsplit('/').next().unwrap_or("").to_string()),
        ("display", "Display", display()),
        ("de", "DE", de),
        ("wm", "WM", wm),
        ("theme", "Theme", gsetting("gtk-theme")),
        ("icons", "Icons", gsetting("icon-theme")),
        ("font", "Font", gsetting("font-name")),
        ("cursor", "Cursor", gsetting("cursor-theme")),
        ("terminal", "Terminal", terminal()),
        ("termfont", "Terminal Font", terminal_font()),
        ("cpu", "CPU", cpu()),
        ("gpu", "GPU", gpu()),
        ("memory", "Memory", format!("{} / {} ({}%)", gb(total - avail), gb(total), pct(total - avail, total))),
        ("swap", "Swap", if stotal > 0 { format!("{} / {} ({}%)", gb(stotal - sfree), gb(stotal), pct(stotal - sfree, stotal)) } else { String::new() }),
        ("ip", "Local IP", local_ip()),
        ("battery", "Battery", battery()),
        ("locale", "Locale", env::var("LANG").unwrap_or_default()),
    ];
    let mut rows: Vec<(&'static str, String, String)> = rows.into_iter().map(|(k, l, v)| (k, l.to_string(), v)).collect();
    let at = rows.iter().position(|r| r.0 == "ip").unwrap_or(rows.len());
    for (n, (label, v)) in disks().into_iter().enumerate() {
        rows.insert(at + n, ("disk", label, v));
    }
    (title, rows.into_iter().filter(|r| !r.2.trim().is_empty()).collect())
}
