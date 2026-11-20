// SPDX-License-Identifier: GPL-3.0-or-later
//! Braille dots, half blocks, colours: the twin of the script module
//! `ptyrc/canvas`. Pure; `draw` and `paint` write into a [`Canvas`].
//!
//! A colour is `Color::Rgb`, or a fixed colour of the 256
//! (`Color::Palette(16..=255)`) read as its value. Another (`Below`,
//! `Default`, `Palette(0..16)`, which the terminal sets) panics.

use crate::{Canvas, Cell, Color, Run};
use std::cell::RefCell;
use std::collections::HashMap;

// ---- colours ----

const CUBE: [u32; 6] = [0, 95, 135, 175, 215, 255];
const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// `0xrrggbb`.
fn value(c: Color) -> u32 {
    match c {
        Color::Rgb(r, g, b) => u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        Color::Palette(n) if n >= 16 => palette_value(n),
        c => panic!("{c:?} is no colour: Color::Rgb, or Color::Palette(16..=255)"),
    }
}

fn rgb(v: u32) -> Color {
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn palette_value(n: u8) -> u32 {
    let n = u32::from(n);
    if n >= 232 {
        let v = 8 + 10 * (n - 232);
        return v << 16 | v << 8 | v;
    }
    let n = n - 16;
    CUBE[(n / 36) as usize] << 16 | CUBE[(n / 6 % 6) as usize] << 8 | CUBE[(n % 6) as usize]
}

/// Rounded half up, as the script's `Math.round`.
fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn mixed(a: u32, b: u32, t: f64) -> u32 {
    if t.is_nan() || t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let ch = |s: u32| {
        let (x, y) = (f64::from(a >> s & 255), f64::from(b >> s & 255));
        round(x + (y - x) * t) as u32
    };
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// From `a` (`t` 0) to `b` (`t` 1), `t` clamped, each channel rounded
/// half up.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    rgb(mixed(value(a), value(b), t))
}

/// `[r, g, b]`.
pub fn channels(c: Color) -> [u8; 3] {
    let v = value(c);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// `"#rrggbb"`.
pub fn hex(c: Color) -> String {
    format!("#{:06x}", value(c))
}

/// The value of colour `n` of the 256, `16..=255`; `None` under 16.
pub fn palette(n: u8) -> Option<Color> {
    (n >= 16).then(|| rgb(palette_value(n)))
}

/// `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`,
/// `white`, and `bright-` each: `0..=15`, the 16 the terminal sets.
pub fn named(name: &str) -> Option<u8> {
    let (bright, base) = match name.strip_prefix("bright-") {
        Some(b) => (8, b),
        None => (0, name),
    };
    NAMES
        .iter()
        .position(|n| *n == base)
        .map(|i| i as u8 + bright)
}

thread_local! {
    static NEAREST: RefCell<HashMap<u32, u8>> = RefCell::new(HashMap::new());
}

fn nearest(v: u32) -> u8 {
    if let Some(n) = NEAREST.with(|m| m.borrow().get(&v).copied()) {
        return n;
    }
    let (r, g, b) = ((v >> 16) as i64, (v >> 8 & 255) as i64, (v & 255) as i64);
    let chroma = r.max(g).max(b) - r.min(g).min(b);
    let (mut best, mut n) = (i64::MAX, 16u8);
    for i in 16..=255u8 {
        let p = palette_value(i);
        let (x, y, z) = ((p >> 16) as i64, (p >> 8 & 255) as i64, (p & 255) as i64);
        // Eyes weigh green most, blue least; a dark colour keeps its hue
        // rather than turn grey.
        let lost = (chroma - (x.max(y).max(z) - x.min(y).min(z))).max(0);
        let far = 3 * (x - r).pow(2) + 4 * (y - g).pow(2) + 2 * (z - b).pow(2) + 2 * lost.pow(2);
        if far < best {
            (best, n) = (far, i);
        }
    }
    NEAREST.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= 4096 {
            m.clear();
        }
        m.insert(v, n);
    });
    n
}

/// The nearest of the 240 fixed colours of the 256, `16..=255`:
/// channels weighed 3, 4, 2, a lost chroma counted.
pub fn xterm(c: Color) -> u8 {
    nearest(value(c))
}

// ---- Braille ----

/// The bit of the dot `(x, y)` of a cell: `DOTS[y][x]`.
pub const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// The glyph of `bits`; `None` when no dot is on.
pub fn glyph(bits: u8) -> Option<char> {
    (bits != 0).then(|| char::from_u32(0x2800 + u32::from(bits)).expect("braille"))
}

/// The bits of a glyph from bits row by row: bit `2y + x` the dot `(x, y)`.
pub const fn from_grid(grid: u8) -> u8 {
    GRID[grid as usize]
}

const GRID: [u8; 256] = {
    let mut out = [0u8; 256];
    let mut g = 0;
    while g < 256 {
        let mut bits = 0;
        let mut i = 0;
        while i < 8 {
            if g >> i & 1 != 0 {
                bits |= DOTS[i / 2][i % 2];
            }
            i += 1;
        }
        out[g] = bits;
        g += 1;
    }
    out
};

/// `2 * width` by `4 * height` dots in `width` by `height` cells, the
/// glyph of a cell `U+2800 + bits`. A dot outside is ignored; read, it
/// is off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Braille {
    w: u32,
    h: u32,
    bits: Vec<u8>,
}

impl Braille {
    /// All dots off.
    pub fn new(width: u32, height: u32) -> Braille {
        Braille {
            w: width,
            h: height,
            bits: vec![0; width as usize * height as usize],
        }
    }

    /// Cells across.
    pub fn width(&self) -> u32 {
        self.w
    }

    /// Cells down.
    pub fn height(&self) -> u32 {
        self.h
    }

    /// A byte a cell, `(cx, cy)` at `cy * width + cx`.
    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    pub fn bits_mut(&mut self) -> &mut [u8] {
        &mut self.bits
    }

    /// Every dot off.
    pub fn clear(&mut self) -> &mut Braille {
        self.bits.fill(0);
        self
    }

    fn dot(&self, x: i64, y: i64) -> Option<(usize, u8)> {
        let inside = x >= 0 && y >= 0 && x < 2 * i64::from(self.w) && y < 4 * i64::from(self.h);
        inside.then(|| {
            let i = (y >> 2) as usize * self.w as usize + (x >> 1) as usize;
            (i, DOTS[(y & 3) as usize][(x & 1) as usize])
        })
    }

    /// The dot `(x, y)` on or off.
    pub fn set(&mut self, x: i64, y: i64, on: bool) {
        if let Some((i, bit)) = self.dot(x, y) {
            if on {
                self.bits[i] |= bit;
            } else {
                self.bits[i] &= !bit;
            }
        }
    }

    pub fn get(&self, x: i64, y: i64) -> bool {
        self.dot(x, y)
            .is_some_and(|(i, bit)| self.bits[i] & bit != 0)
    }

    /// Every dot from `(x0, y0)` to `(x1, y1)`: `max(1, round(max(|dx|,
    /// |dy|)))` steps, each dot rounded half up.
    pub fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, on: bool) {
        let n = round((x1 - x0).abs().max((y1 - y0).abs())).max(1.0);
        for i in 0..=n as i64 {
            let t = i as f64 / n;
            let x = round(x0 + (x1 - x0) * t) as i64;
            let y = round(y0 + (y1 - y0) * t) as i64;
            self.set(x, y, on);
        }
    }

    fn at(&self, cx: u32, cy: u32) -> Option<usize> {
        (cx < self.w && cy < self.h).then(|| cy as usize * self.w as usize + cx as usize)
    }

    /// The bits of the cell `(cx, cy)`; 0 outside.
    pub fn cell(&self, cx: u32, cy: u32) -> u8 {
        self.at(cx, cy).map_or(0, |i| self.bits[i])
    }

    /// The bits of the cell `(cx, cy)`, all at once.
    pub fn set_cell(&mut self, cx: u32, cy: u32, bits: u8) {
        if let Some(i) = self.at(cx, cy) {
            self.bits[i] = bits;
        }
    }

    /// The glyph of the cell `(cx, cy)`; `None` when no dot of it is on.
    pub fn glyph(&self, cx: u32, cy: u32) -> Option<char> {
        glyph(self.cell(cx, cy))
    }

    /// The cells with a dot on, row by row.
    pub fn cells(&self) -> Vec<(u32, u32)> {
        let w = self.w.max(1) as usize;
        let on = self.bits.iter().enumerate().filter(|(_, b)| **b != 0);
        on.map(|(i, _)| ((i % w) as u32, (i / w) as u32)).collect()
    }

    /// A string a row of cells, a space where no dot is on.
    pub fn rows(&self) -> Vec<String> {
        let row = |r: &[u8]| r.iter().map(|&b| glyph(b).unwrap_or(' ')).collect();
        self.bits.chunks(self.w.max(1) as usize).map(row).collect()
    }

    /// Each row written on `canvas` at `(x, y)`, spaces included.
    /// `style`: its `fg`, `bg`, `attrs` on each cell written (a space a
    /// hole unless it has a `bg`); `None`: the colours kept.
    pub fn draw(&self, canvas: &mut Canvas, x: u32, y: u32, style: Option<&Run>) -> &Braille {
        for (cy, row) in self.rows().iter().enumerate() {
            let cy = y.saturating_add(cy as u32);
            match style {
                None => canvas.text(x, cy, row),
                Some(s) => {
                    for (i, ch) in row.chars().enumerate() {
                        let cell = Cell {
                            ch: Some(ch),
                            fg: s.fg,
                            bg: s.bg,
                            attrs: s.attrs,
                        };
                        canvas.set(x.saturating_add(i as u32), cy, cell);
                    }
                }
            }
        }
        self
    }

    /// Every cell written on `canvas` from `(x, y)`: a cell with a dot
    /// its glyph in `fg(cx, cy)`, another nothing drawn.
    pub fn paint(&self, canvas: &mut Canvas, x: u32, y: u32, fg: impl Fn(u32, u32) -> Color) {
        for cy in 0..self.h {
            for cx in 0..self.w {
                let cell = match self.glyph(cx, cy) {
                    Some(ch) => Cell {
                        ch: Some(ch),
                        fg: fg(cx, cy),
                        ..Cell::default()
                    },
                    None => Cell::default(),
                };
                canvas.set(x + cx, y + cy, cell);
            }
        }
    }
}

// ---- half blocks ----

/// Two pixels are alike when the sum of their channel differences is
/// at most this.
pub const NEAR: u32 = 24;

fn alike(a: u32, b: u32) -> bool {
    let d = |s: u32| (a >> s & 255).abs_diff(b >> s & 255);
    d(16) + d(8) + d(0) <= NEAR
}

/// A glyph over a cell: its text, `fg`, `bg` (`None`: not given).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Over {
    ch: char,
    fg: Option<u32>,
    bg: Option<u32>,
}

/// `width` by `2 * height` pixels of colour in `width` by `height`
/// cells, the top pixel the foreground of `▀`, the bottom its
/// background; glyphs over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Half {
    w: u32,
    h: u32,
    px: Vec<u32>,
    over: Vec<Option<Over>>,
}

impl Half {
    /// Every pixel `color`.
    pub fn new(width: u32, height: u32, color: Color) -> Half {
        let n = width as usize * height as usize;
        Half {
            w: width,
            h: height,
            px: vec![value(color); 2 * n],
            over: vec![None; n],
        }
    }

    /// Cells across; pixels too.
    pub fn width(&self) -> u32 {
        self.w
    }

    /// Cells down; pixels: twice as many.
    pub fn height(&self) -> u32 {
        self.h
    }

    /// Every pixel; the glyphs kept.
    pub fn fill(&mut self, color: Color) -> &mut Half {
        self.px.fill(value(color));
        self
    }

    fn pixel(&self, x: i64, y: i64) -> Option<usize> {
        let inside = x >= 0 && y >= 0 && x < i64::from(self.w) && y < 2 * i64::from(self.h);
        inside.then(|| y as usize * self.w as usize + x as usize)
    }

    /// A pixel; `None` outside.
    pub fn get(&self, x: i64, y: i64) -> Option<Color> {
        self.pixel(x, y).map(|i| rgb(self.px[i]))
    }

    fn paint(&mut self, x: i64, y: i64, v: u32, alpha: f64) {
        if let Some(i) = self.pixel(x, y) {
            self.px[i] = if alpha >= 1.0 {
                v
            } else {
                mixed(self.px[i], v, alpha)
            };
        }
    }

    /// A pixel; `alpha` under 1: `mix(old, color, alpha)`.
    pub fn set(&mut self, x: i64, y: i64, color: Color, alpha: f64) {
        self.paint(x, y, value(color), alpha);
    }

    pub fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, color: Color, alpha: f64) {
        let v = value(color);
        for j in y..y.saturating_add(h) {
            for i in x..x.saturating_add(w) {
                self.paint(i, j, v, alpha);
            }
        }
    }

    /// The pixels whose centre `(x + .5, y + .5)` is within `r` of
    /// `(cx, cy)`.
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, color: Color, alpha: f64) {
        let v = value(color);
        for y in (cy - r - 1.0).trunc() as i64..(cy + r + 2.0).trunc() as i64 {
            for x in (cx - r - 1.0).trunc() as i64..(cx + r + 2.0).trunc() as i64 {
                if (x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2) <= r * r {
                    self.paint(x, y, v, alpha);
                }
            }
        }
    }

    /// Glyphs from the cell `(cx, cy)`, one a char; `bg` `None`: the
    /// mean of the two pixels under.
    pub fn put(&mut self, cx: i64, cy: i64, text: &str, fg: Option<Color>, bg: Option<Color>) {
        let (fg, bg) = (fg.map(value), bg.map(value));
        for (i, ch) in text.chars().enumerate() {
            let x = cx.saturating_add(i as i64);
            if (0..i64::from(self.w)).contains(&x) && (0..i64::from(self.h)).contains(&cy) {
                self.over[cy as usize * self.w as usize + x as usize] = Some(Over { ch, fg, bg });
            }
        }
    }

    /// The cells of `braille` with a dot, where no glyph is and the two
    /// pixels are alike: the glyph, its `bg` the pixel under most dots,
    /// `fg` mixed over it by `tint`. Where they differ, `fg` mixed by
    /// `rough` into each pixel under a dot.
    pub fn ink(&mut self, braille: &Braille, fg: Option<Color>, tint: f64, rough: f64) {
        let f = fg.map(value);
        let w = self.w as usize;
        for (cx, cy) in braille.cells() {
            if cx >= self.w || cy >= self.h {
                continue;
            }
            let (cx, cy) = (cx as usize, cy as usize);
            if self.over[cy * w + cx].is_some() {
                continue;
            }
            let ti = 2 * cy * w + cx;
            let li = ti + w;
            let (top, low) = (self.px[ti], self.px[li]);
            let bits = braille.cell(cx as u32, cy as u32);
            if !alike(top, low) {
                if let Some(f) = f.filter(|_| rough != 0.0) {
                    if bits & 0x1b != 0 {
                        self.px[ti] = mixed(top, f, rough);
                    }
                    if bits & 0xe4 != 0 {
                        self.px[li] = mixed(low, f, rough);
                    }
                }
                continue;
            }
            let bg = if (bits & 0x1b).count_ones() > (bits & 0xe4).count_ones() {
                top
            } else {
                low
            };
            let ink = f.map(|f| if tint >= 1.0 { f } else { mixed(bg, f, tint) });
            self.over[cy * w + cx] = Some(Over {
                ch: glyph(bits).expect("a dot"),
                fg: ink,
                bg: Some(bg),
            });
        }
    }

    /// Rows of runs, colours of the 256 (`xterm`), `deep`: `Color::Rgb`;
    /// no `fg`: `Color::Below`. Cells alike one run, a blank joins the
    /// run before it; ink on two pixels the 256 tell apart is left out.
    pub fn rows(&self, deep: bool) -> Vec<Vec<Run>> {
        let name = |v: u32| {
            if deep {
                rgb(v)
            } else {
                Color::Palette(nearest(v))
            }
        };
        let w = self.w as usize;
        let mut out = Vec::with_capacity(self.h as usize);
        for cy in 0..self.h as usize {
            let mut row: Vec<Run> = Vec::new();
            for cx in 0..w {
                let tv = self.px[2 * cy * w + cx];
                let lv = self.px[(2 * cy + 1) * w + cx];
                let (a, b) = (name(tv), name(lv));
                let mut over = self.over[cy * w + cx];
                // Ink on two pixels alike that the 256 tell apart: the
                // glyph would take them for one, and what is behind would
                // move with it.
                let dots = |o: &Over| o.bg.is_some() && ('\u{2801}'..'\u{2900}').contains(&o.ch);
                if !deep && a != b && over.as_ref().is_some_and(dots) {
                    over = None;
                }
                let run = match over {
                    Some(o) => Run {
                        t: o.ch.to_string(),
                        fg: o.fg.map_or(Color::Below, name),
                        bg: name(o.bg.unwrap_or_else(|| mixed(tv, lv, 0.5))),
                        attrs: 0,
                    },
                    None if a == b => Run::new(" ").bg(b),
                    None => Run::new("▀").fg(a).bg(b),
                };
                match row.last_mut() {
                    Some(last) if last.fg == run.fg && last.bg == run.bg => last.t += &run.t,
                    // A blank shows no foreground.
                    Some(last) if run.t == " " && last.bg == run.bg => last.t.push(' '),
                    _ => row.push(run),
                }
            }
            out.push(row);
        }
        out
    }

    /// The cells on `canvas` at `(x, y)`.
    pub fn draw(&self, canvas: &mut Canvas, x: u32, y: u32, deep: bool) -> &Half {
        for (cy, row) in self.rows(deep).iter().enumerate() {
            let cy = y.saturating_add(cy as u32);
            let mut at = x;
            for run in row {
                for ch in run.t.chars() {
                    let cell = Cell {
                        ch: Some(ch),
                        fg: run.fg,
                        bg: run.bg,
                        attrs: run.attrs,
                    };
                    canvas.set(at, cy, cell);
                    at = at.saturating_add(1);
                }
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `test_canvas.py::test_braille`.
    #[test]
    fn dots_and_glyphs() {
        let mut dots = Braille::new(3, 2);
        assert_eq!(dots.rows(), ["   ", "   "]);
        assert!(dots.cells().is_empty());
        assert_eq!((dots.glyph(0, 0), dots.glyph(9, 9)), (None, None));
        dots.set(0, 0, true);
        dots.set(1, 3, true);
        dots.set(5, 7, true);
        dots.set(6, 0, true); // outside: ignored
        dots.set(0, -1, true);
        assert!(dots.get(0, 0) && dots.get(1, 3) && !dots.get(1, 0) && !dots.get(99, 0));
        assert_eq!(dots.rows(), ["⢁  ", "  ⢀"]);
        assert_eq!(dots.cells(), [(0, 0), (2, 1)]);
        assert_eq!(dots.bits(), [0x81, 0, 0, 0, 0, 0x80]);
        dots.set(0, 0, false);
        assert_eq!(dots.glyph(0, 0), Some('⢀'));
        let mut line = Braille::new(2, 1);
        line.line(0.0, 0.0, 3.0, 3.0, true);
        assert_eq!(line.rows(), ["⠑⢄"]);
        line.line(0.0, 0.0, 3.0, 3.0, false);
        assert_eq!(line.clear().rows(), ["  "]);
        // Half up, as the script: 0.5 to 1.
        line.line(0.5, 0.0, 0.5, 0.0, true);
        assert!(line.get(1, 0) && !line.get(0, 0));
        for bits in 1..=255u8 {
            let mut cell = Braille::new(1, 1);
            for (y, row) in DOTS.iter().enumerate() {
                for (x, bit) in row.iter().enumerate() {
                    if bits & bit != 0 {
                        cell.set(x as i64, y as i64, true);
                    }
                }
            }
            assert_eq!(cell.glyph(0, 0), char::from_u32(0x2800 + u32::from(bits)));
        }
    }

    #[test]
    fn a_grid_row_by_row() {
        assert_eq!(from_grid(0), 0);
        assert_eq!(glyph(from_grid(0b1111_1111)), Some('⣿'));
        assert_eq!(glyph(from_grid(0b1000_0000)), Some('⢀'));
        assert_eq!(glyph(from_grid(0b0000_0011)), Some('⠉'));
        for g in 0..=255u8 {
            let mut cell = Braille::new(1, 1);
            for i in 0..8 {
                cell.set(i % 2, i / 2, g >> i & 1 != 0);
            }
            assert_eq!(cell.cell(0, 0), from_grid(g));
        }
    }

    #[test]
    fn painted_and_drawn_into_a_canvas() {
        let mut dots = Braille::new(2, 1);
        dots.set_cell(1, 0, from_grid(0b11));
        let mut canvas = Canvas::new(3, 1).unwrap();
        dots.paint(&mut canvas, 1, 0, |_, _| Color::Palette(2));
        assert_eq!(canvas.get(0, 0), Cell::default());
        assert_eq!(canvas.get(1, 0), Cell::default());
        assert_eq!(canvas.get(2, 0).ch, Some('⠉'));
        assert_eq!(canvas.get(2, 0).fg, Color::Palette(2));
        let mut canvas = Canvas::new(3, 1).unwrap();
        dots.draw(&mut canvas, 1, 0, Some(&Run::new("").fg(Color::Palette(5))));
        assert_eq!(canvas.get(1, 0).ch, Some(' '));
        assert_eq!(canvas.get(2, 0).ch, Some('⠉'));
        assert_eq!(canvas.get(2, 0).fg, Color::Palette(5));
        let mut canvas = Canvas::new(2, 1).unwrap();
        dots.draw(&mut canvas, 0, 0, None);
        assert_eq!(canvas.runs()[0], [Run::new(" ⠉")]);
    }

    /// `canvas.tests.js`: colours.
    #[test]
    fn colours() {
        let (black, white) = (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255));
        assert_eq!(mix(black, white, 0.5), Color::Rgb(128, 128, 128));
        assert_eq!(mix(black, Color::Rgb(1, 3, 5), 0.5), Color::Rgb(1, 2, 3));
        assert_eq!(mix(black, white, -1.0), black);
        assert_eq!(mix(black, white, f64::NAN), black);
        assert_eq!(mix(black, white, 2.0), white);
        assert_eq!(channels(Color::Rgb(1, 2, 3)), [1, 2, 3]);
        assert_eq!(hex(Color::Rgb(1, 2, 255)), "#0102ff");
        assert_eq!(palette(16), Some(black));
        assert_eq!(palette(231), Some(white));
        assert_eq!(palette(196), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(palette(232), Some(Color::Rgb(8, 8, 8)));
        assert_eq!(palette(15), None);
        assert_eq!(hex(Color::Palette(21)), "#0000ff");
        assert_eq!(xterm(Color::Rgb(255, 0, 0)), 196);
        assert_eq!(xterm(Color::Rgb(250, 250, 250)), 231);
        assert_eq!(xterm(Color::Rgb(128, 128, 128)), 244);
        for n in 16..=255u8 {
            assert_eq!(xterm(Color::Palette(n)), n, "{n}");
        }
        assert_eq!(named("red"), Some(1));
        assert_eq!(named("bright-white"), Some(15));
        assert_eq!((named("pink"), named("bright-")), (None, None));
    }

    #[test]
    #[should_panic(expected = "no colour")]
    fn a_colour_the_terminal_sets_is_no_value() {
        hex(Color::Palette(3));
    }

    /// `canvas.tests.js`: half blocks.
    #[test]
    fn half_blocks() {
        let (black, red, blue) = (
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 0, 0),
            Color::Rgb(0, 0, 255),
        );
        let mut h = Half::new(3, 1, black);
        assert_eq!((h.width(), h.height()), (3, 1));
        assert_eq!(h.get(0, 1), Some(black));
        assert_eq!((h.get(3, 0), h.get(0, 2), h.get(-1, 0)), (None, None, None));
        h.set(0, 0, red, 1.0);
        h.set(1, 0, red, 0.5);
        assert_eq!(h.get(1, 0), Some(Color::Rgb(128, 0, 0)));
        let rows = h.rows(false);
        assert_eq!(
            rows[0],
            [
                Run::new("▀").fg(Color::Palette(196)).bg(Color::Palette(16)),
                // A blank joins the run before it: it shows no foreground.
                Run::new("▀ ").fg(Color::Palette(88)).bg(Color::Palette(16)),
            ]
        );
        h.rect(0, 0, 3, 2, blue, 1.0);
        assert_eq!(h.rows(true)[0], [Run::new("   ").bg(blue)]);
        h.put(1, 0, "ab", Some(red), None);
        assert_eq!(
            h.rows(false)[0],
            [
                Run::new(" ").bg(Color::Palette(21)),
                Run::new("ab")
                    .fg(Color::Palette(196))
                    .bg(Color::Palette(21)),
            ]
        );
        h.fill(black);
        h.disc(1.5, 1.0, 0.8, red, 1.0);
        assert_eq!(h.get(1, 0), Some(red));
        assert_eq!(h.get(1, 1), Some(red));
        assert_eq!(h.get(0, 0), Some(black));
        let mut canvas = Canvas::new(3, 1).unwrap();
        h.draw(&mut canvas, 0, 0, true);
        assert_eq!(canvas.get(1, 0).ch, Some('a'));
        assert_eq!(canvas.get(0, 0).bg, black);
    }

    #[test]
    fn ink_over_half_blocks() {
        let (black, white, red) = (
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            Color::Rgb(255, 0, 0),
        );
        let mut dots = Braille::new(2, 1);
        dots.set(0, 0, true);
        dots.set(2, 0, true);
        let mut h = Half::new(2, 1, black);
        h.set(1, 0, white, 1.0); // the cell 1 of two pixels unlike
        h.ink(&dots, Some(red), 0.5, 1.0);
        assert_eq!(
            h.rows(true)[0],
            [
                Run::new("⠁").fg(Color::Rgb(128, 0, 0)).bg(black),
                Run::new("▀").fg(red).bg(black),
            ]
        );
        // A glyph is kept: no ink over it.
        let mut h = Half::new(1, 1, black);
        h.put(0, 0, "x", None, None);
        h.ink(&dots, Some(red), 1.0, 0.0);
        assert_eq!(h.rows(true)[0], [Run::new("x").bg(black)]);
    }

    /// `canvas.tests.js`: `test_ink`, `test_half_draw`.
    #[test]
    fn as_the_script() {
        let (red, blue, white) = (
            Color::Rgb(255, 0, 0),
            Color::Rgb(0, 0, 255),
            Color::Rgb(255, 255, 255),
        );
        let mut half = Half::new(3, 1, blue);
        half.set(1, 0, red, 1.0);
        half.put(2, 0, "x", None, None);
        let mut dots = Braille::new(3, 1);
        for x in 0..6 {
            dots.set(x, 0, true);
        }
        half.ink(&dots, Some(white), 0.5, 0.0);
        assert_eq!(
            half.rows(true)[0],
            [
                Run::new("⠉").fg(Color::Rgb(0x80, 0x80, 0xff)).bg(blue),
                Run::new("▀").fg(red).bg(blue),
                Run::new("x").bg(blue),
            ]
        );
        let mut near = Half::new(1, 1, Color::Rgb(10, 10, 10));
        near.set(0, 1, Color::Rgb(14, 14, 14), 1.0);
        let mut low = Braille::new(1, 1);
        low.set(0, 3, true);
        near.ink(&low, Some(red), 1.0, 0.0);
        assert_eq!(
            near.rows(true)[0],
            [Run::new("⡀").fg(red).bg(Color::Rgb(14, 14, 14))]
        );
        let mut edge = Half::new(1, 1, blue);
        edge.set(0, 1, red, 1.0);
        edge.ink(&low, Some(white), 1.0, 0.5);
        assert_eq!(edge.get(0, 0), Some(blue));
        assert_eq!(channels(edge.get(0, 1).unwrap()), [255, 128, 128]);
        assert_eq!(edge.rows(false)[0][0].t, "▀");
        near.fill(Color::Rgb(100, 100, 100));
        near.set(0, 1, Color::Rgb(112, 112, 112), 1.0);
        near.ink(&low, Some(red), 1.0, 0.0);
        assert_eq!(
            near.rows(false),
            [[Run::new("▀")
                .fg(Color::Palette(241))
                .bg(Color::Palette(242))]]
        );
        assert_eq!(near.rows(true)[0][0].t, "⡀");

        let mut half = Half::new(2, 1, blue);
        half.set(0, 0, red, 1.0);
        let mut canvas = Canvas::new(3, 1).unwrap();
        half.draw(&mut canvas, 1, 0, false);
        assert_eq!(
            canvas.runs(),
            [[
                Run::new(" "),
                Run::new("▀ ")
                    .fg(Color::Palette(196))
                    .bg(Color::Palette(21))
            ]]
        );
    }
}
