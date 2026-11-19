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

mod ffi;

use std::fmt;

pub use ffi::{alloc, free};
pub use serde_json::{self as json, Value, json};

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
        serde_json::from_slice(&ffi::arg(ptr, len)).unwrap_or(Value::Null)
    }

    pub fn init<A: App>(slot: &Slot<A>, ptr: u32, len: u32) {
        std::panic::set_hook(Box::new(|info| ffi::log(&format!("panic: {info}"))));
        let app = A::init(json(ptr, len));
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

/// The error of a request: the protocol's `code`, `message`, `hint`.
/// The runner adds `limit` (the rate), `gone` (the driver), `headless`.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
}

impl Error {
    fn new(code: &str, message: impl Into<String>) -> Error {
        Error {
            code: code.into(),
            message: message.into(),
            hint: None,
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

/// A protocol request (`plan/design.md`, Protocol), answered before it
/// returns. `params`: an object, or `Value::Null` for `{}`.
pub fn call(method: &str, params: Value) -> Result<Value, Error> {
    let params = match params {
        Value::Null => Vec::new(),
        p => serde_json::to_vec(&p).unwrap_or_default(),
    };
    let bytes = ffi::call(method, &params).ok_or_else(|| Error::new("alloc", "no memory"))?;
    let mut answer: Value = serde_json::from_slice(&bytes)
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
    call("subscribe", json!({ "topics": topics })).map(drop)
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

/// `draw.put` of a whole sprite (`plan/design.md`, scene); its id.
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
    put(json!({
        "anchor": {"at": [x, y]},
        "content": {"rows": rows, "style": style},
    }))
}

/// `draw.remove` of a sprite of this app.
pub fn remove(id: u64) -> Result<(), Error> {
    call("draw.remove", json!({ "id": id })).map(drop)
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

    /// Shows the canvas. `place`: the members of a sprite but its content
    /// (`anchor`, `frame`, `z`, `alpha`, `layer`, ...), or `Value::Null`.
    /// Sends what changed since the last time. `false`: refused, the
    /// reason logged.
    pub fn present(&mut self, place: &Value) -> bool {
        let place = match place {
            Value::Null => Vec::new(),
            p => serde_json::to_vec(p).unwrap_or_default(),
        };
        ffi::present(self.id, &place)
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        ffi::release(self.id)
    }
}
