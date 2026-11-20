// SPDX-License-Identifier: GPL-3.0-or-later
//! The pop-up in the driver: a start held while the user answers, the
//! keys of the host keyboard, the answers written. See `popup.rs`.

use super::Driver;
use crate::engine::{ConnId, Engine, Modal, Token};
use crate::grab;
use crate::grants::Identity;
use crate::permissions::{self, Declared, Entry};
use crate::popup::{Ask, MIN_SIZE, Outcome};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};

/// What the answer lets go on.
pub(super) enum Then {
    /// `app.start`, `app.enrol`, `app.restart`: the request, made again
    /// once answered.
    Call { method: String, params: Value },
    /// `app.permissions`: the grants of the app, changed.
    Review { id: u64 },
}

/// A request held by a screen: shown in its turn, one at a time.
pub(super) struct Asking {
    conn: ConnId,
    token: Token,
    ask: Ask,
    then: Then,
}

/// The program's file name: the name of an app not named.
fn file_name(program: &str) -> String {
    std::path::Path::new(program)
        .file_name()
        .map_or_else(|| program.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The answers of the file for `key`; what is not understood in it.
fn file_entry(key: &str) -> (Option<Entry>, Vec<String>) {
    let (mut answers, warnings) = permissions::dir()
        .map(|dir| permissions::load(&dir))
        .unwrap_or_default();
    (answers.apps.remove(key), warnings)
}

impl Driver {
    /// A screen can be shown and answered: a host terminal, its
    /// keyboard open. Else the rule of a headless session.
    pub(super) fn can_ask(&self) -> bool {
        !self.cfg.headless && self.host.interactive && self.stdin_open
    }

    /// The start that `method` asks for, held for the pop-up when what
    /// it declares has permissions without an answer and a screen can
    /// be shown. True: held, answered once the user answers. False: not
    /// for the pop-up; `app_call` serves it (and its errors).
    pub(super) fn ask_first(
        &mut self,
        conn: ConnId,
        method: &str,
        params: &Value,
        in_batch: bool,
    ) -> Result<bool, RpcError> {
        if method == "app.permissions" {
            return self.review(conn, params, in_batch).map(|()| true);
        }
        let Some(apps) = &self.apps else {
            return Ok(false);
        };
        let member = |name: &str| params.get(name).filter(|v| !v.is_null());
        if let (Some(store), "app.start") = (member("store").and_then(Value::as_str), method) {
            // What does not start is told by `app_call`.
            if ["argv", "name", "declares"]
                .iter()
                .any(|m| member(m).is_some())
            {
                return Ok(false);
            }
            let Ok(found) = crate::store::find(store, member("args"), member("files")) else {
                return Ok(false);
            };
            let key = Identity::Store(found.origin.clone()).key();
            let (entry, _) = file_entry(&key);
            let resolved = permissions::resolve(&found.declared, entry.as_ref(), false);
            if resolved.missing.is_empty() || !self.can_ask() {
                return Ok(false);
            }
            self.fits(method, in_batch)?;
            let ask = Ask::new(
                &found.name,
                &key,
                &found.tier,
                &found.declared,
                entry.unwrap_or_default(),
                &resolved.missing,
                false,
            );
            let then = Then::Call {
                method: method.to_string(),
                params: params.clone(),
            };
            return Ok(self.hold(conn, ask, then));
        }
        let Some(declared) = member("declares").and_then(|d| Declared::from_wire(d).ok()) else {
            return Ok(false);
        };
        let argv = member("argv")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str);
        let named = member("name").and_then(Value::as_str);
        let name = match method {
            "app.enrol" => match named {
                Some(name) => name.to_string(),
                None => return Ok(false),
            },
            "app.start" => match (named, argv) {
                (Some(name), _) => name.to_string(),
                (None, Some(program)) => file_name(program),
                (None, None) => return Ok(false),
            },
            "app.restart" => {
                let app = member("app").and_then(Value::as_u64);
                match (named, app.and_then(|id| apps.get(id))) {
                    (_, None) => return Ok(false),
                    (Some(name), Some(_)) => name.to_string(),
                    (None, Some(app)) => app.name.clone(),
                }
            }
            _ => return Ok(false),
        };
        let key = Identity::Native(name.clone()).key();
        let (entry, _) = file_entry(&key);
        let resolved = permissions::resolve(&declared, entry.as_ref(), false);
        if resolved.missing.is_empty() || !self.can_ask() {
            return Ok(false);
        }
        self.fits(method, in_batch)?;
        let ask = Ask::new(
            &name,
            &key,
            "native",
            &declared,
            entry.unwrap_or_default(),
            &resolved.missing,
            false,
        );
        let then = Then::Call {
            method: method.to_string(),
            params: params.clone(),
        };
        Ok(self.hold(conn, ask, then))
    }

    /// The apps of `Config::apps`, as `app.start` asks: the pop-up for
    /// what has no answer. One that does not start is not in the table.
    pub(super) fn start_apps(&mut self, engine: &mut dyn Engine) {
        // No connection has this id: the answer reaches nobody.
        let nobody = ConnId(0);
        for (argv, declares) in std::mem::take(&mut self.cfg.apps) {
            let mut params = json!({"argv": argv});
            if let Some(declares) = declares {
                params["declares"] = declares;
            }
            if let Ok(false) = self.ask_first(nobody, "app.start", &params, false) {
                let _ = self.app_call(nobody, "app.start", &params, engine);
            }
        }
        self.show_ask(engine);
    }

    /// `app.permissions {app}`: the screen of `ptyrc manage`, every
    /// permission the app declared, filled from the file.
    fn review(&mut self, conn: ConnId, params: &Value, in_batch: bool) -> Result<(), RpcError> {
        if let Some((bad, _)) =
            (params.as_object().into_iter().flatten()).find(|(k, _)| *k != "app")
        {
            return Err(
                RpcError::bad_params(format!("app.permissions: no member `{bad}`"))
                    .with_hint("members: app"),
            );
        }
        let id = (params.get("app").and_then(Value::as_u64))
            .ok_or_else(|| RpcError::bad_params("app: the id of an app"))?;
        let Some(apps) = &self.apps else {
            return Err(RpcError::new(
                ErrorCode::BadRequest,
                "this session has no apps",
            ));
        };
        let app = apps.get(id).ok_or_else(|| {
            RpcError::new(ErrorCode::NotUsable, format!("no app {id}"))
                .with_hint("the apps: ptyrc manage ls")
        })?;
        let (Some(declared), Some(identity)) = (&app.declares, &app.identity) else {
            return Err(RpcError::new(
                ErrorCode::NotUsable,
                format!("app {id} declares nothing: it holds the whole session"),
            ));
        };
        if !self.can_ask() {
            return Err(RpcError::new(
                ErrorCode::NotUsable,
                "no host terminal: the screen cannot be shown",
            )
            .with_hint("the answers are in permissions.toml, in the config directory of ptyrc"));
        }
        let key = identity.key();
        let (entry, _) = file_entry(&key);
        let shown: Vec<_> = declared.permissions.keys().copied().collect();
        let ask = Ask::new(
            &app.name,
            &key,
            if app.store().is_some() {
                "store"
            } else {
                "native"
            },
            declared,
            entry.unwrap_or_default(),
            &shown,
            true,
        );
        self.fits("app.permissions", in_batch)?;
        self.hold(conn, ask, Then::Review { id });
        Ok(())
    }

    /// The screen is large enough, and the request may wait.
    fn fits(&self, method: &str, in_batch: bool) -> Result<(), RpcError> {
        if in_batch {
            return Err(RpcError::new(
                ErrorCode::BadRequest,
                format!("{method}: the user is asked: not in a batch"),
            ));
        }
        let (cols, rows) = MIN_SIZE;
        if self.info.cols < cols || self.info.rows < rows {
            return Err(RpcError::new(
                ErrorCode::NotUsable,
                format!(
                    "the screen is too small for the permissions ({}x{}; {cols}x{rows} at least)",
                    self.info.cols, self.info.rows
                ),
            )
            .with_hint("enlarge the terminal, then ask again"));
        }
        Ok(())
    }

    /// Queue a screen; the request is answered when the user answers.
    fn hold(&mut self, conn: ConnId, ask: Ask, then: Then) -> bool {
        let token = Token(self.next_token);
        self.next_token += 1;
        self.asks.push_back(Asking {
            conn,
            token,
            ask,
            then,
        });
        self.repaint = true;
        true
    }

    /// Show the screen first in the queue, or none. A screen whose
    /// answers were all given meanwhile (two starts of one app) is
    /// gone on without being shown.
    pub(super) fn show_ask(&mut self, engine: &mut dyn Engine) {
        while let Some(first) = self.asks.front()
            && let Then::Call { .. } = &first.then
        {
            // What the screen asks: of a store app, its manifest's.
            let (entry, _) = file_entry(&first.ask.key);
            if !permissions::resolve(&first.ask.declared, entry.as_ref(), false)
                .missing
                .is_empty()
            {
                break;
            }
            let asking = self.asks.pop_front().expect("first");
            self.go_on(asking, None, engine);
        }
        let modal = self.asks.front_mut().map(|first| {
            let (grid, at) = first.ask.render(self.info.cols, self.info.rows);
            Modal { grid, at }
        });
        engine.modal(modal);
        self.repaint = true;
    }

    /// Bytes of the host keyboard, and only of it: the screen's while
    /// one is shown, else on their way (`host_input`). A paste is
    /// dropped; keys after the last answer too.
    pub(super) fn host_or_ask(&mut self, bytes: &[u8], engine: &mut dyn Engine) {
        if self.asks.is_empty() {
            return self.host_input(bytes, engine);
        }
        let mut names = Vec::new();
        for piece in grab::pieces(bytes, &mut self.pasting) {
            let grab::Piece::Keys(keys) = piece else {
                continue;
            };
            let sorted = grab::sort_keys(&keys, "", "", true, |_| true);
            for (name, key) in sorted.taken {
                if grab::is_release(&key) || grab::is_report(&key) {
                    continue;
                }
                names.extend(name);
            }
        }
        for name in names {
            let Some(first) = self.asks.front_mut() else {
                break;
            };
            if let Some(outcome) = first.ask.key(&name) {
                let asking = self.asks.pop_front().expect("first");
                self.answered(asking, outcome, engine);
                break;
            }
        }
        self.show_ask(engine);
    }

    /// The user answered: written, then the request goes on; or not.
    fn answered(&mut self, asking: Asking, outcome: Outcome, engine: &mut dyn Engine) {
        let Outcome::Answered(entry) = outcome else {
            let what = match asking.then {
                Then::Review { .. } => "nothing changed",
                Then::Call { .. } => "not started",
            };
            let error = RpcError::new(ErrorCode::Cancelled, format!("cancelled: {what}"))
                .with_hint("the user closed the permissions with esc; nothing was written");
            return self.reply(asking.token, Err(error));
        };
        let written = permissions::dir()
            .ok_or_else(|| "no config directory ($XDG_CONFIG_HOME, $HOME)".to_string())
            .and_then(|dir| {
                permissions::remember(&dir, &asking.ask.key, entry.clone())
                    .map_err(|e| e.to_string())
            });
        let unwritten = written.err();
        let id = self.go_on(asking, Some(entry), engine);
        if let (Some(why), Some(id), Some(apps)) = (unwritten, id, &self.apps) {
            apps.note(id, &format!("permissions.toml not written: {why}"));
        }
    }

    /// The request held goes on, with these answers in place of the
    /// file's when given. The app it is of.
    fn go_on(
        &mut self,
        asking: Asking,
        entry: Option<Entry>,
        engine: &mut dyn Engine,
    ) -> Option<u64> {
        let Asking {
            conn, token, then, ..
        } = asking;
        let result = match then {
            Then::Call { method, params } => {
                self.given = entry;
                let result = self.app_call(conn, &method, &params, engine);
                self.given = None;
                result
            }
            Then::Review { id } => {
                let apps = self.apps.as_mut().expect("asked of the table");
                let declared = apps.get(id).and_then(|a| a.declares.clone());
                match (declared, entry) {
                    (Some(declared), Some(entry)) => {
                        // The one function that sets what an app holds.
                        let grants = permissions::resolve(&declared, Some(&entry), false).grants;
                        apps.grant(id, Some((declared, grants)));
                        // What it hears, and the host mouse, follow.
                        self.refresh_topics();
                        self.sync_host_mouse(engine);
                        Ok(json!({"app": self.app_info(id, Some(conn), engine)}))
                    }
                    _ => Err(RpcError::new(
                        ErrorCode::Gone,
                        format!("app {id} left the table"),
                    )),
                }
            }
        };
        let id = result.as_ref().ok().and_then(|v| v["app"]["id"].as_u64());
        self.reply(token, result);
        id
    }

    /// Answer a request held, when its client is still there.
    fn reply(&mut self, token: Token, result: Result<Value, RpcError>) {
        if let Some((conn, id)) = self.pending.remove(&token) {
            self.respond(conn, &id, result);
        }
    }
}
