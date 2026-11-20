// SPDX-License-Identifier: GPL-3.0-or-later
//! The world: `w` by `h` cells on a torus, in bits. A row is `words`
//! `u64`s, the cell `x` bit `x % 64` of word `x / 64`; the bits past
//! `w` are 0. A plane is `h` rows, flat.
//!
//! `ages[k]`: the cells alive for more than `k` generations; `ages[0]`
//! is the world. `gone[k]`: the cells dead for `k + 1` generations,
//! not born again since; `gone[0]` those of the last step.
//!
//! A [`Mask`] is a plane: the cells where something is allowed.

use crate::patterns::Art;
use ptyrc_app::dice::Dice;
use std::collections::VecDeque;

/// A population that repeats at this distance is still.
pub const STILL: usize = 12;
/// Generations still before the world is renewed.
pub const QUIET: u32 = 48;
/// Cells a side of a block of the map of room.
pub const BLOCK: usize = 4;
/// Cells a glyph loses at once to throw a particle.
pub const BLAST: u32 = 5;

/// A plane of `h` rows of `words` words.
pub type Mask = Vec<u64>;

/// Words of a row of `w` cells.
pub fn words_of(w: usize) -> usize {
    w.div_ceil(64).max(1)
}

/// The mask of the last word of a row of `w` cells.
fn last_word(w: usize) -> u64 {
    match w % 64 {
        0 if w > 0 => u64::MAX,
        0 => 0,
        r => (1 << r) - 1,
    }
}

/// A row with bit `x` that of `x - 1` of `src`, around.
fn rot_up(src: &[u64], dst: &mut [u64], w: usize) {
    let n = src.len();
    for i in (0..n).rev() {
        dst[i] = src[i] << 1 | if i > 0 { src[i - 1] >> 63 } else { 0 };
    }
    dst[0] |= src[(w - 1) / 64] >> ((w - 1) % 64) & 1;
    dst[n - 1] &= last_word(w);
}

/// A row with bit `x` that of `x + 1` of `src`, around.
fn rot_down(src: &[u64], dst: &mut [u64], w: usize) {
    let n = src.len();
    for i in 0..n {
        dst[i] = src[i] >> 1 | if i + 1 < n { src[i + 1] << 63 } else { 0 };
    }
    dst[(w - 1) / 64] |= (src[0] & 1) << ((w - 1) % 64);
}

fn ones(row: &[u64]) -> u32 {
    row.iter().map(|w| w.count_ones()).sum()
}

#[derive(Clone, Debug)]
pub struct World {
    w: usize,
    h: usize,
    words: usize,
    pub ages: Vec<Mask>,
    pub gone: Vec<Mask>,
    pub generation: u64,
}

impl World {
    /// `ages`, `gone`: levels kept, 1 at least.
    pub fn new(w: usize, h: usize, ages: usize, gone: usize) -> World {
        let words = words_of(w);
        let plane = || vec![0; words * h];
        World {
            w,
            h,
            words,
            ages: (0..ages.max(1)).map(|_| plane()).collect(),
            gone: (0..gone.max(1)).map(|_| plane()).collect(),
            generation: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    /// Words of a row.
    pub fn words(&self) -> usize {
        self.words
    }

    /// The row `y` of `plane`.
    pub fn row<'a>(&self, plane: &'a [u64], y: usize) -> &'a [u64] {
        &plane[y * self.words..(y + 1) * self.words]
    }

    pub fn alive(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h && self.ages[0][y * self.words + x / 64] >> (x % 64) & 1 != 0
    }

    /// The cells that died in the last step.
    pub fn died(&self) -> &Mask {
        &self.gone[0]
    }

    pub fn population(&self) -> u32 {
        ones(&self.ages[0])
    }

    /// The cells alive, row by row.
    pub fn cells(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for y in 0..self.h {
            for x in 0..self.w {
                if self.alive(x, y) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// A cell in four alive, the rest forgotten.
    pub fn soup(&mut self, dice: &mut impl Dice) {
        for plane in self.ages[1..].iter_mut().chain(self.gone.iter_mut()) {
            plane.fill(0);
        }
        let last = last_word(self.w);
        for (i, word) in self.ages[0].iter_mut().enumerate() {
            let mask = if i % self.words == self.words - 1 {
                last
            } else {
                u64::MAX
            };
            *word = dice.next_u64() & dice.next_u64() & mask;
        }
    }

    fn bit(&self, x: i64, y: i64) -> (usize, u64) {
        let (x, y) = (
            x.rem_euclid(self.w as i64) as usize,
            y.rem_euclid(self.h as i64) as usize,
        );
        (y * self.words + x / 64, 1 << (x % 64))
    }

    /// Writes a pattern, its top-left cell at `(x, y)`; it wraps.
    pub fn put(&mut self, art: &Art, x: i64, y: i64) {
        if self.w == 0 || self.h == 0 {
            return;
        }
        for (dy, line) in art.iter().enumerate() {
            for (dx, &on) in line.iter().enumerate() {
                let (i, bit) = self.bit(x + dx as i64, y + dy as i64);
                for plane in self.ages[1..].iter_mut().chain(self.gone.iter_mut()) {
                    plane[i] &= !bit;
                }
                if on {
                    self.ages[0][i] |= bit;
                } else {
                    self.ages[0][i] &= !bit;
                }
            }
        }
    }

    /// The mask of `n` cells of a row from `x`; it wraps.
    pub fn span(&self, x: i64, n: usize) -> Vec<u64> {
        let mut out = vec![0; self.words];
        if self.w == 0 {
            return out;
        }
        let x = x.rem_euclid(self.w as i64) as usize;
        for k in 0..n.min(self.w) {
            let c = (x + k) % self.w;
            out[c / 64] |= 1 << (c % 64);
        }
        out
    }

    /// Cells alive in the box; it wraps.
    pub fn count(&self, x: i64, y: i64, w: usize, h: usize) -> u32 {
        if self.h == 0 {
            return 0;
        }
        let mask = self.span(x, w);
        let row = |dy: usize| (y + dy as i64).rem_euclid(self.h as i64) as usize;
        let alive = |dy| {
            self.row(&self.ages[0], row(dy))
                .iter()
                .zip(&mask)
                .map(|(a, m)| (a & m).count_ones())
        };
        (0..h).map(|dy| alive(dy).sum::<u32>()).sum()
    }

    /// No cell alive in the box; it wraps.
    pub fn empty(&self, x: i64, y: i64, w: usize, h: usize) -> bool {
        w <= self.w && h <= self.h && self.count(x, y, w, h) == 0
    }

    /// The whole box is open in `open`; it wraps. `None`: all open.
    pub fn free(&self, open: Option<&[u64]>, x: i64, y: i64, w: usize, h: usize) -> bool {
        let Some(open) = open else { return true };
        let mask = self.span(x, w);
        (0..h).all(|dy| {
            let r = (y + dy as i64).rem_euclid(self.h as i64) as usize;
            self.row(open, r)
                .iter()
                .zip(&mask)
                .all(|(o, m)| o & m == *m)
        })
    }

    /// Kills the cells within `r` of `(x, y)`: they died now. It does
    /// not wrap. How many.
    pub fn wipe(&mut self, x: i64, y: i64, r: i64) -> u32 {
        let mut n = 0;
        for row in (y - r).max(0)..(y + r + 1).min(self.h as i64) {
            let half = ((r * r - (row - y) * (row - y)) as f64).sqrt() as i64;
            let (lo, hi) = ((x - half).max(0), (x + half).min(self.w as i64 - 1));
            if lo > hi {
                continue;
            }
            let mask = self.span(lo, (hi - lo + 1) as usize);
            let at = row as usize * self.words;
            for (k, m) in mask.iter().enumerate() {
                let dead = self.ages[0][at + k] & m;
                if dead == 0 {
                    continue;
                }
                n += dead.count_ones();
                for plane in self.ages.iter_mut().chain(self.gone[1..].iter_mut()) {
                    plane[at + k] &= !dead;
                }
                self.gone[0][at + k] |= dead;
            }
        }
        n
    }

    /// Cut, or grown with empty cells.
    pub fn resize(&mut self, w: usize, h: usize) {
        let words = words_of(w);
        let (last, keep) = (last_word(w), self.words.min(words));
        for plane in self.ages.iter_mut().chain(self.gone.iter_mut()) {
            let mut out = vec![0; words * h];
            for y in 0..self.h.min(h) {
                out[y * words..y * words + keep]
                    .copy_from_slice(&plane[y * self.words..y * self.words + keep]);
                out[y * words + words - 1] &= last;
            }
            *plane = out;
        }
        (self.w, self.h, self.words) = (w, h, words);
    }

    /// One generation. `open`: where life is allowed.
    pub fn step(&mut self, open: Option<&[u64]>) {
        let (w, h, n) = (self.w, self.h, self.words);
        if w == 0 || h == 0 {
            return;
        }
        // Per row, the count of the cell and its two neighbours across,
        // two bits (`three`), and of the neighbours alone (`two`).
        let mut three = (vec![0u64; n * h], vec![0u64; n * h]);
        let mut two = (vec![0u64; n * h], vec![0u64; n * h]);
        let (mut left, mut right) = (vec![0u64; n], vec![0u64; n]);
        let rows = &self.ages[0];
        for y in 0..h {
            let row = &rows[y * n..(y + 1) * n];
            rot_up(row, &mut left, w);
            rot_down(row, &mut right, w);
            for i in 0..n {
                let (x, a, r) = (left[i] ^ right[i], left[i] & right[i], row[i]);
                (two.0[y * n + i], two.1[y * n + i]) = (x, a);
                (three.0[y * n + i], three.1[y * n + i]) = (x ^ r, a | x & r);
            }
        }
        let mut new = vec![0u64; n * h];
        for y in 0..h {
            let (up, down) = (((y + h - 1) % h) * n, ((y + 1) % h) * n);
            for i in 0..n {
                let (u0, u1) = (three.0[up + i], three.1[up + i]);
                let (d0, d1) = (three.0[down + i], three.1[down + i]);
                let (m0, m1) = (two.0[y * n + i], two.1[y * n + i]);
                // Neighbours: `ones` + 2 * (u1 + d1 + m1 + carry).
                let ones = u0 ^ d0 ^ m0;
                let carry = u0 & d0 | m0 & (u0 ^ d0);
                // Exactly one of the four: 2 or 3 neighbours.
                let one = (u1 ^ d1 ^ m1 ^ carry) & !(u1 & d1 | m1 & carry);
                let mut cell = one & (ones | rows[y * n + i]);
                if let Some(open) = open {
                    cell &= open[y * n + i];
                }
                new[y * n + i] = cell;
            }
        }
        for k in (1..self.gone.len()).rev() {
            let (older, newer) = self.gone.split_at_mut(k);
            for (g, (p, c)) in newer[0].iter_mut().zip(older[k - 1].iter().zip(&new)) {
                *g = p & !c;
            }
        }
        for (g, (r, c)) in self.gone[0].iter_mut().zip(self.ages[0].iter().zip(&new)) {
            *g = r & !c;
        }
        for k in (1..self.ages.len()).rev() {
            let (younger, older) = self.ages.split_at_mut(k);
            for (a, (p, c)) in older[0].iter_mut().zip(younger[k - 1].iter().zip(&new)) {
                *a = p & c;
            }
        }
        self.ages[0] = new;
        self.generation += 1;
    }

    /// The 2 cells from `2 * cx` of the row `y` of `plane`, low bit first.
    pub fn pair(&self, plane: &[u64], cx: usize, y: usize) -> u8 {
        let x = 2 * cx;
        (plane[y * self.words + x / 64] >> (x % 64) & 3) as u8
    }

    /// The 2 by 4 cells of the terminal cell `(cx, cy)` in `plane`: bit
    /// `2 * row + column`.
    pub fn quad(&self, plane: &[u64], cx: usize, cy: usize) -> u8 {
        (0..4).fold(0, |g, r| g | self.pair(plane, cx, 4 * cy + r) << (2 * r))
    }
}

/// Tells a world gone still: its population repeated at a distance of
/// `STILL` generations, which holds every period up to 4, 6 and 12 and
/// a ship crossing, for `QUIET` generations; or none is left.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    past: VecDeque<u32>,
    quiet: u32,
}

impl Watch {
    pub fn reset(&mut self) {
        self.past.clear();
        self.quiet = 0;
    }

    pub fn still(&mut self, population: u32) -> bool {
        let same = self.past.len() == STILL && self.past[0] == population;
        self.quiet = if same { self.quiet + 1 } else { 0 };
        if self.past.len() == STILL {
            self.past.pop_front();
        }
        self.past.push_back(population);
        self.quiet >= QUIET || population == 0
    }
}

/// The open cells of a world of `2w` by `4h` under a view of `w` by
/// `h` terminal cells holding `rows` (a shape, or text): closed under
/// each glyph and the cell on each side of it, so that the words stay
/// apart. A wide glyph takes two cells.
pub fn wall(rows: &[String], w: usize, h: usize) -> Mask {
    let ww = 2 * w;
    let words = words_of(ww);
    let mut closed = vec![false; w * h];
    for (y, row) in rows.iter().enumerate().take(h) {
        let mut x = 0;
        for ch in row.chars() {
            let wide = ptyrc_app::width(ch) == 2;
            if !ch.is_whitespace() {
                for c in x..x + 1 + usize::from(wide) {
                    for near in c.saturating_sub(1)..(c + 2).min(w) {
                        closed[y * w + near] = true;
                    }
                }
            }
            x += 1 + usize::from(wide);
        }
    }
    let mut out = vec![0u64; words * 4 * h];
    for y in 0..4 * h {
        for cx in 0..w {
            if !closed[(y / 4) * w + cx] {
                let x = 2 * cx;
                out[y * words + x / 64] |= 3 << (x % 64);
            }
        }
    }
    out
}

/// The world in blocks of `BLOCK` cells a side, `[by][bx]`: the
/// distance of each, in blocks, by any of the 8 ways and around the
/// torus, to the nearest block that holds a cell alive or one closed
/// in `open`; 0 for such a block. None such: the longer side of the
/// map everywhere.
pub fn room(world: &World, open: Option<&[u64]>) -> Vec<Vec<u32>> {
    let (w, h) = (world.width(), world.height());
    let (gw, gh) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
    let far = gw.max(gh) as u32;
    let mut out = vec![vec![u32::MAX; gw]; gh];
    let mut queue = VecDeque::new();
    let last = last_word(w);
    let n = world.words();
    for (by, blocks) in out.iter_mut().enumerate() {
        let mut row = vec![0u64; n];
        for y in by * BLOCK..((by + 1) * BLOCK).min(h) {
            for i in 0..n {
                row[i] |= world.ages[0][y * n + i];
                if let Some(open) = open {
                    row[i] |= !open[y * n + i] & if i == n - 1 { last } else { u64::MAX };
                }
            }
        }
        for (bx, d) in blocks.iter_mut().enumerate() {
            let x = bx * BLOCK;
            if row[x / 64] >> (x % 64) & 0xf != 0 {
                *d = 0;
                queue.push_back((bx, by));
            }
        }
    }
    while let Some((bx, by)) = queue.pop_front() {
        let d = out[by][bx] + 1;
        for dy in [gh - 1, 0, 1] {
            for dx in [gw - 1, 0, 1] {
                let (x, y) = ((bx + dx) % gw, (by + dy) % gh);
                if out[y][x] == u32::MAX {
                    out[y][x] = d;
                    queue.push_back((x, y));
                }
            }
        }
    }
    for d in out.iter_mut().flatten().filter(|d| **d == u32::MAX) {
        *d = far;
    }
    out
}

/// The terminal cells, `(x, y)` of the view, that lost `BLAST` cells or
/// more in the last step and kept none.
pub fn blasts(world: &World) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for cy in 0..world.height() / 4 {
        for cx in 0..world.width() / 2 {
            let died = world.quad(world.died(), cx, cy);
            if died.count_ones() >= BLAST && world.quad(&world.ages[0], cx, cy) == 0 {
                out.push((cx, cy));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::{Kind, LIBRARY, decode, heading, named, turn};
    use ptyrc_app::dice::Seeded;
    use std::collections::{BTreeMap, BTreeSet};

    type Cells = BTreeSet<(usize, usize)>;

    fn art(rows: &[&str]) -> Art {
        rows.iter()
            .map(|r| r.chars().map(|c| c == 'O').collect())
            .collect()
    }

    fn cells(world: &World) -> Cells {
        world.cells().into_iter().collect()
    }

    fn plane(world: &World, p: &[u64]) -> Cells {
        let mut out = Cells::new();
        for y in 0..world.height() {
            for x in 0..world.width() {
                if p[y * world.words() + x / 64] >> (x % 64) & 1 != 0 {
                    out.insert((x, y));
                }
            }
        }
        out
    }

    fn naive(cells: &Cells, w: usize, h: usize) -> Cells {
        let mut out = Cells::new();
        for y in 0..h {
            for x in 0..w {
                let mut n = 0;
                for dy in [h - 1, 0, 1] {
                    for dx in [w - 1, 0, 1] {
                        if (dx, dy) != (0, 0) && cells.contains(&((x + dx) % w, (y + dy) % h)) {
                            n += 1;
                        }
                    }
                }
                if n == 3 || n == 2 && cells.contains(&(x, y)) {
                    out.insert((x, y));
                }
            }
        }
        out
    }

    /// Where the cells are, and their shape from there.
    fn shape(world: &World) -> (usize, usize, Cells) {
        let all = cells(world);
        let x0 = all.iter().map(|c| c.0).min().unwrap_or(0);
        let y0 = all.iter().map(|c| c.1).min().unwrap_or(0);
        (x0, y0, all.iter().map(|&(x, y)| (x - x0, y - y0)).collect())
    }

    /// `test_life_step`, across words too.
    #[test]
    fn a_step_is_the_rule() {
        let mut dice = Seeded::new(1);
        for (w, h) in [(3, 5), (16, 8), (37, 23), (64, 3), (130, 7)] {
            let mut world = World::new(w, h, 4, 3);
            world.soup(&mut dice);
            let mut age: BTreeMap<(usize, usize), usize> =
                cells(&world).into_iter().map(|c| (c, 1)).collect();
            let mut dead: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for _ in 0..20 {
                let (was, new) = (cells(&world), naive(&cells(&world), w, h));
                world.step(None);
                assert_eq!(cells(&world), new, "{w}x{h}");
                assert_eq!(world.population() as usize, new.len());
                age = new
                    .iter()
                    .map(|c| (*c, age.get(c).unwrap_or(&0) + 1))
                    .collect();
                dead = dead
                    .into_iter()
                    .filter(|(c, _)| !new.contains(c))
                    .map(|(c, n)| (c, n + 1))
                    .collect();
                dead.extend(was.difference(&new).map(|c| (*c, 1)));
                for (k, p) in world.ages.iter().enumerate() {
                    let want: Cells = age
                        .iter()
                        .filter(|(_, a)| **a > k)
                        .map(|(c, _)| *c)
                        .collect();
                    assert_eq!(plane(&world, p), want);
                }
                for (k, p) in world.gone.iter().enumerate() {
                    let want: Cells = dead
                        .iter()
                        .filter(|(_, n)| **n == k + 1)
                        .map(|(c, _)| *c)
                        .collect();
                    assert_eq!(plane(&world, p), want);
                }
            }
        }
    }

    /// `test_life_patterns`.
    #[test]
    fn a_glider_goes_around_and_not_through_a_wall() {
        let glider = named("glider").unwrap().art();
        let mut world = World::new(12, 12, 1, 1);
        world.put(&glider, 1, 1);
        let start = cells(&world);
        for _ in 0..4 * 12 {
            world.step(None);
        }
        assert_eq!((cells(&world), world.generation), (start, 48));
        let mut world = World::new(12, 12, 1, 1);
        world.put(&glider, 1, 1);
        let open: Mask = (0..12).map(|y| if y < 8 { 0xfff } else { 0 }).collect();
        for _ in 0..60 {
            world.step(Some(&open));
        }
        assert!(world.cells().iter().all(|&(_, y)| y < 8));
    }

    /// `test_life_library`: each pattern is what the table says, run on
    /// a torus too large for it to meet itself.
    #[test]
    fn the_library() {
        for p in &LIBRARY {
            let a = p.art();
            let (pw, ph) = (a[0].len(), a.len());
            let periodic = matches!(p.kind, Kind::Still | Kind::Oscillator | Kind::Ship);
            let gens = if periodic {
                p.period as usize
            } else if p.kind == Kind::Seed {
                100
            } else {
                240
            };
            let pad = 16 + if periodic { gens } else { gens / 2 };
            let mut world = World::new(pw + 2 * pad, ph + 2 * pad, 1, 1);
            world.put(&a, pad as i64, pad as i64);
            let n = a.iter().flatten().filter(|c| **c).count() as u32;
            assert_eq!(world.population(), n);
            let (x0, y0, start) = shape(&world);
            for g in 1..gens {
                world.step(None);
                assert!(world.population() > 0 || g > 100, "{}", p.name);
                if periodic && world.population() == n {
                    assert_ne!(shape(&world).2, start, "{}: a shorter period", p.name);
                } else if !periodic && g == 8 {
                    assert_ne!(shape(&world).2, start, "{}", p.name);
                }
            }
            world.step(None);
            if periodic {
                let (x, y, now) = shape(&world);
                assert_eq!(now, start, "{}", p.name);
                assert_eq!(
                    (x as i32 - x0 as i32, y as i32 - y0 as i32),
                    p.step,
                    "{}",
                    p.name
                );
                assert_eq!(p.step != (0, 0), p.kind == Kind::Ship, "{}", p.name);
                assert_eq!(p.period == 1, p.kind == Kind::Still, "{}", p.name);
            } else if p.kind == Kind::Filler {
                assert!(world.population() > 20 * n, "{}", p.name);
            } else if p.kind != Kind::Seed {
                assert!(world.population() > n + 4, "{}", p.name);
                assert_eq!((p.period, p.step), (0, (0, 0)), "{}", p.name);
            }
        }
    }

    /// `test_life_turn`.
    #[test]
    fn a_ship_turned_goes_its_heading() {
        for name in ["glider", "lwss", "weekender", "big-glider"] {
            let p = named(name).unwrap();
            let mut seen = BTreeSet::new();
            for k in 0..8 {
                let a = turn(&p.art(), k);
                seen.insert(a.clone());
                let mut world = World::new(60, 60, 1, 1);
                world.put(&a, 25, 25);
                let (x0, y0, start) = shape(&world);
                for _ in 0..p.period {
                    world.step(None);
                }
                let (x, y, now) = shape(&world);
                assert_eq!(now, start);
                assert_eq!(
                    (x as i32 - x0 as i32, y as i32 - y0 as i32),
                    heading(p.step, k),
                    "{name} {k}"
                );
            }
            assert_eq!(
                seen.len(),
                if matches!(name, "weekender" | "big-glider") {
                    4
                } else {
                    8
                }
            );
        }
    }

    /// `test_life_room`.
    #[test]
    fn room_is_the_distance_to_life() {
        let mut world = World::new(40, 24, 1, 1); // 10 by 6 blocks
        assert_eq!(room(&world, None), vec![vec![10; 10]; 6]);
        world.put(&art(&["O"]), 5, 9); // the block (1, 2)
        let far = room(&world, None);
        assert_eq!((far[2][1], far[2][2], far[1][0], far[3][2]), (0, 1, 1, 1));
        assert_eq!((far[2][6], far[2][7]), (5, 4)); // around the torus
        assert_eq!((far[5][1], far[0][1]), (3, 2));
        assert_eq!(far.iter().flatten().max(), Some(&5));
        // A closed cell is as a cell alive.
        let mut open = vec![u64::MAX >> 24; 24];
        open[20] &= !(1 << 38);
        let far = room(&world, Some(&open));
        assert_eq!((far[5][9], far[5][0], far[2][1]), (0, 1, 0));
        // A width that is no multiple of the block.
        let mut world = World::new(10, 8, 1, 1);
        world.put(&art(&["O"]), 9, 0);
        assert_eq!(room(&world, None), [[1, 1, 0], [1, 1, 1]]);
    }

    /// `test_life_resize_and_room`, the world's part.
    #[test]
    fn resized_and_boxes() {
        let mut world = World::new(8, 8, 2, 1);
        world.put(&art(&["OO", "OO"]), 6, 6);
        assert!(world.empty(0, 0, 6, 6) && !world.empty(5, 5, 2, 2));
        assert!(!world.empty(7, 0, 2, 8)); // it wraps
        world.resize(7, 12);
        assert_eq!(world.cells(), [(6, 6), (6, 7)]);
        assert_eq!(world.ages[1].len(), 12);
        let mut world = World::new(70, 3, 1, 1);
        world.put(&art(&["OOO"]), 63, 0);
        world.resize(200, 3);
        assert_eq!(world.cells(), [(63, 0), (64, 0), (65, 0)]);
        world.resize(64, 3);
        assert_eq!(world.cells(), [(63, 0)]);
        world.step(None);
        assert_eq!(world.population(), 0);
    }

    /// `test_life_watch`.
    #[test]
    fn still_when_the_population_repeats() {
        let mut world = World::new(40, 40, 1, 1);
        world.put(&named("pulsar").unwrap().art(), 3, 3);
        world.put(&art(&["OOO"]), 30, 30);
        world.put(&named("glider").unwrap().art(), 22, 2);
        let mut watch = Watch::default();
        let stills: Vec<bool> = (0..300)
            .map(|_| {
                world.step(None);
                watch.still(world.population())
            })
            .collect();
        assert!(stills[STILL + QUIET as usize] && !stills[..QUIET as usize].contains(&true));
        let mut world = World::new(64, 64, 1, 1);
        world.put(&named("r-pentomino").unwrap().art(), 30, 30);
        watch.reset();
        assert!((0..150).all(|_| {
            world.step(None);
            !watch.still(world.population())
        }));
        assert!(Watch::default().still(0));
    }

    /// `test_life_wall_and_blasts`, the wall and the blasts.
    #[test]
    fn walls_and_blasts() {
        let rows = |r: &[&str]| r.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            wall(&rows(&["", "  ab"]), 6, 2),
            [[0xfff; 4], [0b1100_0000_0011; 4]].concat()
        );
        assert_eq!(wall(&rows(&["a世"]), 5, 1)[0], 0b11_0000_0000);
        assert_eq!(wall(&rows(&["a  b"]), 4, 1)[0], 0);
        assert_eq!(wall(&rows(&["#"]), 40, 1)[0], u64::MAX << 4);
        assert_eq!(wall(&rows(&["#"]), 40, 1)[1], 0xffff);
        let mut world = World::new(8, 8, 1, 1);
        world.put(&art(&["OO", "OO", "OO", "OO"]), 2, 4);
        world.put(&art(&["OO", "OO"]), 6, 0);
        world.step(Some(&[[0xff; 4], [0; 4]].concat()));
        assert_eq!(blasts(&world), [(1, 1)]);
    }

    /// `test_life_wipe`.
    #[test]
    fn wiped_around_a_cell() {
        let mut world = World::new(40, 30, 3, 2);
        for p in &mut world.ages {
            p.fill((1 << 40) - 1);
        }
        world.gone[1].fill((1 << 40) - 1);
        let near: Cells = (0..40)
            .flat_map(|x| (0..30).map(move |y| (x, y)))
            .filter(|&(x, y)| (x as i64 - 20).pow(2) + (y as i64 - 15).pow(2) <= 36)
            .collect();
        assert_eq!((world.wipe(20, 15, 6), near.len()), (113, 113));
        let full: Cells = (0..40).flat_map(|x| (0..30).map(move |y| (x, y))).collect();
        assert_eq!(cells(&world), &full - &near);
        for p in &world.ages[1..] {
            assert_eq!(plane(&world, p), cells(&world));
        }
        // They died now: the first level of the trail, no other.
        assert_eq!(ones(world.died()), 113);
        assert!(
            world
                .died()
                .iter()
                .zip(&world.gone[1])
                .all(|(d, g)| d & g == 0)
        );
        assert_eq!(world.wipe(20, 15, 6), 0);
        // At an edge: cut, the other side is kept.
        let corner = (0..7)
            .flat_map(|x| (0..7).map(move |y| (x, y)))
            .filter(|(x, y)| x * x + y * y <= 36)
            .count();
        assert_eq!(world.wipe(0, 0, 6) as usize, corner);
        assert_eq!(world.ages[0][29], world.ages[0][1] | 0x7f);
        assert_eq!(world.ages[0][0] >> 39, 1);
        assert_eq!(world.wipe(39, 29, 2), 6);
    }

    #[test]
    fn decode_puts_what_it_reads() {
        let mut world = World::new(5, 5, 1, 1);
        world.put(&decode("bo$2bo$3o!"), 4, 4);
        assert_eq!(world.cells(), [(1, 0), (0, 1), (1, 1), (4, 1), (0, 4)]);
    }
}
