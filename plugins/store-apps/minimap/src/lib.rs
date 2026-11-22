// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimap: the text of a frame, small, the view marked.
//!
//! - The remembered buffer of the frame (`frame.text`) and its view
//!   (`screen.text`), read again after a burst of `text`, `frames` or
//!   `move` events: never on a period. What was never shown is blank.
//!   The cursor kept by `cursor`, and read with them (`screen.cursor`).
//! - Ink, not letters (`map`): with `shape` alone, the map is whole;
//!   `text` gives the comments (`-n`) and the line under the pointer.
//! - The view a bright band; `-c` the line of the cursor; `-H` the lines
//!   edited and visited, fading over `--fade` seconds; `--mark RE` the
//!   lines found by the driver (`screen.find`, `known`), a colour each.
//! - A widget a column `--width` wide, of the heights the text needs:
//!   declared again as it grows. The pointer on it: the line there told
//!   beside it, in front.

pub mod map;

use std::collections::{BTreeMap, HashMap};

use map::{Doc, GROUND, Lines, Look, face_of, heights, line_at, title};
use ptyrc_app::deck::{Content, Deck, Spec, Widget};
use ptyrc_app::{App, Error, Run, Value, call, exit, json, log, manifest, period, put, remove};

/// ms of silence that end a burst of changes (a tick).
pub const SETTLE: u64 = 33;
/// ms a burst may last before a look.
pub const BURST: u64 = 200;
/// Cells of a tip, at most.
pub const TIP: usize = 48;
const TAG: &str = "minimap";

/// The options of `init`, `--all` spread.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub look: Look,
    pub marks: Vec<String>,
    pub width: u32,
}

impl Options {
    /// `Err`: what is wrong, for the log.
    pub fn of(args: &Value) -> Result<Options, String> {
        let on = |k: &str| args[k] == true || args["all"] == true;
        let fade = args["fade"].as_f64().unwrap_or(60.0);
        if !(fade > 0.0 && fade.is_finite()) {
            return Err(format!("--fade {fade}: not a positive number"));
        }
        let marks = (args["mark"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Ok(Options {
            look: Look {
                cursor: on("cursor"),
                heat: on("heat"),
                notes: on("notes"),
                fade,
            },
            marks,
            width: args["width"].as_u64().unwrap_or(12) as u32,
        })
    }
}

/// The view of a frame: its `origin`, `rect` (`frame.get`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    pub origin: [i64; 2],
    pub rect: [i64; 4],
}

impl View {
    pub fn of(info: &Value) -> Option<View> {
        let n = |v: &Value| v.as_f64().map(|f| f as i64);
        Some(View {
            origin: [n(&info["origin"][0])?, n(&info["origin"][1])?],
            rect: [
                n(&info["rect"][0])?,
                n(&info["rect"][1])?,
                n(&info["rect"][2])?,
                n(&info["rect"][3])?,
            ],
        })
    }

    /// The frame cell of a screen cell; `None` outside the view.
    pub fn to_frame(&self, x: i64, y: i64) -> Option<(i64, i64)> {
        let (dx, dy) = (x - self.rect[0], y - self.rect[1]);
        (0 <= dx && dx < self.rect[2] && 0 <= dy && dy < self.rect[3])
            .then(|| (self.origin[0] + dx, self.origin[1] + dy))
    }
}

/// The doc from a read: the remembered `known` lines from `y0`, the
/// view's `shown` lines. The view is told as it is: not yet remembered,
/// it is shown.
pub fn merge(
    known_y0: i64,
    known: Vec<String>,
    view: &View,
    shown: Vec<String>,
) -> (i64, Vec<String>) {
    let (top, height) = (view.origin[1], view.rect[3]);
    let y0 = known_y0.min(top);
    let mut lines = vec![String::new(); (known_y0 - y0) as usize];
    lines.extend(known);
    let want = (top + height - y0).max(0) as usize;
    if lines.len() < want {
        lines.resize(want, String::new());
    }
    let at = (top - y0) as usize;
    let end = (at + shown.len()).min(lines.len());
    lines.splice(at..end, shown);
    (y0, lines)
}

/// The line told by a tip: blanks folded, cut at `TIP` cells.
pub fn tip_text(line: &str) -> String {
    let text = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= TIP {
        return text;
    }
    let mut cut: String = text.chars().take(TIP - 1).collect();
    cut.push('…');
    cut
}

/// Where a tip goes, screen cells: left of the widget's outer `rect`,
/// else right of it; on the inner row `y` of a variant of `size`.
pub fn tip_at(rect: [i64; 4], size: (u32, u32), y: u32, text: &str) -> (i64, i64) {
    let row = rect[1] + (rect[3] - i64::from(size.1)) / 2 + i64::from(y);
    let x = rect[0] - text.chars().count() as i64 - 3;
    let x = if x < 0 { rect[0] + rect[2] + 1 } else { x };
    (x, row)
}

/// The state the deck's handlers are given.
pub struct State {
    pub doc: Doc,
    pub look: Look,
    pub marks: Vec<String>,
    pub width: u32,
    /// The frame mapped.
    pub frame: Value,
    pub view: View,
    /// The cursor, a screen cell (`cursor`).
    pub at: Option<(i64, i64)>,
    /// Seconds: the time of the last look, the `now` of the faces.
    pub now: f64,
    /// The cell row under the pointer; the line there told (`text`).
    pub over: Option<u32>,
    pub tips: bool,
    tip: Option<(u64, (i64, i64, String))>,
    lines: Option<Lines>,
    faces: HashMap<(u32, u32), Vec<Vec<Run>>>,
}

impl State {
    pub fn new(o: Options, frame: Value) -> State {
        State {
            doc: Doc::default(),
            look: o.look,
            marks: o.marks,
            width: o.width,
            frame,
            view: View::default(),
            at: None,
            now: 0.0,
            over: None,
            tips: true,
            tip: None,
            lines: None,
            faces: HashMap::new(),
        }
    }

    /// The face of `w` by `h`, kept until the doc or the time changes.
    pub fn face(&mut self, w: u32, h: u32) -> Vec<Vec<Run>> {
        let (doc, look, now) = (&self.doc, &self.look, self.now);
        if self.lines.as_ref().is_none_or(|l| l.w != w) {
            self.lines = Some(Lines::of(doc, w));
        }
        let lines = self.lines.as_ref().expect("lines");
        (self.faces.entry((w, h)))
            .or_insert_with(|| face_of(doc, lines, h, look, now))
            .clone()
    }

    /// The faces made again at the next render.
    pub fn touch(&mut self) {
        self.faces.clear();
    }

    /// The doc changed: its lines read again too.
    pub fn changed(&mut self) {
        self.lines = None;
        self.faces.clear();
    }

    pub fn sizes(&self) -> Vec<(u32, u32)> {
        let w = self.width;
        heights(self.doc.lines.len())
            .into_iter()
            .map(|h| (w, h))
            .collect()
    }

    /// The searches of `--mark`, as `batch` calls.
    fn finds(&self) -> Vec<Value> {
        (self.marks.iter())
            .map(|p| {
                json!({"method": "screen.find", "params": {
                    "pattern": (p.as_str()), "regex": true, "known": true,
                    "frame": (self.frame.clone()),
                }})
            })
            .collect()
    }

    /// Reads the buffer, the view and the marks at `now` (seconds);
    /// `false` while the frame cannot be read (gone, settling): the doc
    /// stands.
    pub fn read(&mut self, now: f64) -> Result<bool, Error> {
        let frame = self.frame.clone();
        let mut calls = vec![
            json!({"method": "frame.get", "params": {"frame": (frame.clone())}}),
            json!({"method": "frame.text", "params": {"frame": (frame.clone())}}),
            json!({"method": "screen.text", "params": {"frame": (frame.clone())}}),
            json!({"method": "screen.cursor", "params": {}}),
        ];
        calls.extend(self.finds());
        let out = match call("batch", json!({ "calls": calls })) {
            Ok(out) => out,
            Err(e) if e.code == "no-frame" || e.code == "not-usable" => return Ok(false),
            Err(e) => return Err(e),
        };
        let results = out["results"].as_array().cloned().unwrap_or_default();
        if results.len() < 4 || results[..3].iter().any(|r| !r["error"].is_null()) {
            return Ok(false);
        }
        let Some(view) = View::of(&results[0]["result"]["frame"]) else {
            return Ok(false);
        };
        let strings = |v: &Value| -> Vec<String> {
            (v.as_array().into_iter().flatten())
                .map(|l| l.as_str().unwrap_or_default().to_string())
                .collect()
        };
        let known = &results[1]["result"];
        let known_y0 = known["y0"].as_f64().unwrap_or(0.0) as i64;
        let shown = strings(&results[2]["result"]["lines"]);
        let (y0, lines) = merge(known_y0, strings(&known["lines"]), &view, shown);
        let mut marks = BTreeMap::new();
        if results[3]["error"].is_null() {
            self.at_cursor(&results[3]["result"]);
        }
        for (p, r) in results[4..].iter().enumerate() {
            for m in (r["result"]["matches"].as_array().into_iter()).flatten() {
                if let Some(y) = m["y"].as_f64() {
                    marks.entry(y as i64).or_insert(p);
                }
            }
        }
        let doc = &mut self.doc;
        (doc.y0, doc.lines, doc.marks) = (y0, lines, marks);
        (doc.top, doc.view, doc.width) = (view.origin[1], view.rect[3], view.rect[2]);
        self.view = view;
        self.now = now;
        self.changed();
        self.doc.visit(now);
        self.cursor();
        Ok(true)
    }

    /// The cursor's screen cell, from `cursor` or `screen.cursor`.
    pub fn at_cursor(&mut self, data: &Value) {
        let shown = data["visible"] != false;
        self.at = match (shown, data["x"].as_f64(), data["y"].as_f64()) {
            (true, Some(x), Some(y)) => Some((x as i64, y as i64)),
            _ => None,
        };
    }

    /// The row of the cursor, from its screen cell and the view now.
    pub fn cursor(&mut self) {
        let pos = self.at.and_then(|(x, y)| self.view.to_frame(x, y));
        self.doc.cursor = pos.map(|p| p.1);
        self.touch();
    }

    /// The line under the pointer told beside `widget`, or the tip
    /// taken away.
    pub fn tell(&mut self, widget: &Widget) {
        let (Some(y), Some(size), Some(rect), true) =
            (self.over, widget.size(), widget.rect(), self.tips)
        else {
            return self.hush();
        };
        let Some(row) = line_at(&self.doc, size.0, size.1, y) else {
            return self.hush();
        };
        let text = tip_text(&self.doc.lines[(row - self.doc.y0) as usize]);
        if text.is_empty() {
            return self.hush();
        }
        let (x, y) = tip_at(rect, size, y, &text);
        let said = (x, y, text);
        if self.tip.as_ref().is_some_and(|t| t.1 == said) {
            return;
        }
        self.hush();
        let (vx, vy) = (said.0 - self.view.rect[0], said.1 - self.view.rect[1]);
        let cells = json!([[{"t": (format!(" {} ", said.2)), "fg": 255, "bg": GROUND}]]);
        let sprite = json!({
            "frame": (self.frame.clone()),
            "content": {"cells": cells, "opaque": true},
            "anchor": {"at": [vx, vy], "space": "view"},
            "layer": "front", "z": 9,
        });
        match put(sprite) {
            Ok(id) => self.tip = Some((id, said)),
            Err(e) => log(&format!("minimap: the tip: {e}")),
        }
    }

    pub fn hush(&mut self) {
        if let Some((id, _)) = self.tip.take()
            && let Err(e) = remove(id)
            && e.code != "no-sprite"
        {
            log(&format!("minimap: the tip: {e}"));
        }
    }
}

struct Minimap {
    deck: Deck<State>,
    st: State,
    /// A burst of changes under way; an event heard in it since the
    /// last tick; the tick it began at.
    dirty: bool,
    heard: bool,
    since: Option<u64>,
    /// Rows told edited, stamped at the next tick.
    edited: Vec<i64>,
    offered: Vec<(u32, u32)>,
    titled: String,
    asked: u32,
}

fn failed(e: Error) {
    log(&format!("minimap: {e}"));
}

impl Minimap {
    fn period(&mut self, ms: u32) {
        if ms != self.asked {
            self.asked = ms;
            period(ms);
        }
    }

    /// The period at rest: a look now and then while some heat fades.
    fn rest(&mut self) {
        let d = &self.st.doc;
        let warm = self.st.look.heat && !(d.edited.is_empty() && d.visited.is_empty());
        self.period(if warm {
            (self.st.look.fade * 1000.0 / 6.0) as u32
        } else {
            0
        });
    }

    /// The doc read again; the widget declared again when the text wants
    /// other heights, its title set when it changed.
    fn read(&mut self, now: f64) -> Result<(), Error> {
        if !self.st.read(now)? {
            return Ok(());
        }
        let sizes = self.st.sizes();
        if sizes != self.offered {
            self.deck.resize(TAG, &sizes[..], &mut self.st)?;
            self.offered = sizes;
            self.titled.clear();
        }
        if let Some(w) = self.deck.widget(TAG) {
            self.st.tell(w);
        }
        Ok(())
    }

    fn title(&mut self) -> Result<(), Error> {
        let name = title(&self.st.doc);
        if name != self.titled
            && let Some(w) = self.deck.widget(TAG)
            && w.id().is_some()
        {
            w.set(None, None, Some(&name))?;
            self.titled = name;
        }
        Ok(())
    }

    fn tick(&mut self, now_ms: u64) -> Result<(), Error> {
        let now = now_ms as f64 / 1000.0;
        for y in self.edited.drain(..) {
            self.st.doc.edited.insert(y, now);
        }
        if self.dirty {
            let since = *self.since.get_or_insert(now_ms);
            if self.heard && now_ms < since + BURST {
                self.heard = false;
                return Ok(());
            }
            (self.dirty, self.heard, self.since) = (false, false, None);
            self.read(now)?;
        }
        self.st.now = now;
        self.st.doc.forget(now, self.st.look.fade);
        self.st.touch();
        self.deck.tick(now_ms, &mut self.st)?;
        self.title()?;
        self.rest();
        Ok(())
    }

    fn event(&mut self, topic: &str, data: &Value) -> Result<(), Error> {
        match topic {
            "exit" => exit(0),
            "text" if data["frame"] == self.st.frame => {
                let rows = data["rows"].as_array().into_iter().flatten();
                self.edited
                    .extend(rows.filter_map(Value::as_f64).map(|y| y as i64));
            }
            "cursor" => {
                self.st.at_cursor(data);
                self.st.cursor();
                if !self.dirty {
                    self.deck.draw(None, false, &mut self.st)?;
                }
                return Ok(());
            }
            "move" => {
                self.st.frame = data["frame"].clone();
                self.st.doc = Doc::default();
                self.st.hush();
                self.st.changed();
                self.deck.event(topic, data, &mut self.st)?;
            }
            "frames" | "text" => {}
            _ => {
                self.deck.event(topic, data, &mut self.st)?;
                return Ok(());
            }
        }
        if !self.dirty {
            (self.dirty, self.heard, self.since) = (true, false, None);
            self.period(SETTLE as u32);
        } else {
            self.heard = true;
        }
        Ok(())
    }
}

impl App for Minimap {
    fn init(args: Value) -> Minimap {
        let o = Options::of(&args).unwrap_or_else(|e| {
            log(&format!("minimap: {e}"));
            exit(2)
        });
        let frame = json!(manifest().and_then(|m| m.frame).unwrap_or(0));
        // Below `text` the lines are shapes: no comment, no tip.
        let text = manifest().is_none_or(|m| m.sees("text"));
        let mut st = State::new(o, frame.clone());
        st.look.notes &= text;
        st.tips = text;
        for p in st.marks.clone() {
            let find = json!({"pattern": (p.as_str()), "regex": true, "known": true, "frame": (frame.clone())});
            if let Err(e) = call("screen.find", find)
                && e.code == "bad-params"
            {
                log(&format!(
                    "minimap: --mark {p}: not a pattern: {}",
                    e.message
                ));
                exit(2);
            }
        }
        if let Err(e) = st.read(0.0) {
            failed(e);
        }
        let _ = call(
            "subscribe",
            json!({"topics": ["text", "frames", "cursor"], "snapshot": true}),
        );
        let mut deck = Deck::new(json!({"border": "round", "prefer": "right", "frame": frame}));
        deck.hover_timeout = None;
        let offered = st.sizes();
        let spec = Spec::new(TAG, &offered[..], |s: &mut State, w, h, _| {
            Content::from(s.face(w, h))
        })
        .hover(|s, _x, y, widget| {
            s.over = Some(y);
            s.tell(widget);
            false
        })
        .leave(|s, _| {
            s.over = None;
            s.hush();
            false
        })
        .show(json!({"fill": 1.0, "title": (title(&st.doc))}));
        if let Err(e) = deck.add(spec, &mut st) {
            failed(e);
        }
        period(SETTLE as u32);
        Minimap {
            deck,
            st,
            dirty: false,
            heard: false,
            since: None,
            edited: Vec::new(),
            offered,
            titled: String::new(),
            asked: SETTLE as u32,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        if let Err(e) = self.tick(now_ms) {
            failed(e);
        }
    }

    fn event(&mut self, topic: &str, data: Value) {
        if let Err(e) = self.event(topic, &data) {
            failed(e);
        }
    }
}

ptyrc_app::app!(Minimap);

#[cfg(test)]
mod tests;
