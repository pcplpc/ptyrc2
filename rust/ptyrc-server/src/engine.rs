// SPDX-License-Identifier: GPL-3.0-or-later
//! The seam between the driver and what it drives: screen model, frames,
//! scene. The driver calls the hooks; the engine answers through `Ctx`.

use ptyrc_proto::RpcError;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// A client connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u64);

/// A request handed to the engine. Needed to answer a `Reply::Pending`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub u64);

/// What the driver knows of the session.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub name: String,
    pub socket: PathBuf,
    pub argv: Vec<String>,
    /// Pid of the app.
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub headless: bool,
}

/// Answer of `Engine::handle`.
#[derive(Debug)]
pub enum Reply {
    Ok(Value),
    Err(RpcError),
    /// Answered later with `Ctx::complete`. Refused inside a `batch`.
    Pending,
}

/// What an engine hook asked of the driver; applied when the hook returns.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    pub pty_input: Vec<u8>,
    pub events: Vec<(String, Value)>,
    pub completions: Vec<(Token, Result<Value, RpcError>)>,
    pub repaint: bool,
}

/// The driver as an engine hook sees it.
pub struct Ctx<'a> {
    pub(crate) info: &'a SessionInfo,
    pub(crate) topics: &'a HashSet<String>,
    pub(crate) now: Duration,
    pub(crate) fx: &'a mut Effects,
}

impl Ctx<'_> {
    pub fn info(&self) -> &SessionInfo {
        self.info
    }

    /// Monotonic time since the driver started.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Type bytes into the app, as if they came from the keyboard.
    pub fn write_pty(&mut self, bytes: &[u8]) {
        self.fx.pty_input.extend_from_slice(bytes);
    }

    /// Whether a client subscribed to `topic`: skip building an event
    /// nobody reads.
    pub fn has_subscribers(&self, topic: &str) -> bool {
        self.topics.contains(topic)
    }

    /// Send an event to the subscribers of `topic`.
    pub fn emit(&mut self, topic: &str, data: Value) {
        if self.has_subscribers(topic) {
            self.fx.events.push((topic.to_string(), data));
        }
    }

    /// Answer a request left `Pending`. Unknown tokens (client gone) are
    /// ignored.
    pub fn complete(&mut self, token: Token, result: Result<Value, RpcError>) {
        self.fx.completions.push((token, result));
    }

    /// Ask for one `on_tick` before the driver sleeps again.
    pub fn request_repaint(&mut self) {
        self.fx.repaint = true;
    }
}

/// What the driver drives. Every hook has a pass-through default.
///
/// `now` is monotonic, from driver start. `out` collects the bytes to
/// write to the host; the driver writes them in one piece when the hook
/// returns.
pub trait Engine {
    /// The session is up: app spawned, socket listening.
    fn on_start(&mut self, _ctx: &mut Ctx) {}

    /// A burst of app output. Append to `out` everything the host must
    /// receive for it, the burst included.
    fn on_burst(&mut self, burst: &[u8], _now: Duration, out: &mut Vec<u8>, _ctx: &mut Ctx) {
        out.extend_from_slice(burst);
    }

    /// The deadline of `next_wake` passed, or a repaint was requested.
    fn on_tick(&mut self, _now: Duration, _out: &mut Vec<u8>, _ctx: &mut Ctx) {}

    /// Absolute time of the next `on_tick`, same clock as `now`.
    fn next_wake(&self, _now: Duration) -> Option<Duration> {
        None
    }

    /// The host was resized; the pty already has the new size.
    fn on_resize(
        &mut self,
        _cols: u16,
        _rows: u16,
        _now: Duration,
        _out: &mut Vec<u8>,
        _ctx: &mut Ctx,
    ) {
    }

    /// The app is gone. Last chance to clean the host screen.
    fn on_exit(&mut self, _out: &mut Vec<u8>, _ctx: &mut Ctx) {}

    /// Bytes read from the host keyboard. Return `false` to keep them from
    /// the app.
    fn on_input(&mut self, _bytes: &[u8], _ctx: &mut Ctx) -> bool {
        true
    }

    /// A request the driver does not answer itself.
    fn handle(
        &mut self,
        _conn: ConnId,
        _token: Token,
        method: &str,
        _params: Value,
        _ctx: &mut Ctx,
    ) -> Reply {
        Reply::Err(RpcError::unknown_method(method))
    }

    /// A client left. Its pending requests are forgotten by the driver.
    fn on_disconnect(&mut self, _conn: ConnId, _ctx: &mut Ctx) {}

    /// The app asked for application cursor keys (DECCKM).
    fn app_cursor_keys(&self) -> bool {
        false
    }

    /// The app asked for bracketed paste.
    fn bracketed_paste(&self) -> bool {
        false
    }

    /// What the app asked of the mouse.
    fn mouse_mode(&self) -> crate::mouse::MouseMode {
        crate::mouse::MouseMode::default()
    }

    /// The sprite a click at this screen cell is for: the data of its
    /// `mouse` event. `None`: the click is the app's.
    fn sprite_at(&self, _x: u16, _y: u16) -> Option<Value> {
        None
    }

    /// The default colours of the host: at start, then when the host
    /// tells others.
    fn on_host_colors(&mut self, _colors: ptyrc_core::scene::HostColors, _ctx: &mut Ctx) {}

    /// `subscribe {snapshot}`: the state of `topic` now, as the data of
    /// the events a subscriber would have been sent to know it.
    fn snapshot(&mut self, _topic: &str, _now: Duration) -> Vec<Value> {
        Vec::new()
    }

    /// The clock stands still (a paused replay): the real time at which
    /// `on_stall` is to be called, if any.
    fn stall_wake(&self) -> Option<std::time::Instant> {
        None
    }

    /// The clock stands still and `stall_wake` passed: answer what cannot
    /// wait on it any longer.
    fn on_stall(&mut self, _ctx: &mut Ctx) {}

    /// Members merged into the `session.info` result (an object, or null).
    fn session_extras(&self) -> Value {
        Value::Null
    }
}

/// The engine that does nothing: the driver is a transparent wrapper.
#[derive(Debug, Default)]
pub struct PassThrough;

impl Engine for PassThrough {}
