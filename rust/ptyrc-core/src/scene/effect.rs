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

    /// Time between two renders.
    fn period(&self) -> Duration {
        Duration::from_millis(50)
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
            count: 48,
            colors: [9, 10, 11, 12, 13, 14]
                .iter()
                .map(|n| ColorSpec(Color::Indexed(*n)))
                .collect(),
            duration_ms: 2500,
            origin: None,
            spread: 1.0,
            seed: 1,
        }
    }
}

struct Particle {
    vx: f64,
    vy: f64,
    /// Life, as a share of the duration.
    life: f64,
    color: Color,
    phase: usize,
}

/// A burst of particles thrown up, falling under gravity.
struct Confetti {
    params: ConfettiParams,
    particles: Vec<Particle>,
}

const GLYPHS: [&str; 6] = ["*", "o", "+", "x", "·", "."];
/// Rows per second squared.
const GRAVITY: f64 = 14.0;

impl Confetti {
    fn new(params: ConfettiParams) -> Confetti {
        let mut rng = Rng::new(params.seed);
        let particles = (0..params.count.min(2000))
            .map(|_| {
                let angle = rng.range(0.0, std::f64::consts::TAU);
                let speed = rng.range(3.0, 12.0) * params.spread;
                Particle {
                    // Cells are about twice as tall as wide.
                    vx: angle.cos() * speed * 2.0,
                    vy: angle.sin() * speed - 6.0 * params.spread,
                    life: rng.range(0.5, 1.0),
                    color: params.colors[(rng.next_u64() % params.colors.len() as u64) as usize].0,
                    phase: (rng.next_u64() % 3) as usize,
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
        for p in &self.particles {
            let age = secs / (total * p.life);
            if age >= 1.0 {
                continue;
            }
            let x = ox + p.vx * secs;
            let y = oy + p.vy * secs + 0.5 * GRAVITY * secs * secs;
            let step = (age * 4.0) as usize + p.phase;
            let glyph = GLYPHS[step.min(GLYPHS.len() - 1)];
            let style = StyleSpec {
                bold: age < 0.5,
                dim: age > 0.8,
                ..StyleSpec::fg(p.color)
            };
            canvas.put(x.round() as i32, y.round() as i32, glyph, &style);
        }
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(40)
    }
}

/// Factory of the built-in `confetti`. Params: `count`, `colors`,
/// `duration_ms`, `origin` (`[x, y]` on the canvas), `spread`, `seed`.
pub fn confetti(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    let params: ConfettiParams = if params.is_null() {
        ConfettiParams::default()
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("confetti: {e}"))?
    };
    if params.colors.is_empty() {
        return Err("confetti: colors is empty".into());
    }
    if !params.spread.is_finite() {
        return Err("confetti: spread is not finite".into());
    }
    Ok(Box::new(Confetti::new(params)))
}
