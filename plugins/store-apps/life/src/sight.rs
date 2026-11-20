// SPDX-License-Identifier: GPL-3.0-or-later
//! What an app that plays on its frame knows of the text there, told by
//! the driver: nothing is asked on a period.
//!
//! [`Sight::new`] subscribes `size`, `move`, `text` (and `cursor`); the
//! app hands each event to [`Sight::event`] first. A tick starts with
//! [`Sight::look`]: the view is read again only when an event said it
//! changed, or the last read failed; `changed` says it was.
//!
//! `rows`: the shape of the view (`screen.shape`): a character a cell,
//! `#` inked, a space blank, the whole-word matches of `words` as shown;
//! [`Look::Text`]: its text (`screen.text`), a character a glyph. Below
//! `shape` no `text` event comes: the view is read once, blank, and on
//! each `size` and `move`; `degree` says so.

use ptyrc_app::{Value, call, json, log, subscribe};

/// What a look reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Look {
    /// `screen.shape`.
    Shape,
    /// `screen.shape {words}`: each a word of the manifest's `words`.
    Words(Vec<String>),
    /// `screen.text`.
    Text,
}

#[derive(Clone, Debug)]
pub struct Sight {
    look: Look,
    /// The view, a string a row.
    pub rows: Vec<String>,
    /// `rows` was read by the last `look`.
    pub changed: bool,
    /// The view, `(columns, rows)`.
    pub size: (u32, u32),
    /// The cursor in the view; `None`: hidden, or not asked.
    pub cursor: Option<(u32, u32)>,
    /// The degree of the last read when it was cut (`"none"`, ...).
    pub degree: Option<String>,
    stale: bool,
}

impl Sight {
    /// `cursor`: the topic `cursor` too.
    pub fn new(look: Look, cursor: bool) -> Sight {
        let mut topics = vec!["size", "move", "text"];
        if cursor {
            topics.push("cursor");
        }
        if let Err(e) = subscribe(&topics) {
            log(&format!("sight: subscribe: {e}"));
        }
        let size = ptyrc_app::size().unwrap_or((0, 0));
        Sight {
            look,
            rows: Vec::new(),
            changed: false,
            size,
            cursor: None,
            degree: None,
            stale: true,
        }
    }

    /// Takes an event of its topics; `false`: not one of them. A `size`
    /// is the app's too: `size` holds it.
    pub fn event(&mut self, topic: &str, data: &Value) -> bool {
        match topic {
            "size" => {
                let n = |i: usize| data["size"][i].as_u64().map(|n| n as u32);
                if let (Some(c), Some(r)) = (n(0), n(1)) {
                    self.size = (c, r);
                }
                self.stale = true;
            }
            "move" | "text" => self.stale = true,
            "cursor" => {
                let at = |v: &Value| Some((v[0].as_u64()? as u32, v[1].as_u64()? as u32));
                self.cursor = match data["visible"].as_bool() {
                    Some(true) => at(&data["pos"])
                        .or_else(|| at(&json!([data["x"].clone(), data["y"].clone()]))),
                    _ => None,
                };
            }
            _ => return false,
        }
        true
    }

    /// Reads the view if it changed. `false`: the read failed (the
    /// error logged); asked again at the next look.
    pub fn look(&mut self) -> bool {
        self.changed = false;
        if !self.stale {
            return true;
        }
        let asked = match &self.look {
            Look::Shape => call("screen.shape", Value::Null),
            Look::Words(words) => call("screen.shape", json!({"words": (words.clone())})),
            Look::Text => call("screen.text", Value::Null),
        };
        let answer = match asked {
            Ok(a) => a,
            Err(e) => {
                log(&format!("sight: {e}"));
                return false;
            }
        };
        let lines = answer["lines"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.rows = lines
            .iter()
            .map(|l| l.as_str().unwrap_or_default().to_string())
            .collect();
        self.degree = answer["degree"].as_str().map(str::to_string);
        (self.changed, self.stale) = (true, false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_app::Error;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A driver of one screen `text`: `screen.shape` of it, and the
    /// requests asked.
    fn serve(text: &'static str) -> Rc<RefCell<Vec<String>>> {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = asked.clone();
        ptyrc_app::native::serve(move |method, params| {
            log.borrow_mut().push(method.to_string());
            let shape = |words: &[String]| -> Vec<String> {
                let line = |l: &str| -> String {
                    let mut out = String::new();
                    for word in l.split_inclusive(|c: char| !c.is_alphanumeric()) {
                        let core = word.trim_end_matches(|c: char| !c.is_alphanumeric());
                        let keep = words.iter().any(|w| w.eq_ignore_ascii_case(core));
                        for c in word.chars() {
                            let letter = c.is_alphanumeric() && keep;
                            out.push(if letter {
                                c
                            } else if c == ' ' {
                                ' '
                            } else {
                                '#'
                            });
                        }
                    }
                    out
                };
                text.lines().map(line).collect()
            };
            match method {
                "subscribe" => Ok(json!({})),
                "session.info" => Ok(json!({"size": [40, 10]})),
                "screen.shape" => {
                    let words: Vec<String> = (params["words"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default())
                    .iter()
                    .map(|w| w.as_str().unwrap_or_default().to_string())
                    .collect();
                    Ok(json!({"lines": (shape(&words))}))
                }
                "screen.text" => Ok(json!({"lines": (text.lines().collect::<Vec<_>>())})),
                _ => Err(Error {
                    code: "unknown-method".into(),
                    message: method.into(),
                    hint: None,
                }),
            }
        });
        asked
    }

    /// `test_apps.py::test_sight`: the shape read when the driver tells,
    /// the cursor told, nothing asked on a quiet screen.
    #[test]
    fn read_when_told() {
        let asked = serve("one\n");
        let mut sight = Sight::new(Look::Shape, true);
        assert_eq!(asked.borrow().as_slice(), ["subscribe", "session.info"]);
        assert_eq!(sight.size, (40, 10));
        assert!(sight.look() && sight.changed && sight.rows[0] == "###");
        assert_eq!(asked.borrow().last().unwrap(), "screen.shape");
        asked.borrow_mut().clear();
        // Quiet: the looks cost nothing.
        assert!(sight.look() && !sight.changed && asked.borrow().is_empty());
        assert!(sight.event(
            "cursor",
            &json!({"x": 2, "y": 1, "visible": true, "frame": 1, "pos": [2, 1]})
        ));
        assert_eq!(sight.cursor, Some((2, 1)));
        assert!(sight.event(
            "cursor",
            &json!({"x": 0, "y": 0, "visible": false, "frame": null, "pos": null})
        ));
        assert_eq!(sight.cursor, None);
        // Text written: read again, once.
        assert!(sight.event(
            "text",
            &json!({"frame": 1, "rows": [2], "origin": 0, "rect": [0, 0, 40, 10]})
        ));
        assert!(sight.look() && sight.changed);
        assert!(sight.look() && !sight.changed);
        assert_eq!(asked.borrow().as_slice(), ["screen.shape"]);
        // A resize, a move: read again.
        assert!(sight.event("size", &json!({"size": [60, 20]})));
        assert!(sight.look() && sight.changed && sight.size == (60, 20));
        assert!(sight.event("move", &json!({"frame": 2})));
        assert!(sight.look() && sight.changed);
        assert!(!sight.event("hover", &json!({})));
    }

    /// `test_apps.py::test_sight_words_and_text`.
    #[test]
    fn words_and_text() {
        serve("one fish, a FISH_tank\n");
        let mut sight = Sight::new(Look::Words(vec!["fish".into()]), false);
        assert!(sight.look());
        assert_eq!(sight.rows[0], "### fish# # FISH#####");
        let mut sight = Sight::new(Look::Text, false);
        assert!(sight.look() && sight.rows[0] == "one fish, a FISH_tank");
    }

    #[test]
    fn a_failed_read_is_asked_again() {
        ptyrc_app::native::serve(|method, _| match method {
            "screen.shape" => Err(Error {
                code: "gone".into(),
                message: String::new(),
                hint: None,
            }),
            _ => Ok(json!({"size": [4, 2]})),
        });
        let mut sight = Sight::new(Look::Shape, false);
        assert!(!sight.look() && !sight.look());
        ptyrc_app::native::serve(|_, _| Ok(json!({"lines": ["#"], "degree": "none"})));
        assert!(sight.look() && sight.changed && sight.degree.as_deref() == Some("none"));
    }
}
