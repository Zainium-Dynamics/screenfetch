//! RGBA -> Sixel. Median-cut palette (<= 256 colours), transparent pixels left
//! unset (P2=1), run-length encoding with DECGRI like libsixel's `sixel_put_flash`.

use std::collections::HashMap;

/// Encode a straight-alpha RGBA8 buffer. Returns the full `ESC P ... ESC \` sequence.
pub fn encode(rgba: &[u8], width: usize, height: usize, max_colors: usize) -> String {
    assert_eq!(rgba.len(), width * height * 4, "buffer size mismatch");
    let max_colors = max_colors.clamp(2, 256);

    let opaque = |i: usize| rgba[i * 4 + 3] >= 128;
    let rgb = |i: usize| [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]];

    // histogram on 5 bits/channel buckets
    let key = |c: [u8; 3]| ((c[0] as u16 >> 3) << 10) | ((c[1] as u16 >> 3) << 5) | (c[2] as u16 >> 3);
    let mut hist: HashMap<u16, u32> = HashMap::new();
    for i in (0..width * height).filter(|&i| opaque(i)) {
        *hist.entry(key(rgb(i))).or_default() += 1;
    }
    let palette = median_cut(hist.into_iter().collect(), max_colors);

    let nearest = |c: [u8; 3]| -> usize {
        palette
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| (0..3).map(|k| (p[k] as i32 - c[k] as i32).pow(2)).sum::<i32>())
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    let mut cache: HashMap<u16, usize> = HashMap::new();
    let idx: Vec<Option<usize>> = (0..width * height)
        .map(|i| {
            opaque(i).then(|| {
                let c = rgb(i);
                *cache.entry(key(c)).or_insert_with(|| nearest(c))
            })
        })
        .collect();

    let mut out = String::from("\x1bP0;1;0q");
    out += &format!("\"1;1;{width};{height}");
    for (i, p) in palette.iter().enumerate() {
        let pct = |v: u8| (v as u32 * 100 + 127) / 255;
        out += &format!("#{i};2;{};{};{}", pct(p[0]), pct(p[1]), pct(p[2]));
    }

    for band in (0..height).step_by(6) {
        let mut used = vec![false; palette.len()];
        for y in band..(band + 6).min(height) {
            for x in 0..width {
                if let Some(c) = idx[y * width + x] {
                    used[c] = true;
                }
            }
        }
        let mut first = true;
        for c in (0..palette.len()).filter(|&c| used[c]) {
            if !first {
                out.push('$');
            }
            first = false;
            out += &format!("#{c}");
            let cols = (0..width).map(|x| {
                let bits = (0..6).fold(0u8, |acc, k| {
                    let y = band + k;
                    acc | (((y < height && idx[y * width + x] == Some(c)) as u8) << k)
                });
                (b'?' + bits) as char
            });
            put_rle(&mut out, cols);
        }
        out.push('-');
    }
    out += "\x1b\\";
    out
}

fn put_rle(out: &mut String, mut chars: impl Iterator<Item = char>) {
    let Some(mut cur) = chars.next() else { return };
    let mut n = 1usize;
    let flush = |out: &mut String, ch: char, n: usize| {
        if n > 3 {
            *out += &format!("!{n}{ch}");
        } else {
            (0..n).for_each(|_| out.push(ch));
        }
    };
    for ch in chars {
        if ch == cur {
            n += 1;
        } else {
            flush(out, cur, n);
            cur = ch;
            n = 1;
        }
    }
    flush(out, cur, n);
}

/// Median cut over (5-bit key, count) entries; returns up to `n` averaged colours.
fn median_cut(entries: Vec<(u16, u32)>, n: usize) -> Vec<[u8; 3]> {
    let expand = |k: u16| {
        let e = |v: u16| ((v << 3) | (v >> 2)) as u8;
        [e(k >> 10 & 31), e(k >> 5 & 31), e(k & 31)]
    };
    let mut boxes: Vec<Vec<([u8; 3], u32)>> = vec![entries.into_iter().map(|(k, c)| (expand(k), c)).collect()];
    while boxes.len() < n {
        // split the box with the widest channel range that has >1 colour
        let range = |b: &Vec<([u8; 3], u32)>| {
            (0..3)
                .map(|ch| {
                    let (lo, hi) = b.iter().fold((255u8, 0u8), |(l, h), (c, _)| (l.min(c[ch]), h.max(c[ch])));
                    (hi.saturating_sub(lo), ch)
                })
                .max()
                .unwrap()
        };
        let Some((bi, (r, ch))) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| (i, range(b)))
            .max_by_key(|(_, (r, _))| *r)
        else {
            break;
        };
        if r == 0 {
            break;
        }
        let mut b = boxes.swap_remove(bi);
        b.sort_by_key(|(c, _)| c[ch]);
        let total: u64 = b.iter().map(|(_, c)| *c as u64).sum();
        let (mut acc, mut cut) = (0u64, 1);
        for (i, (_, c)) in b.iter().enumerate() {
            acc += *c as u64;
            if acc * 2 >= total {
                cut = (i + 1).min(b.len() - 1);
                break;
            }
        }
        let hi = b.split_off(cut);
        boxes.push(b);
        boxes.push(hi);
    }
    boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let w: u64 = b.iter().map(|(_, c)| *c as u64).sum();
            let avg = |ch: usize| (b.iter().map(|(c, n)| c[ch] as u64 * *n as u64).sum::<u64>() / w.max(1)) as u8;
            [avg(0), avg(1), avg(2)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_colour_image() {
        // 2x7: red top row of 2, blue elsewhere (exercises a second band), one transparent px
        let mut px = Vec::new();
        for y in 0..7 {
            for x in 0..2 {
                px.extend(match (x, y) {
                    (_, 0) => [255, 0, 0, 255],
                    (1, 6) => [0, 0, 0, 0],
                    _ => [0, 0, 255, 255],
                });
            }
        }
        let s = encode(&px, 2, 7, 16);
        assert!(s.starts_with("\x1bP0;1;0q\"1;1;2;7#0;2;"));
        assert!(s.ends_with("-\x1b\\"));
        assert_eq!(s.matches('-').count(), 2, "two bands");
        assert!(s.contains(";2;100;0;0") && s.contains(";2;0;0;100"));
    }

    #[test]
    fn rle_kicks_in() {
        let px: Vec<u8> = (0..20).flat_map(|_| [10, 200, 10, 255]).collect();
        assert!(encode(&px, 20, 1, 4).contains("!20@"));
    }
}
