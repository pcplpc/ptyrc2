// SPDX-License-Identifier: GPL-3.0-or-later
//! A run without a driver: an empty screen, the clock an argument.
//! The same module, options and seed give the same report.

use crate::core::{CallError, Host, Limits};
use crate::error::Error;
use crate::runner::Runner;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt;

/// A request the guest made, at session time `t` (ms). `content` is
/// left out of the `draw` requests.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub t: u64,
    pub method: String,
    pub params: Value,
    /// The code of the error, when refused.
    pub refused: Option<String>,
}

struct Sprite {
    at: (i64, i64),
    z: i64,
    rows: Vec<Value>,
}

/// The host of a headless run: answers `draw.*`, `subscribe`,
/// `unsubscribe`, `session.info`, `screen.text` and `screen.shape` (a
/// blank screen); refuses the rest (`headless`).
pub struct Headless {
    size: (u32, u32),
    sprites: BTreeMap<u64, Sprite>,
    next: u64,
    now: u64,
    rng: u64,
    pub requests: Vec<Request>,
    pub log: Vec<String>,
}

impl Headless {
    pub fn new(size: (u32, u32), seed: u64) -> Headless {
        Headless {
            size,
            sprites: BTreeMap::new(),
            next: 1,
            now: 0,
            rng: seed | 1,
            requests: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Session time of the requests to come.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now = now_ms;
    }

    fn patch(sprite: &mut Sprite, params: &Value) {
        if let Some(at) = params.pointer("/anchor/at").and_then(Value::as_array) {
            let n = |i: usize| at.get(i).and_then(Value::as_i64).unwrap_or(0);
            sprite.at = (n(0), n(1));
        }
        if let Some(z) = params.get("z").and_then(Value::as_i64) {
            sprite.z = z;
        }
        if let Some(rows) = params.pointer("/content/cells").and_then(Value::as_array) {
            sprite.rows = rows.clone();
        }
    }

    fn answer(&mut self, method: &str, params: &Value) -> Result<Value, CallError> {
        let id = params.get("id").and_then(Value::as_u64).unwrap_or(0);
        let missing = || CallError::new("no-sprite", format!("no such sprite: {id}"));
        let (cols, rows) = self.size;
        match method {
            "draw.put" => {
                let mut sprite = Sprite {
                    at: (0, 0),
                    z: 0,
                    rows: Vec::new(),
                };
                Self::patch(&mut sprite, params);
                let id = self.next;
                self.next += 1;
                self.sprites.insert(id, sprite);
                Ok(json!({"id": id}))
            }
            "draw.update" => {
                Self::patch(self.sprites.get_mut(&id).ok_or_else(missing)?, params);
                Ok(json!({}))
            }
            "draw.remove" => self
                .sprites
                .remove(&id)
                .map(|_| json!({}))
                .ok_or_else(missing),
            "draw.clear" => {
                let removed = self.sprites.len();
                self.sprites.clear();
                Ok(json!({"removed": removed}))
            }
            "subscribe" | "unsubscribe" => Ok(json!({})),
            "session.info" => Ok(json!({"size": [cols, rows], "headless": true})),
            "screen.text" | "screen.shape" => Ok(json!({
                "lines": vec![""; rows as usize],
                "size": [cols, rows],
                "rect": [0, 0, cols, rows],
            })),
            _ => Err(CallError::new(
                "headless",
                format!("{method}: not answered without a session"),
            )),
        }
    }

    /// The canvases as text, by `z` then age; an anchor other than `at`
    /// is at the origin. Trailing spaces are cut.
    pub fn screen(&self) -> Vec<String> {
        let (cols, rows) = (self.size.0 as i64, self.size.1 as i64);
        let mut grid = vec![vec![' '; cols as usize]; rows as usize];
        let mut order: Vec<(&u64, &Sprite)> = self.sprites.iter().collect();
        order.sort_by_key(|(id, s)| (s.z, **id));
        for (_, sprite) in order {
            for (dy, runs) in sprite.rows.iter().enumerate() {
                let y = sprite.at.1 + dy as i64;
                if !(0..rows).contains(&y) {
                    continue;
                }
                let mut x = sprite.at.0;
                for run in runs.as_array().into_iter().flatten() {
                    let hole = run.get("bg").is_none();
                    for c in run.get("t").and_then(Value::as_str).unwrap_or("").chars() {
                        let w = ptyrc_vt::char_width(c) as i64;
                        if (0..cols).contains(&x) && !(c == ' ' && hole) {
                            grid[y as usize][x as usize] = c;
                            if w == 2 && x + 1 < cols {
                                grid[y as usize][x as usize + 1] = '\0';
                            }
                        }
                        x += w;
                    }
                }
            }
        }
        grid.iter()
            .map(|row| {
                let line: String = row.iter().filter(|c| **c != '\0').collect();
                line.trim_end().to_string()
            })
            .collect()
    }
}

impl Host for Headless {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, CallError> {
        let answer = self.answer(method, &params);
        if method.starts_with("draw.")
            && let Some(p) = params.as_object_mut()
        {
            p.remove("content");
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
        self.size
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
    /// The screen, `(columns, rows)`.
    pub size: (u32, u32),
    /// The argument of `init`.
    pub args: Value,
    /// Session time the run lasts, ms.
    pub duration_ms: u64,
    /// Times of the screens wanted, ms; empty: the end.
    pub at: Vec<u64>,
    /// Events delivered: `(ms, topic, data)`.
    pub events: Vec<(u64, String, Value)>,
    /// Seed of `random`.
    pub seed: u64,
    pub limits: Limits,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            size: (80, 24),
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

/// Run a module headless. `Err`: it did not load.
pub fn run(wasm: &[u8], options: &Options) -> Result<Report, Error> {
    let host = Headless::new(options.size, options.seed);
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
    // At one time: the events, the tick, the screen.
    while done.is_ok() {
        let now = [
            events.peek().map(|e| e.0),
            next_tick.filter(|t| *t <= end),
            at.peek().copied(),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(now) = now else { break };
        runner.host_mut().set_now(now);
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
        if at.next_if_eq(&now).is_some() {
            screens.push((now, runner.host().screen()));
        }
    }
    // Ended early: the screens left show what stays.
    for t in at {
        screens.push((t, runner.host().screen()));
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
