// SPDX-License-Identifier: GPL-3.0-or-later
//! Cat: cats that live on the text, and sleep when nothing moves. The
//! twin: `ptyrc_apps/cat.py`.
//!
//! The looks (`look`), the places (`place`), the ball (`yarn`), the
//! cats and their home (`world`) are the twin's, pure; this module is
//! the glue to the host. State of the migration:
//! `plan/sessions/22112026-01-migration/cat-2/STATE.md`.

pub mod look;
pub mod place;
mod twister;
pub mod world;
pub mod yarn;

use std::collections::HashMap;

use look::Look;
use ptyrc_app::dice::{Dice, Host};
use ptyrc_app::sight::{self, Sight};
use ptyrc_app::{App, Value, app, call, exit, json, log, period, subscribe};
use twister::Twister;
use world::{Home, Kinds, Stage, WORDS};

/// At most, `--count`.
pub const MAX_CATS: i64 = 12;

/// The options, `-a` spread.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub count: i64,
    pub random: bool,
    pub food: bool,
    pub mice: bool,
    pub fish: bool,
    pub toys: bool,
    pub hunt: bool,
}

impl Options {
    /// `Err`: what is wrong, for the log.
    pub fn of(args: &Value) -> Result<Options, String> {
        let on = |k: &str| args[k].as_bool() == Some(true);
        let all = on("all");
        let count = args["count"].as_i64().unwrap_or(if all { 2 } else { 1 });
        if !(1..=MAX_CATS).contains(&count) {
            return Err(format!("--count must be between 1 and {MAX_CATS}"));
        }
        Ok(Options {
            count,
            random: all || on("random"),
            food: all || on("food"),
            mice: all || on("mice"),
            fish: all || on("fish"),
            toys: all || on("toys"),
            hunt: all || on("hunt"),
        })
    }
}

type Batch = (Vec<Value>, Vec<(usize, u64)>);

/// The host as a [`Stage`]: sprites by handle (a `draw.put` in a
/// batch has its id when the batch is sent), the view from a [`Sight`].
pub struct Live {
    pub sight: Sight,
    ids: HashMap<u64, u64>,
    n: u64,
    /// The calls, and the handles of the `draw.put` among them.
    batch: Option<Batch>,
    told: bool,
}

/// The members of a sprite of `look`.
fn content(look: &Look) -> Value {
    let v = look.to_value();
    match look {
        Look::Still(_) => json!({"content": (v)}),
        Look::Frames { .. } => v,
    }
}

impl Live {
    /// The frame cell of the view's top left.
    pub fn origin(&self) -> (i64, i64) {
        let o = &self.sight.info["origin"];
        (o[0].as_i64().unwrap_or(0), o[1].as_i64().unwrap_or(0))
    }

    fn said(&mut self, what: &str, e: &ptyrc_app::Error) {
        if !self.told && e.code != "no-sprite" && e.code != "gone" {
            self.told = true;
            log(&format!("cat: {what}: {e}"));
        }
    }

    /// A call now, or in the batch.
    fn send(&mut self, method: &str, params: Value) {
        if let Some((calls, _)) = &mut self.batch {
            calls.push(json!({"method": method, "params": (params)}));
        } else if let Err(e) = call(method, params) {
            self.said(method, &e);
        }
    }

    fn draw(&mut self, spec: Value) -> u64 {
        self.n += 1;
        let h = self.n;
        if let Some((calls, news)) = &mut self.batch {
            news.push((calls.len(), h));
            calls.push(json!({"method": "draw.put", "params": (spec)}));
            return h;
        }
        match call("draw.put", spec) {
            Ok(r) => {
                if let Some(id) = r["id"].as_u64() {
                    self.ids.insert(h, id);
                }
            }
            Err(e) => self.said("draw.put", &e),
        }
        h
    }

    fn update(&mut self, h: u64, mut patch: Value) {
        let Some(&id) = self.ids.get(&h) else { return };
        patch["id"] = json!(id);
        self.send("draw.update", patch);
    }
}

impl Stage for Live {
    fn put(&mut self, h: Option<u64>, x: i64, y: i64, look: &Look, z: i64) -> u64 {
        let mut spec = content(look);
        spec["anchor"] = json!({"at": [x, y], "space": "frame"});
        match h {
            None => {
                spec["z"] = json!(z);
                self.draw(spec)
            }
            Some(h) => {
                self.update(h, spec);
                h
            }
        }
    }

    fn set(&mut self, h: u64, look: &Look) {
        self.update(h, content(look));
    }

    fn remove(&mut self, h: u64) {
        if let Some(id) = self.ids.remove(&h) {
            self.send("draw.remove", json!({"id": id}));
        }
    }

    fn infos(&mut self, hs: &[u64]) -> Vec<Option<(bool, f64)>> {
        let list = match call("draw.list", Value::Null) {
            Ok(r) => r,
            Err(e) => {
                self.said("draw.list", &e);
                return hs.iter().map(|_| Some((true, 0.0))).collect();
            }
        };
        let sprites = list["sprites"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        hs.iter()
            .map(|h| {
                let id = self.ids.get(h)?;
                let s = sprites.iter().find(|s| s["id"].as_u64() == Some(*id))?;
                Some((s["visible"] == true, s["covered"].as_f64().unwrap_or(0.0)))
            })
            .collect()
    }

    fn begin(&mut self) {
        self.batch = Some((Vec::new(), Vec::new()));
    }

    fn end(&mut self) {
        let Some((calls, news)) = self.batch.take() else {
            return;
        };
        if calls.is_empty() {
            return;
        }
        match call("batch", json!({"calls": (calls)})) {
            Ok(r) => {
                for (i, h) in news {
                    if let Some(id) = r["results"][i]["result"]["id"].as_u64() {
                        self.ids.insert(h, id);
                    }
                }
                let results = r["results"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if let Some(e) = results.iter().find(|x| !x["error"].is_null()) {
                    let code = e["error"]["code"].as_str().unwrap_or_default();
                    if !self.told && code != "no-sprite" && code != "gone" {
                        self.told = true;
                        log(&format!("cat: batch: {}", e["error"]));
                    }
                }
            }
            Err(e) => self.said("batch", &e),
        }
    }

    fn mat(&mut self, w: i64, h: i64) -> u64 {
        let mut spec = content(&world::mat_look(w, h));
        spec["anchor"] = json!({"at": [0, 0], "space": "view"});
        spec["z"] = json!(-9);
        self.draw(spec)
    }

    fn view(&mut self) -> Option<(Vec<String>, world::Area)> {
        if !self.sight.usable() || self.sight.size.0 == 0 {
            return None;
        }
        let (w, h) = self.sight.size;
        let (x, y) = self.origin();
        Some((
            self.sight.rows.clone(),
            (x, y, x + i64::from(w), y + i64::from(h)),
        ))
    }
}

/// The ticks: at least this often, at most `SOON`.
pub const SOON: u32 = 33;
pub const LATE: u32 = 250;

pub struct Cat {
    pub o: Options,
    pub home: Home<Live>,
    now: f64,
    /// The pointer moves since the last tick: the last, `None` gone.
    hover: Option<Option<(i64, i64)>>,
}

/// The cell of the host pointer in the frame `frame`; `None`: out of
/// it.
fn pointed(data: &Value, frame: &Value) -> Option<(i64, i64)> {
    if data["sprite"].is_null() || (!frame.is_null() && data["frame"] != *frame) {
        return None;
    }
    Some((data["pos"][0].as_i64()?, data["pos"][1].as_i64()?))
}

impl App for Cat {
    fn init(args: Value) -> Cat {
        let o = match Options::of(&args) {
            Ok(o) => o,
            Err(e) => {
                log(&format!("cat: {e}"));
                exit(2)
            }
        };
        let sight = Sight::new(sight::Options {
            words: WORDS.iter().map(|w| w.to_string()).collect(),
            cursor: true,
            ..sight::Options::default()
        });
        let _ = call("subscribe", json!({"topics": ["idle"], "snapshot": true}));
        if o.hunt
            && let Err(e) = subscribe(&["hover"])
        {
            log(&format!("cat: hover: {e}"));
        }
        let live = Live {
            sight,
            ids: HashMap::new(),
            n: 0,
            batch: None,
            told: false,
        };
        let kinds = Kinds {
            count: o.count as usize,
            varied: o.random,
            food: o.food,
            mice: o.mice,
            fish: o.fish,
            toys: o.toys,
            hunt: o.hunt,
        };
        let rng = Twister::new(Host.next_u64());
        period(SOON);
        Cat {
            home: Home::new(live, rng, &kinds, 0.0),
            o,
            now: 0.0,
            hover: None,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let now = now_ms as f64 / 1000.0;
        self.now = now;
        let home = &mut self.home;
        let seen = home.stage.sight.look();
        if home.stage.sight.changed {
            home.stale = true;
        }
        if let Some(at) = self.hover.take() {
            home.point(at, now);
        }
        let (ox, oy) = home.stage.origin();
        let cursor = match seen {
            true => home.stage.sight.cursor.map(|(x, y)| (x + ox, y + oy)),
            false => home.cursor,
        };
        if cursor != home.cursor {
            home.set_cursor(cursor, now);
        }
        home.step(now);
        let wait = (home.due(now) - now) * 1000.0;
        period(if seen {
            wait.clamp(SOON as f64, LATE as f64) as u32
        } else {
            LATE
        });
    }

    fn event(&mut self, topic: &str, data: Value) {
        let home = &mut self.home;
        let taken = home.stage.sight.event(topic, &data);
        match topic {
            "idle" => home.set_idle(data["idle"] == true, self.now),
            "hover" if self.o.hunt => {
                self.hover = Some(pointed(&data, &home.stage.sight.info["id"]));
            }
            "frames" | "size" | "move" => home.stale = true,
            _ if !taken => return,
            _ => {}
        }
        period(SOON);
    }
}

app!(Cat);

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_app::json;

    #[test]
    fn options() {
        let o = Options::of(&json!({"all": true})).unwrap();
        assert_eq!((o.count, o.hunt, o.toys), (2, true, true));
        assert_eq!(Options::of(&json!({})).unwrap().count, 1);
        assert!(Options::of(&json!({"count": 13})).is_err());
    }
}
