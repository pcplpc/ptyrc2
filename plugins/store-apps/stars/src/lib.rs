// SPDX-License-Identifier: GPL-3.0-or-later
//! Stars: a night sky behind the text, a moon that crosses it, grass in
//! a breeze along the bottom.
//!
//! The sky is a map (`sky`): a star has a place `(u, v)` in [0, 1),
//! laid over the view whatever its size. The moon, the faint stars and
//! the grass are Braille, 2 by 4 dots a cell. The grass grows with the
//! view: twice as tall on 50 rows as on 15 (`scene::growth`).
//!
//! - Two canvases the size of the view's, behind the text: the sky,
//!   looked at twice a second, and the grass over it, 6 times a second
//!   (`scene::Night`). `--no-grass`: the sky alone, to the last row.
//! - The moon: tonight's phase (`grid::phase_of`, the wall clock), the
//!   lit side drawn, the dark one hiding the stars behind it. It
//!   crosses the sky in `--night` seconds, then stays down a fifth of
//!   that.
//! - The pointer in the grass (`hover`, `scene::Hand`): the blades part
//!   and lean the way it goes; let go, they swing back.
//! - `-m`, `-b`: sprites the driver plays: a meteor, a finite animation
//!   with a `motion`; a firefly, looped, moved when it changes cell.
//! - The text, read as a shape (`ptyrc_app::sight`): nothing is drawn
//!   in the cell on each side of a glyph.
//!
//! Every draw is Python's `random.Random` (`twister`), seeded from
//! `random` (the launcher's `--seed N`): the twin's scene for a seed.
//! Steps are counted from session time: a tick a twelfth of a second.

mod grid;
pub mod scene;
mod sky;
#[cfg(test)]
mod twin;
mod twister;

use grid::{Mask, clear};
use ptyrc_app::sight::{self, Sight};
use ptyrc_app::{
    App, Canvas, Value, call, exit, json, log, period, put, random_u32, remove, subscribe,
};
use scene::{Bug, Hand, METEOR, Night, Options, meteor};
use twister::Twister;

/// Ticks a second.
pub const TICKS: u64 = 12;
/// Ticks between two looks at the grass in the breeze.
pub const BEAT_OF: u64 = 2;
/// Ticks between two looks at the sky.
pub const SKY: u64 = 6;

/// The cell of the view under a `hover`, and whether the grass is there:
/// on the sky canvas (`sky`), the grass canvas (`grass`, its top at row
/// `top`), or a firefly of `bugs`. `None`: left, or elsewhere.
pub fn under(
    data: &Value,
    sky: Option<u64>,
    grass: Option<(u64, i64)>,
    bugs: &[(u64, (i64, i64))],
) -> Option<(i64, i64)> {
    let on = data["sprite"].as_u64()?;
    let at = (data["at"][0].as_i64()?, data["at"][1].as_i64()?);
    match grass {
        Some((id, top)) if id == on => return Some((at.0, at.1 + top)),
        _ => {}
    }
    if sky == Some(on) {
        return Some(at);
    }
    bugs.iter().find(|(id, _)| *id == on).map(|b| b.1)
}

pub struct Stars {
    night: Night,
    /// Meteors' and fireflies' own.
    fx: Twister,
    sight: Sight,
    size: (usize, usize),
    shown: Mask,
    sky: Option<Canvas>,
    grass: Option<Canvas>,
    bugs: Vec<Bug>,
    hand: Hand,
    hovers: Vec<Value>,
    /// When the next meteor goes, seconds.
    shot: f64,
    /// The last beat, seconds.
    beat: f64,
    /// The last tick.
    last: Option<u64>,
    told: bool,
}

/// `place` of a canvas at the row `y` of the view.
fn at_row(y: usize, z: i64) -> Value {
    json!({"anchor": {"at": [0, y], "space": "view"}, "z": z})
}

impl Stars {
    fn said(&mut self, what: &str, e: impl std::fmt::Display) {
        if !self.told {
            self.told = true;
            log(&format!("stars: {what}: {e}"));
        }
    }

    fn canvas(w: usize, h: usize) -> Canvas {
        match Canvas::new(w as u32, h as u32) {
            Some(c) => c,
            None => {
                log(&format!("stars: no canvas of {w}x{h}"));
                exit(1)
            }
        }
    }

    /// The hovers since the last tick, into the hand; `true`: one came.
    fn hovers(&mut self, h: usize, rows: usize, t: f64) -> bool {
        let hovers = std::mem::take(&mut self.hovers);
        let sky = self.sky.as_ref().and_then(Canvas::sprite);
        let top = self.size.1 as i64 - self.grass.as_ref().map_or(0, |g| g.height() as i64);
        let grass = self
            .grass
            .as_ref()
            .and_then(Canvas::sprite)
            .map(|id| (id, top));
        let bugs: Vec<(u64, (i64, i64))> = (self.bugs.iter())
            .filter_map(|b| Some((b.sprite?, b.cell?)))
            .collect();
        for data in &hovers {
            // The row over the grass too: the tips feel it.
            match under(data, sky, grass, &bugs) {
                Some((x, y)) if (0..=rows as i64).contains(&(h as i64 - 1 - y)) => {
                    self.hand.move_to(x, h as i64 - 1 - y, t)
                }
                _ => self.hand.leave(t),
            }
        }
        !hovers.is_empty()
    }

    fn fireflies(&mut self, dt: f64, w: usize, h: usize, rows: usize) {
        let want = (w / 14).max(2);
        while self.bugs.len() < want {
            self.bugs.push(Bug::new(&mut self.fx, w, h, rows));
        }
        for bug in self.bugs.drain(want..) {
            if let Some(id) = bug.sprite {
                let _ = remove(id);
            }
        }
        for i in 0..self.bugs.len() {
            let bug = &mut self.bugs[i];
            let cell = bug.step(&mut self.fx, dt, w, h, rows);
            let anchor = json!({"at": [cell.0, cell.1], "space": "view"});
            let done = match bug.sprite {
                None => {
                    let frames = Bug::frames(&mut self.fx);
                    put(json!({"frames": frames, "loop": true, "anchor": anchor, "z": 3}))
                        .map(|id| self.bugs[i].sprite = Some(id))
                }
                Some(id) if bug.cell != Some(cell) => {
                    call("draw.update", json!({"id": id, "anchor": anchor})).map(drop)
                }
                Some(_) => Ok(()),
            };
            self.bugs[i].cell = Some(cell);
            if let Err(e) = done {
                self.said("a firefly", e);
            }
        }
    }
}

/// The phase of the moon now, by the wall clock (`clock`; without it,
/// session time as the epoch).
fn tonight() -> f64 {
    match call("clock.now", json!({})) {
        Ok(v) => grid::phase_of(v["unix_ms"].as_f64().unwrap_or(0.0)),
        Err(e) => {
            log(&format!("stars: clock.now: {e}"));
            grid::phase_of(0.0)
        }
    }
}

impl App for Stars {
    fn init(args: Value) -> Stars {
        let o = match Options::of(&args) {
            Ok(o) => o,
            Err(e) => {
                log(&format!("stars: {e}"));
                exit(2)
            }
        };
        let phase = if o.phase.is_none() { tonight() } else { 0.0 };
        let grass = o.grass;
        let mut rng = Twister::new(u64::from(random_u32()));
        let night = Night::new(o, &mut rng, phase);
        let mut fx = Twister::new(u64::from(rng.bits(32)));
        let shot = fx.uniform(1.0, METEOR.0);
        if grass && let Err(e) = subscribe(&["hover"]) {
            log(&format!("stars: hover: {e}"));
        }
        period((1000 / TICKS) as u32);
        Stars {
            night,
            fx,
            sight: Sight::new(sight::Options::default()),
            size: (0, 0),
            shown: Vec::new(),
            sky: None,
            grass: None,
            bugs: Vec::new(),
            hand: Hand::default(),
            hovers: Vec::new(),
            shot,
            beat: 0.0,
            last: None,
            told: false,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let n = now_ms * TICKS / 1000;
        let prev = self.last;
        if prev == Some(n) {
            return;
        }
        self.last = Some(n);
        if !self.sight.look() {
            return;
        }
        let (w, h) = (self.sight.size.0 as usize, self.sight.size.1 as usize);
        if w == 0 || h == 0 {
            return;
        }
        let t = now_ms as f64 / 1000.0;
        let rows = self.night.rows(h);
        let sized = (w, h) != self.size;
        let changed = sized || self.sight.changed;
        let beat = prev.is_none_or(|p| p / BEAT_OF != n / BEAT_OF);
        let looked = prev.is_none_or(|p| p / SKY != n / SKY);
        let touched = self.hovers(h, rows, t);
        let touches = self.hand.at(t).to_vec();
        let swings = touched || touches.iter().any(|one| one.until <= t);
        if changed {
            self.size = (w, h);
            self.shown = clear(&self.sight.rows, w, h);
        }
        if changed || looked {
            let picture = self.night.above(w, h, t);
            let canvas = match &mut self.sky {
                Some(c) if !sized => c,
                _ => {
                    self.sky = None; // dropped: its sprite removed
                    self.sky.insert(Stars::canvas(w, h))
                }
            };
            picture.paint(canvas, Some(&self.shown));
            canvas.present(&at_row(0, 0));
        }
        if rows == 0 {
            self.grass = None;
        } else if changed || beat || swings {
            let low = self.night.below(w, h, t, &touches);
            let canvas = match &mut self.grass {
                Some(c) if !sized && c.height() as usize == rows => c,
                _ => {
                    self.grass = None;
                    self.grass.insert(Stars::canvas(w, rows))
                }
            };
            low.paint(canvas, Some(&self.shown[h - rows..]));
            canvas.present(&at_row(h - rows, 2));
        }
        if !beat {
            return;
        }
        let dt = (t - self.beat).min((4 * BEAT_OF) as f64 / TICKS as f64);
        self.beat = t;
        let o = &self.night.o;
        let (meteors, bugs) = (o.meteors, o.bugs);
        if meteors && t >= self.shot {
            self.shot = t + self.fx.uniform(METEOR.0, METEOR.1);
            let one = meteor(&mut self.fx, w, h);
            if let Err(e) = put(one) {
                self.said("a meteor", e);
            }
        }
        if bugs {
            self.fireflies(dt, w, h, rows);
        }
    }

    fn event(&mut self, topic: &str, data: Value) {
        if self.sight.event(topic, &data) {
            return;
        }
        if topic == "hover" {
            self.hovers.push(data);
        }
    }
}

ptyrc_app::app!(Stars);

#[cfg(test)]
mod tests {
    use super::*;

    /// `main`'s checks: the bounds the manifest says are the launcher's.
    #[test]
    fn options() {
        let o = Options::of(&json!({"all": true, "phase": 1.25})).unwrap();
        assert!(o.twinkle && o.turn && o.constellations && o.galaxy && o.meteors);
        assert!(o.bugs && o.wild && o.grass);
        assert_eq!(o.phase, Some(0.25));
        let bare = Options::of(&json!({"all": true, "grass": false})).unwrap();
        assert!(!bare.bugs && !bare.wild && bare.meteors);
        let none = Options::of(&json!({})).unwrap();
        assert_eq!(
            (none.stars, none.night, none.moon, none.wind, none.phase),
            (70.0, 300.0, 5.0, 1.0, None)
        );
        assert!(Options::of(&json!({"night": 0})).is_err());
        assert!(Options::of(&json!({"stars": -1})).is_err());
        assert!(Options::of(&json!({"moon": -1})).is_err());
    }

    #[test]
    fn the_cell_under_the_pointer() {
        let bugs = [(9, (4, 20))];
        let at = |v: Value| under(&v, Some(3), Some((5, 20)), &bugs);
        assert_eq!(at(json!({"sprite": 3, "at": [2, 1]})), Some((2, 1)));
        assert_eq!(at(json!({"sprite": 5, "at": [2, 1]})), Some((2, 21)));
        assert_eq!(at(json!({"sprite": 9, "at": [0, 0]})), Some((4, 20)));
        assert_eq!(at(json!({"sprite": 7, "at": [0, 0]})), None);
        assert_eq!(at(json!({"sprite": null, "at": null})), None);
    }
}
