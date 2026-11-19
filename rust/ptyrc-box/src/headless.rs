// SPDX-License-Identifier: GPL-3.0-or-later
//! A run without a driver: the scene of `ptyrc-core` over a screen
//! (empty, a text, a tape), the clock an argument. The same module,
//! options and seed give the same report.

use crate::core::{CallError, Host, Limits};
use crate::error::Error;
use crate::runner::Runner;
use ptyrc_core::frames::{FrameConfig, FrameEvent, Frames, ScreenInput};
use ptyrc_core::scene::{Scene, SpritePatch, SpriteSpec};
use ptyrc_core::shape::{Words, shape};
use ptyrc_core::widgets::{Declare, Set, Show, WidgetConfig, Widgets};
use ptyrc_core::{FrameSel, Surfaces};
use ptyrc_vt::Vt;
use serde_json::{Value, json};
use std::collections::BTreeSet;
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
            scene: Scene::new(),
            widgets: Widgets::new(WidgetConfig::default()),
            topics: BTreeSet::new(),
            pending: Vec::new(),
            now: 0,
            rng: seed | 1,
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

    fn emit(&mut self, topic: &str, data: Value) {
        if self.topics.contains(topic) {
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
                let id = self.scene.add(OWNER, spec, now).map_err(scene_err)?;
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
                for t in topics.into_iter().flatten().filter_map(Value::as_str) {
                    if method == "subscribe" {
                        self.topics.insert(t.to_owned());
                    } else {
                        self.topics.remove(t);
                    }
                }
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

impl Host for Headless {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, CallError> {
        let answer = self.answer(method, &params);
        if method.starts_with("draw.")
            && let Some(p) = params.as_object_mut()
        {
            p.remove("content");
            p.remove("frames");
        }
        self.requests.push(Request {
            t: self.now,
            method: method.to_string(),
            params,
            refused: answer.as_ref().err().map(|e| e.code.clone()),
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
        log: std::mem::take(&mut host.log),
        met,
        ticks,
        end: done.err(),
    })
}
