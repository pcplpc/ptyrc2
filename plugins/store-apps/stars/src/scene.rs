// SPDX-License-Identifier: GPL-3.0-or-later
//! The scene, a function of the time: the sky and the moon (`Night::
//! above`), the grass (`Night::below`), keeping no state but where the
//! pointer has been (`Hand`); meteors and fireflies (`meteor`, `Bug`).

use crate::grid::{DOTS, Dots, Grid, braille, noise};
use crate::sky::{Sky, breeze};
use crate::twister::Twister;
use ptyrc_app::{Value, json};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

pub const MOON: u8 = 230;
/// By third of the grass, bottom up.
pub const GREENS: [[u8; 4]; 3] = [[22, 28, 28, 34], [28, 34, 64, 70], [64, 70, 106, 70]];
pub const PETALS: [u8; 5] = [213, 226, 231, 208, 141];
pub const GLOW: [u8; 3] = [58, 148, 190];

/// Nights a turn of the sky takes, with `--turn`.
pub const TURN: f64 = 4.0;
/// Share of a night the moon stays down.
pub const DOWN: f64 = 0.2;
/// Share of its way the moon has made at the start.
pub const RISEN: f64 = 0.2;
/// Seconds between two meteors.
pub const METEOR: (f64, f64) = (4.0, 15.0);
/// Rows of a view, how tall its grass.
pub const GROWN: [(f64, f64); 2] = [(15.0, 1.0), (50.0, 2.0)];
/// How tall the grass gets, of its height on 15 rows.
pub const TALLEST: f64 = 3.0;
/// How far the pointer leans a blade 4 dots high, in dots.
pub const TOUCH: f64 = 1.6;
/// Dot columns from the pointer its touch falls to 1/e.
pub const SPAN: f64 = 2.5;
/// Dot columns from its cell a touch ends.
pub const NEAR: i64 = 6;
/// Dots over its tip a blade feels the pointer.
pub const GRACE: i64 = 3;
/// Seconds the swing of a blade let go falls to 1/e.
pub const DAMP: f64 = 0.8;
/// Seconds a touch is kept once let go.
pub const LIFE: f64 = 5.0 * DAMP;

/// The middle of the moon, in dots, `f` of its way over `w` by `h`
/// cells: an arc from under the left of the horizon to under its right,
/// a radius from the top at its highest.
pub fn moon_at(w: usize, h: usize, r: f64, f: f64) -> (f64, f64) {
    let (low, top) = (4.0 * h as f64 + r, r + 2.0);
    (
        -r + f * (2.0 * w as f64 + 2.0 * r),
        low - (low - top) * (PI * f).sin(),
    )
}

/// Draws a moon of radius `r` dots, its middle at the dot `(cx, cy)`:
/// the lit side, as seen from the north. The dark side is not drawn; it
/// hides what was in its cells.
pub fn moon(grid: &mut Grid, cx: f64, cy: f64, r: f64, phase: f64) {
    let h = grid.height() as i64;
    let edge = (TAU * phase).cos();
    let mut lit: HashMap<(i64, i64), u8> = HashMap::new();
    let mut dark: HashMap<(i64, i64), u8> = HashMap::new();
    for py in ((cy - r).floor() as i64).max(0)..(4 * h).min((cy + r).ceil() as i64 + 1) {
        let y = py as f64 + 0.5 - cy;
        if y.abs() >= r {
            continue;
        }
        let half = (r * r - y * y).sqrt();
        for px in ((cx - half).floor() as i64).max(0)..(cx + half).ceil() as i64 + 1 {
            let x = px as f64 + 0.5 - cx;
            if x.abs() > half {
                continue;
            }
            let on = if phase < 0.5 {
                x > edge * half
            } else {
                x < -edge * half
            };
            let side = if on { &mut lit } else { &mut dark };
            *side.entry((px >> 1, py >> 2)).or_default() |=
                DOTS[(py & 3) as usize][(px & 1) as usize];
        }
    }
    for &(x, y) in dark.keys() {
        grid.unset(x, y);
    }
    for (&(x, y), &bits) in &lit {
        grid.set(x, y, braille(bits), MOON);
    }
}

/// How tall the grass is in a view of `h` rows, of its height on 15
/// rows or less: 2 on 50 rows, `TALLEST` at most.
pub fn growth(h: usize) -> f64 {
    let [(h0, s0), (h1, s1)] = GROWN;
    (s0 + (h as f64 - h0) * (s1 - s0) / (h1 - h0))
        .min(TALLEST)
        .max(s0)
}

/// The pointer on a cell of the grass. `x`: its column; `up`: its row,
/// 0 the last one of the view; `way`: where it came from, 1 from the
/// left, -1 from the right, 0 from elsewhere; `until`: when it left,
/// infinite while it is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub x: i64,
    pub up: i64,
    pub way: i64,
    pub until: f64,
}

impl Touch {
    pub const fn new(x: i64, up: i64, way: i64, until: f64) -> Touch {
        Touch { x, up, way, until }
    }
}

/// The pointer in the grass: the cell it is on, and those it left in
/// the last `LIFE` seconds.
#[derive(Clone, Debug, Default)]
pub struct Hand {
    pub touches: Vec<Touch>,
}

impl Hand {
    /// The pointer is on the column `x`, `up` rows over the last one,
    /// at `t` seconds.
    pub fn move_to(&mut self, x: i64, up: i64, t: f64) {
        let mut way = 0;
        if let Some(&last) = self.touches.last()
            && last.until == f64::INFINITY
        {
            if (last.x, last.up) == (x, up) {
                return;
            }
            self.leave(t);
            way = (x - last.x).signum();
        }
        self.touches.push(Touch::new(x, up, way, f64::INFINITY));
    }

    /// The pointer is off the grass at `t` seconds.
    pub fn leave(&mut self, t: f64) {
        if let Some(last) = self.touches.last_mut()
            && last.until == f64::INFINITY
        {
            last.until = t;
        }
    }

    /// The touches the grass still shows at `t` seconds.
    pub fn at(&mut self, t: f64) -> &[Touch] {
        self.touches.retain(|one| t - one.until < LIFE);
        &self.touches
    }
}

/// A blade: its height at scale 1, the phase of its flutter, its petal
/// or 0.
pub type Blade = (f64, f64, u8);

/// Grass along the bottom: a blade a dot column, up to 7 dots high, on
/// 2 rows of cells. `wild`: a meadow, up to 11 dots on 3 rows, a flower
/// on some of the tallest. `scale`: how tall it is, of that (`growth`).
#[derive(Clone, Debug)]
pub struct Grass {
    rng: Twister,
    pub wild: bool,
    /// Per dot column, drawn as a view needs them.
    pub blades: Vec<Blade>,
}

impl Grass {
    pub fn new(rng: &mut Twister, wild: bool) -> Grass {
        Grass {
            rng: Twister::new(u64::from(rng.bits(32))),
            wild,
            blades: Vec::new(),
        }
    }

    /// The rows of cells the tallest blade is on.
    pub fn rows(&self, scale: f64) -> usize {
        let top = if self.wild { 12.0 } else { 8.0 };
        ((top * scale - 1.0).ceil() as usize + 3) >> 2
    }

    fn blade(&mut self) -> Blade {
        let rng = &mut self.rng;
        if rng.random() < 0.08 {
            return (0.0, 0.0, 0);
        }
        let tall = if self.wild {
            2.0 + rng.random().powf(1.4) * 10.0
        } else {
            1.0 + rng.random().powf(1.6) * 7.0
        };
        let petal = if tall >= 9.0 && rng.random() < 0.3 {
            rng.choice(&PETALS)
        } else {
            0
        };
        (tall, rng.uniform(0.0, TAU), petal)
    }

    /// How far the pointer leans a blade 4 dots high, in dots, per dot
    /// column, at `t` seconds; `heights`: of the blades.
    ///
    /// A touch parts the blades about its cell, about the edge it came
    /// by when it came from a side: those of the cell lean the way it
    /// goes. It falls off with the distance, and holds a blade by as
    /// much of it as is in the cell, its tip from `GRACE` dots under.
    /// Let go, a blade swings back and dies down; a tall one slower.
    pub fn pushes(heights: &[i64], t: f64, touches: &[Touch]) -> Vec<f64> {
        let mut out = vec![0.0; heights.len()];
        for one in touches {
            let age = t - one.until;
            let (lo, hi) = (
                (2 * one.x - NEAR).max(0),
                (2 * one.x + 2 + NEAR).min(heights.len() as i64),
            );
            for i in lo..hi {
                let height = heights[i as usize];
                let grip = ((height - 4 * one.up + GRACE) as f64 / 4.0).min(1.0);
                if height == 0 || grip <= 0.0 {
                    continue;
                }
                let off = (i - 2 * one.x) as f64 - 0.5;
                let mut much = TOUCH * grip * (-(off / SPAN).powi(2)).exp();
                if age > 0.0 {
                    let swing = 0.7 + 0.05 * height as f64;
                    much *= (-age / DAMP).exp() * (TAU * age / swing).cos();
                }
                out[i as usize] += if off + one.way as f64 > 0.0 {
                    much
                } else {
                    -much
                };
            }
        }
        out.iter()
            .map(|p| p.clamp(-1.5 * TOUCH, 1.5 * TOUCH))
            .collect()
    }

    /// The grass over `w` cells at `t` seconds, `rows(scale)` rows. A
    /// blade bends more towards its tip, and keeps its length: it is
    /// lower when it leans.
    pub fn picture(&mut self, w: usize, t: f64, wind: f64, scale: f64, touches: &[Touch]) -> Grid {
        let rows = self.rows(scale);
        let mut grid = Grid::new(w, rows);
        while self.blades.len() < 2 * w {
            let b = self.blade();
            self.blades.push(b);
        }
        let blades = &self.blades[..2 * w];
        let heights: Vec<i64> = blades.iter().map(|b| (b.0 * scale) as i64).collect();
        let pushes = Grass::pushes(&heights, t, touches);
        let mut dots = Dots::new(w, rows, false);
        let mut heads = Vec::new();
        let bottom = 4 * rows as i64 - 1;
        for (i, &(_, phase, petal)) in blades.iter().enumerate() {
            let height = heights[i];
            if height == 0 {
                continue;
            }
            let hf = height as f64;
            let mut lean = (breeze(i as f64, t, wind) + pushes[i]) * hf / 4.0;
            if wind != 0.0 {
                lean += 0.3 * (2.1 * t + phase).sin();
            }
            // A dot a dot of its length: `y` is how high that one is.
            let (mut px, mut py, mut y) = (0i64, 0i64, 0.0f64);
            for _ in 0..height {
                px = i as i64 + (lean * (y / hf).powf(1.5)).round_ties_even() as i64;
                py = bottom - y.round_ties_even() as i64;
                dots.dot(px, py, 0);
                y += 1.0 / 1f64.hypot(1.5 * lean / hf * (y / hf).sqrt());
            }
            if petal != 0 && (0..2 * w as i64).contains(&px) {
                heads.push((px >> 1, py >> 2, petal));
            }
        }
        for (x, y, bits, _) in dots.cells() {
            let greens = GREENS[3 * (rows - 1 - y) / rows];
            let ink = greens[(4.0 * noise(x as i64, y as i64)) as usize];
            grid.set(x as i64, y as i64, braille(bits), ink);
        }
        for (x, y, petal) in heads {
            let head = ['*', '•', '°'][(3.0 * noise(x, i64::from(petal))) as usize];
            grid.set(x, y, head, petal);
        }
        grid
    }
}

/// A shooting star over a view of `w` by `h`, a sprite of `draw.put`: a
/// trail that falls two cells a row and fades.
pub fn meteor(rng: &mut Twister, w: usize, h: usize) -> Value {
    let way = rng.choice(&[-1.0, 1.0]);
    // The tail on the first row, the head on the second.
    let art = if way > 0.0 {
        ["⠑⢄", "  ⠑⢄"]
    } else {
        ["  ⡠⠊", "⡠⠊"]
    };
    let seconds = rng.uniform(0.35, 0.7);
    let speed = rng.uniform(10.0, 18.0); // rows a second
    let looks = [(244, 231), (240, 253), (237, 246)];
    let ms = (seconds * 1000.0 / looks.len() as f64) as u64;
    let frames: Vec<Value> = (looks.iter())
        .map(|&(a, b)| {
            json!({"cells": [[{"t": (art[0]), "fg": a}], [{"t": (art[1]), "fg": b, "bold": true}]], "ms": ms})
        })
        .collect();
    let x = rng.randrange(0, w as i64);
    let y = rng.randrange(0, (h as i64 / 3).max(1));
    json!({
        "frames": frames,
        "loop": false,
        "anchor": {"at": [x, y], "space": "view"},
        "motion": {"vx": (2.0 * way * speed), "vy": speed, "ax": 0, "ay": 0, "bounce": false},
        "ttl_ms": (((seconds + 0.3) * 1000.0) as u64),
        "z": 1,
    })
}

/// A firefly: it wanders over the grass, and glows now and then. It
/// draws from the app's `rng`, handed to each call.
#[derive(Clone, Debug)]
pub struct Bug {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub sprite: Option<u64>,
    pub cell: Option<(i64, i64)>,
}

impl Bug {
    pub fn new(rng: &mut Twister, w: usize, h: usize, rows: usize) -> Bug {
        let x = rng.uniform(0.0, w as f64);
        let (low, high) = Bug::band(h, rows);
        let y = rng.uniform(low, high);
        Bug {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            sprite: None,
            cell: None,
        }
    }

    /// The rows it keeps to: the top of the grass and 4 above.
    pub fn band(h: usize, rows: usize) -> (f64, f64) {
        let top = h as f64 - rows as f64;
        ((top - 4.0).max(0.0), (top + 0.9).max(0.0))
    }

    pub fn frames(rng: &mut Twister) -> Vec<Value> {
        let dark = rng.randrange(900, 3200);
        let looks = [
            ("·", 0, 140),
            ("•", 1, 160),
            ("•", 2, 380),
            ("•", 1, 160),
            ("·", 0, 140),
        ];
        let mut out = vec![json!({"cells": [[{"t": " "}]], "ms": dark})];
        for (t, c, ms) in looks {
            out.push(json!({"cells": [[{"t": t, "fg": (GLOW[c])}]], "ms": ms}));
        }
        out
    }

    /// Wanders for `dt` seconds; the cell it is in.
    pub fn step(
        &mut self,
        rng: &mut Twister,
        dt: f64,
        w: usize,
        h: usize,
        rows: usize,
    ) -> (i64, i64) {
        self.vx = (self.vx + rng.uniform(-3.0, 3.0) * dt).clamp(-2.0, 2.0);
        self.vy = (self.vy + rng.uniform(-1.5, 1.5) * dt).clamp(-0.6, 0.6);
        let (low, high) = Bug::band(h, rows);
        self.x = (self.x + self.vx * dt).rem_euclid(w.max(1) as f64);
        self.y += self.vy * dt;
        if !(low..=high).contains(&self.y) {
            self.y = self.y.clamp(low, high);
            self.vy = -self.vy;
        }
        (
            (self.x as i64).min(w as i64 - 1),
            (self.y as i64).min(h as i64 - 1),
        )
    }
}

/// The options of `init`, `--all` and `--no-grass` spread.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub stars: f64,
    pub night: f64,
    pub moon: f64,
    /// `None`: tonight's.
    pub phase: Option<f64>,
    pub wind: f64,
    pub twinkle: bool,
    pub turn: bool,
    pub constellations: bool,
    pub galaxy: bool,
    pub meteors: bool,
    pub bugs: bool,
    pub wild: bool,
    pub grass: bool,
}

impl Options {
    /// `Err`: what is wrong, for the log.
    pub fn of(args: &Value) -> Result<Options, String> {
        let on = |k: &str| args[k].as_bool() == Some(true);
        let num = |k: &str, d: f64| args[k].as_f64().unwrap_or(d);
        let all = on("all");
        let grass = args["grass"].as_bool() != Some(false);
        let o = Options {
            stars: num("stars", 70.0),
            night: num("night", 300.0),
            moon: num("moon", 5.0),
            phase: args["phase"].as_f64().map(|p| p.rem_euclid(1.0)),
            wind: num("wind", 1.0),
            twinkle: all || on("twinkle"),
            turn: all || on("turn"),
            constellations: all || on("constellations"),
            galaxy: all || on("galaxy"),
            meteors: all || on("meteors"),
            bugs: grass && (all || on("bugs")),
            wild: grass && (all || on("wild")),
            grass,
        };
        if o.night.is_nan() || o.night <= 0.0 {
            return Err("--night must be positive".into());
        }
        if o.stars.is_nan() || o.stars < 0.0 || o.moon.is_nan() || o.moon < 0.0 {
            return Err("--stars and --moon must not be negative".into());
        }
        Ok(o)
    }
}

/// The scene and its options: what to draw at a time.
#[derive(Clone, Debug)]
pub struct Night {
    pub o: Options,
    pub sky: Sky,
    pub grass: Grass,
    pub phase: f64,
}

impl Night {
    /// `phase`: the moon's when the options name none.
    pub fn new(o: Options, rng: &mut Twister, phase: f64) -> Night {
        let sky = Sky::new(rng, o.stars, o.galaxy, o.constellations);
        let grass = Grass::new(rng, o.wild);
        let phase = o.phase.unwrap_or(phase);
        Night {
            o,
            sky,
            grass,
            phase,
        }
    }

    /// The rows of grass in a view of `h` rows.
    pub fn rows(&self, h: usize) -> usize {
        match self.o.grass {
            true => h.min(self.grass.rows(growth(h))),
            false => 0,
        }
    }

    /// The sky and the moon, `h` rows.
    pub fn above(&mut self, w: usize, h: usize, t: f64) -> Grid {
        let night = self.o.night;
        let turn = if self.o.turn { t / (TURN * night) } else { 0.0 };
        // No star in the last row when it is grass.
        let low = self.rows(h).min(1);
        let mut grid = self.sky.picture(w, h - low, t, self.o.twinkle, turn);
        grid.grow(low);
        let f = (t / night + RISEN).rem_euclid(1.0 + DOWN);
        if self.o.moon > 0.0 && f <= 1.0 {
            let (cx, cy) = moon_at(w, h, self.o.moon, f);
            moon(&mut grid, cx, cy, self.o.moon, self.phase);
        }
        grid
    }

    /// The grass of a view of `h` rows, `rows(h)` rows.
    pub fn below(&mut self, w: usize, h: usize, t: f64, touches: &[Touch]) -> Grid {
        let rows = self.rows(h);
        if rows == 0 {
            return Grid::new(w, 0);
        }
        let mut grid = self.grass.picture(w, t, self.o.wind, growth(h), touches);
        grid.keep_last(rows);
        grid
    }

    /// The whole scene, the grass over the sky: the twin's `--print`.
    pub fn scene(&mut self, w: usize, h: usize, t: f64) -> Grid {
        let mut grid = self.above(w, h, t);
        let low = self.below(w, h, t, &[]);
        grid.over(&low, (h - low.height()) as i64);
        grid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::fnv;
    use crate::twin;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * (1.0 + b.abs())
    }

    fn bits(g: char) -> u8 {
        match g as u32 {
            c @ 0x2801..0x2900 => (c - 0x2800) as u8,
            _ => 0,
        }
    }

    fn dots(grid: &Grid) -> u32 {
        (0..grid.height())
            .flat_map(|y| {
                grid.row(y)
                    .map(|c| bits(c.1).count_ones())
                    .collect::<Vec<_>>()
            })
            .sum()
    }

    /// The twin's skies, pictures, moons and nights, digest for digest.
    #[test]
    fn as_the_twin() {
        for &(seed, density, galaxy, figures, pics) in twin::SKIES {
            let mut sky = Sky::new(&mut Twister::new(seed), density, galaxy, figures);
            for &(w, h, t, twinkle, turn, digest) in pics {
                let got = sky.picture(w, h, t, twinkle, turn);
                assert_eq!(fnv(&got.text()), digest, "sky {seed} {w}x{h} at {t}");
            }
        }
        for &(seed, wild, pics) in twin::GRASS {
            let mut grass = Grass::new(&mut Twister::new(seed), wild);
            for &(w, t, wind, scale, touches, digest) in pics {
                let got = grass.picture(w, t, wind, scale, touches);
                assert_eq!(fnv(&got.text()), digest, "grass {seed} {w} at {t}");
            }
        }
        for &(w, h, cx, cy, r, phase, digest) in twin::MOONS {
            let mut grid = Grid::new(w, h);
            for y in 0..h {
                for x in (0..w).step_by(4) {
                    grid.set(x as i64, y as i64, '✦', 231);
                }
            }
            moon(&mut grid, cx, cy, r, phase);
            assert_eq!(fnv(&grid.text()), digest, "moon {phase} at {cx}");
        }
        for &(f, x, y) in twin::WAYS {
            let got = moon_at(80, 24, 5.0, f);
            assert!(near(got.0, x) && near(got.1, y), "{got:?}");
        }
        for &(seed, options, scenes, word) in twin::NIGHTS {
            let o = Options::of(&options.parse().unwrap()).unwrap();
            let mut rng = Twister::new(seed);
            let mut night = Night::new(o, &mut rng, 0.0);
            for &(w, h, t, digest) in scenes {
                assert_eq!(
                    fnv(&night.scene(w, h, t).text()),
                    digest,
                    "night {options} {w}x{h} at {t}"
                );
            }
            assert_eq!(Twister::new(u64::from(rng.bits(32))).bits(32), word);
        }
        for &(heights, t, touches, want) in twin::PUSHES {
            let got = Grass::pushes(heights, t, touches);
            assert!(got.iter().zip(want).all(|(a, b)| near(*a, *b)), "{got:?}");
        }
        let mut rng = Twister::new(7);
        for &(x, y, ms, ttl, vx) in twin::METEORS {
            let m = meteor(&mut rng, 80, 24);
            assert_eq!(m["anchor"]["at"], json!([x, y]));
            assert_eq!(
                (m["frames"][0]["ms"].as_u64(), m["ttl_ms"].as_u64()),
                (Some(ms), Some(ttl))
            );
            assert!(near(m["motion"]["vx"].as_f64().unwrap(), vx));
        }
        let mut bug = Bug::new(&mut rng, 40, 20, 2);
        let cells: String = (0..200)
            .map(|_| {
                let (x, y) = bug.step(&mut rng, 1.0 / 6.0, 40, 20, 2);
                format!("{x},{y};")
            })
            .collect();
        assert_eq!(
            (
                fnv(&cells),
                Bug::frames(&mut rng)[0]["ms"].as_i64().unwrap()
            ),
            twin::BUG
        );
    }

    /// `test_moon`.
    #[test]
    fn the_moon() {
        let drawn = |phase: f64, cx: f64| {
            let mut g = Grid::new(20, 6);
            moon(&mut g, cx, 12.0, 6.0, phase);
            g
        };
        let disc = dots(&drawn(0.5, 20.0));
        assert!((100..=125).contains(&disc));
        assert_eq!(drawn(0.0, 20.0).count(), 0);
        let sizes: Vec<u32> = [0.1, 0.25, 0.4, 0.5]
            .iter()
            .map(|&p| dots(&drawn(p, 20.0)))
            .collect();
        assert!(sizes.windows(2).all(|p| p[0] < p[1]) && sizes[0] > 4);
        assert!((f64::from(sizes[1]) - f64::from(disc) / 2.0).abs() <= 8.0);
        for (phase, right) in [(0.25, true), (0.75, false)] {
            let g = drawn(phase, 20.0);
            let xs: Vec<usize> = (0..6)
                .flat_map(|y| g.row(y).map(|c| c.0).collect::<Vec<_>>())
                .collect();
            assert!(!xs.is_empty() && xs.iter().all(|&x| (x >= 10) == right));
        }
        assert!((1..disc).contains(&dots(&drawn(0.5, 0.0))));
        assert_eq!(drawn(0.5, -9.0).count(), 0);
        let mut g = Grid::new(20, 6);
        for y in 0..6 {
            g.set(8, y, '✦', 231);
            g.set(4, y, '✦', 231);
        }
        moon(&mut g, 14.0, 12.0, 6.0, 0.25);
        assert!(g.get(4, 3).is_none() && g.get(8, 3).unwrap().1 == MOON);
        assert_eq!(
            (g.get(8, 0), g.get(4, 0)),
            (Some(('✦', 231)), Some(('✦', 231)))
        );
    }

    /// `test_grass`, `test_grass_grows_with_the_view`.
    #[test]
    fn the_grass() {
        let mut grass = Grass::new(&mut Twister::new(5), false);
        let still = grass.picture(80, 0.0, 0.0, 1.0, &[]);
        assert_eq!((still.height(), grass.rows(1.0)), (2, 2));
        assert_eq!(grass.picture(80, 9.0, 0.0, 1.0, &[]), still);
        assert!(still.row(1).count() >= 76 && (10..=70).contains(&still.row(0).count()));
        assert!(
            still
                .row(1)
                .all(|c| GREENS[0].contains(&c.2) && bits(c.1) != 0)
        );
        let looks: Vec<Grid> = (0..60)
            .map(|t| grass.picture(80, f64::from(t) / 6.0, 1.0, 1.0, &[]))
            .collect();
        let mut texts: Vec<String> = looks.iter().map(Grid::text).collect();
        texts.sort();
        texts.dedup();
        assert!(texts.len() > 30);
        let n: Vec<u32> = looks.iter().map(dots).collect();
        let (lo, hi) = (*n.iter().min().unwrap(), *n.iter().max().unwrap());
        assert!(f64::from(hi - lo) < 0.1 * f64::from(hi));
        for look in &looks {
            assert!(look.row(1).filter(|c| bits(c.1) & 0xC0 != 0).count() >= 76);
        }
        let wild = Grass::new(&mut Twister::new(5), true).picture(80, 0.0, 1.0, 1.0, &[]);
        let heads: Vec<(char, u8)> = (0..3)
            .flat_map(|y| {
                wild.row(y)
                    .filter(|c| bits(c.1) == 0)
                    .map(|c| (c.1, c.2))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(wild.height() == 3 && wild.row(0).count() > 10);
        assert!(
            !heads.is_empty()
                && heads
                    .iter()
                    .all(|(g, ink)| "*•°".contains(*g) && PETALS.contains(ink))
        );
        assert!(
            Grass::new(&mut Twister::new(5), false)
                .picture(3, 0.0, 1.0, 1.0, &[])
                .row(1)
                .count()
                > 0
        );

        let g: Vec<f64> = [1, 15, 50, 500].iter().map(|&h| growth(h)).collect();
        assert_eq!(g, [1.0, 1.0, 2.0, 3.0]);
        assert!(near(growth(24), 1.0 + 9.0 / 35.0));
        assert!((15..85).all(|h| growth(h) < growth(h + 1)));
        for (wild, rows) in [(false, [2, 4, 6]), (true, [3, 6, 9])] {
            let mut grass = Grass::new(&mut Twister::new(5), wild);
            for (scale, n) in [1.0, 2.0, 3.0].into_iter().zip(rows) {
                let look = grass.picture(80, 0.0, 0.0, scale, &[]);
                assert_eq!((look.height(), grass.rows(scale)), (n, n));
                assert!(look.row(0).count() > 0);
            }
        }
        let o = Options::of(&json!({"wild": true, "phase": 0.5})).unwrap();
        let mut night = Night::new(o, &mut Twister::new(1), 0.0);
        let rows: Vec<usize> = [2, 15, 24, 50].iter().map(|&h| night.rows(h)).collect();
        assert_eq!(rows, [2, 3, 4, 6]);
        assert_eq!(
            (
                night.below(40, 50, 0.0, &[]).height(),
                night.below(40, 2, 0.0, &[]).height()
            ),
            (6, 2)
        );
        night.o.grass = false;
        assert_eq!(
            (night.rows(50), night.below(40, 50, 0.0, &[]).height()),
            (0, 0)
        );
    }

    /// `test_hand`.
    #[test]
    fn the_hand() {
        let inf = f64::INFINITY;
        let mut hand = Hand::default();
        assert!(hand.at(0.0).is_empty());
        hand.leave(0.0);
        hand.move_to(10, 1, 1.0);
        hand.move_to(10, 1, 2.0);
        assert_eq!(hand.at(2.0), [Touch::new(10, 1, 0, inf)]);
        hand.move_to(12, 1, 3.0);
        hand.move_to(11, 0, 3.5);
        hand.move_to(11, 1, 4.0);
        assert_eq!(
            hand.at(4.0),
            [
                Touch::new(10, 1, 0, 3.0),
                Touch::new(12, 1, 1, 3.5),
                Touch::new(11, 0, -1, 4.0),
                Touch::new(11, 1, 0, inf)
            ]
        );
        hand.leave(5.0);
        hand.leave(6.0);
        hand.move_to(30, 0, 6.5);
        assert_eq!(
            hand.at(6.5)[3..],
            [Touch::new(11, 1, 0, 5.0), Touch::new(30, 0, 0, inf)]
        );
        assert_eq!(hand.at(3.0 + LIFE).len(), 4);
        assert_eq!(hand.at(5.0 + LIFE), [Touch::new(30, 0, 0, inf)]);
    }

    /// `test_the_pointer_in_the_grass`, the swing.
    #[test]
    fn the_pointer_in_the_grass() {
        let mut grass = Grass::new(&mut Twister::new(5), true);
        let scale = 1.5;
        let still = grass.picture(80, 0.0, 0.0, scale, &[]);
        let heights: Vec<i64> = grass.blades.iter().map(|b| (b.0 * scale) as i64).collect();
        let x = (10..60)
            .find(|&x| heights[2 * x] != 0 && heights[2 * x + 1] != 0)
            .unwrap() as i64;
        let (a, b) = (2 * x as usize, 2 * x as usize + 1);
        let push = |t: Touch, at: f64| Grass::pushes(&heights, at, &[t]);
        let held = push(Touch::new(x, 0, 0, f64::INFINITY), 0.0);
        assert!(near(held[a], -TOUCH * (-0.04f64).exp()));
        assert_eq!(push(Touch::new(x, 0, 0, f64::INFINITY), 99.0), held);
        assert!(
            [a, b]
                .iter()
                .all(|&i| push(Touch::new(x, 0, 1, f64::INFINITY), 0.0)[i] > 0.0)
        );
        assert!(
            [a, b]
                .iter()
                .all(|&i| push(Touch::new(x, 0, -1, f64::INFINITY), 0.0)[i] < 0.0)
        );
        assert!(
            push(Touch::new(x, 9, 0, f64::INFINITY), 0.0)
                .iter()
                .all(|&p| p == 0.0)
        );
        let look = grass.picture(80, 0.0, 0.0, scale, &[Touch::new(20, 1, 0, f64::INFINITY)]);
        let (still, look) = (&still, &look);
        let moved: Vec<usize> = (0..still.height())
            .flat_map(|y| (0..80).filter(move |&c| still.get(c, y as i64) != look.get(c, y as i64)))
            .map(|c| c as usize)
            .collect();
        assert!(!moved.is_empty() && moved.iter().all(|&c| (12..=28).contains(&c)));
        // Let go: it swings back through its rest, less each time.
        let swing = |at: f64| push(Touch::new(x, 0, 0, 10.0), at)[b];
        assert_eq!((swing(9.0), swing(10.0)), (held[b], held[b]));
        let trace: Vec<f64> = (1..360)
            .map(|k| swing(10.0 + f64::from(k) / 60.0))
            .collect();
        let turns: Vec<usize> = (0..trace.len() - 1)
            .filter(|&k| trace[k] * trace[k + 1] < 0.0)
            .collect();
        assert!(turns.len() >= 4);
        let peaks: Vec<f64> = turns
            .windows(2)
            .map(|p| {
                trace[p[0]..p[1]]
                    .iter()
                    .fold(0.0, |m: f64, v| m.max(v.abs()))
            })
            .collect();
        assert!(peaks.windows(2).all(|p| p[0] >= p[1]) && peaks[0] < held[b].abs());
        assert!(swing(10.0 + LIFE).abs() < 0.01 * TOUCH);
        let crowd = [Touch::new(20, 0, 1, f64::INFINITY); 9];
        let most = Grass::pushes(&heights, 0.0, &crowd)
            .into_iter()
            .fold(f64::MIN, f64::max);
        assert_eq!(most, 1.5 * TOUCH);
    }

    /// Per dot column with a dot, the height of its highest one.
    fn tips(grid: &Grid) -> std::collections::BTreeMap<i64, i64> {
        let mut out = std::collections::BTreeMap::new();
        let h = grid.height() as i64;
        for y in 0..grid.height() {
            for (x, g, _) in grid.row(y) {
                for (py, pair) in DOTS.iter().enumerate() {
                    for (px, bit) in pair.iter().enumerate() {
                        if bits(g) & bit != 0 {
                            out.entry(2 * x as i64 + px as i64)
                                .or_insert(4 * (h - y as i64) - py as i64);
                        }
                    }
                }
            }
        }
        out
    }

    /// `test_the_pointer_in_the_grass`: a blade leant far is whole, each
    /// dot next to the one before, lower when leant far;
    /// `test_grass_grows_with_the_view`: each blade that much taller.
    #[test]
    fn a_blade_leant_far() {
        let mut one = Grass::new(&mut Twister::new(5), false);
        one.blades = vec![(0.0, 0.0, 0); 80];
        one.blades[40] = (7.0, 0.0, 0);
        for touch in [
            Touch::new(20, 0, 1, f64::INFINITY),
            Touch::new(19, 0, 1, f64::INFINITY),
            Touch::new(21, 2, -1, f64::INFINITY),
        ] {
            let bent = one.picture(40, 0.0, 0.0, 3.0, &[touch]);
            let high = tips(&bent);
            let n = dots(&bent);
            let (lo, hi) = (*high.keys().next().unwrap(), *high.keys().last().unwrap());
            assert!(high.len() > 3 && (15..=21).contains(&n) && lo <= 40 && 40 <= hi);
            assert_eq!(high.len() as i64, hi - lo + 1);
            assert!(*high.values().max().unwrap() <= 21 - i64::from(touch.up == 0));
        }
        let mut grass = Grass::new(&mut Twister::new(5), false);
        let low = tips(&grass.picture(80, 0.0, 0.0, 1.0, &[]));
        for scale in [1.0, 2.0, 3.0] {
            let high = tips(&grass.picture(80, 0.0, 0.0, scale, &[]));
            for (x, l) in &low {
                let h = high[x] as f64;
                assert!((scale * *l as f64).floor() <= h && h < scale * (*l as f64 + 1.0));
            }
        }
    }

    /// `test_meteor_and_bug`.
    #[test]
    fn meteors_and_bugs() {
        let mut rng = Twister::new(7);
        let mut ways = (false, false);
        for _ in 0..20 {
            let m = meteor(&mut rng, 80, 24);
            let (x, y) = (
                m["anchor"]["at"][0].as_i64().unwrap(),
                m["anchor"]["at"][1].as_i64().unwrap(),
            );
            assert!((0..80).contains(&x) && (0..8).contains(&y) && m["loop"] == false);
            let (vx, vy) = (
                m["motion"]["vx"].as_f64().unwrap(),
                m["motion"]["vy"].as_f64().unwrap(),
            );
            assert!(near(vx.abs(), 2.0 * vy));
            let ms: u64 = m["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["ms"].as_u64().unwrap())
                .sum();
            assert!(m["ttl_ms"].as_u64().unwrap() > ms);
            if vx > 0.0 {
                ways.0 = true
            } else {
                ways.1 = true
            }
        }
        assert_eq!(ways, (true, true));
        let mut bug = Bug::new(&mut rng, 40, 20, 2);
        let mut cells: Vec<(i64, i64)> = (0..2000)
            .map(|_| bug.step(&mut rng, 1.0 / 6.0, 40, 20, 2))
            .collect();
        assert!(
            cells
                .iter()
                .all(|&(x, y)| (0..40).contains(&x) && (14..=18).contains(&y))
        );
        cells.sort();
        cells.dedup();
        assert!(cells.len() > 20);
        assert_eq!(Bug::frames(&mut rng)[0]["cells"], json!([[{"t": " "}]]));
    }

    /// `test_print`: the scene of a view, the moon of a phase; without
    /// grass the stars down to the last row.
    #[test]
    fn a_scene() {
        let night = |o: Value, seed: u64| {
            Night::new(Options::of(&o).unwrap(), &mut Twister::new(seed), 0.0)
        };
        let g = night(json!({"phase": 0.5}), 1).scene(40, 12, 0.0);
        assert!(g.height() == 12 && (0..12).any(|y| g.row(y).any(|c| c.2 == MOON)));
        let braille_in = |g: &Grid, y: usize| g.row(y).filter(|c| bits(c.1) != 0).count();
        let bare = night(json!({"all": true, "grass": false}), 2).scene(40, 12, 0.0);
        assert!(braille_in(&bare, 11) < 8);
        let all = night(json!({"all": true}), 2).scene(40, 12, 0.0);
        assert!(braille_in(&all, 11) > 30);
    }
}
