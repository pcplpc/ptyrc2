// SPDX-License-Identifier: GPL-3.0-or-later
//! The deck of widgets, the twin of the script module `ptyrc/deck`: the
//! widgets kept drawn, and the pieces of an interactive one (`Keys`,
//! `Field`, `Listing`, `cut`, `fit`, `wrap`). See `README.md`, Deck.
//!
//! The deck holds the handlers, not the state: `Deck<S>` is given
//! `&mut S` at each `add`, `tick`, `event`, and passes it to them.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::json::{Map, Value};
use crate::{Canvas, Error, Run, attr, base64, call, cells, json};

/// Members of `widget.show` a deck passes on.
pub const SHOW: [&str; 8] = [
    "border", "title", "frame", "prefer", "fill", "margin", "layer", "modal",
];
// What of a widget's info a hover stands on.
const PLACE: [&str; 3] = ["sprite", "variant", "rect"];

fn error_of(e: &Value) -> Error {
    let text = |k: &str| e[k].as_str().map(str::to_string);
    Error {
        code: text("code").unwrap_or_else(|| "bad-answer".into()),
        message: text("message").unwrap_or_default(),
        hint: text("hint"),
    }
}

// ---- content ----

/// The content of a variant: rows of text (a space a hole), or rows of
/// runs (`cells`; a canvas goes so).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Rows(Vec<String>),
    Cells(Vec<Vec<Run>>),
}

impl Content {
    /// The `content` of `widget.declare`, `widget.set`.
    pub fn to_value(&self) -> Value {
        match self {
            Content::Rows(rows) => json!({ "rows": rows }),
            Content::Cells(rows) => {
                let rows: Vec<Value> = (rows.iter())
                    .map(|r| r.iter().map(Run::to_value).collect())
                    .collect();
                json!({ "cells": rows })
            }
        }
    }
}

impl From<&str> for Content {
    /// Lines by `\n`.
    fn from(text: &str) -> Content {
        Content::Rows(text.split('\n').map(str::to_string).collect())
    }
}

impl From<String> for Content {
    fn from(text: String) -> Content {
        Content::from(text.as_str())
    }
}

impl From<Vec<String>> for Content {
    fn from(rows: Vec<String>) -> Content {
        Content::Rows(rows)
    }
}

impl From<Vec<&str>> for Content {
    fn from(rows: Vec<&str>) -> Content {
        Content::Rows(rows.into_iter().map(str::to_string).collect())
    }
}

impl From<Vec<Vec<Run>>> for Content {
    fn from(rows: Vec<Vec<Run>>) -> Content {
        Content::Cells(rows)
    }
}

impl From<Vec<Run>> for Content {
    /// One row.
    fn from(row: Vec<Run>) -> Content {
        Content::Cells(vec![row])
    }
}

impl From<&mut Canvas> for Content {
    /// Its runs: `cells`.
    fn from(canvas: &mut Canvas) -> Content {
        Content::Cells(canvas.runs())
    }
}

/// The variants of a widget: `(w, h)` (named `WxH`) or `(name, w, h)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sizes(pub Vec<(Option<String>, u32, u32)>);

impl<const N: usize> From<[(u32, u32); N]> for Sizes {
    fn from(s: [(u32, u32); N]) -> Sizes {
        Sizes::from(&s[..])
    }
}

impl From<&[(u32, u32)]> for Sizes {
    fn from(s: &[(u32, u32)]) -> Sizes {
        Sizes(s.iter().map(|(w, h)| (None, *w, *h)).collect())
    }
}

impl<const N: usize> From<[(&str, u32, u32); N]> for Sizes {
    fn from(s: [(&str, u32, u32); N]) -> Sizes {
        Sizes::from(&s[..])
    }
}

impl From<&[(&str, u32, u32)]> for Sizes {
    fn from(s: &[(&str, u32, u32)]) -> Sizes {
        Sizes(
            s.iter()
                .map(|(n, w, h)| (Some(n.to_string()), *w, *h))
                .collect(),
        )
    }
}

// ---- widgets ----

/// A widget of a deck: its tag and what the driver last told of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub tag: String,
    /// The info of `widget.show`, then of each `widgets` event.
    pub info: Value,
    pub hidden: bool,
    /// The deck's `now` (session time of its last `tick`).
    pub now: u64,
}

impl Widget {
    pub fn id(&self) -> Option<u64> {
        self.info["id"].as_u64()
    }

    /// The name of the variant shown.
    pub fn variant(&self) -> Option<&str> {
        self.info["variant"].as_str()
    }

    /// The size of the variant shown.
    pub fn size(&self) -> Option<(u32, u32)> {
        let s = &self.info["size"];
        Some((s[0].as_u64()? as u32, s[1].as_u64()? as u32))
    }

    /// `[x, y, w, h]` of the box, border included.
    pub fn rect(&self) -> Option<[i64; 4]> {
        let r = &self.info["rect"];
        Some([
            r[0].as_i64()?,
            r[1].as_i64()?,
            r[2].as_i64()?,
            r[3].as_i64()?,
        ])
    }

    pub fn placed(&self) -> bool {
        self.info["state"] == "placed"
    }

    pub fn over(&self) -> bool {
        self.info["over"] == true
    }

    /// What the widget asks (`widget.ask`); `Null`: nothing.
    pub fn asks(&self) -> &Value {
        &self.info["asks"]
    }

    pub fn focused(&self) -> bool {
        self.info["focused"] == true
    }

    /// `widget.set`: a content, a variant (else the one shown), a title.
    pub fn set(
        &self,
        content: Option<Content>,
        variant: Option<&str>,
        title: Option<&str>,
    ) -> Result<(), Error> {
        let mut p = json!({ "id": (self.info["id"].clone()) });
        if let Some(v) = variant {
            p["variant"] = json!(v);
        }
        if let Some(c) = content {
            p["content"] = c.to_value();
        }
        if let Some(t) = title {
            p["title"] = json!(t);
        }
        call("widget.set", p).map(drop)
    }

    /// `widget.hide`; gone already: fine.
    pub fn hide(&mut self) -> Result<(), Error> {
        let id = self.info["id"].clone();
        gone_is_fine(call("widget.hide", json!({ "id": id })))?;
        for k in ["variant", "size", "rect", "sprite"] {
            self.info[k] = Value::Null;
        }
        self.info["state"] = json!("hidden");
        self.hidden = true;
        Ok(())
    }

    /// `widget.ask`: `opts` of `quiet`, `escape`, `close`.
    pub fn ask(&mut self, opts: &Value) -> Result<(), Error> {
        let mut p = json!({ "id": (self.info["id"].clone()) });
        for k in ["quiet", "escape", "close"] {
            if !opts[k].is_null() {
                p[k] = opts[k].clone();
            }
        }
        let mut answer = call("widget.ask", p)?;
        if answer["widget"].as_object().is_some() {
            self.info = answer["widget"].take();
        }
        Ok(())
    }

    /// `widget.release`: the keys given back; gone already: fine.
    pub fn release(&mut self) -> Result<(), Error> {
        let id = self.info["id"].clone();
        gone_is_fine(call("widget.release", json!({ "id": id })))?;
        self.info["asks"] = Value::Null;
        self.info["focused"] = json!(false);
        Ok(())
    }
}

fn gone_is_fine(r: Result<Value, Error>) -> Result<(), Error> {
    match r {
        Err(e) if e.code != "no-sprite" && e.code != "gone" => Err(e),
        _ => Ok(()),
    }
}

type Render<S> = Box<dyn FnMut(&mut S, u32, u32, &Widget) -> Content>;
type OnText<S> = Box<dyn FnMut(&mut S, &str, &Widget) -> bool>;
type OnHover<S> = Box<dyn FnMut(&mut S, u32, u32, &Widget) -> bool>;
type OnWidget<S> = Box<dyn FnMut(&mut S, &Widget) -> bool>;
type OnClosed<S> = Box<dyn FnMut(&mut S, &Widget)>;

/// A widget to add: its tag, sizes, `render`, and what else it takes.
/// A handler returns `true` when something changed: the widget is drawn
/// again.
pub struct Spec<S> {
    tag: String,
    sizes: Sizes,
    render: Render<S>,
    key: Option<OnText<S>>,
    paste: Option<OnText<S>>,
    hover: Option<OnHover<S>>,
    leave: Option<OnWidget<S>>,
    closed: Option<OnClosed<S>>,
    ask: Value,
    show: Value,
}

impl<S> Spec<S> {
    /// `render(state, w, h, widget)`: the content of the variant `w` by
    /// `h`.
    pub fn new(
        tag: &str,
        sizes: impl Into<Sizes>,
        render: impl FnMut(&mut S, u32, u32, &Widget) -> Content + 'static,
    ) -> Spec<S> {
        Spec {
            tag: tag.into(),
            sizes: sizes.into(),
            render: Box::new(render),
            key: None,
            paste: None,
            hover: None,
            leave: None,
            closed: None,
            ask: Value::Null,
            show: Value::Null,
        }
    }

    /// `key(state, name, widget)`: a key of the widget focused. Asks the
    /// keyboard (`widget.ask {quiet: true}`).
    pub fn key(mut self, f: impl FnMut(&mut S, &str, &Widget) -> bool + 'static) -> Spec<S> {
        self.key = Some(Box::new(f));
        self
    }

    /// `paste(state, text, widget)`: a paste; asks as `key` does.
    pub fn paste(mut self, f: impl FnMut(&mut S, &str, &Widget) -> bool + 'static) -> Spec<S> {
        self.paste = Some(Box::new(f));
        self
    }

    /// `hover(state, x, y, widget)`: the pointer on a cell of the
    /// variant shown. Subscribes `hover`.
    pub fn hover(mut self, f: impl FnMut(&mut S, u32, u32, &Widget) -> bool + 'static) -> Spec<S> {
        self.hover = Some(Box::new(f));
        self
    }

    /// `leave(state, widget)`: the pointer left, or the hover timed out.
    pub fn leave(mut self, f: impl FnMut(&mut S, &Widget) -> bool + 'static) -> Spec<S> {
        self.leave = Some(Box::new(f));
        self
    }

    /// `closed(state, widget)`: out of the deck (`drop`, or closed by
    /// the user).
    pub fn closed(mut self, f: impl FnMut(&mut S, &Widget) + 'static) -> Spec<S> {
        self.closed = Some(Box::new(f));
        self
    }

    /// The `widget.ask` of `key`, `paste`: `{quiet, escape, close}`.
    pub fn ask(mut self, opts: Value) -> Spec<S> {
        self.ask = opts;
        self
    }

    /// Members of `widget.show` (`SHOW`) for this widget.
    pub fn show(mut self, show: Value) -> Spec<S> {
        self.show = show;
        self
    }
}

/// The handlers of a widget: a `Spec` in the deck.
struct Entry<S> {
    w: Widget,
    spec: Spec<S>,
    keys: Option<Keys>,
}

/// Widgets kept drawn: one `batch` a tick of the contents that changed.
pub struct Deck<S> {
    defaults: Map,
    /// `tick` renders each variant, not only the one shown.
    pub every: bool,
    /// A hover ends after this, ms (1500); `None`: never.
    pub hover_timeout: Option<u64>,
    /// The session time of the last `tick`.
    pub now: u64,
    batch: bool,
    subscribed: Vec<String>,
    sent: BTreeMap<String, String>,
    over: Option<String>,
    over_until: Option<u64>,
    entries: Vec<Entry<S>>,
}

impl<S> Deck<S> {
    /// `show`: the defaults of `widget.show` for every widget, members
    /// of `SHOW`; `Null`: none.
    ///
    /// # Panics
    /// On a member not of `SHOW`.
    pub fn new(show: Value) -> Deck<S> {
        let defaults = match show {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        if let Some(k) = defaults.keys().find(|k| !SHOW.contains(&k.as_str())) {
            panic!("Deck::new: {k}: not one of {}", SHOW.join(", "));
        }
        Deck {
            defaults,
            every: false,
            hover_timeout: Some(1500),
            now: 0,
            batch: true,
            subscribed: Vec::new(),
            sent: BTreeMap::new(),
            over: None,
            over_until: None,
            entries: Vec::new(),
        }
    }

    /// The widgets, in the order added.
    pub fn widgets(&self) -> impl Iterator<Item = &Widget> {
        self.entries.iter().map(|e| &e.w)
    }

    pub fn widget(&self, tag: &str) -> Option<&Widget> {
        self.widgets().find(|w| w.tag == tag)
    }

    /// For `set`, `hide`, `ask`, `release`.
    pub fn widget_mut(&mut self, tag: &str) -> Option<&mut Widget> {
        self.entries
            .iter_mut()
            .map(|e| &mut e.w)
            .find(|w| w.tag == tag)
    }

    fn at(&self, tag: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.w.tag == tag)
    }

    fn subscribe(&mut self, topics: &[&str]) {
        let fresh: Vec<&str> = (topics.iter().copied())
            .filter(|t| !self.subscribed.iter().any(|s| s == t))
            .collect();
        if fresh.is_empty() {
            return;
        }
        self.subscribed.extend(fresh.iter().map(|t| t.to_string()));
        if call("subscribe", json!({ "topics": (&fresh[..]) })).is_err() {
            // One topic refused refuses all: each alone.
            for t in fresh {
                let _ = call("subscribe", json!({ "topics": [t] }));
            }
        }
    }

    /// One request for all: `batch`, or one by one where it is not served.
    fn send(&mut self, calls: Vec<Value>) -> Result<(), Error> {
        if calls.len() > 1 && self.batch {
            match call("batch", json!({ "calls": (calls.clone()) })) {
                Ok(out) => {
                    let results = out["results"].as_array().into_iter().flatten();
                    return match results.into_iter().find(|r| !r["error"].is_null()) {
                        Some(r) => Err(error_of(&r["error"])),
                        None => Ok(()),
                    };
                }
                Err(e) if e.code == "headless" => self.batch = false,
                Err(e) => return Err(e),
            }
        }
        for c in calls {
            call(
                c["method"].as_str().unwrap_or_default(),
                c["params"].clone(),
            )?;
        }
        Ok(())
    }

    fn forget(sent: &mut BTreeMap<String, String>, w: &Widget) {
        let p = format!("{}:", w.info["id"]);
        sent.retain(|k, _| !k.starts_with(&p));
    }

    fn declare(&mut self, i: usize, state: &mut S) -> Result<(), Error> {
        let e = &mut self.entries[i];
        e.w.now = self.now;
        let mut list = Vec::new();
        for (name, vw, vh) in &e.spec.sizes.0 {
            let content = (e.spec.render)(state, *vw, *vh, &e.w).to_value();
            let mut v = json!({"tag": (e.w.tag.as_str()), "size": [*vw, *vh], "content": content});
            if let Some(n) = name {
                v["name"] = json!(n.as_str());
            }
            list.push(v);
        }
        call("widget.declare", json!({ "widgets": (list.clone()) }))?;
        let mut show = json!({ "tag": (e.w.tag.as_str()) });
        for k in SHOW {
            let v = e.spec.show.get(k).or(self.defaults.get(k));
            if let Some(v) = v.filter(|v| !v.is_null()) {
                show[k] = v.clone();
            }
        }
        Self::forget(&mut self.sent, &e.w);
        e.w.hidden = false;
        let mut answer = call("widget.show", show)?;
        if answer["widget"].as_object().is_some() {
            e.w.info = answer["widget"].take();
        }
        for v in list {
            let name = match v["name"].as_str() {
                Some(n) => n.to_string(),
                None => format!("{}x{}", v["size"][0], v["size"][1]),
            };
            let key = format!("{}:{name}", e.w.info["id"]);
            self.sent.insert(key, v["content"].to_string());
        }
        if e.keys.is_some() {
            let ask = &e.spec.ask;
            let quiet = if ask["quiet"].is_null() {
                json!(true)
            } else {
                ask["quiet"].clone()
            };
            let opts = json!({"quiet": quiet, "escape": (ask["escape"].clone()),
                              "close": (ask["close"].clone())});
            match e.w.ask(&opts) {
                // A test run that does not focus: the widget is drawn all the same.
                Err(err) if err.code != "headless" => return Err(err),
                _ => {}
            }
        }
        Ok(())
    }

    /// Declares each variant (each rendered once) and shows the widget.
    /// The first subscribes `widgets`, `move`; a `hover` or `leave`
    /// handler subscribes `hover`.
    ///
    /// # Panics
    /// When `tag` is in the deck already.
    pub fn add(&mut self, spec: Spec<S>, state: &mut S) -> Result<&Widget, Error> {
        assert!(
            self.at(&spec.tag).is_none(),
            "Deck::add: {}: already in the deck",
            spec.tag
        );
        self.subscribe(&["widgets", "move"]);
        if spec.hover.is_some() || spec.leave.is_some() {
            self.subscribe(&["hover"]);
        }
        let keys = (spec.key.is_some() || spec.paste.is_some()).then(Keys::default);
        let w = Widget {
            tag: spec.tag.clone(),
            now: self.now,
            ..Widget::default()
        };
        self.entries.push(Entry { w, spec, keys });
        let i = self.entries.len() - 1;
        self.declare(i, state)?;
        Ok(&self.entries[i].w)
    }

    /// Other sizes for the widget `tag`.
    pub fn resize(
        &mut self,
        tag: &str,
        sizes: impl Into<Sizes>,
        state: &mut S,
    ) -> Result<(), Error> {
        let Some(i) = self.at(tag) else { return Ok(()) };
        self.entries[i].spec.sizes = sizes.into();
        self.declare(i, state)
    }

    /// Every widget shown again in `frame` (the event `move`).
    pub fn move_to(&mut self, frame: Value, state: &mut S) -> Result<(), Error> {
        self.defaults.insert("frame".into(), frame.clone());
        for i in 0..self.entries.len() {
            self.entries[i].spec.show["frame"] = frame.clone();
            self.declare(i, state)?;
        }
        Ok(())
    }

    /// The widget `tag` out of the deck; `closed` called.
    pub fn drop(&mut self, tag: &str, state: &mut S) {
        let Some(i) = self.at(tag) else { return };
        if self.over.as_deref() == Some(tag) {
            (self.over, self.over_until) = (None, None);
        }
        let mut e = self.entries.remove(i);
        Self::forget(&mut self.sent, &e.w);
        if let Some(f) = e.spec.closed.as_mut() {
            f(state, &e.w)
        }
    }

    /// Renders now, one batch: the widget `only` or all; `every`: each
    /// variant.
    pub fn draw(&mut self, only: Option<&str>, every: bool, state: &mut S) -> Result<(), Error> {
        let mut calls = Vec::new();
        for e in &mut self.entries {
            if only.is_some_and(|t| t != e.w.tag) {
                continue;
            }
            e.w.now = self.now;
            let info = &e.w.info;
            // Not told yet which variant shows (a `widgets` event): each.
            let all =
                every || (info["variant"].is_null() && !e.w.hidden && info["state"] != "gone");
            let mut shown: Vec<(String, u32, u32)> = (info["variants"].as_array().into_iter())
                .flatten()
                .filter(|v| all || v["name"] == info["variant"])
                .filter_map(|v| {
                    let size = &v["size"];
                    let name = v["name"].as_str()?.to_string();
                    Some((name, size[0].as_u64()? as u32, size[1].as_u64()? as u32))
                })
                .collect();
            if let (true, Some((w, h)), Some(name)) = (shown.is_empty(), e.w.size(), e.w.variant())
            {
                shown.push((name.to_string(), w, h));
            }
            for (name, w, h) in shown {
                let content = (e.spec.render)(state, w, h, &e.w).to_value();
                let key = format!("{}:{name}", e.w.info["id"]);
                let text = content.to_string();
                if self.sent.get(&key) == Some(&text) {
                    continue;
                }
                self.sent.insert(key, text);
                calls.push(json!({"method": "widget.set", "params": {
                    "id": (e.w.info["id"].clone()), "variant": name, "content": content,
                }}));
            }
        }
        self.send(calls)
    }

    /// The periodic draw: `now` kept; a hover past its timeout ended;
    /// every widget shown rendered, the contents that changed sent in
    /// one `batch` (one by one where `batch` answers `headless`).
    pub fn tick(&mut self, now: u64, state: &mut S) -> Result<(), Error> {
        self.now = now;
        if self.over_until.is_some_and(|t| now >= t) {
            self.leave(state)?;
        }
        self.draw(None, self.every, state)
    }

    /// Ends a hover: `leave` called.
    pub fn leave(&mut self, state: &mut S) -> Result<(), Error> {
        self.over_until = None;
        let Some(tag) = self.over.take() else {
            return Ok(());
        };
        let Some(i) = self.at(&tag) else {
            return Ok(());
        };
        let e = &mut self.entries[i];
        let changed = e.spec.leave.as_mut().is_some_and(|f| f(state, &e.w));
        if changed {
            return self.draw(Some(&tag), false, state);
        }
        Ok(())
    }

    fn hover(&mut self, data: &Value, state: &mut S) -> Result<(), Error> {
        let mut found = None;
        for e in &self.entries {
            let (Some(r), Some((w, h))) = (e.w.rect(), e.w.size()) else {
                continue;
            };
            if e.spec.hover.is_none()
                || data["sprite"].is_null()
                || e.w.info["sprite"] != data["sprite"]
            {
                continue;
            }
            let (Some(px), Some(py)) = (data["x"].as_i64(), data["y"].as_i64()) else {
                continue;
            };
            let x = px - r[0] - ((r[2] - i64::from(w)) >> 1);
            let y = py - r[1] - ((r[3] - i64::from(h)) >> 1);
            if (0..i64::from(w)).contains(&x) && (0..i64::from(h)).contains(&y) {
                found = Some((e.w.tag.clone(), x as u32, y as u32));
            }
        }
        if found.as_ref().map(|f| &f.0) != self.over.as_ref() {
            self.leave(state)?;
        }
        let Some((tag, x, y)) = found else {
            return Ok(());
        };
        self.over = Some(tag.clone());
        self.over_until = self.hover_timeout.map(|t| self.now + t);
        let i = self.at(&tag).expect("a widget of the deck");
        let e = &mut self.entries[i];
        let changed = e.spec.hover.as_mut().is_some_and(|f| f(state, x, y, &e.w));
        if changed {
            return self.draw(Some(&tag), false, state);
        }
        Ok(())
    }

    fn input(&mut self, data: &Value, state: &mut S) -> Result<bool, Error> {
        let i = self.entries.iter().position(|e| {
            e.keys.is_some() && !e.w.info["id"].is_null() && e.w.info["id"] == data["widget"]
        });
        let Some(i) = i else { return Ok(false) };
        let tag = self.entries[i].w.tag.clone();
        let e = &mut self.entries[i];
        if data["released"] == "close" {
            e.w.info["state"] = json!("gone");
            e.w.info["asks"] = Value::Null;
            e.w.info["focused"] = json!(false);
            self.drop(&tag, state);
            return Ok(true);
        }
        let keys = e.keys.as_mut().expect("keys");
        let changed = if data.get("released").is_some() {
            keys.reset();
            let was = e.w.focused();
            e.w.info["focused"] = json!(false);
            was
        } else {
            // The key may come before the `widgets` event of the focus.
            let mut changed = !e.w.focused();
            e.w.info["focused"] = json!(true);
            match keys.feed(data) {
                Some(Fed::Key(k)) if e.spec.key.is_some() => {
                    changed |= (e.spec.key.as_mut().expect("key"))(state, &k, &e.w)
                }
                Some(Fed::Paste(p)) if e.spec.paste.is_some() => {
                    changed |= (e.spec.paste.as_mut().expect("paste"))(state, &p, &e.w)
                }
                _ => {}
            }
            changed
        };
        if changed {
            self.draw(Some(&tag), false, state)?;
        }
        Ok(true)
    }

    fn widgets_event(&mut self, data: &Value, state: &mut S) -> Result<bool, Error> {
        let i = (self.entries.iter())
            .position(|e| !data["id"].is_null() && e.w.info["id"] == data["id"]);
        let Some(i) = i else { return Ok(false) };
        let e = &mut self.entries[i];
        let tag = e.w.tag.clone();
        let was = (
            e.w.info["variant"].clone(),
            e.w.focused(),
            e.w.info["asks"].clone(),
        );
        let before = std::mem::replace(&mut e.w.info, data.clone());
        let focused = e.w.focused();
        if let Some(keys) = e.keys.as_mut().filter(|_| !focused) {
            keys.reset();
        }
        let gone = data["state"] == "gone";
        if gone {
            Self::forget(&mut self.sent, &self.entries[i].w);
        }
        if self.over.as_deref() == Some(&tag) && PLACE.iter().any(|k| before[*k] != data[*k]) {
            self.leave(state)?;
        }
        let w = &self.entries[i].w;
        if was
            != (
                w.info["variant"].clone(),
                w.focused(),
                w.info["asks"].clone(),
            )
        {
            self.draw(Some(&tag), false, state)?;
        }
        Ok(true)
    }

    /// An event: `widgets`, `move`, `hover`, `input` are the deck's
    /// (`true`); another: `false`.
    pub fn event(&mut self, topic: &str, data: &Value, state: &mut S) -> Result<bool, Error> {
        match topic {
            "widgets" => self.widgets_event(data, state),
            "move" => self.move_to(data["frame"].clone(), state).map(|_| true),
            "hover" => self.hover(data, state).map(|_| true),
            "input" => self.input(data, state),
            _ => Ok(false),
        }
    }
}

// ---- keys ----

/// What [`Keys::feed`] makes of an `input` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fed {
    Key(String),
    Paste(String),
}

/// Keys of `input` events: names, and a bracketed paste put together.
#[derive(Clone, Debug, Default)]
pub struct Keys {
    paste: Option<Vec<u8>>,
}

const START: &[u8] = b"\x1b[200~";
const END: &[u8] = b"\x1b[201~";

fn newlines(t: &str) -> String {
    t.replace("\r\n", "\n").replace('\r', "\n")
}

impl Keys {
    /// The name of a modified arrow or tilde key the driver does not
    /// name: `shift+up`, `ctrl+alt+pagedown`, ...
    pub fn name(bytes: &[u8]) -> Option<String> {
        let rest = bytes.strip_prefix(b"\x1b[")?;
        let semi = rest.iter().position(|b| *b == b';')?;
        let code: u32 = std::str::from_utf8(&rest[..semi]).ok()?.parse().ok()?;
        let [m @ b'2'..=b'8', last] = rest[semi + 1..] else {
            return None;
        };
        if !rest[..semi].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let key = match (last, code) {
            (b'~', 2) => "insert",
            (b'~', 3) => "delete",
            (b'~', 5) => "pageup",
            (b'~', 6) => "pagedown",
            (b'A', 1) => "up",
            (b'B', 1) => "down",
            (b'C', 1) => "right",
            (b'D', 1) => "left",
            (b'H', 1) => "home",
            (b'F', 1) => "end",
            _ => return None,
        };
        let mods = m - b'1';
        let mut out = String::new();
        for (bit, name) in [(4, "ctrl+"), (2, "alt+"), (1, "shift+")] {
            if mods & bit != 0 {
                out.push_str(name);
            }
        }
        out.push_str(key);
        Some(out)
    }

    /// Forgets a paste begun.
    pub fn reset(&mut self) {
        self.paste = None;
    }

    /// Of `{key}`, `{bytes}` (base64), `{paste}`: a key, a whole paste
    /// (`\r\n`, `\r` made `\n`), or `None`.
    pub fn feed(&mut self, data: &Value) -> Option<Fed> {
        if let Some(p) = data["paste"].as_str() {
            return Some(Fed::Paste(newlines(p)));
        }
        let bytes = base64::decode(data["bytes"].as_str().unwrap_or("")).unwrap_or_default();
        if let Some(buf) = self.paste.as_mut() {
            if bytes != END {
                buf.extend_from_slice(&bytes);
                return None;
            }
            let all = self.paste.take().unwrap_or_default();
            return Some(Fed::Paste(newlines(&String::from_utf8_lossy(&all))));
        }
        if bytes == START {
            self.paste = Some(Vec::new());
            return None;
        }
        let key = data["key"]
            .as_str()
            .map(str::to_string)
            .or_else(|| Keys::name(&bytes));
        key.map(Fed::Key)
    }
}

// ---- pieces of an interactive widget ----

/// The longest start of `text` that holds in `n` cells.
fn head(text: &str, n: usize) -> &str {
    let mut used = 0;
    for (i, ch) in text.char_indices() {
        used += crate::width(ch);
        if used > n {
            return &text[..i];
        }
    }
    text
}

/// `text` in `n` cells at most: cut, `…` in the last.
pub fn cut(text: &str, n: usize) -> String {
    if cells(text) <= n {
        text.to_string()
    } else if n > 0 {
        format!("{}…", head(text, n - 1))
    } else {
        String::new()
    }
}

/// `row` in exactly `w` cells: cut, or blanks added.
pub fn fit(row: &[Run], w: usize) -> Vec<Run> {
    let mut out = Vec::new();
    let mut room = w;
    for piece in row {
        let t = head(&piece.t, room);
        if !t.is_empty() {
            room -= cells(t);
            out.push(piece.with(t));
        }
        if t.len() != piece.t.len() {
            break;
        }
    }
    if room > 0 || out.is_empty() {
        out.push(Run::new(" ".repeat(room)));
    }
    out
}

/// `text` in `n` lines of `w` cells at most, cut at blanks; the rest in
/// the last, cut.
pub fn wrap(text: &str, w: usize, n: usize) -> Vec<String> {
    let mut words = text.split_whitespace().peekable();
    let mut lines: Vec<String> = Vec::new();
    while lines.len() < n {
        let Some(first) = words.next() else { break };
        let mut line = first.to_string();
        while let Some(next) = words.peek() {
            if cells(&line) + 1 + cells(next) > w {
                break;
            }
            line.push(' ');
            line.push_str(next);
            words.next();
        }
        lines.push(line);
    }
    if let Some(last) = lines.last_mut() {
        for word in words {
            last.push(' ');
            last.push_str(word);
        }
    }
    lines.iter().map(|l| cut(l, w)).collect()
}

/// A line of text and its cursor (`ptyrc.ui.Editor`); `cursor`: a byte
/// offset at a char boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub cursor: usize,
    off: usize,
}

impl Field {
    /// `text`, the cursor at its end.
    pub fn new(text: &str) -> Field {
        Field {
            text: text.into(),
            cursor: text.len(),
            off: 0,
        }
    }

    fn back(&self, at: usize) -> usize {
        self.text[..at]
            .chars()
            .next_back()
            .map_or(0, char::len_utf8)
    }

    fn ahead(&self, at: usize) -> usize {
        self.text[at..].chars().next().map_or(0, char::len_utf8)
    }

    /// `text` at the cursor, a control character a space.
    pub fn insert(&mut self, text: &str) {
        let text: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }

    /// Edits by a key name: `left`, `right`, `home`, `end`, `backspace`,
    /// `delete`, `ctrl+a b d e f h k u w`, `space`, a printable glyph.
    /// `false`: not a key of a field.
    pub fn key(&mut self, name: &str) -> bool {
        let at = self.cursor.min(self.text.len());
        match name {
            "left" | "ctrl+b" => self.cursor = at - self.back(at),
            "right" | "ctrl+f" => self.cursor = at + self.ahead(at),
            "home" | "ctrl+a" => self.cursor = 0,
            "end" | "ctrl+e" => self.cursor = self.text.len(),
            "backspace" | "ctrl+h" => {
                let to = at - self.back(at);
                self.text.replace_range(to..at, "");
                self.cursor = to;
            }
            "delete" | "ctrl+d" => {
                let n = self.ahead(at);
                self.text.replace_range(at..at + n, "");
            }
            "ctrl+u" => {
                self.text.replace_range(..at, "");
                self.cursor = 0;
            }
            "ctrl+k" => self.text.truncate(at),
            "ctrl+w" => {
                let start = self.text[..at].trim_end().rfind(' ').map_or(0, |i| i + 1);
                self.text.replace_range(start..at, "");
                self.cursor = start;
            }
            "space" => self.insert(" "),
            _ => {
                let mut chars = name.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if !c.is_control() => self.insert(name),
                    _ => return false,
                }
            }
        }
        true
    }

    /// The row of `w` cells: the text in `style` (its colours and
    /// attributes), the cursor inverse when `focused`, `placeholder` dim
    /// when empty.
    pub fn render(&mut self, w: usize, focused: bool, placeholder: &str, style: &Run) -> Vec<Run> {
        let dim = Run::new(placeholder).attrs(attr::DIM);
        if !focused {
            if self.text.is_empty() {
                return fit(&[dim], w);
            }
            return fit(&[style.with(cut(&self.text, w))], w);
        }
        let at = self.cursor.min(self.text.len());
        let n = self.ahead(at);
        let under = if n > 0 { &self.text[at..at + n] } else { " " };
        self.off = self.off.min(at);
        while self.off < at && cells(&self.text[self.off..at]) + cells(under) > w {
            self.off += self.ahead(self.off);
        }
        let mut row = vec![
            style.with(&self.text[self.off..at]),
            style.with(under).attrs(attr::INVERSE),
            style.with(&self.text[at + n..]),
        ];
        if self.text.is_empty() {
            row.push(dim);
        }
        row.retain(|r| !r.t.is_empty());
        fit(&row, w)
    }
}

/// A bar on a list (`ptyrc.ui.Listing`): `sel` the selected, `top` the
/// first shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub count: usize,
    pub sel: usize,
    pub top: usize,
    page: usize,
}

impl Listing {
    pub fn new(count: usize, sel: usize) -> Listing {
        Listing {
            count,
            sel,
            top: 0,
            page: 1,
        }
    }

    /// `up`, `down`, `tab`, `pageup`, `pagedown`, `home`, `end`; `j`,
    /// `k` when `letters`. `false`: not a key of a list.
    pub fn key(&mut self, name: &str, letters: bool) -> bool {
        let page = self.page.max(1) as i64;
        let count = self.count as i64;
        let step = match name {
            "j" if letters => 1,
            "k" if letters => -1,
            "up" => -1,
            "down" | "tab" => 1,
            "pageup" => -page,
            "pagedown" => page,
            "home" => -count,
            "end" => count,
            _ => return false,
        };
        self.sel = (self.sel as i64 + step).clamp(0, (count - 1).max(0)) as usize;
        true
    }

    /// The indices shown in `h` rows, `sel` among them.
    pub fn window(&mut self, h: usize) -> Range<usize> {
        self.page = h.max(1);
        self.sel = self.sel.min(self.count.saturating_sub(1));
        let top = self.top.min(self.sel).min(self.count.saturating_sub(h));
        self.top = top.max((self.sel + 1).saturating_sub(h));
        self.top..self.count.min(self.top + h)
    }

    /// `"3/12"` when the list does not hold in `h` rows; else empty.
    pub fn mark(&self, h: usize) -> String {
        if self.count > h {
            format!("{}/{}", self.sel + 1, self.count)
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, Color, native};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<(String, Value)>>>;

    /// A host that places widget 1 in a 19x5 box at (10, 2), variant
    /// `big` (17x3), its sprite 7; `widget.ask` and `batch` headless.
    fn host() -> Log {
        let log: Log = Rc::default();
        let l = log.clone();
        native::serve(move |method, params| {
            l.borrow_mut().push((method.to_string(), params));
            match method {
                "widget.show" => Ok(json!({"widget": {
                    "id": 1, "tag": "t", "state": "placed", "variant": "big", "size": [17, 3],
                    "variants": [{"name": "big", "size": [17, 3]}, {"name": "line", "size": [5, 1]}],
                    "rect": [10, 2, 19, 5], "sprite": 7,
                }})),
                "widget.ask" | "batch" => Err(Error {
                    code: "headless".into(),
                    message: String::new(),
                    hint: None,
                }),
                _ => Ok(json!({})),
            }
        });
        log
    }

    fn methods(log: &Log) -> Vec<String> {
        log.borrow_mut().drain(..).map(|(m, _)| m).collect()
    }

    #[derive(Default)]
    struct State {
        n: u32,
        keys: Vec<String>,
        at: Option<(u32, u32)>,
    }

    fn spec() -> Spec<State> {
        Spec::new(
            "t",
            [("big", 17, 3), ("line", 5, 1)],
            |s: &mut State, w, h, _| Content::from(format!("{w}x{h} {}", s.n)),
        )
        .key(|s, name, _| {
            s.keys.push(name.into());
            name == "space"
        })
        .hover(|s, x, y, _| {
            s.at = Some((x, y));
            false
        })
    }

    #[test]
    fn a_deck_declares_shows_and_draws_what_changed() {
        let log = host();
        let mut s = State::default();
        let mut deck = Deck::new(json!({"border": "round"}));
        deck.add(spec(), &mut s).unwrap();
        let calls = log.borrow().clone();
        let names: Vec<&str> = calls.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(
            names,
            [
                "subscribe",
                "subscribe",
                "widget.declare",
                "widget.show",
                "widget.ask"
            ]
        );
        assert_eq!(calls[0].1, json!({"topics": ["widgets", "move"]}));
        let declared = &calls[2].1["widgets"];
        assert_eq!(
            declared[1],
            json!({"tag": "t", "name": "line", "size": [5, 1],
                                       "content": {"rows": ["5x1 0"]}})
        );
        assert_eq!(calls[3].1, json!({"tag": "t", "border": "round"}));
        assert_eq!(calls[4].1, json!({"id": 1, "quiet": true}));
        log.borrow_mut().clear();

        deck.tick(1000, &mut s).unwrap();
        assert!(log.borrow().is_empty(), "nothing changed");
        s.n = 1;
        deck.tick(2000, &mut s).unwrap();
        let calls = log.borrow_mut().drain(..).collect::<Vec<_>>();
        assert_eq!(
            calls,
            [(
                "widget.set".to_string(),
                json!({"id": 1, "variant": "big",
            "content": {"rows": ["17x3 1"]}})
            )]
        );

        // Every variant: two sets, one batch refused, then one by one.
        s.n = 2;
        deck.every = true;
        deck.tick(3000, &mut s).unwrap();
        assert_eq!(methods(&log), ["batch", "widget.set", "widget.set"]);
        s.n = 3;
        deck.tick(4000, &mut s).unwrap();
        assert_eq!(methods(&log), ["widget.set", "widget.set"]);
        assert_eq!(deck.widget("t").unwrap().now, 4000);
    }

    #[test]
    fn keys_hover_and_widgets_events() {
        let log = host();
        let mut s = State::default();
        let mut deck = Deck::new(Value::Null);
        deck.add(spec(), &mut s).unwrap();
        log.borrow_mut().clear();

        assert!(
            deck.event("input", &json!({"widget": 1, "key": "x"}), &mut s)
                .unwrap()
        );
        assert!(deck.widget("t").unwrap().focused());
        // Focused: drawn again, though `x` changed nothing; the same content is not sent.
        assert!(methods(&log).is_empty());
        s.n = 5;
        deck.event("input", &json!({"widget": 1, "key": "space"}), &mut s)
            .unwrap();
        assert_eq!(methods(&log), ["widget.set"]);
        let up = base64::encode(b"\x1b[1;2A");
        deck.event("input", &json!({"widget": 1, "bytes": up}), &mut s)
            .unwrap();
        assert_eq!(s.keys, ["x", "space", "shift+up"]);
        assert!(
            !deck
                .event("input", &json!({"widget": 2, "key": "x"}), &mut s)
                .unwrap()
        );

        // The inner box: (10 + 1, 2 + 1); 17x3 in 19x5.
        deck.event("hover", &json!({"sprite": 7, "x": 13, "y": 4}), &mut s)
            .unwrap();
        assert_eq!(s.at, Some((2, 1)));
        s.at = None;
        deck.event("hover", &json!({"sprite": 8, "x": 13, "y": 4}), &mut s)
            .unwrap();
        assert_eq!(s.at, None);

        // The variant changes: drawn at once.
        s.n = 6;
        let info = json!({"id": 1, "state": "placed", "variant": "line", "size": [5, 1],
                          "variants": [{"name": "line", "size": [5, 1]}]});
        deck.event("widgets", &info, &mut s).unwrap();
        let calls = log.borrow_mut().drain(..).collect::<Vec<_>>();
        assert_eq!(calls[0].1["content"], json!({"rows": ["5x1 6"]}));
        assert!(!deck.event("size", &json!({}), &mut s).unwrap());

        deck.event("input", &json!({"widget": 1, "released": "close"}), &mut s)
            .unwrap();
        assert_eq!(deck.widgets().count(), 0);
    }

    #[test]
    fn a_canvas_goes_as_cells() {
        let mut c = Canvas::new(4, 1).unwrap();
        let ink = Cell {
            ch: Some('a'),
            fg: Color::Palette(14),
            attrs: attr::BOLD,
            ..Cell::default()
        };
        c.set(0, 0, ink);
        c.set(
            1,
            0,
            Cell {
                ch: Some('b'),
                ..ink
            },
        );
        c.set(
            3,
            0,
            Cell {
                ch: Some('漢'),
                fg: Color::Rgb(1, 2, 255),
                ..Cell::default()
            },
        );
        let v = Content::from(&mut c).to_value();
        assert_eq!(
            v,
            json!({"cells": [[{"t": "ab", "fg": 14, "bold": true}, {"t": " "},
                                         {"t": "漢", "fg": "#0102ff"}]]})
        );
    }

    #[test]
    fn key_names() {
        assert_eq!(Keys::name(b"\x1b[1;5C").as_deref(), Some("ctrl+right"));
        assert_eq!(
            Keys::name(b"\x1b[6;8~").as_deref(),
            Some("ctrl+alt+shift+pagedown")
        );
        assert_eq!(Keys::name(b"\x1b[1;1C"), None);
        assert_eq!(Keys::name(b"\x1b[4;2~"), None);
        assert_eq!(Keys::name(b"\x1b[C"), None);
        let mut k = Keys::default();
        let b = |s: &[u8]| json!({"bytes": (base64::encode(s))});
        assert_eq!(k.feed(&b(b"\x1b[200~")), None);
        assert_eq!(k.feed(&b("a\r\nb\u{e9}".as_bytes())), None);
        assert_eq!(
            k.feed(&b(b"\x1b[201~")),
            Some(Fed::Paste("a\nb\u{e9}".into()))
        );
        assert_eq!(k.feed(&json!({"key": "up"})), Some(Fed::Key("up".into())));
        assert_eq!(
            k.feed(&json!({"paste": "x\ry"})),
            Some(Fed::Paste("x\ny".into()))
        );
    }

    #[test]
    fn cut_fit_wrap() {
        assert_eq!(cut("clockface", 5), "cloc…");
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("漢字漢", 4), "漢…");
        assert_eq!(cut("x", 0), "");
        let row = [Run::new("ab").fg(Color::Palette(1)), Run::new("cd")];
        assert_eq!(
            fit(&row, 3),
            [Run::new("ab").fg(Color::Palette(1)), Run::new("c")]
        );
        assert_eq!(
            fit(&row, 6),
            [row[0].clone(), row[1].clone(), Run::new("  ")]
        );
        assert_eq!(fit(&[], 0), [Run::new("")]);
        assert_eq!(
            wrap("the quick brown fox", 9, 2),
            ["the quick", "brown fox"]
        );
        assert_eq!(wrap("the quick brown fox", 9, 1), ["the quic…"]);
        assert_eq!(wrap("  ", 9, 1), Vec::<String>::new());
    }

    #[test]
    fn a_field() {
        let mut f = Field::new("héllo");
        assert!(f.key("left") && f.key("backspace"));
        assert_eq!((f.text.as_str(), f.cursor), ("hélo", 4));
        assert!(f.key("home") && f.key("delete") && f.key("X") && f.key("space"));
        assert_eq!(f.text, "X élo");
        assert!(!f.key("f1") && !f.key("\u{7}"));
        assert!(f.key("end") && f.key("ctrl+w"));
        assert_eq!(f.text, "X ");
        f.insert("a\tb");
        assert_eq!(f.text, "X a b");
        let plain = Run::default();
        let inv = |t: &str| Run::new(t).attrs(attr::INVERSE);
        assert_eq!(f.render(4, true, "", &plain), [Run::new("a b"), inv(" ")]);
        f.key("home");
        assert_eq!(f.render(3, true, "", &plain), [inv("X"), Run::new(" a")]);
        assert_eq!(f.render(3, false, "", &plain), [Run::new("X …")]);
        let mut e = Field::default();
        assert_eq!(
            e.render(4, false, "name", &plain),
            [Run::new("name").attrs(attr::DIM)]
        );
    }

    #[test]
    fn a_listing() {
        let mut l = Listing::new(10, 0);
        assert_eq!(l.window(3), 0..3);
        assert!(l.key("pagedown", true) && l.key("j", true));
        assert_eq!((l.sel, l.window(3)), (4, 2..5));
        assert!(!l.key("j", false) && !l.key("x", true));
        assert!(l.key("end", true));
        assert_eq!(
            (l.sel, l.window(3), l.mark(3)),
            (9, 7..10, "10/10".to_string())
        );
        assert_eq!(l.mark(10), "");
        let mut e = Listing::new(0, 3);
        assert_eq!((e.window(3), e.sel), (0..0, 0));
        assert!(e.key("down", true));
        assert_eq!(e.sel, 0);
    }
}
