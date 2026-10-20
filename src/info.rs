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

/// (label, value) rows, plus the `user@host` title.
pub fn collect(os: &OsRelease) -> (String, Vec<(&'static str, &'static str, String)>) {
    let title = format!("{}@{}", env::var("USER").unwrap_or_else(|_| "user".into()), file("/proc/sys/kernel/hostname"));
    let up = file("/proc/uptime").split('.').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let uptime = match (up / 86400, up % 86400 / 3600, up % 3600 / 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, m) => format!("{d}d {h}h {m}m"),
    };
    let cpu = file("/proc/cpuinfo")
        .lines()
        .find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default();
    let (total, avail) = (meminfo_kb("MemTotal"), meminfo_kb("MemAvailable"));
    let mib = |kb: u64| kb / 1024;
    let shell = env::var("SHELL").unwrap_or_default();
    let rows = vec![
        ("os", "OS", os.pretty.clone()),
        ("kernel", "Kernel", file("/proc/sys/kernel/osrelease")),
        ("uptime", "Uptime", uptime),
        ("shell", "Shell", shell.rsplit('/').next().unwrap_or("").to_string()),
        ("cpu", "CPU", cpu),
        ("memory", "Memory", format!("{} MiB / {} MiB", mib(total - avail), mib(total))),
    ];
    (title, rows)
}
