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

pub fn rasterize(svg: &[u8], w: u32, h: u32) -> Result<tiny_skia::Pixmap, String> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = (w as f32 / size.width()).min(h as f32 / size.height());
    let (dx, dy) = ((w as f32 - size.width() * scale) / 2.0, (h as f32 - size.height() * scale) / 2.0);
    let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("bad size")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy), &mut pm.as_mut());
    Ok(pm)
}

/// Returns the escape sequence / text for the logo, as `rows` printable lines for
/// `Blocks`, or a single image sequence (cursor not moved) for the graphics protocols.
pub enum Logo {
    Lines(Vec<String>),
    Graphic(String),
}

pub fn logo(svg: &[u8], proto: Protocol, cols: u32, rows: u32, cell: (u32, u32)) -> Result<Logo, String> {
    match proto {
        Protocol::Blocks => {
            // quadrant characters: 2x2 sub-pixels per cell, each supersampled 3x
            const SS: u32 = 3;
            let pm = rasterize(svg, cols * 2 * SS, rows * 2 * SS)?;
            let sub = |x: u32, y: u32| -> Option<[i64; 3]> {
                let (mut a, mut c) = (0i64, [0i64; 3]);
                for (dx, dy) in (0..SS).flat_map(|dy| (0..SS).map(move |dx| (dx, dy))) {
                    let p = pm.pixel(x * SS + dx, y * SS + dy).unwrap();
                    a += p.alpha() as i64;
                    c = [c[0] + p.red() as i64, c[1] + p.green() as i64, c[2] + p.blue() as i64];
                }
                (a > 128 * (SS * SS) as i64).then(|| c.map(|v| (v * 255 / a).min(255)))
            };
            let lines = (0..rows)
                .map(|r| {
                    (0..cols)
                        .map(|c| {
                            quad([sub(2 * c, 2 * r), sub(2 * c + 1, 2 * r), sub(2 * c, 2 * r + 1), sub(2 * c + 1, 2 * r + 1)])
                        })
                        .collect::<String>()
                })
                .collect();
            Ok(Logo::Lines(lines))
        }
        Protocol::Sixel => {
            // cursor is saved/restored around the image
            let pm = rasterize(svg, cols * cell.0, rows * cell.1)?;
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
            let png = B64.encode(rasterize(svg, side, side)?.encode_png().map_err(|e| e.to_string())?);
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

/// One cell from its sub-pixels (TL, TR, BL, BR; `None` = transparent).
fn quad(p: [Option<[i64; 3]>; 4]) -> String {
    const CH: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
    let mean = |mask: usize| -> Option<[i64; 3]> {
        let v: Vec<[i64; 3]> = (0..4).filter(|i| mask >> i & 1 == 1).filter_map(|i| p[i]).collect();
        (!v.is_empty()).then(|| [0, 1, 2].map(|k| v.iter().map(|c| c[k]).sum::<i64>() / v.len() as i64))
    };
    let err = |mask: usize, m: Option<[i64; 3]>| -> i64 {
        let Some(m) = m else { return 0 };
        (0..4)
            .filter(|i| mask >> i & 1 == 1)
            .filter_map(|i| p[i])
            .map(|c| (0..3).map(|k| (c[k] - m[k]).pow(2)).sum::<i64>())
            .sum()
    };
    let opaque = (0..4).fold(0usize, |m, i| m | (p[i].is_some() as usize) << i);
    if opaque == 0 {
        return " ".into();
    }
    // transparent sub-pixels must stay background-coloured: draw only the opaque ones
    let best = if opaque != 15 {
        opaque
    } else {
        // solid first so ties prefer a single colour
        let mut best = (err(15, mean(15)), 15usize);
        for mask in 1..8 {
            let e = err(mask, mean(mask)) + err(15 ^ mask, mean(15 ^ mask));
            if e < best.0 {
                best = (e, mask);
            }
        }
        best.1
    };
    let (fg, bg) = (mean(best).unwrap(), mean(15 ^ best));
    let mut s = format!("\x1b[38;2;{};{};{}m", fg[0], fg[1], fg[2]);
    if let Some(b) = bg.filter(|_| opaque == 15) {
        s += &format!("\x1b[48;2;{};{};{}m", b[0], b[1], b[2]);
    }
    s.push(CH[best]);
    s + "\x1b[0m"
}
