// SPDX-License-Identifier: GPL-3.0-or-later
//! The host functions in the driver: `clock.*`, `storage.*`,
//! `app.result`. What reads a disk goes to the worker (`hostio`) and is
//! answered later; the rest here. `plan/design.md`, "Host functions".

use super::Driver;
use crate::engine::{ConnId, Token};
use crate::grants::Verdict;
use crate::hostio::{Job, State};
use crate::storage;
use crate::zone::{self, Missing, Zone};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Instants of one `clock.zone`, at most.
const MAX_INSTANTS: usize = 64;
/// The range of an instant, ms: that of a `Date` in a script.
const MAX_MS: i64 = 8_640_000_000_000_000;
/// `storage.set` a second an app, at most.
pub(super) const WRITES_A_SECOND: u32 = 16;
/// A result, serialised, at most.
const MAX_RESULT: usize = 64 * 1024;

const ZONE_HINT: &str = "an IANA name, as Europe/Paris; \"local\": the machine's";

/// A method served here.
pub(super) fn is_host(method: &str) -> bool {
    matches!(
        method,
        "clock.now"
            | "clock.zone"
            | "storage.get"
            | "storage.set"
            | "storage.remove"
            | "storage.list"
            | "app.result"
    )
}

/// The params of `clock.*`: the zone, the instants (None: now).
fn clock_params(method: &str, params: &Value) -> Result<(String, Option<Vec<i64>>), RpcError> {
    if method == "clock.now" {
        return Ok(("local".into(), None));
    }
    let zone = params.get("zone").and_then(Value::as_str);
    let zone = zone.filter(|z| *z == "local" || zone::valid_name(z));
    let Some(zone) = zone else {
        return Err(RpcError::bad_params("clock.zone: zone").with_hint(ZONE_HINT));
    };
    let at = match params.get("at") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) => {
            let at: Option<Vec<i64>> = list
                .iter()
                .map(|v| v.as_i64().filter(|ms| ms.abs() <= MAX_MS))
                .collect();
            match at.filter(|a| (1..=MAX_INSTANTS).contains(&a.len())) {
                Some(at) => Some(at),
                None => return Err(at_error()),
            }
        }
        Some(_) => return Err(at_error()),
    };
    Ok((zone.to_string(), at))
}

fn at_error() -> RpcError {
    RpcError::bad_params("clock.zone: at").with_hint(format!(
        "1 to {MAX_INSTANTS} instants, Unix ms; absent: now"
    ))
}

fn offset(zone: &Zone, ms: i64) -> Value {
    let o = zone.at(ms.div_euclid(1000));
    json!({
        "at": ms,
        "offset_s": o.offset_s,
        "abbr": o.abbr,
        "next": o.next.map(|s| s.saturating_mul(1000)),
    })
}

/// The answer of `clock.*` for the zone and the instants given.
fn clock_answer(method: &str, zone: &Zone, now: i64, at: Option<Vec<i64>>) -> Value {
    match method {
        "clock.now" => {
            let mut o = offset(zone, now);
            o["unix_ms"] = o["at"].take();
            let object = o.as_object_mut().expect("an object");
            object.remove("at");
            object.insert("zone".into(), json!(zone.name));
            o
        }
        _ => {
            let at = at.unwrap_or_else(|| vec![now]);
            let offsets: Vec<Value> = at.iter().map(|ms| offset(zone, *ms)).collect();
            json!({"zone": zone.name, "offsets": offsets})
        }
    }
}

/// `clock.*` on the worker.
fn clock_job(method: &'static str, name: String, at: Option<Vec<i64>>) -> Job {
    Box::new(move |state: &mut State| {
        let zone = state.zone(&name).map_err(|missing| match missing {
            Missing::Unknown => RpcError::bad_params(format!("clock.zone: no such zone: {name}"))
                .with_hint(ZONE_HINT),
            Missing::NoDatabase => {
                RpcError::new(ErrorCode::NotUsable, "clock.zone: no zone database")
                    .with_hint("UTC and local alone: $TZDIR or /usr/share/zoneinfo is absent")
            }
        })?;
        Ok(clock_answer(method, &zone, state.wall_ms(), at))
    })
}

/// One more in the window `(since, count)` of a second, or none.
fn admit(window: &mut (Instant, u32), now: Instant) -> bool {
    if now.duration_since(window.0) >= Duration::from_secs(1) {
        *window = (now, 0);
    }
    if window.1 >= WRITES_A_SECOND {
        return false;
    }
    window.1 += 1;
    true
}

fn no_app(what: &str) -> RpcError {
    RpcError::new(
        ErrorCode::NotUsable,
        format!("no app: {what} belongs to an app"),
    )
    .with_hint("a connection of the user's has none; an app presents its token ($PTYRC_TOKEN)")
}

impl Driver {
    /// A host function, past the check. `None`: pending on the worker.
    pub(super) fn host_call(
        &mut self,
        conn: ConnId,
        method: &str,
        params: Value,
        in_batch: bool,
    ) -> Option<Result<Value, RpcError>> {
        let app = self.conns.get(&conn).and_then(|c| c.app);
        let identity = match self.asker(conn) {
            Ok(asker) => asker.map(|(who, _)| who.key()),
            Err(_) => None,
        };
        let lowered = matches!(self.verdict(conn, method, &params), Verdict::Lower(_));
        let job: Job = match method {
            "clock.now" | "clock.zone" => {
                let method = if method == "clock.now" {
                    "clock.now"
                } else {
                    "clock.zone"
                };
                let (zone, at) = match clock_params(method, &params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                if lowered {
                    return Some(Ok(self.clock_lowered(app, method, at)));
                }
                clock_job(method, zone, at)
            }
            "app.result" => return Some(self.app_result(app, &params)),
            _ => {
                let Some(who) = identity else {
                    return Some(Err(no_app("storage")));
                };
                let method = method.to_string();
                Box::new(move |state: &mut State| {
                    let Some(root) = &state.storage else {
                        return Err(RpcError::new(ErrorCode::NotUsable, "storage: no home")
                            .with_hint("$HOME or $XDG_STATE_HOME names where storage is kept"));
                    };
                    storage::call(root, &who, &method, &params)
                })
            }
        };
        if in_batch {
            return Some(Err(RpcError::new(
                ErrorCode::BadRequest,
                format!("{method}: not allowed in a batch"),
            )));
        }
        if method == "storage.set"
            && let Some(app) = app
            && let Err(e) = self.count_write(app)
        {
            return Some(Err(e));
        }
        let Some(hostio) = &self.hostio else {
            return Some(Err(RpcError::new(
                ErrorCode::NotUsable,
                "the host worker did not start",
            )));
        };
        let token = Token(self.next_token);
        if let Err(e) = hostio.submit(token, job) {
            return Some(Err(e));
        }
        // `pump_conn` keeps it pending under the token allocated last.
        self.next_token += 1;
        None
    }

    /// `clock.*` not held: session time of the app as the epoch, UTC.
    fn clock_lowered(&self, app: Option<u64>, method: &str, at: Option<Vec<i64>>) -> Value {
        let started = app
            .and_then(|id| self.apps.as_ref()?.get(id))
            .map_or(Duration::ZERO, |a| a.started);
        let now = self.now().saturating_sub(started).as_millis() as i64;
        let mut answer = clock_answer(method, &Zone::utc(), now, at);
        answer["degree"] = json!("none");
        answer
    }

    /// `app.result {value, status}`: kept with the app's entry.
    fn app_result(&mut self, app: Option<u64>, params: &Value) -> Result<Value, RpcError> {
        let Some(id) = app else {
            return Err(no_app("a result"));
        };
        let value = params.get("value").cloned().unwrap_or(Value::Null);
        let size = serde_json::to_vec(&value).map_or(usize::MAX, |v| v.len());
        if size > MAX_RESULT {
            return Err(RpcError::bad_params(format!(
                "app.result: value: {MAX_RESULT} bytes at most, {size} given"
            )));
        }
        let status = match params.get("status") {
            None | Some(Value::Null) => Some(0),
            Some(s) => s.as_u64().filter(|s| *s <= 255),
        };
        let Some(status) = status else {
            return Err(RpcError::bad_params("app.result: status")
                .with_hint("0..255: what the caller's command exits with"));
        };
        if let Some(apps) = &mut self.apps {
            apps.set_result(id, value, status as u8);
        }
        Ok(json!({}))
    }

    /// One `storage.set` of app `id`: refused past the rate.
    fn count_write(&mut self, id: u64) -> Result<(), RpcError> {
        let now = Instant::now();
        if self.writes.len() > 256 {
            let second = Duration::from_secs(1);
            self.writes.retain(|_, w| now.duration_since(w.0) < second);
        }
        let window = self.writes.entry(id).or_insert((now, 0));
        match admit(window, now) {
            true => Ok(()),
            false => Err(RpcError::new(
                ErrorCode::NotUsable,
                format!("storage.set: {WRITES_A_SECOND} a second at most"),
            )
            .with_hint("keep the value in memory; write it when it changes, or on `stop`")),
        }
    }

    /// The answers of the worker: the pending replies completed.
    pub(super) fn host_done(&mut self) {
        let Some(hostio) = &mut self.hostio else {
            return;
        };
        for (token, result) in hostio.drain() {
            if let Some((conn, id)) = self.pending.remove(&token) {
                self.respond(conn, &id, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_params_are_checked() {
        let ok = clock_params("clock.zone", &json!({"zone": "Asia/Tokyo", "at": [0, -5]}));
        assert_eq!(ok.unwrap(), ("Asia/Tokyo".into(), Some(vec![0, -5])));
        assert_eq!(clock_params("clock.now", &json!({})).unwrap().0, "local");
        let many: Vec<i64> = (0..65).collect();
        for bad in [
            json!({"zone": "../../etc/passwd"}),
            json!({"zone": "/etc/passwd"}),
            json!({}),
            json!({"zone": "UTC", "at": []}),
            json!({"zone": "UTC", "at": many}),
            json!({"zone": "UTC", "at": [1.5]}),
            json!({"zone": "UTC", "at": [i64::MAX]}),
            json!({"zone": "UTC", "at": 5}),
        ] {
            let e = clock_params("clock.zone", &bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadParams, "{bad}");
        }
    }

    #[test]
    fn the_host_methods_are_listed() {
        let listed = crate::grants::METHODS
            .iter()
            .filter(|m| is_host(m.0))
            .count();
        assert_eq!(listed, 7);
        assert!(!is_host("clock.later") && !is_host("storage.wipe"));
    }

    #[test]
    fn sixteen_writes_a_second() {
        let t0 = Instant::now();
        let mut window = (t0, 0);
        assert!((0..16).all(|_| admit(&mut window, t0)));
        assert!(!admit(&mut window, t0 + Duration::from_millis(999)));
        assert!(admit(&mut window, t0 + Duration::from_secs(1)));
    }

    #[test]
    fn the_answers_of_the_clock() {
        let paris = Zone::posix("Europe/Paris", "CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        // 2026-03-29 00:30 UTC.
        let t = 1_774_744_200_000;
        let now = clock_answer("clock.now", &paris, t, None);
        assert_eq!(
            now,
            json!({"unix_ms": t, "zone": "Europe/Paris", "offset_s": 3600, "abbr": "CET",
                   "next": 1_774_746_000_000_i64})
        );
        let zone = clock_answer("clock.zone", &paris, t, Some(vec![1_774_746_000_000]));
        assert_eq!(zone["offsets"][0]["offset_s"], 7200);
        assert_eq!(zone["offsets"][0]["abbr"], "CEST");
    }
}
