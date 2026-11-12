// SPDX-License-Identifier: GPL-3.0-or-later
//! Falling sand: a native effect. A thin stream falls from one spout,
//! piles on the app's text and on the floor, and avalanches.
//!
//! Grains are braille dots: a cell holds 2 x 4 of them.
//!
//! Effect `sand`, params:
//!
//! - `rate`: grains per second (default 60).
//! - `cursor`: the spout is over the cursor's column (default false).
//! - `colors`: strata; the stream goes from one to the next and around
//!   (default: sand tones).
//! - `color_ms`: time from one colour to the next (default 20000).
//! - `walls`: rows of text standing for the app's, until the app is seen.
//! - `duration_ms`: life of the effect. Absent: until removed.
//! - `seed`.
//!
//! Walls: every cell under the canvas that shows foreground, read at
//! each render. A cell that gets text loses its grains; one that loses
//! its text lets them fall. A resize keeps the grains, top-left fixed;
//! those outside are lost.
//!
//! Scroll: grains are fixed on the cells of the frame. When the app
//! scrolls, the pile moves with the text; what leaves the canvas, by
//! any side, is put away (`ATTIC` rows of `WIDE` cells at most, the
//! farthest forgotten first) and comes back with its text. A floor that
//! left lets the pile fall. Rows told moved (`shift`: a splice, an
//! island merge) take their grains along.
//!
//! Out of sight: the world is the canvas and up to `MARGIN` rows above
//! and below it that the frame remembers. Their text is a wall as on
//! screen: grains fall past the bottom of the view onto the text under
//! it, and the pile over the view falls in when its floor goes. The
//! first row not remembered is a floor. Further rows are frozen.
//!
//! Heat: a column with cells that changed lately is a worse spout.
//!
//! Spout: the topmost free cell of a column. Chosen near the middle,
//! over text, away from the ends of the run it lands on; kept while it
//! stays so; moved when the pile reaches it. With `cursor`: the cursor's
//! column, unless the cursor is outside the canvas or on column 0.
//!
//! Step (12 ms), a grain, bottom row first:
//!
//! 1. falls one dot when the dot below is free;
//! 2. else, awake, slides down a diagonal, or half the time rolls two
//!    dots aside and one down; on a one-dot drop it may stop and sleep;
//! 3. else, asleep, topples only off a drop of three dots.
//!
//! A grain that moves wakes the three above it: a toppling grain starts
//! an avalanche.
//!
//! A heavy stream (more than `FLOW` grains a step) has a reach `r`, the
//! grains a step over `FLOW`, `REACH` at most. A held grain then slides
//! up to `r` dots a step, stops `r` times less often, gives way sideways
//! through `r - 1` grains, and wakes those it lands on: the slopes carry
//! what the spout pours, and no column stands under it.

use ptyrc_core::frames::HeatClass;
use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::{
    Canvas, ColorSpec, Effect, EffectFactory, Looks, Rng, SpriteSpec, StyleSpec, Under,
};
use ptyrc_vt::Color;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

/// One simulation step: a falling grain moves one dot.
const STEP_MS: u64 = 12;
/// Time between two renders.
const FRAME_MS: u64 = 48;
/// Steps run by one render at most: a late render does not freeze.
const CATCH_UP: u64 = 16;
/// Canvas cells at most.
const MAX_CELLS: usize = 1 << 18;
/// Dots of a cell.
const DW: usize = 2;
const DH: usize = 4;
/// Shades between two colours, at most.
const SHADES: usize = 16;
/// A sleeping grain topples off a drop of this many dots.
const CLIFF: usize = 3;
/// Chance, of 256, that a grain sliding off a one-dot drop stops.
const FRICTION: u64 = 96;
/// Steps a spout stays choked before it moves.
const CHOKE: u32 = 60;
/// Grains a step that slides of one dot carry away from under the spout.
const FLOW: f64 = 1.2;
/// Dots a grain slides in one step, at most.
const REACH: usize = 8;
/// Rows of cells kept out of the canvas, at most.
const ATTIC: usize = 1024;
/// Cells of a row kept, at most.
const WIDE: usize = 1024;
/// Remembered rows simulated on each side of the canvas, at most.
const MARGIN: usize = 16;

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
struct Params {
    walls: Vec<String>,
    rate: f64,
    cursor: bool,
    colors: Vec<ColorSpec>,
    color_ms: u64,
    duration_ms: Option<u64>,
    seed: u64,
}

impl Default for Params {
    fn default() -> Params {
        let rgb = |v: u32| ColorSpec(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
        Params {
            walls: Vec::new(),
            rate: 60.0,
            cursor: false,
            colors: [
                0xe8c98a, 0xd9a65a, 0xc47f3c, 0xa65a32, 0xc9825e, 0xe6b89a, 0xf0dcb4, 0xcdb27a,
                0x9c8a5c, 0xd6b25e,
            ]
            .into_iter()
            .map(rgb)
            .collect(),
            color_ms: 20_000,
            duration_ms: None,
            seed: 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dot {
    Free,
    Wall,
    /// Shade, and whether it is awake.
    Grain(u8, bool),
}

/// RGB of a colour of the 256 palette above the 16 of the terminal.
fn rgb(color: Color) -> Option<[u8; 3]> {
    match color {
        Color::Rgb(r, g, b) => Some([r, g, b]),
        Color::Indexed(n @ 16..=231) => {
            let n = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            Some([level(n / 36), level(n / 6 % 6), level(n % 6)])
        }
        Color::Indexed(n @ 232..) => Some([8 + 10 * (n - 232); 3]),
        _ => None,
    }
}

/// `colors` as a ring of shades: `per` steps from each to the next.
fn shades(colors: &[ColorSpec]) -> (Vec<Color>, usize) {
    let per = (250 / colors.len()).clamp(1, SHADES);
    let mut out = Vec::with_capacity(colors.len() * per);
    for (i, from) in colors.iter().enumerate() {
        let to = colors[(i + 1) % colors.len()].0;
        for k in 0..per {
            out.push(match (rgb(from.0), rgb(to)) {
                (Some(a), Some(b)) if k > 0 => {
                    let mix =
                        |j: usize| ((a[j] as usize * (per - k) + b[j] as usize * k) / per) as u8;
                    Color::Rgb(mix(0), mix(1), mix(2))
                }
                _ => from.0,
            });
        }
    }
    (out, per)
}

/// The grains of one frame row out of the world: `w` cells from the
/// frame column `x0`, `DH` rows of dots.
struct Strip {
    x0: i64,
    w: usize,
    dots: Vec<Dot>,
}

impl Strip {
    fn new(x0: i64, w: usize) -> Strip {
        Strip {
            x0,
            w,
            dots: vec![Dot::Free; w * DW * DH],
        }
    }

    /// Grow to hold the cells `x0..x0 + w`; `WIDE` cells are kept, those
    /// around the new ones.
    fn cover(&mut self, x0: i64, w: usize) {
        let cap = WIDE.max(w) as i64;
        let lo = self.x0.min(x0).max(x0 - (cap - w as i64) / 2);
        let hi = (self.x0 + self.w as i64).max(x0 + w as i64).min(lo + cap);
        if (lo, hi) == (self.x0, self.x0 + self.w as i64) {
            return;
        }
        let mut grown = Strip::new(lo, (hi - lo) as usize);
        let from = lo.max(self.x0);
        let n = (hi.min(self.x0 + self.w as i64) - from).max(0) as usize * DW;
        for dy in 0..DH {
            let src = dy * self.w * DW + (from - self.x0) as usize * DW;
            let dst = dy * grown.w * DW + (from - lo) as usize * DW;
            grown.dots[dst..dst + n].copy_from_slice(&self.dots[src..src + n]);
        }
        *self = grown;
    }

    /// Dot index of the frame column `x`, dot `k`, dot row `dy`.
    fn index(&self, x: i64, k: usize, dy: usize) -> Option<usize> {
        let cx = x - self.x0;
        (cx >= 0 && cx < self.w as i64).then(|| dy * self.w * DW + cx as usize * DW + k)
    }

    fn is_empty(&self) -> bool {
        self.dots.iter().all(|d| *d == Dot::Free)
    }
}

/// Where the stream starts: a cell, and whether text lies under it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Spout {
    x: usize,
    y: usize,
    on_text: bool,
}

/// The pile. Public for tests and for other extensions.
pub struct Sand {
    params: Params,
    rng: Rng,
    shades: Vec<Color>,
    per: usize,
    /// The world, in cells: the canvas, `top` rows above it, the rows
    /// below it.
    w: usize,
    h: usize,
    top: usize,
    /// Rows of the canvas.
    ch: usize,
    /// Frame position of the world cell `(0, 0)`.
    pos: (i64, i64),
    /// `w * DW` by `h * DH`.
    dots: Vec<Dot>,
    /// Remembered rows above and below the canvas, at the last look.
    above: usize,
    below: usize,
    /// Foreground of the app over the world, `seen_w` cells a row, and
    /// whether it changed since the walls were set.
    seen: Vec<bool>,
    seen_w: usize,
    stale: bool,
    /// Per column: cells that changed lately.
    hot: Vec<u16>,
    /// Frame position of the canvas cell `(0, 0)`.
    origin: Option<(i64, i64)>,
    /// Grains out of the world, by frame row.
    attic: BTreeMap<i64, Strip>,
    cursor: Option<(u16, u16)>,
    spout: Option<Spout>,
    choked: u32,
    steps: u64,
    /// Grains owed by the rate, fractional.
    owed: f64,
}

impl Sand {
    fn new(mut params: Params) -> Sand {
        let (shades, per) = shades(&params.colors);
        let walls = std::mem::take(&mut params.walls);
        let mut sand = Sand::bare(params, shades, per);
        sand.seen_w = walls
            .iter()
            .map(|r| Canvas::text_width(r) as usize)
            .max()
            .unwrap_or(0);
        for row in &walls {
            let at = sand.seen.len();
            for ch in row.chars() {
                let width = ptyrc_vt::char_width(ch).max(1) as usize;
                sand.seen.extend(std::iter::repeat_n(ch != ' ', width));
            }
            sand.seen.resize(at + sand.seen_w, false);
        }
        sand
    }

    fn bare(params: Params, shades: Vec<Color>, per: usize) -> Sand {
        Sand {
            rng: Rng::new(params.seed),
            seen_w: 0,
            seen: Vec::new(),
            hot: Vec::new(),
            origin: None,
            attic: BTreeMap::new(),
            params,
            shades,
            per,
            w: 0,
            h: 0,
            top: 0,
            ch: 0,
            pos: (0, 0),
            above: 0,
            below: 0,
            dots: Vec::new(),
            stale: true,
            cursor: None,
            spout: None,
            choked: 0,
            steps: 0,
            owed: 0.0,
        }
    }

    fn at(&self, x: usize, y: usize) -> Dot {
        self.dots[y * self.w * DW + x]
    }

    fn set(&mut self, x: usize, y: usize, dot: Dot) {
        let i = y * self.w * DW + x;
        self.dots[i] = dot;
    }

    fn free(&self, x: usize, y: usize) -> bool {
        self.at(x, y) == Dot::Free
    }

    /// Whether the cell is text.
    fn wall(&self, x: usize, y: usize) -> bool {
        self.at(x * DW, y * DH) == Dot::Wall
    }

    /// Grains in the pile.
    pub fn grains(&self) -> usize {
        self.dots
            .iter()
            .filter(|d| matches!(d, Dot::Grain(..)))
            .count()
    }

    /// Put the grains of the world away.
    fn stash(&mut self) {
        let w = self.w * DW;
        for cy in 0..self.h {
            let rows = &self.dots[cy * DH * w..(cy + 1) * DH * w];
            let y = self.pos.1 + cy as i64;
            if !rows.iter().any(|d| matches!(d, Dot::Grain(..))) && !self.attic.contains_key(&y) {
                continue;
            }
            let strip = self
                .attic
                .entry(y)
                .or_insert_with(|| Strip::new(self.pos.0, self.w));
            strip.cover(self.pos.0, self.w);
            for (i, dot) in rows.iter().enumerate() {
                let x = self.pos.0 + ((i % w) / DW) as i64;
                if let Some(at) = strip.index(x, i % DW, i / w) {
                    strip.dots[at] = if *dot == Dot::Wall { Dot::Free } else { *dot };
                }
            }
            if strip.is_empty() {
                self.attic.remove(&y);
            }
        }
        self.dots.fill(Dot::Free);
    }

    /// Take out the grains put away on the cells of the world.
    fn fetch(&mut self) {
        let w = self.w * DW;
        for cy in 0..self.h {
            let y = self.pos.1 + cy as i64;
            let Some(strip) = self.attic.get_mut(&y) else {
                continue;
            };
            for i in 0..DH * w {
                let x = self.pos.0 + ((i % w) / DW) as i64;
                if let Some(at) = strip.index(x, i % DW, i / w) {
                    self.dots[cy * DH * w + i] = std::mem::replace(&mut strip.dots[at], Dot::Free);
                }
            }
            if strip.is_empty() {
                self.attic.remove(&y);
            }
        }
        let mid = self.pos.1 + self.h as i64 / 2;
        while self.attic.len() > ATTIC {
            let (first, last) = (
                *self.attic.keys().next().unwrap(),
                *self.attic.keys().next_back().unwrap(),
            );
            let far = if mid - first > last - mid {
                first
            } else {
                last
            };
            self.attic.remove(&far);
        }
        self.stale = true;
    }

    /// The world is now `w` by `h` cells at the frame position `pos`:
    /// the grains stay on their cells of the frame. `keep`: those left
    /// outside are put away; otherwise they are lost.
    fn place(&mut self, pos: (i64, i64), w: usize, h: usize, keep: bool) {
        let old = (self.pos, self.w, self.h);
        self.stash();
        if !keep {
            let ((x0, y0), ow, oh) = old;
            for y in y0..y0 + oh as i64 {
                let Some(strip) = self.attic.get_mut(&y) else {
                    continue;
                };
                for (i, dot) in strip.dots.iter_mut().enumerate() {
                    let x = strip.x0 + ((i % (strip.w * DW)) / DW) as i64;
                    let inside =
                        x >= pos.0 && x < pos.0 + w as i64 && y >= pos.1 && y < pos.1 + h as i64;
                    if x >= x0 && x < x0 + ow as i64 && !inside {
                        *dot = Dot::Free;
                    }
                }
                if strip.is_empty() {
                    self.attic.remove(&y);
                }
            }
        }
        (self.pos, self.w, self.h) = (pos, w, h);
        self.dots = vec![Dot::Free; w * DW * h * DH];
        self.fetch();
    }

    /// Set the walls from what was seen: new text eats the grains under
    /// it.
    fn build(&mut self) {
        let (w, h) = (self.w, self.h);
        let mut walls = vec![false; w * h];
        if self.seen_w > 0 {
            for (y, row) in self.seen.chunks(self.seen_w).take(h).enumerate() {
                for (x, ink) in row.iter().take(w).enumerate() {
                    walls[y * w + x] = *ink;
                }
            }
        }
        for (i, wall) in walls.into_iter().enumerate() {
            let (cx, cy) = (i % w, i / w);
            if wall == self.wall(cx, cy) {
                continue;
            }
            let dot = if wall { Dot::Wall } else { Dot::Free };
            for y in cy * DH..(cy + 1) * DH {
                for x in cx * DW..(cx + 1) * DW {
                    self.set(x, y, dot);
                }
            }
        }
        self.stale = false;
    }

    /// The spout of column `x`: its topmost free cell of the canvas.
    fn spout_of(&self, x: usize) -> Option<Spout> {
        let y = (self.top..self.top + self.ch).find(|y| !self.wall(x, *y))?;
        let on_text = (y..self.h).any(|y| self.wall(x, y));
        Some(Spout { x, y, on_text })
    }

    /// Whether a grain fits in the first dot row of the spout.
    fn open(&self, s: &Spout) -> bool {
        (0..DW).any(|k| self.free(s.x * DW + k, s.y * DH))
    }

    /// The best spout: open, near the middle, high, over text, with room
    /// to fall, far from the ends of the run of text it lands on.
    fn pick(&self) -> Option<Spout> {
        let w = self.w as i64;
        (0..self.w)
            .filter_map(|x| self.spout_of(x))
            .filter(|s| self.open(s))
            .min_by_key(|s| {
                let mut cost = (2 * s.x as i64 + 1 - w).abs() + 8 * (s.y - self.top) as i64;
                cost += 6 * self.hot.get(s.x).copied().unwrap_or(0) as i64;
                match (s.y..self.h).find(|y| self.wall(s.x, *y)) {
                    None => cost += 2 * w,
                    Some(ground) => {
                        if ground - s.y < 2 {
                            cost += w;
                        }
                        let run = |dx: i64| {
                            (1..4i64)
                                .take_while(|k| {
                                    let x = s.x as i64 + dx * k;
                                    x >= 0 && x < w && self.wall(x as usize, ground)
                                })
                                .count() as i64
                        };
                        cost -= 2 * run(-1).min(run(1));
                    }
                }
                cost
            })
    }

    /// Keep the spout while it holds; else find one.
    fn aim(&mut self) {
        if self.params.cursor
            && let Some((x, _)) = self.cursor
            && x > 0
            && (x as usize) < self.w
        {
            self.spout = self.spout_of(x as usize);
            return;
        }
        let keep = self
            .spout
            .is_some_and(|s| s.x < self.w && self.spout_of(s.x) == Some(s) && self.open(&s));
        if !keep {
            self.spout = self.pick();
            self.choked = 0;
        }
    }

    /// Free dots under `(x, y)`, `CLIFF` at most.
    fn drop(&self, x: usize, y: usize) -> usize {
        (1..=CLIFF)
            .take_while(|k| y + k < self.h * DH && self.free(x, y + k))
            .count()
    }

    /// Where the grain at `(x, y)`, held from below, goes.
    fn slide(&mut self, x: usize, y: usize, awake: bool, reach: usize) -> Option<(usize, usize)> {
        let w = self.w * DW;
        // The nearest free dot of the row, through `reach - 1` grains at
        // most: under a heavy stream the heap gives way sideways.
        let side = |d: i64| {
            (1..=reach as i64)
                .map(|k| x as i64 + d * k)
                .take_while(|nx| *nx >= 0 && *nx < w as i64)
                .map(|nx| nx as usize)
                .find(|nx| !matches!(self.at(*nx, y), Dot::Grain(..)))
                .filter(|nx| self.free(*nx, y))
        };
        let fall = |nx: Option<usize>| nx.map_or(0, |nx| self.drop(nx, y));
        let (left, right) = (side(-1), side(1));
        let (dl, dr) = (fall(left), fall(right));
        let coin = self.rng.next_u64();
        let need = if awake { 1 } else { CLIFF };
        if dl.max(dr) >= need {
            let go_left = dl > dr || (dl == dr && coin & 1 == 0);
            let (nx, depth) = if go_left { (left, dl) } else { (right, dr) };
            if awake && depth == 1 && (coin >> 8) & 0xff < FRICTION / reach as u64 {
                return None;
            }
            return nx.map(|nx| (nx, y + 1));
        }
        if !awake || coin & 2 == 0 {
            return None;
        }
        // Roll: over one dot of flat ground, then down.
        let roll = |near: Option<usize>, d: i64| {
            let nx = near? as i64 + d;
            (nx >= 0 && nx < w as i64)
                .then_some(nx as usize)
                .filter(|nx| self.free(*nx, y) && self.free(*nx, y + 1))
        };
        let (l, r) = (roll(left, -1), roll(right, 1));
        let nx = match (l, r) {
            (Some(l), Some(r)) => Some(if coin & 4 == 0 { l } else { r }),
            (l, r) => l.or(r),
        };
        nx.map(|nx| (nx, y + 1))
    }

    /// Move a grain; it wakes the three above the place it leaves.
    fn shift(&mut self, from: (usize, usize), to: (usize, usize), shade: u8, knock: bool) {
        let (x, y) = from;
        self.set(x, y, Dot::Free);
        self.set(to.0, to.1, Dot::Grain(shade, true));
        if knock {
            // It shakes what it lands on and beside.
            let w = self.w * DW;
            for ky in to.1..(to.1 + 2).min(self.h * DH) {
                for kx in to.0.saturating_sub(1)..(to.0 + 2).min(w) {
                    if let Dot::Grain(s, false) = self.at(kx, ky) {
                        self.set(kx, ky, Dot::Grain(s, true));
                    }
                }
            }
        }
        if y > 0 {
            for ax in x.saturating_sub(1)..(x + 2).min(self.w * DW) {
                if let Dot::Grain(s, false) = self.at(ax, y - 1) {
                    self.set(ax, y - 1, Dot::Grain(s, true));
                }
            }
        }
    }

    fn step(&mut self) {
        let (w, h) = (self.w * DW, self.h * DH);
        let per_step = self.params.rate * STEP_MS as f64 / 1000.0;
        let reach = (per_step / FLOW).ceil().clamp(1.0, REACH as f64) as usize;
        // Bottom up, so a grain moves once per step; the sweep changes
        // side each step, so neither is favoured.
        let reverse = self.steps & 1 == 1;
        for y in (0..h).rev() {
            for i in 0..w {
                let x = if reverse { w - 1 - i } else { i };
                let Dot::Grain(shade, awake) = self.at(x, y) else {
                    continue;
                };
                if y + 1 < h && self.free(x, y + 1) {
                    self.shift((x, y), (x, y + 1), shade, reach > 1);
                    continue;
                }
                // Held from below: down the slope, further when the
                // stream is heavy, so the slides keep up with it.
                let (mut x, mut y, mut awake, mut moved) = (x, y, awake, false);
                for _ in 0..reach {
                    if y + 1 >= h || self.free(x, y + 1) {
                        break;
                    }
                    let Some((nx, ny)) = self.slide(x, y, awake, reach) else {
                        break;
                    };
                    self.shift((x, y), (nx, ny), shade, reach > 1);
                    (x, y, awake, moved) = (nx, ny, true, true);
                }
                if !moved && awake {
                    self.set(x, y, Dot::Grain(shade, false));
                }
            }
        }
        self.pour();
        self.steps += 1;
    }

    /// Let the grains owed out of the spout.
    fn pour(&mut self) {
        let per_step = self.params.rate * STEP_MS as f64 / 1000.0;
        self.owed += per_step;
        let Some(spout) = self.spout else {
            self.owed = self.owed.fract();
            return;
        };
        let ms = self.steps * STEP_MS;
        let shade = (ms as u128 * self.per as u128 / self.params.color_ms as u128
            % self.shades.len() as u128) as u8;
        // A stream wider than the spout when the rate asks for it.
        let width = (per_step.ceil() as i64).max(DW as i64);
        let y = spout.y * DH;
        let owing = self.owed >= 1.0;
        let mut poured = false;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let start = (self.rng.next_u64() % width as u64) as i64;
            let x = (0..width)
                .map(|k| spout.x as i64 * DW as i64 - (width - DW as i64) / 2 + (start + k) % width)
                .filter(|x| *x >= 0 && *x < (self.w * DW) as i64)
                .map(|x| x as usize)
                .find(|x| self.free(*x, y));
            match x {
                Some(x) => {
                    self.set(x, y, Dot::Grain(shade, true));
                    poured = true;
                }
                None => break,
            }
        }
        self.owed = self.owed.fract();
        if poured {
            self.choked = 0;
        } else if owing {
            self.choked += 1;
            if self.choked > CHOKE {
                // The pile reached the spout: pour elsewhere.
                self.choked = 0;
                self.aim();
            }
        }
    }

    /// The braille glyph of a cell and its colour: the mean of its
    /// grains, so strata blend across a cell; of its top grain when a
    /// colour has no RGB.
    fn glyph(&self, cx: usize, cy: usize) -> Option<(char, Color)> {
        // Dots 1-2-3-7 on the left, 4-5-6-8 on the right.
        const BITS: [[u32; DW]; DH] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
        let mut bits = 0;
        let mut top = None;
        let (mut sum, mut mixed) = ([0u32; 3], 0u32);
        for (dy, row) in BITS.iter().enumerate() {
            for (dx, bit) in row.iter().enumerate() {
                if let Dot::Grain(s, _) = self.at(cx * DW + dx, cy * DH + dy) {
                    bits |= bit;
                    let color = self.shades[s as usize];
                    top.get_or_insert(color);
                    if let Some(c) = rgb(color) {
                        sum.iter_mut().zip(c).for_each(|(a, b)| *a += b as u32);
                        mixed += 1;
                    }
                }
            }
        }
        let color = if mixed == bits.count_ones() && mixed > 0 {
            let mean = |j: usize| ((sum[j] + mixed / 2) / mixed) as u8;
            Color::Rgb(mean(0), mean(1), mean(2))
        } else {
            top?
        };
        Some((char::from_u32(0x2800 + bits)?, color))
    }

    fn draw(&self, canvas: &mut Canvas) {
        let mut text = [0u8; 4];
        for cy in 0..self.ch {
            for cx in 0..self.w {
                if let Some((glyph, color)) = self.glyph(cx, self.top + cy) {
                    let style = StyleSpec::fg(color);
                    canvas.put(cx as i32, cy as i32, glyph.encode_utf8(&mut text), &style);
                }
            }
        }
    }
}

impl Effect for Sand {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool {
        let ms = t.as_millis() as u64;
        if self.params.duration_ms.is_some_and(|d| ms >= d) {
            return false;
        }
        let (w, h) = (canvas.width() as usize, canvas.height() as usize);
        if w == 0 || h == 0 || w * h > MAX_CELLS {
            return true;
        }
        let origin = self.origin.unwrap_or((0, 0));
        let pos = (origin.0, origin.1 - self.above as i64);
        let world = self.above + h + self.below;
        if (pos, w, world, self.above, h) != (self.pos, self.w, self.h, self.top, self.ch) {
            // A resize eats the sand it covers; a scroll keeps it.
            let resized = self.ch > 0 && (w, h) != (self.w, self.ch);
            (self.top, self.ch) = (self.above, h);
            self.place(pos, w, world, !resized);
        }
        if self.stale {
            self.build();
            self.aim();
        } else if self.params.cursor {
            self.aim();
        }
        let due = ms / STEP_MS;
        if due > self.steps + CATCH_UP {
            self.steps = due - CATCH_UP;
        }
        while self.steps < due {
            self.step();
        }
        self.draw(canvas);
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(FRAME_MS)
    }

    fn looks(&self) -> Looks {
        Looks::Heat
    }

    fn look(&mut self, under: &Under) {
        let seen_w = under.size.0 as usize;
        let beyond = |dir: i64, start: i64| -> Vec<Vec<bool>> {
            (0..MARGIN as i64)
                .map_while(|k| under.row(start + dir * k))
                .collect()
        };
        let above = beyond(-1, -1);
        let below = beyond(1, under.size.1 as i64);
        (self.above, self.below) = (above.len(), below.len());
        let mut seen: Vec<bool> = above.into_iter().rev().flatten().collect();
        seen.extend(under.inked());
        seen.extend(below.into_iter().flatten());
        if (seen_w, &seen) != (self.seen_w, &self.seen) {
            (self.seen_w, self.seen) = (seen_w, seen);
            self.stale = true;
        }
        self.cursor = under.cursor;
        self.hot = (0..under.size.0)
            .map(|x| {
                (0..under.size.1)
                    .filter(|y| under.heat(x, *y).is_some_and(|w| w.class == HeatClass::Hot))
                    .count() as u16
            })
            .collect();
        self.origin = Some(under.origin);
    }

    fn shift(&mut self, from: i64, to: i64, dy: i64) {
        self.stash();
        let moved: Vec<i64> = self.attic.range(from..=to).map(|(y, _)| *y).collect();
        let strips: Vec<(i64, Strip)> = moved
            .into_iter()
            .filter_map(|y| Some((y + dy, self.attic.remove(&y)?)))
            .collect();
        self.attic.extend(strips);
        self.fetch();
    }

    fn tune(&mut self, params: &serde_json::Value) -> Result<(), String> {
        let mut merged =
            serde_json::to_value(Shown::of(&self.params)).map_err(|e| e.to_string())?;
        let (Some(old), Some(new)) = (merged.as_object_mut(), params.as_object()) else {
            return Err("sand: params: an object is needed".into());
        };
        old.extend(new.clone());
        let params = checked(&merged)?;
        if params.colors != self.params.colors {
            (self.shades, self.per) = shades(&params.colors);
            let n = self.shades.len();
            for dot in &mut self.dots {
                if let Dot::Grain(shade, _) = dot {
                    *shade = (*shade as usize % n) as u8;
                }
            }
            for strip in self.attic.values_mut() {
                for dot in &mut strip.dots {
                    if let Dot::Grain(shade, _) = dot {
                        *shade = (*shade as usize % n) as u8;
                    }
                }
            }
        }
        self.params = Params {
            walls: Vec::new(),
            seed: self.params.seed,
            ..params
        };
        self.aim();
        Ok(())
    }
}

/// The parameters a running sand may be given again.
#[derive(serde::Serialize)]
struct Shown {
    rate: f64,
    cursor: bool,
    colors: Vec<ColorSpec>,
    color_ms: u64,
    duration_ms: Option<u64>,
}

impl Shown {
    fn of(p: &Params) -> Shown {
        Shown {
            rate: p.rate,
            cursor: p.cursor,
            colors: p.colors.clone(),
            color_ms: p.color_ms,
            duration_ms: p.duration_ms,
        }
    }
}

fn checked(params: &serde_json::Value) -> Result<Params, String> {
    let params: Params = if params.is_null() {
        Params::default()
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("sand: {e}"))?
    };
    if params.colors.is_empty() || params.colors.len() > 250 {
        return Err("sand: colors: 1 to 250 are needed".into());
    }
    if !(params.rate.is_finite() && (0.0..=10_000.0).contains(&params.rate)) {
        return Err("sand: rate is out of range".into());
    }
    if params.color_ms == 0 {
        return Err("sand: color_ms: at least 1".into());
    }
    Ok(params)
}

/// Factory of the `sand` effect.
pub fn sand(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    Ok(Box::new(Sand::new(checked(params)?)))
}

/// The extension: one effect, no draw function.
pub struct SandExt;

impl DrawExt for SandExt {
    fn name(&self) -> &'static str {
        "sand"
    }

    fn functions(&self) -> Vec<FnDoc> {
        Vec::new()
    }

    fn draw(&self, function: &str, _params: &serde_json::Value) -> Result<SpriteSpec, String> {
        Err(format!(
            "sand: no function {function}; use the effect `sand`"
        ))
    }

    fn effects(&self) -> Vec<(&'static str, EffectFactory)> {
        vec![("sand", Box::new(sand))]
    }
}

#[cfg(test)]
mod tests;
