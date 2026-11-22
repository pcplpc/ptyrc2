// SPDX-License-Identifier: GPL-3.0-or-later
//! Shore: a beach behind the text, seen from above; waves that run up
//! the sand and leave their lace, a tide that comes and goes.
//!
//! The sand at the top, the sea at the bottom; all of it Braille, 2 by
//! 4 dots a cell.
//!
//! - A scene that is a function of the time (`scene::Shore::picture`):
//!   no state kept but what the pointer did to the animals. A wave is a
//!   number: where its crest is, how far it ran, what it left follow
//!   from it (`scene::Wave`). One canvas the size of the view, behind
//!   the text, 8 times a second.
//! - A wave breaks at the still-water line, runs up the sand in lobes,
//!   slides back; its foam a net (`scene::lace`). It leaves a line of
//!   foam at its highest reach, wiped by the first wave to pass it, and
//!   wet sand. The tide moves the line up and down in `--tide` seconds.
//! - The text, read as a shape (`ptyrc_app::sight`): the words are
//!   rocks, froth gathers over and under them in the water. Nothing is
//!   drawn in the cell on each side of a glyph.
//! - `-b`, `-c`: sandpipers, crabs. The pointer over the view (`hover`)
//!   scares them while it moves and `scene::STILL` seconds after: the
//!   sandpipers run, and fly when it is on one; the crabs bolt.
//! - `-w`: footprints, kept until a wave that peaked after them reaches
//!   them. `-n`: day, dusk, night in `--day` seconds; the moon on the
//!   water at night.
//!
//! Every draw is Python's `random.Random` (`twister`), seeded from
//! `random` (the launcher's `--seed N`): the twin's shore for a seed.
//! Time: session time plus `--at`, ticks counted from it, 8 a second.

mod grid;
pub mod scene;
#[cfg(test)]
mod twin;
mod twister;

use grid::{Mask, clear};
use ptyrc_app::sight::{self, Sight};
use ptyrc_app::{App, Canvas, Value, exit, json, log, period, random_u32, subscribe};
use scene::{Config, Pointer, Shore, rocks_of};
use twister::Twister;

/// Ticks a second.
pub const TICKS: u64 = 8;

/// The options, as `init` is told them: the scene's, and `--at`.
pub fn options(args: &Value) -> Result<(Config, f64), String> {
    let on = |k: &str| args[k].as_bool() == Some(true) || args["all"].as_bool() == Some(true);
    let num = |k: &str, d: f64| args[k].as_f64().unwrap_or(d);
    let d = Config::default();
    let day = num("day", 240.0);
    let c = Config {
        period: num("waves", d.period),
        ebb: num("tide", d.ebb),
        grains: num("sand", d.grains),
        day: if on("night") { day } else { 0.0 },
        drift: on("drift"),
        ripples: on("ripples"),
        crabs: on("crabs"),
        birds: on("birds"),
        walker: on("walker"),
    };
    if [c.period, c.ebb, day]
        .iter()
        .any(|n| n.is_nan() || *n <= 0.0)
    {
        return Err("--waves, --tide and --day must be positive".into());
    }
    if c.grains.is_nan() || c.grains < 0.0 {
        return Err("--sand must not be negative".into());
    }
    let at = num("at", 0.0);
    if !at.is_finite() {
        return Err("--at: not a time".into());
    }
    Ok((c, at))
}

/// The cell of the view under a `hover` on the beach (`beach`, at row 0
/// of the view): `Some(Some(cell))`; `Some(None)`: off every sprite;
/// `None`: on another.
pub fn under(data: &Value, beach: Option<u64>) -> Option<Option<(i64, i64)>> {
    let Some(on) = data["sprite"].as_u64() else {
        return Some(None);
    };
    if beach != Some(on) {
        return None;
    }
    let at = (data["at"][0].as_i64()?, data["at"][1].as_i64()?);
    Some(Some(at))
}

pub struct ShoreApp {
    shore: Shore,
    at: f64,
    sight: Sight,
    size: (usize, usize),
    shown: Mask,
    rocks: Vec<(i64, i64)>,
    beach: Option<Canvas>,
    pointer: Pointer,
    hovers: Vec<Value>,
    last: Option<u64>,
}

impl App for ShoreApp {
    fn init(args: Value) -> ShoreApp {
        let (c, at) = match options(&args) {
            Ok(o) => o,
            Err(e) => {
                log(&format!("shore: {e}"));
                exit(2)
            }
        };
        let mut rng = Twister::new(u64::from(random_u32()));
        let shore = Shore::new(&mut rng, c);
        scene::lace();
        scene::grit();
        if (c.birds || c.crabs)
            && let Err(e) = subscribe(&["hover"])
        {
            log(&format!("shore: hover: {e}"));
        }
        period((1000 / TICKS) as u32);
        ShoreApp {
            shore,
            at,
            sight: Sight::new(sight::Options::default()),
            size: (0, 0),
            shown: Vec::new(),
            rocks: Vec::new(),
            beach: None,
            pointer: Pointer::default(),
            hovers: Vec::new(),
            last: None,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let n = now_ms * TICKS / 1000;
        if self.last == Some(n) {
            return;
        }
        if !self.sight.look() {
            return;
        }
        let (w, h) = (self.sight.size.0 as usize, self.sight.size.1 as usize);
        if w == 0 || h == 0 {
            return;
        }
        self.last = Some(n);
        let t = now_ms as f64 / 1000.0 + self.at;
        let beach = self.beach.as_ref().and_then(Canvas::sprite);
        for data in std::mem::take(&mut self.hovers) {
            match under(&data, beach) {
                Some(Some((x, y))) => self.pointer.move_to(x, y, t),
                Some(None) => self.pointer.leave(t),
                None => {}
            }
        }
        let sized = (w, h) != self.size;
        if sized || self.sight.changed {
            self.size = (w, h);
            self.shown = clear(&self.sight.rows, w, h);
            self.rocks = rocks_of(&self.sight.rows, w, h);
        }
        let picture = self
            .shore
            .picture(w, h, t, &self.rocks, Some(&self.pointer));
        let canvas = match &mut self.beach {
            Some(c) if !sized => c,
            _ => {
                self.beach = None; // dropped: its sprite removed
                match Canvas::new(w as u32, h as u32) {
                    Some(c) => self.beach.insert(c),
                    None => {
                        log(&format!("shore: no canvas of {w}x{h}"));
                        exit(1)
                    }
                }
            }
        };
        picture.paint(canvas, Some(&self.shown));
        canvas.present(&json!({"anchor": {"at": [0, 0], "space": "view"}, "z": 0}));
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

ptyrc_app::app!(ShoreApp);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options() {
        let (c, at) = options(&json!({"all": true, "at": 100, "waves": 2})).unwrap();
        assert!(c.drift && c.ripples && c.crabs && c.birds && c.walker);
        assert_eq!((c.day, c.period, at), (240.0, 2.0, 100.0));
        let (c, at) = options(&json!({})).unwrap();
        assert_eq!((c, at), (Config::default(), 0.0));
        for bad in [
            json!({"waves": 0}),
            json!({"tide": -1}),
            json!({"day": 0}),
            json!({"sand": -1}),
        ] {
            assert!(options(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_cell_under_the_pointer() {
        let at = |v: Value| under(&v, Some(3));
        assert_eq!(at(json!({"sprite": 3, "at": [2, 1]})), Some(Some((2, 1))));
        assert_eq!(at(json!({"sprite": 7, "at": [0, 0]})), None);
        assert_eq!(at(json!({"sprite": null, "at": null})), Some(None));
    }
}
