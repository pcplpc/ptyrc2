// SPDX-License-Identifier: GPL-3.0-or-later
//! Flock: starlings dance before a sunset, behind the text.
//!
//! - A bird is a Braille dot on its own wings (`scene::Flock`): it flies
//!   with its neighbours, after a lead that wanders; a few stray, now
//!   and then a part of the flock goes its own way. A cell is the
//!   brighter the more birds it holds.
//! - A sky and a land that are functions of the time (`scene::Dusk`,
//!   `scene::Land`); the birds a state, stepped to the time asked.
//! - A dusk of `--dusk` seconds: the sun comes down, sets at
//!   `scene::SET` of it; the birds glide down to roosts along the land;
//!   a night of `scene::REST`, and the dusk again.
//! - The pointer over the view (`hover`): the birds flee it, less once
//!   it rests (`scene::STILL` after `scene::CALM` seconds).
//! - Three canvases in view cells, behind the text: the sky, redrawn
//!   once a second; the sun and the birds, 30 times; the land over
//!   them, 5 times. Flown 10 steps a second, drawn on a curve between.
//! - The text, read as a shape (`ptyrc_app::sight`): nothing drawn in
//!   the cell on each side of a glyph; with `-o` the tops of the lines
//!   are perches.
//!
//! Every draw is Python's `random.Random` (`twister`), seeded from
//! `random` (the launcher's `--seed N`): the twin's flock for a seed.
//! Time: session time from the first tick.

mod grid;
pub mod scene;
mod sky;
#[cfg(test)]
mod twin;
mod twister;

use grid::{Grid, Mask, clear};
use ptyrc_app::sight::{self, Sight};
use ptyrc_app::{App, Canvas, Value, exit, json, log, period, random_u32, subscribe};
use scene::{CALM, Config, LAND, REST, SKY, STILL, Scene, perches, shuffle};
use twister::Twister;

/// Ticks a second.
pub const TICKS: u64 = 30;

/// The options, as `init` is told them.
pub fn options(args: &Value) -> Result<Config, String> {
    let on = |k: &str| args[k].as_bool() == Some(true) || args["all"].as_bool() == Some(true);
    let d = Config::default();
    let num = |k: &str, d: f64| args[k].as_f64().unwrap_or(d);
    let c = Config {
        birds: num("birds", d.birds),
        dusk: num("dusk", d.dusk),
        at: num("at", d.at),
        sun: num("sun", d.sun),
        wind: num("wind", d.wind),
        clouds: on("clouds"),
        water: on("water"),
        reeds: on("reeds"),
        join: on("join"),
        double: on("double"),
        hawk: on("hawk"),
        night: on("night"),
        roost: on("roost"),
    };
    if c.dusk.is_nan() || c.dusk <= 0.0 {
        return Err("--dusk must be positive".into());
    }
    if c.birds.is_nan() || c.birds < 0.0 || c.sun.is_nan() || c.sun < 0.0 {
        return Err("--birds and --sun must not be negative".into());
    }
    if ![c.at, c.wind, c.birds, c.sun, c.dusk]
        .iter()
        .all(|n| n.is_finite())
    {
        return Err("--birds, --dusk, --at, --sun, --wind: not a number".into());
    }
    Ok(Config {
        at: c.at.rem_euclid(1.0 + REST),
        ..c
    })
}

/// The cell of the view under a `hover`, of the canvases `ids` (sky,
/// birds, land; the land's top at row `land` of the view): `None` off
/// them.
pub fn under(data: &Value, ids: [Option<u64>; 3], land: i64) -> Option<(i64, i64)> {
    let on = data["sprite"].as_u64()?;
    let at = (data["at"][0].as_i64()?, data["at"][1].as_i64()?);
    match ids.iter().position(|&id| id == Some(on))? {
        2 => Some((at.0, at.1 + land)),
        _ => Some(at),
    }
}

pub struct FlockApp {
    scene: Scene,
    seed: u32,
    sight: Sight,
    size: (usize, usize),
    shown: Mask,
    roosts: Vec<(i64, i64)>,
    /// Sky, birds, land.
    canvases: [Option<Canvas>; 3],
    /// The host pointer: a dot, when it moved.
    pointer: Option<(f64, f64, f64)>,
    hovers: Vec<Value>,
    first: Option<u64>,
    last: Option<u64>,
}

impl FlockApp {
    fn put(&mut self, i: usize, grid: &Grid, y: usize, sized: bool) {
        let (w, h) = (grid.width(), grid.height());
        if sized || self.canvases[i].is_none() {
            self.canvases[i] = None; // dropped: its sprite removed
            match Canvas::new(w as u32, h as u32) {
                Some(c) => self.canvases[i] = Some(c),
                None => {
                    log(&format!("flock: no canvas of {w}x{h}"));
                    exit(1)
                }
            }
        }
        let canvas = self.canvases[i].as_mut().expect("a canvas");
        grid.paint(canvas, Some(&self.shown[y..]));
        let place = json!({"anchor": {"at": [0, y], "space": "view"}, "z": i});
        canvas.present(&place);
    }
}

impl App for FlockApp {
    fn init(args: Value) -> FlockApp {
        let c = match options(&args) {
            Ok(c) => c,
            Err(e) => {
                log(&format!("flock: {e}"));
                exit(2)
            }
        };
        let seed = random_u32();
        let scene = Scene::new(&mut Twister::new(u64::from(seed)), c);
        if let Err(e) = subscribe(&["hover"]) {
            log(&format!("flock: hover: {e}"));
        }
        period((1000 / TICKS) as u32);
        FlockApp {
            scene,
            seed,
            sight: Sight::new(sight::Options::default()),
            size: (0, 0),
            shown: Vec::new(),
            roosts: Vec::new(),
            canvases: [None, None, None],
            pointer: None,
            hovers: Vec::new(),
            first: None,
            last: None,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let first = *self.first.get_or_insert(now_ms);
        let ms = now_ms - first;
        let n = ms * TICKS / 1000;
        if self.last == Some(n) || !self.sight.look() {
            return;
        }
        let (w, h) = (self.sight.size.0 as usize, self.sight.size.1 as usize);
        if w == 0 || h == 0 {
            return;
        }
        let before = self.last.replace(n);
        let t = ms as f64 / 1000.0;
        let rows = self.scene.rows(h);
        let ids = self
            .canvases
            .each_ref()
            .map(|c| c.as_ref().and_then(Canvas::sprite));
        for data in std::mem::take(&mut self.hovers) {
            self.pointer = under(&data, ids, (h - rows) as i64)
                .map(|(x, y)| (2.0 * x as f64 + 1.0, 4.0 * y as f64 + 2.0, t));
        }
        let scare = self
            .pointer
            .map(|(x, y, at)| (x, y, STILL.max(1.0 - (t - at) / CALM)));
        let sized = (w, h) != self.size;
        let changed = sized || self.sight.changed;
        if changed {
            self.size = (w, h);
            self.shown = clear(&self.sight.rows, w, h);
            if self.scene.c.roost {
                self.roosts = perches(&self.sight.rows, w, h);
                shuffle(&mut Twister::new(u64::from(self.seed)), &mut self.roosts);
            }
        }
        // The sky on the second, the land between: no long tick.
        let due =
            |every: u64, off: u64| before.is_none_or(|b| (b + off) / every != (n + off) / every);
        if changed || due(SKY, 0) {
            let above = self.scene.above(w, h, t);
            self.put(0, &above, 0, sized);
        }
        let fore = self.scene.fore(w, h, t, &self.roosts, scare);
        self.put(1, &fore, 0, sized);
        if changed || due(LAND, LAND / 2) {
            let mut low = self.scene.below(w, h, t);
            low.keep_last(rows);
            self.put(2, &low, h - rows, sized);
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

ptyrc_app::app!(FlockApp);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options() {
        let c = options(&json!({"all": true, "at": 1.5, "dusk": 40})).unwrap();
        assert!(c.clouds && c.water && c.reeds && c.join && c.double);
        assert!(c.hawk && c.night && c.roost);
        assert_eq!(c.dusk, 40.0);
        assert!((c.at - 0.4).abs() < 1e-12);
        assert_eq!(options(&json!({})).unwrap(), Config::default());
        for bad in [
            json!({"dusk": 0}),
            json!({"dusk": -1}),
            json!({"birds": -1}),
            json!({"sun": -1}),
        ] {
            assert!(options(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_cell_under_the_pointer() {
        let ids = [Some(1), Some(2), Some(3)];
        let at = |v: Value| under(&v, ids, 23);
        assert_eq!(at(json!({"sprite": 2, "at": [5, 4]})), Some((5, 4)));
        assert_eq!(at(json!({"sprite": 3, "at": [5, 0]})), Some((5, 23)));
        assert_eq!(at(json!({"sprite": 7, "at": [0, 0]})), None);
        assert_eq!(at(json!({"sprite": null, "at": null})), None);
    }
}
