// SPDX-License-Identifier: GPL-3.0-or-later
//! An app sees its own (`plan/store-model.md`, principle 5): for a
//! connection bound to an app that does not hold the whole session,
//! the lists hold its own objects alone, and an id of another's is
//! answered as one that does not exist. A store app, besides: its own
//! band of layers, behind unless it holds `front`; the limits of the
//! driver.

use super::Driver;
use crate::engine::{ConnId, Engine};
use crate::grants::{Degree, Permission};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

/// Frame names resolved by the cursor or the heat: of `shape`.
const OF_SHAPE: [&str; 3] = ["active", "quietest", "idle"];

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
        })
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
        // permission, else behind (the lesser form).
        if own.store {
            let asked = object.get("layer").filter(|l| !l.is_null()).cloned();
            let front = asked.as_ref().is_some_and(in_front);
            let modal = object.get("modal") == Some(&Value::Bool(true));
            let layer = match method {
                "draw.put" => Some(band(own.app, front && own.front)),
                "draw.ext" if !object.contains_key("id") => Some(band(own.app, front && own.front)),
                "draw.update" | "draw.ext" if asked.is_some() => {
                    Some(band(own.app, front && own.front))
                }
                "widget.show" if asked.is_some() || modal => {
                    Some(band(own.app, (front || modal) && own.front_widget))
                }
                _ => None,
            };
            if let Some(layer) = layer {
                object.insert("layer".into(), json!(layer));
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
}
