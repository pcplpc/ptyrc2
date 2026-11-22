// SPDX-License-Identifier: GPL-3.0-or-later
//! The pond, a function of the time but for the koi and the pointer:
//! drops (`drops`, `Ripple`), the shadows of the words (`shadow`), the
//! pads, the koi (`Koi`), the scene (`Pond::picture`), the dragonflies
//! (`Fly`). Coordinates in dots, 2 by 4 a cell, unless said.

use crate::grid::{Dots, Grid, noise};
use crate::twister::Twister;
use ptyrc_app::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::f64::consts::{PI, TAU};
use std::rc::Rc;

/// Cells the densities count over.
pub const AREA: f64 = 80.0 * 24.0;
/// A ring, as it fades.
pub const RING: [u8; 8] = [152, 109, 103, 67, 66, 60, 59, 238];
/// Light on the swell, by level.
pub const SHEEN: [u8; 3] = [237, 239, 242];
pub const GREENS: [u8; 5] = [65, 65, 71, 101, 108];
pub const BLOOMS: [char; 3] = ['✿', '❀', '✾'];
pub const LOTUS: [u8; 4] = [224, 181, 182, 230];
/// A row in place, one shaken; the glitter, as it fades.
pub const MOON: [u8; 4] = [230, 187, 144, 101];
pub const STRIDER: u8 = 144;
/// A dragonfly: wings out, wings in.
pub const FLIES: [(u8, u8); 3] = [(116, 73), (174, 131), (151, 108)];
pub const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];
/// What floats by, by season of `SEASONS`: colours, dots a piece,
/// pieces over `AREA`.
pub const ADRIFT: [(&[u8], usize, f64); 4] = [
    (&[224, 225, 182, 231], 2, 8.0),
    (&[230, 187, 151], 2, 4.0),
    (&[173, 137, 179, 180, 131], 3, 7.0),
    (&[138, 144, 102], 3, 3.0),
];
/// Coats of a koi, head to tail.
pub const COATS: [&[u8]; 7] = [
    &[173],
    &[216, 216, 173, 173],
    &[254, 254, 167, 167, 167, 254, 254, 167, 167, 254],
    &[254, 167, 167, 254, 254, 254, 167, 254],
    &[222, 222, 179],
    &[254, 254, 245, 254, 245, 245, 254],
    &[253],
];
/// Half the width of a koi along its length; the last two the tail.
pub const BODY: [f64; 13] = [
    1.0, 1.5, 1.8, 1.9, 1.8, 1.6, 1.3, 1.0, 0.8, 0.6, 0.5, 0.9, 1.4,
];

/// How wide a ring is when it dies.
pub const REACH: f64 = 12.0;
/// Share of the life of a drop from a ring to the next.
pub const LAG: f64 = 0.22;
/// Bearings a ring breaks along.
pub const ARCS: usize = 40;
/// The current, dots a second.
pub const FLOW: f64 = 0.6;
/// Seconds a pointer at rest is feared.
pub const CALM: f64 = 3.0;
/// How far a koi keeps from the pointer.
pub const SHY: f64 = 26.0;
/// How far a strider keeps from it.
pub const SKIT: f64 = 16.0;
/// Cells a dragonfly keeps from it.
pub const WARY: f64 = 7.0;
/// Bearings of a shadow.
pub const BINS: usize = 96;
/// Drops a minute over `AREA` at the height of a shower.
pub const RAIN: f64 = 110.0;
/// Seconds from a shower to the next.
pub const SHOWER: f64 = 50.0;
/// Share of its way the moon has made at the start.
pub const RISEN: f64 = 0.2;
/// Share of a night the moon stays out of the water.
pub const DOWN: f64 = 0.2;
/// A drop of rain: its life, its reach, its rings, as `drops` takes them.
pub const SHOWERS: ((f64, f64), (f64, f64), u32) = ((0.7, 1.3), (2.5, 6.0), 1);
/// Radius of the moon.
pub const DISC: f64 = 6.0;
/// Seconds between two rings of a koi.
pub const KISS: (f64, f64) = (6.0, 20.0);

/// Python's `sum` of floats (3.12 on): compensated.
fn fsum(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for x in xs {
        let t = s + x;
        c += if s.abs() >= x.abs() {
            (s - t) + x
        } else {
            (x - t) + s
        };
        s = t;
    }
    if c != 0.0 && c.is_finite() { s + c } else { s }
}

/// A drop at `(x, y)` at `t0`: `rings` rings that go for `life`
/// seconds, the first `reach` dots wide by then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ripple {
    pub key: i64,
    pub x: f64,
    pub y: f64,
    pub t0: f64,
    pub life: f64,
    pub rings: u32,
    pub reach: f64,
}

impl Ripple {
    pub const fn new(
        key: i64,
        x: f64,
        y: f64,
        t0: f64,
        life: f64,
        rings: u32,
        reach: f64,
    ) -> Ripple {
        Ripple {
            key,
            x,
            y,
            t0,
            life,
            rings,
            reach,
        }
    }

    /// Ring `k` at `age` seconds: its radius, its strength from 1 to 0.
    /// It leaves fast and slows down: the cube of what is left of its
    /// time. `None`: not out yet, or gone.
    pub fn ring(&self, age: f64, k: u32) -> Option<(f64, f64)> {
        let late = LAG * f64::from(k);
        let a = (age / self.life - late) / (1.0 - late);
        if !(0.0..1.0).contains(&a) {
            return None;
        }
        Some((
            self.reach * (1.0 - 0.3 * f64::from(k)) * (1.0 - (1.0 - a).powf(3.0)),
            (1.0 - a) * 0.75f64.powf(f64::from(k)),
        ))
    }
}

/// The drops whose rings still go at `t` over `w` by `h` cells: `rate`
/// a minute over `AREA`, a function of the second. A drop is drawn from
/// the second it falls in: the same at every call. `life`, `reach`: of
/// the smallest, of the largest; `rings`: at most.
#[allow(clippy::too_many_arguments)]
pub fn drops(
    seed: i64,
    t: f64,
    w: usize,
    h: usize,
    rate: &dyn Fn(i64) -> f64,
    life: (f64, f64),
    reach: (f64, f64),
    rings: u32,
) -> Vec<Ripple> {
    let (low, high) = life;
    let share = (w * h) as f64 / AREA / 60.0;
    let mut out = Vec::new();
    for second in (t - high).floor() as i64..=t.floor() as i64 {
        let mean = rate(second) * share;
        let slot = seed.wrapping_mul(0x2545F491).wrapping_add(second);
        let whole = mean.trunc();
        let count = whole as i64 + i64::from(noise(slot, 0) < mean - whole);
        for j in 0..count {
            let key = (noise(slot, j + 1) * 4294967296.0) as i64;
            let t0 = second as f64 + noise(key, 1);
            // Most drops are small.
            let size = noise(key, 2).powf(2.0);
            let lasts = low + (high - low) * size;
            if t0 <= t && t < t0 + lasts {
                let n =
                    1 + u32::from(rings > 1 && size > 0.25) + u32::from(rings > 2 && size > 0.6);
                out.push(Ripple::new(
                    key,
                    noise(key, 3) * 2.0 * w as f64,
                    noise(key, 4) * 4.0 * h as f64,
                    t0,
                    lasts,
                    n,
                    reach.0 + (reach.1 - reach.0) * size,
                ));
            }
        }
    }
    out
}

/// `drops` of the twin's defaults: lives 1.4 to 3.2 s, reach 6 to 20,
/// two rings.
pub fn plain_drops(seed: i64, t: f64, w: usize, h: usize, rate: f64) -> Vec<Ripple> {
    drops(seed, t, w, h, &|_| rate, (1.4, 3.2), (6.0, 20.0), 2)
}

/// The glyphs of a view: per row, the columns that hold one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Walls {
    rows: Vec<Vec<i64>>,
}

impl Walls {
    /// The columns of row `y` holding a glyph.
    pub fn get(&self, y: i64) -> &[i64] {
        usize::try_from(y)
            .ok()
            .and_then(|y| self.rows.get(y))
            .map_or(&[], Vec::as_slice)
    }

    /// No glyph.
    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(Vec::is_empty)
    }

    /// The dot `(x, y)` is under a glyph.
    pub fn stone(&self, x: f64, y: f64) -> bool {
        self.get((y as i64) >> 2).contains(&((x as i64) >> 1))
    }

    /// The rows that hold a glyph.
    pub fn held(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&y| !self.rows[y].is_empty())
            .collect()
    }
}

/// The glyphs of a view of `w` by `h` cells holding the text `rows`.
pub fn index(rows: &[String], w: usize, h: usize) -> Walls {
    let mut out = vec![Vec::new(); h];
    for (x, y) in crate::grid::walls_of(rows) {
        if x < w && y < h {
            out[y].push(x as i64);
        }
    }
    Walls { rows: out }
}

/// Per bearing, `BINS` a turn: the distances a ring is hidden from, to.
pub type Shade = (Vec<f64>, Vec<f64>);

/// What the words hide of the rings of a drop at `(x, y)`, as far as
/// `reach`. A ring closes again behind a stone, three times its width
/// further; behind a line it does not. `None`: the drop fell on a word.
pub fn shadow(x: f64, y: f64, reach: f64, walls: &Walls) -> Option<Shade> {
    let mut near = vec![f64::INFINITY; BINS];
    for row in ((y - reach) as i64) >> 2..=((y + reach) as i64) >> 2 {
        for &col in walls.get(row) {
            let (dx, dy) = ((2 * col + 1) as f64 - x, (4 * row + 2) as f64 - y);
            let d = dx.hypot(dy);
            if d > reach + 3.0 {
                continue;
            }
            if d < 3.0 {
                return None;
            }
            let half = (2.3 / d).min(1.0).asin();
            let mid = dy.atan2(dx);
            let first = ((mid - half) / TAU * BINS as f64).floor() as i64;
            let last = ((mid + half) / TAU * BINS as f64).floor() as i64;
            for b in first..=last {
                let b = b.rem_euclid(BINS as i64) as usize;
                near[b] = near[b].min(d - 2.0);
            }
        }
    }
    let mut far = vec![f64::INFINITY; BINS];
    let Some(free) = (0..BINS).find(|&b| near[b] == f64::INFINITY) else {
        return Some((near, far));
    };
    let mut run: Vec<usize> = Vec::new();
    for b in free + 1..=free + BINS {
        let b = b % BINS;
        if near[b] < f64::INFINITY {
            run.push(b);
            continue;
        }
        let width = fsum(run.iter().map(|&i| near[i])) * TAU / BINS as f64;
        for &i in &run {
            far[i] = near[i] + (3.0 * width).max(6.0);
        }
        run.clear();
    }
    Some((near, far))
}

/// The middle of the moon in the water, `f` of its way over `w` by `h`
/// cells: from the left to the right, on a shallow arc.
pub fn moon_at(w: usize, h: usize, r: f64, f: f64) -> (f64, f64) {
    (
        -r + f * (2.0 * w as f64 + 2.0 * r),
        4.0 * h as f64 * (0.55 - 0.25 * (PI * f).sin()),
    )
}

/// The dots of a disc of radius `r` about `(cx, cy)`.
fn disc(water: &mut Dots, cx: f64, cy: f64, r: f64, colour: u8) {
    for py in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
        for px in (cx - r).floor() as i64..=(cx + r).ceil() as i64 {
            let (dx, dy) = (px as f64 + 0.5 - cx, py as f64 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                water.dot(px, py, colour);
            }
        }
    }
}

/// The season of the day `day` of the year (1: 1 January), north.
pub fn season_of(day: i64) -> usize {
    match day {
        355.. => 3,
        266.. => 2,
        172.. => 1,
        80.. => 0,
        _ => 3,
    }
}

/// The day of the year (1: 1 January) of `unix_ms`, `offset_s` east.
pub fn day_of_year(unix_ms: f64, offset_s: f64) -> i64 {
    let days = ((unix_ms / 1000.0 + offset_s) / 86400.0).floor() as i64;
    // Civil from days (Hinnant): the year of the day.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    days - days_from_civil(year) + 1
}

/// Days from 1970-01-01 to the first of January of `year`.
fn days_from_civil(year: i64) -> i64 {
    let y = year - 1;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = 306; // 1 January, from 1 March
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// A lily pad of the map: `(u, v)` the middle of its cluster, `(ox,
/// oy)` from it; `r` its radius, `turn` where its notch points, `bloom`
/// a glyph of `BLOOMS` plus one, or 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad {
    pub u: f64,
    pub v: f64,
    pub ox: f64,
    pub oy: f64,
    pub r: f64,
    pub turn: f64,
    pub phase: f64,
    pub green: u8,
    pub bloom: usize,
    pub petal: u8,
}

/// The dots of a pad of radius `r10` tenths of a dot, about its middle:
/// a disc less a notch towards `turn`, of 32 a turn.
fn leaf(r10: i64, turn: i64) -> Vec<(i64, i64)> {
    let r = r10 as f64 / 10.0;
    let span = r.ceil() as i64;
    let mut out = Vec::new();
    for dy in -span..=span {
        for dx in -span..=span {
            let d = (dx as f64).hypot(dy as f64);
            if d > r {
                continue;
            }
            let off = ((dy as f64).atan2(dx as f64) - TAU * turn as f64 / 32.0).rem_euclid(TAU);
            if d > 0.9 && off.min(TAU - off) < 0.34 {
                continue;
            }
            out.push((dx, dy));
        }
    }
    out
}

/// The pointer as felt: its dot, the fright from 1 to 0.
pub type Scare = (f64, f64, f64);

/// A koi under the surface: it wanders, turns away from the words and
/// the banks, and trails its body along the way it came.
#[derive(Clone, Debug)]
pub struct Koi {
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    /// Radians a second.
    pub turn: f64,
    /// Dots a second.
    pub speed: f64,
    pub size: f64,
    pub coat: &'static [u8],
    pub beat: f64,
    /// Seconds to its next ring.
    pub kiss: f64,
    /// Its fright, 1 to 0: it swims faster.
    pub dash: f64,
    /// Where its head was, a dot apart, the last first.
    pub trail: VecDeque<(f64, f64)>,
}

/// Dots of the trail of a koi.
const TRAIL: usize = 24;

impl Koi {
    pub fn new(rng: &mut Twister, w: usize, h: usize) -> Koi {
        let x = rng.uniform(0.0, 2.0 * w as f64);
        let y = rng.uniform(0.0, 4.0 * h as f64);
        let heading = rng.uniform(0.0, TAU);
        let speed = rng.uniform(2.6, 4.6);
        let size = rng.uniform(1.2, 1.7);
        let coat = rng.choice(&COATS);
        let beat = rng.uniform(0.0, TAU);
        let kiss = rng.uniform(2.0, KISS.1);
        Koi {
            x,
            y,
            heading,
            turn: 0.0,
            speed,
            size,
            coat,
            beat,
            kiss,
            dash: 0.0,
            trail: VecDeque::from([(x, y)]),
        }
    }

    /// Swims for `dt` seconds over `w` by `h` cells; `stone(x, y)`: the
    /// dot is under a word. Within `SHY` of `scare` it turns its tail to
    /// the pointer and darts.
    pub fn step(
        &mut self,
        rng: &mut Twister,
        dt: f64,
        w: usize,
        h: usize,
        stone: &dyn Fn(f64, f64) -> bool,
        scare: Option<Scare>,
    ) {
        let (across, down) = (2.0 * w as f64, 4.0 * h as f64);
        let shut = |k: &Koi, angle: f64| {
            let x = k.x + 10.0 * (k.heading + angle).cos();
            let y = k.y + 10.0 * (k.heading + angle).sin();
            !((-4.0..across + 4.0).contains(&x) && (-4.0..down + 4.0).contains(&y)) || stone(x, y)
        };
        self.turn += (rng.gauss(0.0, 2.2) - 1.4 * self.turn) * dt;
        self.dash *= (-dt / 0.8).exp();
        let mut limit = 1.6;
        if let Some((sx, sy, fright)) = scare
            && (self.x - sx).hypot(self.y - sy) < SHY
        {
            let away = (self.y - sy).atan2(self.x - sx);
            let off = (away - self.heading + PI).rem_euclid(TAU) - PI;
            limit = 1.6 + 4.4 * fright;
            self.turn = (6.0 * off).min(limit).max(-limit);
            self.dash = self.dash.max(fright);
        }
        if !((0.0..across).contains(&self.x) && (0.0..down).contains(&self.y)) {
            // Out, after a resize: back to the middle.
            let home = (down / 2.0 - self.y).atan2(across / 2.0 - self.x);
            let off = (home - self.heading + PI).rem_euclid(TAU) - PI;
            self.turn = (2.0 * off).clamp(-1.6, 1.6);
        } else {
            let (left, ahead, right) = (shut(self, -0.6), shut(self, 0.0), shut(self, 0.6));
            if left && !right {
                self.turn += 5.0 * dt;
            } else if right && !left {
                self.turn -= 5.0 * dt;
            } else if ahead {
                self.turn += if self.turn >= 0.0 { 5.0 } else { -5.0 } * dt;
            }
        }
        self.turn = self.turn.min(limit).max(-limit);
        self.heading += self.turn * dt;
        let pace = self.speed * (1.0 + 2.5 * self.dash) * dt;
        self.x += pace * self.heading.cos();
        self.y += pace * self.heading.sin();
        let (mut px, mut py) = self.trail[0];
        let mut gone = (self.x - px).hypot(self.y - py);
        if gone > 8.0 {
            // A jump: the tail does not stretch over it.
            self.trail.clear();
            self.trail.push_back((self.x, self.y));
        }
        while gone >= 1.0 {
            px += (self.x - px) / gone;
            py += (self.y - py) / gone;
            self.trail.push_front((px, py));
            self.trail.truncate(TRAIL);
            gone -= 1.0;
        }
        self.kiss -= dt;
    }

    /// Its body, tail first: a disc a point of `BODY`, swung across its
    /// way more and more towards the tail.
    pub fn draw(&self, water: &mut Dots, t: f64) {
        let n = BODY.len();
        let trail = &self.trail;
        for i in (0..n).rev() {
            let j = (i as f64 * self.size).round_ties_even() as usize;
            if j >= trail.len() {
                continue;
            }
            let (mut x, mut y) = trail[j];
            let (ax, ay) = trail[j.saturating_sub(1)];
            let (bx, by) = trail[(j + 1).min(trail.len() - 1)];
            let mut way = (ax - bx).hypot(ay - by);
            if way == 0.0 {
                way = 1.0;
            }
            let f = i as f64 / n as f64;
            let swing = 1.3 * f * f * (1.4 * self.speed * t + self.beat - 5.0 * f).sin();
            x += swing * (by - ay) / way;
            y += swing * (ax - bx) / way;
            let colour = self.coat[i * self.coat.len() / n];
            disc(
                water,
                x,
                y,
                (BODY[i] * self.size.powf(0.7)).max(0.8),
                colour,
            );
        }
    }
}

/// What the pond shows; the twin's keywords of `Pond`.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Lily pads over `AREA`.
    pub pads: f64,
    /// Drops a minute over `AREA`.
    pub drops: f64,
    pub koi: bool,
    pub lotus: bool,
    pub rain: bool,
    pub moon: bool,
    pub adrift: bool,
    pub striders: bool,
    /// Of the moon.
    pub phase: f64,
    /// Seconds the moon takes to cross.
    pub night: f64,
    /// Of `SEASONS`.
    pub season: usize,
    /// Dots a second the water goes by.
    pub current: f64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            pads: 6.0,
            drops: 10.0,
            koi: false,
            lotus: false,
            rain: false,
            moon: false,
            adrift: false,
            striders: false,
            phase: 0.5,
            night: 300.0,
            season: 1,
            current: FLOW,
        }
    }
}

/// Light on the swell: where it starts, how dull.
type Glint = (f64, f64, f64);
/// What floats: pace, sway, where it starts, colour, turn.
type Piece = (f64, f64, f64, f64, u8, f64);

/// The scene: what the water shows at a time. The same seed gives the
/// same pond, at every size. The pointer (`touch`) is feared for `CALM`
/// seconds after its last move (`fear`).
#[derive(Clone, Debug)]
pub struct Pond {
    pub c: Config,
    pub seed: i64,
    pads_rng: Twister,
    fish: Twister,
    /// Where the water goes.
    pub current: f64,
    /// The current, dots a second across and down.
    pub flow: (f64, f64),
    pub pointer: Option<(f64, f64, f64)>,
    pub pads: Vec<Pad>,
    cluster: Vec<Pad>,
    left: i64,
    at: (f64, f64),
    pub koi: Option<Vec<Koi>>,
    pub kisses: Vec<Ripple>,
    walls: Option<Walls>,
    shadows: HashMap<i64, Option<Rc<Shade>>>,
    // What does not change with the time, by size: worked out once.
    glints: ((usize, usize), Vec<Glint>),
    drift: ((usize, usize, usize), Vec<Piece>),
    leaves: HashMap<(i64, i64), Vec<(i64, i64)>>,
    arcs: HashMap<(i64, u32), Vec<f64>>,
    unit: Vec<(f64, f64)>,
}

impl Pond {
    pub fn new(rng: &mut Twister, c: Config) -> Pond {
        let seed = i64::from(rng.bits(30));
        let pads_rng = Twister::new(u64::from(rng.bits(32)));
        let fish = Twister::new(u64::from(rng.bits(32)));
        let current = rng.uniform(0.0, TAU);
        let flow = (c.current * current.cos(), c.current * current.sin());
        let koi = c.koi.then(Vec::new);
        Pond {
            c,
            seed,
            pads_rng,
            fish,
            current,
            flow,
            pointer: None,
            pads: Vec::new(),
            cluster: Vec::new(),
            left: 0,
            at: (0.0, 0.0),
            koi,
            kisses: Vec::new(),
            walls: None,
            shadows: HashMap::new(),
            glints: ((usize::MAX, 0), Vec::new()),
            drift: ((usize::MAX, 0, 0), Vec::new()),
            leaves: HashMap::new(),
            arcs: HashMap::new(),
            unit: (0..512)
                .map(|i| {
                    let a = TAU * i as f64 / 512.0;
                    (a.cos(), a.sin())
                })
                .collect(),
        }
    }

    /// The pointer came on the dot `(x, y)` at `t`.
    pub fn touch(&mut self, x: f64, y: f64, t: f64) {
        self.pointer = Some((x, y, t));
    }

    /// The pointer as felt at `t`: its dot, and the fright from 1, as it
    /// moves, to 0, `CALM` seconds after. `None`: none is felt.
    pub fn fear(&self, t: f64) -> Option<Scare> {
        let (x, y, t0) = self.pointer?;
        let f = 1.0 - (t - t0) / CALM;
        if !(f > 0.0 && f <= 1.0) {
            return None;
        }
        Some((x, y, f * f * (3.0 - 2.0 * f)))
    }

    fn pad(&mut self) -> Pad {
        let rng = &mut self.pads_rng;
        if self.left <= 0 {
            self.cluster.clear();
            self.left = rng.choice(&[1, 2, 2, 3, 3, 4, 5]);
            let u = rng.random();
            self.at = (u, rng.random());
        }
        self.left -= 1;
        let r = 2.4 + 3.2 * rng.random().powf(1.6);
        let (mut ox, mut oy) = (0.0, 0.0);
        for tries in 0..48 {
            if (self.cluster.iter()).all(|p| (ox - p.ox).hypot(oy - p.oy) >= r + p.r + 0.6) {
                break;
            }
            // Further out at each try: one fits in the end.
            let far = r + 3.0 + 2.0 * self.cluster.len() as f64 + tries as f64;
            ox = rng.uniform(-far, far);
            oy = rng.uniform(-far, far);
        }
        let turn = rng.uniform(0.0, TAU);
        let phase = rng.uniform(0.0, TAU);
        let green = rng.choice(&GREENS);
        let glyph = rng.randrange(1, BLOOMS.len() as i64 + 1) as usize;
        let bloom = if r >= 3.4 && rng.random() < 0.45 {
            glyph
        } else {
            0
        };
        let petal = rng.choice(&LOTUS);
        let pad = Pad {
            u: self.at.0,
            v: self.at.1,
            ox,
            oy,
            r,
            turn,
            phase,
            green,
            bloom,
            petal,
        };
        self.cluster.push(pad);
        pad
    }

    /// The pads over `w` by `h` cells at `t`, each with the dot of its
    /// middle: it sways by a dot.
    pub fn leaves(&mut self, w: usize, h: usize, t: f64) -> Vec<(i64, i64, Pad)> {
        let n = (self.c.pads * w as f64 * h as f64 / AREA) as usize;
        while self.pads.len() < n {
            let pad = self.pad();
            self.pads.push(pad);
        }
        (self.pads[..n].iter())
            .map(|p| {
                let x = p.u * 2.0 * w as f64 + p.ox + 0.7 * (0.31 * t + p.phase).sin();
                let y = p.v * 4.0 * h as f64 + p.oy + 0.7 * (0.23 * t + 2.0 * p.phase).cos();
                (x.round_ties_even() as i64, y.round_ties_even() as i64, *p)
            })
            .collect()
    }

    /// Drops a minute over `AREA` at `second`, with `rain`: showers come
    /// and go.
    pub fn shower(seed: i64, second: i64) -> f64 {
        let wave = 0.5 + 0.5 * (TAU * second as f64 / SHOWER + seed as f64).sin();
        RAIN * (0.12 + 0.88 * wave * wave)
    }

    /// The water striders at `t`: where each is, and the ring of its last
    /// push. One rests, adrift on the current, then darts to a place
    /// near its home that `dry` does not hold.
    pub fn skaters(
        &self,
        w: usize,
        h: usize,
        t: f64,
        dry: &dyn Fn(f64, f64) -> bool,
    ) -> Vec<(f64, f64, Ripple)> {
        let (fx, fy) = self.flow;
        let (across, down) = (2.0 * w as f64, 4.0 * h as f64);
        let n = ((3 * w * h) as f64 / AREA) as usize;
        (0..n.max(1))
            .map(|i| {
                let key = (noise(self.seed, 7000 + i as i64) * 4294967296.0) as i64;
                let rest = 1.6 + 2.2 * noise(key, 1);
                let beat = t / rest + noise(key, 2);
                let hop = beat.floor() as i64;
                let place = |hop: i64| {
                    let (mut x, mut y) = (0.0, 0.0);
                    for k in 0..4 {
                        x = noise(key, 3) * across + 18.0 * (noise(key, 8 * hop + k) - 0.5);
                        y = noise(key, 4) * down + 18.0 * (noise(key, 8 * hop + k + 4) - 0.5);
                        if !dry(x, y) {
                            break;
                        }
                    }
                    (x, y)
                };
                let ((x0, y0), (x1, y1)) = (place(hop - 1), place(hop));
                // Where the rest left it.
                let (x0, y0) = (x0 + fx * rest, y0 + fy * rest);
                let since = (beat - hop as f64) * rest;
                let f = (since / 0.25).min(1.0);
                let f = f * (2.0 - f);
                (
                    x0 + (x1 - x0) * f + fx * since,
                    y0 + (y1 - y0) * f + fy * since,
                    Ripple::new(key + hop, x0, y0, t - since, 0.9, 1, 4.0),
                )
            })
            .collect()
    }

    /// Moves the koi by `dt` seconds, to the time `t`.
    pub fn step(&mut self, dt: f64, w: usize, h: usize, t: f64, walls: Option<&Walls>) {
        self.kisses.retain(|one| t < one.t0 + one.life);
        let scare = self.fear(t);
        let Some(koi) = self.koi.as_mut() else {
            return;
        };
        let fish = &mut self.fish;
        let want = (2.0 * ((w * h) as f64 / AREA).sqrt())
            .round_ties_even()
            .clamp(1.0, 7.0) as usize;
        while koi.len() < want {
            koi.push(Koi::new(fish, w, h));
        }
        koi.truncate(want);
        let stone = |x: f64, y: f64| walls.is_some_and(|w| w.stone(x, y));
        for one in koi.iter_mut() {
            let calm = one.dash < 0.15;
            one.step(fish, dt, w, h, &stone, scare);
            // A ring where it is, now and then; one where it took fright.
            if one.kiss <= 0.0 || (calm && one.dash > 0.5) {
                one.kiss = fish.uniform(KISS.0, KISS.1);
                let key = i64::from(fish.bits(30));
                (self.kisses).push(Ripple::new(key, one.x, one.y, t, 2.0, 2, 9.0));
            }
        }
    }

    fn rings(&mut self, water: &mut Dots, ripples: &[Ripple], t: f64, walls: Option<&Walls>) {
        if self.walls.as_ref() != walls {
            self.walls = walls.cloned();
            self.shadows.clear();
        }
        let walls = walls.filter(|w| !w.is_empty());
        let mut shadows: HashMap<i64, Option<Rc<Shade>>> = HashMap::new();
        let mut rings: Vec<(f64, f64, u32, Ripple)> = Vec::new();
        for one in ripples {
            if let Some(walls) = walls {
                let shade = match self.shadows.get(&one.key) {
                    Some(shade) => shade.clone(),
                    None => shadow(one.x, one.y, one.reach, walls).map(Rc::new),
                };
                let none = shade.is_none();
                shadows.insert(one.key, shade);
                if none {
                    continue;
                }
            }
            let age = t - one.t0;
            if !(0.0..one.life).contains(&age) {
                continue;
            }
            for k in 0..one.rings {
                if let Some((r, strength)) = one.ring(age, k) {
                    rings.push((strength, r, k, *one));
                }
            }
        }
        // The strongest last: it has the cells it shares.
        rings.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (strength, r, k, one) in rings {
            let colour = RING[((1.0 - strength).powf(1.3) * (RING.len() as f64 - 0.01)) as usize];
            if r < 1.0 {
                water.dot(one.x.floor() as i64, one.y.floor() as i64, colour);
                continue;
            }
            let shade = shadows.get(&one.key).and_then(Option::as_deref);
            let whole = (0.5 + strength).min(1.0);
            // Whole as it leaves; then every other dot, then one in three.
            let step = if strength > 0.8 || r < 3.0 {
                1
            } else if strength > 0.35 {
                2
            } else {
                3
            };
            if self.arcs.len() > 4096 {
                self.arcs.clear();
            }
            let arcs = (self.arcs.entry((one.key, k))).or_insert_with(|| {
                (0..ARCS)
                    .map(|a| noise(one.key, 64 * i64::from(k) + a as i64))
                    .collect()
            });
            let n = ((TAU * r) as usize).max(8);
            for i in (0..n).step_by(step) {
                if arcs[i * ARCS / n] >= whole {
                    continue;
                }
                if let Some((near, far)) = shade {
                    let b = i * BINS / n;
                    if near[b] <= r && r < far[b] {
                        continue;
                    }
                }
                let (cos, sin) = self.unit[i * 512 / n];
                water.dot(
                    (one.x + r * cos).floor() as i64,
                    (one.y + r * sin).floor() as i64,
                    colour,
                );
            }
        }
        self.shadows = shadows;
    }

    /// Light on the swell: dashes adrift on the current, that show where
    /// two waves meet.
    fn sheen(&mut self, water: &mut Dots, w: usize, h: usize, t: f64) {
        let (fx, fy) = self.flow;
        let (across, down) = (2.0 * w as f64, 4.0 * h as f64);
        if self.glints.0 != (w, h) {
            let n = (90.0 * (w * h) as f64 / AREA) as i64;
            let seed = self.seed;
            self.glints = (
                (w, h),
                (0..n)
                    .map(|i| {
                        (
                            noise(seed, 2 * i + 100000) * across,
                            noise(seed, 2 * i + 100001) * down,
                            0.2 * noise(i, 5),
                        )
                    })
                    .collect(),
            );
        }
        for &(x0, y0, dull) in &self.glints.1 {
            let x = (x0 + fx * t).rem_euclid(across) as i64;
            let y = (y0 + fy * t).rem_euclid(down) as i64;
            let (xf, yf) = (x as f64, y as f64);
            let mut wave = (0.07 * xf + 0.045 * yf - 0.5 * t).sin();
            wave += (-0.035 * xf + 0.09 * yf - 0.37 * t + 1.7).sin();
            let level = ((wave - 1.15 - dull) / 0.25) as i64;
            if level < 0 {
                continue;
            }
            let level = level.min(SHEEN.len() as i64 - 1);
            for dx in 0..=level {
                water.dot(x + dx, y, SHEEN[level as usize]);
            }
        }
    }

    fn moon(&self, water: &mut Dots, w: usize, h: usize, t: f64) {
        let f = (t / self.c.night + RISEN).rem_euclid(1.0 + DOWN);
        if f > 1.0 {
            return;
        }
        let (r, phase) = (DISC, self.c.phase);
        let (cx, cy) = moon_at(w, h, r, f);
        let edge = (TAU * phase).cos();
        // The share of the disc that shows.
        let lit = 0.5 - 0.5 * edge;
        for py in (cy - 2.6 * r).floor() as i64..=(cy + 2.6 * r).ceil() as i64 {
            let pf = py as f64;
            let y = pf + 0.5 - cy;
            let shift = (1.5 * (0.8 * pf + 1.9 * t).sin() + 0.8 * (0.31 * pf - 1.3 * t + 2.0).sin())
                .round_ties_even() as i64;
            if y.abs() >= r {
                // The glitter: dashes over and under, fewer further out.
                let out = (y.abs() - r) / (1.6 * r);
                if noise(py, (1.5 * t + 7.0 * noise(py, 0)) as i64) < out + 0.35 / (lit + 0.05) {
                    continue;
                }
                let reach =
                    ((lit * r * (1.0 - out) * noise(py, 3)).round_ties_even() as i64).max(1);
                let colour = MOON[2 + usize::from(out > 0.5)];
                for px in -reach..=reach {
                    water.dot(cx.round_ties_even() as i64 + 2 * shift + px, py, colour);
                }
                continue;
            }
            if (0.9 * pf - 2.3 * t).sin() > 0.86 {
                continue;
            }
            let half = (r * r - y * y).sqrt();
            for px in (cx - half).floor() as i64..=(cx + half).ceil() as i64 {
                let x = px as f64 + 0.5 - cx;
                if x.abs() > half {
                    continue;
                }
                let on = if phase < 0.5 {
                    x > edge * half
                } else {
                    x < -edge * half
                };
                if on {
                    water.dot(px + shift, py, MOON[usize::from(shift.abs() > 1)]);
                }
            }
        }
    }

    fn adrift(&mut self, water: &mut Dots, w: usize, h: usize, t: f64) {
        let (colours, size, density) = ADRIFT[self.c.season];
        let (across, down) = (2.0 * w as f64, 4.0 * h as f64);
        if self.drift.0 != (w, h, self.c.season) {
            let n = (density * w as f64 * h as f64 / AREA) as i64;
            let seed = self.seed;
            self.drift = (
                (w, h, self.c.season),
                (0..n)
                    .map(|i| {
                        let key = (noise(seed, 5000 + i) * 4294967296.0) as i64;
                        (
                            0.8 + 0.8 * noise(key, 1),
                            TAU * noise(key, 2),
                            noise(key, 3) * across,
                            noise(key, 4) * down,
                            colours[key as usize % colours.len()],
                            4.0 * noise(key, 5),
                        )
                    })
                    .collect(),
            );
        }
        for &(pace, sway, x, y, colour, turn) in &self.drift.1 {
            let mut x = x + pace * t * self.flow.0;
            let mut y = y + pace * t * self.flow.1;
            x += 3.0 * (0.17 * t + sway).sin();
            y += 2.0 * (0.13 * t + sway).cos();
            let (px, py) = (
                x.rem_euclid(across).floor() as i64,
                y.rem_euclid(down).floor() as i64,
            );
            // It turns as it goes.
            let spin = ((0.12 * t + turn) as i64).rem_euclid(4) as usize;
            let (dx, dy) = [(1, 0), (1, 1), (0, 1), (-1, 1)][spin];
            for k in 0..size as i64 {
                water.dot(px + k * dx, py + k * dy, colour);
            }
        }
    }

    /// The ripples drawn at `t`: the drops off the pads, the showers', the
    /// koi's, and the striders with the ring of their last push.
    pub fn ripples(
        &self,
        w: usize,
        h: usize,
        t: f64,
        leaves: &[(i64, i64, Pad)],
    ) -> (Vec<Ripple>, Vec<(f64, f64, Ripple)>) {
        let dry = |x: f64, y: f64| {
            (leaves.iter()).any(|&(px, py, p)| (x - px as f64).hypot(y - py as f64) < p.r + 1.5)
        };
        let mut ripples: Vec<Ripple> = plain_drops(self.seed, t, w, h, self.c.drops)
            .into_iter()
            .filter(|one| !dry(one.x, one.y))
            .collect();
        if self.c.rain {
            let seed = self.seed;
            let ((l0, l1), (r0, r1), n) = SHOWERS;
            let rain = drops(
                seed + 1,
                t,
                w,
                h,
                &|s| Pond::shower(seed, s),
                (l0, l1),
                (r0, r1),
                n,
            );
            ripples.extend(rain.into_iter().filter(|one| !dry(one.x, one.y)));
        }
        ripples.extend_from_slice(&self.kisses);
        let mut skaters = if self.c.striders {
            self.skaters(w, h, t, &dry)
        } else {
            Vec::new()
        };
        if let Some((sx, sy, fright)) = self.fear(t) {
            // A strider slides off the pointer, and back as it calms.
            for one in &mut skaters {
                let (x, y) = (one.0, one.1);
                let mut d = (x - sx).hypot(y - sy);
                if d == 0.0 {
                    d = 1.0;
                }
                if d < SKIT {
                    let k = fright * (SKIT - d) / d;
                    one.0 = x + k * (x - sx);
                    one.1 = y + k * (y - sy);
                }
            }
        }
        ripples.extend(skaters.iter().map(|s| s.2));
        (ripples, skaters)
    }

    /// The pond over `w` by `h` cells at `t` seconds; `walls`: the glyphs
    /// of the view.
    pub fn picture(&mut self, w: usize, h: usize, t: f64, walls: Option<&Walls>) -> Grid {
        let mut grid = Grid::new(w, h);
        if w == 0 || h == 0 {
            return grid;
        }
        let mut water = Dots::new(w, h, false);
        let leaves = self.leaves(w, h, t);
        self.sheen(&mut water, w, h, t);
        let (ripples, skaters) = self.ripples(w, h, t, &leaves);
        self.rings(&mut water, &ripples, t, walls);
        for fish in self.koi.iter().flatten() {
            fish.draw(&mut water, t);
        }
        if self.c.moon {
            self.moon(&mut water, w, h, t);
        }
        if self.c.adrift {
            self.adrift(&mut water, w, h, t);
        }
        for &(px, py, pad) in &leaves {
            let turn = pad.turn + 0.3 * (0.11 * t + pad.phase).sin();
            let key = (
                (10.0 * pad.r).round_ties_even() as i64,
                ((turn / TAU * 32.0).round_ties_even() as i64).rem_euclid(32),
            );
            let dots = self.leaves.entry(key).or_insert_with(|| leaf(key.0, key.1));
            for &(dx, dy) in dots.iter() {
                water.dot(px + dx, py + dy, pad.green);
            }
        }
        for &(x, y, _) in &skaters {
            let (px, py) = (x.floor() as i64, y.floor() as i64);
            for (dx, dy) in [(0, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                water.dot(px + dx, py + dy, STRIDER);
            }
        }
        water.onto(&mut grid);
        if self.c.lotus {
            let (across, down) = (2 * w as i64, 4 * h as i64);
            for &(px, py, pad) in &leaves {
                if pad.bloom > 0 && (0..across).contains(&px) && (0..down).contains(&py) {
                    grid.set(px >> 1, py >> 2, BLOOMS[pad.bloom - 1], pad.petal);
                }
            }
        }
        grid
    }
}

/// A dragonfly: it hovers, then darts to another place. In cells.
#[derive(Clone, Debug)]
pub struct Fly {
    pub x: f64,
    pub y: f64,
    pub goal: (f64, f64),
    /// Seconds it still hovers.
    pub wait: f64,
    pub inks: (u8, u8),
    pub sprite: Option<u64>,
    pub cell: Option<(i64, i64)>,
}

impl Fly {
    pub fn new(rng: &mut Twister, w: usize, h: usize) -> Fly {
        let x = rng.uniform(0.0, w as f64);
        let y = rng.uniform(0.0, h as f64);
        let wait = rng.uniform(0.5, 3.0);
        let inks = rng.choice(&FLIES);
        Fly {
            x,
            y,
            goal: (x, y),
            wait,
            inks,
            sprite: None,
            cell: None,
        }
    }

    pub fn frames(&self) -> Value {
        let (out, wings) = self.inks;
        json!([
            {"cells": [[{"t": "‡", "fg": out, "bold": true}]], "ms": 90},
            {"cells": [[{"t": "╪", "fg": wings}]], "ms": 90},
        ])
    }

    /// Flies for `dt` seconds; the cell it is in. `scare`: the pointer,
    /// a cell and a fright: it leaves a place within `WARY` cells of it.
    pub fn step(
        &mut self,
        rng: &mut Twister,
        dt: f64,
        w: usize,
        h: usize,
        scare: Option<Scare>,
    ) -> (i64, i64) {
        let (wf, hf) = (w as f64, h as f64);
        if let Some((sx, sy, fright)) = scare
            && fright > 0.3
        {
            let (dx, dy) = (self.goal.0 - sx, 2.0 * (self.goal.1 - sy));
            if dx.hypot(dy) < WARY {
                let mut way = if dx != 0.0 || dy != 0.0 {
                    dy.atan2(dx)
                } else {
                    rng.uniform(0.0, TAU)
                };
                way += rng.uniform(-0.7, 0.7);
                let far = rng.uniform(1.6, 2.6) * WARY;
                self.goal = (sx + far * way.cos(), sy + far * way.sin() / 2.0);
                self.wait = rng.uniform(0.8, 3.0);
            }
        }
        let (gx, gy) = self.goal;
        let far = (gx - self.x).hypot(2.0 * (gy - self.y));
        if far > 0.5 {
            // Cells a second, a row two.
            let go = (34.0 * dt / far).min(1.0);
            self.x += (gx - self.x) * go;
            self.y += (gy - self.y) * go;
        } else {
            self.wait -= dt;
            if self.wait <= 0.0 {
                self.wait = rng.uniform(0.8, 5.0);
                if rng.random() < 0.35 {
                    // A long way; else a hop.
                    let x = rng.uniform(0.0, wf);
                    self.goal = (x, rng.uniform(0.0, hf));
                } else {
                    let x = self.x + rng.uniform(-9.0, 9.0);
                    self.goal = (x, self.y + rng.uniform(-3.0, 3.0));
                }
            }
        }
        self.goal = (
            self.goal.0.min(wf - 0.01).max(0.0),
            self.goal.1.min(hf - 0.01).max(0.0),
        );
        (
            (self.x as i64).min(w as i64 - 1).max(0),
            (self.y as i64).min(h as i64 - 1).max(0),
        )
    }
}

#[cfg(test)]
mod tests;
