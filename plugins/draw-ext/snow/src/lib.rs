// SPDX-License-Identifier: GPL-3.0-or-later
//! Snow: a native effect. Flakes drift down behind the app's text,
//! settle on the top of its lines and on the floor, and melt where the
//! screen is warm.
//!
//! Effect `snow`, params:
//!
//! - `rate`: flakes per second over 80 columns (default 14).
//! - `wind`: drift, columns per second, to the right (default 0.6).
//! - `duration_ms`: life of the effect. Absent: until removed.
//! - `seed`.
//!
//! `tune`: `rate`, `wind`, `duration_ms`; the flakes and the snow stay.
//!
//! A flake is near (bright) or far (slow, faint: drawn at half
//! opacity). It sways as it falls, and is not drawn in the blank
//! between two words.
//!
//! Settling. A ledge is a cell without ink, and none beside it, over a
//! cell with ink, over
//! the blank between two such cells, over a cell full of snow, or the
//! last cell of a column: snow lies over a line, not between its words. A flake takes a
//! ledge over text with probability `TAKE`, any other always. It adds
//! `GAIN` eighths to the cell, or to the lower of its two neighbours
//! when that one is a ledge and lower by more than `GAIN`: drifts are
//! smooth. A cell is drawn `▁` to `█`. The last cell of a column
//! fills up, eight eighths, then holds `CAP` more in the cell above;
//! text holds `CAP` eighths: a white cap on a line, a drift on the floor.
//!
//! The snow lies on the rows of the frame: it scrolls with the text,
//! and is kept out of sight (`ATTIC` rows around the view).
//!
//! Thaw. Lying snow loses an eighth every `THAW_MS` on average: under a
//! light fall the snow thins from the top lines down and never covers
//! all.
//!
//! Melting. Snow goes in `MELT_MS` where the cell, or the cell that
//! holds it, changed lately (heat class `hot`), and within `WARM`
//! columns of the cursor on its row and the row above. Snow written
//! over is gone. Snow that lost what held it (text erased, a floor
//! scrolled away) falls again as flakes.

use ptyrc_core::frames::HeatClass;
use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::{Canvas, Effect, EffectFactory, Looks, Rng, SpriteSpec, StyleSpec, Under};
use ptyrc_vt::Color;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

/// Time between two renders.
const FRAME_MS: u64 = 80;
/// A late render advances the snow by this much at most.
const CATCH_UP_MS: u64 = 400;
/// Canvas cells at most.
const MAX_CELLS: usize = 1 << 18;
/// Flakes at once, at most.
const MAX_FLAKES: usize = 4096;
/// Chance that a flake settles on a ledge over text.
const TAKE: f64 = 0.25;
/// Eighths of a cell a flake adds.
const GAIN: f32 = 2.0;
/// Eighths in a full cell.
const FULL: f32 = 8.0;
/// Eighths of snow on text, and on a full cell of snow, at most.
const CAP: f32 = 4.0;
/// A full cell melts in this long.
const MELT_MS: f32 = 1500.0;
/// Lying snow loses an eighth in this long, on average.
const THAW_MS: f64 = 30_000.0;
/// Columns on each side of the cursor that are warm.
const WARM: i64 = 3;
/// Rows of snow kept above and below the view.
const ATTIC: i64 = 2000;
/// Speed, rows per second: near, far.
const NEAR: (f64, f64) = (2.6, 4.2);
const FAR: (f64, f64) = (1.2, 2.0);
/// Share of the flakes that are far.
const FAR_SHARE: f64 = 0.4;
/// Sway: columns, and turns per second.
const SWAY: f64 = 1.1;
const TURN: (f64, f64) = (0.15, 0.4);
/// Opacity of a far flake.
const FAINT: f32 = 0.5;
/// Glyphs of a near flake as it turns, and of a far one.
const FLAKES: [&str; 4] = ["*", "+", "*", "·"];
const SPECK: &str = "·";
/// Depths of a cell, one eighth to eight.
const DRIFT: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// Colours: a near flake, a far one, the snow.
const BRIGHT: Color = Color::Rgb(238, 243, 255);
const PALE: Color = Color::Rgb(150, 165, 190);
const WHITE: Color = Color::Rgb(226, 232, 245);

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Params {
    rate: f64,
    wind: f64,
    duration_ms: Option<u64>,
    seed: u64,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            rate: 14.0,
            wind: 0.6,
            duration_ms: None,
            seed: 1,
        }
    }
}

struct Flake {
    /// Column it falls along, before the sway.
    x: f64,
    /// Rows from the top.
    y: f64,
    /// Rows per second.
    v: f64,
    phase: f64,
    turn: f64,
    far: bool,
}

impl Flake {
    /// The cell it is in at `secs`.
    fn col(&self, secs: f64) -> i64 {
        let turn = (self.phase + self.turn * secs) * std::f64::consts::TAU;
        (self.x + SWAY * turn.sin()).floor() as i64
    }
}

/// The snow. Public for tests and for other extensions.
pub struct Snow {
    params: Params,
    rng: Rng,
    w: usize,
    h: usize,
    /// Foreground of the app and cells that changed lately, a cell each,
    /// row-major, `seen_w` a row.
    ink: Vec<bool>,
    hot: Vec<bool>,
    seen_w: usize,
    cursor: Option<(u16, u16)>,
    /// Frame position of the canvas cell `(0, 0)`.
    origin: (i64, i64),
    /// Eighths of snow by frame cell, `(row, column)`.
    lying: BTreeMap<(i64, i64), f32>,
    flakes: Vec<Flake>,
    /// Time of the last render.
    last: Option<u64>,
    /// Flakes owed, a fraction.
    owed: f64,
}

impl Snow {
    fn new(params: Params) -> Snow {
        Snow {
            rng: Rng::new(params.seed),
            params,
            w: 0,
            h: 0,
            ink: Vec::new(),
            hot: Vec::new(),
            seen_w: 0,
            cursor: None,
            origin: (0, 0),
            lying: BTreeMap::new(),
            flakes: Vec::new(),
            last: None,
            owed: 0.0,
        }
    }

    fn seen(&self, map: &[bool], x: usize, y: usize) -> bool {
        x < self.seen_w && map.get(y * self.seen_w + x).is_some_and(|b| *b)
    }

    fn wall(&self, x: usize, y: usize) -> bool {
        self.seen(&self.ink, x, y)
    }

    /// Eighths of snow in the canvas cell.
    pub fn depth(&self, x: usize, y: usize) -> f32 {
        let key = (self.origin.1 + y as i64, self.origin.0 + x as i64);
        self.lying.get(&key).copied().unwrap_or(0.0)
    }

    /// Eighths of snow in sight.
    pub fn lying(&self) -> f32 {
        (0..self.h)
            .flat_map(|y| (0..self.w).map(move |x| (x, y)))
            .map(|(x, y)| self.depth(x, y))
            .sum()
    }

    fn put(&mut self, x: usize, y: usize, depth: f32) {
        let key = (self.origin.1 + y as i64, self.origin.0 + x as i64);
        if depth > 0.0 {
            self.lying.insert(key, depth.min(FULL));
        } else {
            self.lying.remove(&key);
        }
    }

    /// Something holds snow in `(x, y)`: text, the blank between two
    /// words, full snow, the floor.
    fn held(&self, x: usize, y: usize) -> bool {
        let gap = || x > 0 && self.wall(x - 1, y + 1) && self.wall(x + 1, y + 1);
        y + 1 >= self.h || self.wall(x, y + 1) || gap() || self.depth(x, y + 1) >= FULL
    }

    /// Eighths the cell `(x, y)` takes: a full cell on the floor, a cap
    /// on text and on snow.
    fn room(&self, y: usize) -> f32 {
        if y + 1 >= self.h { FULL } else { CAP }
    }

    /// Snow may lie in `(x, y)`: free, a cell away from the text of its
    /// row, held, and with room.
    fn ledge(&self, x: usize, y: usize) -> bool {
        let text = self.wall(x, y) || self.wall(x + 1, y) || (x > 0 && self.wall(x - 1, y));
        let clear = !text;
        clear && self.held(x, y) && self.depth(x, y) < self.room(y)
    }

    /// A flake comes to rest in `(x, y)`.
    fn settle(&mut self, x: usize, y: usize) {
        let mut at = x;
        let mut low = self.depth(x, y);
        for nx in [x.wrapping_sub(1), x + 1] {
            if nx < self.w && self.ledge(nx, y) && self.depth(nx, y) + GAIN < low {
                (at, low) = (nx, self.depth(nx, y));
            }
        }
        self.put(at, y, (low + GAIN).min(self.room(y)));
    }

    fn spawn(&mut self, dt: f64) {
        self.owed += self.params.rate * self.w as f64 / 80.0 * dt;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            if self.flakes.len() >= MAX_FLAKES {
                continue;
            }
            let far = self.rng.unit() < FAR_SHARE;
            let (lo, hi) = if far { FAR } else { NEAR };
            let flake = Flake {
                x: self.rng.range(0.0, self.w as f64),
                y: 0.0,
                v: self.rng.range(lo, hi),
                phase: self.rng.unit(),
                turn: self.rng.range(TURN.0, TURN.1),
                far,
            };
            self.flakes.push(flake);
        }
    }

    fn fall(&mut self, secs: f64, dt: f64) {
        let (w, h) = (self.w as i64, self.h);
        let mut flakes = std::mem::take(&mut self.flakes);
        flakes.retain_mut(|f| {
            let from = f.y as usize;
            f.y += f.v * dt;
            f.x += self.params.wind * dt * if f.far { 0.6 } else { 1.0 };
            let x = f.col(secs);
            if x < 0 || x >= w {
                // Blown out: it comes back by the other side.
                f.x = f.x.rem_euclid(w as f64);
                return (f.y as usize) < h;
            }
            let x = x as usize;
            // Each row it enters, once.
            for y in (from + 1)..=(f.y as usize).min(h - 1) {
                if self.depth(x, y) >= FULL {
                    // The cell filled up under it: it stays above.
                    if self.ledge(x, y - 1) {
                        self.settle(x, y - 1);
                    }
                    return false;
                }
                if !self.ledge(x, y) {
                    continue;
                }
                let sure = y + 1 >= h || self.depth(x, y + 1) >= FULL;
                if sure || self.rng.unit() < TAKE {
                    self.settle(x, y);
                    return false;
                }
            }
            (f.y as usize) < h
        });
        self.flakes = flakes;
    }

    /// The snow in sight: written over, melting, or let go.
    fn weather(&mut self, dt: f64) {
        let melt = FULL * (dt * 1000.0) as f32 / MELT_MS;
        for y in (0..self.h).rev() {
            for x in 0..self.w {
                let depth = self.depth(x, y);
                if depth <= 0.0 {
                    continue;
                }
                if self.wall(x, y) {
                    self.put(x, y, 0.0);
                    continue;
                }
                if !self.held(x, y) {
                    // Nothing under it: it falls again.
                    self.put(x, y, 0.0);
                    for k in 0..(depth / GAIN).ceil() as usize {
                        if self.flakes.len() < MAX_FLAKES {
                            let flake = Flake {
                                x: x as f64 + 0.5,
                                y: y as f64 + 0.3 * k as f64,
                                v: self.rng.range(NEAR.0, NEAR.1) * 1.5,
                                phase: self.rng.unit(),
                                turn: self.rng.range(TURN.0, TURN.1),
                                far: false,
                            };
                            self.flakes.push(flake);
                        }
                    }
                    continue;
                }
                let warm = self.cursor.is_some_and(|(cx, cy)| {
                    let (dx, dy) = (cx as i64 - x as i64, cy as i64 - y as i64);
                    dx.abs() <= WARM && (0..=1).contains(&dy)
                });
                let hot = self.seen(&self.hot, x, y) || self.seen(&self.hot, x, y + 1);
                if warm || hot {
                    self.put(x, y, depth - melt);
                } else if self.rng.unit() < dt * 1000.0 / THAW_MS {
                    self.put(x, y, depth - 1.0);
                }
            }
        }
    }

    fn draw(&self, secs: f64, canvas: &mut Canvas) {
        let (bright, pale, white) = (
            StyleSpec::fg(BRIGHT),
            StyleSpec::fg(PALE),
            StyleSpec::fg(WHITE),
        );
        // Not in the blank between two words: the text stays easy to read.
        let open = |f: &&Flake| {
            let (x, y) = (f.col(secs), f.y as usize);
            x < 1 || !(self.wall(x as usize - 1, y) && self.wall(x as usize + 1, y))
        };
        for f in self.flakes.iter().filter(|f| f.far).filter(open) {
            canvas.put_faded(f.col(secs) as i32, f.y as i32, SPECK, &pale, FAINT);
        }
        for f in self.flakes.iter().filter(|f| !f.far).filter(open) {
            let face = ((f.phase + f.turn * secs) * 4.0) as usize % FLAKES.len();
            canvas.put(f.col(secs) as i32, f.y as i32, FLAKES[face], &bright);
        }
        for y in 0..self.h {
            for x in 0..self.w {
                let depth = self.depth(x, y);
                if depth > 0.0 {
                    let level = (depth.ceil() as usize).clamp(1, 8);
                    canvas.put(x as i32, y as i32, DRIFT[level - 1], &white);
                }
            }
        }
    }
}

impl Effect for Snow {
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
            (self.w, self.h) = (w, h);
            self.flakes.retain(|f| f.x < w as f64 && (f.y as usize) < h);
        }
        let dt = ms.saturating_sub(self.last.unwrap_or(ms)).min(CATCH_UP_MS) as f64 / 1000.0;
        self.last = Some(ms);
        let secs = ms as f64 / 1000.0;
        self.weather(dt);
        self.spawn(dt);
        self.fall(secs, dt);
        self.draw(secs, canvas);
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(FRAME_MS)
    }

    fn looks(&self) -> Looks {
        Looks::Heat
    }

    fn look(&mut self, under: &Under) {
        let (w, h) = under.size;
        self.seen_w = w as usize;
        self.ink = under.inked();
        self.hot = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| under.heat(x, y).is_some_and(|w| w.class == HeatClass::Hot))
            .collect();
        self.cursor = under.cursor;
        if under.origin != self.origin {
            self.origin = under.origin;
            let rows = (self.origin.1 - ATTIC, self.origin.1 + h as i64 + ATTIC);
            self.lying.retain(|(y, _), _| *y >= rows.0 && *y < rows.1);
        }
    }

    fn tune(&mut self, params: &serde_json::Value) -> Result<(), String> {
        let Some(new) = params.as_object() else {
            return Err("snow: params: an object is needed".into());
        };
        let mut merged = serde_json::Map::new();
        merged.insert("rate".into(), self.params.rate.into());
        merged.insert("wind".into(), self.params.wind.into());
        merged.insert("duration_ms".into(), self.params.duration_ms.into());
        merged.extend(new.clone());
        let params = checked(&merged.into())?;
        self.params = Params {
            seed: self.params.seed,
            ..params
        };
        Ok(())
    }
}

fn checked(params: &serde_json::Value) -> Result<Params, String> {
    let params: Params = if params.is_null() {
        Params::default()
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("snow: {e}"))?
    };
    if !(params.rate.is_finite() && (0.0..=1_000.0).contains(&params.rate)) {
        return Err("snow: rate is out of range".into());
    }
    if !(params.wind.is_finite() && (-20.0..=20.0).contains(&params.wind)) {
        return Err("snow: wind is out of range".into());
    }
    Ok(params)
}

/// Factory of the `snow` effect.
pub fn snow(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    Ok(Box::new(Snow::new(checked(params)?)))
}

/// The extension: one effect, no draw function.
pub struct SnowExt;

impl DrawExt for SnowExt {
    fn name(&self) -> &'static str {
        "snow"
    }

    fn functions(&self) -> Vec<FnDoc> {
        Vec::new()
    }

    fn draw(&self, function: &str, _params: &serde_json::Value) -> Result<SpriteSpec, String> {
        Err(format!(
            "snow: no function {function}; use the effect `snow`"
        ))
    }

    fn effects(&self) -> Vec<(&'static str, EffectFactory)> {
        vec![("snow", Box::new(snow))]
    }
}

#[cfg(test)]
mod tests;
