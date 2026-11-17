// SPDX-License-Identifier: GPL-3.0-or-later
//! The focus: the widget that has the host keyboard. A widget asks
//! (`widget.ask`), the user gives it the keys (the hotkey,
//! `widget.focus`); the escape key takes them back. The widgets keep who
//! asks and who is focused (`ptyrc-core`); here are the escape keys, the
//! requests and the word sent to a connection that loses the focus.

use crate::engine::{ConnId, Ctx};
use crate::grab::{self, Grab};
use crate::screen::ScreenEngine;
use ptyrc_core::widgets::{WidgetError, WidgetId};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};
use std::collections::HashMap;

type Res = Result<Value, RpcError>;

/// The escape key of a widget that names none.
pub(crate) const ESCAPE: &str = "esc";

/// The widget focused, as the driver's keyboard sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub conn: ConnId,
    pub widget: WidgetId,
    /// Bytes of the escape key; empty: none.
    pub escape: Vec<u8>,
}

impl Held {
    /// The sorter of its keys: all are the widget's but the escape key.
    pub(crate) fn sorter(&self) -> Grab {
        Grab::all(self.conn, self.escape.clone())
    }
}

/// The escape keys of the widgets that ask, and the focus last told.
#[derive(Default)]
pub(crate) struct Focus {
    /// Per widget that asks: the bytes of its escape key; empty: none.
    escapes: HashMap<WidgetId, Vec<u8>>,
    told: Option<(WidgetId, ConnId)>,
}

fn id_of(params: &Value) -> Result<Option<WidgetId>, RpcError> {
    match params.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(id) => id
            .as_u64()
            .map(Some)
            .ok_or_else(|| RpcError::bad_params("id: a widget id")),
    }
}

fn members(params: &Value, known: &[&str]) -> Result<(), RpcError> {
    let unknown = params
        .as_object()
        .and_then(|o| o.keys().find(|k| !known.contains(&k.as_str())));
    match unknown {
        Some(key) => Err(RpcError::bad_params(format!("unknown member: {key}"))
            .with_hint(format!("members: {}", known.join(", ")))),
        None => Ok(()),
    }
}

fn no_widget(engine: &ScreenEngine, conn: ConnId, id: WidgetId) -> RpcError {
    let e = RpcError::new(ErrorCode::NoSprite, WidgetError::NoWidget(id).to_string());
    crate::hints::widget(engine, conn.0, Some(id), e)
}

impl ScreenEngine {
    /// The widget focused.
    pub(crate) fn held(&self) -> Option<Held> {
        let (widget, owner) = self.widgets.focused()?;
        Some(Held {
            conn: ConnId(owner),
            widget,
            escape: self.focus.escapes.get(&widget).cloned().unwrap_or_default(),
        })
    }

    /// Tell the connection that lost the focus by no doing of its own,
    /// and forget the escape keys of the widgets that ask no more.
    /// `reason`: why, when the caller knows; else it is told from the
    /// widget: its owner paused, its frame forgotten.
    pub(crate) fn sync_focus(&mut self, reason: Option<&str>, ctx: &mut Ctx) {
        let now = self
            .widgets
            .focused()
            .map(|(id, owner)| (id, ConnId(owner)));
        let was = std::mem::replace(&mut self.focus.told, now);
        if let Some((id, conn)) = was.filter(|w| Some(*w) != now) {
            let info = self.widgets.info(id).filter(|i| i.asks.is_some());
            let reason = match (reason, info) {
                // Released, hidden, gone: its own doing, or told by `widgets`.
                (_, None) => None,
                (Some(reason), Some(_)) => Some(reason),
                (None, Some(_)) if self.widgets.owner_hidden(conn.0) => Some("pause"),
                (None, Some(_)) => Some("hidden"),
            };
            if let Some(reason) = reason {
                ctx.send(conn, "input", json!({"widget": id, "released": reason}));
            }
        }
        let widgets = &self.widgets;
        self.focus
            .escapes
            .retain(|id, _| widgets.info(*id).is_some_and(|i| i.asks.is_some()));
    }

    /// Focus `id`, or drop the focus; the one that had it is told
    /// `reason`.
    fn set_focus(&mut self, id: Option<WidgetId>, reason: &str, ctx: &mut Ctx) -> Res {
        match id {
            Some(id) => self.widgets.focus(id).map_err(|e| {
                let code = match e {
                    WidgetError::NoWidget(_) => ErrorCode::NoSprite,
                    _ => ErrorCode::NotUsable,
                };
                let asking: Vec<String> = (self.widgets.order().iter())
                    .map(|id| id.to_string())
                    .collect();
                RpcError::new(code, e.to_string()).with_hint(match asking.is_empty() {
                    true => "no widget asks: `widget.ask` first".to_string(),
                    false => format!("widgets that ask: {}", asking.join(", ")),
                })
            })?,
            None => {
                self.widgets.blur();
            }
        }
        self.sync_focus(Some(reason), ctx);
        // Laid out at once: one hidden is put over the text.
        self.tick_widgets(ctx.now(), ctx);
        ctx.request_repaint();
        Ok(json!({"widget": id.and_then(|id| self.widgets.info(id))}))
    }

    /// The hotkey: the next widget of the order, the first when none is
    /// focused. False past the last: the focus is dropped.
    pub(crate) fn focus_next(&mut self, reason: &str, ctx: &mut Ctx) -> bool {
        let next = self.widgets.next();
        let _ = self.set_focus(next, reason, ctx);
        self.widgets.focused().is_some()
    }

    /// Drop the focus.
    pub(crate) fn blur(&mut self, reason: &str, ctx: &mut Ctx) {
        let _ = self.set_focus(None, reason, ctx);
    }

    /// `widget.ask {id, quiet, escape}`: `{"widget": info}`.
    pub(crate) fn widget_ask(&mut self, conn: ConnId, params: &Value, ctx: &mut Ctx) -> Res {
        members(params, &["id", "quiet", "escape"])?;
        let id = id_of(params)?.ok_or_else(|| RpcError::bad_params("id: required"))?;
        let quiet = match params.get("quiet") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(quiet)) => *quiet,
            Some(_) => return Err(RpcError::bad_params("quiet: true or false")),
        };
        let escape = match params.get("escape") {
            None | Some(Value::Null) => grab::escape_key(ESCAPE),
            Some(Value::Bool(false)) => Ok(Vec::new()),
            Some(Value::String(key)) => grab::escape_key(key),
            Some(_) => Err("escape: a key name, or false".to_string()),
        }
        .map_err(|e| {
            RpcError::bad_params(e)
                .with_hint("a named key that is not text, as esc or f12; false: none")
        })?;
        if self.widgets.ask(conn.0, id, quiet).is_err() {
            return Err(no_widget(self, conn, id));
        }
        self.focus.escapes.insert(id, escape);
        self.tick_widgets(ctx.now(), ctx);
        Ok(json!({"widget": self.widgets.info(id)}))
    }

    /// `widget.release {id}`: it asks no more.
    pub(crate) fn widget_release(&mut self, conn: ConnId, params: &Value, ctx: &mut Ctx) -> Res {
        members(params, &["id"])?;
        let id = id_of(params)?.ok_or_else(|| RpcError::bad_params("id: required"))?;
        if !self.widgets.release(conn.0, id) {
            return Err(no_widget(self, conn, id));
        }
        self.tick_widgets(ctx.now(), ctx);
        Ok(json!({}))
    }

    /// `widget.focus {id}`: that widget, or the next of the order;
    /// `{"widget": info | null}`, null past the last.
    pub(crate) fn widget_focus(&mut self, params: &Value, ctx: &mut Ctx) -> Res {
        members(params, &["id"])?;
        let id = match id_of(params)? {
            Some(id) => Some(id),
            None => self.widgets.next(),
        };
        self.set_focus(id, "blur", ctx)
    }

    /// `widget.blur {}`.
    pub(crate) fn widget_blur(&mut self, params: &Value, ctx: &mut Ctx) -> Res {
        members(params, &[])?;
        self.set_focus(None, "blur", ctx)?;
        Ok(json!({}))
    }
}
