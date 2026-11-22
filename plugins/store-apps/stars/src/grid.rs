// SPDX-License-Identifier: GPL-3.0-or-later
//! A picture of cells, a glyph and a colour each, and what the scenes
//! of `stars`, `pond`, `shore`, `flock` draw it with: Braille dots
//! (`Dots`), a hash (`noise`), the cells off the text (`clear`), the
//! moon's phase (`phase_of`). The twin's `stars.Grid`, `noise`,
//! `clear`, `runs`, `phase_of`; `life.DOTS`; `snake.walls_of`.

#![allow(dead_code)]

use ptyrc_app::{Canvas, Cell, Color, Value, json};

/// The bit of a Braille dot: `DOTS[row][column]`, 4 rows of 2.
pub const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// Days from a new moon to the next.
pub const SYNODIC: f64 = 29.530588853;
/// A new moon, 2000-01-06T18:14Z, in Unix seconds.
pub const NEW_MOON_S: f64 = 947182440.0;

/// Per row of cells, a mask: `true` where the scene is drawn.
pub type Mask = Vec<Vec<bool>>;

/// `w` by `h` cells, each empty or a glyph and a colour (256-colour
/// palette). Writes outside are ignored, reads outside are empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    w: usize,
    h: usize,
    cells: Vec<Option<(char, u8)>>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Grid {
        Grid {
            w,
            h,
            cells: vec![None; w * h],
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        let inside = (0..self.w as i64).contains(&x) && (0..self.h as i64).contains(&y);
        inside.then(|| y as usize * self.w + x as usize)
    }

    pub fn get(&self, x: i64, y: i64) -> Option<(char, u8)> {
        self.index(x, y).and_then(|i| self.cells[i])
    }

    pub fn set(&mut self, x: i64, y: i64, glyph: char, ink: u8) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = Some((glyph, ink));
        }
    }

    /// Empties a cell.
    pub fn unset(&mut self, x: i64, y: i64) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = None;
        }
    }

    /// The cells drawn of row `y`, left to right.
    pub fn row(&self, y: usize) -> impl Iterator<Item = (usize, char, u8)> + '_ {
        let row = &self.cells[y * self.w..(y + 1) * self.w];
        (row.iter().enumerate()).filter_map(|(x, c)| c.map(|(g, i)| (x, g, i)))
    }

    /// The cells drawn.
    pub fn count(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }

    /// `n` empty rows more, at the bottom.
    pub fn grow(&mut self, n: usize) {
        self.h += n;
        self.cells.resize(self.w * self.h, None);
    }

    /// The last `n` rows alone.
    pub fn keep_last(&mut self, n: usize) {
        let n = n.min(self.h);
        self.cells.drain(..(self.h - n) * self.w);
        self.h = n;
    }

    /// The cells drawn of `other` over this one, its top at row `y0`.
    pub fn over(&mut self, other: &Grid, y0: i64) {
        for y in 0..other.h {
            for (x, g, ink) in other.row(y) {
                self.set(x as i64, y0 + y as i64, g, ink);
            }
        }
    }

    /// `cells` of a sprite: rows of runs, one a colour, a space
    /// transparent; `shown`: a mask a row (`clear`).
    pub fn runs(&self, shown: Option<&[Vec<bool>]>) -> Value {
        let mut out = Vec::with_capacity(self.h);
        for y in 0..self.h {
            let mut line: Vec<Value> = Vec::new();
            let mut text = String::new();
            let (mut ink, mut at) = (0u8, 0usize);
            for (x, glyph, colour) in self.row(y) {
                if shown.is_some_and(|m| !m[y][x]) {
                    continue;
                }
                if colour != ink && !text.is_empty() {
                    line.push(json!({"t": (std::mem::take(&mut text)), "fg": ink}));
                }
                ink = colour;
                text.extend(std::iter::repeat_n(' ', x - at));
                text.push(glyph);
                at = x + 1;
            }
            if !text.is_empty() {
                line.push(json!({"t": text, "fg": ink}));
            }
            if line.is_empty() {
                line.push(json!({"t": " "}));
            }
            out.push(Value::Array(line));
        }
        Value::Array(out)
    }

    /// Onto `canvas` from its top-left: a cell drawn and shown takes
    /// its glyph and colour, any other is nothing drawn.
    pub fn paint(&self, canvas: &mut Canvas, shown: Option<&[Vec<bool>]>) {
        for y in 0..self.h {
            for x in 0..self.w {
                let cell = match self.cells[y * self.w + x] {
                    Some((g, ink)) if shown.is_none_or(|m| m[y][x]) => Cell {
                        ch: Some(g),
                        fg: Color::Palette(ink),
                        ..Cell::default()
                    },
                    _ => Cell::default(),
                };
                canvas.set(x as u32, y as u32, cell);
            }
        }
    }

    /// A row a line, each cell drawn `X GLYPH INK,`: what the twin's
    /// fixtures compare.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.h {
            if y > 0 {
                out.push('\n');
            }
            for (x, g, ink) in self.row(y) {
                out.push_str(&format!("{x} {g} {ink},"));
            }
        }
        out
    }
}

/// FNV-1a of `text`, 32 bits.
pub fn fnv(text: &str) -> u32 {
    (text.bytes()).fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    })
}

/// Braille dots of one colour a cell, `w` by `h` cells; `wrap`: a dot
/// out on a side comes back on the other.
#[derive(Clone, Debug)]
pub struct Dots {
    w: usize,
    h: usize,
    wrap: bool,
    /// Per cell: its dots, its colour.
    cells: Vec<(u8, u8)>,
}

impl Dots {
    pub fn new(w: usize, h: usize, wrap: bool) -> Dots {
        Dots {
            w,
            h,
            wrap,
            cells: vec![(0, 0); w * h],
        }
    }

    /// Sets the dot `(px, py)`; the cell takes `colour`.
    pub fn dot(&mut self, mut px: i64, py: i64, colour: u8) {
        let across = 2 * self.w as i64;
        if self.wrap && self.w > 0 {
            px = px.rem_euclid(across);
        }
        if (0..across).contains(&px) && (0..4 * self.h as i64).contains(&py) {
            let cell = &mut self.cells[(py >> 2) as usize * self.w + (px >> 1) as usize];
            cell.0 |= DOTS[(py & 3) as usize][(px & 1) as usize];
            cell.1 = colour;
        }
    }

    /// Every other dot from `a` to `b`.
    pub fn line(&mut self, a: (i64, i64), b: (i64, i64), colour: u8) {
        let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
        let step = |d: i64, i: i64| ((d * i) as f64 / n as f64).round_ties_even() as i64;
        for i in (0..=n).step_by(2) {
            self.dot(a.0 + step(b.0 - a.0, i), a.1 + step(b.1 - a.1, i), colour);
        }
    }

    /// The cells with a dot: `(x, y, bits, colour)`.
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize, u8, u8)> + '_ {
        let w = self.w.max(1);
        (self.cells.iter().enumerate())
            .filter(|(_, c)| c.0 != 0)
            .map(move |(i, &(bits, colour))| (i % w, i / w, bits, colour))
    }

    /// Each cell with a dot into `grid`, its glyph and colour.
    pub fn onto(&self, grid: &mut Grid) {
        for (x, y, bits, colour) in self.cells() {
            grid.set(x as i64, y as i64, braille(bits), colour);
        }
    }
}

/// The Braille glyph of `bits`.
pub fn braille(bits: u8) -> char {
    char::from_u32(0x2800 + u32::from(bits)).expect("a Braille glyph")
}

fn mix(n: u32) -> u32 {
    let n = (n ^ n >> 16).wrapping_mul(0x45D9F3B);
    let n = (n ^ n >> 16).wrapping_mul(0x45D9F3B);
    n ^ n >> 16
}

/// A number in [0, 1), the same for the same `(a, b)`.
pub fn noise(a: i64, b: i64) -> f64 {
    let n = mix(mix(a as u32).wrapping_add((b as u32).wrapping_mul(0x9E3779B1)));
    f64::from(n) / 4294967296.0
}

/// Cells holding a glyph of `rows` (the shape or the text), `(x, y)`;
/// a wide glyph takes two columns.
pub fn walls_of(rows: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        let mut x = 0;
        for ch in row.chars() {
            let wide = ptyrc_app::width(ch) == 2;
            if !ch.is_whitespace() {
                out.push((x, y));
                if wide {
                    out.push((x + 1, y));
                }
            }
            x += if wide { 2 } else { 1 };
        }
    }
    out
}

/// A mask a row of a view of `w` by `h` cells holding the text `rows`:
/// set where the scene is drawn, off under each glyph and under the
/// cell on each side of it.
pub fn clear(rows: &[String], w: usize, h: usize) -> Mask {
    let mut out = vec![vec![true; w]; h];
    for (x, y) in walls_of(rows) {
        if x < w && y < h {
            out[y][x.saturating_sub(1)..(x + 2).min(w)].fill(false);
        }
    }
    out
}

/// The phase of the moon at `unix_ms`: 0 new, 0.25 the first quarter,
/// 0.5 full; from the mean month, right within a day.
pub fn phase_of(unix_ms: f64) -> f64 {
    let days = (unix_ms / 1000.0 - NEW_MOON_S) / 86400.0;
    (days / SYNODIC).rem_euclid(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    /// `test_runs_and_clear`.
    #[test]
    fn runs_and_clear() {
        let mut g = Grid::new(8, 2);
        for (x, c, ink) in [(0, 'a', 1), (1, 'b', 1), (4, 'c', 2), (9, 'z', 3)] {
            g.set(x, 0, c, ink);
        }
        assert_eq!(
            g.runs(None),
            json!([[{"t": "ab", "fg": 1}, {"t": "  c", "fg": 2}], [{"t": " "}]])
        );
        let bits = |n: u32, w: usize| (0..w).map(|x| n >> x & 1 != 0).collect::<Vec<_>>();
        let shown = vec![bits(0b10010, 8), bits(0, 8)];
        assert_eq!(
            g.runs(Some(&shown)),
            json!([[{"t": " b", "fg": 1}, {"t": "  c", "fg": 2}], [{"t": " "}]])
        );
        assert_eq!(
            clear(&rows(&["", "  ab"]), 8, 2),
            [bits(0xFF, 8), bits(0b11100001, 8)]
        );
        assert_eq!(clear(&rows(&["a世"]), 6, 1), [bits(0b110000, 6)]);
        assert_eq!(
            clear(&rows(&["a", "b", "c"]), 2, 2),
            [bits(0, 2), bits(0, 2)]
        );
    }

    /// The twin's `noise` and `walls_of`.
    #[test]
    fn noise_and_walls() {
        // stars.noise of the twin.
        assert_eq!(noise(0, 0), 0.0);
        assert_eq!(noise(1, 0), 0.0883708749897778);
        assert_eq!(noise(977, 12), 0.8266763794235885);
        assert_eq!(noise(-3, 7), 0.39387425454333425);
        assert_eq!(noise(1 << 33, 1 << 31), noise(0, 1 << 31));
        assert_eq!(
            walls_of(&rows(&["a b", " 世x"])),
            [(0, 0), (2, 0), (1, 1), (2, 1), (3, 1)]
        );
    }

    #[test]
    fn a_grid() {
        let mut g = Grid::new(3, 2);
        g.set(0, 0, 'a', 1);
        g.set(5, 0, 'z', 1);
        g.set(2, 1, 'b', 2);
        assert_eq!(g.text(), "0 a 1,\n2 b 2,");
        let mut sky = g.clone();
        sky.grow(1);
        assert_eq!(sky.height(), 3);
        let mut low = Grid::new(3, 1);
        low.set(1, 0, 'g', 3);
        sky.over(&low, 2);
        assert_eq!(sky.text(), "0 a 1,\n2 b 2,\n1 g 3,");
        sky.keep_last(1);
        assert_eq!(sky.text(), "1 g 3,");
        g.unset(0, 0);
        assert_eq!(g.count(), 1);
        assert_eq!(fnv(""), 2166136261);
    }

    #[test]
    fn dots() {
        let mut d = Dots::new(2, 1, true);
        d.dot(-1, 0, 5);
        d.dot(0, 3, 6);
        let mut g = Grid::new(2, 1);
        d.onto(&mut g);
        assert_eq!(g.text(), "0 ⡀ 6,1 ⠈ 5,");
        let mut d = Dots::new(4, 1, false);
        d.line((0, 0), (6, 3), 1);
        let cells: Vec<_> = d.cells().collect();
        assert_eq!(
            cells,
            [
                (0, 0, 0x01, 1),
                (1, 0, 0x02, 1),
                (2, 0, 0x04, 1),
                (3, 0, 0x40, 1)
            ]
        );
    }

    /// `test_moon_way_and_phase`: new moons of 2024-04-08 (an eclipse)
    /// and 2026-11-09; a full one.
    #[test]
    fn phases() {
        for (unix_s, phase) in [
            (1712599200.0, 0.0),
            (1794211200.0, 0.0),
            (1795532400.0, 0.5),
        ] {
            let got: f64 = phase_of(unix_s * 1000.0);
            assert!(
                ((got - phase + 0.5).rem_euclid(1.0) - 0.5).abs() < 0.03,
                "{got}"
            );
        }
        assert!((0.0..1.0).contains(&phase_of(0.0)));
    }
}
