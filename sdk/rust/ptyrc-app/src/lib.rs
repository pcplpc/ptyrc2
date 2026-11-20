// SPDX-License-Identifier: GPL-3.0-or-later
//! A ptyrc store app in Rust: a guest of ABI 1 for
//! `wasm32-unknown-unknown`. See `README.md`.
//!
//! ```ignore
//! use ptyrc_app::{App, Canvas, Value, period};
//! struct Hello(Canvas);
//! impl App for Hello {
//!     fn init(_args: Value) -> Hello { period(100); Hello(Canvas::new(5, 1).unwrap()) }
//!     fn tick(&mut self, _now_ms: u64) { self.0.text(0, 0, "hi"); self.0.present(&Value::Null); }
//! }
//! ptyrc_app::app!(Hello);
//! ```

pub mod deck;
#[cfg(target_arch = "wasm32")]
mod ffi;
#[cfg(not(target_arch = "wasm32"))]
#[path = "native.rs"]
mod ffi;
mod host;
pub mod json;

use std::fmt;
use std::sync::OnceLock;

pub use ffi::{alloc, free};
pub use host::{File, Stat, asset, base64, clock, result, storage};
pub use json::{Map, Value};

/// Off `wasm32` (`cargo test`): no host. A canvas is cells in memory,
/// `present` shows nothing; `log` goes to stderr; `random` is a fixed
/// sequence; `exit` panics; `call` answers `native` unless `serve` set
/// a handler for the thread.
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::{Error, Value, json};

    /// Answers each `call` of this thread from now: `f(method, params)`;
    /// `params`: `Null` for `{}`.
    pub fn serve(mut f: impl FnMut(&str, Value) -> Result<Value, Error> + 'static) {
        super::ffi::serve(Box::new(move |method, params| {
            let params = json::parse_bytes(params).unwrap_or(Value::Null);
            let answer = match f(method, params) {
                Ok(result) => crate::json!({ "result": result }),
                Err(e) => crate::json!({"error": {
                    "code": (e.code), "message": (e.message), "hint": (e.hint),
                }}),
            };
            answer.to_string().into_bytes()
        }))
    }
}

/// An app. One value of it lives from `init` to the end.
pub trait App: Sized + 'static {
    /// Once, first. `args`: the arguments and options, parsed.
    fn init(args: Value) -> Self;
    /// At the period asked (`period`); `now_ms`: session time since start.
    fn tick(&mut self, now_ms: u64) {
        let _ = now_ms;
    }
    /// A topic subscribed (`subscribe`); `data` as the protocol gives it.
    fn event(&mut self, topic: &str, data: Value) {
        let _ = (topic, data);
    }
    /// SIGTERM: 500 ms to finish.
    fn stop(&mut self) {}
}

/// Exports `init`, `tick`, `event`, `stop` for an [`App`]. Once per module.
#[macro_export]
macro_rules! app {
    ($t:ty) => {
        static __PTYRC_APP: $crate::__rt::Slot<$t> = $crate::__rt::Slot::new();
        #[unsafe(no_mangle)]
        pub extern "C" fn init(ptr: u32, len: u32) {
            $crate::__rt::init(&__PTYRC_APP, ptr, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tick(now_ms: i64) {
            $crate::__rt::tick(&__PTYRC_APP, now_ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn event(tp: u32, tl: u32, dp: u32, dl: u32) {
            $crate::__rt::event(&__PTYRC_APP, tp, tl, dp, dl)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn stop() {
            $crate::__rt::stop(&__PTYRC_APP)
        }
    };
}

#[doc(hidden)]
pub mod __rt {
    use super::{App, Value, ffi};
    pub use ffi::Slot;

    fn json(ptr: u32, len: u32) -> Value {
        super::json::parse_bytes(&ffi::arg(ptr, len)).unwrap_or(Value::Null)
    }

    pub fn init<A: App>(slot: &Slot<A>, ptr: u32, len: u32) {
        std::panic::set_hook(Box::new(|info| ffi::log(&format!("panic: {info}"))));
        let args = json(ptr, len);
        let _ = super::MANIFEST.set(super::Manifest::of(&args));
        let app = A::init(args);
        *slot.cell().borrow_mut() = Some(app);
    }

    fn with<A: App>(slot: &Slot<A>, f: impl FnOnce(&mut A)) {
        if let Some(app) = slot.cell().borrow_mut().as_mut() {
            f(app)
        }
    }

    pub fn tick<A: App>(slot: &Slot<A>, now_ms: i64) {
        with(slot, |a| a.tick(now_ms.max(0) as u64))
    }

    pub fn event<A: App>(slot: &Slot<A>, tp: u32, tl: u32, dp: u32, dl: u32) {
        let topic = String::from_utf8_lossy(&ffi::arg(tp, tl)).into_owned();
        with(slot, |a| a.event(&topic, json(dp, dl)))
    }

    pub fn stop<A: App>(slot: &Slot<A>) {
        with(slot, |a| a.stop())
    }
}

/// What the app is told of itself at `init`: `args["ptyrc:manifest"]`,
/// set by the runner from the manifest and the grants (`PROTOCOL.md`,
/// Manifest).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    /// `[permissions]`, by name.
    pub permissions: Vec<String>,
    /// The words of `words`, as declared.
    pub words: Vec<String>,
    /// The permissions held, the limits' among them.
    pub granted: Vec<String>,
    /// `[limits]` granted: `(key, value)`.
    pub limits: Vec<(String, u64)>,
}

impl Manifest {
    /// Of the argument of `init`; `None`: no member `ptyrc:manifest`.
    pub fn of(args: &Value) -> Option<Manifest> {
        let m = args.get("ptyrc:manifest")?.as_object()?;
        let text = |k: &str| {
            m.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let strings = |v: Option<&Value>| -> Vec<String> {
            let all = v.and_then(Value::as_array).into_iter().flatten();
            all.filter_map(|s| s.as_str().map(str::to_string)).collect()
        };
        let permissions = m.get("permissions").and_then(Value::as_array);
        let permissions = permissions.into_iter().flatten();
        Some(Manifest {
            id: text("id"),
            version: text("version"),
            permissions: (permissions.clone())
                .filter_map(|p| p.get("name")?.as_str().map(str::to_string))
                .collect(),
            words: permissions.flat_map(|p| strings(p.get("words"))).collect(),
            granted: strings(m.get("granted")),
            limits: (m
                .get("limits")
                .and_then(Value::as_object)
                .into_iter()
                .flatten())
            .filter_map(|(k, v)| Some((k.clone(), v.as_u64()?)))
            .collect(),
        })
    }

    /// `name` is held.
    pub fn holds(&self, name: &str) -> bool {
        self.granted.iter().any(|g| g == name)
    }
}

static MANIFEST: OnceLock<Option<Manifest>> = OnceLock::new();

/// The manifest told at `init`; `None` before `init`, or when the
/// runner was given none (`ptyrc-box` without `--manifest`).
pub fn manifest() -> Option<&'static Manifest> {
    MANIFEST.get().and_then(Option::as_ref)
}

/// The error of a request: the protocol's `code`, `message`, `hint`.
/// The runner adds `limit` (the rate), `gone` (the driver), `headless`.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
}

impl Error {
    /// `code`, `message`, no hint: `native::serve` answers with it.
    pub fn new(code: &str, message: impl Into<String>) -> Error {
        Error {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }

    /// The same, with `hint`.
    pub fn with_hint(self, hint: impl Into<String>) -> Error {
        Error {
            hint: Some(hint.into()),
            ..self
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " ({hint})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// A protocol request (`PROTOCOL.md`, Methods), answered before it
/// returns. `params`: an object, or `Value::Null` for `{}`.
pub fn call(method: &str, params: Value) -> Result<Value, Error> {
    let params = match params {
        Value::Null => Vec::new(),
        p => p.to_string().into_bytes(),
    };
    let bytes = ffi::call(method, &params).ok_or_else(|| Error::new("alloc", "no memory"))?;
    let mut answer = json::parse_bytes(&bytes)
        .map_err(|e| Error::new("bad-answer", format!("{method}: {e}")))?;
    if let Some(result) = answer.get_mut("result") {
        return Ok(result.take());
    }
    let e = answer.get("error").unwrap_or(&Value::Null);
    let text = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
    Err(Error {
        code: text("code").unwrap_or_else(|| "bad-answer".into()),
        message: text("message").unwrap_or_default(),
        hint: text("hint"),
    })
}

/// The tick wanted, ms; under 33: 33; 0: none (the default).
pub fn period(ms: u32) {
    ffi::period(ms)
}

/// A line of the app's log.
pub fn log(text: &str) {
    ffi::log(text)
}

/// Fills `buf` with random bytes.
pub fn random(buf: &mut [u8]) {
    ffi::random(buf)
}

/// A random `u32`.
pub fn random_u32() -> u32 {
    let mut b = [0; 4];
    ffi::random(&mut b);
    u32::from_le_bytes(b)
}

/// A random integer in `0..n`; 0 when `n` is 0.
pub fn random_below(n: u32) -> u32 {
    ((u64::from(random_u32()) * u64::from(n)) >> 32) as u32
}

/// Ends the app with `code`.
pub fn exit(code: i32) -> ! {
    ffi::exit(code)
}

/// `subscribe` to `topics`; their events reach [`App::event`].
pub fn subscribe(topics: &[&str]) -> Result<(), Error> {
    call("subscribe", crate::json!({ "topics": topics })).map(drop)
}

/// The screen, `(columns, rows)` (`session.info`). Event `size` tells a
/// change once subscribed.
pub fn size() -> Result<(u32, u32), Error> {
    let info = call("session.info", Value::Null)?;
    let n = |i: usize| info.pointer(&format!("/size/{i}")).and_then(Value::as_u64);
    match (n(0), n(1)) {
        (Some(c), Some(r)) => Ok((c as u32, r as u32)),
        _ => Err(Error::new("bad-answer", "session.info: no size")),
    }
}

/// `draw.put` of a whole sprite (`PROTOCOL.md`, Methods); its id.
pub fn put(sprite: Value) -> Result<u64, Error> {
    let answer = call("draw.put", sprite)?;
    answer
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new("bad-answer", "draw.put: no id"))
}

/// A text sprite: `rows` at `(x, y)` of the raw frame, one `style`
/// (`fg`, `bg`, `bold`, ...; a space is a hole); its id.
pub fn text(x: i32, y: i32, rows: &[&str], style: Value) -> Result<u64, Error> {
    put(crate::json!({
        "anchor": {"at": [x, y]},
        "content": {"rows": rows, "style": style},
    }))
}

/// `draw.remove` of a sprite of this app.
pub fn remove(id: u64) -> Result<(), Error> {
    call("draw.remove", crate::json!({ "id": id })).map(drop)
}

/// A colour of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    /// That of the cell below.
    #[default]
    Below,
    /// The terminal's default.
    Default,
    /// The palette, `0..=255`.
    Palette(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    fn bytes(self) -> [u8; 4] {
        match self {
            Color::Below => [0; 4],
            Color::Default => [1, 0, 0, 0],
            Color::Palette(i) => [2, i, 0, 0],
            Color::Rgb(r, g, b) => [3, r, g, b],
        }
    }

    fn of(b: &[u8]) -> Color {
        match b[0] {
            1 => Color::Default,
            2 => Color::Palette(b[1]),
            3 => Color::Rgb(b[1], b[2], b[3]),
            _ => Color::Below,
        }
    }
}

/// A run of a row: text in one style (`PROTOCOL.md`, `cells`).
/// `Color::Below`: absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub t: String,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u16,
}

const ATTRS: [(u16, &str); 7] = [
    (attr::BOLD, "bold"),
    (attr::DIM, "dim"),
    (attr::ITALIC, "italic"),
    (attr::UNDERLINE, "underline"),
    (attr::INVERSE, "inverse"),
    (attr::STRIKE, "strike"),
    (attr::BLINK, "blink"),
];

impl Run {
    /// `t` in no style.
    pub fn new(t: impl Into<String>) -> Run {
        Run {
            t: t.into(),
            ..Run::default()
        }
    }

    pub fn fg(self, fg: Color) -> Run {
        Run { fg, ..self }
    }

    pub fn bg(self, bg: Color) -> Run {
        Run { bg, ..self }
    }

    /// The attributes `attr::*` added.
    pub fn attrs(self, attrs: u16) -> Run {
        Run {
            attrs: self.attrs | attrs,
            ..self
        }
    }

    /// The style of `self` on another text.
    pub fn with(&self, t: impl Into<String>) -> Run {
        Run {
            t: t.into(),
            ..self.clone()
        }
    }

    /// `{"t", "fg", "bg", "bold", ...}`: a colour a palette index,
    /// `"#rrggbb"` or `"default"`.
    pub fn to_value(&self) -> Value {
        let color = |c: Color| match c {
            Color::Below => None,
            Color::Default => Some(Value::from("default")),
            Color::Palette(i) => Some(Value::from(i)),
            Color::Rgb(r, g, b) => Some(Value::from(format!("#{r:02x}{g:02x}{b:02x}"))),
        };
        let mut v = crate::json!({ "t": (self.t.as_str()) });
        if let Some(c) = color(self.fg) {
            v["fg"] = c;
        }
        if let Some(c) = color(self.bg) {
            v["bg"] = c;
        }
        for (bit, name) in ATTRS {
            if self.attrs & bit != 0 {
                v[name] = Value::Bool(true);
            }
        }
        v
    }
}

/// Columns of a glyph on a terminal, coarsely (the runner has the last
/// word): 0 a mark that joins the glyph before, 2 wide, else 1.
pub fn width(c: char) -> u32 {
    const ZERO: [(u32, u32); 7] = [
        (0x0300, 0x036f),
        (0x200b, 0x200f),
        (0x202a, 0x202e),
        (0x2060, 0x2064),
        (0xfe00, 0xfe0f),
        (0xfeff, 0xfeff),
        (0xe0000, 0xe0fff),
    ];
    const WIDE: [(u32, u32); 14] = [
        (0x1100, 0x115f),
        (0x2e80, 0x303e),
        (0x3041, 0x33ff),
        (0x3400, 0x4dbf),
        (0x4e00, 0x9fff),
        (0xa000, 0xa4cf),
        (0xac00, 0xd7a3),
        (0xf900, 0xfaff),
        (0xfe30, 0xfe4f),
        (0xff00, 0xff60),
        (0xffe0, 0xffe6),
        (0x1f300, 0x1f64f),
        (0x1f900, 0x1f9ff),
        (0x20000, 0x3fffd),
    ];
    let c = u32::from(c);
    let within = |t: &[(u32, u32)]| t.iter().any(|(a, b)| (*a..=*b).contains(&c));
    if c < 0x300 {
        1
    } else if within(&ZERO) {
        0
    } else if within(&WIDE) {
        2
    } else {
        1
    }
}

/// Columns of `text` on a terminal ([`width`] of each glyph).
pub fn cells(text: &str) -> u32 {
    text.chars().map(width).sum()
}

/// The attributes of a cell, or'ed.
pub mod attr {
    pub const BOLD: u16 = 1;
    pub const DIM: u16 = 2;
    pub const ITALIC: u16 = 4;
    pub const UNDERLINE: u16 = 8;
    pub const INVERSE: u16 = 16;
    pub const STRIKE: u16 = 32;
    pub const BLINK: u16 = 64;
}

/// A cell of a canvas. `ch` `None`: nothing drawn, the cell below shows.
/// A space is a hole unless `bg` is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub ch: Option<char>,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u16,
}

impl Cell {
    fn bytes(&self) -> [u8; 16] {
        let mut b = [0; 16];
        b[0..4].copy_from_slice(&self.ch.map_or(0, u32::from).to_le_bytes());
        b[4..8].copy_from_slice(&self.fg.bytes());
        b[8..12].copy_from_slice(&self.bg.bytes());
        b[12..14].copy_from_slice(&self.attrs.to_le_bytes());
        b
    }

    fn of(b: &[u8; 16]) -> Cell {
        let ch = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        Cell {
            ch: char::from_u32(ch).filter(|_| ch != 0),
            fg: Color::of(&b[4..8]),
            bg: Color::of(&b[8..12]),
            attrs: u16::from_le_bytes([b[12], b[13]]),
        }
    }
}

/// A buffer of `w * h` cells shown as one sprite by [`Canvas::present`].
/// Dropped: released, its picture removed. Writes outside are ignored.
pub struct Canvas {
    id: u32,
    w: u32,
    h: u32,
    cells: ffi::Cells,
}

impl Canvas {
    /// `None`: refused (no cell, or more cells in all than four screens).
    pub fn new(w: u32, h: u32) -> Option<Canvas> {
        let n = (w as usize).checked_mul(h as usize).filter(|n| *n > 0)?;
        let id = ffi::canvas(w, h);
        if id == 0 {
            return None;
        }
        match ffi::Cells::of(id, n) {
            Some(cells) => Some(Canvas { id, w, h, cells }),
            None => {
                ffi::release(id);
                None
            }
        }
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn height(&self) -> u32 {
        self.h
    }

    fn at(&mut self, x: u32, y: u32) -> Option<&mut [u8; 16]> {
        (x < self.w && y < self.h).then(|| &mut self.cells.get()[(y * self.w + x) as usize])
    }

    pub fn get(&mut self, x: u32, y: u32) -> Cell {
        self.at(x, y).map_or(Cell::default(), |b| Cell::of(b))
    }

    pub fn set(&mut self, x: u32, y: u32, cell: Cell) {
        if let Some(b) = self.at(x, y) {
            *b = cell.bytes();
        }
    }

    /// The glyph only. `None`: nothing drawn. A wide glyph: its first
    /// cell; leave the next one `None`.
    pub fn set_char(&mut self, x: u32, y: u32, ch: Option<char>) {
        if let Some(b) = self.at(x, y) {
            b[0..4].copy_from_slice(&ch.map_or(0, u32::from).to_le_bytes());
        }
    }

    pub fn set_fg(&mut self, x: u32, y: u32, c: Color) {
        if let Some(b) = self.at(x, y) {
            b[4..8].copy_from_slice(&c.bytes());
        }
    }

    pub fn set_bg(&mut self, x: u32, y: u32, c: Color) {
        if let Some(b) = self.at(x, y) {
            b[8..12].copy_from_slice(&c.bytes());
        }
    }

    pub fn set_attrs(&mut self, x: u32, y: u32, attrs: u16) {
        if let Some(b) = self.at(x, y) {
            b[12..14].copy_from_slice(&attrs.to_le_bytes());
        }
    }

    /// The glyphs of `s` from `(x, y)` rightward, one cell each; the
    /// colours and attributes of the cells kept.
    pub fn text(&mut self, x: u32, y: u32, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.set_char(x.saturating_add(i as u32), y, Some(ch));
        }
    }

    /// Every cell to `cell`.
    pub fn fill(&mut self, cell: Cell) {
        let b = cell.bytes();
        self.cells.get().fill(b);
    }

    /// Every cell to nothing drawn.
    pub fn clear(&mut self) {
        self.fill(Cell::default());
    }

    /// Rows of runs: a widget's content as `cells`; a cell with no
    /// glyph is a blank of no style.
    pub fn runs(&mut self) -> Vec<Vec<Run>> {
        let w = self.w as usize;
        let rows = self.cells.get().chunks(w.max(1));
        let rows = rows.map(|row| {
            let mut out: Vec<Run> = Vec::new();
            let mut key: Option<[u8; 10]> = None;
            let mut x = 0;
            while x < row.len() {
                let cell = Cell::of(&row[x]);
                let k = cell
                    .ch
                    .map_or([0xff; 10], |_| row[x][4..14].try_into().unwrap());
                if out.is_empty() || key != Some(k) {
                    out.push(match cell.ch {
                        Some(_) => Run {
                            t: String::new(),
                            fg: cell.fg,
                            bg: cell.bg,
                            attrs: cell.attrs,
                        },
                        None => Run::default(),
                    });
                    key = Some(k);
                }
                let ch = cell.ch.unwrap_or(' ');
                out.last_mut().expect("a run").t.push(ch);
                x += if cell.ch.is_some() && cells(&ch.to_string()) == 2 {
                    2
                } else {
                    1
                };
            }
            out
        });
        rows.collect()
    }

    /// Shows the canvas. `place`: the members of a sprite but its content
    /// (`anchor`, `frame`, `z`, `alpha`, `layer`, ...), or `Value::Null`.
    /// Sends what changed since the last time. `false`: refused, the
    /// reason logged.
    pub fn present(&mut self, place: &Value) -> bool {
        let place = match place {
            Value::Null => Vec::new(),
            p => p.to_string().into_bytes(),
        };
        ffi::present(self.id, &place)
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        ffi::release(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn the_manifest_told_at_init() {
        let args = json!({"rate": 3, "ptyrc:manifest": {
            "id": "snake", "version": "0.1.0",
            "permissions": [{"name": "shape"}, {"name": "words", "words": ["food"]}],
            "granted": ["shape", "limit-time"],
            "limits": {"tick-ms": 100},
        }});
        let m = Manifest::of(&args).unwrap();
        assert_eq!((m.id.as_str(), m.version.as_str()), ("snake", "0.1.0"));
        assert_eq!(m.permissions, ["shape", "words"]);
        assert_eq!(m.words, ["food"]);
        assert!(m.holds("shape") && m.holds("limit-time") && !m.holds("words"));
        assert_eq!(m.limits, [("tick-ms".to_string(), 100)]);
        assert_eq!(Manifest::of(&json!({"rate": 3})), None);
        assert_eq!(manifest(), None);
    }
}
