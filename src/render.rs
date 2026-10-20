use base64::{Engine, engine::general_purpose::STANDARD as B64};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Protocol {
    Kitty,
    Iterm2,
    Sixel,
    Blocks,
}

impl Protocol {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "kitty" => Some(Self::Kitty),
            "iterm2" => Some(Self::Iterm2),
            "sixel" => Some(Self::Sixel),
            "blocks" => Some(Self::Blocks),
            _ => None,
        }
    }

    pub fn detect(sixel_probed: bool) -> Self {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let (term, prog) = (var("TERM"), var("TERM_PROGRAM"));
        if var("KITTY_WINDOW_ID") != "" || term == "xterm-kitty" || prog == "ghostty" || term == "xterm-ghostty" {
            Self::Kitty
        } else if prog == "iTerm.app" || prog == "WezTerm" {
            Self::Iterm2
        } else if term.starts_with("foot") || term.starts_with("mlterm") || sixel_probed || var("SCREENFETCH_SIXEL") != "" {
            Self::Sixel
        } else {
            Self::Blocks
        }
    }
}

/// Draw the SVG centred in a `w` x `h` pixel grid whose pixels are `aspect` (width / height) shaped.
pub fn rasterize(svg: &[u8], w: u32, h: u32, aspect: f32) -> Result<tiny_skia::Pixmap, String> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size();
    // fit in physical units (x stretched by `aspect`), then convert back to pixels
    let phys = (w as f32 * aspect / size.width()).min(h as f32 / size.height());
    let (sx, sy) = (phys / aspect, phys);
    let (dx, dy) = ((w as f32 - size.width() * sx) / 2.0, (h as f32 - size.height() * sy) / 2.0);
    let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("bad size")?;
    resvg::render(&tree, tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, dx, dy), &mut pm.as_mut());
    Ok(pm)
}

/// Returns the escape sequence / text for the logo, as `rows` printable lines for
/// `Blocks`, or a single image sequence (cursor not moved) for the graphics protocols.
pub enum Logo {
    Lines(Vec<String>),
    Graphic(String),
}

pub fn logo(svg: &[u8], proto: Protocol, cols: u32, rows: u32, cell: (u32, u32), bg: Option<[u8; 3]>, octants: bool) -> Result<Logo, String> {
    match proto {
        Protocol::Blocks => {
            // each cell is gx x gy sub-pixels (2x2 quadrants or 2x4 octants), supersampled
            let (gx, gy) = if octants { (2, 4) } else { (2, 2) };
            let ss: u32 = if octants { 3 } else { 4 };
            let aspect = (cell.0 as f32 / gx as f32) / (cell.1 as f32 / gy as f32);
            let pm = rasterize(svg, cols * gx * ss, rows * gy * ss, aspect)?;
            let sub = |x: u32, y: u32| -> Option<[i64; 3]> {
                let (mut a, mut c) = (0i64, [0i64; 3]);
                for (dx, dy) in (0..ss).flat_map(|dy| (0..ss).map(move |dx| (dx, dy))) {
                    let p = pm.pixel(x * ss + dx, y * ss + dy).unwrap();
                    // squaring alpha fades the SVG's soft drop shadow (faint, dark) far more than
                    // the logo body, which would otherwise ring it with a black halo
                    let al = p.alpha() as i64;
                    a += al * al / 255;
                    let q = |v: u8| v as i64 * al / 255;
                    c = [c[0] + q(p.red()), c[1] + q(p.green()), c[2] + q(p.blue())];
                }
                let n = (ss * ss) as i64;
                match bg {
                    // known background: composite over it so edges anti-alias instead of snapping on/off
                    Some(b) => Some([0, 1, 2].map(|k| ((c[k] + b[k] as i64 * (255 * n - a) / 255) / n).min(255))),
                    None => (a > 128 * n).then(|| c.map(|v| (v * 255 / a).min(255))),
                }
            };
            let lines = (0..rows)
                .map(|r| {
                    (0..cols)
                        .map(|c| {
                            let p: Vec<_> = (0..gx * gy).map(|i| sub(c * gx + i % gx, r * gy + i / gx)).collect();
                            glyph_cell(&p, bg, octants)
                        })
                        .collect::<String>()
                })
                .collect();
            Ok(Logo::Lines(lines))
        }
        Protocol::Sixel => {
            // cursor is saved/restored around the image
            let pm = rasterize(svg, cols * cell.0, rows * cell.1, 1.0)?;
            let rgba: Vec<u8> = pm
                .pixels()
                .iter()
                .flat_map(|p| {
                    let c = p.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                })
                .collect();
            let sx = sixel_enc::encode(&rgba, pm.width() as usize, pm.height() as usize, 256);
            Ok(Logo::Graphic(format!("\x1b7{sx}\x1b8")))
        }
        Protocol::Kitty | Protocol::Iterm2 => {
            let side = 40 * rows; // plenty of pixels; the terminal scales to the cell box
            let png = B64.encode(rasterize(svg, side, side, 1.0)?.encode_png().map_err(|e| e.to_string())?);
            if proto == Protocol::Iterm2 {
                return Ok(Logo::Graphic(format!(
                    "\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=1;doNotMoveCursor=1:{png}\x07"
                )));
            }
            let mut out = String::new();
            let chunks: Vec<&str> = png.as_bytes().chunks(4096).map(|c| std::str::from_utf8(c).unwrap()).collect();
            for (i, c) in chunks.iter().enumerate() {
                let more = (i + 1 < chunks.len()) as u8;
                if i == 0 {
                    out += &format!("\x1b_Ga=T,f=100,c={cols},r={rows},C=1,q=2,m={more};{c}\x1b\\");
                } else {
                    out += &format!("\x1b_Gm={more};{c}\x1b\\");
                }
            }
            Ok(Logo::Graphic(out))
        }
    }
}

/// Whether `font` (a "Family 11" style name) has every octant glyph we use, read straight from
/// the installed font file so no external tool is needed.
pub fn font_has_octants(font: &str) -> bool {
    let family = font.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    if family.is_empty() {
        return false;
    }
    // scanning every system font is slow, so remember the answer per family
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .map(|d| d.join("screenfetch/octants"));
    let line = |ok: bool| format!("{family}\t{}", ok as u8);
    if let Some(text) = cache.as_ref().and_then(|c| std::fs::read_to_string(c).ok()) {
        for l in text.lines() {
            if l.starts_with(&format!("{family}\t")) {
                return l.ends_with('1');
            }
        }
    }
    let ok = scan_octants(family);
    if let Some(c) = &cache {
        let _ = std::fs::create_dir_all(c.parent().unwrap());
        let mut text = std::fs::read_to_string(c).unwrap_or_default();
        text.push_str(&(line(ok) + "\n"));
        let _ = std::fs::write(c, text);
    }
    ok
}

fn scan_octants(family: &str) -> bool {
    use skrifa::{FontRef, MetadataProvider};
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    let query = usvg::fontdb::Query { families: &[usvg::fontdb::Family::Name(family)], ..Default::default() };
    let Some(id) = db.query(&query) else { return false };
    db.with_face_data(id, |data, index| {
        let font = FontRef::from_index(data, index).ok()?;
        let map = font.charmap();
        Some(['\u{1CD00}', '\u{1CDE5}', '\u{1CEA8}', '\u{1FBE6}', '\u{1FB82}'].iter().all(|&c| map.map(c).is_some()))
    })
    .flatten()
    .unwrap_or(false)
}

/// One cell from its sub-pixels (row-major, 2 wide; `None` = transparent): picks the two-colour
/// split with the least error and the glyph that draws it.
fn glyph_cell(p: &[Option<[i64; 3]>], bg: Option<[u8; 3]>, octants: bool) -> String {
    const QUAD: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
    let n = p.len();
    let full = (1usize << n) - 1;
    let glyph = |m: usize| if octants { crate::octants::OCTANT[m] } else { QUAD[m] };
    let mean = |mask: usize| -> Option<[i64; 3]> {
        let v: Vec<[i64; 3]> = (0..n).filter(|i| mask >> i & 1 == 1).filter_map(|i| p[i]).collect();
        (!v.is_empty()).then(|| [0, 1, 2].map(|k| v.iter().map(|c| c[k]).sum::<i64>() / v.len() as i64))
    };
    let err = |mask: usize| -> i64 {
        let Some(m) = mean(mask) else { return 0 };
        (0..n)
            .filter(|i| mask >> i & 1 == 1)
            .filter_map(|i| p[i])
            .map(|c| (0..3).map(|k| (c[k] - m[k]).pow(2)).sum::<i64>())
            .sum()
    };
    // a cell that is just the terminal background stays a plain space
    if let Some(b) = bg {
        if p.iter().flatten().all(|c| (0..3).all(|k| (c[k] - b[k] as i64).abs() < 6)) {
            return " ".into();
        }
    }
    let opaque = (0..n).fold(0usize, |m, i| m | (p[i].is_some() as usize) << i);
    if opaque == 0 {
        return " ".into();
    }
    // transparent sub-pixels must stay background-coloured: draw only the opaque ones
    let best = if opaque != full {
        opaque
    } else {
        // solid first so ties prefer a single colour
        let mut best = (err(full), full);
        for mask in 1..full >> 1 << 1 {
            if mask >> (n - 1) & 1 == 1 {
                continue;
            }
            let e = err(mask) + err(full ^ mask);
            if e < best.0 {
                best = (e, mask);
            }
        }
        best.1
    };
    let fg = mean(best).unwrap();
    let mut s = format!("\x1b[38;2;{};{};{}m", fg[0], fg[1], fg[2]);
    if let Some(b) = mean(full ^ best).filter(|_| opaque == full) {
        s += &format!("\x1b[48;2;{};{};{}m", b[0], b[1], b[2]);
    }
    s.push(glyph(best));
    s + "\x1b[0m"
}
