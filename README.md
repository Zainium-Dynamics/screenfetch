# screenfetch

A small, fast system-info tool that draws your distro's **SVG logo** straight in the terminal.

Terminal graphics: kitty / ghostty, iTerm2 / WezTerm, Sixel (foot, mlterm), with a truecolor half-block fallback that works everywhere.

## Build

```sh
cargo build --release
./target/release/screenfetch
```

## Install

```sh
cargo install --path .
screenfetch
```

After this just type `screenfetch` in any terminal.

## Logo lookup

For each id (`ID`, then `ID_LIKE` from `/etc/os-release`, or `--distro`), it looks for

```
<share>/<id>.pixmap/<id>_logo.svg
```

in `$SCREENFETCH_SHARE_DIR`, `/overlayer/syshub/share`, `/usr/local/share`, `/usr/share`, then falls back to the logos embedded at build time (`assets/logos/<id>_logo.svg`: zainium, fedora, arch, alma, cachyos).

## Build it for your own distro

Bake your name, logo and share dir into the binary:

```sh
# Zainium
SCREENFETCH_DISTRO=zainium \
SCREENFETCH_LOGO=assets/logos/zainium_logo.svg \
SCREENFETCH_SHARE_DIR=/overlayer/syshub/share \
cargo build --release

# Fedora
SCREENFETCH_DISTRO=fedora SCREENFETCH_LOGO=assets/logos/fedora_logo.svg \
SCREENFETCH_SHARE_DIR=/usr/share cargo build --release

# Anything else
SCREENFETCH_DISTRO=mydistro SCREENFETCH_LOGO=/path/to/logo.svg cargo build --release
```

## Usage

```
screenfetch [--logo FILE] [--distro ID] [--protocol auto|kitty|iterm2|sixel|blocks]
            [--rows N] [--list]
```

Sixel is detected automatically by asking the terminal (DA1); force it with `--protocol sixel`. The terminal is also asked for its cell size so the logo stays square.

## Terminals without an image protocol (Ptyxis, COSMIC Terminal)

Ptyxis/VTE does not enable Sixel and COSMIC Terminal has no image protocol at all, so nothing can draw real pixels there. The fallback renderer blends the logo over your terminal background and draws it with block glyphs: 2x4 sub-pixels per cell (Unicode 16 octants) when your terminal font has them (Adwaita Mono, Cascadia, ... checked with `fc-list`), otherwise 2x2 quadrants. Force one with `--glyphs octant|quadrant`.

## Config

`~/.config/screenfetch/config.toml` (all keys optional, CLI flags win):

```toml
logo     = "/path/to/logo.svg"
distro   = "fedora"
protocol = "auto"        # auto | kitty | iterm2 | sixel | blocks
rows     = 20
glyphs   = "auto"        # fallback renderer: auto | quadrant | octant
color    = "cyan"        # black red green yellow blue magenta cyan white
show     = ["os", "kernel", "uptime", "shell", "cpu", "memory"]
# keys: os release host kernel uptime shell display de wm theme icons font
#       cursor terminal termfont cpu gpu memory swap disk ip battery locale
```

## Crates

- `screenfetch`: the CLI
- `crates/sixel-enc`: dependency-free RGBA to Sixel encoder (format ported from libsixel)

## License

MIT. Distro logos are trademarks of their respective owners.
