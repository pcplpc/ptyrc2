// SPDX-License-Identifier: GPL-3.0-or-later
//! What the imports do: the host behind a trait, the canvases and
//! their diff, the limits that are counts. No engine here.

use crate::abi::{CELL, MIN_PERIOD_MS};
use serde_json::{Map, Value, json};
use std::fmt;
use std::time::Duration;

/// The limits of one app (plan, Limits).
#[derive(Clone, Debug)]
pub struct Limits {
    /// Linear memory, bytes.
    pub memory: usize,
    /// A `tick` or an `event` longer than this is late.
    pub late: Duration,
    /// Late calls in a row before the app is paused.
    pub late_row: u32,
    /// A `tick`, an `event` or `stop` longer than this ends the app.
    pub hard: Duration,
    /// `init` longer than this ends the app.
    pub init: Duration,
    /// `call` a second.
    pub calls: u32,
    /// Canvas cells in all, in screens.
    pub canvas_screens: u32,
    /// Log bytes; the rest is dropped.
    pub log: usize,
    /// A JSON argument or answer, bytes.
    pub json: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            memory: 64 << 20,
            late: Duration::from_millis(50),
            late_row: 3,
            hard: Duration::from_secs(2),
            init: Duration::from_secs(2),
            calls: 1000,
            canvas_screens: 4,
            log: 1 << 20,
            json: 4 << 20,
        }
    }
}

/// An error of a request, as the guest reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct CallError {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
}

impl CallError {
    pub fn new(code: &str, message: impl Into<String>) -> CallError {
        CallError {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }

    fn wire(&self) -> Value {
        let mut e = json!({"code": self.code, "message": self.message});
        if let Some(h) = &self.hint {
            e["hint"] = json!(h);
        }
        json!({"error": e})
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// What the runner asks of the outside: a driver, or nothing.
pub trait Host: Send + 'static {
    /// One protocol request.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError>;
    /// The screen, `(columns, rows)`.
    fn size(&self) -> (u32, u32);
    /// A line of the app's log.
    fn log(&mut self, text: &str);
    fn random(&mut self, buf: &mut [u8]);
    /// The app's assets, answered by the runner (`asset.*`).
    fn assets(&self) -> Option<&crate::assets::Assets> {
        None
    }
}

struct Canvas {
    w: u32,
    h: u32,
    ptr: u32,
    sprite: Option<u64>,
    /// The cells last presented.
    shown: Vec<u8>,
    place: Map<String, Value>,
}

/// A row of `content.grid`; `trim`: trailing holes left out.
fn grid_row(cells: &[u8], trim: bool) -> Value {
    let mut s = String::new();
    ptyrc_core::scene::grid::encode_row(cells, trim, &mut s);
    s.into()
}

/// Sprite members a `place` may not set: the content is the canvas.
const NOT_PLACE: [&str; 7] = [
    "id", "content", "frames", "loop", "effect", "persist", "rows",
];

/// The state behind the imports.
pub struct Core<H> {
    pub host: H,
    pub limits: Limits,
    canvases: Vec<Option<Canvas>>,
    period: u32,
    exit: Option<i32>,
    now_ms: u64,
    window: (u64, u32),
    logged: usize,
    met: Vec<&'static str>,
}

impl<H: Host> Core<H> {
    pub fn new(host: H, limits: Limits) -> Core<H> {
        Core {
            host,
            limits,
            canvases: Vec::new(),
            period: 0,
            exit: None,
            now_ms: 0,
            window: (0, 0),
            logged: 0,
            met: Vec::new(),
        }
    }

    /// The clock the call rate is counted on.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The tick period wanted, ms; 0: events only.
    pub fn period(&self) -> u32 {
        self.period
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit
    }

    /// The limits met so far, each named once.
    pub fn met(&self) -> &[&'static str] {
        &self.met
    }

    pub fn meet(&mut self, limit: &'static str) {
        if !self.met.contains(&limit) {
            self.met.push(limit);
            self.host.log(&format!("limit: {limit}"));
        }
    }

    // Imports.

    /// `call`: the answer, JSON.
    pub fn call(&mut self, method: &[u8], params: &[u8]) -> Vec<u8> {
        let answer = self.call_value(method, params);
        let wire = match answer {
            Ok(v) => json!({"result": v}),
            Err(e) => e.wire(),
        };
        serde_json::to_vec(&wire).unwrap_or_default()
    }

    fn call_value(&mut self, method: &[u8], params: &[u8]) -> Result<Value, CallError> {
        if self.now_ms.saturating_sub(self.window.0) >= 1000 {
            self.window = (self.now_ms, 0);
        }
        self.window.1 += 1;
        if self.window.1 > self.limits.calls {
            self.meet("call");
            return Err(CallError::new(
                "limit",
                format!("call: more than {} a second", self.limits.calls),
            ));
        }
        let method = std::str::from_utf8(method)
            .map_err(|_| CallError::new("bad-request", "method: not UTF-8"))?;
        let params: Value = if params.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(params)
                .map_err(|e| CallError::new("bad-request", format!("params: {e}")))?
        };
        // Assets: the runner's, never the driver's.
        if method.starts_with("asset.") {
            let none = crate::assets::Assets::default();
            return self.host.assets().unwrap_or(&none).call(method, &params);
        }
        self.host.call(method, params)
    }

    /// `canvas`: an id, or 0 when refused. The caller allocates the
    /// cells in the guest and gives the pointer to `bind`.
    pub fn canvas(&mut self, w: u32, h: u32) -> u32 {
        let (cols, rows) = self.host.size();
        let budget = u64::from(cols) * u64::from(rows) * u64::from(self.limits.canvas_screens);
        let used: u64 = self
            .canvases
            .iter()
            .flatten()
            .map(|c| u64::from(c.w) * u64::from(c.h))
            .sum();
        let cells = u64::from(w) * u64::from(h);
        if w == 0 || h == 0 || used + cells > budget {
            self.meet("canvas");
            self.host.log(&format!(
                "canvas {w}x{h} refused: {budget} cells in all ({} screens), {used} held",
                self.limits.canvas_screens
            ));
            return 0;
        }
        let canvas = Canvas {
            w,
            h,
            ptr: 0,
            sprite: None,
            shown: Vec::new(),
            place: Map::new(),
        };
        let slot = match self.canvases.iter().position(Option::is_none) {
            Some(i) => i,
            None => {
                self.canvases.push(None);
                self.canvases.len() - 1
            }
        };
        self.canvases[slot] = Some(canvas);
        slot as u32 + 1
    }

    /// Bytes of the cells of a canvas.
    pub fn span(&self, id: u32) -> Option<(u32, usize)> {
        let c = self.get(id)?;
        Some((c.ptr, c.w as usize * c.h as usize * CELL))
    }

    pub fn bind(&mut self, id: u32, ptr: u32) {
        if let Some(Some(c)) = self.canvases.get_mut(id.wrapping_sub(1) as usize) {
            c.ptr = ptr;
        }
    }

    fn get(&self, id: u32) -> Option<&Canvas> {
        self.canvases.get(id.checked_sub(1)? as usize)?.as_ref()
    }

    /// `release`: the canvas forgotten, its sprite removed. The span to
    /// free in the guest.
    pub fn release(&mut self, id: u32) -> Option<(u32, usize)> {
        let span = self.span(id)?;
        let canvas = self.canvases[id as usize - 1].take()?;
        if let Some(sprite) = canvas.sprite {
            let _ = self.host.call("draw.remove", json!({"id": sprite}));
        }
        Some(span)
    }

    /// `present`: 0, or 1 when the driver refused (the reason is
    /// logged). Nothing is sent when nothing changed.
    pub fn present(&mut self, id: u32, place: &[u8], cells: &[u8]) -> i32 {
        match self.present_inner(id, place, cells) {
            Ok(()) => 0,
            Err(e) => {
                self.host.log(&format!("present: {e}"));
                1
            }
        }
    }

    fn present_inner(&mut self, id: u32, place: &[u8], cells: &[u8]) -> Result<(), CallError> {
        let mut place: Map<String, Value> = if place.is_empty() {
            Map::new()
        } else {
            serde_json::from_slice(place)
                .map_err(|e| CallError::new("bad-params", format!("place: {e}")))?
        };
        for key in NOT_PLACE {
            place.remove(key);
        }
        let slot = id.wrapping_sub(1) as usize;
        let Some(Some(canvas)) = self.canvases.get(slot) else {
            return Err(CallError::new("bad-params", format!("no canvas {id}")));
        };
        let width = canvas.w as usize * CELL;
        let content = |cells: &[u8]| {
            let rows: Vec<Value> = cells
                .chunks_exact(width)
                .map(|r| grid_row(r, false))
                .collect();
            json!({"grid": rows})
        };
        let fresh = canvas.shown != cells;
        let moved = canvas.place != place;
        if let Some(sprite) = canvas.sprite {
            if !fresh && !moved {
                return Ok(());
            }
            let mut patch = if moved { place.clone() } else { Map::new() };
            patch.insert("id".into(), json!(sprite));
            if fresh {
                // The rows that changed; trailing holes are left out.
                let rows: Map<String, Value> = cells
                    .chunks_exact(width)
                    .zip(canvas.shown.chunks_exact(width))
                    .enumerate()
                    .filter(|(_, (now, was))| now != was)
                    .map(|(y, (now, _))| (y.to_string(), grid_row(now, true)))
                    .collect();
                patch.insert("rows".into(), Value::Object(rows));
            }
            match self.host.call("draw.update", Value::Object(patch)) {
                Ok(_) => return self.keep(slot, None, place, fresh.then_some(cells)),
                // Removed meanwhile (a `ttl_ms`, a `draw.clear`): put again.
                Err(e) if e.code == "no-sprite" => {}
                Err(e) => return Err(e),
            }
        }
        let mut spec = place.clone();
        spec.insert("content".into(), content(cells));
        let put = self.host.call("draw.put", Value::Object(spec))?;
        let sprite = put.get("id").and_then(Value::as_u64);
        self.keep(slot, Some(sprite), place, Some(cells))
    }

    fn keep(
        &mut self,
        slot: usize,
        sprite: Option<Option<u64>>,
        place: Map<String, Value>,
        cells: Option<&[u8]>,
    ) -> Result<(), CallError> {
        if let Some(Some(c)) = self.canvases.get_mut(slot) {
            if let Some(sprite) = sprite {
                c.sprite = sprite;
            }
            c.place = place;
            if let Some(cells) = cells {
                c.shown.clear();
                c.shown.extend_from_slice(cells);
            }
        }
        Ok(())
    }

    /// `period`.
    pub fn set_period(&mut self, ms: u32) {
        self.period = match ms {
            0 => 0,
            ms => ms.max(MIN_PERIOD_MS),
        };
    }

    /// `log`.
    pub fn log(&mut self, text: &[u8]) {
        let left = self.limits.log.saturating_sub(self.logged);
        if left == 0 {
            return;
        }
        let text = String::from_utf8_lossy(&text[..text.len().min(left)]);
        self.logged += text.len().max(1);
        if self.logged >= self.limits.log {
            self.meet("log");
        }
        let clean: String = text
            .chars()
            .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
            .collect();
        self.host.log(&clean);
    }

    /// `exit`.
    pub fn set_exit(&mut self, code: i32) {
        self.exit.get_or_insert(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{Cell, Color};

    #[derive(Default)]
    struct Calls(Vec<(String, Value)>);

    impl Host for Calls {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
            self.0.push((method.into(), params));
            Ok(json!({"id": 9}))
        }
        fn size(&self) -> (u32, u32) {
            (80, 24)
        }
        fn log(&mut self, _: &str) {}
        fn random(&mut self, _: &mut [u8]) {}
    }

    /// Cells as sent: base64, bytes 14..16 the repeats.
    fn b64_of(cells: &[(Cell, u16)]) -> Value {
        let mut bytes = Vec::new();
        for (c, n) in cells {
            bytes.extend_from_slice(&c.bytes()[..14]);
            bytes.extend(n.to_le_bytes());
        }
        let mut s = String::new();
        ptyrc_core::scene::grid::encode(&bytes, &mut s);
        s.into()
    }

    /// The first `present` puts every row, equal cells as repeats; then
    /// the rows that changed, trailing holes left out; nothing when
    /// nothing changed.
    #[test]
    fn present_sends_changed_rows() {
        let mut core = Core::new(Calls::default(), Limits::default());
        let id = core.canvas(3, 2);
        let blank = [0u8; 3 * 2 * CELL];
        let mut cells = blank;
        let a = Cell {
            ch: Some('a'),
            fg: Color::Rgb(1, 2, 3),
            bg: Color::None,
            attrs: 1,
        };
        cells[CELL..2 * CELL].copy_from_slice(&a.bytes());
        assert_eq!(core.present(id, b"", &cells), 0);
        assert_eq!(core.present(id, b"", &cells), 0);
        cells[3 * CELL..4 * CELL].copy_from_slice(&a.bytes());
        assert_eq!(core.present(id, b"{\"z\":2,\"rows\":{}}", &cells), 0);
        let none = Cell {
            ch: None,
            fg: Color::None,
            bg: Color::None,
            attrs: 0,
        };
        let calls = &core.host.0;
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "draw.put");
        assert_eq!(
            calls[0].1,
            json!({"content": {"grid": [b64_of(&[(none, 0), (a, 0), (none, 0)]), b64_of(&[(none, 2)])]}})
        );
        assert_eq!(
            calls[1],
            (
                "draw.update".into(),
                json!({"id": 9, "z": 2, "rows": {"1": b64_of(&[(a, 0)])}})
            )
        );
    }
}
