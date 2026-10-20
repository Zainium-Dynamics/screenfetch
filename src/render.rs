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

    pub fn detect() -> Self {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let (term, prog) = (var("TERM"), var("TERM_PROGRAM"));
        if var("KITTY_WINDOW_ID") != "" || term == "xterm-kitty" || prog == "ghostty" || term == "xterm-ghostty" {
            Self::Kitty
        } else if prog == "iTerm.app" || prog == "WezTerm" {
            Self::Iterm2
        } else if term.starts_with("foot") || term.starts_with("mlterm") || var("SCREENFETCH_SIXEL") != "" {
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

pub fn logo(svg: &[u8], proto: Protocol, cols: u32, rows: u32) -> Result<Logo, String> {
    match proto {
        Protocol::Blocks => {
            let pm = rasterize(svg, cols, rows * 2)?;
            let px = |x: u32, y: u32| {
                let p = pm.pixel(x, y).unwrap().demultiply();
                (p.alpha() > 100).then(|| (p.red(), p.green(), p.blue()))
            };
            let lines = (0..rows)
                .map(|r| {
                    let mut s = String::new();
                    for x in 0..cols {
                        match (px(x, r * 2), px(x, r * 2 + 1)) {
                            (None, None) => s.push(' '),
                            (Some(t), None) => s += &format!("\x1b[38;2;{};{};{}m▀\x1b[0m", t.0, t.1, t.2),
                            (None, Some(b)) => s += &format!("\x1b[38;2;{};{};{}m▄\x1b[0m", b.0, b.1, b.2),
                            (Some(t), Some(b)) => s += &format!(
                                "\x1b[38;2;{};{};{};48;2;{};{};{}m▀\x1b[0m",
                                t.0, t.1, t.2, b.0, b.1, b.2
                            ),
                        }
                    }
                    s
                })
                .collect();
            Ok(Logo::Lines(lines))
        }
        Protocol::Sixel => {
            // assume ~10x20 px cells (1:2, like the block fallback); cursor is saved/restored around it
            let pm = rasterize(svg, cols * 10, rows * 20)?;
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
