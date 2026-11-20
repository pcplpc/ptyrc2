// SPDX-License-Identifier: GPL-3.0-or-later
//! An app sees its own (`plan/store-model.md`, principle 5): for a
//! connection bound to an app that does not hold the whole session,
//! the lists hold its own objects alone, and an id of another's is
//! answered as one that does not exist. A store app, besides: its own
//! band of layers, behind unless it holds `front`; its surface (a
//! draw put on its frame, or nowhere); the limits of the driver.

use super::Driver;
use crate::apps::Given;
use crate::engine::{ConnId, Engine};
use crate::grants::{Degree, Permission, Surface};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

/// Frame names resolved by the cursor or the heat: of `shape`.
const OF_SHAPE: [&str; 3] = ["active", "quietest", "idle"];

/// Layers of the top band, `hud` and above: the driver's.
const TOP_BAND: i64 = 100;

/// Lines of the double border, of the driver's pop-up.
const DOUBLE: &str = "╔╗╚╝═║╠╣╦╩╬";

/// A border on the wire that only the driver may show: `alert`; double
/// lines on a background, the look of its pop-up.
fn reserved(border: &Value) -> bool {
    let named = |name: Option<&str>| matches!(name, Some("alert"));
    let Some(object) = border.as_object() else {
        return named(border.as_str());
    };
    let base = object.get("base").and_then(Value::as_str);
    let lines = ["corners", "top", "bottom", "left", "right"];
    let given = lines.iter().filter_map(|m| object.get(*m)?.as_str());
    let double =
        base == Some("double") || given.clone().any(|l| l.chars().any(|c| DOUBLE.contains(c)));
    let filled = |style: Option<&Value>| {
        style.is_some_and(|s| {
            s.get("bg").is_some_and(|b| !b.is_null())
                || s.get("inverse") == Some(&Value::Bool(true))
        })
    };
    let title = object.get("title").and_then(|t| t.get("style"));
    named(base) || (double && (filled(object.get("style")) || filled(title)))
}

/// The limits of the driver for a store app, all its connections.
pub(super) const MAX_SPRITES: usize = 256;
pub(super) const MAX_WIDGETS: usize = 16;
pub(super) const MAX_TOPICS: usize = 32;

/// Of a store app: its layer, one band of its own between those of the
/// driver: behind, between `wallpaper` and `back` (-98 to -2); in
/// front, between `front` and `hud` (2 to 98).
pub(super) fn band(app: u64, front: bool) -> i32 {
    let band = 2 + (app % 97) as i32;
    if front { band } else { -band }
}

/// A layer on the wire asks to be in front of the text.
fn in_front(layer: &Value) -> bool {
    match layer {
        Value::String(name) => matches!(name.as_str(), "front" | "hud"),
        layer => layer.as_i64().is_some_and(|z| z > 0),
    }
}

fn limit(method: &str, what: &str, max: usize) -> RpcError {
    RpcError::new(
        ErrorCode::NotUsable,
        format!("{method}: {max} {what} at most for a store app"),
    )
    .with_hint(format!(
        "the limit of {what} of the driver ({max}): remove one first"
    ))
}

/// What a narrowed connection may see of the table: its app, its
/// connections, its degree.
pub(super) struct Own {
    pub app: u64,
    pub conns: BTreeSet<ConnId>,
    pub degree: Degree,
    /// A store app: its band, the limits.
    pub store: bool,
    pub front: bool,
    pub front_widget: bool,
    pub surface: Option<Surface>,
    pub screen_wide: bool,
    /// The launcher's options of a store app.
    pub given: Given,
}

impl Own {
    /// The frame of its surface, when it has one.
    pub fn frame(&self) -> Option<u32> {
        match self.surface? {
            Surface::Frame(frame) | Surface::Screen(frame) => Some(frame),
            Surface::Widget => None,
        }
    }

    /// The frame a draw of a store app is put on, whatever it named:
    /// its frame; for a bare sprite of `widget`, nowhere. None: as
    /// named.
    fn clip(&self, widget: bool) -> Option<Value> {
        match self.surface? {
            Surface::Widget if widget => None,
            Surface::Widget => Some(json!(crate::rpc::NOWHERE)),
            Surface::Frame(frame) => Some(json!(frame)),
            Surface::Screen(_) if self.screen_wide => None,
            Surface::Screen(frame) => Some(json!(frame)),
        }
    }
}

fn absent(id: &Value) -> RpcError {
    RpcError::new(ErrorCode::NoSprite, format!("no such sprite: {id}"))
}

impl Driver {
    /// `Some` when `conn` is bound to an app that does not hold the
    /// whole session.
    pub(super) fn own(&self, conn: ConnId) -> Option<Own> {
        let id = self.conns.get(&conn)?.app?;
        let app = self.apps.as_ref()?.get(id)?;
        app.grants.narrowed().then(|| Own {
            app: id,
            conns: app.conns.clone(),
            degree: app.grants.degree(),
            store: app.store().is_some(),
            front: app.grants.holds(Permission::Front),
            front_widget: (app.grants.holds(Permission::Front)
                || app.grants.holds(Permission::FrontWidget)),
            surface: app.surface,
            screen_wide: app.grants.holds(Permission::ScreenWide),
            given: Given {
                files: Vec::new(),
                ..app.given.clone()
            },
        })
    }

    /// The file `handle` of the app `conn` is bound to: its path, its
    /// kind, whether it may be written. `None`: no app, or not one of
    /// its handles. For `file.*`.
    pub(super) fn file_of(
        &self,
        conn: ConnId,
        handle: &str,
    ) -> Option<(std::path::PathBuf, ptyrc_store::FileKind, bool)> {
        let id = self.conns.get(&conn)?.app?;
        let named = self.apps.as_ref()?.get(id)?.file(handle)?;
        Some((named.path.to_path_buf(), named.kind, named.writable))
    }

    /// Before the request: a sprite of another, absent; `draw.clear
    /// {all}`, its own; a frame it may not resolve, the surface.
    pub(super) fn own_params(
        &self,
        own: &Own,
        method: &str,
        params: &mut Value,
    ) -> Result<(), RpcError> {
        let by_id = matches!(method, "draw.update" | "draw.remove")
            || (method == "draw.ext" && params.get("id").is_some());
        if by_id {
            let id = params.get("id").cloned().unwrap_or(Value::Null);
            let mine = id.as_u64().is_some_and(|id| {
                (own.conns.iter()).any(|c| self.drawn.get(c).is_some_and(|d| d.contains(&id)))
            });
            if !mine && !id.is_null() {
                return Err(absent(&id));
            }
        }
        let Some(object) = params.as_object_mut() else {
            return Ok(());
        };
        if method == "draw.clear" {
            object.remove("all");
        }
        // A store app names no layer: its band; in front with the
        // permission, else behind (the lesser form); `--front`,
        // `--back`: as the user said.
        if own.store {
            let asked = object.get("layer").filter(|l| !l.is_null()).cloned();
            let front = asked.as_ref().is_some_and(in_front);
            let modal = object.get("modal") == Some(&Value::Bool(true));
            let by_hand = own.given.front;
            let new = matches!(method, "draw.put")
                || (method == "draw.ext" && !object.contains_key("id"));
            let layer = match method {
                _ if new => Some(by_hand.unwrap_or(front && own.front)),
                "draw.update" | "draw.ext" if asked.is_some() => {
                    Some(by_hand.unwrap_or(front && own.front))
                }
                "widget.show" if by_hand.is_some() => by_hand,
                "widget.show" if asked.is_some() || modal => {
                    Some((front || modal) && own.front_widget)
                }
                // Not raised by the focus above the text.
                "widget.show" if !own.front_widget => Some(false),
                _ => None,
            };
            if let Some(front) = layer {
                object.insert("layer".into(), json!(band(own.app, front)));
            }
            // `--alpha`: the opacity of its sprites, multiplied.
            if let Some(alpha) = own.given.alpha {
                let asked = object.get("alpha").and_then(Value::as_f64);
                if new || (asked.is_some() && matches!(method, "draw.update" | "draw.ext")) {
                    object.insert("alpha".into(), json!(alpha * asked.unwrap_or(1.0)));
                }
            }
            // `--prefer`: of its widgets, of an anchor placed by the heat.
            if let Some(prefer) = &own.given.prefer {
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
            // `--border`: of its widgets.
            if let Some(border) = &own.given.border {
                let declared = method == "widget.declare" && object.contains_key("border");
                if method == "widget.show" || declared {
                    object.insert("border".into(), json!(border));
                }
            }
        }
        // The driver's looks: lowered, not refused.
        if let Some(border) = object.get_mut("border").filter(|b| reserved(b)) {
            *border = json!("plain");
        }
        if !own.store
            && matches!(
                method,
                "draw.put" | "draw.ext" | "draw.update" | "widget.show"
            )
        {
            let top =
                |l: &Value| l.as_str() == Some("hud") || l.as_i64().is_some_and(|z| z >= TOP_BAND);
            let modal = object.get("modal") == Some(&Value::Bool(true));
            let layer = object.get("layer").filter(|l| !l.is_null());
            if layer.is_some_and(top) || (method == "widget.show" && modal && layer.is_none()) {
                object.insert("layer".into(), json!(TOP_BAND - 1));
            }
        }
        // Below `layout` one frame, the surface; below `shape` no name
        // that the cursor or the heat resolves.
        let member = if method == "frame.at" {
            "name"
        } else {
            "frame"
        };
        if let Some(frame) = object.get_mut(member).filter(|f| !f.is_null()) {
            let of_shape = frame.as_str().is_some_and(|f| OF_SHAPE.contains(&f));
            if own.degree < Degree::Layout || (own.degree < Degree::Shape && of_shape) {
                *frame = match member {
                    "name" => json!("raw"),
                    _ => json!(0),
                };
            }
        }
        // Its surface: clipped, never refused, nothing told.
        if own.store {
            let named = object.get("frame").is_some_and(|f| !f.is_null());
            let clip = match method {
                "draw.put" => own.clip(false),
                "draw.ext" if !object.contains_key("id") => own.clip(false),
                "draw.update" | "draw.ext" if named => own.clip(false),
                "widget.show" => own.clip(true),
                _ => None,
            };
            if let Some(frame) = clip {
                object.insert("frame".into(), frame);
            }
        }
        Ok(())
    }

    /// The limits of a store app: sprites, widgets, topics.
    pub(super) fn own_limits(
        &self,
        conn: ConnId,
        own: &Own,
        method: &str,
        params: &Value,
        engine: &mut dyn Engine,
        now: Duration,
    ) -> Result<(), RpcError> {
        if !own.store {
            return Ok(());
        }
        match method {
            "draw.put" | "draw.ext" if params.get("id").is_none() => {
                let sprites: usize = own.conns.iter().map(|c| engine.owned(*c).sprites).sum();
                if sprites >= MAX_SPRITES {
                    return Err(limit(method, "sprites", MAX_SPRITES));
                }
            }
            "widget.show" => {
                let tag = params.get("tag");
                let widgets = engine.snapshot("widgets", now);
                let mine = |w: &&Value| {
                    let owner = w.get("owner").and_then(Value::as_u64);
                    owner.is_some_and(|o| own.conns.contains(&ConnId(o)))
                };
                let mine: Vec<&Value> = widgets.iter().filter(mine).collect();
                let again = mine.iter().any(|w| w.get("tag") == tag);
                if !again && mine.len() >= MAX_WIDGETS {
                    return Err(limit(method, "widgets", MAX_WIDGETS));
                }
            }
            "subscribe" => {
                let of = |c: &ConnId| self.conns.get(c).map(|c| &c.topics);
                let held: usize = own.conns.iter().filter_map(of).map(|t| t.len()).sum();
                let asked = params.get("topics").and_then(Value::as_array);
                let asked = asked.into_iter().flatten().filter_map(Value::as_str);
                let mut new: Vec<&str> = asked
                    .filter(|t| !of(&conn).is_some_and(|have| have.contains(*t)))
                    .collect();
                new.sort_unstable();
                new.dedup();
                if held + new.len() > MAX_TOPICS {
                    return Err(limit(method, "subscriptions", MAX_TOPICS));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// After the request: the lists cut to its own; the sprites it
    /// drew noted.
    pub(super) fn own_answer(&mut self, conn: ConnId, own: &Own, method: &str, answer: &mut Value) {
        let mine = |item: &Value| {
            let owner = item.get("owner").and_then(Value::as_u64);
            owner.is_some_and(|o| own.conns.contains(&ConnId(o)))
        };
        let (member, keep): (&str, &dyn Fn(&Value) -> bool) = match method {
            "draw.list" => ("sprites", &mine),
            "widget.list" => ("widgets", &mine),
            "app.list" => ("apps", &|a: &Value| a["id"].as_u64() == Some(own.app)),
            "draw.put" | "draw.ext" => {
                if let Some(id) = answer.get("id").and_then(Value::as_u64) {
                    self.drawn.entry(conn).or_default().insert(id);
                }
                return;
            }
            _ => return,
        };
        if let Some(Value::Array(items)) = answer.get_mut(member) {
            items.retain(|item| keep(item));
        }
    }
}

/// Members of a report on a sprite for the driver alone.
const HIT: [&str; 3] = ["owner", "widget", "layer"];

impl Driver {
    /// A report of the host mouse on a sprite (`mouse`, `hover`) to
    /// `conn`: heard, and taken from the program. On its own widget or
    /// its own sprite in front of the text: by default. Else `pointer`
    /// (`hover`) or `clicks` (`mouse`) to hear it; `clicks-grab`
    /// besides to take it.
    fn pointer_heard(&self, conn: ConnId, topic: &str, data: &Value) -> (bool, bool) {
        let Some(id) = self.conns.get(&conn).and_then(|c| c.app) else {
            return (true, true);
        };
        let Some(app) = self.apps.as_ref().and_then(|apps| apps.get(id)) else {
            return (false, false);
        };
        if !app.grants.narrowed() {
            return (true, true);
        }
        let owner = data.get("owner").and_then(Value::as_u64);
        let own = owner.is_some_and(|o| app.conns.contains(&ConnId(o)));
        let widget = data.get("widget") == Some(&Value::Bool(true));
        let front = data
            .get("layer")
            .and_then(Value::as_i64)
            .is_some_and(|l| l > 0);
        if own && (widget || front) {
            return (true, true);
        }
        let needs = match topic {
            "hover" => Permission::Pointer,
            _ => Permission::Clicks,
        };
        let heard = app.grants.holds(needs);
        (heard, heard && app.grants.holds(Permission::ClicksGrab))
    }

    /// `topic` of a report on a sprite, or of a leave (no `owner`), to
    /// each listener that hears it; `also`: listeners that hear it
    /// whatever it is. True: taken from the program. The listeners of
    /// a `hover` are kept, for the leave.
    pub(super) fn emit_pointer(
        &mut self,
        topic: &str,
        data: Value,
        also: &BTreeSet<ConnId>,
    ) -> bool {
        let mut bare = data.clone();
        if let Some(object) = bare.as_object_mut() {
            HIT.iter().for_each(|m| drop(object.remove(*m)));
        }
        let message = ptyrc_proto::envelope::event(topic, bare);
        let listeners: Vec<ConnId> = (self.conns.iter())
            .filter(|(_, c)| c.topics.contains(topic) && !c.paused)
            .map(|(id, _)| *id)
            .collect();
        let mut taken = false;
        let mut heard = BTreeSet::new();
        for id in listeners {
            let (hears, takes) = self.pointer_heard(id, topic, &data);
            if hears || also.contains(&id) {
                heard.insert(id);
                taken |= hears && takes;
                if let Some(conn) = self.conns.get_mut(&id) {
                    conn.send(&message);
                }
            }
        }
        if topic == "hover" {
            self.hover_heard = heard;
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_of_its_own_between_the_drivers() {
        assert_eq!((band(1, false), band(1, true)), (-3, 3));
        for app in [0, 95, 96, 97, 1000] {
            assert!((-98..=-2).contains(&band(app, false)));
            assert!((2..=98).contains(&band(app, true)));
        }
        assert!(in_front(&json!("hud")) && in_front(&json!(5)));
        assert!(!in_front(&json!("back")) && !in_front(&json!(-5)) && !in_front(&json!("x")));
    }

    #[test]
    fn the_drivers_looks_are_reserved() {
        assert!(reserved(&json!("alert")) && reserved(&json!({"base": "alert"})));
        let blue = json!({"bg": 4, "fg": 15, "bold": true});
        assert!(reserved(&json!({"base": "double", "style": blue})));
        assert!(reserved(
            &json!({"corners": "╔╗╚╝", "style": {"inverse": true}})
        ));
        assert!(reserved(&json!({"top": "═", "title": {"style": blue}})));
        assert!(!reserved(&json!("double")) && !reserved(&json!("plain")));
        assert!(!reserved(&json!({"base": "double", "style": {"fg": 4}})));
        assert!(!reserved(&json!({"base": "heavy", "style": blue})));
    }
}
