// SPDX-License-Identifier: GPL-3.0-or-later
//! Pond: still water behind the text, seen from above; rings that widen
//! and break against the words, lily pads.
//!
//! The pond is a map: a pad has a place `(u, v)` in [0, 1), laid over
//! the view whatever its size. All of it is Braille, 2 by 4 dots a
//! cell; a dot is square, a ring is round.
//!
//! - A scene that is a function of the time (`scene::Pond::picture`):
//!   a drop is drawn from the second it falls in (`scene::drops`); no
//!   state kept but the koi and the pointer. One canvas the size of the
//!   view, behind the text, 20 times a second.
//! - A slow current (`--current`): the light on the swell, what floats
//!   and the striders at rest go with it.
//! - The pointer over the view (`hover`): feared while it moves and
//!   `scene::CALM` seconds after. The koi turn their tail to it and
//!   dart, a ring where they took fright; the striders slide off; a
//!   dragonfly leaves for another place.
//! - The text, read as a shape (`ptyrc_app::sight`): the words are
//!   stones. A drop on one makes no ring; a ring stops against them,
//!   and closes again behind a small one (`scene::shadow`). Nothing is
//!   drawn in the cell on each side of a glyph; the koi steer clear.
//! - `-m`: the moon in the water, tonight's phase (`grid::phase_of`),
//!   its rows shaken by the swell; it crosses in `--night` seconds.
//! - `-p`: the season of today (`scene::season_of`): petals in spring,
//!   leaves in autumn, adrift on the current.
//! - `-i`: water striders, a ring at each push; dragonflies, sprites
//!   the driver plays, moved when they change cell.
//!
//! Every draw is Python's `random.Random` (`twister`), seeded from
//! `random` (the launcher's `--seed N`): the twin's pond for a seed.
//! Ticks are counted from session time, 20 a second.

mod grid;
pub mod scene;
#[cfg(test)]
mod twin;
mod twister;

use grid::{Mask, clear};
use ptyrc_app::sight::{self, Sight};
use ptyrc_app::{
    App, Canvas, Value, call, exit, json, log, period, put, random_u32, remove, subscribe,
};
use scene::{Config, Fly, Pond, SEASONS, Walls, index};
use twister::Twister;

/// Ticks a second.
pub const TICKS: u64 = 20;

/// The options, as `init` is told them; `None`: today's.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub c: Config,
    pub phase: Option<f64>,
    pub season: Option<usize>,
}

impl Options {
    /// `main`'s checks: those the manifest cannot say.
    pub fn of(args: &Value) -> Result<Options, String> {
        let on = |k: &str| args[k].as_bool() == Some(true) || args["all"].as_bool() == Some(true);
        let num = |k: &str, d: f64| args[k].as_f64().unwrap_or(d);
        let d = Config::default();
        let season = match args["season"].as_str() {
            None => None,
            Some(s) => Some(
                (SEASONS.iter().position(|n| *n == s))
                    .ok_or(format!("--season: not one of {SEASONS:?}"))?,
            ),
        };
        let c = Config {
            pads: num("pads", d.pads),
            drops: num("drops", d.drops),
            koi: on("koi"),
            lotus: on("lotus"),
            rain: on("rain"),
            moon: on("moon"),
            adrift: on("petals"),
            striders: on("insects"),
            phase: d.phase,
            night: num("night", d.night),
            season: d.season,
            current: num("current", d.current),
        };
        if c.night.is_nan() || c.night <= 0.0 {
            return Err("--night must be positive".into());
        }
        if [c.pads, c.drops, c.current]
            .iter()
            .any(|n| n.is_nan() || *n < 0.0)
        {
            return Err("--pads, --drops and --current must not be negative".into());
        }
        let phase = args["phase"].as_f64().map(|p| p.rem_euclid(1.0));
        Ok(Options { c, phase, season })
    }
}

/// The cell of the view under a `hover`: on the water (`water`, at row
/// 0 of the view) or on a dragonfly of `flies`. `None`: left, or
/// elsewhere.
pub fn under(data: &Value, water: Option<u64>, flies: &[(u64, (i64, i64))]) -> Option<(i64, i64)> {
    let on = data["sprite"].as_u64()?;
    let at = (data["at"][0].as_i64()?, data["at"][1].as_i64()?);
    if water == Some(on) {
        return Some(at);
    }
    (flies.iter().find(|(id, _)| *id == on)).map(|f| (f.1.0 + at.0, f.1.1 + at.1))
}

/// Today by the wall clock (`clock`; without it, session time as the
/// epoch): the moon's phase, the day of the year.
fn today() -> (f64, i64) {
    match call("clock.now", json!({})) {
        Ok(v) => {
            let ms = v["unix_ms"].as_f64().unwrap_or(0.0);
            let off = v["offset_s"].as_f64().unwrap_or(0.0);
            (grid::phase_of(ms), scene::day_of_year(ms, off))
        }
        Err(e) => {
            log(&format!("pond: clock.now: {e}"));
            (grid::phase_of(0.0), 1)
        }
    }
}

pub struct PondApp {
    pond: Pond,
    /// The dragonflies' own.
    fx: Twister,
    sight: Sight,
    size: (usize, usize),
    shown: Mask,
    walls: Walls,
    water: Option<Canvas>,
    flies: Vec<Fly>,
    hovers: Vec<Value>,
    /// The last tick, and its time in seconds.
    last: Option<(u64, f64)>,
    told: bool,
}

impl PondApp {
    fn said(&mut self, what: &str, e: impl std::fmt::Display) {
        if !self.told {
            self.told = true;
            log(&format!("pond: {what}: {e}"));
        }
    }

    fn dragonflies(&mut self, dt: f64, w: usize, h: usize, t: f64) {
        let want = (w * h / 2400).clamp(1, 4);
        while self.flies.len() < want {
            self.flies.push(Fly::new(&mut self.fx, w, h));
        }
        for fly in self.flies.drain(want..) {
            if let Some(id) = fly.sprite {
                let _ = remove(id);
            }
        }
        // The pointer, in cells.
        let scare = (self.pond.fear(t)).map(|(x, y, f)| ((x - 1.0) / 2.0, (y - 2.0) / 4.0, f));
        for i in 0..self.flies.len() {
            let fly = &mut self.flies[i];
            let cell = fly.step(&mut self.fx, dt, w, h, scare);
            let anchor = json!({"at": [cell.0, cell.1], "space": "view"});
            let done = match fly.sprite {
                None => {
                    put(json!({"frames": fly.frames(), "loop": true, "anchor": anchor, "z": 3}))
                        .map(|id| self.flies[i].sprite = Some(id))
                }
                Some(id) if fly.cell != Some(cell) => {
                    call("draw.update", json!({"id": id, "anchor": anchor})).map(drop)
                }
                Some(_) => Ok(()),
            };
            self.flies[i].cell = Some(cell);
            if let Err(e) = done {
                self.said("a dragonfly", e);
            }
        }
    }
}

impl App for PondApp {
    fn init(args: Value) -> PondApp {
        let mut o = match Options::of(&args) {
            Ok(o) => o,
            Err(e) => {
                log(&format!("pond: {e}"));
                exit(2)
            }
        };
        if o.phase.is_none() || o.season.is_none() {
            let (phase, day) = today();
            o.phase.get_or_insert(phase);
            o.season.get_or_insert(scene::season_of(day));
        }
        o.c.phase = o.phase.unwrap_or(0.5);
        o.c.season = o.season.unwrap_or(1);
        let mut rng = Twister::new(u64::from(random_u32()));
        let pond = Pond::new(&mut rng, o.c);
        let fx = Twister::new(u64::from(rng.bits(32)));
        if let Err(e) = subscribe(&["hover"]) {
            log(&format!("pond: hover: {e}"));
        }
        period((1000 / TICKS) as u32);
        PondApp {
            pond,
            fx,
            sight: Sight::new(sight::Options::default()),
            size: (0, 0),
            shown: Vec::new(),
            walls: Walls::default(),
            water: None,
            flies: Vec::new(),
            hovers: Vec::new(),
            last: None,
            told: false,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let n = now_ms * TICKS / 1000;
        if self.last.is_some_and(|(p, _)| p == n) {
            return;
        }
        if !self.sight.look() {
            return;
        }
        let (w, h) = (self.sight.size.0 as usize, self.sight.size.1 as usize);
        if w == 0 || h == 0 {
            return;
        }
        let t = now_ms as f64 / 1000.0;
        let dt = self.last.map_or(1.0 / TICKS as f64, |(_, t0)| {
            (t - t0).min(4.0 / TICKS as f64)
        });
        self.last = Some((n, t));
        let water = self.water.as_ref().and_then(Canvas::sprite);
        let flies: Vec<(u64, (i64, i64))> = (self.flies.iter())
            .filter_map(|f| Some((f.sprite?, f.cell?)))
            .collect();
        for data in std::mem::take(&mut self.hovers) {
            if let Some((x, y)) = under(&data, water, &flies) {
                self.pond.touch((2 * x + 1) as f64, (4 * y + 2) as f64, t);
            }
        }
        let sized = (w, h) != self.size;
        if sized || self.sight.changed {
            self.size = (w, h);
            self.shown = clear(&self.sight.rows, w, h);
            self.walls = index(&self.sight.rows, w, h);
        }
        self.pond.step(dt, w, h, t, Some(&self.walls));
        let picture = self.pond.picture(w, h, t, Some(&self.walls));
        let canvas = match &mut self.water {
            Some(c) if !sized => c,
            _ => {
                self.water = None; // dropped: its sprite removed
                match Canvas::new(w as u32, h as u32) {
                    Some(c) => self.water.insert(c),
                    None => {
                        log(&format!("pond: no canvas of {w}x{h}"));
                        exit(1)
                    }
                }
            }
        };
        picture.paint(canvas, Some(&self.shown));
        canvas.present(&json!({"anchor": {"at": [0, 0], "space": "view"}, "z": 0}));
        if self.pond.c.striders {
            self.dragonflies(dt, w, h, t);
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

ptyrc_app::app!(PondApp);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options() {
        let o = Options::of(&json!({"all": true, "phase": 1.25, "season": "autumn"})).unwrap();
        let c = &o.c;
        assert!(c.koi && c.lotus && c.rain && c.moon && c.adrift && c.striders);
        assert_eq!((o.phase, o.season), (Some(0.25), Some(2)));
        let none = Options::of(&json!({})).unwrap();
        assert_eq!(
            (none.c, none.phase, none.season),
            (Config::default(), None, None)
        );
        for bad in [
            json!({"night": 0}),
            json!({"pads": -1}),
            json!({"drops": -1}),
            json!({"current": -1}),
            json!({"season": "monsoon"}),
        ] {
            assert!(Options::of(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_cell_under_the_pointer() {
        let flies = [(9, (4, 20))];
        let at = |v: Value| under(&v, Some(3), &flies);
        assert_eq!(at(json!({"sprite": 3, "at": [2, 1]})), Some((2, 1)));
        assert_eq!(at(json!({"sprite": 9, "at": [0, 0]})), Some((4, 20)));
        assert_eq!(at(json!({"sprite": 7, "at": [0, 0]})), None);
        assert_eq!(at(json!({"sprite": null, "at": null})), None);
    }
}
