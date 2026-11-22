// SPDX-License-Identifier: GPL-3.0-or-later
//! The night sky of `stars` and `flock`: a map of stars at `(u, v)` in
//! [0, 1), laid over the view whatever its size; the milky way, three
//! constellations; and the breeze in the grass. The twin's `stars.Sky`,
//! `breeze`. Needs `grid.rs`, `twister.rs`.

#![allow(dead_code)]

use crate::grid::{Dots, Grid};
use crate::twister::Twister;
use std::f64::consts::TAU;

/// Cells `density` counts over.
pub const AREA: f64 = 80.0 * 24.0;
/// By level; level 0 is a Braille dot.
pub const GLYPHS: [char; 4] = [' ', '·', '⋆', '✦'];
pub const GREYS: [u8; 4] = [242, 248, 253, 231];
/// Tints, by level: blue, warm, red.
pub const TINTS: [[u8; 4]; 3] = [
    [67, 110, 153, 195],
    [137, 180, 223, 230],
    [131, 174, 217, 224],
];
/// The milky way, from its middle out.
pub const HAZE: [u8; 4] = [146, 103, 60, 239];
/// The lines of a constellation.
pub const LINE: u8 = 238;
/// Seconds a star keeps its look, twinkling.
pub const BEAT: f64 = 0.4;

/// A constellation: its name, stars `(x, y, level)` in dots, and the
/// line through them as indices, -1 lifting the pen.
pub type Figure = (&'static str, &'static [(i64, i64, i64)], &'static [i64]);

/// By name, as `sorted(FIGURES)` of the twin.
#[rustfmt::skip]
pub const FIGURES: [Figure; 5] = [
    ("cassiopeia", &[(0, 0, 2), (5, 7, 3), (10, 2, 3), (15, 8, 2), (20, 1, 2)], &[0, 1, 2, 3, 4]),
    ("crux", &[(5, 0, 2), (5, 13, 3), (0, 6, 3), (11, 5, 2)], &[0, 1, -1, 2, 3]),
    ("cygnus", &[(10, 0, 3), (10, 6, 2), (10, 17, 2), (2, 4, 2), (18, 9, 2)], &[0, 1, 2, -1, 3, 1, 4]),
    ("dipper", &[(0, 3, 2), (5, 1, 3), (9, 2, 3), (13, 3, 2), (14, 8, 2), (21, 9, 3), (22, 3, 3)], &[0, 1, 2, 3, 4, 5, 6, 3]),
    ("orion", &[(2, 0, 3), (14, 1, 3), (5, 9, 2), (8, 8, 3), (11, 7, 2), (4, 17, 2), (15, 16, 3)], &[0, 2, 3, 4, 1, 0, -1, 2, 5, 6, 4]),
];

/// A star of the map. `level`: 0 a dot, to 3 the brightest; `tint`: 0
/// grey, else a row of `TINTS` plus one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    pub u: f64,
    pub v: f64,
    pub level: i64,
    pub tint: usize,
    pub key: i64,
}

/// A map of the sky; the same draws give the same map, at every size.
/// `density`: stars over `AREA`. `galaxy`: the milky way, a band of
/// faint dots. `figures`: three constellations of `FIGURES`.
#[derive(Clone, Debug)]
pub struct Sky {
    pub density: f64,
    pub galaxy: bool,
    stars_rng: Twister,
    haze_rng: Twister,
    /// Drawn as a view needs them.
    pub stars: Vec<Star>,
    pub haze: Vec<(f64, f64, u8)>,
    /// Phase and middle of the milky way.
    pub bend: (f64, f64),
    /// `(u, v, index of FIGURES)`.
    pub figures: Vec<(f64, f64, usize)>,
}

impl Sky {
    pub fn new(rng: &mut Twister, density: f64, galaxy: bool, figures: bool) -> Sky {
        let stars_rng = Twister::new(u64::from(rng.bits(32)));
        let haze_rng = Twister::new(u64::from(rng.bits(32)));
        let bend = (rng.uniform(0.0, 1.0), rng.uniform(0.3, 0.5));
        let mut sky = Sky {
            density,
            galaxy,
            stars_rng,
            haze_rng,
            stars: Vec::new(),
            haze: Vec::new(),
            bend,
            figures: Vec::new(),
        };
        if figures {
            let names = rng.sample(&[0, 1, 2, 3, 4], 3);
            for (i, f) in names.into_iter().enumerate() {
                let u = (0.06 + i as f64 / 3.0 + rng.uniform(0.0, 0.1)).rem_euclid(1.0);
                sky.figures.push((u, rng.uniform(0.04, 0.4), f));
            }
        }
        sky
    }

    fn star(&mut self) -> Star {
        let rng = &mut self.stars_rng;
        let level = rng.weighted(&[0, 1, 2, 3], &[58.0, 27.0, 11.0, 4.0]);
        let tint = if rng.random() < 0.2 {
            rng.choice(&[1, 1, 2, 3])
        } else {
            0
        };
        // More stars high in the sky than over the horizon.
        let u = rng.random();
        let v = rng.random().powf(1.3);
        let key = i64::from(rng.bits(30));
        Star {
            u,
            v,
            level,
            tint,
            key,
        }
    }

    fn dust(&mut self) -> (f64, f64, u8) {
        let rng = &mut self.haze_rng;
        let u = rng.random();
        let off = rng.gauss(0.0, 0.07);
        let (phase, mid) = self.bend;
        let v = mid + 0.22 * (TAU * (u + phase)).sin() + off;
        (u, v, HAZE[((off.abs() / 0.045) as usize).min(3)])
    }

    /// The sky over `w` by `h` cells at `t` seconds. `twinkle`: a star
    /// changes level now and then, each on its own beat. `turn`: how
    /// much of a turn the sky has made, to the right.
    pub fn picture(&mut self, w: usize, h: usize, t: f64, twinkle: bool, turn: f64) -> Grid {
        let mut grid = Grid::new(w, h);
        if w == 0 || h == 0 {
            return grid;
        }
        let (across, down) = (2 * w as i64, 4 * h as i64);
        let share = (w * h) as f64 / AREA;
        let mut dots = Dots::new(w, h, true);
        // Per cell: level and colour of the brightest.
        let mut bright: Vec<Option<(i64, u8)>> = vec![None; w * h];
        let place = |u: f64, v: f64| {
            (
                ((u + turn).rem_euclid(1.0) * across as f64) as i64,
                (v * down as f64) as i64,
            )
        };
        let mut star = |dots: &mut Dots, px: i64, py: i64, level: i64, tint: usize, key: i64| {
            let mut level = level;
            if twinkle {
                let roll = noise(key, (t / BEAT + 16.0 * noise(key, 0)) as i64);
                level += i64::from(roll > 0.93) - i64::from(roll < 0.14);
            }
            let level = level.min(3);
            if level < 0 || !(0..across).contains(&px) || !(0..down).contains(&py) {
                return;
            }
            let colour = match tint {
                0 => GREYS[level as usize],
                n => TINTS[n - 1][level as usize],
            };
            if level == 0 {
                dots.dot(px, py, colour);
                return;
            }
            let cell = &mut bright[(py >> 2) as usize * w + (px >> 1) as usize];
            if cell.is_none_or(|(l, _)| l <= level) {
                *cell = Some((level, colour));
            }
        };
        for &(u, v, f) in &self.figures {
            let (x0, y0) = place(u, v);
            let (_, points, path) = FIGURES[f];
            for ab in path.windows(2) {
                if ab[0] >= 0 && ab[1] >= 0 {
                    let (xa, ya, _) = points[ab[0] as usize];
                    let (xb, yb, _) = points[ab[1] as usize];
                    dots.line((x0 + xa, y0 + ya), (x0 + xb, y0 + yb), LINE);
                }
            }
        }
        if self.galaxy {
            let n = (6.0 * self.density * share) as usize;
            while self.haze.len() < n {
                let d = self.dust();
                self.haze.push(d);
            }
            for &(u, v, colour) in &self.haze[..n] {
                let (px, py) = place(u, v);
                dots.dot(px, py, colour);
            }
        }
        let n = (self.density * share) as usize;
        while self.stars.len() < n {
            let s = self.star();
            self.stars.push(s);
        }
        for s in &self.stars[..n] {
            let (px, py) = place(s.u, s.v);
            star(&mut dots, px, py, s.level, s.tint, s.key);
        }
        for (i, &(u, v, f)) in self.figures.iter().enumerate() {
            let (x0, y0) = place(u, v);
            for (j, &(x, y, level)) in FIGURES[f].1.iter().enumerate() {
                let key = 977 * i as i64 + j as i64;
                star(
                    &mut dots,
                    (x0 + x).rem_euclid(across),
                    y0 + y,
                    level,
                    0,
                    key,
                );
            }
        }
        dots.onto(&mut grid);
        for (i, b) in bright.iter().enumerate() {
            if let Some((level, colour)) = b {
                grid.set(
                    (i % w) as i64,
                    (i / w) as i64,
                    GLYPHS[*level as usize],
                    *colour,
                );
            }
        }
        grid
    }
}

use crate::grid::noise;

/// How far the breeze leans a blade 4 dots high at the dot column `x`,
/// in dots, at `t` seconds: a steady lean, a wave that travels with the
/// wind, a gust now and then.
pub fn breeze(x: f64, t: f64, wind: f64) -> f64 {
    let way = if wind >= 0.0 { 1.0 } else { -1.0 };
    let wave = (0.11 * x - 1.3 * way * t).sin();
    let gust = (0.023 * x - 0.31 * way * t).sin().max(0.0).powi(3);
    wind * (0.25 + 0.45 * wave + 0.9 * gust)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sky(seed: u64) -> Sky {
        Sky::new(&mut Twister::new(seed), 70.0, false, false)
    }

    /// `test_sky_is_a_map`.
    #[test]
    fn a_map() {
        let mut s = sky(1);
        let small = s.picture(40, 12, 0.0, false, 0.0);
        assert_eq!(s.picture(40, 12, 0.0, false, 0.0), small);
        assert_eq!(sky(1).picture(40, 12, 0.0, false, 0.0), small);
        assert_ne!(sky(2).picture(40, 12, 0.0, false, 0.0), small);
        let large = s.picture(80, 24, 0.0, false, 0.0);
        assert!((10..=18).contains(&small.count()) && (55..=70).contains(&large.count()));
        assert_eq!((s.stars.len(), large.height()), (70, 24));
        let mut none = Sky::new(&mut Twister::new(1), 0.0, false, false);
        assert_eq!(none.picture(80, 24, 0.0, false, 0.0).count(), 0);
        assert_eq!(s.picture(0, 5, 0.0, false, 0.0).height(), 5);
        assert_eq!(s.picture(5, 0, 0.0, false, 0.0).height(), 0);
        assert_eq!(s.picture(80, 24, 50.0, false, 0.0), large);
        let looks: Vec<Grid> = (0..20)
            .map(|t| s.picture(80, 24, f64::from(t) / 2.0, true, 0.0))
            .collect();
        let mut texts: Vec<String> = looks.iter().map(Grid::text).collect();
        texts.sort();
        texts.dedup();
        assert!(texts.len() > 10);
        assert!(looks.iter().filter(|l| **l == large).count() < 20);
        assert_eq!(s.picture(80, 24, 0.0, false, 1.0), large);
        let half = s.picture(80, 24, 0.0, false, 0.5);
        assert_ne!(half, large);
        assert_eq!(half.count(), large.count());
    }

    /// `test_sky_galaxy_and_figures`.
    #[test]
    fn galaxy_and_figures() {
        let plain = sky(3).picture(80, 24, 0.0, false, 0.0);
        let galaxy =
            Sky::new(&mut Twister::new(3), 70.0, true, false).picture(80, 24, 0.0, false, 0.0);
        assert!(galaxy.count() > 4 * plain.count());
        let inks: Vec<u8> = (0..24)
            .flat_map(|y| galaxy.row(y).map(|c| c.2).collect::<Vec<_>>())
            .collect();
        assert!(HAZE.iter().all(|h| inks.contains(h)));
        let mut s = Sky::new(&mut Twister::new(3), 70.0, false, true);
        let mut names: Vec<usize> = s.figures.iter().map(|f| f.2).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 3);
        let drawn = s.picture(120, 40, 0.0, false, 0.0);
        assert!((0..40).any(|y| drawn.row(y).any(|c| c.2 == LINE)));
        for (_, points, path) in FIGURES {
            assert!(path.iter().all(|&i| -1 <= i && i < points.len() as i64));
            assert!((0..points.len() as i64).all(|i| path.contains(&i)));
            assert!(points.iter().all(|p| (1..=3).contains(&p.2)));
        }
    }

    /// `test_grass`: the breeze.
    #[test]
    fn the_breeze() {
        assert_eq!(breeze(10.0, 3.0, 0.0), 0.0);
        assert_eq!(breeze(10.0, 3.0, -1.0), -breeze(10.0, -3.0, 1.0));
        for x in 0..160 {
            for t in 0..90 {
                let b = breeze(f64::from(x), f64::from(t) / 3.0, 1.0);
                assert!(-0.3 < b && b < 1.7);
            }
        }
    }
}
