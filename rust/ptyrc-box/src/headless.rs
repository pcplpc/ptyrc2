// SPDX-License-Identifier: GPL-3.0-or-later
//! A run without a driver: the scene of `ptyrc-core` over a screen
//! (empty, a text, a tape), the clock an argument. The same module,
//! options and seed give the same report.

use crate::core::{CallError, Host, Limits};
use crate::error::Error;
use crate::grants::{self, Degree, Gate, Grants};
use crate::runner::Runner;
use ptyrc_core::frames::{FrameConfig, FrameEvent, Frames, ScreenInput};
use ptyrc_core::plugin::DrawExt;
use ptyrc_core::scene::{Scene, SpritePatch, SpriteSpec};
use ptyrc_core::shape::{Words, shape};
use ptyrc_core::widgets::{Declare, Set, Show, WidgetConfig, Widgets};
use ptyrc_core::{FrameSel, Surfaces};
use ptyrc_vt::Vt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

/// The owner of the app's sprites and widgets.
const OWNER: u64 = 1;

/// A request the guest made, at session time `t` (ms). `content` and
/// `frames` are left out of the `draw` requests.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub t: u64,
    pub method: String,
    pub params: Value,
    /// The code of the error, when refused.
    pub refused: Option<String>,
    /// The degree the answer was cut to (`grants`).
    pub lowered: Option<String>,
}

/// A permission an app was refused or answered without, headless.
#[derive(Clone, Debug, PartialEq)]
pub struct Lack {
    pub permission: String,
    /// Declared, and denied; else not declared.
    pub declared: bool,
    /// `words`: the words asked and not held.
    pub words: Vec<String>,
}

/// What the user does during a headless run.
#[derive(Clone, Debug, PartialEq)]
pub enum Inject {
    /// An event of a topic, delivered as the driver would: to a topic
    /// subscribed and held.
    Event(String, Value),
    /// A key of the host keyboard, named as `input.send` names it: to
    /// the app when it grabbed it; `ctrl+t` releases the grab.
    Key(String),
}

/// The draw extensions of the driver (`ptyrc-cli`, `plugins.rs`).
pub fn draw_ext() -> Vec<Box<dyn DrawExt>> {
    vec![
        Box::new(ptyrc_draw_shapes::Shapes),
        Box::new(ptyrc_draw_bigtext::BigText),
        Box::new(ptyrc_draw_sand::SandExt),
        Box::new(ptyrc_draw_rain::RainExt),
        Box::new(ptyrc_draw_snow::SnowExt),
        Box::new(ptyrc_draw_borders::Borders),
    ]
}

/// Bytes of a key named as `input.send` names it; `None`: not a name.
pub fn key_bytes(key: &str) -> Option<Vec<u8>> {
    let named: &[u8] = match key {
        "enter" => b"\r",
        "tab" => b"\t",
        "esc" | "escape" => b"\x1b",
        "space" => b" ",
        "backspace" => b"\x7f",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "insert" => b"\x1b[2~",
        "delete" => b"\x1b[3~",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        _ => {
            let mut chars = key.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => Some(c.to_string().into_bytes()),
                _ => {
                    let mut c = key.strip_prefix("ctrl+")?.chars();
                    match (c.next(), c.next()) {
                        (Some(c), None) if c.is_ascii_alphabetic() => {
                            Some(vec![c.to_ascii_lowercase() as u8 & 0x1f])
                        }
                        _ => None,
                    }
                }
            };
        }
    };
    Some(named.to_vec())
}

/// The names `key_bytes` takes, for a hint.
pub const KEY_NAMES: &str = "a character, ctrl+A..ctrl+Z, enter, tab, esc, space, backspace, \
    up, down, left, right, home, end, insert, delete, pageup, pagedown";

fn base64_encode(bytes: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char),
                false => out.push('='),
            }
        }
    }
    out
}

/// The state of `random` for `seed`: each seed its own (splitmix64).
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    // xorshift64 has no state 0.
    if z == 0 { 0x9e37_79b9_7f4a_7c15 } else { z }
}

/// A tape (`plan/design.md`, Tapes): what the wrapped program wrote.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tape {
    pub size: (u16, u16),
    /// `(µs, record)`, in order.
    pub records: Vec<(u64, Record)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Output(Vec<u8>),
    Resize(u16, u16),
}

fn size_of(v: &Value) -> Option<(u16, u16)> {
    let a = v.as_array().filter(|a| a.len() == 2)?;
    let n = |i: usize| a[i].as_u64().filter(|n| (1..=4096).contains(n));
    Some((n(0)? as u16, n(1)? as u16))
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.bytes().filter(|b| *b != b'=') {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

impl Tape {
    /// Versions 1 and 2. Output and resizes are kept; input, colours,
    /// the exit, unknown records are skipped.
    pub fn parse(text: &str) -> Result<Tape, String> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let not = "not a tape: the first line is not {\"ptyrc\":\"tape\",...}";
        let (_, first) = lines.next().ok_or("the tape is empty")?;
        let header: Value = serde_json::from_str(first).map_err(|_| not)?;
        if header["ptyrc"] != "tape" {
            return Err(not.into());
        }
        if !matches!(header["version"].as_u64(), Some(1 | 2)) {
            return Err(format!(
                "tape version {}: versions 1 and 2 are read",
                header["version"]
            ));
        }
        let size = size_of(&header["size"]).ok_or("the header has no size: [cols, rows]")?;
        let mut records = Vec::new();
        let mut last = 0;
        for (n, line) in lines {
            let bad = |what: &str| format!("line {}: {what}", n + 1);
            let v: Value = serde_json::from_str(line).map_err(|e| bad(&e.to_string()))?;
            let t = v["t"].as_u64().ok_or_else(|| bad("no time `t`"))?;
            if t < last {
                return Err(bad("time goes backwards"));
            }
            last = t;
            let record = if let Some(s) = v["o"].as_str() {
                Record::Output(s.as_bytes().to_vec())
            } else if let Some(s) = v["ob"].as_str() {
                Record::Output(base64(s).ok_or_else(|| bad("`ob` is not base64"))?)
            } else if !v["size"].is_null() {
                let (c, r) = size_of(&v["size"]).ok_or_else(|| bad("size: [c, r]"))?;
                Record::Resize(c, r)
            } else {
                continue;
            };
            records.push((t, record));
        }
        Ok(Tape { size, records })
    }
}

/// The wrapped program's screen under the app.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Under {
    /// Blank.
    #[default]
    Empty,
    /// Text written at time 0 from the top left; `\n` ends a line,
    /// escape sequences are read as a terminal would.
    Text(String),
    /// Played in session time: a record at `t` µs is fed at `t / 1000`
    /// ms. Its size is the screen's.
    Tape(Tape),
}

/// The host of a headless run: answers `draw.put`, `draw.update`,
/// `draw.remove`, `draw.clear`, `widget.declare`, `widget.show`,
/// `widget.set`, `widget.hide`, `subscribe`, `unsubscribe`,
/// `session.info`, `screen.text`, `screen.shape`; refuses the rest
/// (`headless`).
pub struct Headless {
    vt: Vt,
    frames: Frames,
    scene: Scene,
    widgets: Widgets,
    topics: BTreeSet<String>,
    /// Topics subscribed and not held: no event comes.
    silent: BTreeSet<String>,
    /// `None`: the whole session, nothing checked.
    grants: Option<Grants>,
    /// The keys grabbed: `Some(empty)`, all.
    grab: Option<Vec<String>>,
    pub lacks: Vec<Lack>,
    /// What was injected and reached nobody: `(ms, what)`.
    pub dropped: Vec<(u64, String)>,
    /// Events of subscribed topics, not yet delivered.
    pending: Vec<(String, Value)>,
    now: u64,
    rng: u64,
    pub requests: Vec<Request>,
    pub log: Vec<String>,
}

fn bad(e: serde_json::Error) -> CallError {
    CallError::new("bad-params", e.to_string())
}

/// `params` without `id`, the `id`.
fn take_id(params: &Value) -> Result<(u64, Value), CallError> {
    let mut p = params.clone();
    let id = p
        .as_object_mut()
        .and_then(|o| o.remove("id"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CallError::new("bad-params", "id: required"))?;
    Ok((id, p))
}

impl Headless {
    pub fn new(size: (u32, u32), seed: u64) -> Headless {
        let clamp = |n: u32| n.clamp(1, 4096) as u16;
        let mut scene = Scene::new();
        let mut widgets = Widgets::new(WidgetConfig::default());
        for ext in draw_ext() {
            for (name, make) in ext.effects() {
                scene.register_effect(name, make);
            }
            for (name, border) in ext.borders() {
                widgets.register_border(name, border);
            }
        }
        let mut h = Headless {
            vt: Vt::new(clamp(size.0), clamp(size.1)),
            // The raw frame usable at once: a test sees what it drew at 0.
            frames: Frames::new(
                FrameConfig {
                    raw_settle_ms: 0,
                    ..FrameConfig::default()
                },
                Vec::new(),
            ),
            scene,
            widgets,
            topics: BTreeSet::new(),
            silent: BTreeSet::new(),
            grants: None,
            grab: None,
            lacks: Vec::new(),
            dropped: Vec::new(),
            pending: Vec::new(),
            now: 0,
            rng: mix(seed),
            requests: Vec::new(),
            log: Vec::new(),
        };
        h.observe();
        h
    }

    /// Session time of the requests to come.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now = now_ms;
    }

    fn at(&self) -> Duration {
        Duration::from_millis(self.now)
    }

    /// Bytes of the wrapped program.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.vt.feed(bytes);
        self.observe();
    }

    /// The screen resized; `size` to the subscribers.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.vt.resize(cols, rows);
        self.observe();
        self.emit("size", json!({"size": [cols, rows]}));
    }

    /// Check the requests against `grants`, as the driver does for a
    /// store app; `None`: the whole session.
    pub fn set_grants(&mut self, grants: Option<Grants>) {
        self.grants = grants;
    }

    fn lack(&mut self, permission: &str, words: &[String]) {
        let declared = self.grants.as_ref().is_some_and(|g| g.declares(permission));
        match self.lacks.iter_mut().find(|l| l.permission == permission) {
            Some(l) => {
                for w in words {
                    if !l.words.contains(w) {
                        l.words.push(w.clone());
                    }
                }
            }
            None => self.lacks.push(Lack {
                permission: permission.to_owned(),
                declared,
                words: words.to_vec(),
            }),
        }
    }

    /// What the user does, now.
    pub fn inject(&mut self, what: &Inject) {
        match what {
            Inject::Event(topic, data) => {
                let why = match () {
                    _ if !self.topics.contains(topic) => "not subscribed",
                    _ if self.silent.contains(topic) => "not held",
                    _ => {
                        self.pending.push((topic.clone(), data.clone()));
                        return;
                    }
                };
                self.dropped
                    .push((self.now, format!("event {topic}: {why}")));
            }
            Inject::Key(key) => {
                let taken = match &self.grab {
                    Some(keys) => keys.is_empty() || keys.contains(key),
                    None => false,
                };
                if self.grab.is_some() && key == "ctrl+t" {
                    self.grab = None;
                    self.pending
                        .push(("input".into(), json!({"released": "escape"})));
                } else if taken {
                    let bytes = key_bytes(key).unwrap_or_default();
                    self.pending.push((
                        "input".into(),
                        json!({"key": key, "bytes": base64_encode(&bytes), "grabbed": true}),
                    ));
                } else {
                    self.dropped
                        .push((self.now, format!("key {key}: to the program, not grabbed")));
                }
            }
        }
    }

    fn emit(&mut self, topic: &str, data: Value) {
        if self.topics.contains(topic) && !self.silent.contains(topic) {
            self.pending.push((topic.to_owned(), data));
        }
    }

    /// The events to deliver, in order.
    pub fn take_events(&mut self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.pending)
    }

    fn observe(&mut self) {
        let events = self.vt.take_events();
        let input = ScreenInput {
            grid: self.vt.grid(),
            cursor: self.vt.cursor(),
            alt_screen: self.vt.modes().alt_screen,
            title: self.vt.title(),
            events: &events,
        };
        let changes = self.frames.update(&input, self.at());
        self.widgets.touch();
        for change in changes {
            match change {
                FrameEvent::Shift {
                    frame,
                    from,
                    to,
                    dy,
                } => self.scene.shift(frame, from, to, dy),
                FrameEvent::Forgotten { frame, from, to } => {
                    self.scene.forget_rows(frame, from, to);
                }
                FrameEvent::Dropped { frame } => {
                    self.scene.drop_frame(frame);
                }
                _ => {}
            }
        }
    }

    /// Lay the widgets out, advance the scene to now.
    pub fn settle(&mut self) {
        let now = self.at();
        let cursor = self.vt.cursor();
        let cursor = cursor.visible.then_some((cursor.col, cursor.row));
        self.widgets
            .tick(now, self.vt.grid(), cursor, &self.frames, &mut self.scene);
        self.scene.tick_on(now, self.vt.grid(), &self.frames);
    }

    fn check_frame(&self, sel: &FrameSel) -> Result<(), CallError> {
        match sel {
            FrameSel::Id(id) if self.frames.info(*id).is_none() => {
                Err(CallError::new("no-frame", format!("no frame {id}")))
            }
            _ => Ok(()),
        }
    }

    fn answer(&mut self, method: &str, params: &Value) -> Result<Value, CallError> {
        let now = self.at();
        let scene_err = |e: ptyrc_core::scene::SceneError| CallError::new(e.code(), e.to_string());
        let widget_err =
            |e: ptyrc_core::widgets::WidgetError| CallError::new(e.code(), e.to_string());
        let (cols, rows) = (self.vt.cols(), self.vt.rows());
        let mut params = params.clone();
        if params.is_null() {
            params = json!({});
        }
        match method {
            "draw.put" => {
                self.scene.name_layer(&mut params);
                let spec: SpriteSpec = serde_json::from_value(params).map_err(bad)?;
                self.check_frame(&spec.frame)?;
                let id = self.scene.add(OWNER, spec, now).map_err(|e| {
                    let mut e = scene_err(e);
                    if e.message.starts_with("unknown effect") {
                        e.hint = Some(format!("effects: {}", self.scene.effects().join(", ")));
                    }
                    e
                })?;
                Ok(json!({"id": id}))
            }
            "draw.update" => {
                let (id, mut p) = take_id(&params)?;
                self.scene.name_layer(&mut p);
                let patch: SpritePatch = serde_json::from_value(p).map_err(bad)?;
                if let Some(sel) = &patch.frame {
                    self.check_frame(sel)?;
                }
                self.scene.update(id, patch, now).map_err(scene_err)?;
                Ok(json!({}))
            }
            "draw.remove" => {
                let (id, _) = take_id(&params)?;
                if self.scene.remove(id) {
                    Ok(json!({}))
                } else {
                    Err(CallError::new("no-sprite", format!("no sprite {id}")))
                }
            }
            "draw.clear" => Ok(json!({"removed": self.scene.clear(Some(OWNER))})),
            "widget.declare" => {
                let decl: Declare = serde_json::from_value(params).map_err(bad)?;
                let tags = self.widgets.declare(OWNER, decl).map_err(widget_err)?;
                Ok(json!({"tags": tags}))
            }
            "widget.show" => {
                self.scene.name_layer(&mut params);
                let mut show: Show = serde_json::from_value(params).map_err(bad)?;
                self.check_frame(&show.frame)?;
                if let Some(id) = self.frames.resolve(&show.frame) {
                    show.frame = FrameSel::Id(id);
                }
                let id = self.widgets.show(OWNER, show).map_err(widget_err)?;
                self.settle();
                Ok(json!({"widget": self.widgets.info(id)}))
            }
            "widget.set" => {
                let set: Set = serde_json::from_value(params).map_err(bad)?;
                self.widgets.set(OWNER, set).map_err(widget_err)?;
                Ok(json!({}))
            }
            "widget.hide" => {
                let (id, _) = take_id(&params)?;
                if self.widgets.hide(OWNER, id) {
                    Ok(json!({}))
                } else {
                    Err(CallError::new("no-sprite", format!("no widget {id}")))
                }
            }
            "subscribe" | "unsubscribe" => {
                let topics = params.get("topics").and_then(Value::as_array);
                let topics: Vec<&str> = topics
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let mut silent = Vec::new();
                if let Some(g) = self.grants.clone()
                    && method == "subscribe"
                {
                    for t in &topics {
                        match grants::judge(&g, grants::topic_gate(t)) {
                            Ok(None) => {}
                            Err(None) => {
                                return Err(CallError {
                                    hint: Some(grants::denied_hint(&g, None)),
                                    ..CallError::new("denied", format!("{t}: denied"))
                                });
                            }
                            Ok(Some(_)) | Err(Some(_)) => silent.push(*t),
                        }
                    }
                }
                for t in topics {
                    if method == "subscribe" {
                        self.topics.insert(t.to_owned());
                        if silent.contains(&t) {
                            if let Gate::Read(d) = grants::topic_gate(t) {
                                self.lack(d.name(), &[]);
                            } else if let Gate::Needs(p) = grants::topic_gate(t) {
                                self.lack(p, &[]);
                            }
                            self.silent.insert(t.to_owned());
                        }
                    } else {
                        self.topics.remove(t);
                        self.silent.remove(t);
                    }
                }
                Ok(json!({}))
            }
            "input.grab" => {
                let keys = params.get("keys").and_then(Value::as_array);
                let keys = keys.into_iter().flatten().filter_map(Value::as_str);
                self.grab = Some(keys.map(str::to_owned).collect());
                Ok(json!({}))
            }
            "input.release" => {
                self.grab = None;
                Ok(json!({}))
            }
            "session.info" => Ok(json!({
                "protocol": ptyrc_proto::PROTOCOL,
                "server": "ptyrc-box",
                "size": [cols, rows],
                "headless": true,
            })),
            "screen.text" => Ok(json!({
                "lines": self.vt.grid().text(),
                "size": [cols, rows],
                "rect": [0, 0, cols, rows],
            })),
            "screen.shape" => {
                let words: Vec<String> = params
                    .get("words")
                    .map(|w| serde_json::from_value(w.clone()))
                    .transpose()
                    .map_err(bad)?
                    .unwrap_or_default();
                let words =
                    Words::new(&words).map_err(|e| CallError::new("bad-params", e.to_string()))?;
                let lines = shape(self.vt.grid(), self.vt.grid().rect(), &words);
                Ok(json!({
                    "lines": lines,
                    "size": [cols, rows],
                    "rect": [0, 0, cols, rows],
                }))
            }
            _ => Err(CallError::new(
                "headless",
                format!("{method}: not answered without a session"),
            )),
        }
    }

    /// The screen under the app, the app's sprites and widgets over
    /// it, as text: one string a row, trailing spaces cut.
    pub fn screen(&self) -> Vec<String> {
        self.scene.compose(self.vt.grid(), &self.frames).text()
    }
}

impl Headless {
    /// The answer as the driver gives it to an app of these grants:
    /// denied, cut to its degree, or whole; the degree when cut.
    fn checked(
        &mut self,
        method: &str,
        params: &Value,
    ) -> (Result<Value, CallError>, Option<Degree>) {
        let Some(g) = self.grants.clone() else {
            return (self.answer(method, params), None);
        };
        if let Err(lacks) = grants::judge(&g, grants::gate(method, params)) {
            if let Some(p) = lacks {
                self.lack(p, &[]);
            }
            let e = CallError {
                hint: Some(grants::denied_hint(&g, lacks)),
                ..CallError::new("denied", format!("{method}: denied"))
            };
            return (Err(e), None);
        }
        let degree = g.degree();
        if degree >= Degree::Text {
            return (self.answer(method, params), None);
        }
        let shape = degree >= Degree::Shape;
        let (cols, rows) = (self.vt.cols(), self.vt.rows());
        let lines = |h: &Headless, words: &[String]| -> Result<Vec<String>, CallError> {
            let words =
                Words::new(words).map_err(|e| CallError::new("bad-params", e.to_string()))?;
            let lines = shape_lines(h, &words);
            Ok(match shape {
                true => lines,
                false => vec![String::new(); lines.len()],
            })
        };
        let cut = |lines: Vec<String>| {
            json!({"lines": lines, "size": [cols, rows], "rect": [0, 0, cols, rows],
                   "degree": degree.name()})
        };
        let answer = match method {
            "screen.text" => {
                self.lack("text", &[]);
                if !shape {
                    self.lack("shape", &[]);
                }
                lines(self, g.held_words()).map(cut)
            }
            "screen.shape" => {
                let asked = params.get("words").and_then(Value::as_array);
                let asked: Vec<String> = asked
                    .into_iter()
                    .flatten()
                    .filter_map(|w| w.as_str().map(str::to_owned))
                    .collect();
                let (kept, gone): (Vec<String>, Vec<String>) =
                    asked.iter().cloned().partition(|w| g.holds_word(w));
                if shape && gone.is_empty() {
                    return (self.answer(method, params), None);
                }
                if !shape {
                    self.lack("shape", &[]);
                }
                if !gone.is_empty() {
                    self.lack("words", &gone);
                }
                lines(self, &kept).map(cut)
            }
            // Marked; not told as lowered: it lacks only `host_colors`.
            "session.info" => {
                let answer = self.answer(method, params).map(|mut v| {
                    v["degree"] = json!(degree.name());
                    v
                });
                return (answer, None);
            }
            _ => return (self.answer(method, params), None),
        };
        (answer, Some(degree))
    }
}

fn shape_lines(h: &Headless, words: &Words) -> Vec<String> {
    shape(h.vt.grid(), h.vt.grid().rect(), words)
}

impl Host for Headless {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, CallError> {
        let (answer, lowered) = self.checked(method, &params);
        if method.starts_with("draw.")
            && let Some(p) = params.as_object_mut()
        {
            p.remove("content");
            p.remove("frames");
            p.remove("rows");
        }
        self.requests.push(Request {
            t: self.now,
            method: method.to_string(),
            params,
            refused: answer.as_ref().err().map(|e| e.code.clone()),
            lowered: lowered.map(|d| d.name().to_owned()),
        });
        answer
    }

    fn size(&self) -> (u32, u32) {
        (u32::from(self.vt.cols()), u32::from(self.vt.rows()))
    }

    fn log(&mut self, text: &str) {
        self.log.push(format!("{:>6} {text}", self.now));
    }

    fn random(&mut self, buf: &mut [u8]) {
        for b in buf {
            // xorshift64
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            *b = (self.rng >> 32) as u8;
        }
    }
}

/// What a headless run is given.
#[derive(Clone, Debug)]
pub struct Options {
    /// The screen, `(columns, rows)`; a tape gives its own.
    pub size: (u32, u32),
    /// The screen under the app.
    pub under: Under,
    /// The argument of `init`.
    pub args: Value,
    /// Session time the run lasts, ms.
    pub duration_ms: u64,
    /// Times of the screens wanted, ms; empty: the end.
    pub at: Vec<u64>,
    /// Events delivered whether subscribed or not: `(ms, topic, data)`.
    pub events: Vec<(u64, String, Value)>,
    /// What the user does, delivered as the driver would: `(ms, what)`.
    pub inject: Vec<(u64, Inject)>,
    /// The permissions held, checked as the driver checks a store app;
    /// `None`: the whole session.
    pub grants: Option<Grants>,
    /// Seed of `random`.
    pub seed: u64,
    pub limits: Limits,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            size: (80, 24),
            under: Under::Empty,
            args: json!({}),
            duration_ms: 1000,
            at: Vec::new(),
            events: Vec::new(),
            inject: Vec::new(),
            grants: None,
            seed: 1,
            limits: Limits::default(),
        }
    }
}

/// What a headless run gives back.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// `(ms, rows)`.
    pub screens: Vec<(u64, Vec<String>)>,
    pub requests: Vec<Request>,
    pub log: Vec<String>,
    /// The limits met.
    pub met: Vec<String>,
    /// `tick` calls made.
    pub ticks: u64,
    /// How the guest ended before the run did. `exit(0)`: `Exit(0)`.
    pub end: Option<Error>,
    /// The permissions the app was refused or answered without.
    pub lacks: Vec<Lack>,
    /// What was injected and reached nobody: `(ms, what)`.
    pub dropped: Vec<(u64, String)>,
}

impl Lack {
    /// One line: what the manifest lacks, or what was denied.
    pub fn line(&self) -> String {
        match self.declared {
            true => format!("denied: {}, declared and answered no", self.permission),
            false => format!(
                "not declared: {}; the line of the manifest that would ask for it: {}",
                self.permission,
                grants::manifest_line(&self.permission, &self.words)
            ),
        }
    }
}

impl Report {
    /// Each method cut to a degree: `(first ms, method, degree, times)`.
    pub fn lowered(&self) -> Vec<(u64, &str, &str, usize)> {
        let mut out: BTreeMap<(&str, &str), (u64, usize)> = BTreeMap::new();
        for r in &self.requests {
            if let Some(d) = &r.lowered {
                let e = out
                    .entry((r.method.as_str(), d.as_str()))
                    .or_insert((r.t, 0));
                e.1 += 1;
            }
        }
        let mut v: Vec<_> = out
            .into_iter()
            .map(|((m, d), (t, n))| (t, m, d, n))
            .collect();
        v.sort();
        v
    }

    /// The report as one JSON value: `screens`, `refused`, `lowered`,
    /// `dropped`, `lacks`, `limits`, `end`.
    pub fn to_json(&self) -> Value {
        let screens: Vec<Value> = self
            .screens
            .iter()
            .map(|(t, rows)| {
                let last = rows
                    .iter()
                    .rposition(|r| !r.is_empty())
                    .map_or(0, |i| i + 1);
                json!({"t": t, "rows": &rows[..last]})
            })
            .collect();
        let refused: Vec<Value> = self
            .requests
            .iter()
            .filter_map(|r| {
                let code = r.refused.as_ref()?;
                Some(json!({"t": r.t, "method": r.method, "code": code}))
            })
            .collect();
        let lowered: Vec<Value> = self
            .lowered()
            .into_iter()
            .map(|(t, m, d, n)| json!({"t": t, "method": m, "degree": d, "times": n}))
            .collect();
        let dropped: Vec<Value> = (self.dropped.iter())
            .map(|(t, w)| json!({"t": t, "what": w}))
            .collect();
        let lacks: Vec<Value> = self
            .lacks
            .iter()
            .map(|l| {
                json!({"permission": l.permission, "declared": l.declared, "words": l.words,
                       "line": l.line()})
            })
            .collect();
        json!({
            "screens": screens,
            "refused": refused,
            "lowered": lowered,
            "dropped": dropped,
            "lacks": lacks,
            "limits": self.met,
            "end": self.end.as_ref().map(|e| e.to_string()),
        })
    }
}

impl fmt::Display for Report {
    /// The screens, the requests refused, the limits met, the end.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (t, rows) in &self.screens {
            writeln!(f, "--- {t} ms")?;
            let last = rows
                .iter()
                .rposition(|r| !r.is_empty())
                .map_or(0, |i| i + 1);
            for row in &rows[..last] {
                writeln!(f, "{row}")?;
            }
        }
        for r in self.requests.iter().filter(|r| r.refused.is_some()) {
            let code = r.refused.as_deref().unwrap_or("");
            writeln!(f, "refused at {} ms: {} ({code})", r.t, r.method)?;
        }
        for (t, method, degree, n) in self.lowered() {
            let times = if n > 1 {
                format!(", {n} times")
            } else {
                String::new()
            };
            writeln!(
                f,
                "lowered at {t} ms: {method}, to the degree {degree}{times}"
            )?;
        }
        for (t, what) in &self.dropped {
            writeln!(f, "dropped at {t} ms: {what}")?;
        }
        for lack in &self.lacks {
            writeln!(f, "{}", lack.line())?;
        }
        for limit in &self.met {
            writeln!(f, "limit met: {limit}")?;
        }
        if let Some(e) = &self.end {
            writeln!(f, "ended: {e}")?;
        }
        Ok(())
    }
}

/// `\n` as `\r\n`, from the top left.
fn screen_bytes(text: &str) -> Vec<u8> {
    let mut out = b"\x1b[H".to_vec();
    let text = text.strip_suffix('\n').unwrap_or(text);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(line.strip_suffix('\r').unwrap_or(line).as_bytes());
    }
    out
}

/// Run a module headless. `Err`: it did not load.
pub fn run(wasm: &[u8], options: &Options) -> Result<Report, Error> {
    let (size, tape) = match &options.under {
        Under::Tape(t) => ((u32::from(t.size.0), u32::from(t.size.1)), &t.records[..]),
        _ => (options.size, &[][..]),
    };
    let mut host = Headless::new(size, options.seed);
    host.set_grants(options.grants.clone());
    if let Under::Text(text) = &options.under {
        host.feed(&screen_bytes(text));
    }
    let mut tape = tape.iter().peekable();
    // What the tape wrote at 0 is there before `init`.
    while let Some((_, r)) = tape.next_if(|(t, _)| *t / 1000 == 0) {
        match r {
            Record::Output(b) => host.feed(b),
            Record::Resize(c, r) => host.resize(*c, *r),
        }
    }
    host.take_events();
    let mut runner = Runner::load(wasm, host, options.limits.clone())?;
    let end = options.duration_ms;
    let mut at: Vec<u64> = options.at.iter().copied().filter(|t| *t <= end).collect();
    if at.is_empty() {
        at.push(end);
    }
    at.sort_unstable();
    at.dedup();
    let mut events: Vec<&(u64, String, Value)> =
        options.events.iter().filter(|e| e.0 <= end).collect();
    events.sort_by_key(|e| e.0);
    let (mut events, mut at) = (events.into_iter().peekable(), at.into_iter().peekable());
    let mut inject: Vec<&(u64, Inject)> = options.inject.iter().filter(|e| e.0 <= end).collect();
    inject.sort_by_key(|e| e.0);
    let mut inject = inject.into_iter().peekable();
    let mut screens = Vec::new();
    let mut ticks = 0;

    let mut done = runner.init(&options.args, 0);
    let mut period = runner.period();
    let mut next_tick = (period > 0).then_some(u64::from(period));
    // At one time: the tape, the events, the tick, the screen.
    while done.is_ok() {
        let now = [
            tape.peek().map(|r| r.0 / 1000).filter(|t| *t <= end),
            events.peek().map(|e| e.0),
            inject.peek().map(|e| e.0),
            next_tick.filter(|t| *t <= end),
            at.peek().copied(),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(now) = now else { break };
        runner.host_mut().set_now(now);
        while let Some((_, r)) = tape.next_if(|(t, _)| *t / 1000 <= now) {
            match r {
                Record::Output(b) => runner.host_mut().feed(b),
                Record::Resize(c, r) => runner.host_mut().resize(*c, *r),
            }
        }
        while let Some((_, what)) = inject.next_if(|e| e.0 == now) {
            runner.host_mut().inject(what);
        }
        for (topic, data) in runner.host_mut().take_events() {
            if done.is_ok() {
                done = runner.event(&topic, &data, now);
            }
        }
        while done.is_ok()
            && let Some(e) = events.next_if(|e| e.0 == now)
        {
            done = runner.event(&e.1, &e.2, now);
        }
        if done.is_ok() && next_tick == Some(now) {
            if !runner.paused() {
                ticks += 1;
            }
            done = runner.tick(now);
        }
        if runner.period() != period || next_tick == Some(now) {
            period = runner.period();
            next_tick = (period > 0).then_some(now + u64::from(period));
        }
        runner.host_mut().settle();
        if at.next_if_eq(&now).is_some() {
            screens.push((now, runner.host().screen()));
        }
    }
    // Ended early: the screens left show what stays.
    for t in at {
        let host = runner.host_mut();
        host.set_now(t);
        host.settle();
        screens.push((t, host.screen()));
    }
    let met = runner.met().iter().map(|m| m.to_string()).collect();
    let host = runner.host_mut();
    Ok(Report {
        screens,
        requests: std::mem::take(&mut host.requests),
        lacks: std::mem::take(&mut host.lacks),
        dropped: std::mem::take(&mut host.dropped),
        log: std::mem::take(&mut host.log),
        met,
        ticks,
        end: done.err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(declared: &[&str], denied: &[&str]) -> Headless {
        let mut h = Headless::new((20, 3), 1);
        h.feed(b"hello fish\r\nworld");
        h.set_grants(Some(Grants::new(declared, denied, &["fish"])));
        h
    }

    #[test]
    fn the_effects_of_the_driver() {
        let mut h = Headless::new((20, 3), 1);
        let put = |name: &str| json!({"effect": {"name": name, "params": {}}});
        assert!(h.call("draw.put", put("sand")).is_ok());
        let e = h.call("draw.put", put("dune")).unwrap_err();
        assert_eq!(e.code, "bad-params");
        let hint = e.hint.unwrap();
        assert!(
            hint.starts_with("effects: ") && hint.contains("sand"),
            "{hint}"
        );
    }

    #[test]
    fn each_seed_its_own() {
        let bytes = |seed| {
            let mut b = [0u8; 16];
            Headless::new((1, 1), seed).random(&mut b);
            b
        };
        assert_ne!(bytes(2), bytes(3));
        assert_ne!(bytes(0), bytes(1));
        assert_eq!(bytes(7), bytes(7));
    }

    /// `plan/store-model.md`, "A no lowers, it does not break": the table.
    #[test]
    fn a_no_lowers() {
        // screen.text, granted shape, words: the rows of screen.shape.
        let mut h = host(&["shape", "words"], &[]);
        let t = h.call("screen.text", json!({})).unwrap();
        assert_eq!(t["degree"], "words");
        assert_eq!(t["lines"], json!(["##### fish", "#####", ""]));
        let mut h = host(&["shape"], &[]);
        let t = h.call("screen.text", json!({})).unwrap();
        assert_eq!(t["degree"], "shape");
        assert_eq!(t["lines"][0], "##### ####");
        // screen.shape with words, granted shape: the shape, no word.
        let s = h.call("screen.shape", json!({"words": ["fish"]})).unwrap();
        assert_eq!(
            (&s["degree"], &s["lines"][0]),
            (&json!("shape"), &json!("##### ####"))
        );
        assert_eq!(h.lacks[1].permission, "words");
        assert_eq!(h.lacks[1].words, ["fish"]);
        // Held whole: not cut, not marked.
        let s = h.call("screen.shape", json!({})).unwrap();
        assert!(s.get("degree").is_none());
        // Less than shape: a blank screen of that size.
        let mut h = host(&["shape"], &["shape"]);
        let s = h.call("screen.shape", json!({})).unwrap();
        assert_eq!(s["lines"], json!(["", "", ""]));
        assert_eq!(
            (&s["degree"], &s["size"]),
            (&json!("none"), &json!([20, 3]))
        );
        let i = h.call("session.info", json!({})).unwrap();
        assert_eq!(i["degree"], "none");
        assert_eq!(h.lacks[0].line(), "denied: shape, declared and answered no");
        // A topic not held: subscribed, no event comes.
        h.call("subscribe", json!({"topics": ["input", "size"]}))
            .unwrap();
        h.inject(&Inject::Event("input".into(), json!({"key": "a"})));
        h.inject(&Inject::Event("size".into(), json!({"size": [1, 1]})));
        assert_eq!(h.take_events().len(), 1);
        assert_eq!(h.dropped[0].1, "event input: not held");
        assert!(h.lacks[1].line().contains("[permissions] keys-read = "));
        // An act: denied, the hint names the permission.
        let e = h.call("input.grab", json!({"keys": ["up"]})).unwrap_err();
        assert_eq!(e.code, "denied");
        assert!(e.hint.unwrap().contains("[permissions] keys-arrows"));
        let e = h.call("input.send", json!({"keys": ["a"]})).unwrap_err();
        assert_eq!(e.code, "denied");
        // Whole: as before.
        let mut h = Headless::new((20, 3), 1);
        h.feed(b"hi");
        assert_eq!(h.call("screen.text", json!({})).unwrap()["lines"][0], "hi");
    }

    #[test]
    fn keys_to_a_grab() {
        let mut h = host(&["keys-arrows"], &[]);
        h.inject(&Inject::Key("up".into()));
        assert!(h.take_events().is_empty());
        h.call("input.grab", json!({"keys": ["up"]})).unwrap();
        h.inject(&Inject::Key("up".into()));
        h.inject(&Inject::Key("x".into()));
        h.inject(&Inject::Key("ctrl+t".into()));
        let events = h.take_events();
        assert_eq!(
            events[0].1,
            json!({"key": "up", "bytes": "G1tB", "grabbed": true})
        );
        assert_eq!(events[1].1, json!({"released": "escape"}));
        assert_eq!(h.dropped.len(), 2);
        assert_eq!(key_bytes("ctrl+c"), Some(vec![3]));
        assert_eq!(key_bytes("nope"), None);
    }
}
