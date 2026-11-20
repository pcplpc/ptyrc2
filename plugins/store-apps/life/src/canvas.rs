// SPDX-License-Identifier: GPL-3.0-or-later
//! Braille: `2w` by `4h` dots in `w` by `h` cells, a glyph of U+2800
//! a cell. Pure; [`Braille::paint`] writes it into a [`Canvas`].
//!
//! The bit of the dot `(x, y)` of a cell is `DOTS[y][x]`, the
//! glyph `U+2800 + bits`. A dot outside is ignored; read, it is off.
//! [`from_grid`] turns 8 bits row by row (bit `2y + x`) into those of
//! the glyph, for a picture made from rows of bits.

use ptyrc_app::{Canvas, Cell, Color};

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

/// `2w` by `4h` dots in `w` by `h` cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Braille {
    w: u32,
    h: u32,
    bits: Vec<u8>,
}

impl Braille {
    pub fn new(w: u32, h: u32) -> Braille {
        Braille {
            w,
            h,
            bits: vec![0; w as usize * h as usize],
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

    /// Every dot off.
    pub fn clear(&mut self) {
        self.bits.fill(0);
    }

    fn at(&self, cx: i64, cy: i64) -> Option<usize> {
        let inside = (0..i64::from(self.w)).contains(&cx) && (0..i64::from(self.h)).contains(&cy);
        inside.then(|| cy as usize * self.w as usize + cx as usize)
    }

    /// The dot `(x, y)` on or off.
    pub fn set(&mut self, x: i64, y: i64, on: bool) {
        if let Some(i) = self
            .at(x.div_euclid(2), y.div_euclid(4))
            .filter(|_| x >= 0 && y >= 0)
        {
            let bit = DOTS[(y & 3) as usize][(x & 1) as usize];
            if on {
                self.bits[i] |= bit;
            } else {
                self.bits[i] &= !bit;
            }
        }
    }

    pub fn get(&self, x: i64, y: i64) -> bool {
        let i = self
            .at(x.div_euclid(2), y.div_euclid(4))
            .filter(|_| x >= 0 && y >= 0);
        i.is_some_and(|i| self.bits[i] & DOTS[(y & 3) as usize][(x & 1) as usize] != 0)
    }

    /// Every dot from `(x0, y0)` to `(x1, y1)`.
    pub fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let n = ((x1 - x0).abs().max((y1 - y0).abs()).round_ties_even() as i64).max(1);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let x = (x0 + (x1 - x0) * t).round_ties_even() as i64;
            let y = (y0 + (y1 - y0) * t).round_ties_even() as i64;
            self.set(x, y, true);
        }
    }

    /// The bits of the cell `(cx, cy)`; 0 outside.
    pub fn cell(&self, cx: u32, cy: u32) -> u8 {
        self.at(i64::from(cx), i64::from(cy))
            .map_or(0, |i| self.bits[i])
    }

    /// The bits of the cell `(cx, cy)`, all at once.
    pub fn set_cell(&mut self, cx: u32, cy: u32, bits: u8) {
        if let Some(i) = self.at(i64::from(cx), i64::from(cy)) {
            self.bits[i] = bits;
        }
    }

    /// The glyph of the cell `(cx, cy)`; `None` when no dot of it is on.
    pub fn glyph(&self, cx: u32, cy: u32) -> Option<char> {
        glyph(self.cell(cx, cy))
    }

    /// The cells that have a dot on, row by row.
    pub fn cells(&self) -> Vec<(u32, u32)> {
        let w = self.w.max(1) as usize;
        let on = self.bits.iter().enumerate().filter(|(_, b)| **b != 0);
        on.map(|(i, _)| ((i % w) as u32, (i / w) as u32)).collect()
    }

    /// A string a row, a space where a cell is empty.
    pub fn rows(&self) -> Vec<String> {
        let row = |r: &[u8]| r.iter().map(|&b| glyph(b).unwrap_or(' ')).collect();
        self.bits.chunks(self.w.max(1) as usize).map(row).collect()
    }

    /// Writes every cell into `canvas` from `(x, y)`: a cell with a dot
    /// in the colour `fg(cx, cy)`, an empty one nothing drawn.
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
        dots.set(0, 0, false);
        assert_eq!(dots.glyph(0, 0), Some('⢀'));
        let mut line = Braille::new(2, 1);
        line.line(0.0, 0.0, 3.0, 3.0);
        assert_eq!(line.rows(), ["⠑⢄"]);
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
    fn painted_into_a_canvas() {
        let mut dots = Braille::new(2, 1);
        dots.set_cell(1, 0, from_grid(0b11));
        let mut canvas = Canvas::new(3, 1).unwrap();
        dots.paint(&mut canvas, 1, 0, |_, _| Color::Palette(2));
        assert_eq!(canvas.get(0, 0), Cell::default());
        assert_eq!(canvas.get(1, 0), Cell::default());
        assert_eq!(canvas.get(2, 0).ch, Some('⠉'));
        assert_eq!(canvas.get(2, 0).fg, Color::Palette(2));
    }
}
