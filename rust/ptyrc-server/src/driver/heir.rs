// SPDX-License-Identifier: GPL-3.0-or-later
//! The surface follows its pane: when the frame of an app's surface
//! leaves the screen, the pane that took its place is its frame
//! (`heir`); its sprites and widgets go along; the app is told (`move`,
//! `size`; each widget in `widgets`).
//! So when the user moves it (`app.move`). A surface: of a store app;
//! of a native app below `layout`, the frame the user gave it.

use super::Driver;
use crate::apps::App;
use crate::engine::{ConnId, Ctx, Effects, Engine, Reply, Token};
use crate::grants::{Degree, Surface};
use serde_json::{Value, json};
use std::collections::HashMap;

/// What the driver keeps to follow the panes.
#[derive(Default)]
pub(super) struct Follow {
    /// The frames as last told by the engine, not yet looked at.
    pub seen: Option<Value>,
    /// Of each app: the info of its frame and the size of the screen,
    /// when both were last stable.
    settled: HashMap<u64, (Value, [f64; 2])>,
}

fn rect(info: &Value) -> Option<[f64; 4]> {
    let r = info.get("rect")?.as_array()?;
    let v = |i: usize| r.get(i)?.as_f64();
    Some([v(0)?, v(1)?, v(2)?, v(3)?])
}

/// A native app narrowed below `layout`: it knows one frame, so a frame
/// the user gives it is its surface.
fn blind(app: &App) -> bool {
    app.store().is_none() && app.grants.narrowed() && app.grants.degree() < Degree::Layout
}

/// The surface of `app`: of a store app; of a native app while it is
/// narrowed below `layout`. An entry kept from before (restarted with
/// declarations that raise it) is not one.
pub(super) fn surface(app: &App) -> Option<Surface> {
    app.surface.filter(|_| app.store().is_some() || blind(app))
}

fn size(info: &Value) -> Option<[f64; 2]> {
    rect(info).map(|r| [r[2], r[3]])
}

/// Of `frames`, the pane that took the place of `gone`: usable, of the
/// same key, over at least half of the smaller of the two; the one that
/// overlaps most. `scale`: how much the screen grew since `gone` was
/// seen, across and down. The rule of `_cli.heir`.
pub(super) fn heir(gone: &Value, frames: &[Value], scale: [f64; 2]) -> Option<u32> {
    let [x, y, w, h] = rect(gone)?;
    let (x, y, w, h) = (x * scale[0], y * scale[1], w * scale[0], h * scale[1]);
    let shared = |pane: &Value| {
        let Some([px, py, pw, ph]) = rect(pane) else {
            return 0.0;
        };
        let across = (x + w).min(px + pw) - x.max(px);
        let down = (y + h).min(py + ph) - y.max(py);
        across.max(0.0) * down.max(0.0)
    };
    let heirs = frames.iter().filter(|pane| {
        pane["id"] != gone["id"]
            && pane["kind"] == "pane"
            && pane["usable"] == true
            && pane["key"] == gone["key"]
            && size(pane).is_some_and(|[pw, ph]| 2.0 * shared(pane) >= (w * h).min(pw * ph))
    });
    let best = heirs.max_by(|a, b| shared(a).total_cmp(&shared(b)))?;
    best["id"].as_u64().and_then(|id| u32::try_from(id).ok())
}

impl Driver {
    /// The frames the engine told last: each store app whose frame left
    /// the screen is moved to its heir.
    pub(super) fn follow_panes(&mut self, engine: &mut dyn Engine) {
        let Some(seen) = self.follow.seen.take() else {
            return;
        };
        let frames = seen["frames"].as_array().cloned().unwrap_or_default();
        let raw = frames.iter().find(|f| f["kind"] == "raw");
        let screen = raw.and_then(size);
        let stable = |f: &Value| f["state"] == "stable";
        let surfaces: Vec<(u64, u32)> = (self.apps.iter())
            .flat_map(|apps| apps.iter())
            .filter_map(|app| match surface(app)? {
                Surface::Frame(frame) | Surface::Screen(frame) | Surface::Widget(frame)
                    if frame != 0 =>
                {
                    Some((app.id, frame))
                }
                _ => None,
            })
            .collect();
        let mut moves = Vec::new();
        for (app, frame) in surfaces {
            let mine = frames.iter().find(|f| f["id"] == frame);
            if let Some(mine) = mine.filter(|f| f["state"] != "inactive") {
                if let Some(screen) = screen.filter(|_| raw.is_some_and(stable) && stable(mine)) {
                    self.follow.settled.insert(app, (mine.clone(), screen));
                }
                continue;
            }
            let (gone, scale) = match self.follow.settled.get(&app) {
                Some((info, [w, h])) => {
                    let [sw, sh] = screen.unwrap_or([*w, *h]);
                    (info.clone(), [sw / w.max(1.0), sh / h.max(1.0)])
                }
                None => match mine {
                    Some(mine) => (mine.clone(), [1.0, 1.0]),
                    None => continue,
                },
            };
            if let Some(new) = heir(&gone, &frames, scale) {
                let info = frames.iter().find(|f| f["id"] == new).cloned();
                if let (Some(info), Some(screen)) = (info, screen) {
                    self.follow.settled.insert(app, (info, screen));
                }
                moves.push((app, frame, new));
            }
        }
        for (app, from, to) in moves {
            self.rehome(app, from, to, engine);
        }
        self.follow
            .settled
            .retain(|id, _| (self.apps.as_ref()).is_some_and(|apps| apps.get(*id).is_some()));
    }

    /// The frame the user gave `app` (`app.start {frame}`,
    /// `app.restart {frame}`): its surface, of a store app or of a
    /// native app narrowed below `layout`; for another, nothing.
    pub(super) fn give_frame(&mut self, app: u64, frame: u32) {
        let Some(apps) = self.apps.as_mut() else {
            return;
        };
        let Some(entry) = apps.get(app) else {
            return;
        };
        match surface(entry) {
            Some(surface) => apps.set_surface(app, surface.moved(frame)),
            None if blind(entry) => apps.set_surface(app, Surface::Frame(frame)),
            None => {}
        }
    }

    /// The frame of `app`'s surface when `move_surface` moves it: a
    /// surface on a frame, that of its widgets, or a blind app (all it
    /// names on frame 0).
    pub(super) fn surface_frame(&self, app: u64) -> Option<u32> {
        let entry = self.apps.as_ref()?.get(app)?;
        match surface(entry) {
            Some(Surface::Frame(frame) | Surface::Screen(frame) | Surface::Widget(frame)) => {
                Some(frame)
            }
            // Its frame so far: the one all it names is put on.
            None if blind(entry) => Some(0),
            None => None,
        }
    }

    /// The event `move` to `conn`: the frame `to`; 0 below `layout`, the
    /// one frame such an app knows (as `frame.get`, `draw.list`).
    pub(super) fn send_move(&mut self, conn: ConnId, to: u32) {
        let blind = self.own(conn).is_some_and(|o| o.degree < Degree::Layout);
        let frame = if blind { 0 } else { to };
        self.send_event(conn, "move", json!({"frame": frame}));
    }

    /// `app.move` by the user: as `give_frame`, its sprites going along
    /// as to an heir. True: it has a surface, moved.
    pub(super) fn move_surface(&mut self, app: u64, to: u32, engine: &mut dyn Engine) -> bool {
        let Some(from) = self.surface_frame(app) else {
            return false;
        };
        self.give_frame(app, from);
        self.rehome(app, from, to, engine);
        true
    }

    /// The surface of `app` from frame `from` to `to`: its sprites
    /// there, or shown nowhere, and its widgets there go along; it is
    /// told. Of `surface = "widget"`: its widgets alone; below `layout`,
    /// nothing told but its widgets in `widgets`, in their own cells.
    fn rehome(&mut self, app: u64, from: u32, to: u32, engine: &mut dyn Engine) {
        let Some(apps) = self.apps.as_mut() else {
            return;
        };
        let Some(entry) = apps.get(app) else {
            return;
        };
        let (Some(surface), conns) = (surface(entry), entry.conns.clone()) else {
            return;
        };
        let widget = matches!(surface, Surface::Widget(_));
        let quiet = widget && entry.grants.degree() < Degree::Layout;
        apps.set_surface(app, surface.moved(to));
        let now = self.now();
        let Some(&first) = conns.iter().next() else {
            return;
        };
        let mut call = |driver: &mut Driver, method: &str, params: Value| {
            let token = Token(driver.next_token);
            driver.next_token += 1;
            hook!(driver, engine, now, |ctx| engine
                .handle(first, token, method, params, ctx))
        };
        let listed = match (!widget).then(|| call(self, "draw.list", json!({}))) {
            Some(Reply::Ok(list)) => list["sprites"].as_array().cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        let mine =
            |id: u64| (conns.iter()).any(|c| self.drawn.get(c).is_some_and(|d| d.contains(&id)));
        let there = |f: &Value| {
            f.as_u64()
                .is_some_and(|f| f == u64::from(from) || f == u64::from(crate::rpc::NOWHERE))
        };
        let ids: Vec<u64> = (listed.iter())
            .filter(|s| there(&s["frame"]))
            .filter_map(|s| s["id"].as_u64())
            .filter(|id| mine(*id))
            .collect();
        for id in ids {
            call(self, "draw.update", json!({"id": id, "frame": to}));
        }
        // Its widgets, by the same rule: each is told in `widgets`.
        for &conn in &conns {
            hook!(self, engine, now, |ctx| engine
                .move_widgets(conn, from, to, ctx));
        }
        let size = json!({"size": [self.info.cols, self.info.rows]});
        for conn in conns.into_iter().filter(|_| !quiet) {
            let topics = self.conns.get(&conn).map(|c| c.topics.clone());
            let topics = topics.unwrap_or_default();
            if topics.contains("move") {
                self.send_move(conn, to);
            }
            if topics.contains("size") {
                self.send_event(conn, "size", size.clone());
            }
        }
        self.repaint = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u32, rect: [u16; 4]) -> Value {
        json!({"id": id, "kind": "pane", "usable": true, "key": null, "rect": rect,
               "state": "stable"})
    }

    #[test]
    fn the_heir_is_the_pane_that_took_the_place() {
        let gone = pane(1, [0, 0, 19, 16]);
        let frames = [pane(2, [0, 0, 39, 16]), pane(3, [40, 0, 20, 16])];
        assert_eq!(heir(&gone, &frames, [1.0, 1.0]), Some(2));
        // Far from it: none.
        assert_eq!(heir(&gone, &frames[1..], [1.0, 1.0]), None);
        // Another key: none.
        let mut other = pane(2, [0, 0, 39, 16]);
        other["key"] = json!("w2");
        assert_eq!(heir(&gone, &[other], [1.0, 1.0]), None);
        // Not usable: none.
        let mut hidden = pane(2, [0, 0, 39, 16]);
        hidden["usable"] = json!(false);
        assert_eq!(heir(&gone, &[hidden], [1.0, 1.0]), None);
        // The screen doubled across: the place is looked for in proportion.
        let gone = pane(1, [20, 0, 20, 16]);
        let frames = [pane(2, [0, 0, 39, 16]), pane(3, [40, 0, 40, 16])];
        assert_eq!(heir(&gone, &frames, [1.0, 1.0]), Some(2));
        assert_eq!(heir(&gone, &frames, [2.0, 1.0]), Some(3));
    }
}
