// SPDX-License-Identifier: GPL-3.0-or-later
//! The shore, a function of the time: the twin's `Shore`, `Wave`,
//! `swash`, `tide`, `light`, `scare`, `lace`, `grit`, `rocks_of`. The
//! sand at the top, the sea at the bottom; lengths in Braille dots, 2 by
//! 4 a cell, `y` down.

use crate::grid::{Grid, braille, noise, walls_of};
use crate::twister::Twister;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::{PI, TAU};
use std::rc::Rc;
use std::sync::OnceLock;

/// A colour by light: day, dusk, night.
pub type Ink = [u8; 3];

/// Cells `--sand` counts over.
pub const AREA: f64 = 80.0 * 24.0;
/// Dots a side of the foam net.
pub const TILE: usize = 64;
/// Share of its time on the sand a wave takes to run up.
pub const PEAK: f64 = 0.3;
/// Time a wave stays on the sand, in wave periods.
pub const STAY: f64 = 1.05;
/// Wave periods a line of foam lasts.
pub const LIFE: f64 = 4.0;
/// Wave periods the sand stays wet.
pub const DRY: f64 = 4.0;
/// The still-water line, down the pane: high, low.
pub const HIGH: f64 = 0.52;
pub const LOW: f64 = 0.82;
/// Seconds between two walkers; a step; dots a step.
pub const WALK: f64 = 50.0;
pub const STEP: f64 = 0.5;
pub const STRIDE: i64 = 5;
/// Columns a group of sandpipers.
pub const PACK: usize = 26;
/// Dots a sandpiper keeps from the water, at least.
pub const OFF: f64 = 8.0;
/// Share of its time a group is up the sand; of a change. A bird alone.
pub const ROAM: (f64, f64) = (0.35, 0.08);
pub const STRAY: (f64, f64) = (0.25, 0.1);
/// Windrows of sand down the pane.
pub const ROWS: f64 = 11.0;
/// Share of a day a change of light takes.
pub const DUSK: f64 = 0.04;
/// Seconds a pointer at rest scares; seconds its scare then takes to go.
pub const STILL: f64 = 2.0;
pub const FADE: f64 = 3.0;
/// Cells from the pointer: a sandpiper runs, flies; a crab runs.
pub const WARY: f64 = 9.0;
pub const NEAR: f64 = 4.5;
pub const SHY: f64 = 11.0;
/// Cells a second: a crab, a sandpiper that flees.
pub const BOLT: f64 = 14.0;
pub const RUN: f64 = 9.0;
/// Seconds a flight; cells a flight.
pub const FLY: f64 = 0.9;
pub const FLEE: f64 = 13.0;

pub const FOAM: [Ink; 4] = [
    [231, 231, 123],
    [255, 230, 87],
    [195, 223, 51],
    [153, 217, 44],
];
pub const LINE: [Ink; 6] = [
    [254, 224, 37],
    [250, 223, 30],
    [246, 181, 30],
    [243, 245, 23],
    [241, 240, 23],
    [239, 239, 23],
];
/// A crest, from the shore out.
pub const SEA: [Ink; 6] = [
    [116, 222, 31],
    [74, 180, 25],
    [38, 138, 24],
    [32, 96, 24],
    [25, 60, 18],
    [24, 24, 17],
];
pub const GLINT: Ink = [195, 222, 67];
/// Deeper, nearer.
pub const SHALLOW: [Ink; 2] = [[37, 137, 23], [80, 180, 30]];
pub const SAND: [Ink; 6] = [
    [137, 131, 238],
    [137, 131, 239],
    [101, 95, 238],
    [137, 131, 238],
    [101, 95, 239],
    [180, 173, 241],
];
pub const WET: [Ink; 3] = [[66, 96, 237], [67, 96, 238], [103, 95, 238]];
/// Water thin on the sand a wave leaves.
pub const FILM: Ink = [153, 216, 60];
pub const SUNK: [Ink; 3] = [[66, 96, 23], [67, 60, 24], [73, 66, 23]];
pub const FROTH: [Ink; 2] = [[153, 217, 44], [110, 174, 38]];
pub const WRACK: [Ink; 4] = [[58, 58, 236], [64, 58, 237], [94, 94, 236], [100, 94, 238]];
/// Dry, wet.
pub const RIPPLE: [Ink; 2] = [[95, 95, 236], [60, 96, 236]];
pub const PRINT: [Ink; 2] = [[180, 173, 243], [109, 138, 240]];
pub const BIRD: Ink = [255, 230, 250];
pub const CRAB: Ink = [209, 203, 131];
pub const MOON: [Ink; 3] = [[230, 230, 230], [229, 229, 229], [223, 223, 223]];

/// What the sea leaves, `-d`: glyph, colours, weight.
pub const FINDS: [(char, &[Ink], f64); 6] = [
    (
        '•',
        &[[245, 245, 240], [250, 181, 242], [102, 95, 238]],
        6.0,
    ),
    ('°', &[[224, 217, 245], [230, 223, 247]], 4.0),
    ('∘', &[[223, 216, 244], [255, 224, 248]], 3.0),
    ('◠', &[[224, 217, 246], [217, 174, 243]], 3.0),
    ('✶', &[[209, 203, 131], [174, 167, 95]], 2.0),
    ('~', &[[65, 58, 238], [71, 64, 239]], 2.0),
];

fn tile(n: i64) -> usize {
    n.rem_euclid(TILE as i64) as usize
}

/// How far up a wave is, 0 to 1, `p` of its time on the sand: a fast
/// run up to `PEAK`, a slow way back.
pub fn swash(p: f64) -> f64 {
    if !(0.0 < p && p < 1.0) {
        return 0.0;
    }
    if p < PEAK {
        return (PI / 2.0 * p / PEAK).sin();
    }
    0.5 + 0.5 * (PI * (p - PEAK) / (1.0 - PEAK)).cos()
}

/// The still-water line, in dots from the top of `h` rows, at `t`
/// seconds: half-way and rising at 0.
pub fn tide(t: f64, period: f64, h: usize) -> f64 {
    let (mid, half) = ((HIGH + LOW) / 2.0, (LOW - HIGH) / 2.0);
    (4 * h) as f64 * (mid - half * (TAU * t / period).sin())
}

/// The share of a day at `t`, the afternoon at 0.
fn of_day(t: f64, day: f64) -> f64 {
    (t / day + 0.3).rem_euclid(1.0)
}

/// The light at `t` seconds, a day lasting `day`: the palette, 0 day, 1
/// dusk, 2 night, and how far it is into the next, 0 to 1.
pub fn light(t: f64, day: f64) -> (usize, f64) {
    let f = of_day(t, day);
    for (at, palette) in [(0.45, 0), (0.55, 1), (0.9, 2), (0.97, 1)] {
        if at <= f && f < at + DUSK {
            return (palette, (f - at) / DUSK);
        }
    }
    let palette = match f {
        f if f < 0.45 => 0,
        f if f < 0.55 => 1,
        f if f < 0.9 => 2,
        f if f < 0.97 => 1,
        _ => 0,
    };
    (palette, 0.0)
}

/// The palette that follows `palette` at `f` of a day.
fn after(palette: usize, f: f64) -> usize {
    match palette {
        1 if f < 0.75 => 2,
        1 => 0,
        _ => 1,
    }
}

/// The host pointer over the pane, in cells: the twin's `Pointer`, its
/// last cell alone.
#[derive(Clone, Debug)]
pub struct Pointer {
    /// The last cell, and when it was told.
    pub last: Option<(i64, i64, f64)>,
    pub here: bool,
    /// When it came in, or went.
    pub since: f64,
}

impl Default for Pointer {
    fn default() -> Pointer {
        Pointer {
            last: None,
            here: false,
            since: f64::NEG_INFINITY,
        }
    }
}

impl Pointer {
    pub fn move_to(&mut self, x: i64, y: i64, t: f64) {
        if !self.here {
            self.since = t;
        }
        self.here = true;
        self.last = Some((x, y, t));
    }

    pub fn leave(&mut self, t: f64) {
        if self.here {
            self.here = false;
            self.since = t;
        }
    }
}

/// The cell of the pointer at `t` seconds and how much it scares, 0 to
/// 1: all it can while it moves and `STILL` seconds after, less over the
/// next `FADE`, and as it leaves.
pub fn scare(pointer: Option<&Pointer>, t: f64) -> Option<(f64, f64, f64)> {
    let p = pointer?;
    let (x, y, moved) = p.last?;
    let rest = if p.here {
        t - moved - STILL
    } else {
        t - p.since
    };
    let much = 1.0 - rest.max(0.0) / FADE;
    (much > 0.0).then_some((x as f64, y as f64, much))
}

/// 0 to 1 and back, `s` of a cycle: up over `ramp`, down by `share`.
fn spell(s: f64, (share, ramp): (f64, f64)) -> f64 {
    if !(0.0 < s && s < share) {
        return 0.0;
    }
    let e = (s / ramp).min((share - s) / ramp).min(1.0);
    e * e * (3.0 - 2.0 * e)
}

/// The foam net, `TILE` dots a side, it tiles: per dot a number in [0,
/// 1), low on the walls between the bubbles; as many dots under `q` as
/// `q` says. `[y][x]`.
pub fn lace() -> &'static [Vec<f64>] {
    static LACE: OnceLock<Vec<Vec<f64>>> = OnceLock::new();
    LACE.get_or_init(|| {
        let mut rng = Twister::new(0x5EA);
        let n = TILE as f64;
        let seeds: Vec<(f64, f64)> = (0..TILE * TILE / 60)
            .map(|_| (rng.uniform(0.0, n), rng.uniform(0.0, n)))
            .collect();
        let mut gaps = Vec::with_capacity(TILE * TILE);
        for y in 0..TILE {
            for x in 0..TILE {
                let (mut a, mut b) = (1e9, 1e9);
                for &(sx, sy) in &seeds {
                    let (dx, dy) = ((x as f64 - sx).abs(), (y as f64 - sy).abs());
                    let d = dx.min(n - dx).hypot(dy.min(n - dy));
                    if d < a {
                        (a, b) = (d, a);
                    } else if d < b {
                        b = d;
                    }
                }
                gaps.push((b - a + 0.3 * rng.random(), y * TILE + x));
            }
        }
        gaps.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
        let mut out = vec![vec![0.0; TILE]; TILE];
        for (rank, &(_, at)) in gaps.iter().enumerate() {
            out[at / TILE][at % TILE] = rank as f64 / (TILE * TILE) as f64;
        }
        out
    })
}

/// Noise, `TILE` dots a side: per dot a number in [0, 1).
pub fn grit() -> &'static [Vec<f64>] {
    static GRIT: OnceLock<Vec<Vec<f64>>> = OnceLock::new();
    GRIT.get_or_init(|| {
        let mut rng = Twister::new(0x5A4D);
        (0..TILE)
            .map(|_| (0..TILE).map(|_| rng.random()).collect())
            .collect()
    })
}

/// Wave `k` over `across` dot columns: `at`, the second its middle
/// breaks; per column, `late`, the seconds it breaks after, and `run`,
/// the dots it runs up the sand.
#[derive(Clone, Debug, PartialEq)]
pub struct Wave {
    pub k: i64,
    pub at: f64,
    pub late: Vec<f64>,
    pub run: Vec<f64>,
    /// Phase of the bends of its crest.
    pub bend: f64,
}

/// What the pointer did to a sandpiper.
#[derive(Clone, Debug)]
struct Bird {
    /// Cells from where it would be.
    off: f64,
    /// From when, from, to.
    flight: Option<(f64, f64, f64)>,
    face: i64,
    /// When it landed.
    down: f64,
}

impl Default for Bird {
    fn default() -> Bird {
        Bird {
            off: 0.0,
            flight: None,
            face: 1,
            down: f64::NEG_INFINITY,
        }
    }
}

/// What the pointer did to a crab.
#[derive(Clone, Debug, Default)]
struct Crab {
    off: f64,
    /// Where it was.
    x: Option<f64>,
    /// The side it runs to; 0: it does not.
    way: i64,
}

/// The options of the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Seconds between two waves.
    pub period: f64,
    /// Seconds a tide takes.
    pub ebb: f64,
    /// Grains of sand over 80 by 24 cells.
    pub grains: f64,
    /// Seconds a day; 0: day alone.
    pub day: f64,
    pub drift: bool,
    pub ripples: bool,
    pub crabs: bool,
    pub birds: bool,
    pub walker: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            period: 8.0,
            ebb: 300.0,
            grains: 420.0,
            day: 0.0,
            drift: false,
            ripples: false,
            crabs: false,
            birds: false,
            walker: false,
        }
    }
}

/// Braille dots, a cell takes the colour of its dot of highest rank.
struct Dots {
    w: usize,
    h: usize,
    /// Per cell: its dots, colour, rank.
    cells: Vec<Option<(u8, Ink, u8)>>,
}

impl Dots {
    fn new(w: usize, h: usize) -> Dots {
        Dots {
            w,
            h,
            cells: vec![None; w * h],
        }
    }

    fn dot(&mut self, px: i64, py: i64, ink: Ink, rank: u8) {
        if !((0..2 * self.w as i64).contains(&px) && (0..4 * self.h as i64).contains(&py)) {
            return;
        }
        let bit = crate::grid::DOTS[(py & 3) as usize][(px & 1) as usize];
        let cell = &mut self.cells[(py >> 2) as usize * self.w + (px >> 1) as usize];
        match cell {
            None => *cell = Some((bit, ink, rank)),
            Some(c) => {
                c.0 |= bit;
                if rank >= c.2 {
                    (c.1, c.2) = (ink, rank);
                }
            }
        }
    }
}

/// A number a dot column.
type Column = Rc<Vec<f64>>;

/// A wave gone, when it peaked, and what it reached.
type Mark = (Rc<Wave>, f64, Column);

/// The scene: the same draws, the same shore.
pub struct Shore {
    pub c: Config,
    pub key: i64,
    sand_rng: Twister,
    finds_rng: Twister,
    sand: Vec<(f64, f64, u32)>,
    finds: Vec<(f64, f64, usize, u32)>,
    waves: RefCell<HashMap<(i64, usize), Rc<Wave>>>,
    across: RefCell<usize>,
    bends: RefCell<HashMap<(i64, usize), Column>>,
    gone: RefCell<HashMap<(i64, usize), (f64, Column)>>,
    marks: ((usize, usize), Vec<(i64, f64)>),
    /// The time of the last picture.
    seen: f64,
    flock: Vec<Bird>,
    dug: Vec<Crab>,
}

impl Shore {
    pub fn new(rng: &mut Twister, c: Config) -> Shore {
        let key = i64::from(rng.bits(24));
        let sand_rng = Twister::new(u64::from(rng.bits(32)));
        let finds_rng = Twister::new(u64::from(rng.bits(32)));
        Shore {
            c,
            key,
            sand_rng,
            finds_rng,
            sand: Vec::new(),
            finds: Vec::new(),
            waves: RefCell::default(),
            across: RefCell::new(0),
            bends: RefCell::default(),
            gone: RefCell::default(),
            marks: ((0, 0), Vec::new()),
            seen: f64::NEG_INFINITY,
            flock: Vec::new(),
            dug: Vec::new(),
        }
    }

    /// Dots a wave of full size runs up the sand, on `h` rows.
    pub fn reach(&self, h: usize) -> f64 {
        (0.19 * (4 * h) as f64).clamp(5.0, 60.0)
    }

    /// Dots between two crests at sea.
    pub fn spacing(&self, h: usize) -> f64 {
        (0.11 * (4 * h) as f64).max(8.0)
    }

    /// Wave `k`, over `across` dot columns of `h` rows.
    pub fn wave(&self, k: i64, across: usize, h: usize) -> Rc<Wave> {
        if across != *self.across.borrow() || self.waves.borrow().len() > 64 {
            self.waves.borrow_mut().clear();
            self.bends.borrow_mut().clear();
            self.gone.borrow_mut().clear();
            *self.across.borrow_mut() = across;
        }
        if let Some(got) = self.waves.borrow().get(&(k, h)) {
            return got.clone();
        }
        let period = self.c.period;
        let n: Vec<f64> = (0..8).map(|i| noise(self.key + i, k)).collect();
        let size = self.reach(h) * (0.5 + 0.5 * n[1].powf(0.8));
        let skew = (n[2] - 0.5) * 0.012 * period / 8.0;
        let lobe = 5.0 * self.reach(h);
        let (mut late, mut run) = (Vec::with_capacity(across), Vec::with_capacity(across));
        for x in 0..across {
            let x = x as f64;
            late.push(
                skew * (x - across as f64 / 2.0)
                    + 0.045 * period * (x * TAU / 130.0 + TAU * n[3]).sin(),
            );
            // Lobes: round towards the land, a cusp between two.
            run.push(
                size * (0.62
                    + 0.3 * (x * PI / lobe + PI * n[4]).cos().abs()
                    + 0.12 * (x * PI / (0.43 * lobe) + PI * n[5]).cos().abs()
                    + 0.08 * (x * TAU / (2.9 * lobe) + TAU * n[6]).sin()),
            );
        }
        let at = (k as f64 + 0.3 * (n[0] - 0.5)) * period;
        let out = Rc::new(Wave {
            k,
            at,
            late,
            run,
            bend: TAU * n[7],
        });
        self.waves.borrow_mut().insert((k, h), out.clone());
        out
    }

    /// The water's edge at the dot column `px`, at `t` seconds.
    pub fn edge(&self, px: i64, t: f64, across: usize, h: usize) -> f64 {
        let base = tide(t, self.c.ebb, h);
        let stay = STAY * self.c.period;
        let mut out = base;
        let px = px.min(across as i64 - 1).max(0) as usize;
        let k0 = (t / self.c.period).floor() as i64;
        for k in k0 - 2..k0 + 2 {
            let one = self.wave(k, across, h);
            let p = (t - one.at - one.late[px]) / stay;
            if 0.0 < p && p < 1.0 {
                out = out.min(base - one.run[px] * swash(p));
            }
        }
        out
    }

    /// The shore over `w` by `h` cells at `t` seconds. `rocks`: the
    /// cells where froth gathers when the water covers them. `pointer`:
    /// what the animals flee, in cells; what it did to them is kept from
    /// a picture to the next, a second apart at most.
    pub fn picture(
        &mut self,
        w: usize,
        h: usize,
        t: f64,
        rocks: &[(i64, i64)],
        pointer: Option<&Pointer>,
    ) -> Grid {
        let mut grid = Grid::new(w, h);
        if w == 0 || h == 0 {
            return grid;
        }
        let mut dt = t - self.seen;
        self.seen = t;
        if !(0.0..=1.0).contains(&dt) {
            dt = 0.0;
            self.flock.clear();
            self.dug.clear();
        }
        let fear = scare(pointer, t);
        let (across, down) = (2 * w, 4 * h);
        let downf = down as f64;
        let period = self.c.period;
        let stay = STAY * period;
        let base = tide(t, self.c.ebb, h);
        let net = lace();
        let mut dots = Dots::new(w, h);
        let k0 = (t / period).floor() as i64;
        let gap = self.spacing(h);
        let speed = gap / period;
        let far = ((downf - base) / gap) as i64 + 2;

        // Per column: the edge, how far the sand is wet, and per wave on
        // the sand its front; per wave gone, what it reached.
        let mut edge = vec![base; across];
        let mut wet = vec![base; across];
        let mut back = vec![false; across];
        let mut fronts: Vec<(Rc<Wave>, Vec<f64>, Vec<f64>)> = Vec::new();
        let mut marks: Vec<Mark> = Vec::new();
        for k in k0 - (LIFE + DRY) as i64 - 2..k0 + 2 {
            let one = self.wave(k, across, h);
            if t < one.at - 0.5 * period {
                continue;
            }
            let age = (t - one.at) / period;
            let fade = ((age - DRY / 2.0) / (DRY / 2.0)).clamp(0.0, 1.0);
            let gone = self.gone.borrow().get(&(k, h)).cloned();
            let gone = match gone {
                Some(g) => g,
                None => {
                    let was = tide(one.at, self.c.ebb, h);
                    let most = one.late.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let g = (most, Rc::new(one.run.iter().map(|r| was - r).collect()));
                    self.gone.borrow_mut().insert((k, h), g.clone());
                    g
                }
            };
            if (t - one.at - gone.0) / stay >= 1.0 {
                // Off the sand at every column: its reach, the wet sand.
                for (a, &top) in wet.iter_mut().zip(gone.1.iter()) {
                    *a = a.min(top + (base - top) * fade);
                }
                let peak = one.at + PEAK * stay;
                marks.push((one, peak, gone.1));
                continue;
            }
            let was = tide(one.at, self.c.ebb, h);
            let mut front = vec![0.0; across];
            let mut share = vec![0.0; across];
            let mut reach = vec![0.0; across];
            let mut live = false;
            for x in 0..across {
                let p = (t - one.at - one.late[x]) / stay;
                let run = one.run[x];
                if p <= 0.0 {
                    front[x] = downf + 1.0;
                    reach[x] = downf + 1.0;
                    share[x] = -1.0;
                    continue;
                }
                let mut top;
                if p < 1.0 {
                    live = true;
                    let y = base - run * swash(p);
                    (front[x], share[x]) = (y, p);
                    if y < edge[x] {
                        edge[x] = y;
                        back[x] = p >= PEAK;
                    }
                    top = if p < PEAK { y } else { was - run };
                } else {
                    share[x] = 2.0;
                    top = was - run;
                }
                reach[x] = top;
                top += (base - top) * fade;
                wet[x] = wet[x].min(top);
            }
            let peak = one.at + PEAK * stay;
            if live {
                fronts.push((one.clone(), front, share));
            }
            marks.push((one, peak, Rc::new(reach)));
        }

        self.ground(&mut dots, w, h, &edge, &wet, &back);
        if self.c.ripples {
            self.ripples(&mut dots, across, h, &edge, &wet);
        }
        if self.c.drift {
            self.wrack(&mut dots, across, h);
        }

        // The lines of foam the waves gone have left. Per wave, the
        // highest reach of those after it.
        let mut over: Vec<Vec<f64>> = Vec::with_capacity(marks.len());
        let mut least = vec![f64::INFINITY; across];
        for (_, _, reach) in marks.iter().rev() {
            let next: Vec<f64> = least
                .iter()
                .zip(reach.iter())
                .map(|(a, b)| a.min(*b))
                .collect();
            over.push(std::mem::replace(&mut least, next));
        }
        over.reverse();
        for ((one, peak, reach), later) in marks.iter().zip(&over) {
            let age = (t - peak) / (LIFE * period);
            if !(0.0..1.0).contains(&age) {
                continue;
            }
            let ink = LINE[((age * LINE.len() as f64) as usize).min(LINE.len() - 1)];
            let row = &net[tile(one.k)];
            for x in 0..across {
                let y = reach[x];
                if y >= edge[x] || t < peak + one.late[x] {
                    continue;
                }
                if row[x % TILE] < age || later[x] <= y + 0.5 {
                    continue;
                }
                dots.dot(x as i64, y as i64, ink, 3);
            }
        }

        self.sea(&mut dots, across, down, &edge, base, t, k0, far, speed, h);

        // The waves on the sand: a front, the net behind it.
        for (one, front, share) in &fronts {
            let shift = one.k * 23;
            for x in 0..across {
                let p = share[x];
                if !(0.0 < p && p < 1.0) {
                    continue;
                }
                let y = front[x];
                let over = base - y; // dots of sand under the wave
                let column = &net[tile(x as i64 + shift)];
                let (line, body, scale, pale);
                if p < PEAK {
                    (line, body, scale, pale) = (1.0, 0.3, 1.5 + 0.3 * over, 0);
                } else {
                    let q = (p - PEAK) / (1.0 - PEAK);
                    line = 0.7 * (1.0 - q).powf(1.5);
                    pale = 1 + (2.5 * q) as usize;
                    body = 0.25 * (1.0 - q).powf(3.0);
                    scale = 1.5 + 0.35 * over;
                }
                let y0 = y.floor() as i64;
                let deep = (over as i64 + 3).min(30);
                for d in 0..deep.max(0) {
                    let py = y0 + d;
                    let want = if d < 2 {
                        if d == 0 { line } else { line * 0.4 }
                    } else {
                        let want = body * (-d as f64 / scale).exp();
                        if want < 0.04 {
                            break;
                        }
                        want
                    };
                    if column[tile(py - y0 + shift)] < want {
                        let ink = FOAM[(d as usize / 3 + pale).min(3)];
                        dots.dot(x as i64, py, ink, if d < 2 { 6 } else { 5 });
                    }
                }
            }
        }

        self.froth(&mut dots, rocks, &edge, base, t, h);
        let mut glyphs: BTreeMap<(i64, i64), (char, Ink)> = BTreeMap::new();
        if self.c.walker {
            self.walker(&mut dots, across, h, t, &wet, &edge, &marks);
        }
        if self.c.drift {
            self.drift(&mut glyphs, w, h, &edge);
        }
        if self.c.birds {
            self.birds(&mut glyphs, w, h, t, fear, dt);
        }
        if self.c.crabs {
            self.crabs(&mut glyphs, w, h, t, &edge, fear, dt);
        }
        let day = self.c.day;
        if day != 0.0 && light(t, day).0 == 2 {
            self.moon(&mut dots, across, down, &edge, t);
        }

        let (palette, blend) = if day != 0.0 { light(t, day) } else { (0, 0.0) };
        let next = if day != 0.0 {
            after(palette, of_day(t, day))
        } else {
            0
        };
        let ink = |x: usize, y: usize, ink: Ink| {
            let turn = blend > 0.0 && noise((x * 131 + y) as i64, 7) < blend;
            ink[if turn { next } else { palette }]
        };
        for y in 0..h {
            for x in 0..w {
                if let Some((bits, i, _)) = dots.cells[y * w + x] {
                    grid.set(x as i64, y as i64, braille(bits), ink(x, y, i));
                }
            }
        }
        for (&(x, y), &(glyph, i)) in &glyphs {
            if (0..w as i64).contains(&x) && (0..h as i64).contains(&y) {
                grid.set(x, y, glyph, ink(x as usize, y as usize, i));
            }
        }
        grid
    }

    /// The grains: dry, wet, or seen through shallow water; the wet
    /// sand, darker, a film of water where a wave just left.
    fn ground(
        &mut self,
        dots: &mut Dots,
        w: usize,
        h: usize,
        edge: &[f64],
        wet: &[f64],
        back: &[bool],
    ) {
        let (across, down) = (2 * w, 4 * h);
        let rough = grit();
        for x in 0..across {
            let column = &rough[x % TILE];
            let y1 = edge[x].floor() as i64;
            for py in (wet[x].ceil() as i64).max(0)..y1 {
                let roll = column[tile(py)];
                if back[x] && y1 - py <= 4 && roll < 0.2 - 0.04 * (y1 - py) as f64 {
                    dots.dot(x as i64, py, FILM, 1);
                } else if roll < 0.035 {
                    dots.dot(x as i64, py, WET[(roll * 997.0) as usize % WET.len()], 0);
                }
            }
        }
        let n = (self.c.grains * w as f64 * h as f64 / AREA) as usize;
        let rng = &mut self.sand_rng;
        while self.sand.len() < n {
            let (u, mut v) = (rng.random(), rng.random());
            if rng.random() < 0.85 {
                // Windrows: most grains lie along lines the wind drew.
                let row = (v * ROWS) as i64 as f64;
                v = (row + 0.5 + 0.3 * (TAU * (3.0 * u + 0.37 * row)).sin()) / ROWS;
                v += rng.gauss(0.0, 0.004);
            }
            self.sand.push((u, v.rem_euclid(1.0), rng.bits(16)));
        }
        for &(u, v, key) in &self.sand[..n] {
            let (px, py) = ((u * across as f64) as usize, (v * down as f64) as i64);
            let depth = py as f64 - edge[px];
            let key = key as usize;
            if depth < 0.0 {
                if py as f64 >= wet[px] {
                    dots.dot(px as i64, py, WET[key % WET.len()], 1);
                } else {
                    dots.dot(px as i64, py, SAND[key % SAND.len()], 1);
                }
            } else if depth < 10.0 && key % 2 == 1 {
                dots.dot(px as i64, py, SUNK[key % SUNK.len()], 0);
            }
        }
    }

    /// The crests on their way in, the water's stipple, glints.
    #[allow(clippy::too_many_arguments)]
    fn sea(
        &self,
        dots: &mut Dots,
        across: usize,
        down: usize,
        edge: &[f64],
        base: f64,
        t: f64,
        k0: i64,
        far: i64,
        speed: f64,
        h: usize,
    ) {
        let period = self.c.period;
        let net = lace();
        let rough = grit();
        let downf = down as f64;
        let drift = (3.0 * t) as i64;
        for k in k0..k0 + far + 2 {
            let one = self.wave(k, across, h);
            let cached = self.bends.borrow().get(&(k, h)).cloned();
            let bends = cached.unwrap_or_else(|| {
                // The shape of its crest: the same all its way in.
                let b: Rc<Vec<f64>> = Rc::new(
                    (0..across)
                        .map(|x| {
                            let x = x as f64;
                            (x * TAU / 83.0 + one.bend).sin()
                                + 0.5 * (x * TAU / 31.0 + 2.0 * one.bend).sin()
                        })
                        .collect(),
                );
                self.bends.borrow_mut().insert((k, h), b.clone());
                b
            });
            let row = &net[tile(one.k * 7)];
            let spots = &rough[tile(one.k * 5)];
            for x in 0..across {
                let xi = x as i64;
                let ahead = one.at + one.late[x] - t; // seconds before it breaks
                if ahead <= 0.0 {
                    continue;
                }
                let out = speed * ahead; // dots from the still-water line
                let sway = 0.35 + 0.65 * (out / (2.0 * speed * period)).min(1.0);
                let y = base + out + sway * 0.2 * speed * period * bends[x];
                if y >= downf || y <= edge[x] + 1.0 {
                    continue;
                }
                let near = ahead / period;
                let yi = y as i64;
                if near < 0.22 {
                    // It breaks, here and there first: white, foam behind.
                    let rise = 1.0 - near / 0.22;
                    if spots[tile(xi >> 3)] < 0.15 + rise {
                        let ink = if rise > 0.5 { FOAM[0] } else { FOAM[2] };
                        dots.dot(xi, yi, ink, 4);
                        if row[tile(xi + drift)] < 0.6 * rise {
                            dots.dot(xi, yi - 1, ink, 4);
                        }
                        for d in 1..1 + (3.0 * rise) as i64 {
                            if net[tile(xi + k)][tile(d + drift)] < 0.25 * rise {
                                dots.dot(xi, yi + d, FOAM[3], 4);
                            }
                        }
                        continue;
                    }
                }
                // Long strokes, shorter and fewer farther out.
                let stroke = row[tile((xi + drift) >> 4)];
                if stroke > 0.92 - 0.3 * (near / 3.0).min(1.0) {
                    continue;
                }
                let ink = SEA[((out / (0.55 * speed * period)) as usize).min(SEA.len() - 1)];
                dots.dot(xi, yi, ink, 2);
            }
        }
        // Shallow water over the sand, thinner as it deepens.
        let slide = (0.7 * t) as i64;
        for x in 0..across {
            let y0 = edge[x].floor() as i64 + 1;
            let column = &rough[(x * 5 + 11) % TILE];
            for d in 1..13.min(down as i64 - y0) {
                if column[tile(y0 + d + slide)] < 0.06 - 0.005 * d as f64 {
                    dots.dot(x as i64, y0 + d, SHALLOW[usize::from(d < 5)], 1);
                }
            }
        }
        // Short ripples between the crests.
        let acrossf = across as f64;
        for j in 0..((downf - base).max(0.0) * acrossf / 600.0) as i64 {
            let px = (noise(j, 11) * acrossf + 3.0 * (0.3 * t + j as f64).sin()) as i64;
            let py = (base + 8.0 + noise(j, 12) * (downf - base - 8.0)) as i64;
            let band = ((py as f64 - base) / (0.55 * speed * period)) as i64;
            let ink = SEA[((1 + band).max(0) as usize).min(SEA.len() - 1)];
            for dx in 0..3 + (5.0 * noise(j, 13)) as i64 {
                let at = px + dx;
                if (0..across as i64).contains(&at) && py as f64 > edge[at as usize] + 8.0 {
                    dots.dot(at, py, ink, 1);
                }
            }
        }
        // Glints of the light, each on a beat of its own.
        let cells = (downf - base).max(0.0) * acrossf / 8.0;
        for j in 0..(cells / 110.0) as i64 {
            let beat = (t / 0.6 + 8.0 * noise(j, 1)) as i64;
            let px = (noise(j, beat) * acrossf) as usize;
            let py = (edge[px] + 6.0 + noise(beat, j) * (downf - edge[px] - 6.0)) as i64;
            dots.dot(px as i64, py, GLINT, 2);
        }
    }

    /// Foam over and under the words the water covers.
    fn froth(
        &self,
        dots: &mut Dots,
        rocks: &[(i64, i64)],
        edge: &[f64],
        base: f64,
        t: f64,
        h: usize,
    ) {
        let net = lace();
        let size = self.reach(h);
        let across = edge.len() as i64;
        let mut by_row: HashMap<i64, (f64, i64)> = HashMap::new();
        for &(x, y) in rocks {
            let (px, py) = (2 * x, 4 * y);
            if px + 1 >= across || px < 0 {
                continue;
            }
            let near = edge[px as usize];
            let depth = (py + 2) as f64 - near;
            if depth < 0.0 {
                continue;
            }
            let row = *by_row.entry(y).or_insert_with(|| {
                (
                    0.03 + 0.14 * (-(py as f64 - base).max(0.0) / size).exp(),
                    (2.5 * (0.8 * t + y as f64).sin()) as i64,
                )
            });
            let swell = 0.6 + 0.4 * (1.9 * t + 0.7 * x as f64 + 2.3 * y as f64).sin();
            let want = row.0 * swell;
            let left = &net[tile(px + row.1)];
            let right = &net[tile(px + 1 + row.1)];
            let far = edge[px as usize + 1];
            let ink = FROTH[usize::from(depth > size)];
            for qy in py..py + 4 {
                if qy as f64 > near && left[tile(qy)] < want {
                    dots.dot(px, qy, ink, 4);
                }
                if qy as f64 > far && right[tile(qy)] < want {
                    dots.dot(px + 1, qy, ink, 4);
                }
            }
        }
    }

    /// Ripple marks in the sand the tide uncovers.
    fn ripples(&mut self, dots: &mut Dots, across: usize, h: usize, edge: &[f64], wet: &[f64]) {
        if self.marks.0 != (across, h) {
            // Where they are does not change: worked out once a size.
            let mut marks = Vec::new();
            let top = (4 * h) as f64 * HIGH - self.reach(h);
            let low = (4 * h) as f64 * LOW;
            for j in 0..((low - top) / 9.0) as i64 + 1 {
                let jf = j as f64;
                let y0 = top + 9.0 * jf;
                let phase = 2.4 * jf + noise(self.key, j) * 9.0;
                for x in 0..across as i64 {
                    let xf = x as f64;
                    if (0.19 * xf + phase).sin() < 0.45 {
                        continue;
                    }
                    marks.push((x, y0 + 1.4 * (xf * TAU / 19.0 + 1.9 * jf).sin()));
                }
            }
            self.marks = ((across, h), marks);
        }
        for &(x, y) in &self.marks.1 {
            let i = x as usize;
            if y < edge[i] - 1.0 {
                dots.dot(x, y as i64, RIPPLE[usize::from(y >= wet[i])], 0);
            }
        }
    }

    /// Weed and bits along the high-water mark.
    fn wrack(&self, dots: &mut Dots, across: usize, h: usize) {
        let net = lace();
        let y0 = (4 * h) as f64 * HIGH - 0.9 * self.reach(h);
        let (a, b) = (noise(self.key, 91) * 9.0, noise(self.key, 92) * 9.0);
        for x in 0..across {
            let clump = net[(x / 3) % TILE][17];
            if clump > 0.3 {
                continue;
            }
            let xf = x as f64;
            let y = y0 + 2.2 * (xf / 23.0 + a).sin() + 1.2 * (xf / 7.0 + b).sin();
            let ink = WRACK[(clump * 13.0) as usize % WRACK.len()];
            let (x, y) = (x as i64, y as i64);
            dots.dot(x, y, ink, 2);
            if net[x as usize % TILE][29] < 0.5 {
                dots.dot(x, y + 1, ink, 2);
            }
            if net[x as usize % TILE][41] < 0.15 {
                dots.dot(x, y - 1, ink, 2);
            }
        }
    }

    /// Shells, pebbles, a starfish: dimmer under water.
    fn drift(
        &mut self,
        glyphs: &mut BTreeMap<(i64, i64), (char, Ink)>,
        w: usize,
        h: usize,
        edge: &[f64],
    ) {
        let n = (11.0 * w as f64 * h as f64 / AREA) as usize;
        let weights: Vec<f64> = FINDS.iter().map(|f| f.2).collect();
        let kinds: Vec<usize> = (0..FINDS.len()).collect();
        let rng = &mut self.finds_rng;
        while self.finds.len() < n {
            let kind = rng.weighted(&kinds, &weights);
            let (u, v) = (rng.random(), rng.random());
            self.finds.push((u, v, kind, rng.bits(8)));
        }
        for &(u, v, kind, key) in &self.finds[..n] {
            let (x, y) = ((u * w as f64) as i64, (v * h as f64) as i64);
            let (glyph, inks, _) = FINDS[kind];
            let depth = (4 * y + 2) as f64 - edge[(2 * x as usize).min(edge.len() - 1)];
            if depth < 0.0 {
                glyphs.insert((x, y), (glyph, inks[key as usize % inks.len()]));
            } else if depth < 16.0 {
                glyphs.insert((x, y), (glyph, SUNK[key as usize % SUNK.len()]));
            }
        }
    }

    /// Footprints of someone who walks along the shore. A print lasts
    /// until a wave that peaked after it was made reaches it.
    #[allow(clippy::too_many_arguments)]
    fn walker(
        &self,
        dots: &mut Dots,
        across: usize,
        h: usize,
        t: f64,
        wet: &[f64],
        edge: &[f64],
        marks: &[Mark],
    ) {
        let washed = |px: usize, py: f64, since: f64| {
            py >= edge[px]
                || (marks.iter())
                    .any(|(one, peak, reach)| peak + one.late[px] > since && reach[px] <= py)
        };
        let size = self.reach(h);
        let walks = (t / WALK).floor() as i64;
        let a = across as i64;
        for c in walks - 2..walks + 1 {
            if c < 0 {
                continue;
            }
            let start = c as f64 * WALK;
            let way = if noise(self.key + 5, c) < 0.5 { 1 } else { -1 };
            let line = tide(start, self.c.ebb, h) - size * (1.0 + 0.25 * noise(self.key, c));
            let bend = noise(self.key + 9, c) * TAU;
            let steps = (((t - start) / STEP) as i64).min(a / STRIDE);
            for j in 0..steps {
                let made = start + j as f64 * STEP;
                let old = (t - made) / (2.5 * WALK);
                if noise(j, c) < old * old {
                    continue;
                }
                let mut px = j * STRIDE;
                if way < 0 {
                    px = a - 1 - px;
                }
                if !(0 <= px && px < a - 1) {
                    continue;
                }
                let py = (line + 3.0 * (px as f64 * TAU / 120.0 + bend).sin()) as i64 + (j & 1) * 2;
                if washed(px as usize, py as f64, made) {
                    continue;
                }
                let ink = PRINT[usize::from(py as f64 >= wet[px as usize])];
                dots.dot(px, py, ink, 2);
                dots.dot(px + way, py, ink, 2);
            }
        }
    }

    /// Sandpipers, in groups: they follow the water down and run from
    /// it; a group walks off the pane and back, or wanders up the sand.
    /// They run from the pointer, and fly when it is on them.
    fn birds(
        &mut self,
        glyphs: &mut BTreeMap<(i64, i64), (char, Ink)>,
        w: usize,
        h: usize,
        t: f64,
        fear: Option<(f64, f64, f64)>,
        dt: f64,
    ) {
        let mut flock = std::mem::take(&mut self.flock);
        let across = 2 * w;
        let wf = w as f64;
        let size = self.reach(h);
        let key = self.key;
        let groups = (w / PACK).clamp(1, 4);
        let sizes: Vec<i64> = (0..groups).map(|g| 3 + (key >> (2 * g)) % 3).collect();
        let who: Vec<(usize, i64)> = (sizes.iter().enumerate())
            .flat_map(|(g, &m)| (0..m).map(move |j| (g, j)))
            .collect();
        let n = who.len();
        let inside = |x: f64| -0.5 <= x && x < wf - 0.5;
        let roam = |i: usize, at: f64| {
            let g = who[i].0 as i64;
            let a = (at / (53 + 17 * g) as f64 + noise(key + 31, g)).rem_euclid(1.0);
            let b = (at / (41 + 13 * (i as i64 % 5)) as f64 + noise(key + 47, i as i64))
                .rem_euclid(1.0);
            1.0 - (1.0 - spell(a, ROAM)) * (1.0 - spell(b, STRAY))
        };
        let home = |i: usize, at: f64| {
            let (g, j) = who[i];
            // The first group stays; the others leave by a side.
            let swing = if g > 0 { 0.62 } else { 0.36 };
            let gf = g as f64;
            let turn = at * TAU / (97 + 34 * g as i64) as f64 + key as f64 + 2.1 * gf;
            let mut x =
                wf * (0.5 + swing * turn.sin()) + (j as f64 - (sizes[g] - 1) as f64 / 2.0) * 3.4;
            x += 1.8 * (0.7 * at + 2.1 * i as f64).sin();
            x + 4.0 * roam(i, at) * (0.17 * at + 1.9 * i as f64).sin()
        };
        let stand = |i: usize, x: f64, at: f64| {
            let x = (x.round_ties_even() as i64).min(w as i64 - 1).max(0);
            let lag = 0.25 + 0.12 * (i % 3) as f64;
            let mut y = self.edge(2 * x, at - lag, across, h) - OFF - (3 * (i % 3)) as f64;
            let u = roam(i, at);
            if u != 0.0 {
                let mut dry = tide(at, self.c.ebb, h) - size - (4 * (2 + i % 4)) as f64;
                dry += 3.0 * (0.29 * at + 1.7 * i as f64).sin();
                y += (dry.min(y) - y) * u;
            }
            (x, ((y as i64) >> 2).min(h as i64 - 1).max(0))
        };
        let place = |i: usize, at: f64| {
            let x = home(i, at);
            inside(x).then(|| stand(i, x, at))
        };

        if flock.len() != n {
            flock = vec![Bird::default(); n];
        }
        // Where each is, how far from the pointer, the side it flees to.
        let mut spots: Vec<(f64, f64, f64)> = Vec::with_capacity(n);
        for (i, bird) in flock.iter().enumerate() {
            let x = home(i, t) + bird.off;
            match fear {
                Some((fx, fy, _)) if inside(x) => {
                    let d = (x - fx).hypot(2.0 * (stand(i, x, t).1 as f64 - fy));
                    let side = if x - fx != 0.0 {
                        x - fx
                    } else {
                        (i & 1) as f64 - 0.5
                    };
                    spots.push((x, d, if side > 0.0 { 1.0 } else { -1.0 }));
                }
                _ => spots.push((x, f64::INFINITY, 1.0)),
            }
        }
        let (at, much) = fear.map_or((0.0, 0.0), |f| (f.0, f.2));
        // One flies: those about it do.
        let up = (flock.iter().zip(&spots)).any(|(b, s)| s.1 < NEAR * much && b.flight.is_none());
        for (i, bird) in flock.iter_mut().enumerate() {
            let (mut x, mut d, way) = spots[i];
            d -= 1.7 * (i % 4) as f64; // each its own nerve
            let hx = x - bird.off;
            let mut lift = 0.0;
            if let Some((since, a, b)) = bird.flight {
                let u = (t - since) / FLY;
                if u >= 1.0 {
                    (bird.flight, bird.down, x) = (None, t, b);
                } else if u > 0.0 {
                    x = a + (b - a) * u * u * (3.0 - 2.0 * u);
                    lift = (PI * u).sin();
                    bird.face = if b > a { 1 } else { -1 };
                }
            } else if d < NEAR * much
                || (up && d < (WARY + 3.0) * much && t - bird.down > 2.0 * FLY)
            {
                let mut to = x + way * (FLEE + 6.0 * noise(i as i64, (8.0 * t) as i64));
                let wall = if way < 0.0 { 1.0 } else { wf - 2.0 };
                if (wall - x).abs() < FLEE / 2.0 {
                    // No room: over the pointer.
                    to = x - way * (FLEE + 2.0 * (x - at).abs());
                }
                to = to.min(wf - 2.0).max(1.0);
                let wait = 0.3 * noise(i as i64, t as i64) * d.max(0.0) / WARY;
                bird.flight = Some((t + wait, x, to));
            } else if d < WARY * much {
                x = (x + way * RUN * dt).min(wf - 1.0).max(0.0);
                bird.face = way as i64;
            } else if bird.off != 0.0
                && fear.is_none_or(|f| {
                    (hx - f.0) * (x - f.0) > 0.0 && (hx - f.0).abs() > WARY * much + 2.0
                })
            {
                let step = bird.off.abs().min(0.3 * RUN * dt);
                x -= step.copysign(bird.off);
                bird.face = if bird.off > 0.0 { -1 } else { 1 };
            }
            if !inside(x) && !inside(hx) && bird.flight.is_none() {
                x = hx; // out of sight: where it would be
            }
            bird.off = x - hx;
            let (cx, mut cy) = stand(i, x, t);
            let away = bird.off != 0.0 || bird.flight.is_some();
            if !inside(x) {
                continue;
            }
            let glyph;
            if lift != 0.0 {
                cy = (cy - (2.4 * lift).round_ties_even() as i64).max(0);
                glyph = if (8.0 * t + i as f64) as i64 & 1 == 0 {
                    'v'
                } else {
                    '^'
                };
            } else if away {
                glyph = if bird.face > 0 { '›' } else { '‹' };
            } else {
                glyph = match place(i, t - 0.3) {
                    Some(before) if before == (cx, cy) => {
                        if noise(i as i64, (2.0 * t) as i64) < 0.4 {
                            ','
                        } else if i & 1 == 0 {
                            '›'
                        } else {
                            '‹'
                        }
                    }
                    // It comes in.
                    None => {
                        if 2 * cx < w as i64 {
                            '›'
                        } else {
                            '‹'
                        }
                    }
                    Some(before) => {
                        if before.0 <= cx {
                            '›'
                        } else {
                            '‹'
                        }
                    }
                };
            }
            glyphs.insert((cx, cy), (glyph, BIRD));
        }
        self.flock = flock;
    }

    /// Crabs: sideways, in bursts. They run from the pointer, to the side
    /// they took, past it when the side of the pane is near.
    #[allow(clippy::too_many_arguments)]
    fn crabs(
        &mut self,
        glyphs: &mut BTreeMap<(i64, i64), (char, Ink)>,
        w: usize,
        h: usize,
        t: f64,
        edge: &[f64],
        fear: Option<(f64, f64, f64)>,
        dt: f64,
    ) {
        let n = (w / 12).max(1);
        let wf = w as f64;
        let size = self.reach(h);
        let base = tide(t, self.c.ebb, h);
        if self.dug.len() != n {
            self.dug = vec![Crab::default(); n];
        }
        for (i, crab) in self.dug.iter_mut().enumerate() {
            let key = self.key + 977 * i as i64;
            let x0 = t / 2.6 + 5.0 * noise(key, 0);
            let (beat, part) = (x0.floor() as i64, x0.rem_euclid(1.0));
            let (a, b) = (noise(key, beat), noise(key, beat + 1));
            let mut ease = (part / 0.35).min(1.0);
            ease = ease * ease * (3.0 - 2.0 * ease);
            let hx = wf * ((i as f64 + 0.3 + 0.4 * (a + (b - a) * ease)) / n as f64);
            let y = ((base - size * (0.7 + 0.5 * noise(key, 3))) as i64) >> 2;
            let mut fx = hx + crab.off;
            let was = crab.x.unwrap_or(fx);
            let mut d = f64::INFINITY;
            if let Some(f) = fear {
                d = (was - f.0).hypot(2.0 * (y as f64 - f.1)) / f.2;
                d -= 5.0 * noise(key, 7); // each its own nerve
            }
            if d < SHY {
                let f = fear.expect("a pointer");
                if crab.way == 0 {
                    crab.way = if was >= f.0 { 1 } else { -1 };
                    let wall = if crab.way < 0 { 0.0 } else { wf - 1.0 };
                    if (wall - was).abs() < 3.0 {
                        crab.way = -crab.way;
                    }
                } else if was == 0.0 || was == wf - 1.0 {
                    crab.way = if was == 0.0 { 1 } else { -1 };
                }
                fx = was + crab.way as f64 * BOLT * (0.7 + 0.6 * noise(key, (4.0 * t) as i64)) * dt;
            } else if d >= SHY + 2.0 {
                crab.way = 0;
                if fear.is_some_and(|f| (hx - f.0).abs() < (SHY + 2.0) * f.2) {
                    fx = was; // the pointer is where it would go
                } else {
                    let step = crab.off.abs().min(0.3 * RUN * dt);
                    fx -= step.copysign(crab.off);
                }
            } else {
                fx = was;
            }
            fx = fx.min(wf - 1.0).max(0.0);
            (crab.off, crab.x) = (fx - hx, Some(fx));
            let x = fx.round_ties_even() as i64;
            if (0..h as i64).contains(&y) && ((4 * y + 3) as f64) < edge[2 * x as usize] {
                glyphs.insert((x, y), ('ж', CRAB));
            }
        }
    }

    /// The moon on the water: a path of glitter, wider towards the open
    /// sea, that crosses the pane in a night.
    fn moon(&self, dots: &mut Dots, across: usize, down: usize, edge: &[f64], t: f64) {
        let f = of_day(t, self.c.day);
        let (acrossf, downf) = (across as f64, down as f64);
        let mid = acrossf * (0.2 + 0.6 * (f - 0.55) / 0.35);
        for j in 0..(1.6 * downf / 4.0) as i64 {
            let beat = (t / 0.45 + 9.0 * noise(j, 2)) as i64;
            let out = noise(j, 5).powf(0.8);
            let half = 2.0 + 0.2 * out * downf;
            let px = (mid + (noise(j, beat) + noise(beat, j) - 1.0) * half) as i64;
            if !(0..across as i64).contains(&px) {
                continue;
            }
            let e = edge[px as usize];
            let py = (e + 3.0 + out * (downf - e - 3.0)) as i64;
            dots.dot(px, py, MOON[j as usize % 3], 3);
        }
    }
}

/// The cells over and under the glyphs of the text `rows`, in a view of
/// `w` by `h`, that hold no glyph and are beside none.
pub fn rocks_of(rows: &[String], w: usize, h: usize) -> Vec<(i64, i64)> {
    let walls = walls_of(rows);
    let mut shut = BTreeSet::new();
    let mut out = BTreeSet::new();
    for &(x, y) in &walls {
        let (x, y) = (x as i64, y as i64);
        for dx in -1..=1 {
            shut.insert((x + dx, y));
            for dy in [-1, 1] {
                out.insert((x + dx, y + dy));
            }
        }
    }
    let mut cells: Vec<(i64, i64)> = (out.difference(&shut))
        .copied()
        .filter(|&(x, y)| (0..w as i64).contains(&x) && (0..h as i64).contains(&y))
        .collect();
    cells.sort();
    cells
}

#[cfg(test)]
mod tests;
