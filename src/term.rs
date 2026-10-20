//! Ask the terminal what it can do: cell size (`CSI 16 t`) and Sixel support (DA1 attribute 4).
use std::{os::fd::RawFd, time::Duration};

#[derive(Default)]
pub struct Probe {
    pub sixel: bool,
    /// (width, height) of one cell in pixels
    pub cell: Option<(u32, u32)>,
}

pub fn probe() -> Probe {
    unsafe {
        if libc::isatty(1) == 0 {
            return Probe::default();
        }
        let fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        if fd < 0 {
            return Probe::default();
        }
        let mut old: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut old) != 0 {
            libc::close(fd);
            return Probe::default();
        }
        let mut raw = old;
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(fd, libc::TCSANOW, &raw);
        let reply = query(fd, b"\x1b[16t\x1b[c", Duration::from_millis(200));
        libc::tcsetattr(fd, libc::TCSANOW, &old);
        libc::close(fd);
        parse(&reply)
    }
}

unsafe fn query(fd: RawFd, req: &[u8], timeout: Duration) -> String {
    unsafe {
        libc::write(fd, req.as_ptr().cast(), req.len());
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        // DA1 is always answered last and ends with 'c'
        while !out.contains(&b'c') {
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            if libc::poll(&mut p, 1, timeout.as_millis() as i32) <= 0 {
                break;
            }
            let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
            if n <= 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

fn parse(reply: &str) -> Probe {
    let cell = reply.find("\x1b[6;").and_then(|i| {
        let body = reply[i + 4..].split('t').next()?;
        let (h, w) = body.split_once(';')?;
        Some((w.parse().ok()?, h.parse().ok()?)).filter(|&(w, h): &(u32, u32)| w > 0 && h > 0)
    });
    let sixel = reply
        .find("\x1b[?")
        .and_then(|i| reply[i + 3..].split('c').next())
        .is_some_and(|a| a.split(';').any(|x| x == "4"));
    Probe { sixel, cell }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_reply() {
        let p = super::parse("\x1b[6;20;9t\x1b[?62;4;22c");
        assert_eq!(p.cell, Some((9, 20)));
        assert!(p.sixel);
        assert!(!super::parse("\x1b[?62;22c").sixel);
    }
}
