// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen engine: terminal model, frames, scene, and the protocol
//! methods on them. Implements the burst pipeline of `plan/design.md`,
//! "Rendering".

use crate::engine::{ConnId, Ctx, Engine, Reply, Token};
use crate::rpc;
use ptyrc_core::frames::{FrameConfig, Frames, ScreenInput};
use ptyrc_core::plugin::{DrawExt, TuiSupport};
use ptyrc_core::scene::Scene;
use ptyrc_core::{FrameId, OwnerId, TextQuery};
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_vt::{Grid, Vt, VtEvent, repair};
use serde_json::{Value, json};
use std::time::Duration;

/// Begin and end of a synchronized update.
const BSU: &[u8] = b"\x1b[?2026h";
const ESU: &[u8] = b"\x1b[?2026l";

/// Owner of the sprites of `debug.overlay`.
pub(crate) const DEBUG_OWNER: OwnerId = u64::MAX;
/// Refresh period of the debug overlay.
const DEBUG_PERIOD: Duration = Duration::from_millis(400);

/// A `screen.wait` left pending.
pub(crate) struct Wait {
    pub token: Token,
    pub conn: ConnId,
    pub frame: FrameId,
    pub query: Option<TextQuery>,
    pub gone: bool,
    pub stable: Option<Duration>,
    pub deadline: Duration,
}

/// What `debug.overlay` shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overlay {
    Off,
    Frames,
    Heat,
}

pub struct ScreenEngine {
    /// What the app means to show.
    pub(crate) app: Vt,
    /// What the host shows.
    pub(crate) host: Vt,
    /// App grid at the end of the last cycle.
    pub(crate) base: Grid,
    /// The host may show sprite cells.
    painted: bool,
    /// No sprite since the host model was last fed: it is the app model,
    /// and is copied from it when the first sprite comes.
    host_stale: bool,
    pub(crate) frames: Frames,
    pub(crate) scene: Scene,
    pub(crate) exts: Vec<Box<dyn DrawExt>>,
    pub(crate) waits: Vec<Wait>,
    /// Last time the app grid changed.
    pub(crate) last_change: Duration,
    last_cursor: (u16, u16),
    pub(crate) overlay: Overlay,
    overlay_due: Duration,
    /// Bytes of our own written to the host, for `debug.dump`.
    pub(crate) injected: u64,
    /// Bursts seen.
    pub(crate) bursts: u64,
    /// Time of the last tick.
    ticked: Duration,
    /// Ticks run.
    pub(crate) ticks: u64,
    /// Time spent in `on_burst`, for `debug.dump`.
    pub(crate) burst_time: Duration,
}

impl ScreenEngine {
    pub fn new(supports: Vec<Box<dyn TuiSupport>>, exts: Vec<Box<dyn DrawExt>>) -> ScreenEngine {
        let mut scene = Scene::new();
        for ext in &exts {
            for (name, make) in ext.effects() {
                scene.register_effect(name, make);
            }
        }
        ScreenEngine {
            app: Vt::new(80, 24),
            host: Vt::new(80, 24),
            base: Grid::new(80, 24),
            painted: false,
            host_stale: false,
            frames: Frames::new(FrameConfig::default(), supports),
            scene,
            exts,
            waits: Vec::new(),
            last_change: Duration::ZERO,
            last_cursor: (0, 0),
            overlay: Overlay::Off,
            overlay_due: Duration::ZERO,
            injected: 0,
            bursts: 0,
            ticked: Duration::ZERO,
            ticks: 0,
            burst_time: Duration::ZERO,
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.app.resize(cols, rows);
        if !self.host_stale {
            self.host.resize(cols, rows);
        }
        // The host model is resized as the app model: crop and pad, which
        // is what hosts do on the alternate screen. Sprite cells stay known
        // and are repaired from there. A host that reflows its main screen
        // is out of reach of both models until the app redraws.
        self.app.take_events();
        self.host.take_events();
        self.base = self.app.grid().clone();
    }

    /// Feed the frames with the app screen as it is now.
    fn observe(&mut self, events: &[VtEvent], now: Duration, ctx: &mut Ctx) {
        let input = ScreenInput {
            grid: self.app.grid(),
            cursor: self.app.cursor(),
            alt_screen: self.app.modes().alt_screen,
            title: self.app.title(),
            events,
        };
        let changes = self.frames.update(&input, now);
        self.shift_sprites(&changes);
        if !changes.is_empty() && ctx.has_subscribers("frames") {
            ctx.emit("frames", json!({"frames": self.frames.list()}));
        }
    }

    /// Frame rows that moved in frame space take their sprites along.
    fn shift_sprites(&mut self, changes: &[ptyrc_core::frames::FrameEvent]) {
        for change in changes {
            if let ptyrc_core::frames::FrameEvent::Shift {
                frame,
                from,
                to,
                dy,
            } = *change
            {
                self.scene.shift(frame, from, to, dy);
            }
        }
    }

    /// Steps 4 and 5: tick the scene, compose, repair the host.
    fn paint(&mut self, now: Duration, out: &mut Vec<u8>) {
        if self.overlay != Overlay::Off && now >= self.overlay_due {
            self.overlay_due = now + DEBUG_PERIOD;
            rpc::refresh_overlay(self, now);
        }
        self.scene.tick(now, &self.frames);
        self.scene.take_dirty();
        if self.scene.is_empty() && !self.painted {
            return;
        }
        self.fresh_host();
        if !self.app.in_ground() || self.host.grid().cols() != self.app.grid().cols() {
            return;
        }
        let before = out.len();
        if self.scene.is_empty() {
            repair(&mut self.host, self.app.grid(), out);
            self.painted = false;
        } else {
            let desired = self.scene.compose(self.app.grid(), &self.frames);
            repair(&mut self.host, &desired, out);
            self.painted = true;
        }
        self.host.take_events();
        self.injected += (out.len() - before) as u64;
    }

    /// Bring the host model up to date after a time without sprites.
    fn fresh_host(&mut self) {
        if self.host_stale {
            self.host.clone_from(&self.app);
            self.host.take_events();
            self.host_stale = false;
        }
    }

    /// The model of the host: the app's while no sprite is up.
    pub(crate) fn host(&self) -> &Vt {
        if self.host_stale {
            &self.app
        } else {
            &self.host
        }
    }

    /// Write bytes of our own to the host.
    fn inject(&mut self, bytes: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(bytes);
        self.host.feed(bytes);
        self.injected += bytes.len() as u64;
    }

    /// End a synchronized update we opened and the app does not hold.
    fn close_sync(&mut self, out: &mut Vec<u8>) {
        if !self.host_stale
            && self.host.modes().sync_update
            && !self.app.modes().sync_update
            && self.app.in_ground()
        {
            self.inject(ESU, out);
        }
    }

    /// Answer the waits that are satisfied or expired.
    fn settle_waits(&mut self, now: Duration, ctx: &mut Ctx) {
        if self.waits.is_empty() {
            return;
        }
        let waits = std::mem::take(&mut self.waits);
        for wait in waits {
            match rpc::check_wait(self, &wait, now) {
                Some(result) => ctx.complete(wait.token, Ok(result)),
                None if now >= wait.deadline => {
                    ctx.complete(wait.token, Ok(json!({"matched": false})))
                }
                None => self.waits.push(wait),
            }
        }
    }

    fn emit_changes(&mut self, changed_rows: Vec<u16>, ctx: &mut Ctx) {
        if !changed_rows.is_empty() {
            ctx.emit("screen", json!({"rows": changed_rows}));
        }
        let cursor = self.app.cursor();
        if (cursor.col, cursor.row) != self.last_cursor {
            self.last_cursor = (cursor.col, cursor.row);
            ctx.emit(
                "cursor",
                json!({"x": cursor.col, "y": cursor.row, "visible": cursor.visible}),
            );
        }
    }

    pub(crate) fn no_frame(sel: impl std::fmt::Display) -> RpcError {
        RpcError::new(ErrorCode::NoFrame, format!("no such frame: {sel}"))
    }
}

impl Engine for ScreenEngine {
    fn on_start(&mut self, ctx: &mut Ctx) {
        let (cols, rows) = (ctx.info().cols, ctx.info().rows);
        self.resize(cols, rows);
        let now = ctx.now();
        self.observe(&[], now, ctx);
    }

    fn on_burst(&mut self, burst: &[u8], now: Duration, out: &mut Vec<u8>, ctx: &mut Ctx) {
        self.bursts += 1;
        let started = std::time::Instant::now();
        let sprites = self.painted || !self.scene.is_empty();
        if sprites {
            self.fresh_host();
        } else {
            // No sprite: the host model is the app model, fed once.
            self.host_stale = true;
        }
        self.app.feed(burst);
        let events = self.app.take_events();
        let to_scrollback = events.iter().any(|e| {
            matches!(
                e,
                VtEvent::Scroll {
                    scrollback: true,
                    ..
                }
            )
        });
        let mut tail: &[u8] = &[];
        if !sprites {
            out.extend_from_slice(burst);
        } else {
            // With sprites up, the burst and its repair are one synchronized
            // update: the host never shows the app's redraw without them.
            let can_inject = self.host.in_ground();
            if sprites && can_inject && self.app.in_ground() && !self.host.modes().sync_update {
                self.inject(BSU, out);
            }
            if self.painted && to_scrollback && can_inject {
                // Sprites never enter the host scrollback.
                let before = out.len();
                repair(&mut self.host, &self.base, out);
                self.injected += (out.len() - before) as u64;
                self.painted = false;
            }
            // An app that ends its own synchronized update with the burst:
            // the repair goes before that end, inside the app's update.
            let mut body = burst;
            if sprites && burst.ends_with(ESU) && !self.app.modes().sync_update {
                body = &burst[..burst.len() - ESU.len()];
                tail = ESU;
            }
            out.extend_from_slice(body);
            self.host.feed(body);
            let closes = self.host.in_ground() && self.host.modes().sync_update;
            if !tail.is_empty() && !closes {
                // Not the end of an open update: no split.
                out.extend_from_slice(tail);
                self.host.feed(tail);
                tail = &[];
            }
            self.host.take_events();
        }

        let grid = self.app.grid();
        let changed_rows: Vec<u16> =
            if grid.rows() == self.base.rows() && grid.cols() == self.base.cols() {
                (0..grid.rows())
                    .filter(|&r| grid.row(r) != self.base.row(r))
                    .collect()
            } else {
                (0..grid.rows()).collect()
            };
        if !changed_rows.is_empty() {
            self.last_change = now;
        }
        self.observe(&events, now, ctx);
        self.paint(now, out);
        if !tail.is_empty() {
            out.extend_from_slice(tail);
            self.fresh_host();
            self.host.feed(tail);
        }
        self.close_sync(out);
        if self.base.cols() != self.app.grid().cols() || self.base.rows() != self.app.grid().rows()
        {
            self.base.clone_from(self.app.grid());
        } else {
            for &row in &changed_rows {
                self.base
                    .row_mut(row)
                    .clone_from_slice(self.app.grid().row(row));
            }
        }
        self.emit_changes(changed_rows, ctx);
        self.settle_waits(now, ctx);
        self.burst_time += started.elapsed();
    }

    fn on_tick(&mut self, now: Duration, out: &mut Vec<u8>, ctx: &mut Ctx) {
        self.ticked = now;
        self.ticks += 1;
        let changes = self.frames.tick(now);
        self.shift_sprites(&changes);
        if !changes.is_empty() && ctx.has_subscribers("frames") {
            ctx.emit("frames", json!({"frames": self.frames.list()}));
        }
        self.paint(now, out);
        self.close_sync(out);
        self.settle_waits(now, ctx);
    }

    fn next_wake(&self, _now: Duration) -> Option<Duration> {
        let mut wake = self.scene.next_wake();
        let mut merge = |t: Option<Duration>| {
            if let Some(t) = t {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
        };
        merge(self.frames.next_wake());
        if self.overlay != Overlay::Off {
            merge(Some(self.overlay_due));
        }
        for wait in &self.waits {
            merge(Some(wait.deadline));
            if let Some(stable) = wait.stable {
                merge(Some(self.last_change + stable));
            }
        }
        // A deadline the last tick did not clear must not spin the loop.
        wake.map(|w| {
            if w <= self.ticked {
                self.ticked + Duration::from_millis(10)
            } else {
                w
            }
        })
    }

    fn on_resize(&mut self, cols: u16, rows: u16, now: Duration, out: &mut Vec<u8>, ctx: &mut Ctx) {
        self.resize(cols, rows);
        self.last_change = now;
        self.observe(&[], now, ctx);
        self.paint(now, out);
        self.close_sync(out);
    }

    fn on_exit(&mut self, out: &mut Vec<u8>, _ctx: &mut Ctx) {
        self.fresh_host();
        if !self.host.in_ground() {
            // The app died inside a sequence: CAN ends it on the host.
            self.app.feed(b"\x18");
            self.inject(b"\x18", out);
        }
        if self.painted {
            repair(&mut self.host, self.app.grid(), out);
            self.painted = false;
        }
        if self.host.modes().sync_update {
            // Ours or the app's: the host is not left frozen.
            self.inject(ESU, out);
        }
    }

    fn handle(
        &mut self,
        conn: ConnId,
        token: Token,
        method: &str,
        params: Value,
        ctx: &mut Ctx,
    ) -> Reply {
        rpc::handle(self, conn, token, method, params, ctx)
    }

    fn on_disconnect(&mut self, conn: ConnId, ctx: &mut Ctx) {
        self.waits.retain(|w| w.conn != conn);
        self.scene.drop_owner(conn.0);
        ctx.request_repaint();
    }

    fn app_cursor_keys(&self) -> bool {
        self.app.modes().app_cursor
    }

    fn bracketed_paste(&self) -> bool {
        self.app.modes().bracketed_paste
    }

    fn session_extras(&self) -> Value {
        json!({
            "alt_screen": self.app.modes().alt_screen,
            "title": self.app.title(),
        })
    }
}
