// SPDX-License-Identifier: GPL-3.0-or-later
//! The shape of the view of a frame, kept current by events: the twin
//! of the script module `ptyrc/sight`. Nothing is read at a period.
//!
//! [`Sight::new`] subscribes `frames`, `text`, `size`, `move` (and
//! `cursor`); the app hands each event to [`Sight::event`] first, and
//! starts a tick with [`Sight::look`]. The frame read: the app's,
//! told at `init`, unless named. An app that does not hold
//! `layout` (`manifest`) sees the raw frame alone: no `frames`, no
//! `frame.get`, each `text` its own.

use crate::{Error, Value, call, json, log};

/// What a [`Sight`] reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The frame read, an id or a name; by default the app's frame told
    /// at `init` (`Manifest::frame`), else `"raw"`.
    pub frame: Value,
    /// Words shown as they are (`screen.shape {words}`).
    pub words: Vec<String>,
    /// The text (`screen.text`), not the shape.
    pub text: bool,
    /// The topic `cursor` too.
    pub cursor: bool,
}

/// The frame of `manifest`, else `"raw"`.
fn told(manifest: Option<&crate::Manifest>) -> Value {
    match manifest.and_then(|m| m.frame) {
        Some(frame) => json!(frame),
        None => json!("raw"),
    }
}

impl Default for Options {
    fn default() -> Options {
        Options {
            frame: told(crate::manifest()),
            words: Vec::new(),
            text: false,
            cursor: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sight {
    /// The frame read (id or name); a `move` names another.
    pub frame: Value,
    pub words: Vec<String>,
    pub text: bool,
    /// The view, one string a row, padded to the width: `#` ink, a
    /// blank, a word of `words` as shown (`text`: the text).
    pub rows: Vec<String>,
    /// The view's size, `(columns, rows)`.
    pub size: (u32, u32),
    /// The frame on the screen, `[x, y, w, h]`.
    pub rect: Option<[i64; 4]>,
    /// In view cells; `None`: hidden, or `shape` not held.
    pub cursor: Option<(i64, i64)>,
    /// What the last read was cut to (`"none"`, `"shape"`, `"words"`);
    /// `None`: whole.
    pub degree: Option<String>,
    /// The last `look` read the view.
    pub changed: bool,
    /// The frame's info (`frame.get`); `Null`: none.
    pub info: Value,
    stale: bool,
    at: Value,
    layout: bool,
}

fn subscribe(topics: &[&str]) {
    if call("subscribe", json!({ "topics": topics })).is_ok() {
        return;
    }
    // One refused refuses all: each alone.
    for t in topics {
        let _ = call("subscribe", json!({ "topics": [*t] }));
    }
}

fn rect(v: &Value) -> Option<[i64; 4]> {
    let n = |i: usize| v[i].as_i64();
    Some([n(0)?, n(1)?, n(2)?, n(3)?])
}

fn said(e: &Error) {
    log(&format!("sight: {e}"));
}

impl Sight {
    pub fn new(options: Options) -> Sight {
        let layout = crate::manifest().is_none_or(|m| m.holds("layout"));
        let mut topics = vec!["frames", "text", "size", "move"];
        if !layout {
            topics.remove(0);
        }
        if options.cursor {
            topics.push("cursor");
        }
        subscribe(&topics);
        let mut sight = Sight {
            frame: options.frame,
            words: options.words,
            text: options.text,
            rows: Vec::new(),
            size: (0, 0),
            rect: None,
            cursor: None,
            degree: None,
            changed: false,
            info: Value::Null,
            stale: true,
            at: Value::Null,
            layout,
        };
        sight.read();
        if options.cursor {
            match call("screen.cursor", Value::Null) {
                Ok(at) => sight.at = at,
                Err(e) => said(&e),
            }
        }
        sight
    }

    /// The frame's info: where it is, whether it is on screen.
    fn read(&mut self) {
        if !self.layout {
            // The raw frame alone, always there.
            return self.take(json!({}));
        }
        match call("frame.get", json!({"frame": (self.frame.clone())})) {
            Ok(v) => self.take(v["frame"].clone()),
            Err(e) => {
                if e.code != "no-frame" && e.code != "not-usable" {
                    said(&e);
                }
                self.take(Value::Null)
            }
        }
    }

    fn take(&mut self, info: Value) {
        let was = std::mem::replace(&mut self.info, info);
        let info = &self.info;
        if info.is_null()
            || was.is_null()
            || (&info["rect"], &info["origin"]) != (&was["rect"], &was["origin"])
        {
            self.stale = true;
        }
        self.rect = rect(&self.info["rect"]);
    }

    /// The frame is known and on screen.
    pub fn usable(&self) -> bool {
        !self.info.is_null() && self.info["usable"] != false && self.info["state"] != "inactive"
    }

    fn mine(&self, id: &Value) -> bool {
        !id.is_null() && (*id == self.frame || (!self.info.is_null() && *id == self.info["id"]))
    }

    /// Takes `frames`, `text`, `cursor` (`true`); reads `size`, `move`
    /// (a `move` names the frame read from then on) and returns `false`
    /// for them, which are the app's too.
    pub fn event(&mut self, topic: &str, data: &Value) -> bool {
        match topic {
            "frames" => {
                let list = data["frames"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let found = list.iter().find(|f| {
                    self.mine(&f["id"]) || (!f["name"].is_null() && f["name"] == self.frame)
                });
                let info = match found {
                    Some(f) => f.clone(),
                    None if self.info.is_null() => Value::Null,
                    None => {
                        let mut gone = self.info.clone();
                        gone["usable"] = json!(false);
                        gone
                    }
                };
                self.take(info);
                true
            }
            "text" => {
                if !self.layout || self.mine(&data["frame"]) {
                    if !self.info.is_null() {
                        self.info["origin"] = data["origin"].clone();
                        self.info["rect"] = data["rect"].clone();
                    }
                    self.rect = rect(&data["rect"]).or(self.rect);
                    self.stale = true;
                }
                true
            }
            "cursor" => {
                self.at = data.clone();
                true
            }
            "size" => {
                self.stale = true;
                false
            }
            "move" => {
                if !data["frame"].is_null() {
                    self.frame = data["frame"].clone();
                }
                self.read();
                self.stale = true;
                false
            }
            _ => false,
        }
    }

    /// Read again at the next `look`.
    pub fn touch(&mut self) {
        self.stale = true;
    }

    /// At the tick: reads the view again when an event said it changed
    /// (`screen.shape {frame, words}`, or `screen.text` with `text`).
    /// `false`: the frame not usable (off screen), or the read failed
    /// (logged; asked again at the next look).
    pub fn look(&mut self) -> bool {
        self.changed = false;
        if !self.usable() {
            self.read();
            if !self.usable() {
                return false;
            }
        }
        if self.stale {
            let frame = match &self.info["id"] {
                Value::Null => self.frame.clone(),
                id => id.clone(),
            };
            let mut params = json!({ "frame": frame });
            if !self.text && !self.words.is_empty() {
                params["words"] = json!(self.words.clone());
            }
            let method = if self.text {
                "screen.text"
            } else {
                "screen.shape"
            };
            let r = match call(method, params) {
                Ok(r) => r,
                Err(e) => {
                    if e.code == "not-usable" {
                        if !self.info.is_null() {
                            self.info["usable"] = json!(false);
                        }
                    } else {
                        said(&e);
                    }
                    return false;
                }
            };
            let at = rect(&r["rect"]);
            let n = |v: &Value| v.as_u64().unwrap_or(0) as u32;
            let (w, h) = match at {
                Some(a) => (a[2].max(0) as u32, a[3].max(0) as u32),
                None => (n(&r["size"][0]), n(&r["size"][1])),
            };
            self.size = (w, h);
            let lines = r["lines"].as_array().map(Vec::as_slice).unwrap_or_default();
            self.rows = (0..h as usize)
                .map(|y| {
                    let mut row = lines
                        .get(y)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let len = row.chars().count();
                    row.extend(std::iter::repeat_n(' ', (w as usize).saturating_sub(len)));
                    row
                })
                .collect();
            self.degree = r["degree"].as_str().map(str::to_string);
            self.rect = at.or(self.rect);
            self.changed = true;
            self.stale = false;
        }
        self.cursor = self.cursor_now();
        true
    }

    fn cursor_now(&self) -> Option<(i64, i64)> {
        let at = &self.at;
        if at["visible"] != true || self.degree.as_deref() == Some("none") {
            return None;
        }
        let pos = (at["pos"][0].as_i64(), at["pos"][1].as_i64());
        let own = !self.info.is_null() && at["frame"] == self.info["id"];
        if let (true, (Some(x), Some(y))) = (own, pos) {
            return Some((x, y));
        }
        let r = self.rect?;
        Some((at["x"].as_i64()? - r[0], at["y"].as_i64()? - r[1]))
    }

    /// `rows[y][x]` is not a blank; `false` outside.
    pub fn ink(&self, x: i64, y: i64) -> bool {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        let row = self.rows.get(y);
        row.and_then(|r| r.chars().nth(x)).is_some_and(|c| c != ' ')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Asked = Rc<RefCell<Vec<(String, Value)>>>;

    /// A driver of frame 1 (`raw`) at `(0, 0)`, 6 by 2, the screen
    /// `text`: `screen.shape` of it, and the requests asked.
    fn serve(text: &'static str, layout: bool) -> Asked {
        let asked: Asked = Rc::default();
        let log = asked.clone();
        native::serve(move |method, params| {
            log.borrow_mut().push((method.to_string(), params.clone()));
            let words: Vec<String> = (params["words"].as_array().map(Vec::as_slice))
                .unwrap_or_default()
                .iter()
                .map(|w| w.as_str().unwrap_or_default().to_string())
                .collect();
            let shape = |l: &str| -> String {
                let mut out = String::new();
                for word in l.split_inclusive(|c: char| !c.is_alphanumeric()) {
                    let core = word.trim_end_matches(|c: char| !c.is_alphanumeric());
                    let keep = words.iter().any(|w| w.eq_ignore_ascii_case(core));
                    for c in word.chars() {
                        out.push(match c {
                            ' ' => ' ',
                            c if c.is_alphanumeric() && keep => c,
                            _ => '#',
                        });
                    }
                }
                out
            };
            let rect = json!([0, 0, 6, 2]);
            match method {
                "subscribe" if !layout && params["topics"].as_array().unwrap().len() > 1 => {
                    Err(Error::new("denied", "layout"))
                }
                "subscribe" if !layout && params["topics"][0] == "frames" => {
                    Err(Error::new("denied", "layout"))
                }
                "subscribe" => Ok(json!({})),
                "frame.get" => Ok(json!({"frame": {"id": 1, "name": "raw", "rect": rect,
                                       "origin": 0, "usable": true, "state": "active"}})),
                "screen.cursor" => {
                    Ok(json!({"x": 1, "y": 0, "visible": true, "frame": 1, "pos": [1, 0]}))
                }
                "screen.shape" => Ok(
                    json!({"lines": (text.lines().map(shape).collect::<Vec<_>>()), "rect": rect}),
                ),
                "screen.text" => {
                    Ok(json!({"lines": (text.lines().collect::<Vec<_>>()), "rect": rect}))
                }
                _ => Err(Error::new("unknown-method", method)),
            }
        });
        asked
    }

    /// The frame told at `init` is read from the first look.
    #[test]
    fn the_frame_told_by_default() {
        assert_eq!(told(None), json!("raw"));
        let mut m = crate::Manifest::default();
        assert_eq!(told(Some(&m)), json!("raw"));
        m.frame = Some(2);
        assert_eq!(told(Some(&m)), json!(2));
        assert_eq!(Options::default().frame, json!("raw"));
    }

    fn methods(asked: &Asked) -> Vec<String> {
        asked.borrow_mut().drain(..).map(|(m, _)| m).collect()
    }

    /// `sight.tests.js`: read when told, the cursor told, nothing asked
    /// on a quiet screen.
    #[test]
    fn read_when_told() {
        let asked = serve("one\n", true);
        let mut sight = Sight::new(Options {
            cursor: true,
            ..Options::default()
        });
        assert_eq!(methods(&asked), ["subscribe", "frame.get", "screen.cursor"]);
        assert!(sight.usable() && sight.rect == Some([0, 0, 6, 2]));
        assert!(sight.look() && sight.changed);
        assert_eq!(sight.rows, ["###   ", "      "]);
        assert_eq!((sight.size, sight.cursor), ((6, 2), Some((1, 0))));
        assert!(sight.ink(0, 0) && !sight.ink(3, 0) && !sight.ink(-1, 0) && !sight.ink(0, 9));
        assert_eq!(methods(&asked), ["screen.shape"]);
        assert_eq!(asked.borrow().len(), 0);
        // Quiet: the looks cost nothing.
        assert!(sight.look() && !sight.changed && asked.borrow().is_empty());
        assert!(sight.event("cursor", &json!({"x": 0, "y": 0, "visible": false})));
        assert!(sight.look() && sight.cursor.is_none());
        // Text of another frame: nothing; of its own: read once.
        assert!(sight.event("text", &json!({"frame": 2, "rect": [0, 0, 6, 2]})));
        assert!(sight.look() && !sight.changed);
        assert!(sight.event(
            "text",
            &json!({"frame": 1, "origin": 0, "rect": [0, 0, 6, 2]})
        ));
        assert!(sight.look() && sight.changed);
        assert!(sight.look() && !sight.changed);
        // `size`, `move`: the app's too; read again.
        assert!(!sight.event("size", &json!({"size": [60, 20]})));
        assert!(sight.look() && sight.changed);
        assert!(!sight.event("move", &json!({"frame": 1})));
        assert!(sight.look() && sight.changed);
        // Its frame gone from the list: not usable.
        assert!(sight.event("frames", &json!({"frames": [{"id": 2}]})));
        assert!(!sight.usable());
        assert!(!sight.event("hover", &json!({})));
        let asked = methods(&asked);
        assert!(asked.iter().all(|m| m != "subscribe"), "{asked:?}");
    }

    #[test]
    fn words_and_text() {
        serve("one fish, a FISH_tank\n", true);
        let mut sight = Sight::new(Options {
            words: vec!["fish".into()],
            ..Options::default()
        });
        assert!(sight.look());
        assert_eq!(sight.rows[0], "### fish# # FISH#####");
        let mut sight = Sight::new(Options {
            text: true,
            ..Options::default()
        });
        assert!(sight.look() && sight.rows[0] == "one fish, a FISH_tank");
    }

    /// A `move`: the frame it names read from then on; below `layout`,
    /// told 0, the raw frame read again.
    #[test]
    fn follows_a_move() {
        let asked = serve("x\n", true);
        let mut sight = Sight::new(Options::default());
        assert!(sight.look());
        asked.borrow_mut().clear();
        assert!(!sight.event("move", &json!({"frame": 4})));
        assert_eq!(sight.frame, json!(4));
        assert!(sight.look() && sight.changed);
        let asked: Vec<(String, Value)> = asked.borrow_mut().drain(..).collect();
        assert_eq!(asked[0], ("frame.get".into(), json!({"frame": 4})));
        assert_eq!(asked[1].0, "screen.shape");

        let asked = serve("x\n", true);
        let mut sight = Sight::new(Options {
            frame: json!(0),
            ..Options::default()
        });
        sight.layout = false;
        assert!(sight.look());
        asked.borrow_mut().clear();
        assert!(!sight.event("move", &json!({"frame": 0})));
        assert_eq!(sight.frame, json!(0));
        assert!(sight.look() && sight.usable());
        let asked: Vec<(String, Value)> = asked.borrow_mut().drain(..).collect();
        assert_eq!(asked, [("screen.shape".into(), json!({"frame": 0}))]);
    }

    /// `layout` refused: `frames` refused alone, the others taken.
    #[test]
    fn subscribed_one_by_one_when_refused() {
        let asked = serve("x\n", false);
        let _sight = Sight::new(Options::default());
        let topics: Vec<Value> = (asked.borrow().iter())
            .filter(|(m, _)| m == "subscribe")
            .map(|(_, p)| p["topics"].clone())
            .collect();
        assert_eq!(
            topics,
            [
                json!(["frames", "text", "size", "move"]),
                json!(["frames"]),
                json!(["text"]),
                json!(["size"]),
                json!(["move"]),
            ]
        );
    }

    #[test]
    fn a_failed_read_is_asked_again() {
        native::serve(|method, _| match method {
            "frame.get" => Ok(json!({"frame": {"id": 1, "rect": [0, 0, 1, 1]}})),
            "screen.shape" => Err(Error::new("gone", "")),
            _ => Ok(json!({})),
        });
        let mut sight = Sight::new(Options::default());
        assert!(!sight.look() && !sight.look());
        native::serve(|method, _| match method {
            "frame.get" => Ok(json!({"frame": {"id": 1}})),
            _ => Ok(json!({"lines": ["#"], "size": [2, 1], "degree": "none"})),
        });
        assert!(sight.look() && sight.changed && sight.degree.as_deref() == Some("none"));
        assert_eq!(sight.rows, ["# "]);
        native::serve(|method, _| match method {
            "frame.get" => Err(Error::new("no-frame", "")),
            _ => Err(Error::new("not-usable", "")),
        });
        sight.touch();
        assert!(!sight.look() && !sight.usable());
    }
}
