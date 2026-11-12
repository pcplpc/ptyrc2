// SPDX-License-Identifier: GPL-3.0-or-later
//! Native effects: generators that own a sprite's cells.

use super::raster::Canvas;
use super::spec::{ColorSpec, StyleSpec};
use ptyrc_vt::Color;
use serde::Deserialize;
use std::time::Duration;

/// A generator. `render` draws the state at `t` (time since the sprite was
/// added) on a cleared canvas and returns `false` once finished: the
/// sprite is then removed.
pub trait Effect: Send {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool;

    /// Whether `look` is wanted.
    fn looks(&self) -> bool {
        false
    }

    /// What the app shows under the canvas; called before each `render`
    /// while the frame is usable.
    fn look(&mut self, _under: &Under) {}

    /// Time between two renders.
    fn period(&self) -> Duration {
        Duration::from_millis(50)
    }
}

/// The app under an effect's canvas. Exact when the canvas is the view
/// of its frame (no `size`, default anchor).
pub struct Under<'a> {
    /// Rows of the view, top first, trailing spaces trimmed.
    pub rows: &'a [String],
    /// The cursor as a canvas cell; `None`: outside.
    pub cursor: Option<(u16, u16)>,
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

    fn period(&self) -> Duration {
        Duration::from_millis(33)
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
    if let Some([x, y]) = params.origin
        && !(x.is_finite() && y.is_finite())
    {
        return Err("confetti: origin is not finite".into());
    }
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
