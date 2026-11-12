// SPDX-License-Identifier: GPL-3.0-or-later
//! Falling sand: a native effect. Grains fall from the top of the canvas,
//! slide off slopes and pile on the walls the client gives.
//!
//! Effect `sand`, params:
//!
//! - `walls`: rows of text, top-left of the canvas; a cell that is not a
//!   space holds the sand. Absent: only the floor does.
//! - `rate`: grains per second (default 30).
//! - `colors`: grain colours (default: sand tones).
//! - `duration_ms`: life of the effect. Absent: until removed.
//! - `seed`.
//!
//! An effect does not see the screen: the client reads the frame's text,
//! passes it as `walls`, and starts the effect again when the text
//! changes.

use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::{Canvas, ColorSpec, Effect, EffectFactory, Rng, SpriteSpec, StyleSpec};
use ptyrc_vt::Color;
use serde::Deserialize;
use std::time::Duration;

/// One simulation step.
const STEP_MS: u64 = 50;
/// Steps run by one render at most: a late render does not freeze.
const CATCH_UP: u64 = 8;
/// Canvas cells at most.
const MAX_CELLS: usize = 1 << 20;

const FALLING: &str = "░";
const RESTING: &str = "▒";

#[derive(Deserialize)]
#[serde(default)]
struct Params {
    walls: Vec<String>,
    rate: f64,
    colors: Vec<ColorSpec>,
    duration_ms: Option<u64>,
    seed: u64,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            walls: Vec::new(),
            rate: 30.0,
            colors: [178, 179, 180, 214, 220, 221, 222]
                .iter()
                .map(|n| ColorSpec(Color::Indexed(*n)))
                .collect(),
            duration_ms: None,
            seed: 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Spot {
    Free,
    Wall,
    /// A grain: colour index, and whether it moved in the last step.
    Grain(u8, bool),
}

/// The pile. Public for tests and for other extensions.
pub struct Sand {
    params: Params,
    rng: Rng,
    w: usize,
    h: usize,
    spots: Vec<Spot>,
    steps: u64,
    /// Grains owed by the rate, fractional.
    owed: f64,
}

impl Sand {
    fn new(params: Params) -> Sand {
        Sand {
            rng: Rng::new(params.seed),
            params,
            w: 0,
            h: 0,
            spots: Vec::new(),
            steps: 0,
            owed: 0.0,
        }
    }

    /// A new canvas size: an empty pile between the walls.
    fn reset(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.spots = vec![Spot::Free; w * h];
        for (y, row) in self.params.walls.iter().take(h).enumerate() {
            let mut x = 0usize;
            for ch in row.chars() {
                let width = ptyrc_vt::char_width(ch).max(1) as usize;
                for k in 0..width {
                    if ch != ' ' && x + k < w {
                        self.spots[y * w + x + k] = Spot::Wall;
                    }
                }
                x += width;
            }
        }
    }

    fn free(&self, x: usize, y: usize) -> bool {
        self.spots[y * self.w + x] == Spot::Free
    }

    /// Grains in the pile.
    pub fn grains(&self) -> usize {
        self.spots
            .iter()
            .filter(|s| matches!(s, Spot::Grain(..)))
            .count()
    }

    fn step(&mut self) {
        let (w, h) = (self.w, self.h);
        // Bottom up, so a grain moves once per step.
        for y in (0..h).rev() {
            for x in 0..w {
                let Spot::Grain(color, _) = self.spots[y * w + x] else {
                    continue;
                };
                let mut to = None;
                if y + 1 < h {
                    if self.free(x, y + 1) {
                        to = Some(x);
                    } else {
                        // Slide: the side and the cell below it are free.
                        let left = x > 0 && self.free(x - 1, y) && self.free(x - 1, y + 1);
                        let right = x + 1 < w && self.free(x + 1, y) && self.free(x + 1, y + 1);
                        to = match (left, right) {
                            (true, true) if self.rng.next_u64() & 1 == 0 => Some(x - 1),
                            (true, false) => Some(x - 1),
                            (_, true) => Some(x + 1),
                            _ => None,
                        };
                    }
                }
                match to {
                    Some(nx) => {
                        self.spots[y * w + x] = Spot::Free;
                        self.spots[(y + 1) * w + nx] = Spot::Grain(color, true);
                    }
                    None => self.spots[y * w + x] = Spot::Grain(color, false),
                }
            }
        }
        self.owed += self.params.rate * STEP_MS as f64 / 1000.0;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let x = (self.rng.next_u64() % w as u64) as usize;
            if self.free(x, 0) {
                let color = (self.rng.next_u64() % self.params.colors.len() as u64) as u8;
                self.spots[x] = Spot::Grain(color, true);
            }
        }
        self.steps += 1;
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
        if (w, h) != (self.w, self.h) {
            self.reset(w, h);
        }
        let due = ms / STEP_MS;
        if due > self.steps + CATCH_UP {
            self.steps = due - CATCH_UP;
        }
        while self.steps < due {
            self.step();
        }
        for y in 0..h {
            for x in 0..w {
                if let Spot::Grain(color, moving) = self.spots[y * w + x] {
                    let style = StyleSpec::fg(self.params.colors[color as usize].0);
                    let glyph = if moving { FALLING } else { RESTING };
                    canvas.put(x as i32, y as i32, glyph, &style);
                }
            }
        }
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(STEP_MS)
    }
}

/// Factory of the `sand` effect.
pub fn sand(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    let params: Params = if params.is_null() {
        Params::default()
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("sand: {e}"))?
    };
    if params.colors.is_empty() || params.colors.len() > 255 {
        return Err("sand: colors: 1 to 255 are needed".into());
    }
    if !(params.rate.is_finite() && (0.0..=10_000.0).contains(&params.rate)) {
        return Err("sand: rate is out of range".into());
    }
    Ok(Box::new(Sand::new(params)))
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
mod tests {
    use super::*;
    use serde_json::json;

    fn run(params: serde_json::Value, w: u16, h: u16, ms: u64) -> (Sand, Canvas) {
        let params: Params = serde_json::from_value(params).unwrap();
        let mut sand = Sand::new(params);
        let mut canvas = Canvas::new(w, h);
        for t in (0..=ms).step_by(STEP_MS as usize) {
            canvas.clear();
            assert!(sand.render(Duration::from_millis(t), &mut canvas));
        }
        (sand, canvas)
    }

    fn rows(sand: &Sand) -> Vec<String> {
        (0..sand.h)
            .map(|y| {
                (0..sand.w)
                    .map(|x| match sand.spots[y * sand.w + x] {
                        Spot::Free => ' ',
                        Spot::Wall => '#',
                        Spot::Grain(..) => 'o',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn piles_on_the_floor_and_fills() {
        let (sand, _) = run(json!({"rate": 200}), 10, 5, 20_000);
        assert_eq!(sand.grains(), 50, "{:#?}", rows(&sand));
    }

    #[test]
    fn walls_hold_the_sand() {
        let walls = ["", "", "  ######", "", ""];
        let (sand, _) = run(json!({"rate": 100, "walls": walls}), 10, 5, 20_000);
        let rows = rows(&sand);
        assert_eq!(rows[2], "oo######oo");
        // Sand lies on the wall, and under it only what slid in.
        assert_eq!(&rows[1][2..8], "oooooo");
        assert_eq!(&rows[4][4..6], "  ", "{rows:#?}");
    }

    #[test]
    fn same_seed_same_pile() {
        let a = rows(&run(json!({"seed": 7}), 20, 8, 3000).0);
        let b = rows(&run(json!({"seed": 7}), 20, 8, 3000).0);
        let c = rows(&run(json!({"seed": 8}), 20, 8, 3000).0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn a_late_render_catches_up_bounded() {
        let mut sand = Sand::new(Params::default());
        let mut canvas = Canvas::new(8, 4);
        assert!(sand.render(Duration::from_secs(3600), &mut canvas));
        assert_eq!(sand.steps, 3600 * 1000 / STEP_MS);
    }

    #[test]
    fn ends_and_rejects() {
        let (mut sand, mut canvas) = run(json!({"duration_ms": 500}), 8, 4, 400);
        assert!(!sand.render(Duration::from_millis(500), &mut canvas));
        assert!(super::sand(&json!({"colors": []})).is_err());
        assert!(super::sand(&json!({"rate": -1})).is_err());
        assert!(super::sand(&json!(null)).is_ok());
    }
}
