// SPDX-License-Identifier: GPL-3.0-or-later
//! Native effects: generators that own a sprite's cells.

use super::raster::Canvas;
use super::spec::{ColorSpec, StyleSpec};
use crate::frames::{HeatClass, HeatView};
use crate::types::{FrameId, Surfaces};
use ptyrc_vt::{Cell, Color, Grid, Rect};
use serde::Deserialize;
use std::time::Duration;

/// A generator. `render` draws the state at `t` (time since the sprite was
/// first drawn) on a cleared canvas and returns `false` once finished: the
/// sprite is then removed.
pub trait Effect: Send {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool;

    /// What `look` is given. `No`: `look` is not called, nothing is read.
    fn looks(&self) -> Looks {
        Looks::No
    }

    /// What the app shows under the canvas; called before each `render`
    /// while the frame is usable.
    fn look(&mut self, _under: &Under) {}

    /// Rows `from..=to` of the frame moved by `dy` in frame space (a
    /// splice, an island merge): what is kept on them moves too.
    fn shift(&mut self, _from: i64, _to: i64, _dy: i64) {}

    /// New `params` for a running effect: its state is kept. Nothing is
    /// changed on error.
    fn tune(&mut self, _params: &serde_json::Value) -> Result<(), String> {
        Err("this effect cannot be changed while it runs".into())
    }

    /// Time between two renders.
    fn period(&self) -> Duration {
        Duration::from_millis(50)
    }
}

/// What an effect asks to see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Looks {
    No,
    /// The cells under the canvas, the cursor, the frame position.
    Cells,
    /// `Cells`, and the heat of those cells.
    Heat,
}

/// The heat of a cell under an effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warmth {
    pub class: HeatClass,
    /// Seconds without a visible change.
    pub still: f32,
    /// Visible change, decaying, `0..=1`.
    pub activity: f32,
}

/// The app under an effect's canvas, cell for cell. A canvas cell
/// outside the view of the frame shows nothing.
pub struct Under<'a> {
    grid: &'a Grid,
    /// Screen cell of the canvas cell `(0, 0)`.
    at: (i64, i64),
    /// The view of the frame, screen cells.
    clip: Rect,
    heat: Option<&'a HeatView>,
    /// The frame, for its rows out of the view.
    frame: Option<(&'a dyn Surfaces, FrameId)>,
    /// The canvas, in cells.
    pub size: (u16, u16),
    /// The cursor as a canvas cell; `None`: outside.
    pub cursor: Option<(u16, u16)>,
    /// Frame position of the canvas cell `(0, 0)`: it changes when the
    /// app scrolls under a canvas fixed in the view.
    pub origin: (i64, i64),
}

impl<'a> Under<'a> {
    /// A canvas at the screen cell `at` of `grid`, in a view `clip`.
    pub fn new(grid: &'a Grid, at: (i64, i64), clip: Rect) -> Under<'a> {
        Under {
            grid,
            at,
            clip: clip.intersect(&grid.rect()),
            heat: None,
            frame: None,
            size: (clip.w, clip.h),
            cursor: None,
            origin: at,
        }
    }

    /// A canvas that covers `grid`.
    pub fn of(grid: &'a Grid) -> Under<'a> {
        Under::new(grid, (0, 0), grid.rect())
    }

    /// With the heat of the view `clip`.
    pub fn with_heat(mut self, heat: Option<&'a HeatView>) -> Under<'a> {
        self.heat = heat;
        self
    }

    /// With the frame the canvas is on: `row` reads what it remembers.
    pub fn with_frame(mut self, surfaces: &'a dyn Surfaces, id: FrameId) -> Under<'a> {
        self.frame = Some((surfaces, id));
        self
    }

    /// Ink of the canvas columns on the row `y` of the canvas, above
    /// (negative) or below it and out of the view: what the frame
    /// remembers there. `None`: in the view, or not remembered.
    pub fn row(&self, y: i64) -> Option<Vec<bool>> {
        let sy = self.at.1 + y;
        if sy >= self.clip.y as i64 && sy < self.clip.bottom() as i64 {
            return None;
        }
        let (surfaces, id) = self.frame?;
        surfaces.ink_row(id, self.origin.1 + y, self.origin.0, self.size.0)
    }

    /// Foreground of the canvas, row-major: `ink` of every cell.
    pub fn inked(&self) -> Vec<bool> {
        let (w, h) = self.size;
        let mut out = Vec::with_capacity(w as usize * h as usize);
        for y in 0..h {
            out.extend((0..w).map(|x| self.ink(x, y)));
        }
        out
    }

    fn screen(&self, x: u16, y: u16) -> Option<(u16, u16)> {
        let (sx, sy) = (self.at.0 + x as i64, self.at.1 + y as i64);
        let c = self.clip;
        let inside =
            sx >= c.x as i64 && sy >= c.y as i64 && sx < c.right() as i64 && sy < c.bottom() as i64;
        inside.then_some((sx as u16, sy as u16))
    }

    /// The app's cell under the canvas cell `(x, y)`.
    pub fn cell(&self, x: u16, y: u16) -> Option<&'a Cell> {
        let (sx, sy) = self.screen(x, y)?;
        Some(self.grid.cell(sx, sy))
    }

    /// Whether the cell shows foreground: a glyph, either half of a wide
    /// one, a line.
    pub fn ink(&self, x: u16, y: u16) -> bool {
        match self.cell(x, y) {
            Some(cell) if cell.width == 0 && x > 0 => {
                self.cell(x - 1, y).is_some_and(|head| head.has_ink())
            }
            Some(cell) => cell.has_ink(),
            None => false,
        }
    }

    /// The heat of the cell; `None`: not asked (`Looks::Heat`), or outside.
    pub fn heat(&self, x: u16, y: u16) -> Option<Warmth> {
        let heat = self.heat?;
        let (sx, sy) = self.screen(x, y)?;
        let (hx, hy) = (sx - self.clip.x, sy - self.clip.y);
        if hx >= heat.cols || hy >= heat.rows {
            return None;
        }
        let i = hy as usize * heat.cols as usize + hx as usize;
        Some(Warmth {
            class: heat.class[i],
            still: heat.still[i],
            activity: heat.activity[i],
        })
    }
}

/// Builds an effect from its `params`.
pub type EffectFactory = Box<dyn Fn(&serde_json::Value) -> Result<Box<dyn Effect>, String> + Send>;

/// xorshift64*: small, deterministic.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

fn default_colors() -> Vec<ColorSpec> {
    [9, 10, 11, 12, 13, 14]
        .iter()
        .map(|n| ColorSpec(Color::Indexed(*n)))
        .collect()
}

fn parse<T: Default + serde::de::DeserializeOwned>(
    name: &str,
    params: &serde_json::Value,
) -> Result<T, String> {
    if params.is_null() {
        Ok(T::default())
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("{name}: {e}"))
    }
}

/// The fields of `params` a running effect takes, each one of `keys`.
fn tunable<'a>(
    name: &str,
    params: &'a serde_json::Value,
    keys: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    let Some(new) = params.as_object() else {
        return Err(format!("{name}: params: an object is needed"));
    };
    match new.keys().find(|k| !keys.contains(&k.as_str())) {
        Some(k) => Err(format!("{name}: {k} cannot be changed while it runs")),
        None => Ok(new),
    }
}

/// `new[key]` when given, else `old`.
fn tuned<T: serde::de::DeserializeOwned>(
    name: &str,
    new: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    old: T,
) -> Result<T, String> {
    match new.get(key) {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("{name}: {key}: {e}")),
        None => Ok(old),
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct ConfettiParams {
    count: u32,
    colors: Vec<ColorSpec>,
    duration_ms: u64,
    /// Canvas cell the burst starts from. Absent: a third down the middle.
    origin: Option<[f64; 2]>,
    /// Speed scale.
    spread: f64,
    seed: u64,
}

impl Default for ConfettiParams {
    fn default() -> ConfettiParams {
        ConfettiParams {
            count: 64,
            colors: default_colors(),
            duration_ms: 3000,
            origin: None,
            spread: 1.0,
            seed: 1,
        }
    }
}

struct Particle {
    /// Launch velocity, columns and rows per second.
    vx: f64,
    vy: f64,
    /// Death, as a share of the duration.
    life: f64,
    color: Color,
    /// Flutter: phase, turns per second, sway in columns.
    phase: f64,
    spin: f64,
    sway: f64,
}

/// A burst of paper: thrown from one point, slowed by the air, falling
/// and fluttering, dimming before it goes.
struct Confetti {
    params: ConfettiParams,
    particles: Vec<Particle>,
}

/// A piece of paper turning: face, edge, face, edge.
const FLUTTER: [&str; 4] = ["❋", "✦", "*", "·"];
/// Rows per second squared.
const GRAVITY: f64 = 16.0;
/// Air drag, per second. Terminal fall speed is `GRAVITY / DRAG`.
const DRAG: f64 = 2.2;

impl Confetti {
    fn new(params: ConfettiParams) -> Confetti {
        let mut rng = Rng::new(params.seed);
        let particles = (0..params.count.min(2000))
            .map(|_| {
                // A fan opened upwards, wider than tall.
                let angle = rng.range(-0.95, 0.95) * std::f64::consts::FRAC_PI_2;
                let speed = rng.range(8.0, 30.0) * params.spread;
                Particle {
                    // Cells are about twice as tall as wide.
                    vx: angle.sin() * speed * 2.0,
                    vy: -angle.cos() * speed,
                    life: rng.range(0.55, 1.0),
                    color: params.colors[(rng.next_u64() % params.colors.len() as u64) as usize].0,
                    phase: rng.unit(),
                    spin: rng.range(2.0, 5.0),
                    sway: rng.range(0.3, 1.4),
                }
            })
            .collect();
        Confetti { params, particles }
    }
}

impl Effect for Confetti {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool {
        let total = self.params.duration_ms.max(1) as f64 / 1000.0;
        let secs = t.as_secs_f64();
        if secs >= total {
            return false;
        }
        let (w, h) = (canvas.width() as f64, canvas.height() as f64);
        let [ox, oy] = self.params.origin.unwrap_or([w / 2.0, h / 3.0]);
        // Drag in closed form: no state, any `t` renders the same.
        let glide = (1.0 - (-DRAG * secs).exp()) / DRAG;
        let fall = GRAVITY / DRAG;
        let mut alive = false;
        for p in &self.particles {
            let age = secs / (total * p.life);
            if age >= 1.0 {
                continue;
            }
            let turn = p.phase + p.spin * secs;
            let x = ox + p.vx * glide + p.sway * age * (turn * std::f64::consts::TAU).sin();
            let y = oy + (p.vy - fall) * glide + fall * secs;
            if y >= h {
                continue;
            }
            alive = true;
            let glyph = if age > 0.85 {
                "·"
            } else {
                FLUTTER[(turn * FLUTTER.len() as f64) as usize % FLUTTER.len()]
            };
            let style = StyleSpec {
                bold: age < 0.35,
                dim: age > 0.7,
                ..StyleSpec::fg(p.color)
            };
            canvas.put(x.round() as i32, y.round() as i32, glyph, &style);
        }
        alive
    }

    /// `duration_ms`, `origin`: the pieces thrown keep their flight.
    fn tune(&mut self, params: &serde_json::Value) -> Result<(), String> {
        let new = tunable("confetti", params, &["duration_ms", "origin"])?;
        let duration_ms = tuned("confetti", new, "duration_ms", self.params.duration_ms)?;
        let origin = tuned("confetti", new, "origin", self.params.origin)?;
        finite_origin(origin)?;
        self.params.duration_ms = duration_ms;
        self.params.origin = origin;
        Ok(())
    }

    fn period(&self) -> Duration {
        Duration::from_millis(33)
    }
}

fn finite_origin(origin: Option<[f64; 2]>) -> Result<(), String> {
    match origin {
        Some([x, y]) if !(x.is_finite() && y.is_finite()) => {
            Err("confetti: origin is not finite".into())
        }
        _ => Ok(()),
    }
}

/// Factory of the built-in `confetti`. Params: `count`, `colors`,
/// `duration_ms`, `origin` (`[x, y]` on the canvas), `spread`, `seed`.
pub fn confetti(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    let params: ConfettiParams = parse("confetti", params)?;
    if params.colors.is_empty() {
        return Err("confetti: colors is empty".into());
    }
    if !params.spread.is_finite() {
        return Err("confetti: spread is not finite".into());
    }
    finite_origin(params.origin)?;
    Ok(Box::new(Confetti::new(params)))
}

#[derive(Deserialize)]
#[serde(default)]
struct SparkleParams {
    /// Twinkles alive at once.
    count: u32,
    colors: Vec<ColorSpec>,
    /// Life of one twinkle.
    life_ms: u64,
    /// Absent: until the sprite is removed.
    duration_ms: Option<u64>,
    seed: u64,
}

impl Default for SparkleParams {
    fn default() -> SparkleParams {
        SparkleParams {
            count: 6,
            colors: default_colors(),
            life_ms: 900,
            duration_ms: None,
            seed: 1,
        }
    }
}

/// Twinkles at random cells: a dot opens into a star and closes. With
/// `"blend": "behind"` they only show where the screen is free.
struct Sparkle {
    params: SparkleParams,
}

/// One twinkle, from birth to death.
const TWINKLE: [&str; 7] = ["·", "*", "✦", "❋", "✦", "*", "·"];

impl Effect for Sparkle {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool {
        let ms = t.as_millis() as u64;
        if self.params.duration_ms.is_some_and(|d| ms >= d) {
            return false;
        }
        let (w, h) = (canvas.width() as u64, canvas.height() as u64);
        if w == 0 || h == 0 {
            return true;
        }
        let life = self.params.life_ms.max(TWINKLE.len() as u64);
        // Each lane plays one twinkle after another, a little out of step
        // with the others. Lane and round seed the place and the colour:
        // no state to keep.
        for lane in 0..self.params.count.min(500) as u64 {
            let cycle = life + life / 3;
            let at = ms + lane * cycle / self.params.count.max(1) as u64;
            let (round, age) = (at / cycle, at % cycle);
            if age >= life {
                continue;
            }
            let mut rng =
                Rng::new(self.params.seed ^ (lane << 32) ^ round.wrapping_mul(0x9E3779B1));
            let x = rng.next_u64() % w;
            let y = rng.next_u64() % h;
            let color =
                self.params.colors[(rng.next_u64() % self.params.colors.len() as u64) as usize].0;
            let step = (age * TWINKLE.len() as u64 / life) as usize;
            let style = StyleSpec {
                bold: (2..=4).contains(&step),
                dim: step == 0 || step == TWINKLE.len() - 1,
                ..StyleSpec::fg(color)
            };
            canvas.put(x as i32, y as i32, TWINKLE[step], &style);
        }
        true
    }

    /// `count`, `colors`, `life_ms`, `duration_ms`: nothing is kept
    /// between two renders but the seed.
    fn tune(&mut self, params: &serde_json::Value) -> Result<(), String> {
        let keys = ["count", "colors", "life_ms", "duration_ms"];
        let new = tunable("sparkle", params, &keys)?;
        let old = &self.params;
        let colors = tuned("sparkle", new, "colors", old.colors.clone())?;
        if colors.is_empty() {
            return Err("sparkle: colors is empty".into());
        }
        self.params = SparkleParams {
            count: tuned("sparkle", new, "count", old.count)?,
            colors,
            life_ms: tuned("sparkle", new, "life_ms", old.life_ms)?,
            duration_ms: tuned("sparkle", new, "duration_ms", old.duration_ms)?,
            seed: old.seed,
        };
        Ok(())
    }

    fn period(&self) -> Duration {
        Duration::from_millis(self.params.life_ms.max(7) / TWINKLE.len() as u64)
            .clamp(Duration::from_millis(33), Duration::from_millis(250))
    }
}

/// Factory of the built-in `sparkle`. Params: `count`, `colors`,
/// `life_ms`, `duration_ms`, `seed`.
pub fn sparkle(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    let params: SparkleParams = parse("sparkle", params)?;
    if params.colors.is_empty() {
        return Err("sparkle: colors is empty".into());
    }
    Ok(Box::new(Sparkle { params }))
}

#[cfg(test)]
mod tune_tests {
    use super::*;
    use serde_json::json;

    fn drawn(effect: &mut dyn Effect, ms: u64) -> Option<usize> {
        let mut canvas = Canvas::new(40, 12);
        let alive = effect.render(Duration::from_millis(ms), &mut canvas);
        let slots = (0..12).flat_map(|y| (0..40).map(move |x| (x, y)));
        alive.then(|| {
            slots
                .filter(|&(x, y)| *canvas.slot(x, y) != super::super::raster::Slot::Empty)
                .count()
        })
    }

    #[test]
    fn confetti_tuned_keeps_its_pieces() {
        let mut fx = confetti(&json!({"count": 30, "duration_ms": 1000})).unwrap();
        assert!(drawn(&mut *fx, 300).is_some());
        for bad in [
            json!({"count": 3}),
            json!({"seed": 2}),
            json!({"origin": "here"}),
            json!([1]),
        ] {
            assert!(fx.tune(&bad).is_err(), "{bad}");
        }
        assert_eq!(drawn(&mut *fx, 1000), None);
        // Longer: the same pieces, later in a longer life.
        fx.tune(&json!({"duration_ms": 4000})).unwrap();
        assert!(drawn(&mut *fx, 1000).is_some());
        // Moved out of the canvas: alive, nothing shown.
        fx.tune(&json!({"origin": [500.0, 2.0]})).unwrap();
        assert_eq!(drawn(&mut *fx, 300), Some(0));
        fx.tune(&json!({"origin": null})).unwrap();
        assert!(drawn(&mut *fx, 300).unwrap() > 0);
    }

    #[test]
    fn sparkle_tuned() {
        let mut fx = sparkle(&json!({"count": 4, "life_ms": 700})).unwrap();
        let few = drawn(&mut *fx, 100).unwrap();
        assert!((1..=4).contains(&few));
        for bad in [
            json!({"seed": 2}),
            json!({"colors": []}),
            json!({"count": -1}),
        ] {
            assert!(fx.tune(&bad).is_err(), "{bad}");
        }
        assert_eq!(drawn(&mut *fx, 100), Some(few));
        fx.tune(&json!({"count": 120, "colors": [9]})).unwrap();
        assert!(drawn(&mut *fx, 100).unwrap() > few);
        fx.tune(&json!({"duration_ms": 50})).unwrap();
        assert_eq!(drawn(&mut *fx, 100), None);
    }
}
