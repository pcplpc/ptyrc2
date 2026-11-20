// SPDX-License-Identifier: GPL-3.0-or-later
//! A run without a driver: the scene of `ptyrc-core` over a screen
//! (empty, a text, a tape), the clock an argument. The same module,
//! options and seed give the same report.

use crate::assets::Assets;
use crate::core::{CallError, Host, Limits};
use crate::error::Error;
use crate::grants::{self, Degree, Gate, Grants};
use crate::hostfn::{self, HostFns, Named};
use crate::nozzle::{self, Nozzles};
use crate::runner::Runner;
use ptyrc_core::frames::{FrameConfig, FrameEvent, Frames, ScreenInput};
use ptyrc_core::plugin::DrawExt;
use ptyrc_core::scene::{ColorSpec, Scene, SpritePatch, SpriteSpec};
use ptyrc_core::shape::{Words, shape};
use ptyrc_core::widgets::{Declare, Set, Show, WidgetConfig, Widgets};
use ptyrc_core::{FrameId, FrameSel, RAW, Surfaces, TextMatch, TextQuery};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style, Vt, char_width};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

/// The owner of the app's sprites and widgets.
const OWNER: u64 = 1;

/// The methods a headless run answers.
pub const SERVED: &[&str] = &[
    "batch",
    "draw.put",
    "draw.update",
    "draw.remove",
    "draw.clear",
    "draw.list",
    "draw.ext",
    "draw.exts",
    "widget.declare",
    "widget.show",
    "widget.set",
    "widget.hide",
    "widget.ask",
    "widget.release",
    "widget.list",
    "widget.borders",
    "app.list",
    "subscribe",
    "input.grab",
    "input.release",
    "session.info",
    "screen.text",
    "screen.shape",
    "screen.cells",
    "screen.find",
    "screen.wait",
    "screen.cursor",
    "frame.list",
    "frame.get",
    "frame.at",
    "frame.idle",
    "frame.text",
    "frame.heat",
];

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
    /// A request refused for it (`denied`); else answered lowered.
    pub refused: bool,
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
    /// the app when it grabbed it; `ctrl+t` releases the grab. To the
    /// focused widget, when one is: its escape key takes the focus back,
    /// its close key hides it.
    Key(String),
    /// The host mouse moved to a cell, no button down: `hover`.
    Pointer(u16, u16),
    /// A left click on a cell: `Press`, then `Release` at once.
    Click(u16, u16),
    /// The left button down on a cell: `mouse`; told to an effect that
    /// asked `bomb`.
    Press(u16, u16),
    /// The left button up on a cell: `mouse`, of the sprite pressed;
    /// `bomb` to the effect pressed, held since the press.
    Release(u16, u16),
    /// The user gives the keys to the app's widget of this tag, when
    /// it asks (`widget.ask`), as the hotkey does.
    Focus(String),
    /// Text pasted: the focused widget's; under a grab of every key,
    /// key by key.
    Paste(String),
    /// The host resized, columns and rows: the screen resized, then
    /// `size` (a tape's resize, without the tape).
    Resize(u16, u16),
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
    /// Texts as `Text`, each from its time (ms) on: the screen cleared,
    /// the text written, the topics told as the driver tells them.
    Screens(Vec<(u64, String)>),
}

/// The host of a headless run: answers [`SERVED`]; another method of
/// the driver is refused `headless`, one it does not know
/// `unknown-method`.
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
    /// `text`: per frame, its rect, origin and the text of its rows at
    /// the last look.
    shown: BTreeMap<FrameId, Shown>,
    /// `screen`: the rows at the last look.
    rows: Option<Vec<String>>,
    /// Session time of the last change of the screen, ms.
    changed: u64,
    exts: Vec<Box<dyn DrawExt>>,
    /// In a `batch`: what the driver would leave pending is refused.
    in_batch: bool,
    /// Per widget that asks: its escape and close keys; empty: none.
    escapes: BTreeMap<u64, (String, String)>,
    /// The sprite the pointer is on, last told (`hover`).
    hovered: Option<Value>,
    /// The press on a sprite, its event and whether it is heard without
    /// `clicks`, until its release.
    mouse_held: Option<(Value, bool)>,
    /// The effects told the pointer (`nozzle`).
    nozzles: Nozzles,
    /// `app.list`: the app's name and origin.
    pub name: String,
    pub store: Option<String>,
    /// Clock, storage, result, files, probes.
    pub fns: HostFns,
    pub assets: Option<Assets>,
    /// The launcher's options of the run.
    pub launcher: Launcher,
    /// Topic `idle` (`ptyrc-server`, `watch_idle`): heard, the next
    /// look, the last, the frames seen idle.
    idle_watch: bool,
    idle_due: Option<u64>,
    idle_looked: u64,
    idle_seen: BTreeMap<FrameId, bool>,
    /// Bytes of the program fed: else the screen is blank, idle.
    fed: bool,
}

/// The launcher's options (`ptyrc app ... --front`): applied to the
/// app's requests as the driver applies them to a store app's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Launcher {
    /// `--front`, `--back`: the band of its sprites and widgets.
    pub front: Option<bool>,
    /// `--prefer P`: of its widgets, of an anchor placed by the heat.
    pub prefer: Option<String>,
    /// `--alpha A`: the opacity of its sprites, multiplied.
    pub alpha: Option<f64>,
    /// `--border B`: of its widgets.
    pub border: Option<String>,
}

/// `ptyrc-server`, `screen.rs`: `IDLE_PERIOD`, `IDLE_STEP`.
const IDLE_PERIOD_MS: u64 = 250;
const IDLE_STEP_MS: u64 = 20;

/// A frame at the last look: rect, origin, the text of its rows.
type Shown = ([u16; 4], [i64; 2], Vec<String>);

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
        let exts = draw_ext();
        for ext in &exts {
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
            shown: BTreeMap::new(),
            rows: None,
            changed: 0,
            exts,
            in_batch: false,
            escapes: BTreeMap::new(),
            hovered: None,
            mouse_held: None,
            nozzles: Nozzles::default(),
            name: "app".into(),
            store: None,
            fns: HostFns::default(),
            assets: None,
            launcher: Launcher::default(),
            idle_watch: false,
            idle_due: None,
            idle_looked: 0,
            idle_seen: BTreeMap::new(),
            fed: false,
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
        self.fed = true;
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
        self.lack_as(permission, words, false);
    }

    fn lack_as(&mut self, permission: &str, words: &[String], refused: bool) {
        let g = self.grants.as_ref();
        // A word asked that the list does not hold is not declared.
        let listed = words
            .iter()
            .all(|w| g.is_some_and(|g| g.words.iter().any(|h| h.eq_ignore_ascii_case(w))));
        let declared = g.is_some_and(|g| g.declares(permission)) && listed;
        let found =
            (self.lacks.iter_mut()).find(|l| l.permission == permission && l.refused == refused);
        match found {
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
                refused,
                words: words.to_vec(),
            }),
        }
    }

    /// What the user does, now.
    pub fn inject(&mut self, what: &Inject) {
        match what {
            Inject::Resize(c, r) => self.resize(*c, *r),
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
            Inject::Pointer(x, y) => self.hover(*x, *y),
            Inject::Press(x, y) | Inject::Release(x, y) => {
                let press = matches!(what, Inject::Press(..));
                if let Some(why) = self.button(press, *x, *y) {
                    let what = if press { "press" } else { "release" };
                    self.dropped
                        .push((self.now, format!("{what} {x},{y}: {why}")));
                }
            }
            Inject::Click(x, y) => {
                let why = [self.button(true, *x, *y), self.button(false, *x, *y)];
                if let Some(why) = why[0].or(why[1]) {
                    self.dropped
                        .push((self.now, format!("click {x},{y}: {why}")));
                }
            }
            Inject::Focus(tag) => {
                let id = (self.widgets.list().into_iter())
                    .find(|w| w.owner == OWNER && w.tag == *tag && w.asks.is_some())
                    .map(|w| w.id);
                match id.map(|id| self.widgets.focus(id)) {
                    Some(Ok(())) => self.settle(),
                    _ => self.dropped.push((
                        self.now,
                        format!("focus {tag}: no widget of this tag asks (widget.ask)"),
                    )),
                }
            }
            Inject::Paste(text) => match self.widgets.focused() {
                Some((id, _)) if !text.is_empty() => {
                    (self.pending).push(("input".into(), json!({"widget": id, "paste": text})));
                }
                _ if self.grab.as_ref().is_some_and(Vec::is_empty) => {
                    for c in text.chars() {
                        self.inject(&Inject::Key(c.to_string()));
                    }
                }
                _ => self
                    .dropped
                    .push((self.now, "paste: to the program, nothing focused".into())),
            },
            Inject::Key(key) if self.widgets.focused().is_some() => {
                let (id, _) = self.widgets.focused().expect("focused");
                let (escape, close) = self.escapes.get(&id).cloned().unwrap_or_default();
                let reason = match key.as_str() {
                    k if k == escape => Some("escape"),
                    k if k == close => Some("close"),
                    _ => None,
                };
                match reason {
                    Some(reason) => {
                        if reason == "close" {
                            self.widgets.hide(OWNER, id);
                        } else {
                            self.widgets.blur();
                        }
                        self.settle();
                        let data = json!({"widget": id, "released": reason});
                        self.pending.push(("input".into(), data));
                    }
                    None => {
                        let bytes = key_bytes(key).unwrap_or_default();
                        let data =
                            json!({"key": key, "bytes": base64_encode(&bytes), "widget": id});
                        self.pending.push(("input".into(), data));
                    }
                }
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

    /// The app's sprite at a cell, as the driver hit-tests it
    /// (`screen::sprite_at`): the topmost, no effect; and whether it
    /// is heard without `pointer`/`clicks`: a widget, or in front.
    fn sprite_at(&self, x: u16, y: u16) -> Option<(Value, bool)> {
        let (cx, cy) = (i64::from(x), i64::from(y));
        let sprites = self.scene.list();
        let (hit, [sx, sy]) = sprites.iter().rev().find_map(|s| {
            let [sx, sy] = s.screen?;
            let [w, h] = s.size;
            let inside = cx >= sx && cx < sx + i64::from(w) && cy >= sy && cy < sy + i64::from(h);
            (inside && !s.kind.starts_with("effect")).then_some((s, [sx, sy]))
        })?;
        let at = self.frames.at(x, y);
        let widget = self.widgets.list().iter().any(|w| w.sprite == Some(hit.id));
        let data = json!({
            "sprite": hit.id, "at": [cx - sx, cy - sy], "x": x, "y": y,
            "frame": at.map(|(id, _)| id), "pos": at.map(|(_, pos)| [pos.x, pos.y]),
            "mods": [],
        });
        Some((data, widget || hit.layer > 0))
    }

    /// A move of the pointer to a cell, no button down: told to the
    /// effects that asked it; `hover` to the app as the driver tells it
    /// to a store app: on its own widget or front sprite, else with
    /// `pointer`.
    fn hover(&mut self, x: u16, y: u16) {
        self.settle();
        let (scene, frames) = (&self.scene, &self.frames);
        let tunes = (self.nozzles).hover(x, y, &|id, x, y| canvas_at(scene, frames, id, x, y));
        self.tune(tunes);
        let by_effect = self.nozzles.wants().0;
        let hit = self.sprite_at(x, y);
        let heard =
            |h: &Headless, own: bool| own || h.grants.as_ref().is_none_or(|g| g.holds("pointer"));
        let why = match hit {
            None if self.hovered.take().is_some() => {
                let data = json!({"sprite": null, "at": null, "x": x, "y": y,
                                  "frame": null, "pos": null, "mods": []});
                self.emit("hover", data);
                return;
            }
            None if by_effect => return,
            None => "on no sprite of the app",
            Some(_) if !self.topics.contains("hover") => "not subscribed",
            Some((_, own)) if !heard(self, own) => {
                self.lack("pointer", &[]);
                "not held"
            }
            Some((data, _)) => {
                self.hovered = Some(data["sprite"].clone());
                self.emit("hover", data);
                return;
            }
        };
        self.dropped
            .push((self.now, format!("pointer {x},{y}: {why}")));
    }

    /// A press (`press`) or a release of the left button at a cell, as
    /// the driver tells it (`sort_mouse`): to the effects that asked the
    /// clicks, the press taken from the program with `clicks-grab`; to
    /// the app on its own widget or front sprite, else with `clicks`. A
    /// release after a press on a sprite is of that sprite, `at`,
    /// `frame`, `pos` null when off it. Why it reached nobody, if so.
    fn button(&mut self, press: bool, x: u16, y: u16) -> Option<&'static str> {
        self.settle();
        let (scene, frames) = (&self.scene, &self.frames);
        let (tunes, by_effect) = (self.nozzles).click(press, (x, y), self.now, &|id, x, y| {
            canvas_at(scene, frames, id, x, y)
        });
        self.tune(tunes);
        if press
            && by_effect
            && self
                .grants
                .as_ref()
                .is_some_and(|g| !g.holds("clicks-grab"))
        {
            // The program has it too.
            self.lack("clicks-grab", &[]);
        }
        let hit = match self.mouse_held.take() {
            Some((mut held, own)) if !press => match self.sprite_at(x, y) {
                Some(at) if at.0["sprite"] == held["sprite"] => Some(at),
                _ => {
                    held["x"] = json!(x);
                    held["y"] = json!(y);
                    for k in ["at", "frame", "pos"] {
                        held[k] = Value::Null;
                    }
                    Some((held, own))
                }
            },
            _ => self.sprite_at(x, y),
        };
        if press {
            self.mouse_held = hit.clone();
        }
        let heard =
            |h: &Headless, own: bool| own || h.grants.as_ref().is_none_or(|g| g.holds("clicks"));
        match hit {
            None if by_effect => None,
            None => Some("on no sprite of the app"),
            Some(_) if !self.topics.contains("mouse") => Some("not subscribed"),
            Some((_, own)) if !heard(self, own) => {
                self.lack("clicks", &[]);
                Some("not held")
            }
            Some((mut data, _)) => {
                data["button"] = json!("left");
                data["action"] = json!(if press { "press" } else { "release" });
                self.emit("mouse", data);
                None
            }
        }
    }

    /// The driver's tunes of the effects told the pointer; a sprite
    /// gone is forgotten.
    fn tune(&mut self, tunes: Vec<(u64, Value)>) {
        for (id, effect) in tunes {
            if self
                .answer("draw.update", &json!({"id": id, "effect": effect}))
                .is_err()
            {
                self.nozzles.forget(id);
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
        self.watch(true);
    }

    fn told(&self, topic: &str) -> bool {
        self.topics.contains(topic) && !self.silent.contains(topic)
    }

    /// Topics `screen` and `text`, as the driver tells them: the rows
    /// changed since the last look. `emit` false: a first look.
    fn watch(&mut self, emit: bool) {
        let grid = self.vt.grid();
        let rows: Vec<String> = (0..grid.rows())
            .map(|r| row_key(grid, 0, grid.cols(), r))
            .collect();
        let screen: Vec<u16> = match &self.rows {
            Some(was) => (0..rows.len())
                .filter(|&i| was.get(i) != Some(&rows[i]))
                .map(|i| i as u16)
                .collect(),
            None => Vec::new(),
        };
        if self.rows.as_ref() != Some(&rows) {
            self.changed = self.now;
            // A change on screen: the next look at idleness.
            let at = self.now.max(self.idle_looked + IDLE_PERIOD_MS);
            self.idle_due = Some(self.idle_due.map_or(at, |due| due.min(at)));
        }
        let mut texts = Vec::new();
        let mut shown = BTreeMap::new();
        if self.told("text") {
            for info in self.frames.list() {
                let [x, y, w, h] = info.rect;
                if info.state == ptyrc_core::FrameState::Inactive
                    || x + w > grid.cols()
                    || y + h > grid.rows()
                {
                    continue;
                }
                let lines: Vec<String> = (y..y + h).map(|r| row_key(grid, x, w, r)).collect();
                if let Some((rect, origin, was)) = self.shown.get(&info.id) {
                    let same = rect[2] == w && origin[0] == info.origin[0];
                    let changed: Vec<i64> = (0..lines.len())
                        .filter(|&i| {
                            let k = info.origin[1] + i as i64 - origin[1];
                            let before = usize::try_from(k).ok().and_then(|k| was.get(k));
                            before.is_some_and(|b| *b != lines[i] || !same)
                        })
                        .map(|i| info.origin[1] + i as i64)
                        .collect();
                    if !changed.is_empty() || *origin != info.origin || *rect != info.rect {
                        texts.push(json!({"frame": info.id, "rows": changed,
                            "origin": info.origin, "rect": info.rect}));
                    }
                }
                shown.insert(info.id, (info.rect, info.origin, lines));
            }
        }
        self.shown = shown;
        self.rows = Some(rows);
        if emit {
            if !screen.is_empty() {
                self.emit("screen", json!({"rows": screen}));
            }
            for t in texts {
                self.emit("text", t);
            }
        }
    }

    /// Lay the widgets out (topic `widgets`), advance the scene to now.
    /// Session time the driver would step the scene and the widgets
    /// next (`ptyrc-server`, `next_wake`), ms; past due: 10 ms on.
    pub fn next_wake(&self) -> Option<u64> {
        let scene = self.scene.next_wake().map(|t| t.as_millis() as u64);
        let widgets = self.widgets.next_wake().map(|t| t.as_millis() as u64);
        let idle = self.idle_due.filter(|_| self.told("idle"));
        let wake = [scene, widgets, idle].into_iter().flatten().min()?;
        Some(if wake <= self.now {
            self.now + 10
        } else {
            wake
        })
    }

    pub fn settle(&mut self) {
        let now = self.at();
        let cursor = self.vt.cursor();
        let cursor = cursor.visible.then_some((cursor.col, cursor.row));
        let changed =
            (self.widgets).tick(now, self.vt.grid(), cursor, &self.frames, &mut self.scene);
        // Topic `widgets`: each widget laid out anew.
        for info in changed {
            self.emit("widgets", json!(info));
        }
        self.scene.tick_on(now, self.vt.grid(), &self.frames);
        self.watch_idle();
    }

    /// Topic `idle`, as the driver tells it: an event a frame on screen
    /// whose idleness changed since the last look; the next look when
    /// the first busy frame can be idle, or at a change on screen. A
    /// screen nothing was written on is blank: idle from the start.
    fn watch_idle(&mut self) {
        let watch = self.told("idle");
        if watch != self.idle_watch {
            self.idle_watch = watch;
            self.idle_due = Some(self.now);
        }
        if !watch {
            self.idle_seen.clear();
            return;
        }
        if self.idle_due.is_none_or(|due| self.now < due) {
            return;
        }
        self.idle_looked = self.now;
        self.idle_due = None;
        let now = self.at();
        let mut seen = BTreeMap::new();
        let mut events = Vec::new();
        for info in self.frames.list() {
            if info.state == ptyrc_core::FrameState::Inactive {
                continue;
            }
            let Some(heat) = self.frames.heat_at(info.id, now) else {
                continue;
            };
            let (idle, score, free) = match self.fed {
                false => (true, 1.0, 1.0),
                true => {
                    let i = heat.idle();
                    (i.idle, i.score, i.free)
                }
            };
            if let Some(left) = heat.idle_in().filter(|_| self.fed) {
                let at = self.now + ((left * 1000.0) as u64).max(IDLE_STEP_MS);
                self.idle_due = Some(self.idle_due.map_or(at, |due| due.min(at)));
            }
            if self.idle_seen.get(&info.id).copied().unwrap_or(false) != idle {
                events.push(json!({"frame": info.id, "idle": idle, "score": score,
                                   "free": free}));
            }
            seen.insert(info.id, idle);
        }
        self.idle_seen = seen;
        for e in events {
            self.emit("idle", e);
        }
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
            "draw.list" => {
                let list = self.scene.list_on(self.vt.grid(), &self.frames);
                let mut v = json!({"sprites": list});
                own_items(&mut v, "sprites");
                Ok(v)
            }
            "draw.exts" => {
                let exts: Vec<Value> = (self.exts.iter())
                    .map(|e| json!({"name": e.name(), "functions": e.functions()}))
                    .collect();
                Ok(json!({"exts": exts, "effects": self.scene.effects()}))
            }
            "draw.ext" => self.draw_ext_call(params),
            "widget.ask" => self.widget_ask(&params),
            "widget.release" => {
                members(&params, &["id"])?;
                let id = widget_id(&params)?;
                if !self.widgets.release(OWNER, id) {
                    return Err(CallError::new("no-sprite", format!("no widget {id}")));
                }
                self.escapes.remove(&id);
                self.settle();
                Ok(json!({}))
            }
            "widget.list" => {
                let mut v = json!({"widgets": self.widgets.list()});
                own_items(&mut v, "widgets");
                Ok(v)
            }
            "widget.borders" => Ok(json!({"borders": self.widgets.borders()})),
            "app.list" => Ok(json!({"apps": [self.app_info()]})),
            "subscribe" => {
                let topics = params.get("topics").and_then(Value::as_array);
                let topics: Vec<&str> = topics
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let known = ptyrc_proto::TOPICS;
                if let Some(t) = topics.iter().find(|t| !known.contains(t)) {
                    return Err(CallError {
                        hint: Some(format!(
                            "{}topics: {}",
                            ptyrc_proto::suggest::did_you_mean(t, known),
                            known.join(", ")
                        )),
                        ..CallError::new("bad-params", format!("no such topic: {t}"))
                    });
                }
                let mut silent = Vec::new();
                if let Some(g) = self.grants.clone() {
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
                let look = topics.iter().any(|t| *t == "text" || *t == "screen");
                for t in topics {
                    self.topics.insert(t.to_owned());
                    if silent.contains(&t) {
                        if let Gate::Read(d) = grants::topic_gate(t) {
                            self.lack(d.name(), &[]);
                        } else if let Gate::Needs(p) = grants::topic_gate(t) {
                            self.lack(p, &[]);
                        }
                        self.silent.insert(t.to_owned());
                    }
                }
                if look {
                    self.watch(false);
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
            "screen.text" => {
                let lines = match params.get("sprites") == Some(&Value::Bool(true)) {
                    true => self.own_composed().text(),
                    false => self.vt.grid().text(),
                };
                Ok(json!({"lines": lines, "size": [cols, rows], "rect": [0, 0, cols, rows]}))
            }
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
            "screen.cells" => {
                let rect = self.view_rect(&params)?;
                let sprites = params.get("sprites") == Some(&Value::Bool(true));
                let composed;
                let grid = match sprites {
                    true => {
                        composed = self.own_composed();
                        &composed
                    }
                    false => self.vt.grid(),
                };
                let rows: Vec<Value> = (rect.y..rect.bottom())
                    .map(|row| {
                        let cells: Vec<&Cell> =
                            (rect.x..rect.right()).map(|c| grid.cell(c, row)).collect();
                        row_runs(&cells)
                    })
                    .collect();
                Ok(json!({"rows": rows, "size": [rect.w, rect.h],
                          "rect": [rect.x, rect.y, rect.w, rect.h]}))
            }
            "frame.text" => {
                let id = self.frame_of(&params)?;
                let info = (self.frames.list().into_iter().find(|f| f.id == id))
                    .ok_or_else(|| CallError::new("no-frame", format!("no frame {id}")))?;
                // As the driver: without a range, the island of the view,
                // at most `MAX` rows around it.
                const MAX: i64 = 100_000;
                let row = |k: &str| params.get(k).and_then(Value::as_i64);
                let (lo, hi) = (info.extent[0], info.extent[1]);
                let y0 =
                    row("y0").unwrap_or_else(|| lo.max(info.origin[1].saturating_sub(MAX / 2)));
                let y1 = row("y1").unwrap_or_else(|| (hi + 1).min(y0.saturating_add(MAX)));
                let y1 = y1.max(y0);
                if y1.saturating_sub(y0) > MAX {
                    return Err(CallError::new(
                        "bad-params",
                        format!("rows {y0}..{y1}: more than {MAX}; give a narrower y0 and y1"),
                    ));
                }
                Ok(json!({"lines": self.frames.text(id, y0, y1), "y0": y0}))
            }
            "frame.heat" => {
                let id = self.frame_of(&params)?;
                let heat = (self.frames.heat_at(id, now))
                    .ok_or_else(|| CallError::new("no-frame", format!("no frame {id}")))?;
                Ok(json!({"rows": heat.digits(), "classes": heat.classes()}))
            }
            "screen.find" => {
                let id = self.frame_of(&params)?;
                let query: TextQuery = serde_json::from_value(params.clone()).map_err(bad)?;
                let known = params
                    .get("known")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let found = (self.frames.try_find(id, &query, known))
                    .map_err(|e| CallError::new("bad-params", e))?;
                let matches: Vec<Value> = found.iter().map(match_json).collect();
                Ok(json!({"matches": matches}))
            }
            "screen.wait" => {
                let id = self.frame_of(&params)?;
                let has = |k: &str| params.get(k).is_some_and(|v| !v.is_null());
                let idle = params.get("idle") == Some(&Value::Bool(true));
                if !has("pattern") && !has("stable_ms") && !idle {
                    return Err(CallError::new(
                        "bad-params",
                        "pattern, stable_ms or idle is required",
                    ));
                }
                let mut out = json!({"matched": true, "match": null});
                // No session time passes in a call: true now, or a timeout;
                // in a batch, what would wait is refused, as the driver.
                let timeout = match self.in_batch {
                    true => Err(not_in_batch(method)),
                    false => Ok(json!({"matched": false})),
                };
                if let Some(ms) = params.get("stable_ms").and_then(Value::as_u64)
                    && self.now < self.changed + ms
                {
                    return timeout;
                }
                if idle {
                    match self.frames.idle(id, now).filter(|i| i.idle) {
                        Some(i) => {
                            out["frame"] = json!(id);
                            out["idle"] = json!(i);
                        }
                        None => return timeout,
                    }
                }
                if has("pattern") {
                    let query: TextQuery = serde_json::from_value(params.clone()).map_err(bad)?;
                    let found = (self.frames.try_find(id, &query, false))
                        .map_err(|e| CallError::new("bad-params", e))?;
                    let gone = params.get("gone") == Some(&Value::Bool(true));
                    match (found.first(), gone) {
                        (Some(m), false) => out["match"] = match_json(m),
                        (None, true) => {}
                        _ => return timeout,
                    }
                }
                Ok(out)
            }
            "screen.cursor" => {
                let c = self.vt.cursor();
                let at = self.frames.at(c.col, c.row);
                let mut out = json!({
                    "x": c.col, "y": c.row, "visible": c.visible,
                    "frame": at.map(|(id, _)| id),
                    "pos": at.map(|(_, pos)| [pos.x, pos.y]),
                });
                if params.get("trail") == Some(&Value::Bool(true)) {
                    let trail: Vec<Value> = (self.frames.cursor_trail().iter())
                        .map(|s| json!([s.at.as_millis() as u64, s.col, s.row]))
                        .collect();
                    out["trail"] = json!(trail);
                }
                Ok(out)
            }
            "frame.list" => Ok(json!({
                "frames": self.frames.list_at(now),
                "layout": self.frames.layout(),
            })),
            "frame.get" => {
                let id = self.frame_of(&params)?;
                let info = self.frames.list_at(now).into_iter().find(|f| f.id == id);
                Ok(json!({"frame": info}))
            }
            "frame.at" => {
                if let Some(name) = params.get("name").and_then(Value::as_str) {
                    let id = self.frame_of(&json!({"frame": name}))?;
                    return Ok(json!({"frame": id}));
                }
                let coord = |k: &str| {
                    let v = params.get(k).and_then(Value::as_u64);
                    v.and_then(|v| u16::try_from(v).ok()).ok_or_else(|| {
                        CallError::new("bad-params", format!("{k}: a screen coordinate"))
                    })
                };
                let (x, y) = (coord("x")?, coord("y")?);
                Ok(match self.frames.at(x, y) {
                    Some((id, pos)) => json!({"frame": id, "pos": [pos.x, pos.y]}),
                    None => json!({"frame": null, "pos": null}),
                })
            }
            "frame.idle" => {
                let id = self.frame_of(&params)?;
                let idle = self.frames.idle(id, now);
                Ok(json!(idle))
            }
            _ if method.starts_with("asset.") && self.in_batch => Err(hostfn::not_in_batch(method)),
            _ if method.starts_with("asset.") => Err(CallError::new(
                "not-usable",
                format!("{method}: answered by the runner"),
            )),
            _ if grants::METHODS.contains(&method) => Err(CallError {
                hint: Some(
                    "a method of the driver, answered in a session (ptyrc dev run), not in a \
                     test: PROTOCOL.md, Headless"
                        .into(),
                ),
                ..CallError::new(
                    "headless",
                    format!("{method}: not answered without a session"),
                )
            }),
            _ => Err(CallError {
                hint: Some(format!(
                    "{}the methods: PROTOCOL.md",
                    ptyrc_proto::suggest::did_you_mean(method, grants::METHODS.iter().copied())
                )),
                ..CallError::new("unknown-method", format!("unknown method: {method}"))
            }),
        }
    }

    /// The frame of `params.frame`, default the raw one.
    fn frame_of(&self, params: &Value) -> Result<FrameId, CallError> {
        let sel = params.get("frame").filter(|f| !f.is_null());
        let Some(sel) = sel else { return Ok(RAW) };
        let sel: FrameSel = serde_json::from_value(sel.clone()).map_err(bad)?;
        (self.frames.resolve(&sel))
            .filter(|id| self.frames.info(*id).is_some())
            .ok_or_else(|| CallError::new("no-frame", format!("no frame {sel:?}")))
    }

    /// The cells a view reads, as the driver: the frame's rect on the
    /// screen, `rect` within it.
    fn view_rect(&self, params: &Value) -> Result<Rect, CallError> {
        let id = self.frame_of(params)?;
        let info = (self.frames.info(id))
            .ok_or_else(|| CallError::new("no-frame", format!("no frame {id}")))?;
        let view = info.rect.intersect(&self.vt.grid().rect());
        let rect = params.get("rect").filter(|r| !r.is_null());
        Ok(match rect {
            None => view,
            Some(r) => {
                let [x, y, w, h]: [u16; 4] = serde_json::from_value(r.clone()).map_err(bad)?;
                Rect::new(view.x.saturating_add(x), view.y.saturating_add(y), w, h).intersect(&view)
            }
        })
    }

    /// `screen.text`, `screen.cells {sprites}`: the app's own sprites
    /// alone over the screen (`ptyrc-server`, `own.rs`: `owners`).
    fn own_composed(&self) -> Grid {
        (self.scene).compose_of(self.vt.grid(), &self.frames, &[OWNER])
    }

    /// The screen under the app, the app's sprites and widgets over
    /// it, as text: one string a row, trailing spaces cut.
    pub fn screen(&self) -> Vec<String> {
        self.scene.compose(self.vt.grid(), &self.frames).text()
    }

    /// `screen`, and its cells not of the default style.
    pub fn shot(&self) -> (Vec<String>, Vec<StyleRun>) {
        let grid = self.scene.compose(self.vt.grid(), &self.frames);
        (grid.text(), style_runs(&grid))
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
        // The pointer told to an effect: the driver's flags.
        let mut params = params.clone();
        let flags = nozzle::take_flags(method, &mut params);
        let (answer, degree) = self.checked_as(method, &params);
        if let (Some(flags), Ok(answer)) = (flags, &answer) {
            let id = (params.get("id").and_then(Value::as_u64))
                .or_else(|| answer.get("id").and_then(Value::as_u64));
            if let Some(id) = id {
                self.nozzles.note(id, flags);
            }
        }
        (answer, degree)
    }

    fn checked_as(
        &mut self,
        method: &str,
        params: &Value,
    ) -> (Result<Value, CallError>, Option<Degree>) {
        let Some(g) = self.grants.clone() else {
            if hostfn::is_host(method) {
                let answer = (self.fns).call(method, params, self.now, false, true, self.in_batch);
                return (answer, None);
            }
            return (self.answer(method, params), None);
        };
        let mut placed = params.clone();
        self.own_layer(&g, method, &mut placed);
        let params = &placed;
        if let Err(lacks) = grants::judge(&g, grants::gate(method, params)) {
            if let Some(p) = lacks {
                self.lack_as(p, &[], true);
            }
            let e = CallError {
                hint: Some(grants::denied_hint(&g, lacks)),
                ..CallError::new("denied", format!("{method}: denied"))
            };
            return (Err(e), None);
        }
        // Clock, storage, result, files, probes: nothing cut but what
        // is lowered whole.
        if hostfn::is_host(method) {
            let low = match grants::gate(method, params) {
                Gate::Lowered(p) if !g.holds(p) => Some(p),
                _ => None,
            };
            let writable = g.holds("files-write");
            let answer = (self.fns).call(
                method,
                params,
                self.now,
                low.is_some(),
                writable,
                self.in_batch,
            );
            if let Some(p) = low {
                self.lack(p, &[]);
                return (answer, Some(Degree::None));
            }
            return (answer, None);
        }
        let degree = g.degree();
        if degree >= Degree::Text {
            return (self.answer(method, params), None);
        }
        let shape = degree >= Degree::Shape;
        let (cols, rows) = (self.vt.cols(), self.vt.rows());
        let rows_n = rows;
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
                lines(self, &g.held_words()).map(cut)
            }
            "screen.cells" => {
                self.lack("text", &[]);
                if !shape {
                    self.lack("shape", &[]);
                }
                lines(self, &g.held_words()).map(|lines| {
                    let rows: Vec<Value> = (lines.into_iter())
                        .map(|l| match l.is_empty() {
                            true => json!([]),
                            false => json!([{"t": l}]),
                        })
                        .collect();
                    json!({"rows": rows, "size": [cols, rows_n], "rect": [0, 0, cols, rows_n],
                           "degree": degree.name()})
                })
            }
            "frame.text" => {
                self.lack("text", &[]);
                if !shape {
                    self.lack("shape", &[]);
                    return (
                        Ok(mark(json!({"lines": [], "y0": 0}), degree)),
                        Some(degree),
                    );
                }
                let mut p = json!({});
                for k in ["frame", "y0", "y1"] {
                    if let Some(v) = params.get(k).filter(|v| !v.is_null())
                        && (k != "frame" || degree >= Degree::Layout)
                    {
                        p[k] = v.clone();
                    }
                }
                let words = g.held_words();
                self.answer(method, &p).map(|v| {
                    let lines: Vec<String> = (v["lines"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .map(|l| text_mask(l, &words))
                        .collect();
                    mark(json!({"lines": lines, "y0": v["y0"]}), degree)
                })
            }
            "frame.heat" if !shape => {
                self.lack("shape", &[]);
                let mut p = json!({});
                if let Some(f) = params.get("frame").filter(|_| degree >= Degree::Layout) {
                    p["frame"] = f.clone();
                }
                self.answer(method, &p).map(|v| {
                    let fill = |member: &str, with: &str| -> Vec<String> {
                        (v[member].as_array().into_iter().flatten())
                            .filter_map(Value::as_str)
                            .map(|row| with.repeat(row.chars().count()))
                            .collect()
                    };
                    mark(
                        json!({"rows": fill("rows", "9"), "classes": fill("classes", ".")}),
                        degree,
                    )
                })
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
            "screen.find" | "screen.wait" if params.get("pattern").is_some() => {
                let gone = params.get("gone") == Some(&Value::Bool(true));
                let none = match method {
                    "screen.find" => json!({"matches": []}),
                    _ => json!({"matched": gone, "match": null}),
                };
                let pattern = params.get("pattern").and_then(Value::as_str).unwrap_or("");
                let regex = params.get("regex") == Some(&Value::Bool(true));
                // The user's pattern, exactly: answered as under `text`.
                if g.user_pattern(pattern, regex) {
                    let mut p = params.clone();
                    if let Some(o) = p.as_object_mut().filter(|_| degree < Degree::Layout) {
                        o.remove("frame");
                    }
                    return (
                        self.answer(method, &p).map(|v| mark(v, degree)),
                        Some(degree),
                    );
                }
                let word =
                    !regex && !pattern.is_empty() && pattern.chars().all(char::is_alphanumeric);
                if !(word && g.finds_word(pattern)) {
                    match word {
                        true => self.lack("words", &[pattern.to_owned()]),
                        false => self.lack("text", &[]),
                    }
                    return (Ok(mark(none, degree)), Some(degree));
                }
                let mut query = json!({"pattern": format!(r"\b{pattern}\b"), "regex": true,
                                       "ignore_case": true});
                for k in ["frame", "known", "gone"] {
                    if let Some(v) = params
                        .get(k)
                        .filter(|_| k != "frame" || degree >= Degree::Layout)
                    {
                        query[k] = v.clone();
                    }
                }
                self.answer(method, &query).map(|v| mark(v, degree))
            }
            "screen.wait" | "screen.cursor" | "frame.idle" if !shape => {
                self.lack("shape", &[]);
                let still = json!({"score": 1.0, "idle": true, "free": 1.0});
                Ok(mark(
                    match method {
                        "screen.cursor" => {
                            let mut c = json!({"x": 0, "y": 0, "visible": false,
                                               "frame": null, "pos": null});
                            if params.get("trail") == Some(&Value::Bool(true)) {
                                c["trail"] = json!([]);
                            }
                            c
                        }
                        "frame.idle" => still,
                        _ if params.get("idle") == Some(&Value::Bool(true)) => {
                            json!({"matched": true, "match": null, "frame": RAW, "idle": still})
                        }
                        _ => json!({"matched": true, "match": null}),
                    },
                    degree,
                ))
            }
            "frame.list" | "frame.get" | "frame.at" => {
                let mut p = params.clone();
                if degree < Degree::Layout {
                    self.lack("layout", &[]);
                    if let Some(o) = p.as_object_mut() {
                        o.remove("frame");
                        o.remove("name");
                    }
                }
                let answer = match method {
                    "frame.list" if degree < Degree::Layout => self
                        .answer("frame.get", &json!({}))
                        .map(|v| json!({"frames": [v["frame"]], "layout": 0})),
                    "frame.at" if degree < Degree::Layout => {
                        let (x, y) = (
                            p.get("x").and_then(Value::as_u64),
                            p.get("y").and_then(Value::as_u64),
                        );
                        Ok(match (x, y) {
                            (Some(x), Some(y)) => json!({"frame": RAW, "pos": [x, y]}),
                            _ => json!({"frame": RAW}),
                        })
                    }
                    _ => self.answer(method, &p),
                };
                answer.map(|mut v| {
                    if let Some(f) = v.get_mut("frame") {
                        frame_info(f, degree);
                    }
                    for f in v
                        .get_mut("frames")
                        .and_then(Value::as_array_mut)
                        .into_iter()
                        .flatten()
                    {
                        frame_info(f, degree);
                    }
                    mark(v, degree)
                })
            }
            // Where an anchor landed, at the degree that finds it.
            "draw.list" => {
                let mut answer = self.answer(method, params);
                let mut needs = Vec::new();
                if let Ok(v) = &mut answer {
                    for item in (v["sprites"].as_array_mut().into_iter()).flatten() {
                        if let Some(d) = grants::landing(item, &g) {
                            let word = item["anchor"]["text"].as_str().map(str::to_owned);
                            needs.push((d, word));
                        }
                    }
                }
                for (d, word) in needs {
                    match (d, word) {
                        (Degree::Words, Some(w)) => self.lack("words", &[w]),
                        (d, _) => self.lack(d.name(), &[]),
                    }
                }
                return (answer, None);
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

/// The shape of a line of text, the words held shown
/// (`ptyrc-server`, `lower::mask`).
fn text_mask(line: &str, words: &[String]) -> String {
    let ink = |c: char, out: &mut String| match (c.is_whitespace(), char_width(c)) {
        (true, _) => out.push(' '),
        (false, 2) => out.push_str("##"),
        (false, 0) => {}
        (false, _) => out.push('#'),
    };
    let held = |w: &str| {
        let w = w.to_lowercase();
        words.iter().any(|h| h.to_lowercase() == w)
    };
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_alphanumeric() {
            let end = (i..chars.len())
                .find(|&j| !chars[j].is_alphanumeric())
                .unwrap_or(chars.len());
            let word: String = chars[i..end].iter().collect();
            let narrow = chars[i..end].iter().all(|&c| char_width(c) == 1);
            match narrow && held(&word) {
                true => out.push_str(&word),
                false => chars[i..end].iter().for_each(|&c| ink(c, &mut out)),
            }
            i = end;
        } else {
            ink(chars[i], &mut out);
            i += 1;
        }
    }
    out.truncate(out.trim_end_matches(' ').len());
    out
}

/// A row as runs of equal style (`ptyrc-server`, `rpc::row_runs`).
fn row_runs(cells: &[&Cell]) -> Value {
    let mut runs: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut style: Option<Style> = None;
    let mut flush = |text: &mut String, style: &Style| {
        let mut run = serde_json::Map::new();
        run.insert("t".into(), Value::String(std::mem::take(text)));
        let colour = |c: Color| serde_json::to_value(ColorSpec(c)).unwrap_or(Value::Null);
        if style.fg != Color::Default {
            run.insert("fg".into(), colour(style.fg));
        }
        if style.bg != Color::Default {
            run.insert("bg".into(), colour(style.bg));
        }
        for (bit, name) in [
            (Attrs::BOLD, "bold"),
            (Attrs::DIM, "dim"),
            (Attrs::ITALIC, "italic"),
            (Attrs::UNDERLINE, "underline"),
            (Attrs::INVERSE, "inverse"),
            (Attrs::STRIKE, "strike"),
            (Attrs::BLINK, "blink"),
        ] {
            if style.attrs.contains(bit) {
                run.insert(name.into(), Value::Bool(true));
            }
        }
        runs.push(Value::Object(run));
    };
    for cell in cells.iter().filter(|c| c.width != 0) {
        match style {
            Some(s) if s == cell.style => {}
            Some(s) => {
                flush(&mut text, &s);
                style = Some(cell.style);
            }
            None => style = Some(cell.style),
        }
        text.push_str(&cell.text);
    }
    if let Some(s) = style {
        flush(&mut text, &s);
    }
    Value::Array(runs)
}

/// `v` marked with the degree it was cut to.
fn mark(mut v: Value, degree: Degree) -> Value {
    if v.is_object() {
        v["degree"] = json!(degree.name());
    }
    v
}

/// A frame info cut to `degree` (`ptyrc-server`, `lower::frame_info`).
fn frame_info(info: &mut Value, degree: Degree) {
    let Some(object) = info.as_object_mut() else {
        return;
    };
    for member in ["key", "support"] {
        if object.contains_key(member) {
            object.insert(member.into(), Value::Null);
        }
    }
    if degree < Degree::Shape {
        object.insert("idle".into(), json!(1.0));
        object.insert("is_idle".into(), json!(true));
        let rows = object
            .get("rect")
            .and_then(|r| r.get(3))
            .and_then(Value::as_i64);
        if object.contains_key("origin") {
            object.insert("origin".into(), json!([0, 0]));
        }
        if let Some(rows) = rows {
            object.insert("extent".into(), json!([0, (rows - 1).max(0)]));
        }
    }
}

fn match_json(m: &TextMatch) -> Value {
    json!({
        "frame": m.frame, "x": m.x, "y": m.y, "len": m.len, "text": m.text,
        "screen": m.screen.map(|(c, r)| json!([c, r])),
    })
}

/// The text of the cells `x..x + w` of row `r`.
fn row_key(grid: &Grid, x: u16, w: u16, r: u16) -> String {
    (x..x + w)
        .map(|c| grid.cell(c, r))
        .filter(|c| c.width != 0)
        .map(|c| c.text.as_str())
        .collect()
}

/// A layer on the wire that asks to be in front of the text.
fn in_front(layer: &Value) -> bool {
    match layer {
        Value::String(name) => matches!(name.as_str(), "front" | "hud"),
        layer => layer.as_i64().is_some_and(|z| z > 0),
    }
}

/// The layer of a store app (`ptyrc-server`, `own::band`): its band,
/// behind the text unless it holds the permission.
fn band(front: bool) -> i64 {
    let band = 2 + (OWNER % 97) as i64;
    if front { band } else { -band }
}

impl Headless {
    /// The layer of a draw, as the driver sets it for a store app: in
    /// front only with `front` (a widget: or `front-widget`); else
    /// behind, told as lowered.
    fn own_layer(&mut self, g: &Grants, method: &str, params: &mut Value) {
        let Some(object) = params.as_object_mut() else {
            return;
        };
        let asked = object.get("layer").filter(|l| !l.is_null()).cloned();
        let front = asked.as_ref().is_some_and(in_front);
        let modal = object.get("modal") == Some(&Value::Bool(true));
        let sprites = g.holds("front");
        let widgets = sprites || g.holds("front-widget");
        let by_hand = self.launcher.front;
        let new = method == "draw.put" || (method == "draw.ext" && !object.contains_key("id"));
        let (layer, lacks) = match method {
            _ if new => (Some(by_hand.unwrap_or(front && sprites)), front && !sprites),
            "draw.update" | "draw.ext" if asked.is_some() => {
                (Some(by_hand.unwrap_or(front && sprites)), front && !sprites)
            }
            "widget.show" if by_hand.is_some() => (by_hand, false),
            "widget.show" if asked.is_some() || modal => {
                (Some((front || modal) && widgets), front && !widgets)
            }
            "widget.show" if !widgets => (Some(false), false),
            _ => (None, false),
        };
        if let Some(front) = layer {
            object.insert("layer".into(), json!(band(front)));
        }
        // The launcher's options (`ptyrc-server`, `own_params`).
        let l = &self.launcher;
        if let Some(alpha) = l.alpha {
            let asked = object.get("alpha").and_then(Value::as_f64);
            if new || (asked.is_some() && matches!(method, "draw.update" | "draw.ext")) {
                object.insert("alpha".into(), json!(alpha * asked.unwrap_or(1.0)));
            }
        }
        if let Some(prefer) = &l.prefer {
            if method == "widget.show" {
                object.insert("prefer".into(), json!(prefer));
            }
            let auto = (object.get_mut("anchor"))
                .and_then(|a| a.get_mut("auto"))
                .and_then(Value::as_object_mut);
            if let Some(auto) = auto {
                auto.insert("prefer".into(), json!(prefer));
            }
        }
        if let Some(border) = &l.border {
            let declared = method == "widget.declare" && object.contains_key("border");
            if method == "widget.show" || declared {
                object.insert("border".into(), json!(border));
            }
        }
        if lacks && by_hand.is_none() {
            let p = if method == "widget.show" {
                "front-widget"
            } else {
                "front"
            };
            self.lack(p, &[]);
        }
    }
}

/// The cell of the canvas of `sprite` at a screen cell, when it is
/// shown there, in the view of its frame (`ptyrc-server`, `canvas_at`).
fn canvas_at(scene: &Scene, frames: &Frames, sprite: u64, x: u16, y: u16) -> Option<[i64; 2]> {
    let list = scene.list();
    let s = list.iter().find(|s| s.id == sprite)?;
    let [sx, sy] = s.screen?;
    let (cx, cy) = (i64::from(x) - sx, i64::from(y) - sy);
    let [w, h] = s.size;
    let inside = (0..i64::from(w)).contains(&cx) && (0..i64::from(h)).contains(&cy);
    let in_view = match s.frame {
        Some(frame) if frame != RAW => frames.info(frame).is_some_and(|i| {
            let r = i.rect;
            i.usable && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
        }),
        _ => true,
    };
    (inside && in_view).then_some([cx, cy])
}

/// The items of `member` of the app's own (`ptyrc-server`, `own_answer`).
fn own_items(v: &mut Value, member: &str) {
    if let Some(Value::Array(items)) = v.get_mut(member) {
        items.retain(|i| i.get("owner").and_then(Value::as_u64) == Some(OWNER));
    }
}

/// Refused unless every member of `params` is in `known`
/// (`ptyrc-server`, `focus::members`).
fn members(params: &Value, known: &[&str]) -> Result<(), CallError> {
    let unknown =
        (params.as_object()).and_then(|o| o.keys().find(|k| !known.contains(&k.as_str())));
    match unknown {
        Some(key) => Err(CallError {
            hint: Some(format!("members: {}", known.join(", "))),
            ..CallError::new("bad-params", format!("unknown member: {key}"))
        }),
        None => Ok(()),
    }
}

fn widget_id(params: &Value) -> Result<u64, CallError> {
    match params.get("id") {
        None | Some(Value::Null) => Err(CallError::new("bad-params", "id: required")),
        Some(id) => id
            .as_u64()
            .ok_or_else(|| CallError::new("bad-params", "id: a widget id")),
    }
}

/// What the driver leaves pending, in a `batch`.
fn not_in_batch(method: &str) -> CallError {
    CallError::new("bad-request", format!("{method}: not allowed in a batch"))
}

/// A key of `widget.ask`: a named key that is not text; `false`: none.
fn ask_key(params: &Value, member: &str, default: &str, hint: &str) -> Result<String, CallError> {
    let named = |key: &str| {
        let ok = key_bytes(key).is_some() && key.chars().count() > 1 && key != "space";
        match ok {
            true => Ok(key.to_owned()),
            false => Err(format!("{member}: not a key: {key}")),
        }
    };
    match params.get(member) {
        None | Some(Value::Null) => named(default),
        Some(Value::Bool(false)) => Ok(String::new()),
        Some(Value::String(key)) => named(key),
        Some(_) => Err(format!("{member}: a key name, or false")),
    }
    .map_err(|e| CallError {
        hint: Some(hint.into()),
        ..CallError::new("bad-params", e)
    })
}

impl Headless {
    /// `draw.ext {name, function, params, ...}`: the sprite the
    /// extension draws, the other members laid over it
    /// (`ptyrc-server`, `rpc::draw_ext`).
    fn draw_ext_call(&mut self, params: Value) -> Result<Value, CallError> {
        let Value::Object(mut fields) = params else {
            return Err(CallError::new("bad-params", "an object is required"));
        };
        let mut take = |k: &str| fields.remove(k).and_then(|v| v.as_str().map(String::from));
        let (name, function) = (take("name"), take("function"));
        let (Some(name), Some(function)) = (name, function) else {
            return Err(CallError::new(
                "bad-params",
                "name and function are required",
            ));
        };
        let ext_params = fields.remove("params").unwrap_or_else(|| json!({}));
        let ext = (self.exts.iter().find(|e| e.name() == name))
            .ok_or_else(|| CallError::new("bad-params", format!("no draw extension {name}")))?;
        let spec = ext
            .draw(&function, &ext_params)
            .map_err(|e| CallError::new("bad-params", e))?;
        let mut merged = serde_json::to_value(&spec).map_err(bad)?;
        let update = fields.contains_key("id");
        if let Value::Object(target) = &mut merged {
            target.retain(|_, v| !v.is_null());
            if update {
                target
                    .retain(|k, _| matches!(k.as_str(), "content" | "frames" | "loop" | "effect"));
            }
            target.extend(fields);
        }
        if update {
            let id = merged["id"].clone();
            self.answer("draw.update", &merged)?;
            return Ok(json!({"id": id}));
        }
        self.answer("draw.put", &merged)
    }

    /// `widget.ask {id, quiet, escape, close}` (`ptyrc-server`,
    /// `focus::widget_ask`).
    fn widget_ask(&mut self, params: &Value) -> Result<Value, CallError> {
        members(params, &["id", "quiet", "escape", "close"])?;
        let id = widget_id(params)?;
        let quiet = match params.get("quiet") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(q)) => *q,
            Some(_) => return Err(CallError::new("bad-params", "quiet: true or false")),
        };
        let escape = ask_key(
            params,
            "escape",
            "esc",
            "a named key that is not text, as esc or f12; false: none",
        )?;
        let close = ask_key(
            params,
            "close",
            "ctrl+x",
            "a named key that is not text, as ctrl+x or f10; false: none",
        )?;
        if !close.is_empty() && close == escape {
            return Err(CallError {
                hint: Some("the close key differs from the escape key".into()),
                ..CallError::new("bad-params", format!("close: the escape key: {close}"))
            });
        }
        if self.widgets.ask(OWNER, id, quiet).is_err() {
            return Err(CallError::new("no-sprite", format!("no widget {id}")));
        }
        self.escapes.insert(id, (escape, close));
        self.settle();
        Ok(json!({"widget": self.widgets.info(id)}))
    }

    /// The app's entry of `app.list`, as the driver's (`apps::info`):
    /// no process, `pid` null.
    fn app_info(&self) -> Value {
        let sprites = (self.scene.list_on(self.vt.grid(), &self.frames).iter())
            .filter(|s| s.owner == OWNER)
            .count();
        let widgets = (self.widgets.list().iter())
            .filter(|w| w.owner == OWNER)
            .count();
        json!({
            "id": OWNER, "kind": "child", "name": self.name, "argv": [],
            "pid": null, "state": "running", "status": null, "runs": 1,
            "since": self.now as f64 / 1000.0, "conns": [OWNER], "log": null,
            "declares": self.grants.is_some(), "store": self.store,
            "foreground": false, "result": null,
            "sprites": sprites, "widgets": widgets, "frames": [], "me": true,
        })
    }

    /// `batch {calls}`: each call as alone, checked one by one; what
    /// would be pending refused (`ptyrc-server`, `Driver::batch`).
    fn batch(&mut self, params: &Value) -> Result<Value, CallError> {
        let Some(calls) = params.get("calls").and_then(Value::as_array).cloned() else {
            return Err(CallError::new(
                "bad-params",
                "calls: array of {method, params}",
            ));
        };
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            let method = call
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let result = match method {
                None => Err(CallError::new("bad-request", "missing field `method`")),
                Some(m) if m == "batch" => Err(CallError::new("bad-request", "batch inside batch")),
                Some(m) => {
                    let params = call.get("params").cloned().unwrap_or(Value::Null);
                    self.in_batch = true;
                    let r = self.call(&m, params);
                    self.in_batch = false;
                    r
                }
            };
            results.push(match result {
                Ok(value) => json!({"result": value}),
                Err(e) => {
                    let mut error = json!({"code": e.code, "message": e.message});
                    if let Some(h) = e.hint {
                        error["hint"] = json!(h);
                    }
                    json!({"error": error})
                }
            });
        }
        Ok(json!({"results": results}))
    }
}

fn shape_lines(h: &Headless, words: &Words) -> Vec<String> {
    shape(h.vt.grid(), h.vt.grid().rect(), words)
}

impl Host for Headless {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, CallError> {
        if method == "batch" && !self.in_batch {
            // The calls are told each as alone; the batch, when refused.
            let answer = self.batch(&params);
            if let Err(e) = &answer {
                self.requests.push(Request {
                    t: self.now,
                    method: method.into(),
                    params: json!({}),
                    refused: Some(e.code.clone()),
                    lowered: None,
                });
            }
            return answer;
        }
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

    fn assets(&self) -> Option<&Assets> {
        self.assets.as_ref()
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
    /// Clock, zone, storage, files, probes as given (`hostfn`); each
    /// file under its option in `args`, as a handle.
    pub host: HostFns,
    pub assets: Option<Assets>,
    pub launcher: Launcher,
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
            host: HostFns {
                wall0: DEFAULT_CLOCK,
                ..HostFns::default()
            },
            assets: None,
            launcher: Launcher::default(),
        }
    }
}

/// `--clock` by default: 2000-01-01T00:00:00Z.
pub const DEFAULT_CLOCK: i64 = 946_684_800_000;

/// What a headless run gives back.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// `(ms, rows)`.
    pub screens: Vec<(u64, Vec<String>)>,
    /// Per screen, the cells not of the default style.
    pub styles: Vec<Vec<StyleRun>>,
    /// What the run did that the app did not ask: one line each.
    pub notes: Vec<String>,
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
    /// `app.result`: `(value, status)`, the last.
    pub result: Option<(Value, u8)>,
    /// The storage at the end.
    pub storage: BTreeMap<String, String>,
    /// The files the app wrote, in memory.
    pub written: Vec<Named>,
}

impl Lack {
    /// One line: refused or answered lowered, and why.
    pub fn line(&self) -> String {
        let (how, p) = match self.refused {
            true => ("refused", &self.permission),
            false => ("lowered", &self.permission),
        };
        match (self.declared, self.refused) {
            (true, _) => format!("{how}: {p}, declared and answered no"),
            (false, true) => format!(
                "{how}: {p}, not declared; the manifest would ask for it: {}",
                grants::manifest_line(p, &self.words)
            ),
            (false, false) => format!(
                "{how}: {p}, not declared: fine if intended; else the manifest would ask \
                 for it: {}",
                grants::manifest_line(p, &self.words)
            ),
        }
    }
}

/// Said once under the lacks not declared.
pub const LESS: &str = "note: declaring less is good: the user is asked less, and a lowered \
    answer is an answer";

/// A run of cells of one style: `(y, x, w, style)`; `style` as
/// [`style_name`] gives it.
pub type StyleRun = (u16, u16, u16, String);

/// A style as words: `fg=N`, `fg=#rrggbb`, `bg=...`, the attributes
/// (`bold`, `dim`, `italic`, `underline`, `blink`, `inverse`, `hidden`,
/// `strike`, `overline`); `""` the default.
pub fn style_name(s: &Style) -> String {
    let color = |c: Color| match c {
        Color::Default => None,
        Color::Indexed(n) => Some(n.to_string()),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    };
    let mut parts = Vec::new();
    if let Some(c) = color(s.fg) {
        parts.push(format!("fg={c}"));
    }
    if let Some(c) = color(s.bg) {
        parts.push(format!("bg={c}"));
    }
    for (bit, name) in [
        (Attrs::BOLD, "bold"),
        (Attrs::DIM, "dim"),
        (Attrs::ITALIC, "italic"),
        (Attrs::UNDERLINE, "underline"),
        (Attrs::BLINK, "blink"),
        (Attrs::INVERSE, "inverse"),
        (Attrs::HIDDEN, "hidden"),
        (Attrs::STRIKE, "strike"),
        (Attrs::OVERLINE, "overline"),
    ] {
        if s.attrs.0 & bit != 0 {
            parts.push(name.to_owned());
        }
    }
    parts.join(" ")
}

/// The runs of `grid` not of the default style, row by row.
pub fn style_runs(grid: &Grid) -> Vec<StyleRun> {
    let mut out: Vec<StyleRun> = Vec::new();
    for y in 0..grid.rows() {
        let mut run: Option<StyleRun> = None;
        for x in 0..grid.cols() {
            let name = style_name(&grid.cell(x, y).style);
            match &mut run {
                Some(r) if r.3 == name => r.2 += 1,
                _ => {
                    out.extend(run.take().filter(|r| !r.3.is_empty()));
                    run = Some((y, x, 1, name));
                }
            }
        }
        out.extend(run.filter(|r| !r.3.is_empty()));
    }
    out
}

/// The second grid of `--styles`: a letter per style, in order of
/// first cell, and the legend.
fn style_grid(runs: &[StyleRun]) -> Vec<String> {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut names: Vec<&str> = Vec::new();
    let mut rows: Vec<Vec<u8>> = Vec::new();
    for (y, x, w, name) in runs {
        let i = match names.iter().position(|n| n == name) {
            Some(i) => i,
            None => {
                names.push(name);
                names.len() - 1
            }
        };
        let letter = LETTERS.get(i).copied().unwrap_or(b'?');
        let (y, x, w) = (*y as usize, *x as usize, *w as usize);
        if rows.len() <= y {
            rows.resize(y + 1, Vec::new());
        }
        let row = &mut rows[y];
        if row.len() < x + w {
            row.resize(x + w, b' ');
        }
        row[x..x + w].fill(letter);
    }
    let mut out: Vec<String> = rows
        .into_iter()
        .map(|r| String::from_utf8_lossy(&r).into_owned())
        .collect();
    for (i, name) in names.iter().enumerate() {
        let letter = LETTERS.get(i).map_or('?', |b| *b as char);
        out.push(format!("{letter}: {name}"));
    }
    out
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

    /// The report as one JSON value: `screens` (`t`, `rows`, `styles`:
    /// `[y, x, w, style]`), `refused`, `lowered`, `dropped`, `notes`,
    /// `lacks`, `limits`, `end`, `result` (`{value, status}` or null),
    /// `storage` (key: value), `files` (written: `option`, `name`,
    /// `size`, `text` or `base64`).
    pub fn to_json(&self) -> Value {
        let screens: Vec<Value> = self
            .screens
            .iter()
            .enumerate()
            .map(|(i, (t, rows))| {
                let last = rows
                    .iter()
                    .rposition(|r| !r.is_empty())
                    .map_or(0, |i| i + 1);
                let styles = self.styles.get(i).map(Vec::as_slice).unwrap_or_default();
                json!({"t": t, "rows": &rows[..last], "styles": styles})
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
                json!({"permission": l.permission, "declared": l.declared,
                       "refused": l.refused, "words": l.words, "line": l.line()})
            })
            .collect();
        let files: Vec<Value> = (self.written.iter())
            .map(|f| {
                let bytes = f.content.bytes.as_deref().unwrap_or_default();
                let mut v = json!({"option": f.option, "name": f.name, "size": bytes.len()});
                match std::str::from_utf8(bytes) {
                    Ok(text) => v["text"] = json!(text),
                    Err(_) => v["base64"] = json!(crate::assets::base64_encode(bytes)),
                }
                v
            })
            .collect();
        json!({
            "screens": screens,
            "refused": refused,
            "lowered": lowered,
            "dropped": dropped,
            "notes": self.notes,
            "lacks": lacks,
            "limits": self.met,
            "end": self.end.as_ref().map(|e| e.to_string()),
            "result": self.result.as_ref().map(|(v, s)| json!({"value": v, "status": s})),
            "storage": self.storage,
            "files": files,
        })
    }
}

impl fmt::Display for Report {
    /// The screens, the requests refused, the limits met, the end.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write(f, false)
    }
}

impl Report {
    /// The text report; `styles`: under each screen, its styles.
    pub fn text(&self, styles: bool) -> String {
        struct With<'a>(&'a Report, bool);
        impl fmt::Display for With<'_> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.0.write(f, self.1)
            }
        }
        With(self, styles).to_string()
    }

    fn write(&self, f: &mut fmt::Formatter, styles: bool) -> fmt::Result {
        for (i, (t, rows)) in self.screens.iter().enumerate() {
            writeln!(f, "--- {t} ms")?;
            let last = rows
                .iter()
                .rposition(|r| !r.is_empty())
                .map_or(0, |i| i + 1);
            for row in &rows[..last] {
                writeln!(f, "{row}")?;
            }
            let runs = self.styles.get(i).map(Vec::as_slice).unwrap_or_default();
            if styles && !runs.is_empty() {
                writeln!(f, "--- {t} ms, styles")?;
                for line in style_grid(runs) {
                    writeln!(f, "{line}")?;
                }
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
        for note in &self.notes {
            writeln!(f, "note: {note}")?;
        }
        for lack in &self.lacks {
            writeln!(f, "{}", lack.line())?;
        }
        if self.lacks.iter().any(|l| !l.declared) {
            writeln!(f, "{LESS}")?;
        }
        if let Some((value, status)) = &self.result {
            let value = match value {
                Value::String(s) => s.escape_debug().to_string(),
                v => v.to_string(),
            };
            writeln!(f, "result: {value} (status {status})")?;
        }
        if !self.storage.is_empty() {
            let keys: Vec<String> = (self.storage.iter())
                .map(|(k, v)| format!("{k} {} bytes", v.len()))
                .collect();
            writeln!(f, "storage: {}", keys.join(", "))?;
        }
        for file in &self.written {
            let size = file.content.bytes.as_ref().map_or(0, Vec::len);
            writeln!(f, "files: {} written, {size} bytes", file.option)?;
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

/// Width of a line of a screen file, its escape sequences left out.
fn line_width(line: &str) -> usize {
    let mut width = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: to a final byte.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: to BEL or ST.
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            c if c.is_control() => {}
            c => width += ptyrc_vt::char_width(c) as usize,
        }
    }
    width
}

/// The note on a screen file that does not fit `(cols, rows)`: it is
/// wrapped and scrolled as a terminal does.
fn fit_note(t: u64, text: &str, (cols, rows): (u16, u16)) -> Option<String> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    let (mut wide, mut lines) = (0, 0usize);
    for line in text.split('\n') {
        let w = line_width(line);
        if w > usize::from(cols) {
            wide += 1;
        }
        lines += w.max(1).div_ceil(usize::from(cols.max(1)));
    }
    let past = lines.saturating_sub(usize::from(rows));
    let mut said = Vec::new();
    if wide > 0 {
        said.push(format!("{wide} lines wider than {cols} columns, wrapped"));
    }
    if past > 0 {
        said.push(format!("{past} rows past {rows}, scrolled off the top"));
    }
    (!said.is_empty()).then(|| {
        format!(
            "the screen at {t} ms: {}, as a terminal does",
            said.join("; ")
        )
    })
}

/// `args` with each file named under its option: `{"file": "f0",
/// "name": "TODO.md"}`; an option given twice, a list of them.
fn with_handles(args: &Value, files: &[Named]) -> Value {
    let mut args = args.clone();
    if !args.is_object() {
        args = json!({});
    }
    for (i, f) in files.iter().enumerate() {
        let handle = json!({"file": format!("f{i}"), "name": f.name});
        let slot = &mut args[f.option.as_str()];
        // Named already (`ptyrc dev test`, the launcher's reading).
        let given = |v: &Value| v.get("file").and_then(Value::as_str) == Some(&format!("f{i}"));
        let named = match &*slot {
            Value::Array(list) => list.iter().any(given),
            v => given(v),
        };
        if named {
            continue;
        }
        match slot {
            Value::Array(list) => list.push(handle),
            Value::Object(o) if o.contains_key("file") => {
                *slot = json!([slot.clone(), handle]);
            }
            _ => *slot = handle,
        }
    }
    args
}

/// Run a module headless. `Err`: it did not load.
pub fn run(wasm: &[u8], options: &Options) -> Result<Report, Error> {
    let (size, tape) = match &options.under {
        Under::Tape(t) => ((u32::from(t.size.0), u32::from(t.size.1)), &t.records[..]),
        _ => (options.size, &[][..]),
    };
    let mut host = Headless::new(size, options.seed);
    host.set_grants(options.grants.clone());
    let manifest = options.args.get(crate::manifest::MEMBER);
    if let Some(id) = manifest.and_then(|m| m.get("id")).and_then(Value::as_str) {
        host.name = id.to_owned();
    }
    host.fns = options.host.clone();
    host.assets = options.assets.clone();
    host.launcher = options.launcher.clone();
    let args = with_handles(&options.args, &options.host.files);
    let mut texts: Vec<(u64, &str)> = match &options.under {
        Under::Text(text) => vec![(0, text.as_str())],
        Under::Screens(s) => s.iter().map(|(t, text)| (*t, text.as_str())).collect(),
        _ => Vec::new(),
    };
    texts.sort_by_key(|s| s.0);
    let mut notes = Vec::new();
    let fits = (host.vt.cols(), host.vt.rows());
    for (t, text) in &texts {
        notes.extend(fit_note(*t, text, fits));
    }
    let mut texts = texts
        .into_iter()
        .filter(|s| s.0 <= options.duration_ms)
        .peekable();
    while let Some((_, text)) = texts.next_if(|s| s.0 == 0) {
        host.feed(b"\x1b[0m\x1b[2J");
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
    let mut styles = Vec::new();
    let mut ticks = 0;

    let mut done = runner.init(&args, 0);
    // A step after `init`, as the driver's: what it tells (`idle` of a
    // blank screen) is heard at 0.
    if done.is_ok() {
        runner.host_mut().settle();
        for (topic, data) in runner.host_mut().take_events() {
            if done.is_ok() {
                done = runner.event(&topic, &data, 0);
            }
        }
    }
    let mut period = runner.period();
    let mut next_tick = (period > 0).then_some(u64::from(period));
    // At one time: the tape, the events, the tick, the screen.
    while done.is_ok() {
        let now = [
            tape.peek().map(|r| r.0 / 1000).filter(|t| *t <= end),
            texts.peek().map(|s| s.0),
            events.peek().map(|e| e.0),
            inject.peek().map(|e| e.0),
            next_tick.filter(|t| *t <= end),
            at.peek().copied(),
            // The scene steps on its own clock, as the driver's: the
            // screen at T is the same whatever else is asked.
            runner.host().next_wake().filter(|t| *t <= end),
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
        while let Some((_, text)) = texts.next_if(|s| s.0 <= now) {
            let host = runner.host_mut();
            host.feed(b"\x1b[0m\x1b[2J");
            host.feed(&screen_bytes(text));
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
        // What the step told (`idle`, `widgets`): heard at once.
        let told = runner.host_mut().take_events();
        let again = !told.is_empty();
        for (topic, data) in told {
            if done.is_ok() {
                done = runner.event(&topic, &data, now);
            }
        }
        if again {
            runner.host_mut().settle();
        }
        if at.next_if_eq(&now).is_some() {
            let (rows, runs) = runner.host().shot();
            screens.push((now, rows));
            styles.push(runs);
        }
    }
    // Ended early: the screens left show what stays.
    for t in at {
        let host = runner.host_mut();
        host.set_now(t);
        host.settle();
        let (rows, runs) = host.shot();
        screens.push((t, rows));
        styles.push(runs);
    }
    let met = runner.met().iter().map(|m| m.to_string()).collect();
    let host = runner.host_mut();
    for p in &host.fns.unanswered {
        notes.push(format!(
            "probe.{p}: held, no --probe {p}=FILE.json: answered as not held"
        ));
    }
    let fns = std::mem::take(&mut host.fns);
    Ok(Report {
        screens,
        styles,
        notes,
        result: fns.result,
        storage: fns.storage,
        written: fns.files.into_iter().filter(|f| f.written).collect(),
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

    /// `screen.cells`, `frame.text`, `frame.heat`: whole with `text`,
    /// cut below it as the driver cuts them (`lower.rs`).
    #[test]
    fn cells_text_and_heat_lowered() {
        let mut h = host(&["text"], &[]);
        h.feed(b"\x1b[1;31mred\x1b[0m");
        let c = h.call("screen.cells", json!({})).unwrap();
        assert_eq!(
            c["rows"][1],
            json!([{"t": "world", }, {"t": "red", "fg": "red", "bold": true}, {"t": "            "}])
        );
        let c = h
            .call("screen.cells", json!({"rect": [6, 0, 4, 1]}))
            .unwrap();
        assert_eq!(
            (&c["rows"], &c["rect"]),
            (&json!([[{"t": "fish"}]]), &json!([6, 0, 4, 1]))
        );
        let t = h.call("frame.text", json!({})).unwrap();
        assert_eq!(
            (&t["lines"][0], &t["y0"]),
            (&json!("hello fish"), &json!(0))
        );
        let heat = h.call("frame.heat", json!({})).unwrap();
        assert_eq!(heat["rows"].as_array().unwrap().len(), 3);
        assert!(heat.get("degree").is_none());
        // `shape` and the word `fish`.
        let mut h = host(&["shape", "words"], &[]);
        let c = h.call("screen.cells", json!({})).unwrap();
        assert_eq!(
            c["rows"],
            json!([[{"t": "##### fish"}], [{"t": "#####"}], []])
        );
        assert_eq!(c["degree"], "words");
        let t = h.call("frame.text", json!({})).unwrap();
        assert_eq!(t["lines"][0], "##### fish");
        assert_eq!(t["degree"], "words");
        // Below `shape`: blank, cold, nothing remembered.
        let mut h = host(&["shape"], &["shape"]);
        let c = h.call("screen.cells", json!({})).unwrap();
        assert_eq!(c["rows"], json!([[], [], []]));
        let t = h.call("frame.text", json!({})).unwrap();
        assert_eq!(t, json!({"lines": [], "y0": 0, "degree": "none"}));
        let heat = h.call("frame.heat", json!({})).unwrap();
        let nines = heat["rows"].as_array().unwrap();
        assert!(
            nines
                .iter()
                .all(|r| r.as_str().unwrap().chars().all(|c| c == '9'))
        );
        assert_eq!(heat["degree"], "none");
    }

    #[test]
    fn known_unknown_and_served() {
        let mut h = host(&["text"], &[]);
        let e = Headless::new((4, 1), 1)
            .call("session.colors", json!({}))
            .unwrap_err();
        assert_eq!(e.code, "headless");
        assert!(e.hint.unwrap().contains("ptyrc dev run"));
        let e = h.call("screen.txt", json!({})).unwrap_err();
        assert_eq!(e.code, "unknown-method");
        assert_eq!(e.message, "unknown method: screen.txt");
        assert!(e.hint.unwrap().starts_with("did you mean screen.text? "));
        let e = h.call("unsubscribe", json!({})).unwrap_err();
        assert_eq!(e.code, "unknown-method");
        let e = h
            .call("subscribe", json!({"topics": ["sizes"]}))
            .unwrap_err();
        assert_eq!(
            (e.code.as_str(), e.message.as_str()),
            ("bad-params", "no such topic: sizes")
        );
        for m in SERVED {
            assert!(grants::METHODS.contains(m), "{m}");
            let e = h.call(m, json!({})).err().map(|e| e.code);
            assert!(
                !matches!(e.as_deref(), Some("headless" | "unknown-method")),
                "{m}"
            );
        }
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

    /// As the driver: every row of the rect, an empty one `""`.
    #[test]
    fn the_shape_keeps_every_row() {
        let mut h = host(&["shape"], &[]);
        let s = h.call("screen.shape", json!({})).unwrap();
        assert_eq!(s["lines"], json!(["##### ####", "#####", ""]));
        h.feed(b"\x1b[2J");
        let s = h.call("screen.shape", json!({})).unwrap();
        assert_eq!(s["lines"], json!(["", "", ""]));
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
        assert_eq!(
            h.lacks[0].line(),
            "lowered: shape, declared and answered no"
        );
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

    /// `screen.find`, `screen.wait` below `text`: the declared words
    /// only, whole, case ignored.
    #[test]
    fn find_and_wait_lowered() {
        let mut h = host(&["shape", "words"], &[]);
        let f = h.call("screen.find", json!({"pattern": "FISH"})).unwrap();
        assert_eq!(
            (f["matches"][0]["x"].clone(), f["degree"].clone()),
            (json!(6), json!("words"))
        );
        let f = h.call("screen.find", json!({"pattern": "hello"})).unwrap();
        assert_eq!(f["matches"], json!([]));
        assert_eq!(h.lacks[0].permission, "words");
        assert!(
            h.lacks[0].line().contains("fine if intended"),
            "{}",
            h.lacks[0].line()
        );
        let w = h.call("screen.wait", json!({"pattern": "fish"})).unwrap();
        assert_eq!(w["matched"], true);
        let w = h.call("screen.wait", json!({"pattern": "worl"})).unwrap();
        assert_eq!(w["matched"], false);
        let mut h = host(&["text"], &[]);
        let f = h
            .call("screen.find", json!({"pattern": "wor", "regex": true}))
            .unwrap();
        assert_eq!(f["matches"][0]["y"], 1);
        let w = h.call("screen.wait", json!({"pattern": "nope"})).unwrap();
        assert_eq!(w, json!({"matched": false}));
        let c = h.call("screen.cursor", json!({})).unwrap();
        assert_eq!((c["x"].clone(), c["y"].clone()), (json!(5), json!(1)));
        assert!(h.call("frame.list", json!({})).unwrap()["frames"][0].is_object());
    }

    /// The user's word found at any degree, shown from `shape`; the
    /// user's pattern opens `screen.find`, `screen.wait` for itself
    /// alone (`ptyrc-server`, `the_users_pattern_opens_itself_alone`).
    #[test]
    fn the_users_word_and_pattern() {
        for declared in [&[][..], &["layout"], &["shape"]] {
            let mut h = Headless::new((20, 3), 1);
            h.feed(b"hello fish\r\n12 ms TODO:");
            let mut g = Grants::new(declared, &[], &[]);
            g.user_words = vec!["Fish".into()];
            g.patterns = vec![("TODO:".into(), false), (r"\d+ ms".into(), true)];
            h.set_grants(Some(g));
            let f = h.call("screen.find", json!({"pattern": "fish"})).unwrap();
            assert_eq!(f["matches"][0]["x"], 6, "{declared:?}");
            let f = h.call("screen.find", json!({"pattern": "TODO:"})).unwrap();
            assert_eq!(f["matches"][0]["x"], 6, "{declared:?}");
            let regex = json!({"pattern": r"\d+ ms", "regex": true});
            let w = h.call("screen.wait", regex).unwrap();
            assert_eq!(w["matched"], true, "{declared:?}");
            for other in [
                json!({"pattern": "TODO:", "regex": true}),
                json!({"pattern": r"\d+ ms"}),
                json!({"pattern": "TODO"}),
                json!({"pattern": ".*", "regex": true}),
                json!({"pattern": "hello"}),
            ] {
                let f = h.call("screen.find", other.clone()).unwrap();
                assert_eq!(f["matches"], json!([]), "{other}");
            }
            let t = h.call("screen.text", json!({})).unwrap();
            let shown = match declared == ["shape"] {
                true => "##### fish",
                false => "",
            };
            assert_eq!(t["lines"][0], shown, "{declared:?}");
        }
    }

    /// `--front`, `--alpha`, `--prefer`, `--border`: on the app's
    /// requests, as `own_params` sets them.
    #[test]
    fn the_launchers_options() {
        let mut h = host(&[], &[]);
        h.launcher = Launcher {
            front: Some(true),
            prefer: Some("top".into()),
            alpha: Some(0.5),
            border: Some("rounded".into()),
        };
        let mut put = json!({"content": {"rows": ["X"]}, "alpha": 0.5,
                             "anchor": {"auto": {"w": 1, "h": 1}}});
        h.own_layer(&Grants::new(&[] as &[&str], &[], &[]), "draw.put", &mut put);
        assert_eq!(put["layer"], band(true));
        assert_eq!(put["alpha"], 0.25);
        assert_eq!(put["anchor"]["auto"]["prefer"], "top");
        let mut show = json!({"tag": "w"});
        let g = Grants::new(&[] as &[&str], &[], &[]);
        h.own_layer(&g, "widget.show", &mut show);
        assert_eq!(
            (&show["layer"], &show["prefer"], &show["border"]),
            (&json!(band(true)), &json!("top"), &json!("rounded"))
        );
        assert!(h.lacks.is_empty(), "the user's hand: nothing lacks");
        let put = json!({"content": {"rows": ["X"]}, "anchor": {"at": [0, 0], "space": "view"}});
        h.call("draw.put", put).unwrap();
        h.settle();
        assert_eq!(h.screen()[0], "Xello fish");
    }

    /// In front only with `front`; else behind, told as lowered.
    #[test]
    fn front_only_held() {
        let put = json!({"content": {"rows": ["X"]}, "layer": "front",
                         "anchor": {"at": [0, 0], "space": "view"}});
        let mut h = host(&[], &[]);
        h.call("draw.put", put.clone()).unwrap();
        h.settle();
        assert_eq!(h.screen()[0], "hello fish");
        assert_eq!(h.lacks[0].permission, "front");
        let mut h = host(&["front"], &[]);
        h.call("draw.put", put).unwrap();
        h.settle();
        assert_eq!(h.screen()[0], "Xello fish");
    }

    #[test]
    fn styles_and_fit() {
        let mut h = Headless::new((10, 2), 1);
        h.feed(b"a\x1b[1;31mbc\x1b[0m d");
        let (_, runs) = h.shot();
        assert_eq!(runs, [(0, 1, 2, "fg=1 bold".to_string())]);
        assert_eq!(style_grid(&runs), [" aa", "a: fg=1 bold"]);
        assert_eq!(fit_note(0, "abc\n", (10, 2)), None);
        let note = fit_note(5, "0123456789ab\n\x1b[1mx\x1b[0m\n", (10, 2)).unwrap();
        assert_eq!(
            note,
            "the screen at 5 ms: 1 lines wider than 10 columns, wrapped; 1 rows past 2, \
             scrolled off the top, as a terminal does"
        );
    }

    #[test]
    fn text_events_on_a_change() {
        let mut h = host(&["shape"], &[]);
        h.call("subscribe", json!({"topics": ["text"]})).unwrap();
        h.feed(b"\x1b[2;1Hworms");
        let events = h.take_events();
        assert_eq!(events[0].0, "text");
        assert_eq!(events[0].1["rows"], json!([1]));
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
    #[test]
    fn a_batch_as_the_driver() {
        let mut h = host(&["shape"], &[]);
        let put = json!({"content": {"rows": ["x"]}, "anchor": {"at": [0, 0]}});
        let b = h
            .call(
                "batch",
                json!({"calls": [
                    {"method": "draw.put", "params": put},
                    {"method": "draw.list"},
                    {"method": "screen.wait", "params": {"stable_ms": 60000}},
                    {"method": "screen.wait", "params": {"stable_ms": 0}},
                    {"method": "storage.get", "params": {"key": "k"}},
                    {"method": "batch", "params": {"calls": []}},
                    {"params": {}},
                ]}),
            )
            .unwrap();
        let r = &b["results"];
        let id = r[0]["result"]["id"].as_u64().unwrap();
        assert_eq!(r[1]["result"]["sprites"][0]["id"], id);
        assert_eq!(r[2]["error"]["code"], "bad-request");
        assert_eq!(
            r[2]["error"]["message"],
            "screen.wait: not allowed in a batch"
        );
        assert_eq!(r[3]["result"]["matched"], true);
        assert_eq!(r[4]["error"]["code"], "denied");
        assert_eq!(r[5]["error"]["message"], "batch inside batch");
        assert_eq!(r[6]["error"]["code"], "bad-request");
        // Each call told as alone; the batch is not.
        let told: Vec<&str> = h.requests.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(
            told,
            [
                "draw.put",
                "draw.list",
                "screen.wait",
                "screen.wait",
                "storage.get"
            ]
        );
        let e = h.call("batch", json!({})).unwrap_err();
        assert_eq!(e.code, "bad-params");
        // Not in a batch, a wait not met is a timeout.
        let w = h.call("screen.wait", json!({"stable_ms": 60000})).unwrap();
        assert_eq!(w["matched"], false);
    }

    #[test]
    fn lists_exts_and_the_ask() {
        let mut h = host(&[], &[]);
        let exts = h.call("draw.exts", json!({})).unwrap();
        assert!(exts["effects"].as_array().unwrap().contains(&json!("sand")));
        let names: Vec<&str> = (exts["exts"].as_array().unwrap().iter())
            .filter_map(|e| e["name"].as_str())
            .collect();
        assert!(names.contains(&"shapes"), "{names:?}");
        let e = h
            .call("draw.ext", json!({"name": "nope", "function": "box"}))
            .unwrap_err();
        assert_eq!(e.message, "no draw extension nope");
        let b = h.call("widget.borders", json!({})).unwrap();
        assert!(!b["borders"].as_array().unwrap().is_empty());
        let w = json!({"widgets": [{"tag": "w", "name": "small", "size": [5, 1],
                                     "content": {"rows": ["hello"]}}]});
        h.call("widget.declare", w).unwrap();
        let w = h.call("widget.show", json!({"tag": "w"})).unwrap();
        let id = w["widget"]["id"].as_u64().unwrap();
        // Behind the text, without `front-widget`.
        let list = h.call("widget.list", json!({})).unwrap();
        assert_eq!(list["widgets"][0]["id"], id);
        let e = h
            .call("widget.ask", json!({"id": id, "escape": "a"}))
            .unwrap_err();
        assert_eq!(e.message, "escape: not a key: a");
        let e = h
            .call("widget.ask", json!({"id": id, "close": "esc"}))
            .unwrap_err();
        assert_eq!(e.message, "close: the escape key: esc");
        let e = h.call("widget.ask", json!({"id": id, "x": 1})).unwrap_err();
        assert_eq!(e.message, "unknown member: x");
        let a = h.call("widget.ask", json!({"id": id})).unwrap();
        assert!(!a["widget"]["asks"].is_null(), "{a}");
        assert!(h.call("widget.release", json!({"id": id})).is_ok());
        let e = h.call("widget.release", json!({"id": 999})).unwrap_err();
        assert_eq!(e.code, "no-sprite");
        let apps = h.call("app.list", json!({})).unwrap();
        assert_eq!(apps["apps"][0]["widgets"], 1);
        assert_eq!(apps["apps"][0]["me"], true);
    }
    #[test]
    fn the_hands_on_the_scene() {
        let mut h = host(&["shape"], &[]);
        h.call("subscribe", json!({"topics": ["hover", "mouse", "input"]}))
            .unwrap();
        let put = json!({"content": {"rows": ["xx"]}, "anchor": {"at": [3, 1]}});
        let id = h.call("draw.put", put).unwrap()["id"].as_u64().unwrap();
        h.settle();
        // Behind the text, not a widget: not heard without the permission.
        h.inject(&Inject::Click(4, 1));
        assert!(h.take_events().is_empty());
        assert_eq!(h.dropped.last().unwrap().1, "click 4,1: not held");
        let mut h = host(&["shape", "pointer", "clicks"], &[]);
        h.call("subscribe", json!({"topics": ["hover", "mouse", "input"]}))
            .unwrap();
        let put = json!({"content": {"rows": ["xx"]}, "anchor": {"at": [3, 1]}});
        assert_eq!(h.call("draw.put", put).unwrap()["id"], id);
        h.settle();
        h.inject(&Inject::Pointer(4, 1));
        h.inject(&Inject::Pointer(0, 0));
        h.inject(&Inject::Click(4, 1));
        h.inject(&Inject::Click(0, 2));
        let e = h.take_events();
        assert_eq!(
            e[0].1,
            json!({"sprite": id, "at": [1, 0], "x": 4, "y": 1, "frame": 1,
                                  "pos": [4, 1], "mods": []})
        );
        assert_eq!(e[1].1["sprite"], Value::Null);
        assert_eq!(
            (e[2].0.as_str(), &e[2].1["action"]),
            ("mouse", &json!("press"))
        );
        assert_eq!(e[3].1["action"], "release");
        assert_eq!(e[3].1["button"], "left");
        assert_eq!(e.len(), 4);
        assert_eq!(
            h.dropped.last().unwrap().1,
            "click 0,2: on no sprite of the app"
        );
        // The focus, the keys, the paste, the escape.
        let w = json!({"widgets": [{"tag": "w", "name": "small", "size": [5, 1],
                                     "content": {"rows": ["hello"]}}]});
        h.call("widget.declare", w).unwrap();
        h.inject(&Inject::Focus("w".into()));
        assert!(
            h.dropped
                .last()
                .unwrap()
                .1
                .starts_with("focus w: no widget")
        );
        let wid = h.call("widget.show", json!({"tag": "w"})).unwrap()["widget"]["id"].clone();
        h.call("widget.ask", json!({"id": wid})).unwrap();
        h.inject(&Inject::Focus("w".into()));
        h.inject(&Inject::Key("a".into()));
        h.inject(&Inject::Paste("pasted".into()));
        h.inject(&Inject::Key("esc".into()));
        h.inject(&Inject::Key("b".into()));
        let e = h.take_events();
        assert_eq!(e[0].1, json!({"key": "a", "bytes": "YQ==", "widget": wid}));
        assert_eq!(e[1].1, json!({"widget": wid, "paste": "pasted"}));
        assert_eq!(e[2].1, json!({"widget": wid, "released": "escape"}));
        assert_eq!(e.len(), 3);
    }

    #[test]
    fn the_pointer_told_to_an_effect() {
        let mut h = host(&["shape"], &[]);
        let effect = json!({"name": "sand", "params": {"rate": 1, "pointer": true, "bomb": true}});
        let put = json!({"effect": effect, "size": [10, 5], "anchor": {"at": [2, 1]}});
        let id = h.call("draw.put", put).unwrap()["id"].as_u64().unwrap();
        h.settle();
        assert_eq!(h.nozzles.told(id), Some((None, false)));
        // No permission: the app is told nothing; nothing is dropped.
        h.inject(&Inject::Pointer(3, 2));
        assert_eq!(h.nozzles.told(id), Some((Some([1, 1]), false)));
        h.inject(&Inject::Pointer(0, 0));
        assert_eq!(h.nozzles.told(id), Some((None, false)));
        h.set_now(100);
        h.inject(&Inject::Press(4, 3));
        assert_eq!(h.nozzles.told(id), Some((None, true)));
        h.set_now(400);
        h.inject(&Inject::Release(6, 3));
        assert_eq!(h.nozzles.told(id), Some((None, false)));
        assert!(h.dropped.is_empty(), "{:?}", h.dropped);
        // Not taken from the program without `clicks-grab`.
        assert!(h.lacks.iter().any(|l| l.permission == "clicks-grab"));
        assert!(h.take_events().is_empty());
        // `bomb` off, `pointer` kept; off both: forgotten.
        let off = json!({"id": id, "effect": {"name": "sand", "params": {"bomb": false}}});
        h.call("draw.update", off).unwrap();
        h.inject(&Inject::Press(4, 3));
        assert_eq!(h.nozzles.told(id), Some((None, false)));
        let off = json!({"id": id, "effect": {"name": "sand", "params": {"pointer": false}}});
        h.call("draw.update", off).unwrap();
        assert_eq!(h.nozzles.told(id), None);
    }

    #[test]
    fn a_press_held_and_released_off_its_sprite() {
        let mut h = host(&["shape", "clicks"], &[]);
        h.call("subscribe", json!({"topics": ["mouse"]})).unwrap();
        let put = json!({"content": {"rows": ["xx"]}, "anchor": {"at": [3, 1]}});
        let id = h.call("draw.put", put).unwrap()["id"].as_u64().unwrap();
        h.settle();
        h.inject(&Inject::Press(4, 1));
        h.inject(&Inject::Release(9, 2));
        let e = h.take_events();
        assert_eq!(
            e[0].1,
            json!({"sprite": id, "at": [1, 0], "x": 4, "y": 1, "frame": 1, "pos": [4, 1],
                   "mods": [], "button": "left", "action": "press"})
        );
        assert_eq!(
            e[1].1,
            json!({"sprite": id, "at": null, "x": 9, "y": 2, "frame": null, "pos": null,
                   "mods": [], "button": "left", "action": "release"})
        );
        // A release with no press on a sprite: nobody's.
        h.inject(&Inject::Release(9, 2));
        assert!(h.take_events().is_empty());
        assert_eq!(
            h.dropped.last().unwrap().1,
            "release 9,2: on no sprite of the app"
        );
    }

    #[test]
    fn where_an_anchor_landed_and_own_sprites_alone() {
        let at = |held: &[&str]| {
            let mut h = Headless::new((20, 5), 1);
            h.feed(b"a fish and a cat");
            h.set_grants(Some(Grants::new(held, &[], &["fish"])));
            h
        };
        let mut h = at(&["shape", "words"]);
        let put = |text: &str| json!({"content": {"rows": ["^"]}, "anchor": {"text": text, "side": "below"}});
        h.call("draw.put", put("cat")).unwrap();
        h.call("draw.put", put("fish")).unwrap();
        h.settle();
        let list = h.call("draw.list", json!({})).unwrap();
        let s = list["sprites"].as_array().unwrap();
        assert_eq!(
            (s[0]["screen"].clone(), s[0]["visible"].clone()),
            (Value::Null, json!(false))
        );
        assert!(s[1]["screen"].is_array(), "{}", s[1]);
        assert!(h.lacks.iter().any(|l| l.permission == "text"));
        // `text` held: the sprites of the app alone, composed.
        let mut h = at(&["shape", "words", "text"]);
        h.call("draw.put", put("cat")).unwrap();
        let other = serde_json::from_value(json!({"content": {"rows": ["OTHER"]},
                                                   "anchor": {"at": [0, 3]}}))
        .unwrap();
        h.scene.add(OWNER + 1, other, h.at()).unwrap();
        h.settle();
        let shown = |h: &mut Headless, sprites: bool| {
            let t = h.call("screen.text", json!({"sprites": sprites})).unwrap();
            let c = h.call("screen.cells", json!({"sprites": sprites})).unwrap();
            (
                t["lines"][1].clone(),
                t["lines"][3].clone(),
                c["rows"][1].clone(),
            )
        };
        let (fish, other, cells) = shown(&mut h, true);
        assert_eq!((fish, other), (json!("              ^"), json!("")));
        assert!(cells.to_string().contains('^'), "{cells}");
        assert_eq!(shown(&mut h, false).0, json!(""));
        assert!(h.screen()[3].starts_with("OTHER"));
    }
}
