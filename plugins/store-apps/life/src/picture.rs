// SPDX-License-Identifier: GPL-3.0-or-later
//! The world as Braille, a glyph for 2 by 4 cells, one colour a glyph.

use crate::world::World;
use ptyrc_app::canvas::{Braille, from_grid};

/// The colour of a glyph by the age of its youngest cell, in generations.
pub const AGES: [u8; 8] = [231, 228, 156, 114, 79, 73, 67, 61];
/// The colour of the cells that died, by generations since.
pub const TRAIL: [u8; 4] = [243, 240, 238, 236];
/// The colour of a glyph without `colour`.
pub const GREEN: u8 = 2;

/// A glyph and its colour (of the 256) a cell; 0 where nothing is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub dots: Braille,
    pub ink: Vec<u8>,
}

impl Picture {
    pub fn new(w: u32, h: u32) -> Picture {
        Picture {
            dots: Braille::new(w, h),
            ink: vec![0; w as usize * h as usize],
        }
    }

    /// The colour of the cell `(cx, cy)`; 0: none.
    pub fn ink(&self, cx: u32, cy: u32) -> u8 {
        self.ink[(cy * self.dots.width() + cx) as usize]
    }
}

/// Draws `world` into `out` (of its size, in terminal cells). `colour`:
/// by the age of the youngest cell of the glyph, as far as the world
/// keeps ages. `trail`: where no cell lives, those that died, in the
/// colour of the last. `shown`: the cells to draw, a mask a row of the
/// world; a glyph is shown where its first row is.
pub fn picture(world: &World, colour: bool, trail: bool, shown: Option<&[u64]>, out: &mut Picture) {
    let (w, h) = (world.width() / 2, world.height() / 4);
    out.dots.clear();
    out.ink.fill(0);
    let ghosts = if trail { &world.gone[..] } else { &[] };
    let alive = &world.ages[0];
    for cy in 0..h {
        for cx in 0..w {
            // The two columns of the glyph that have something to show.
            let mut seen = (0..4).fold(0, |s, r| s | world.pair(alive, cx, 4 * cy + r));
            for g in ghosts {
                seen |= (0..4).fold(0, |s, r| s | world.pair(g, cx, 4 * cy + r));
            }
            if let Some(shown) = shown {
                seen &= world.pair(shown, cx, 4 * cy);
            }
            if seen == 0 {
                continue;
            }
            let mut bits = world.quad(alive, cx, cy);
            let mut ink = GREEN;
            if bits != 0 && colour {
                // The first level that lacks a cell of the glyph.
                let age = world.ages[1..]
                    .iter()
                    .take_while(|older| world.quad(older, cx, cy) == bits)
                    .count();
                ink = AGES[age.min(AGES.len() - 1)];
            } else if bits == 0 {
                bits = ghosts.iter().fold(0, |b, g| b | world.quad(g, cx, cy));
                if let Some(k) = ghosts.iter().position(|g| world.quad(g, cx, cy) != 0) {
                    ink = TRAIL[k]; // of the last to die
                }
            }
            out.dots.set_cell(cx as u32, cy as u32, from_grid(bits));
            out.ink[cy * w + cx] = ink;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::Art;

    fn art(rows: &[&str]) -> Art {
        rows.iter()
            .map(|r| r.chars().map(|c| c == 'O').collect())
            .collect()
    }

    /// Each row as `(text, colour)` runs, as the twin's `picture` gives them.
    fn runs(
        world: &World,
        colour: bool,
        trail: bool,
        shown: Option<&[u64]>,
    ) -> Vec<Vec<(String, u8)>> {
        let mut p = Picture::new(world.width() as u32 / 2, world.height() as u32 / 4);
        picture(world, colour, trail, shown, &mut p);
        let mut out = Vec::new();
        for (cy, row) in p.dots.rows().iter().enumerate() {
            let mut line: Vec<(String, u8)> = Vec::new();
            let mut pad = String::new();
            for (cx, ch) in row.chars().enumerate() {
                let ink = p.ink(cx as u32, cy as u32);
                if ink == 0 {
                    pad.push(' ');
                } else if line.last().is_some_and(|l| l.1 == ink) {
                    let last = line.last_mut().unwrap();
                    last.0 += &pad;
                    last.0.push(ch);
                    pad.clear();
                } else {
                    line.push((std::mem::take(&mut pad) + &ch.to_string(), ink));
                }
            }
            out.push(line);
        }
        out
    }

    fn run(t: &str, ink: u8) -> Vec<(String, u8)> {
        vec![(t.to_string(), ink)]
    }

    /// `test_life_picture`.
    #[test]
    fn glyphs_and_their_colours() {
        let mut world = World::new(8, 8, AGES.len(), TRAIL.len());
        world.put(&art(&["OO", "OO"]), 0, 0); // a block: still
        world.put(&art(&["O"]), 5, 5); // dies alone
        assert_eq!(
            runs(&world, false, false, None),
            [run("⠛", GREEN), run("  ⠐", GREEN)]
        );
        world.step(None);
        assert_eq!(runs(&world, false, false, None), [run("⠛", GREEN), vec![]]);
        assert_eq!(
            runs(&world, true, true, None),
            [run("⠛", AGES[1]), run("  ⠐", TRAIL[0])]
        );
        for _ in 0..3 {
            world.step(None);
        }
        assert_eq!(runs(&world, true, true, None)[1], run("  ⠐", TRAIL[3]));
        for _ in 0..10 {
            world.step(None);
        }
        assert_eq!(runs(&world, true, true, None), [run("⠛", AGES[7]), vec![]]);
    }

    /// `test_life_wall_and_blasts`, the picture's part.
    #[test]
    fn only_what_is_shown() {
        let mut world = World::new(8, 8, 1, 1);
        world.put(&art(&["OO", "OO", "OO", "OO"]), 2, 4);
        world.put(&art(&["OO", "OO"]), 6, 0);
        assert_eq!(
            runs(&world, false, false, Some(&[0xf0; 8])),
            [run("   ⠛", GREEN), vec![]]
        );
    }
}
