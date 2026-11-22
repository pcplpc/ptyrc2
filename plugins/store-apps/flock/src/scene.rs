// SPDX-License-Identifier: GPL-3.0-or-later
//! The scene: a dusk (`Dusk`), a land (`Land`), the birds (`Flock`),
//! and what to draw at a time (`Scene`). The twin's `flock`; the same
//! draws for the same seed, the same operations in the same order.

use crate::grid::{DOTS, Grid, braille, clear, noise, walls_of};
use crate::sky::{AREA, Sky, breeze};
use crate::twister::Twister;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

/// A point in dots.
pub type Dot = (f64, f64);
type Vec3 = (f64, f64, f64);
/// What scares: a dot, and how much, 0 to 1.
pub type Scare = (f64, f64, f64);

/// Seconds a tick: the birds are drawn.
pub const PERIOD: f64 = 1.0 / 30.0;
/// Seconds a step of the birds.
pub const DT: f64 = 1.0 / 10.0;
/// Steps a flock just made has flown.
pub const WARM: i64 = 10;
/// Steps at most from a call to the next.
pub const LEAP: i64 = 50;
/// Ticks between two looks at the sky; and at the land.
pub const SKY: u64 = 30;
pub const LAND: u64 = 6;

/// Share of a dusk at which the sun is down.
pub const SET: f64 = 0.8;
/// The night, in dusks.
pub const REST: f64 = 0.1;
/// Share of a dusk gone at the start.
pub const START: f64 = 0.3;
/// The first bird goes down, the last, a flight.
pub const ROOST: (f64, f64, f64) = (0.86, 0.08, 0.04);
/// The first bird goes up, the last.
pub const RISE: (f64, f64) = (0.0, 0.045);
/// Places along the land they roost at.
pub const SITES: i64 = 9;
/// The first group comes in, the last, a flight.
pub const JOIN: (f64, f64, f64) = (0.08, 0.62, 0.035);
/// Share of the birds that roost here, with `join`.
pub const CORE: f64 = 0.3;
/// The second flock: its birds, over the first.
pub const SECOND: f64 = 0.35;
/// Seconds between two hunts, and one at most.
pub const HAWK: (f64, f64) = (90.0, 14.0);
/// Seconds between two splits; how often there is one.
pub const SPLIT: (f64, f64) = (50.0, 0.6);
/// Seconds a bird strays; the share that does.
pub const STRAY: (f64, f64) = (8.0, 0.05);
/// The leads of the tribes: how fast each goes.
pub const LEADS: [f64; 3] = [1.0, 1.13, 0.89];
/// Ranks in a tribe; seconds a rank is behind the one before.
pub const LAGS: (usize, f64) = (6, 0.6);
/// How much a pointer that rests scares, of 1.
pub const STILL: f64 = 0.35;
/// Seconds it takes to calm down to `STILL`.
pub const CALM: f64 = 4.0;
// The flight. Lengths in `Flock::reach`, speeds in `PACE`.
pub const PACE: f64 = 0.55;
const HOOD: f64 = 0.24;
const PACK: f64 = 0.08;
const ALIGN: f64 = 1.2;
const COHERE: f64 = 1.0;
const PART: f64 = 2.0;
const LEAD: f64 = 0.9;
const SLACK: f64 = 0.45;
const ROAM: f64 = 2.0;
const WANDER: f64 = 0.35;
const WALL: f64 = 3.0;
const LEVEL: f64 = 0.5;
const FEAR: f64 = 4.5;
const DASH: f64 = 0.9;
const SHY: f64 = 0.9;
const WARY: f64 = 0.85;
const STEEP: f64 = 0.5;
/// A bird: on its roost, in the air, coming in.
pub const DOWN: u8 = 0;
pub const UP: u8 = 1;
pub const IN: u8 = 2;

/// The sun, from high to the horizon.
pub const SUN: [u8; 8] = [229, 228, 221, 220, 214, 208, 202, 196];
/// The sky, from the sun out; later in the dusk it is read further.
pub const RAMP: [u8; 15] = [
    229, 221, 220, 214, 208, 209, 203, 204, 168, 133, 97, 61, 60, 60, 59,
];
/// By birds in a cell: 1, 2, 3, more.
pub const BIRDS: [u8; 4] = [243, 247, 251, 255];
/// The land; towards the sun.
pub const SOIL: [u8; 2] = [239, 95];
pub const REEDS: [u8; 4] = [58, 94, 58, 101];
pub const WATER: [u8; 3] = [60, 61, 24];
/// A cloud at night; by day it takes the sky's colours.
pub const CLOUD: [u8; 2] = [240, 243];
/// The hawk.
pub const PREY: u8 = 223;
/// Its dots: wings swept back.
const HAWK_DOTS: [(i64, i64); 10] = [
    (0, 0),
    (0, 1),
    (-1, 0),
    (1, 0),
    (-2, -1),
    (2, -1),
    (-3, -1),
    (3, -1),
    (-4, -2),
    (4, -2),
];

fn dot(px: i64, py: i64) -> u8 {
    DOTS[(py & 3) as usize][(px & 1) as usize]
}

fn or_at(cells: &mut HashMap<(i64, i64), u8>, px: i64, py: i64) {
    *cells.entry((px >> 1, py >> 2)).or_insert(0) |= dot(px, py);
}

pub fn ease(x: f64) -> f64 {
    let x = 1.0f64.min(0.0f64.max(x));
    x * x * (3.0 - 2.0 * x)
}

/// Smooth noise in [0, 1) along `x`, a knot at each integer.
pub fn wave(x: f64, seed: i64) -> f64 {
    let i = x.floor();
    let k = i as i64;
    noise(k, seed) + (noise(k + 1, seed) - noise(k, seed)) * ease(x - i)
}

/// Between the places `a`, `b`, `c` of three steps in a row, the share
/// `s` of a step on: a quadratic B-spline; place and speed continuous.
pub fn glide(a: f64, b: f64, c: f64, s: f64) -> f64 {
    let r = 1.0 - s;
    0.5 * (r * r * a + (1.0 + 2.0 * s * r) * b + s * s * c)
}

/// Where in the dusk `t` seconds are: 0 the sun over the view, `SET`
/// it is down, 1 the night, to `1 + REST`.
pub fn stage(t: f64, dusk: f64, start: f64) -> f64 {
    (t / dusk + start).rem_euclid(1.0 + REST)
}

/// The sun over the horizon at the stage `f`: 1 over the view, 0 on
/// the horizon, under 0 down.
pub fn height(f: f64) -> f64 {
    1.0 - f / SET
}

/// How much the sky glows, 0 to 1: most when the sun is low.
pub fn glow(f: f64) -> f64 {
    let a = height(f);
    if a >= 0.5 {
        return 0.55 * ease((1.0 - a) / 0.5);
    }
    if a >= 0.1 {
        return 0.55 + 0.45 * ease((0.5 - a) / 0.4);
    }
    if a >= -0.06 {
        return 1.0;
    }
    1.0 - ease((-0.06 - a) / 0.2)
}

/// The middle of the sun, in dots, at the stage `f`: from over a view
/// `w` cells wide down to the right, under the dot row `top` once set.
pub fn sun_at(w: usize, top: f64, r: f64, f: f64) -> Dot {
    let share = 1.0f64.min(f / SET);
    (
        (2 * w) as f64 * (0.3 + 0.28 * share),
        -r + share * (top + 2.0 * r),
    )
}

/// The dots of the sun over the land, as bits by cell.
fn disc(cx: f64, cy: f64, r: f64, land: &Land, h: usize) -> HashMap<(i64, i64), u8> {
    let mut out = HashMap::new();
    let py0 = 0.max((cy - r).floor() as i64);
    let py1 = (4 * h as i64).min((cy + r).ceil() as i64 + 1);
    for py in py0..py1 {
        let y = py as f64 + 0.5 - cy;
        if y.abs() >= r {
            continue;
        }
        let half = (r * r - y * y).sqrt();
        for px in 0.max((cx - half).floor() as i64)..(cx + half).ceil() as i64 + 1 {
            if (px as f64 + 0.5 - cx).abs() > half {
                continue;
            }
            if py < land.top(px, h) {
                or_at(&mut out, px, py);
            }
        }
    }
    out
}

/// A cloud: across, down, long in widths, thick, its key.
type Bar = (f64, f64, f64, f64, i64);

/// The sky of a sunset: strokes of light around the sun and along the
/// horizon. `clouds`: bars of cloud that drift, lit from under.
pub struct Dusk {
    seed: i64,
    clouds: Vec<Bar>,
    size: (usize, usize),
    dots: Vec<Vec<Vec<(f64, u8)>>>,
}

impl Dusk {
    pub fn new(rng: &mut Twister, clouds: bool) -> Dusk {
        let seed = i64::from(rng.bits(24));
        let mut bars = Vec::new();
        if clouds {
            for i in 0..7 {
                let u = rng.random();
                let v = 0.12 + 0.62 * (i as f64 + rng.random()) / 7.0;
                let long = rng.uniform(0.1, 0.24);
                let thick = rng.uniform(0.6, 1.4);
                bars.push((u, v, long, thick, i64::from(rng.bits(20))));
            }
        }
        Dusk {
            seed,
            clouds: bars,
            size: (0, 0),
            dots: Vec::new(),
        }
    }

    /// Per cell, its dots that can light: the density of light from
    /// which each does, lowest first.
    fn strokes(&mut self, w: usize, h: usize) {
        if (w, h) == self.size {
            return;
        }
        let seed = self.seed;
        let mut rows = Vec::with_capacity(h);
        for y in 0..h {
            let mut row: Vec<Vec<(f64, u8)>> = vec![Vec::new(); w];
            for j in 0..4 {
                let py = 4 * y as i64 + j;
                let gain = 1.5 * noise(py, seed + 1).powf(1.5);
                if gain < 0.15 {
                    continue;
                }
                let slide = 97.0 * noise(py, seed);
                for px in 0..2 * w as i64 {
                    // A stroke grows from its middle as the light does.
                    let need = (1.0 - wave(px as f64 / 19.0 + slide, seed + py)) / gain;
                    if need < 1.0 {
                        row[(px >> 1) as usize].push((need, DOTS[j as usize][(px & 1) as usize]));
                    }
                }
            }
            for cell in &mut row {
                cell.sort_by(|a, b| a.partial_cmp(b).unwrap());
            }
            rows.push(row);
        }
        self.size = (w, h);
        self.dots = rows;
    }

    /// The sky over `w` by `h` cells, the horizon under its last row,
    /// at the stage `f` and `t` seconds; `sun`: its middle.
    pub fn picture(&mut self, w: usize, h: usize, f: f64, sun: Dot, t: f64) -> Grid {
        let mut grid = Grid::new(w, h);
        let g = glow(f);
        if w == 0 || h == 0 || g <= 0.0 {
            return grid;
        }
        let (across, down) = (2.0 * w as f64, 4.0 * h as f64);
        let (sx, sy) = sun;
        let a = height(f);
        let late = 2.5 * ease(1.0 - a) + 4.5 * ease(-a / 0.3);
        let reach = 0.62 * down;
        self.strokes(w, h);
        let last = RAMP.len() - 1;
        let offs: Vec<f64> = (0..w)
            .map(|x| ((2 * x) as f64 + 1.0 - sx) / reach)
            .collect();
        let round: Vec<f64> = (offs.iter())
            .map(|dx| 0.42 * g * (-3.0 * dx * dx / 3.2).exp())
            .collect();
        let along: Vec<f64> = (offs.iter())
            .map(|dx| {
                let q = dx * reach / across;
                g * (-5.0 * (q * q)).exp()
            })
            .collect();
        for y in 0..h {
            let cy = (4 * y) as f64 + 2.0;
            let dy = (cy - sy.min(down)) / reach;
            let near = (-3.0 * dy * dy).exp();
            let band = 0.34 * (-(down - cy) / (0.2 * down)).exp();
            for (x, dots) in self.dots[y].iter().enumerate() {
                if dots.is_empty() {
                    continue;
                }
                let dens = round[x] * near + along[x] * band;
                if dots[0].0 >= dens {
                    continue;
                }
                let mut bits = 0u8;
                for &(need, bit) in dots {
                    if need >= dens {
                        break;
                    }
                    bits |= bit;
                }
                let far = 1.0f64.min((offs[x] * offs[x] / 3.2 + dy * dy).sqrt() * 0.95);
                let step = (late + 9.5 * far + noise(x as i64, y as i64) * 0.9) as usize;
                grid.set(x as i64, y as i64, braille(bits), RAMP[last.min(step)]);
            }
        }
        for i in 0..self.clouds.len() {
            let bar = self.clouds[i];
            cloud(&mut grid, w, h, bar, (sx, sy, g, late), t);
        }
        grid
    }
}

fn cloud(grid: &mut Grid, w: usize, h: usize, bar: Bar, light: (f64, f64, f64, f64), t: f64) {
    let (u, v, long, thick, key) = bar;
    let (sx, sy, g, late) = light;
    let (across, down) = (2 * w as i64, 4 * h as i64);
    let half = long * across as f64 / 2.0;
    let deep = 2i64.max((thick * (1.5 + down as f64 / 50.0)).round_ties_even() as i64);
    let cx = (u * across as f64 + (0.25 + 0.4 * noise(key, 0)) * t)
        .rem_euclid(across as f64 + 2.0 * half)
        - half;
    let cy = (v * down as f64) as i64;
    let mut cells: HashMap<(i64, i64), u8> = HashMap::new();
    for j in -deep..=deep {
        // A lens of strokes: flat under, frayed at its ends.
        let lean = j as f64 / deep as f64;
        let mut span = half * 0.0f64.max(1.0 - lean * lean).sqrt();
        span *= 0.55 + 0.9 * noise(j, key);
        let mid = cx + half * 0.5 * (noise(j, key + 1) - 0.5);
        let py = cy + j;
        if !(0..down).contains(&py) || (j != 0 && noise(j, key + 2) < 0.3) {
            continue;
        }
        for px in 0.max((mid - span) as i64)..across.min((mid + span) as i64) {
            let edge = 1.0 - (px as f64 - mid).abs() / 1.0f64.max(span);
            if wave(px as f64 / 6.0, key + j) < 0.2 + 0.9 * edge {
                or_at(&mut cells, px, py);
            }
        }
    }
    let last = (RAMP.len() - 1) as i64;
    for ((x, y), bits) in cells {
        // Lit from under, and from the side of the sun.
        let under = (4 * y + 2 - cy) as f64 / deep as f64;
        let near = ((2 * x) as f64 - sx) / across as f64;
        let near = near.hypot(((4 * y) as f64 - sy) / down as f64);
        let colour = if g > 0.12 {
            let step = late + 1.5 + 7.0 * near - 2.0 * under + 3.0 * (1.0 - g);
            RAMP[0.max(last.min(step as i64)) as usize]
        } else {
            CLOUD[usize::from(under > 0.0)]
        };
        grid.set(x, y, braille(bits), colour);
    }
}

/// A bird: `beat`, the phase of its wander; `pace`, its speed over the
/// flock's; `order`: 0 the first of its lot up, 1 the last; `group`:
/// the one it comes in with, 0 the core; `clan`: the one it roosts
/// with; `tribe`: the lead it follows, 1 the second flock; `rank`: how
/// far behind its lead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bird {
    pub beat: f64,
    pub pace: f64,
    pub order: f64,
    pub group: i64,
    pub clan: i64,
    pub tribe: usize,
    pub rank: usize,
}

/// Where the birds fly: a view `w` cells wide, `over` rows of sky; the
/// stage from `dusk` and `at`; the skyline of `land` in a view `h`
/// high (`None`: flat, the dot row `4 over`); `roosts`: dots to perch
/// on, the bird `i` on the `i`th.
#[derive(Clone, Copy)]
pub struct Air<'a> {
    pub w: usize,
    pub over: usize,
    pub dusk: f64,
    pub at: f64,
    pub land: Option<(&'a Land, usize)>,
    pub roosts: &'a [(i64, i64)],
}

impl Air<'_> {
    pub fn stage(&self, t: f64) -> f64 {
        stage(t, self.dusk, self.at)
    }

    pub fn ground(&self, px: i64) -> i64 {
        match self.land {
            Some((land, h)) => land.top(px, h),
            None => 4 * self.over as i64,
        }
    }
}

/// The hawk: where, its heading, its aim, away (0: in a stoop), born,
/// passes left, since when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hawk {
    pub x: f64,
    pub y: f64,
    pub way: f64,
    ax: f64,
    ay: f64,
    away: bool,
    born: f64,
    passes: f64,
    since: f64,
}

/// Birds, each on its own wings, stepped `DT` by `DT`: with their
/// neighbours, after the lead of their tribe, away from what scares
/// them. Neighbours: those of the 27 cells around a bird's own.
/// `fly(air, t)` steps to `t`; a time before the last, or more than
/// `LEAP` steps after: all anew `WARM` steps before. What is drawn is
/// a curve through the last three steps (`glide`).
pub struct Flock {
    rng: Twister,
    pub seed: i64,
    density: f64,
    second: f64,
    join: bool,
    hunted: bool,
    phases: Vec<[f64; 5]>,
    pub birds: Vec<Bird>,
    pub k: Option<i64>,
    pub f: f64,
    size: (usize, usize),
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub z: Vec<f64>,
    pub vx: Vec<f64>,
    pub vy: Vec<f64>,
    pub vz: Vec<f64>,
    px: Vec<f64>,
    py: Vec<f64>,
    qx: Vec<f64>,
    qy: Vec<f64>,
    part: f64,
    pub state: Vec<u8>,
    pub tribe: Vec<usize>,
    up: Vec<f64>,
    down: Vec<f64>,
    site: Vec<f64>,
    stray: Vec<bool>,
    pub rift: Option<i64>,
    bed: bool,
    pub hawk: Option<Hawk>,
    trail: [Dot; 2],
}

/// Cells of neighbours, in the order first met: a key, `[n, Σx, Σy, Σz,
/// Σvx, Σvy, Σvz]`.
#[derive(Default)]
struct Cells {
    at: HashMap<(i64, i64, i64, usize), usize>,
    keys: Vec<(i64, i64, i64, usize)>,
    sums: Vec<[f64; 7]>,
}

impl Flock {
    pub fn new(rng: &mut Twister, birds: f64, second: f64, join: bool, hawk: bool) -> Flock {
        let own = Twister::new(u64::from(rng.bits(32)));
        let seed = i64::from(rng.bits(20));
        let phases = (0..LEADS.len())
            .map(|_| std::array::from_fn(|_| rng.uniform(0.0, TAU)))
            .collect();
        Flock {
            rng: own,
            seed,
            density: birds * (1.0 + second),
            second: second / (1.0 + second),
            join,
            hunted: hawk,
            phases,
            birds: Vec::new(),
            k: None,
            f: 0.0,
            size: (0, 0),
            x: Vec::new(),
            y: Vec::new(),
            z: Vec::new(),
            vx: Vec::new(),
            vy: Vec::new(),
            vz: Vec::new(),
            px: Vec::new(),
            py: Vec::new(),
            qx: Vec::new(),
            qy: Vec::new(),
            part: 0.0,
            state: Vec::new(),
            tribe: Vec::new(),
            up: Vec::new(),
            down: Vec::new(),
            site: Vec::new(),
            stray: Vec::new(),
            rift: None,
            bed: false,
            hawk: None,
            trail: [(0.0, 0.0); 2],
        }
    }

    pub fn count(&self, w: usize, h: usize) -> usize {
        (self.density * w as f64 * h as f64 / AREA) as usize
    }

    fn bird(&mut self) -> Bird {
        let rng = &mut self.rng;
        let beat = rng.uniform(0.0, TAU);
        let pace = rng.uniform(0.9, 1.12);
        let order = rng.random();
        let group = if rng.random() >= CORE {
            rng.randrange(1, 9)
        } else {
            0
        };
        let clan = rng.randrange(0, SITES);
        let tribe = usize::from(rng.random() < self.second);
        let rank = rng.randrange(0, LAGS.0 as i64) as usize;
        Bird {
            beat,
            pace,
            order,
            group,
            clan,
            tribe,
            rank,
        }
    }

    /// The unit of the flight, in dots: a flock at ease is about three.
    pub fn reach(&self, across: f64, down: f64) -> f64 {
        1.0f64.max((0.18 * across).min(0.36 * down))
    }

    /// Where the birds of `tribe` are drawn to at `t` seconds, in dots;
    /// the third, into the depth.
    pub fn lead(&self, tribe: usize, across: f64, down: f64, t: f64) -> Vec3 {
        let p = &self.phases[tribe];
        let t = t * LEADS[tribe];
        (
            across * (0.5 + 0.3 * (0.27 * t + p[0]).sin() + 0.08 * (0.61 * t + p[1]).sin()),
            down * (0.4 + 0.17 * (0.19 * t + p[2]).sin() + 0.06 * (0.67 * t + p[3]).sin()),
            0.5 * self.reach(across, down) * (0.23 * t + p[4]).sin(),
        )
    }

    /// The dot column of the roost `site`.
    pub fn home(&self, site: i64, across: f64) -> f64 {
        across * (site as f64 + 0.2 + 0.6 * noise(site, self.seed)) / SITES as f64
    }

    fn up_of(&self, bird: &Bird) -> f64 {
        if self.join && bird.group != 0 {
            let came = JOIN.0 + (JOIN.1 - JOIN.0) * noise(bird.group, 3);
            return came + 0.25 * JOIN.2 * bird.order;
        }
        // A clan after the other.
        let turn = (4 * bird.clan + self.seed).rem_euclid(SITES);
        RISE.0 + RISE.1 * (turn as f64 + 3.0 * bird.order) / (SITES + 2) as f64
    }

    /// The birds of this view: the first of one list.
    fn fit(&mut self, air: &Air) {
        let n = self.count(air.w, air.over);
        let size = (air.w, air.over);
        if size != self.size && !self.x.is_empty() {
            let kx = if self.size.0 != 0 {
                air.w as f64 / self.size.0 as f64
            } else {
                1.0
            };
            let ky = if self.size.1 != 0 {
                air.over as f64 / self.size.1 as f64
            } else {
                1.0
            };
            for row in [
                &mut self.x,
                &mut self.z,
                &mut self.site,
                &mut self.px,
                &mut self.qx,
            ] {
                row.iter_mut().for_each(|v| *v *= kx);
            }
            for row in [&mut self.y, &mut self.py, &mut self.qy] {
                row.iter_mut().for_each(|v| *v *= ky);
            }
        }
        self.size = size;
        while self.birds.len() < n {
            let b = self.bird();
            self.birds.push(b);
        }
        for row in [
            &mut self.x,
            &mut self.y,
            &mut self.z,
            &mut self.vx,
            &mut self.vy,
            &mut self.vz,
            &mut self.px,
            &mut self.py,
            &mut self.qx,
            &mut self.qy,
        ] {
            row.resize(n, 0.0);
        }
        for i in self.state.len()..n {
            // On the ground: up at the next step, if it is its time.
            let bird = self.birds[i];
            self.state.push(DOWN);
            self.tribe.push(bird.tribe);
            let up = self.up_of(&bird);
            self.up.push(up);
            self.down.push(ROOST.0 + ROOST.1 * bird.order);
            let site = self.home(bird.clan, 2.0 * air.w as f64)
                + (noise(i as i64, 5) - 0.5) * 0.3 * air.w as f64;
            self.site.push(site);
            self.stray.push(false);
        }
        self.state.truncate(n);
        self.tribe.truncate(n);
        self.up.truncate(n);
        self.down.truncate(n);
        self.site.truncate(n);
        self.stray.truncate(n);
    }

    /// All anew at the step `k`: in the air about their leads, or down.
    fn start(&mut self, k: i64, air: &Air) {
        for row in [
            &mut self.x,
            &mut self.up,
            &mut self.down,
            &mut self.site,
            &mut self.y,
            &mut self.px,
            &mut self.py,
            &mut self.qx,
            &mut self.qy,
        ] {
            row.clear();
        }
        self.state.clear();
        self.tribe.clear();
        self.stray.clear();
        self.fit(air);
        self.k = Some(k);
        self.rift = None;
        self.bed = false;
        self.hawk = None;
        let t = k as f64 * DT;
        let f = air.stage(t);
        self.f = f;
        let (across, down) = (2.0 * air.w as f64, 4.0 * air.over as f64);
        let u = self.reach(across, down);
        let mut rng = Twister::new((self.seed + 7919 * k).unsigned_abs());
        for i in 0..self.x.len() {
            let bird = self.birds[i];
            if !(self.up[i] <= f && f < self.down[i] + 0.5 * ROOST.2) {
                continue;
            }
            let (lx, ly, lz) = self.lead(bird.tribe, across, down, t);
            let (ax, ay, az) = self.lead(bird.tribe, across, down, t + 0.5);
            let (mut dx, dy, dz) = (ax - lx, ay - ly, az - lz);
            let mut d = (dx * dx + dy * dy + dz * dz).sqrt();
            if d < 1e-6 {
                (dx, d) = (1.0, 1.0);
            }
            let v = PACE * u / d;
            self.state[i] = UP;
            self.x[i] = lx + rng.gauss(0.0, 0.55 * u);
            self.y[i] = ly + rng.gauss(0.0, 0.28 * u);
            self.z[i] = lz + rng.gauss(0.0, 0.35 * u);
            (self.vx[i], self.vy[i], self.vz[i]) = (v * dx, v * dy, v * dz);
        }
    }

    /// Where the bird `i` roosts: its perch, or on the land.
    pub fn spot(&self, i: usize, air: &Air) -> Dot {
        if let Some(&(x, y)) = air.roosts.get(i) {
            return (x as f64, y as f64);
        }
        let px = (0.max(2 * air.w as i64 - 1)).min(0.max(self.site[i] as i64));
        (px as f64, air.ground(px) as f64)
    }

    /// Up from its roost, low and along; or in from a side.
    fn launch(&mut self, i: usize, air: &Air, lead: &[Vec3], u: f64) {
        let bird = self.birds[i];
        let (across, down) = (2.0 * air.w as f64, 4.0 * air.over as f64);
        let v = PACE * u;
        let n = |k| noise(i as i64, k);
        let (x, y, vx, vy);
        if self.join && bird.group != 0 {
            let left = bird.group % 2 != 0;
            x = across * (if left { -0.08 } else { 1.08 }) + (n(11) - 0.5) * 0.8 * u;
            y = down * (0.15 + 0.4 * noise(bird.group, 4)) + (n(12) - 0.5) * 0.5 * u;
            (vx, vy) = (if left { v } else { -v }, 0.0);
        } else {
            (x, y) = self.spot(i, air);
            let way = if lead[bird.tribe].0 > x { 1.0 } else { -1.0 };
            let climb = (6.0 + 22.0 * n(13)).to_radians();
            (vx, vy) = (way * v * climb.cos(), -v * climb.sin());
        }
        self.state[i] = UP;
        self.tribe[i] = bird.tribe;
        (self.x[i], self.y[i]) = (x, y);
        self.z[i] = (n(14) - 0.5) * 0.8 * u;
        (self.px[i], self.qx[i]) = (x, x);
        (self.py[i], self.qy[i]) = (y, y);
        (self.vx[i], self.vy[i], self.vz[i]) = (vx, vy, 0.0);
    }

    /// The roosts: the birds in the air, from left to right, go to the
    /// sites from left to right.
    fn deal(&mut self, air: &Air) {
        let across = 2.0 * air.w as f64;
        let mut up: Vec<usize> = (0..self.state.len())
            .filter(|&i| self.state[i] == UP)
            .collect();
        up.sort_by(|&a, &b| self.x[a].partial_cmp(&self.x[b]).unwrap());
        let len = up.len() as i64;
        for (rank, &i) in up.iter().enumerate() {
            let site = rank as i64 * SITES / len;
            let turn = (4 * site + self.seed).rem_euclid(SITES);
            self.down[i] =
                ROOST.0 + ROOST.1 * (turn as f64 + 1.2 * noise(i as i64, 7)) / (SITES as f64 + 0.2);
            self.site[i] = self.home(site, across) + (noise(i as i64, 5) - 0.5) * 0.14 * across;
        }
        self.bed = true;
    }

    /// Now and then a side of the main flock goes its own way.
    fn split(&mut self, t: f64) {
        let slot = (t / SPLIT.0).floor() as i64;
        let seed = self.seed;
        let into = t / SPLIT.0 - slot as f64;
        let on = noise(slot, seed + 1) < SPLIT.1 && (0.2..0.75).contains(&into);
        if !on {
            if self.rift.is_some() {
                self.rift = None;
                self.tribe = self.birds[..self.x.len()].iter().map(|b| b.tribe).collect();
            }
            return;
        }
        if self.rift == Some(slot) {
            return;
        }
        self.rift = Some(slot);
        let mut main: Vec<usize> = (0..self.state.len())
            .filter(|&i| self.state[i] == UP && self.tribe[i] == 0)
            .collect();
        if main.len() < 8 {
            return;
        }
        // Cut across a bearing: those the furthest along it.
        let lean = TAU * noise(slot, seed + 2);
        let (ca, sa) = (lean.cos(), lean.sin());
        let key = |i: usize| self.x[i] * ca + self.y[i] * sa;
        main.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap());
        let share = 0.2 + 0.3 * noise(slot, seed + 3);
        let to = if noise(slot, seed + 4) < 0.5 { 1 } else { 2 };
        let from = (main.len() as f64 * (1.0 - share)) as usize;
        for &i in &main[from..] {
            self.tribe[i] = to;
        }
    }

    /// The hawk: from over the view it stoops at the thick of the flock,
    /// pulls up and goes; at times it comes round again.
    fn hunt(&mut self, t: f64, air: &Air, thick: Option<Dot>, u: f64) {
        let (across, down) = (2.0 * air.w as f64, 4.0 * air.over as f64);
        let (every, long) = HAWK;
        let v = PACE * u;
        let Some(mut h) = self.hawk else {
            let slot = (t / every).floor();
            let k = slot as i64;
            let seed = self.seed;
            let begin = slot * every + (every - long) * (0.2 + 0.6 * noise(k, seed + 5));
            let calm = JOIN.0 + 0.02 < self.f && self.f < ROOST.0 - 0.03;
            let Some(thick) = thick else { return };
            if !(self.hunted && calm && t - DT < begin && begin <= t) {
                return;
            }
            let side = if noise(k, seed + 6) < 0.5 { 1.0 } else { -1.0 };
            let x = thick.0 + side * (0.5 + 0.8 * noise(k, seed + 7)) * u;
            let y = -0.06 * down;
            let again = if noise(k, seed + 8) < 0.35 { 2.0 } else { 1.0 };
            let way = (thick.1 - y).atan2(thick.0 - x);
            self.hawk = Some(Hawk {
                x,
                y,
                way,
                ax: thick.0,
                ay: thick.1,
                away: false,
                born: t,
                passes: again,
                since: t,
            });
            // As if it came along its way.
            let (bx, by) = (2.2 * v * DT * way.cos(), 2.2 * v * DT * way.sin());
            self.trail = [(x - 2.0 * bx, y - 2.0 * by), (x - bx, y - by)];
            return;
        };
        if let Some(thick) = thick {
            h.ax += 0.2 * (thick.0 - h.ax);
            h.ay += 0.2 * (thick.1 - h.ay);
        }
        let (want, rate, speed);
        if !h.away {
            (want, rate, speed) = ((h.ay - h.y).atan2(h.ax - h.x), 1.6, 2.2 * v);
            if (h.ax - h.x).hypot(h.ay - h.y) < 0.3 * u || t - h.since > 5.0 {
                (h.away, h.since, h.passes) = (true, t, h.passes - 1.0);
            }
        } else {
            want = (-0.45f64).atan2(if h.way.cos() >= 0.0 { 1.0 } else { -1.0 });
            rate = if h.y > 0.8 * down { 3.0 } else { 1.1 };
            speed = 1.7 * v;
            if h.passes > 0.0 && t - h.since > 1.6 {
                (h.away, h.since) = (false, t);
            }
        }
        self.trail = [self.trail[1], (h.x, h.y)];
        let turn = (want - h.way + PI).rem_euclid(TAU) - PI;
        h.way += (-rate * DT).max((rate * DT).min(turn));
        h.x += speed * h.way.cos() * DT;
        h.y += speed * h.way.sin() * DT;
        let out = h.x < -0.1 * across || h.x > 1.1 * across || h.y < -0.1 * down;
        self.hawk = if (h.away && out) || t - h.born > long {
            None
        } else {
            Some(h)
        };
    }

    #[allow(clippy::needless_range_loop)]
    fn step(&mut self, air: &Air, scare: Option<Scare>) {
        let k = self.k.expect("started") + 1;
        self.k = Some(k);
        let t = k as f64 * DT;
        let f = air.stage(t);
        self.f = f;
        let n = self.x.len();
        let (across, down) = (2.0 * air.w as f64, 4.0 * air.over as f64);
        if n == 0 || across <= 0.0 {
            return;
        }
        let u = self.reach(across, down);
        let v0 = PACE * u;
        self.qx.copy_from_slice(&self.px);
        self.qy.copy_from_slice(&self.py);
        self.px.copy_from_slice(&self.x);
        self.py.copy_from_slice(&self.y);
        // A rank follows where the lead was: a tribe trails along its way.
        let leads: Vec<Vec<Vec3>> = (0..LEADS.len())
            .map(|j| {
                (0..LAGS.0)
                    .map(|rank| self.lead(j, across, down, t - LAGS.1 * rank as f64))
                    .collect()
            })
            .collect();
        let firsts: Vec<Vec3> = leads.iter().map(|r| r[0]).collect();

        // Who takes off, who heads for its roost.
        if f < ROOST.0 {
            self.bed = false;
        } else if !self.bed {
            self.deal(air);
        }
        for i in 0..n {
            let s = self.state[i];
            if s == DOWN {
                if self.up[i] <= f && f < self.down[i] {
                    self.launch(i, air, &firsts, u);
                }
            } else if s == UP && f >= self.down[i] {
                self.state[i] = IN;
            }
        }
        self.split(t);
        if k.rem_euclid(10) == 0 || self.stray.len() != n {
            let seed = self.seed;
            self.stray = (0..n)
                .map(|i| {
                    let b = self.birds[i];
                    noise(i as i64, seed + ((t + b.beat) / STRAY.0) as i64) < STRAY.1
                })
                .collect();
        }

        // The neighbours, of its tribe: the birds by cell, then by 27.
        let inv = 1.0 / 3.0f64.max(HOOD * u);
        let mut cells = Cells::default();
        let mut keys: Vec<usize> = vec![usize::MAX; n];
        for i in 0..n {
            if self.state[i] != UP {
                continue;
            }
            let (x, y, z) = (self.x[i], self.y[i], self.z[i]);
            let key = (
                (x * inv + 64.0) as i64,
                (y * inv + 64.0) as i64,
                (z * inv + 64.0) as i64,
                self.tribe[i],
            );
            let (vx, vy, vz) = (self.vx[i], self.vy[i], self.vz[i]);
            let at = match cells.at.get(&key) {
                Some(&at) => {
                    let c = &mut cells.sums[at];
                    c[0] += 1.0;
                    c[1] += x;
                    c[2] += y;
                    c[3] += z;
                    c[4] += vx;
                    c[5] += vy;
                    c[6] += vz;
                    at
                }
                None => {
                    cells.at.insert(key, cells.keys.len());
                    cells.keys.push(key);
                    cells.sums.push([1.0, x, y, z, vx, vy, vz]);
                    cells.keys.len() - 1
                }
            };
            keys[i] = at;
        }
        let mut hood: Vec<[f64; 7]> = Vec::with_capacity(cells.keys.len());
        let (mut most, mut thick) = (0.0, None);
        for &(a, b, c, own) in &cells.keys {
            let mut s = [0.0f64; 7];
            for da in -1..=1 {
                for db in -1..=1 {
                    for dc in -1..=1 {
                        if let Some(&at) = cells.at.get(&(a + da, b + db, c + dc, own)) {
                            for (sum, v) in s.iter_mut().zip(cells.sums[at]) {
                                *sum += v;
                            }
                        }
                    }
                }
            }
            let m = s[0];
            hood.push([
                m,
                s[1] / m,
                s[2] / m,
                s[3] / m,
                s[4] / m,
                s[5] / m,
                s[6] / m,
            ]);
            if m > most {
                (most, thick) = (m, Some((s[1] / m, s[2] / m)));
            }
        }

        self.hunt(t, air, thick, u);
        // What scares: where, how far, how much.
        let mut threats: Vec<(f64, f64, f64, f64)> = Vec::new();
        if let Some(s) = scare {
            let gain = 0.5 + 0.5 * s.2;
            threats.push((s.0, s.1, SHY * u * gain, gain));
        }
        if let Some(h) = self.hawk {
            let ahead = 0.45 * 2.0 * v0;
            threats.push((h.x, h.y, WARY * u, 1.0));
            threats.push((
                h.x + ahead * h.way.cos(),
                h.y + ahead * h.way.sin(),
                0.75 * WARY * u,
                0.8,
            ));
        }

        let dt = DT;
        let full = PACK * n as f64;
        let coh = COHERE * v0 * inv;
        let apart = -PART * coh;
        let few = 3.0f64.max(2.5 * full / 27.0);
        let wall = WALL * v0 / u;
        let pull = LEAD * v0;
        let (x0, x1) = (0.04 * across, 0.96 * across);
        let (y0, y1) = (0.08 * down, 0.78 * down);
        let zm = 0.8 * u;
        let (lo, hi) = (0.7 * v0, 1.45 * v0);
        let slow = 0.9 * v0;
        let seconds = air.dusk;
        for i in 0..n {
            let s = self.state[i];
            if s == DOWN {
                continue;
            }
            let (x, y, z) = (self.x[i], self.y[i], self.z[i]);
            let (mut vx, mut vy, mut vz) = (self.vx[i], self.vy[i], self.vz[i]);
            if s == IN {
                // To its roost: straight when the way is not steep, else
                // on along and down, and back.
                let (tx, ty) = self.spot(i, air);
                let left = (self.down[i] + ROOST.2 - f) * seconds;
                let (dx, dy) = (tx - x, ty - y);
                let far = (dx * dx + dy * dy).sqrt();
                let speed = slow.max(far / left.max(dt));
                if left <= dt || far <= speed * dt + 0.5 {
                    self.state[i] = DOWN;
                    (self.x[i], self.y[i]) = (tx, ty);
                    continue;
                }
                let (wx, wy) = if dy > STEEP * dx.abs() && left > 4.0 * dy / v0 + 1.0 {
                    (if vx >= 0.0 { 0.94 } else { -0.94 }, 0.34)
                } else {
                    (dx / far, dy / far)
                };
                let rate = 1.0f64.min((3.0 * dt).max(2.0 * dt / left));
                vx += (wx * speed - vx) * rate;
                vy += (wy * speed - vy) * rate;
                vz *= 0.8;
            } else {
                let bird = self.birds[i];
                let [m, cx, cy, cz, ux, uy, uz] = hood[keys[i]];
                let stray = self.stray[i];
                // With its neighbours.
                let g = if stray { ALIGN * 0.3 } else { ALIGN };
                let (mut ax, mut ay, mut az) = (g * (ux - vx), g * (uy - vy), g * (uz - vz));
                // Towards them when few, away when many.
                let mut g = coh * (1.0 - m / full);
                if g < apart {
                    g = apart;
                } else if stray && g > 0.0 {
                    g = 0.0;
                }
                ax += g * (cx - x);
                ay += g * (cy - y);
                az += g * (cz - z);
                // Never wing to wing.
                let own = cells.sums[keys[i]];
                if own[0] > few {
                    let g = apart * (own[0] - few) / own[0];
                    ax += g * (own[1] / own[0] - x);
                    ay += g * (own[2] / own[0] - y);
                    az += g * (own[3] / own[0] - z);
                }
                // After its lead, when far from it.
                let (lx, ly, lz) = leads[self.tribe[i]][bird.rank];
                let (dx, dy, dz) = (lx - x, ly - y, lz - z);
                let d = (dx * dx + dy * dy + dz * dz).sqrt();
                let slack = (if stray { ROAM } else { SLACK }) * u;
                if d > slack {
                    let g = pull * 1.0f64.min((d - slack) / u) / d;
                    ax += g * dx;
                    ay += g * dy;
                    az += g * dz;
                }
                // Its own mind.
                let a = bird.beat + 0.8 * t * bird.pace;
                let g = (if stray { WANDER * 2.5 } else { WANDER }) * v0;
                ax += g * a.cos();
                ay += 0.6 * g * (1.31 * a).sin();
                az += g * a.sin();
                // Back into the view, and level.
                if x < x0 {
                    ax += wall * (x0 - x);
                } else if x > x1 {
                    ax -= wall * (x - x1);
                }
                if y < y0 {
                    ay += wall * (y0 - y);
                } else if y > y1 {
                    ay -= 0.4 * wall * (y - y1);
                }
                if z < -zm {
                    az -= wall * (z + zm);
                } else if z > zm {
                    az -= wall * (z - zm);
                }
                ay -= LEVEL * vy;
                let mut top = hi;
                for &(hx, hy, r, gain) in &threats {
                    let (ex, ey) = (x - hx, y - hy);
                    let d2 = ex * ex + ey * ey;
                    if d2 < r * r {
                        let d = match d2.sqrt() {
                            0.0 => 1.0,
                            d => d,
                        };
                        let near = gain * (1.0 - d / r);
                        let g = FEAR * v0 * near / d;
                        ax += g * ex;
                        ay += g * ey;
                        top = top.max(hi + DASH * v0 * near);
                    }
                }
                vx += ax * dt;
                vy += ay * dt;
                vz += az * dt;
                // At about its speed: a bird does not stop.
                let speed = match (vx * vx + vy * vy + vz * vz).sqrt() {
                    0.0 => 1.0,
                    s => s,
                };
                let want = speed + (bird.pace * v0 - speed) * 0.8 * dt;
                let want = lo.max(want.min(top));
                let g = want / speed;
                vx *= g;
                vy *= g;
                vz *= g;
            }
            self.x[i] = x + vx * dt;
            self.y[i] = y + vy * dt;
            self.z[i] = z + vz * dt;
            (self.vx[i], self.vy[i], self.vz[i]) = (vx, vy, vz);
        }
    }

    /// Step to `t` seconds; the birds to draw, in dots: in the air, or on
    /// their perch. `scare`: what they flee.
    pub fn fly(&mut self, air: &Air, t: f64, scare: Option<Scare>) -> Vec<Dot> {
        let k = (t / DT + 1e-6).floor() as i64;
        let s = 1.0f64.min(0.0f64.max(t / DT - k as f64));
        self.part = s;
        match self.k {
            Some(at) if (0..=LEAP).contains(&(k - at)) => self.fit(air),
            _ => self.start(k - WARM, air),
        }
        while self.k.is_some_and(|at| at < k) {
            self.step(air, scare);
        }
        let mut out = Vec::new();
        let perched = air.roosts.len();
        let night = self.f >= ROOST.0;
        let r = 1.0 - s;
        let (a, b, c) = (0.5 * r * r, 0.5 + s * r, 0.5 * s * s);
        for i in 0..self.state.len() {
            let (x, y) = (self.x[i], self.y[i]);
            // In the air, or just down: the curve ends on its roost.
            if self.state[i] != DOWN || self.qx[i] != x || self.qy[i] != y {
                out.push((
                    a * self.qx[i] + b * self.px[i] + c * x,
                    a * self.qy[i] + b * self.py[i] + c * y,
                ));
            } else if i < perched && (night || !(self.join && self.birds[i].group != 0)) {
                out.push(self.spot(i, air));
            }
        }
        out
    }

    /// The hawk to draw, in dots, at the time of the last `fly`.
    pub fn hunter(&self) -> Option<Dot> {
        let h = self.hawk?;
        let [(qx, qy), (px, py)] = self.trail;
        Some((glide(qx, px, h.x, self.part), glide(qy, py, h.y, self.part)))
    }
}

/// Dots to perch on: two in five of the lowest of the free cells over
/// the glyphs of `rows`, in a view of `w` by `h`.
pub fn perches(rows: &[String], w: usize, h: usize) -> Vec<(i64, i64)> {
    let mut walls = walls_of(rows);
    walls.sort();
    walls.dedup();
    let shown = clear(rows, w, h);
    let mut out = Vec::new();
    for (x, y) in walls {
        if 0 < y && y < h && x < w && shown[y - 1][x] {
            for i in 0..2 {
                let px = 2 * x as i64 + i;
                if noise(px, y as i64) < 0.4 {
                    out.push((px, 4 * y as i64 - 1));
                }
            }
        }
    }
    out
}

/// Python's `Random.shuffle`.
pub fn shuffle<T>(rng: &mut Twister, xs: &mut [T]) {
    for i in (1..xs.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        xs.swap(i, j);
    }
}

/// The land under the sky: a line of low hills. `water`: a lake before
/// a far shore, the sun's path on it. `reeds`: reeds in the breeze.
pub struct Land {
    seed: i64,
    water: bool,
    reeds: bool,
    rng: Twister,
    blades: Vec<(i64, f64)>,
}

impl Land {
    pub fn new(rng: &mut Twister, water: bool, reeds: bool) -> Land {
        let seed = i64::from(rng.bits(24));
        let own = Twister::new(u64::from(rng.bits(32)));
        Land {
            seed,
            water,
            reeds,
            rng: own,
            blades: Vec::new(),
        }
    }

    /// Rows of cells the land takes in a view `h` high.
    pub fn rows(&self, h: usize) -> usize {
        if self.water {
            return h.min(1 + 3.max(h / 7));
        }
        if self.reeds {
            return h.min(3);
        }
        h.min(if h < 40 { 1 } else { 2 })
    }

    /// The dot row the sun sets on.
    pub fn horizon(&self, h: usize) -> i64 {
        if self.water {
            4 * (h as i64 - self.rows(h) as i64 + 1) - 2
        } else {
            4 * h as i64 - 2
        }
    }

    /// The dot row of the skyline at the dot column `px`.
    pub fn top(&self, px: i64, h: usize) -> i64 {
        let hill = wave(px as f64 / 23.0, self.seed) + 0.5 * wave(px as f64 / 7.0, self.seed + 1);
        if self.water {
            return 4 * (h as i64 - self.rows(h) as i64 + 1) - 1 - (1.6 * hill) as i64;
        }
        let k = if h >= 40 { 2.0 } else { 1.0 };
        4 * h as i64 - 1 - (2.6 * hill * k) as i64
    }

    fn blade(&mut self, px: i64) -> (i64, f64) {
        // In clumps.
        let clump = wave(px as f64 / 17.0, self.seed + 2);
        let rng = &mut self.rng;
        if rng.random() > 1.5 * clump - 0.5 {
            return (0, 0.0);
        }
        let tall = 2 + (rng.random().powf(1.5) * 10.0) as i64;
        (tall, rng.uniform(0.0, TAU))
    }

    /// The land of a view of `w` by `h` cells at `t` seconds, `rows(h)`
    /// rows. `sun`: its dot column, its radius, its colour.
    pub fn picture(
        &mut self,
        w: usize,
        h: usize,
        t: f64,
        wind: f64,
        sun: Option<(f64, f64, u8)>,
        f: f64,
    ) -> Grid {
        let rows = self.rows(h);
        let mut grid = Grid::new(w, rows);
        if w == 0 {
            return grid;
        }
        let g = glow(f);
        let off = 4 * (h - rows) as i64;
        let base = if self.water { 4 } else { 4 * rows as i64 };
        let mut soil = HashMap::new();
        for px in 0..2 * w as i64 {
            for py in 0.max(self.top(px, h) - off)..base {
                or_at(&mut soil, px, py);
            }
        }
        for ((x, y), bits) in soil {
            let warm = sun.is_some_and(|s| g > 0.3 && ((2 * x) as f64 - s.0).abs() < 5.0 * s.1);
            grid.set(x, y, braille(bits), SOIL[usize::from(warm)]);
        }
        if self.water {
            self.lake(&mut grid, w, t, sun, f);
        }
        if self.reeds {
            self.bank(&mut grid, w, t, wind);
        }
        grid
    }

    fn lake(&self, grid: &mut Grid, w: usize, t: f64, sun: Option<(f64, f64, u8)>, f: f64) {
        let (a, g) = (height(f), glow(f));
        let shine = 1.0 - ease((-0.04 - a) / 0.14);
        let deep = 4 * (grid.height() as i64 - 1);
        let mut lit = HashMap::new();
        let mut dim = HashMap::new();
        let across = 2 * w as i64;
        for j in 0..deep {
            let py = 4 + j;
            let far = j as f64 / deep as f64;
            if let Some((sx, r, _)) = sun
                && shine > 0.0
            {
                // The path of light: wider and broken towards the bank.
                let half = r * (0.6 + 1.5 * far);
                let beat = (3.0 * t + 8.0 * noise(j, self.seed)) as i64;
                for px in 0.max((sx - half) as i64)..across.min((sx + half) as i64 + 1) {
                    let near = 1.0 - (px as f64 + 0.5 - sx).abs() / half;
                    let odds = shine * (0.25 + 0.75 * near) * (1.0 - 0.55 * far);
                    if noise(px.div_euclid(2) + 977 * j, beat) < odds {
                        or_at(&mut lit, px, py);
                    }
                }
            }
            // Ripples: dashes that slide, slower far away.
            let slide = (t * (0.5 + 1.5 * far)) as i64;
            for px in 0..across {
                if noise((px - slide).div_euclid(3) + 613 * j, self.seed) < 0.03 + 0.04 * g {
                    or_at(&mut dim, px, py);
                }
            }
        }
        for ((x, y), bits) in dim {
            let ink = WATER[(3.0 * noise(x, y + 40)) as usize];
            grid.set(x, y, braille(bits), ink);
        }
        if let Some((_, _, colour)) = sun {
            for ((x, y), bits) in lit {
                grid.set(x, y, braille(bits), colour);
            }
        }
    }

    fn bank(&mut self, grid: &mut Grid, w: usize, t: f64, wind: f64) {
        let rows = grid.height() as i64;
        while self.blades.len() < 2 * w {
            let b = self.blade(self.blades.len() as i64);
            self.blades.push(b);
        }
        let mut cells = HashMap::new();
        for (i, &(tall, phase)) in self.blades[..2 * w].iter().enumerate() {
            if tall == 0 {
                continue;
            }
            let mut lean = breeze(i as f64, t, wind) * tall as f64 / 5.0;
            if wind != 0.0 {
                lean += 0.3 * (1.7 * t + phase).sin();
            }
            for j in 0..tall {
                let px =
                    i as i64 + (lean * (j as f64 / tall as f64).powf(1.6)).round_ties_even() as i64;
                let py = 4 * rows - 1 - j;
                if (0..2 * w as i64).contains(&px) && py >= 0 {
                    or_at(&mut cells, px, py);
                }
            }
        }
        for ((x, y), mut bits) in cells {
            // In front of the water; one with the land.
            if let Some((g, ink)) = grid.get(x, y)
                && SOIL.contains(&ink)
            {
                bits |= (g as u32 - 0x2800) as u8;
            }
            grid.set(
                x,
                y,
                braille(bits),
                REEDS[(4.0 * noise(x, y + 90)) as usize],
            );
        }
    }
}

/// The options of the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub birds: f64,
    pub dusk: f64,
    pub at: f64,
    pub sun: f64,
    pub wind: f64,
    pub clouds: bool,
    pub water: bool,
    pub reeds: bool,
    pub join: bool,
    pub double: bool,
    pub hawk: bool,
    pub night: bool,
    pub roost: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            birds: 100.0,
            dusk: 300.0,
            at: START,
            sun: 7.0,
            wind: 1.0,
            clouds: false,
            water: false,
            reeds: false,
            join: false,
            double: false,
            hawk: false,
            night: false,
            roost: false,
        }
    }
}

/// The scene and its options: what to draw at a time.
pub struct Scene {
    pub c: Config,
    pub dusk: Dusk,
    pub land: Land,
    pub flock: Flock,
    pub sky: Option<Sky>,
}

impl Scene {
    pub fn new(rng: &mut Twister, c: Config) -> Scene {
        let dusk = Dusk::new(rng, c.clouds);
        let land = Land::new(rng, c.water, c.reeds);
        let second = if c.double { SECOND } else { 0.0 };
        let flock = Flock::new(rng, c.birds, second, c.join, c.hawk);
        let sky = c.night.then(|| Sky::new(rng, 0.0, false, false));
        Scene {
            c,
            dusk,
            land,
            flock,
            sky,
        }
    }

    pub fn stage(&self, t: f64) -> f64 {
        stage(t, self.c.dusk, self.c.at)
    }

    /// Rows of the land in a view `h` high.
    pub fn rows(&self, h: usize) -> usize {
        self.land.rows(h)
    }

    /// The sun's radius, in dots: it grows with the view.
    pub fn radius(&self, w: usize, h: usize) -> f64 {
        self.c.sun * 3.0f64.min(0.7f64.max(((w * h) as f64 / AREA).sqrt()))
    }

    /// The sun: its middle in dots, its colour.
    pub fn sun(&self, w: usize, h: usize, t: f64) -> (f64, f64, u8) {
        let f = self.stage(t);
        let (cx, cy) = sun_at(w, self.land.horizon(h) as f64, self.radius(w, h), f);
        let a = 0.0f64.max(height(f));
        let i = ((1.0 - a).powf(1.5) * SUN.len() as f64) as usize;
        (cx, cy, SUN[(SUN.len() - 1).min(i)])
    }

    /// The sky: stars, the glow, clouds; `h` rows.
    pub fn above(&mut self, w: usize, h: usize, t: f64) -> Grid {
        let f = self.stage(t);
        let over = h.saturating_sub(self.rows(h));
        let mut grid = Grid::new(w, h);
        if let Some(sky) = &mut self.sky {
            let a = height(f);
            let dark = ease((0.3 - a) / 0.5) * (1.0 - ease((f - 1.0) / REST));
            sky.density = 60.0 * dark;
            grid.over(&sky.picture(w, over, t, true, 0.0), 0);
        }
        let (sx, sy, _) = self.sun(w, h, t);
        let low = self.dusk.picture(w, over, f, (sx, sy), t);
        grid.over(&low, 0);
        grid
    }

    /// The birds, then the hawk, in dots, the flock stepped to `t`.
    /// `roosts`: dots to perch on at night. `scare`: the pointer.
    pub fn birds(
        &mut self,
        w: usize,
        h: usize,
        t: f64,
        roosts: &[(i64, i64)],
        scare: Option<Scare>,
    ) -> (Vec<Dot>, Option<Dot>) {
        let air = Air {
            w,
            over: 1.max(h.saturating_sub(self.rows(h))),
            dusk: self.c.dusk,
            at: self.c.at,
            land: Some((&self.land, h)),
            roosts,
        };
        let flown = self.flock.fly(&air, t, scare);
        (flown, self.flock.hunter())
    }

    /// The sun and the birds before it, `h` rows.
    pub fn fore(
        &mut self,
        w: usize,
        h: usize,
        t: f64,
        roosts: &[(i64, i64)],
        scare: Option<Scare>,
    ) -> Grid {
        if w == 0 || h == 0 {
            return Grid::new(w, h);
        }
        let (flown, stoop) = self.birds(w, h, t, roosts, scare);
        self.fore_of(w, h, t, &flown, stoop)
    }

    /// `fore` of the birds `flown` and the hawk `stoop`.
    pub fn fore_of(&self, w: usize, h: usize, t: f64, flown: &[Dot], stoop: Option<Dot>) -> Grid {
        let mut grid = Grid::new(w, h);
        let f = self.stage(t);
        let (cx, cy, colour) = self.sun(w, h, t);
        let r = self.radius(w, h);
        let mut disc = if r > 0.0 {
            disc(cx, cy, r, &self.land, h)
        } else {
            HashMap::new()
        };
        let (across, down) = (2 * w as i64, 4 * h as i64);
        let mut cells: HashMap<(i64, i64), (u8, i64)> = HashMap::new();
        for &(x, y) in flown {
            let (px, py) = (x.floor() as i64, y.floor() as i64);
            if !((0..across).contains(&px) && (0..down).contains(&py)) {
                continue;
            }
            let cell = (px >> 1, py >> 2);
            if let Some(bits) = disc.get_mut(&cell) {
                // Before the sun: a shadow, two dots wide.
                let row = DOTS[(py & 3) as usize];
                *bits &= !(row[0] | row[1]);
                continue;
            }
            let one = cells.entry(cell).or_insert((0, 0));
            one.0 |= dot(px, py);
            one.1 += 1;
        }
        let dim = (1.2 * ease((-0.05 - height(f)) / 0.3)).round_ties_even() as i64;
        for ((x, y), (bits, n)) in cells {
            let ink = BIRDS[0.max(3.min(n - 1) - dim) as usize];
            grid.set(x, y, braille(bits), ink);
        }
        for ((x, y), bits) in disc {
            let g = if bits != 0 { braille(bits) } else { ' ' };
            grid.set(x, y, g, colour);
        }
        if let Some((x, y)) = stoop {
            // Wings swept back: up in a stoop, down in a climb.
            let flip = match self.flock.hawk {
                Some(h) if h.way.sin() < 0.0 => -1,
                _ => 1,
            };
            for (ox, oy) in HAWK_DOTS {
                let (px, py) = (x.floor() as i64 + ox, y.floor() as i64 + flip * oy);
                if (0..across).contains(&px) && (0..down).contains(&py) {
                    let (g, ink) = grid.get(px >> 1, py >> 2).unwrap_or(('\u{2800}', PREY));
                    let mut bits = if ink == PREY {
                        (g as u32 - 0x2800) as u8
                    } else {
                        0
                    };
                    bits |= dot(px, py);
                    grid.set(px >> 1, py >> 2, braille(bits), PREY);
                }
            }
        }
        grid
    }

    /// The land, `rows(h)` rows.
    pub fn below(&mut self, w: usize, h: usize, t: f64) -> Grid {
        let (cx, _, colour) = self.sun(w, h, t);
        let sun = (self.c.sun > 0.0).then(|| (cx, self.radius(w, h), colour));
        let f = self.stage(t);
        self.land.picture(w, h, t, self.c.wind, sun, f)
    }
}

#[cfg(test)]
mod tests;
