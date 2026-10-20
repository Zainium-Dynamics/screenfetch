use std::{env, fs};

pub struct OsRelease {
    pub id: String,
    pub id_like: Vec<String>,
    pub pretty: String,
    pub version: String,
}

/// System data dirs: Zainium's base and userland layers first, then the FHS ones.
pub const SHARE_DIRS: &[&str] = &["/overlayer/syshub/share", "/overlayer/zexlib/union/share", "/usr/local/share", "/usr/share"];

/// `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn config_home() -> std::path::PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_default()
}

pub fn os_release() -> OsRelease {
    let text = ["/etc/os-release", "/overlayer/syshub/etc/os-release", "/usr/lib/os-release", "/overlayer/syshub/lib/os-release"]
        .iter()
        .find_map(|p| fs::read_to_string(p).ok())
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
        version: get("VERSION"),
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
    for ids in SHARE_DIRS.iter().flat_map(|d| [format!("{d}/hwdata/pci.ids"), format!("{d}/misc/pci.ids")]) {
        let Ok(t) = fs::read_to_string(&ids) else { continue };
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
    // no pci.ids (Zainium's base has none): vendor name and the kernel driver
    let vendor = match v.as_str() {
        "8086" => "Intel",
        "1002" => "AMD",
        "10de" => "NVIDIA",
        "1af4" => "Virtio",
        "15ad" => "VMware",
        "80ee" => "VirtualBox",
        "1234" => "QEMU",
        _ => "",
    };
    let driver = fs::read_link(format!("{card}/driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    match (vendor, driver.is_empty()) {
        ("", true) => format!("{v}:{d}"),
        ("", false) => format!("{v}:{d} ({driver})"),
        (vn, true) => format!("{vn} {d}"),
        (vn, false) => format!("{vn} ({driver})"),
    }
}

fn gib(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / (1u64 << 30) as f64)
}

fn disk_usage(path: &str, fstype: &str) -> String {
    let Ok(c) = std::ffi::CString::new(path) else { return String::new() };
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return String::new();
    }
    let size = st.f_blocks as u64 * st.f_frsize as u64;
    let used = size - st.f_bfree as u64 * st.f_frsize as u64;
    let ext = if path.starts_with("/run/media/") || path.starts_with("/media/") { " [External]" } else { "" };
    format!("{} / {} ({}%) - {fstype}{ext}", gib(used), gib(size), used * 100 / size.max(1))
}

/// `/` plus user-visible mounts (USB drives and friends).
fn disks() -> Vec<(String, String)> {
    const PSEUDO: &[&str] = &["rootfs", "tmpfs", "ramfs", "overlay", "erofs", "squashfs"];
    let table: Vec<(String, String, String)> = file("/proc/mounts")
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let [src, mp, fs, ..] = f[..] else { return None };
            Some((src.to_string(), mp.replace("\\040", " "), fs.to_string()))
        })
        .collect();
    // (label, path to measure, fs type)
    let mut mounts: Vec<(String, String, String)> = Vec::new();
    for (_, mp, fs) in &table {
        let wanted = mp == "/" || ["/run/media/", "/media/", "/mnt/"].iter().any(|p| mp.starts_with(p));
        if wanted && !mounts.iter().any(|(m, _, _)| m == mp) {
            mounts.push((mp.clone(), mp.clone(), fs.clone()));
        }
    }
    // Zainium keeps the initramfs' rootfs as `/` and runs from the disk mounted under /overlayer
    // and /home: measure that disk for `/`
    if let Some(root) = mounts.iter_mut().find(|(m, _, fs)| m == "/" && PSEUDO.contains(&fs.as_str())) {
        let disk = ["/overlayer/zexlib", "/home", "/overlayer"]
            .iter()
            .find_map(|want| table.iter().find(|(src, mp, _)| mp == want && src.starts_with("/dev/")));
        if let Some((_, mp, fs)) = disk {
            root.1 = mp.clone();
            root.2 = fs.clone();
        }
    }
    mounts
        .into_iter()
        .map(|(label, path, fs)| (format!("Disk ({label})"), disk_usage(&path, &fs)))
        .filter(|(_, v)| !v.starts_with("0.00 GiB / 0.00 GiB"))
        .collect()
}

pub fn terminal_font() -> String {
    let gs = |schema: &str, key: &str| cmd("gsettings", &["get", schema, key]).trim_matches('\'').to_string();
    match terminal().to_lowercase().as_str() {
        t if t.contains("ptyxis") => {
            let own = gs("org.gnome.Ptyxis", "font-name");
            let sys = gs("org.gnome.Ptyxis", "use-system-font") != "false";
            if sys || own.is_empty() { gs("org.gnome.desktop.interface", "monospace-font-name") } else { own }
        }
        "kitty" => {
            file(&config_home().join("kitty/kitty.conf").to_string_lossy())
                .lines()
                .find_map(|l| l.strip_prefix("font_family").map(|v| v.trim().to_string()))
                .unwrap_or_default()
        }
        "cosmic-term" => {
            // the user's choice, else the system default COSMIC ships
            let key = "cosmic/com.system76.CosmicTerm/v1/font_name";
            let cfg = Some(file(&config_home().join(key).to_string_lossy()))
                .filter(|s| !s.is_empty())
                .or_else(|| SHARE_DIRS.iter().map(|d| file(&format!("{d}/{key}"))).find(|s| !s.is_empty()))
                .unwrap_or_default();
            let name = cfg.trim_start_matches("Some(").trim_end_matches(')').trim_matches('"').to_string();
            if name.is_empty() { "monospace".to_string() } else { name }
        }
        _ => {
            let f = gs("org.gnome.desktop.interface", "monospace-font-name");
            if f.is_empty() { "monospace".to_string() } else { f }
        }
    }
}

fn local_ip() -> String {
    unsafe {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut list) != 0 {
            return String::new();
        }
        let mut found = String::new();
        let mut cur = list;
        while !cur.is_null() && found.is_empty() {
            let a = &*cur;
            let up = a.ifa_flags & libc::IFF_UP as u32 != 0 && a.ifa_flags & libc::IFF_LOOPBACK as u32 == 0;
            if up && !a.ifa_addr.is_null() && (*a.ifa_addr).sa_family as i32 == libc::AF_INET {
                let ip = u32::from_be((*(a.ifa_addr as *const libc::sockaddr_in)).sin_addr.s_addr);
                let mask = u32::from_be((*(a.ifa_netmask as *const libc::sockaddr_in)).sin_addr.s_addr);
                let name = std::ffi::CStr::from_ptr(a.ifa_name).to_string_lossy();
                found = format!("{name}: {}/{}", std::net::Ipv4Addr::from(ip), mask.count_ones());
            }
            cur = a.ifa_next;
        }
        libc::freeifaddrs(list);
        found
    }
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
    let release = os.version.clone();
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
