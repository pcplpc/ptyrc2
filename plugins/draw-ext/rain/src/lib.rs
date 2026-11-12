// SPDX-License-Identifier: GPL-3.0-or-later
//! Quiet rain: a native effect. Drops fall behind the app's text and
//! splash on the top of its lines, on the cursor and on the floor.
//!
//! Effect `rain`, params:
//!
//! - `rate`: drops per second over 80 columns of still text (default
//!   20).
//! - `duration_ms`: life of the effect. Absent: until removed.
//! - `seed`.
//!
//! A drop is near (fast, bright, with a tail) or far (slow, dark). It
//! falls down one column. Drawn on a back layer, it shows in the cells
//! the app leaves free.
//!
//! Landing. A ledge is a free cell over a cell with text, over the
//! cursor, or the last cell of a column. A drop takes each ledge on its
//! way with probability `TAKE`, the cursor and the floor always: it
//! falls behind some lines and splashes on the next. A text change
//! draws the landing again, below the drop.
//!
//! Cold. The effect sees the text, not the heat: it keeps, per column,
//! the time of the last change of a cell within `REACH` columns. Rain
//! falls on a column in proportion of its stillness, from `THIN` just
//! after a change to 1 after `STILL_MS`.

use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::{Canvas, Effect, EffectFactory, Rng, SpriteSpec, StyleSpec, Under};
use ptyrc_vt::Color;
use serde::Deserialize;
use std::time::Duration;

/// Time between two renders.
const FRAME_MS: u64 = 40;
/// A late render advances the rain by this much at most.
const CATCH_UP_MS: u64 = 200;
/// Canvas cells at most.
const MAX_CELLS: usize = 1 << 18;
/// Drops at once, at most.
const MAX_DROPS: usize = 4096;
/// Chance that a drop lands on a ledge it meets.
const TAKE: f64 = 0.45;
/// Columns on each side of a change that count as changed.
const REACH: usize = 3;
/// A column is cold after this long without a change.
const STILL_MS: u64 = 10_000;
/// Share of the rain that falls on a column just changed.
const THIN: f64 = 0.1;
/// Stillness assumed at the start.
const START_MS: u64 = 5_000;
/// Speed, rows per second: near, far.
const NEAR: (f64, f64) = (26.0, 38.0);
const FAR: (f64, f64) = (12.0, 18.0);
/// Share of the drops that are far.
const FAR_SHARE: f64 = 0.45;
/// Life of a splash.
const SPLASH_MS: u64 = 280;
/// Colours of the 256 palette: a near drop, its tail, a far drop, a
/// splash.
const HEAD: u8 = 110;
const TAIL: u8 = 67;
const DARK: u8 = 24;
const SPRAY: u8 = 153;

#[derive(Deserialize)]
#[serde(default)]
struct Params {
    rate: f64,
    duration_ms: Option<u64>,
    seed: u64,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            rate: 20.0,
            duration_ms: None,
            seed: 1,
        }
    }
}

struct Drop {
    x: usize,
    /// Rows from the top; the drop is in cell `y as usize`.
    y: f64,
    /// Rows per second.
    v: f64,
    far: bool,
    /// The row it lands in.
    land: usize,
}

struct Splash {
    x: usize,
    y: usize,
    born: u64,
}

struct Rain {
    params: Params,
    rng: Rng,
    w: usize,
    h: usize,
    /// The text, a glyph per cell, row-major; empty until the app is
    /// seen.
    text: Vec<char>,
    rows: Vec<String>,
    cursor: Option<(u16, u16)>,
    /// The text changed since the last render.
    stale: bool,
    /// Per column: changed since the last render.
    touched: Vec<bool>,
    /// Per column: time of the last change; `None`: never seen changing.
    changed: Vec<Option<u64>>,
    drops: Vec<Drop>,
    splashes: Vec<Splash>,
    /// Time of the last render.
    last: Option<u64>,
    /// Drops owed, a fraction.
    owed: f64,
}

impl Rain {
    fn new(params: Params) -> Rain {
        Rain {
            rng: Rng::new(params.seed),
            params,
            w: 0,
            h: 0,
            text: Vec::new(),
            rows: Vec::new(),
            cursor: None,
            stale: true,
            touched: Vec::new(),
            changed: Vec::new(),
            drops: Vec::new(),
            splashes: Vec::new(),
            last: None,
            owed: 0.0,
        }
    }

    fn wall(&self, x: usize, y: usize) -> bool {
        self.text
            .get(y * self.w + x)
            .is_some_and(|c| !c.is_whitespace())
    }

    /// Something a drop lands on is under cell `(x, y)`.
    fn ledge(&self, x: usize, y: usize) -> bool {
        y + 1 >= self.h || self.wall(x, y + 1) || self.cursor == Some((x as u16, y as u16 + 1))
    }

    /// The row where a drop of column `x`, now in row `from`, lands.
    fn landing(&mut self, x: usize, from: usize) -> usize {
        for y in from..self.h {
            if self.wall(x, y) || !self.ledge(x, y) {
                continue;
            }
            let sure = y + 1 >= self.h || self.cursor == Some((x as u16, y as u16 + 1));
            if sure || self.rng.unit() < TAKE {
                return y;
            }
        }
        // The last row holds text: the drop ends behind it.
        self.h
    }

    /// Read the text again: walls, columns that changed, landings.
    fn build(&mut self) {
        let (w, h) = (self.w, self.h);
        let mut text = vec![' '; w * h];
        for (y, row) in self.rows.iter().take(h).enumerate() {
            for (x, ch) in row.chars().take(w).enumerate() {
                text[y * w + x] = ch;
            }
        }
        if self.text.len() == text.len() {
            for (i, (a, b)) in self.text.iter().zip(&text).enumerate() {
                if a != b {
                    let x = i % w;
                    let hi = (x + REACH).min(w - 1);
                    for t in &mut self.touched[x.saturating_sub(REACH)..=hi] {
                        *t = true;
                    }
                }
            }
        }
        self.text = text;
        for i in 0..self.drops.len() {
            let (x, from) = (self.drops[i].x, self.drops[i].y as usize);
            self.drops[i].land = self.landing(x, from);
        }
        self.stale = false;
    }

    fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.text = Vec::new();
        self.touched = vec![false; w];
        self.changed.resize(w, None);
        self.drops.retain(|d| d.x < w && (d.y as usize) < h);
        self.splashes.retain(|s| s.x < w && s.y < h);
        self.stale = true;
    }

    /// Share of the rain that falls on column `x` at `ms`.
    fn weight(&self, x: usize, ms: u64) -> f64 {
        let still = match self.changed[x] {
            Some(at) => ms.saturating_sub(at),
            None => ms + START_MS,
        };
        THIN + (1.0 - THIN) * (still as f64 / STILL_MS as f64).min(1.0)
    }

    fn spawn(&mut self, ms: u64, dt: f64) {
        self.owed += self.params.rate * self.w as f64 / 80.0 * dt;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let x = ((self.rng.unit() * self.w as f64) as usize).min(self.w - 1);
            if self.rng.unit() >= self.weight(x, ms) || self.drops.len() >= MAX_DROPS {
                continue;
            }
            let far = self.rng.unit() < FAR_SHARE;
            let (lo, hi) = if far { FAR } else { NEAR };
            let v = self.rng.range(lo, hi);
            let land = self.landing(x, 0);
            self.drops.push(Drop {
                x,
                y: 0.0,
                v,
                far,
                land,
            });
        }
    }

    fn fall(&mut self, ms: u64, dt: f64) {
        let h = self.h;
        let mut landed = Vec::new();
        self.drops.retain_mut(|d| {
            d.y += d.v * dt;
            if (d.y as usize) < d.land.min(h) {
                return true;
            }
            if d.land < h {
                landed.push(Splash {
                    x: d.x,
                    y: d.land,
                    born: ms,
                });
            }
            false
        });
        self.splashes.append(&mut landed);
        self.splashes.retain(|s| ms < s.born + SPLASH_MS);
    }

    fn draw(&self, ms: u64, canvas: &mut Canvas) {
        let ink = |n: u8| StyleSpec::fg(Color::Indexed(n));
        let (head, tail, dark, spray) = (ink(HEAD), ink(TAIL), ink(DARK), ink(SPRAY));
        // Far drops first: a near one passes in front.
        for d in self.drops.iter().filter(|d| d.far) {
            canvas.put(d.x as i32, d.y as i32, "╎", &dark);
        }
        for d in self.drops.iter().filter(|d| !d.far) {
            canvas.put(d.x as i32, d.y as i32 - 1, "╷", &tail);
            canvas.put(d.x as i32, d.y as i32, "│", &head);
        }
        for s in &self.splashes {
            let (x, y) = (s.x as i32, s.y as i32);
            match (ms - s.born) * 3 / SPLASH_MS {
                0 => canvas.put(x, y, ".", &spray),
                1 => {
                    canvas.put(x - 1, y, "'", &spray);
                    canvas.put(x + 1, y, "'", &spray);
                }
                _ => {
                    canvas.put(x - 2, y, "·", &tail);
                    canvas.put(x + 2, y, "·", &tail);
                }
            }
        }
    }
}

impl Effect for Rain {
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
            self.resize(w, h);
        }
        if self.stale {
            self.build();
        }
        for (touched, changed) in self.touched.iter_mut().zip(&mut self.changed) {
            if std::mem::take(touched) {
                *changed = Some(ms);
            }
        }
        let dt = ms.saturating_sub(self.last.unwrap_or(ms)).min(CATCH_UP_MS) as f64 / 1000.0;
        self.last = Some(ms);
        self.spawn(ms, dt);
        self.fall(ms, dt);
        self.draw(ms, canvas);
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(FRAME_MS)
    }

    fn looks(&self) -> bool {
        true
    }

    fn look(&mut self, under: &Under) {
        if self.rows != under.rows || self.cursor != under.cursor {
            self.rows = under.rows.to_vec();
            self.cursor = under.cursor;
            self.stale = true;
        }
    }
}

/// Factory of the `rain` effect.
pub fn rain(params: &serde_json::Value) -> Result<Box<dyn Effect>, String> {
    let params: Params = if params.is_null() {
        Params::default()
    } else {
        serde_json::from_value(params.clone()).map_err(|e| format!("rain: {e}"))?
    };
    if !(params.rate.is_finite() && (0.0..=2_000.0).contains(&params.rate)) {
        return Err("rain: rate is out of range".into());
    }
    Ok(Box::new(Rain::new(params)))
}

/// The extension: one effect, no draw function.
pub struct RainExt;

impl DrawExt for RainExt {
    fn name(&self) -> &'static str {
        "rain"
    }

    fn functions(&self) -> Vec<FnDoc> {
        Vec::new()
    }

    fn draw(&self, function: &str, _params: &serde_json::Value) -> Result<SpriteSpec, String> {
        Err(format!(
            "rain: no function {function}; use the effect `rain`"
        ))
    }

    fn effects(&self) -> Vec<(&'static str, EffectFactory)> {
        vec![("rain", Box::new(rain))]
    }
}

#[cfg(test)]
mod tests;
