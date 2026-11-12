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
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.app.resize(cols, rows);
        self.host.resize(cols, rows);
        // What the host did with our sprites on resize is unknown: forget
        // them as painted.
        self.host.set_grid(self.app.grid());
        self.app.take_events();
        self.host.take_events();
        self.base = self.app.grid().clone();
        self.painted = false;
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
        if self.painted && to_scrollback {
            // Sprites never enter the host scrollback.
            let before = out.len();
            repair(&mut self.host, &self.base, out);
            self.injected += (out.len() - before) as u64;
            self.painted = false;
        }
        out.extend_from_slice(burst);
        self.host.feed(burst);
        self.host.take_events();

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
        if !changed_rows.is_empty() {
            self.base.clone_from(self.app.grid());
        }
        self.emit_changes(changed_rows, ctx);
        self.settle_waits(now, ctx);
    }

    fn on_tick(&mut self, now: Duration, out: &mut Vec<u8>, ctx: &mut Ctx) {
        self.ticked = now;
        let changes = self.frames.tick(now);
        self.shift_sprites(&changes);
        if !changes.is_empty() && ctx.has_subscribers("frames") {
            ctx.emit("frames", json!({"frames": self.frames.list()}));
        }
        self.paint(now, out);
        self.settle_waits(now, ctx);
    }

    fn next_wake(&self, now: Duration) -> Option<Duration> {
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
    }

    fn on_exit(&mut self, out: &mut Vec<u8>, _ctx: &mut Ctx) {
        if self.painted && self.app.in_ground() {
            repair(&mut self.host, self.app.grid(), out);
            self.painted = false;
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
