// SPDX-License-Identifier: GPL-3.0-or-later
//! The host functions of a headless run, from memory: `clock.*`,
//! `storage.*`, `app.result`, `file.*`, `probe.*`. The answers of the
//! driver (`ptyrc-server/src/driver/hostfn.rs`, `storage.rs`); nothing
//! of the machine is read after the start, nothing written.

use crate::assets::{base64_decode, read_answer, valid_component};
use crate::core::CallError;
use ptyrc_core::zone::{self, Missing, Zone};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Instants of one `clock.zone`, at most.
const MAX_INSTANTS: usize = 64;
/// The range of an instant, ms: that of a `Date` in a script.
const MAX_MS: i64 = 8_640_000_000_000_000;
/// `storage.set` and `file.write` a second, at most.
pub const WRITES_A_SECOND: u32 = 16;
/// A result, serialised, at most.
const MAX_RESULT: usize = 64 * 1024;
/// A value of storage, at most.
pub const MAX_VALUE: usize = 256 * 1024;
/// Keys of storage, at most.
pub const MAX_KEYS: usize = 64;
/// Bytes of storage in all, at most.
pub const QUOTA: u64 = 1024 * 1024;
/// A file named by the user, at most.
pub const MAX_FILE: u64 = 16 << 20;
/// `file.write`, at most.
pub const MAX_WRITE: usize = 1 << 20;
/// Entries of a directory, at most.
pub const MAX_ENTRIES: usize = 1024;
/// Handles a run, at most.
pub const MAX_HANDLES: usize = 16;

const ZONE_HINT: &str = "an IANA name, as Europe/Paris; \"local\": the machine's";
const KEY_RULE: &str = "a key: 1 to 64 bytes of [A-Za-z0-9._-], not starting with `.`";
const FILE_HINT: &str =
    "a file is named by the user at the start: [options] x = { type = \"file\" }";

/// The methods served here.
pub fn is_host(method: &str) -> bool {
    matches!(
        method,
        "clock.now"
            | "clock.zone"
            | "storage.get"
            | "storage.set"
            | "storage.remove"
            | "storage.list"
            | "app.result"
            | "file.read"
            | "file.stat"
            | "file.write"
            | "file.list"
            | "probe.git"
            | "probe.system"
    )
}

/// The zone database, read whole before the profile: a name, its
/// TZif bytes. `None`: no database.
#[derive(Clone, Debug, Default)]
pub struct Zones {
    files: Option<BTreeMap<String, Arc<Vec<u8>>>>,
}

impl Zones {
    /// The TZif files of `dir` whose names keep the rule; a link is
    /// followed within `dir` only.
    pub fn read(dir: &Path) -> Zones {
        let Ok(root) = dir.canonicalize() else {
            return Zones { files: None };
        };
        let mut files = BTreeMap::new();
        let mut seen: BTreeMap<std::path::PathBuf, Arc<Vec<u8>>> = BTreeMap::new();
        walk_zones(&root, &root, "", 0, &mut files, &mut seen);
        Zones { files: Some(files) }
    }

    /// The zone `name`, as `zone::load` reads it.
    pub fn get(&self, name: &str) -> Result<Zone, Missing> {
        if !zone::valid_name(name) {
            return Err(Missing::Unknown);
        }
        let Some(files) = &self.files else {
            return match name {
                "UTC" => Ok(Zone::utc()),
                _ => Err(Missing::NoDatabase),
            };
        };
        let bytes = files.get(name).ok_or(Missing::Unknown)?;
        Zone::parse(name, bytes).ok_or(Missing::Unknown)
    }
}

fn walk_zones(
    root: &Path,
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut BTreeMap<String, Arc<Vec<u8>>>,
    seen: &mut BTreeMap<std::path::PathBuf, Arc<Vec<u8>>>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let full = match prefix.is_empty() {
            true => name,
            false => format!("{prefix}/{name}"),
        };
        if !zone::valid_name(&full) {
            continue;
        }
        let Ok(path) = entry.path().canonicalize() else {
            continue;
        };
        if !path.starts_with(root) {
            continue;
        }
        if path.is_dir() {
            if depth + 1 < 3 && entry.file_type().is_ok_and(|t| t.is_dir()) {
                walk_zones(root, &path, &full, depth + 1, out, seen);
            }
            continue;
        }
        if let Some(bytes) = seen.get(&path) {
            out.insert(full, bytes.clone());
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.starts_with(b"TZif") && bytes.len() <= 256 * 1024 {
            let bytes = Arc::new(bytes);
            seen.insert(path, bytes.clone());
            out.insert(full, bytes);
        }
    }
}

/// A file or a directory named by the user, in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Named {
    /// The option (`--file NAME=PATH`).
    pub option: String,
    /// The last component of the path, as typed.
    pub name: String,
    /// `None`: a file; else a directory: its regular files.
    pub entries: Option<BTreeMap<String, Content>>,
    /// The file's content (a directory: unused).
    pub content: Content,
    /// Written by the app, in memory.
    pub written: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Content {
    /// `None`: it does not exist.
    pub bytes: Option<Vec<u8>>,
    pub mtime_ms: i64,
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    let t = meta.modified().ok().and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as i64)
    });
    t.unwrap_or(0)
}

impl Named {
    /// `PATH` read once: a regular file (absent: created on a write), or
    /// a directory and its regular files. A link is followed only as
    /// the user named it.
    pub fn read(option: &str, path: &Path) -> Result<Named, String> {
        let shown = path.display();
        let name = (path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut named = Named {
            option: option.to_owned(),
            name,
            entries: None,
            content: Content::default(),
            written: false,
        };
        // A link in place of the file named: absent, as the driver.
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(named),
            Err(e) => return Err(format!("{shown}: {e}")),
        };
        if meta.file_type().is_symlink() {
            return Ok(named);
        }
        if meta.is_dir() {
            let mut entries = BTreeMap::new();
            let list = std::fs::read_dir(path).map_err(|e| format!("{shown}: {e}"))?;
            for e in list.flatten() {
                let Some(n) = e.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                // `file_type`: a link is not followed.
                if !valid_component(&n) || !e.file_type().is_ok_and(|t| t.is_file()) {
                    continue;
                }
                entries.insert(n, e.path());
            }
            // Sorted by name, then cut.
            let entries = (entries.into_iter().take(MAX_ENTRIES))
                .map(|(n, p)| Ok((n, read_file(&p)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            named.entries = Some(entries);
        } else if meta.is_file() {
            named.content = read_file(path)?;
        } else {
            return Err(format!("{shown}: regular files and directories only"));
        }
        Ok(named)
    }
}

fn read_file(path: &Path) -> Result<Content, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_FILE {
        return Err(format!("{}: more than {MAX_FILE} bytes", path.display()));
    }
    let mut bytes = Vec::new();
    (file.take(MAX_FILE).read_to_end(&mut bytes))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Content {
        bytes: Some(bytes),
        mtime_ms: mtime_ms(&meta),
    })
}

/// The state of the host functions of a run.
#[derive(Clone, Debug, Default)]
pub struct HostFns {
    /// Unix ms at session time 0 (`--clock`).
    pub wall0: i64,
    /// The local zone (`--zone`; UTC).
    pub zone: Option<Zone>,
    pub zones: Zones,
    pub storage: BTreeMap<String, String>,
    /// `(value, status)` of the last `app.result`.
    pub result: Option<(Value, u8)>,
    /// The handles: `f0`, `f1`, ... in order.
    pub files: Vec<Named>,
    /// `git`, `system`: the answer given (`--probe NAME=FILE.json`).
    pub probes: BTreeMap<String, Value>,
    /// Probes held and asked with no answer given.
    pub unanswered: Vec<String>,
    /// `(second, count)` of the writes.
    pub window: (u64, u32),
}

fn bad(message: impl Into<String>, hint: impl Into<String>) -> CallError {
    CallError {
        hint: Some(hint.into()),
        ..CallError::new("bad-params", message)
    }
}

fn not_usable(message: impl Into<String>, hint: impl Into<String>) -> CallError {
    CallError {
        hint: Some(hint.into()),
        ..CallError::new("not-usable", message)
    }
}

fn clock_params(method: &str, params: &Value) -> Result<(String, Option<Vec<i64>>), CallError> {
    if method == "clock.now" {
        return Ok(("local".into(), None));
    }
    let zone = params.get("zone").and_then(Value::as_str);
    let zone = zone.filter(|z| *z == "local" || zone::valid_name(z));
    let Some(zone) = zone else {
        return Err(bad("clock.zone: zone", ZONE_HINT));
    };
    let at_error = || {
        bad(
            "clock.zone: at",
            format!("1 to {MAX_INSTANTS} instants, Unix ms; absent: now"),
        )
    };
    let at = match params.get("at") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) => {
            let at: Option<Vec<i64>> = (list.iter())
                .map(|v| v.as_i64().filter(|ms| ms.abs() <= MAX_MS))
                .collect();
            Some(
                at.filter(|a| (1..=MAX_INSTANTS).contains(&a.len()))
                    .ok_or_else(at_error)?,
            )
        }
        Some(_) => return Err(at_error()),
    };
    Ok((zone.to_string(), at))
}

fn offset(zone: &Zone, ms: i64) -> Value {
    let o = zone.at(ms.div_euclid(1000));
    json!({
        "at": ms, "offset_s": o.offset_s, "abbr": o.abbr,
        "next": o.next.map(|s| s.saturating_mul(1000)),
    })
}

/// The answer of `clock.*` (`ptyrc-server`, `hostfn::clock_answer`).
fn clock_answer(method: &str, zone: &Zone, now: i64, at: Option<Vec<i64>>) -> Value {
    match method {
        "clock.now" => {
            let o = zone.at(now.div_euclid(1000));
            json!({"unix_ms": now, "zone": zone.name, "offset_s": o.offset_s, "abbr": o.abbr,
                   "next": o.next.map(|s| s.saturating_mul(1000))})
        }
        _ => {
            let at = at.unwrap_or_else(|| vec![now]);
            let offsets: Vec<Value> = at.iter().map(|ms| offset(zone, *ms)).collect();
            json!({"zone": zone.name, "offsets": offsets})
        }
    }
}

fn key_of(method: &str, params: &Value) -> Result<String, CallError> {
    let key = params.get("key").and_then(Value::as_str);
    match key.filter(|k| valid_component(k)) {
        Some(key) => Ok(key.to_string()),
        None => Err(bad(format!("{method}: key"), KEY_RULE)),
    }
}

impl HostFns {
    /// A host function past the check: `lowered`, not held (`clock`,
    /// `probe-*`); `writable`: `files-write` held; `in_batch`: what
    /// the driver answers on its worker is refused.
    pub fn call(
        &mut self,
        method: &str,
        params: &Value,
        now_ms: u64,
        lowered: bool,
        writable: bool,
        in_batch: bool,
    ) -> Result<Value, CallError> {
        let params = match params {
            Value::Null => &json!({}),
            p => p,
        };
        match method {
            "clock.now" | "clock.zone" => {
                let (zone, at) = clock_params(method, params)?;
                if lowered {
                    let mut answer = clock_answer(method, &Zone::utc(), now_ms as i64, at);
                    answer["degree"] = json!("none");
                    return Ok(answer);
                }
                if in_batch {
                    return Err(not_in_batch(method));
                }
                let zone = match zone.as_str() {
                    "local" => self.zone.clone().unwrap_or_else(Zone::utc),
                    name => self.zones.get(name).map_err(|m| match m {
                        Missing::Unknown => {
                            bad(format!("clock.zone: no such zone: {name}"), ZONE_HINT)
                        }
                        Missing::NoDatabase => not_usable(
                            "clock.zone: no zone database",
                            "UTC and local alone: $TZDIR or /usr/share/zoneinfo is absent",
                        ),
                    })?,
                };
                Ok(clock_answer(method, &zone, self.wall0 + now_ms as i64, at))
            }
            "app.result" => self.app_result(params),
            "probe.git" | "probe.system" => {
                let part = &method["probe.".len()..];
                let none = match part {
                    "git" => json!({"repo": null, "degree": "none"}),
                    _ => json!({"glance": {}, "logins": {}, "listeners": [], "disks": [],
                                "degree": "none"}),
                };
                if lowered {
                    return Ok(none);
                }
                if in_batch {
                    return Err(not_in_batch(method));
                }
                match self.probes.get(part) {
                    Some(answer) => Ok(answer.clone()),
                    None => {
                        if !self.unanswered.iter().any(|p| p == part) {
                            self.unanswered.push(part.to_owned());
                        }
                        Ok(none)
                    }
                }
            }
            _ if in_batch => Err(not_in_batch(method)),
            _ if method.starts_with("storage.") => self.storage(method, params, now_ms),
            _ => self.file(method, params, now_ms, writable),
        }
    }

    /// One write more this second, or `not-usable`.
    fn count_write(&mut self, method: &str, now_ms: u64) -> Result<(), CallError> {
        let second = now_ms / 1000;
        if self.window.0 != second {
            self.window = (second, 0);
        }
        if self.window.1 >= WRITES_A_SECOND {
            return Err(not_usable(
                format!("{method}: {WRITES_A_SECOND} a second at most"),
                "keep the value in memory; write it when it changes, or on `stop`",
            ));
        }
        self.window.1 += 1;
        Ok(())
    }

    fn app_result(&mut self, params: &Value) -> Result<Value, CallError> {
        let value = params.get("value").cloned().unwrap_or(Value::Null);
        let size = serde_json::to_vec(&value).map_or(usize::MAX, |v| v.len());
        if size > MAX_RESULT {
            return Err(CallError::new(
                "bad-params",
                format!("app.result: value: {MAX_RESULT} bytes at most, {size} given"),
            ));
        }
        let status = match params.get("status") {
            None | Some(Value::Null) => Some(0),
            Some(s) => s.as_u64().filter(|s| *s <= 255),
        };
        let Some(status) = status else {
            return Err(bad(
                "app.result: status",
                "0..255: what the caller's command exits with",
            ));
        };
        self.result = Some((value, status as u8));
        Ok(json!({}))
    }

    fn used(&self) -> u64 {
        self.storage.values().map(|v| v.len() as u64).sum()
    }

    fn storage(&mut self, method: &str, params: &Value, now_ms: u64) -> Result<Value, CallError> {
        if method == "storage.set" {
            self.count_write(method, now_ms)?;
        }
        let quota = |used: u64| json!({"used": used, "quota": QUOTA});
        match method {
            "storage.get" => {
                let key = key_of(method, params)?;
                Ok(json!({"value": self.storage.get(&key)}))
            }
            "storage.set" => {
                let key = key_of(method, params)?;
                let Some(value) = params.get("value").and_then(Value::as_str) else {
                    return Err(bad(
                        "storage.set: value",
                        "a string: JSON, or base64, made by the app",
                    ));
                };
                let used = self.used();
                let old = self.storage.get(&key).map(|v| v.len() as u64);
                let after = used - old.unwrap_or(0) + value.len() as u64;
                let count = self.storage.len() + usize::from(old.is_none());
                if value.len() > MAX_VALUE || after > QUOTA || count > MAX_KEYS {
                    return Err(not_usable(
                        "storage: full",
                        format!(
                            "storage: {QUOTA} bytes at most, {used} used; {MAX_KEYS} keys, \
                             {MAX_VALUE} bytes a value"
                        ),
                    ));
                }
                self.storage.insert(key, value.to_owned());
                Ok(quota(after))
            }
            "storage.remove" => {
                let key = key_of(method, params)?;
                self.storage.remove(&key);
                Ok(json!({}))
            }
            _ => {
                let keys: Vec<Value> = (self.storage.iter())
                    .map(|(k, v)| json!({"key": k, "size": v.len()}))
                    .collect();
                let mut answer = quota(self.used());
                answer["keys"] = json!(keys);
                Ok(answer)
            }
        }
    }

    /// The handle of `params.file`.
    fn handle(&self, params: &Value) -> Result<usize, CallError> {
        let handle = params.get("file").and_then(Value::as_str).unwrap_or("");
        let i = (handle.strip_prefix('f'))
            .filter(|n| !n.starts_with('0') || *n == "0")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|i| *i < self.files.len());
        i.ok_or_else(|| CallError {
            hint: Some(FILE_HINT.into()),
            ..CallError::new("no-file", format!("no file {handle:?}"))
        })
    }

    fn file(
        &mut self,
        method: &str,
        params: &Value,
        now_ms: u64,
        writable: bool,
    ) -> Result<Value, CallError> {
        let i = self.handle(params)?;
        let child = match params.get("name") {
            None | Some(Value::Null) => None,
            Some(v) => match v.as_str().filter(|n| valid_component(n)) {
                Some(n) => Some(n.to_owned()),
                None => {
                    return Err(bad(
                        format!("{method}: name"),
                        KEY_RULE.replace("key", "name"),
                    ));
                }
            },
        };
        let dir = self.files[i].entries.is_some();
        if method == "file.list" && child.is_some() {
            return Err(bad("file.list: name", "file.list takes the handle alone"));
        }
        if !dir && (child.is_some() || method == "file.list") {
            return Err(bad(
                format!(
                    "{method}: {} is a file, not a directory",
                    self.files[i].name
                ),
                "name: a file of a directory handle",
            ));
        }
        if method == "file.write" {
            if dir {
                return Err(bad(
                    "file.write: a directory is never written",
                    "a file handle",
                ));
            }
            if !writable {
                return Err(CallError {
                    hint: Some(
                        "a box ticked is written back: the manifest's `write = true` and \
                         `files-write`"
                            .into(),
                    ),
                    ..CallError::new("denied", "file.write: denied")
                });
            }
            self.count_write(method, now_ms)?;
            return self.write(i, params, now_ms);
        }
        let named = &self.files[i];
        let content = match (&named.entries, &child) {
            (Some(entries), Some(c)) => entries.get(c).cloned().unwrap_or_default(),
            (Some(entries), None) if method == "file.list" => {
                let list: Vec<Value> = (entries.iter())
                    .map(|(n, c)| json!({"name": n, "size": c.bytes.as_ref().map_or(0, Vec::len)}))
                    .collect();
                return Ok(json!({"entries": list}));
            }
            (Some(_), None) => {
                return Err(bad(
                    format!("{method}: name: a file of the directory {}", named.name),
                    "file.list names them",
                ));
            }
            (None, _) => named.content.clone(),
        };
        match (method, &content.bytes) {
            ("file.stat", None) => Ok(json!({"exists": false})),
            ("file.stat", Some(b)) => {
                Ok(json!({"exists": true, "size": b.len(), "mtime_ms": content.mtime_ms}))
            }
            (_, None) => Err(CallError::new(
                "no-file",
                format!(
                    "{method}: {} does not exist",
                    child.as_deref().unwrap_or(&named.name)
                ),
            )),
            (_, Some(b)) => {
                let mut answer = read_answer(b, params, method)?;
                answer["mtime_ms"] = json!(content.mtime_ms);
                Ok(answer)
            }
        }
    }

    fn write(&mut self, i: usize, params: &Value, now_ms: u64) -> Result<Value, CallError> {
        let bytes = match (params.get("text"), params.get("base64")) {
            (Some(Value::String(t)), None) => t.as_bytes().to_vec(),
            (None, Some(Value::String(b))) => {
                base64_decode(b).ok_or_else(|| bad("file.write: base64", "the content, base64"))?
            }
            _ => {
                return Err(bad(
                    "file.write: text or base64",
                    "the whole content, one of the two",
                ));
            }
        };
        if bytes.len() > MAX_WRITE {
            return Err(bad(
                format!(
                    "file.write: {MAX_WRITE} bytes at most, {} given",
                    bytes.len()
                ),
                "the whole content, 1 MiB at most",
            ));
        }
        let named = &mut self.files[i];
        if let Some(want) = params.get("if_mtime_ms").filter(|v| !v.is_null())
            && (want.as_i64() != Some(named.content.mtime_ms) || named.content.bytes.is_none())
        {
            return Err(not_usable(
                "file.write: changed since it was read",
                "read it again, then write",
            ));
        }
        let mtime = self.wall0 + now_ms as i64;
        let size = bytes.len();
        named.content = Content {
            bytes: Some(bytes),
            mtime_ms: mtime,
        };
        named.written = true;
        Ok(json!({"size": size, "mtime_ms": mtime}))
    }
}

/// What the driver leaves pending, in a `batch`.
pub fn not_in_batch(method: &str) -> CallError {
    CallError::new("bad-request", format!("{method}: not allowed in a batch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(h: &mut HostFns, method: &str, params: Value) -> Result<Value, CallError> {
        h.call(method, &params, 0, false, true, false)
    }

    #[test]
    fn the_clock_held_and_not() {
        let mut h = HostFns {
            wall0: 1_774_744_200_000,
            zone: Zone::posix("Europe/Paris", "CET-1CEST,M3.5.0,M10.5.0/3"),
            ..HostFns::default()
        };
        let now = h
            .call("clock.now", &json!({}), 5, false, false, false)
            .unwrap();
        assert_eq!(
            now,
            json!({"unix_ms": 1_774_744_200_005_i64, "zone": "Europe/Paris", "offset_s": 3600,
                   "abbr": "CET", "next": 1_774_746_000_000_i64})
        );
        // Past the change: the summer offset.
        let later = h.call("clock.now", &json!({}), 3_600_000, false, false, false);
        assert_eq!(later.unwrap()["offset_s"], 7200);
        let low = h
            .call("clock.now", &json!({}), 5, true, false, true)
            .unwrap();
        assert_eq!(
            low,
            json!({"unix_ms": 5, "zone": "UTC", "offset_s": 0, "abbr": "UTC", "next": null,
                   "degree": "none"})
        );
        let e = h
            .call("clock.now", &json!({}), 5, false, false, true)
            .unwrap_err();
        assert_eq!(e.code, "bad-request");
        let many: Vec<i64> = (0..65).collect();
        for p in [
            json!({"zone": "../../etc/passwd"}),
            json!({"zone": "/etc/passwd"}),
            json!({"zone": "UTC", "at": many}),
        ] {
            assert_eq!(
                call(&mut h, "clock.zone", p).unwrap_err().code,
                "bad-params"
            );
        }
        // No database: UTC alone.
        let z = call(&mut h, "clock.zone", json!({"zone": "UTC", "at": [0]})).unwrap();
        assert_eq!(z["offsets"][0]["abbr"], "UTC");
        let e = call(&mut h, "clock.zone", json!({"zone": "Asia/Tokyo"})).unwrap_err();
        assert_eq!(e.code, "not-usable");
    }

    #[test]
    fn a_database_read_before() {
        let db = zone::database();
        if !db.join("Asia/Tokyo").is_file() {
            return;
        }
        let zones = Zones::read(&db);
        assert_eq!(zones.get("Asia/Tokyo").unwrap().at(0).offset_s, 32400);
        assert_eq!(zones.get("Nowhere/City"), Err(Missing::Unknown));
        assert_eq!(zones.get("../etc/passwd"), Err(Missing::Unknown));
        assert_eq!(zones.get("zone.tab"), Err(Missing::Unknown));
    }

    #[test]
    fn storage_in_memory() {
        let mut h = HostFns::default();
        let s = call(&mut h, "storage.set", json!({"key": "a", "value": "xyz"})).unwrap();
        assert_eq!(s, json!({"used": 3, "quota": QUOTA}));
        let g = call(&mut h, "storage.get", json!({"key": "a"})).unwrap();
        assert_eq!(g, json!({"value": "xyz"}));
        let g = call(&mut h, "storage.get", json!({"key": "b"})).unwrap();
        assert_eq!(g, json!({"value": null}));
        for key in ["..", "a/b", ".x", "", &"k".repeat(65), "a\0b"] {
            let e = call(&mut h, "storage.get", json!({"key": key})).unwrap_err();
            assert_eq!(e.code, "bad-params", "{key:?}");
        }
        let big = "v".repeat(MAX_VALUE + 1);
        let e = h.call(
            "storage.set",
            &json!({"key": "b", "value": big}),
            1000,
            false,
            false,
            false,
        );
        assert_eq!(e.unwrap_err().message, "storage: full");
        let l = call(&mut h, "storage.list", json!({})).unwrap();
        assert_eq!(l["keys"], json!([{"key": "a", "size": 3}]));
        call(&mut h, "storage.remove", json!({"key": "a"})).unwrap();
        assert!(h.storage.is_empty());
        // Sixteen a second.
        let set = |h: &mut HostFns, t| {
            h.call(
                "storage.set",
                &json!({"key": "a", "value": ""}),
                t,
                false,
                false,
                false,
            )
        };
        assert!((0..16).all(|_| set(&mut h, 5000).is_ok()));
        assert_eq!(set(&mut h, 5999).unwrap_err().code, "not-usable");
        assert!(set(&mut h, 6000).is_ok());
        // The count, the sum.
        let mut h = HostFns::default();
        for k in 0..64 {
            h.storage.insert(format!("k{k}"), String::new());
        }
        let e = call(&mut h, "storage.set", json!({"key": "z", "value": ""})).unwrap_err();
        assert_eq!(e.message, "storage: full");
        let mut h = HostFns::default();
        for k in 0..4 {
            h.storage.insert(format!("k{k}"), "v".repeat(MAX_VALUE));
        }
        let e = call(&mut h, "storage.set", json!({"key": "z", "value": "v"})).unwrap_err();
        assert_eq!(e.message, "storage: full");
    }

    #[test]
    fn a_result_kept() {
        let mut h = HostFns::default();
        call(&mut h, "app.result", json!({"value": [1, 2], "status": 3})).unwrap();
        call(&mut h, "app.result", json!({"value": "last"})).unwrap();
        assert_eq!(h.result, Some((json!("last"), 0)));
        let big = "x".repeat(MAX_RESULT);
        let e = call(&mut h, "app.result", json!({"value": big})).unwrap_err();
        assert_eq!(e.code, "bad-params");
        let e = call(&mut h, "app.result", json!({"status": 256})).unwrap_err();
        assert_eq!(e.code, "bad-params");
        let ok = h.call("app.result", &json!({}), 0, false, false, true);
        assert!(ok.is_ok(), "answered at once, in a batch too");
    }

    #[test]
    fn files_in_memory() {
        let file = |bytes: Option<&[u8]>| Named {
            option: "save".into(),
            name: "TODO.md".into(),
            entries: None,
            content: Content {
                bytes: bytes.map(<[u8]>::to_vec),
                mtime_ms: 7,
            },
            written: false,
        };
        let mut dir = file(None);
        dir.name = "pics".into();
        dir.entries = Some(BTreeMap::from([(
            "ship.pic".into(),
            Content {
                bytes: Some(b"ship".to_vec()),
                mtime_ms: 9,
            },
        )]));
        let mut h = HostFns {
            files: vec![file(Some(b"todo")), dir, file(None)],
            ..HostFns::default()
        };
        let r = call(&mut h, "file.read", json!({"file": "f0"})).unwrap();
        assert_eq!(
            r,
            json!({"text": "todo", "size": 4, "eof": true, "mtime_ms": 7})
        );
        let s = call(&mut h, "file.stat", json!({"file": "f2"})).unwrap();
        assert_eq!(s, json!({"exists": false}));
        let l = call(&mut h, "file.list", json!({"file": "f1"})).unwrap();
        assert_eq!(l, json!({"entries": [{"name": "ship.pic", "size": 4}]}));
        let r = call(
            &mut h,
            "file.read",
            json!({"file": "f1", "name": "ship.pic"}),
        )
        .unwrap();
        assert_eq!(r["text"], "ship");
        for p in [
            json!({"file": "f3"}),
            json!({"file": "f01"}),
            json!({"file": "/etc/passwd"}),
            json!({"file": 0}),
        ] {
            assert_eq!(
                call(&mut h, "file.read", p.clone()).unwrap_err().code,
                "no-file",
                "{p}"
            );
        }
        for name in ["../x", "a/b", ".x"] {
            let e = call(&mut h, "file.read", json!({"file": "f1", "name": name})).unwrap_err();
            assert_eq!(e.code, "bad-params", "{name}");
        }
        let e = call(&mut h, "file.read", json!({"file": "f1", "name": "nope"})).unwrap_err();
        assert_eq!(e.code, "no-file");
        // Writes stay in memory; a stale mtime is refused.
        let w = h.call(
            "file.write",
            &json!({"file": "f0", "text": "new"}),
            0,
            false,
            false,
            false,
        );
        assert_eq!(w.unwrap_err().code, "denied");
        let e = call(
            &mut h,
            "file.write",
            json!({"file": "f0", "text": "x", "if_mtime_ms": 1}),
        );
        assert_eq!(
            e.unwrap_err().message,
            "file.write: changed since it was read"
        );
        let w = call(
            &mut h,
            "file.write",
            json!({"file": "f0", "base64": "eHk=", "if_mtime_ms": 7}),
        );
        assert_eq!(w.unwrap(), json!({"size": 2, "mtime_ms": 0}));
        assert!(h.files[0].written);
        let big = "x".repeat(MAX_WRITE + 1);
        let e = call(&mut h, "file.write", json!({"file": "f2", "text": big})).unwrap_err();
        assert_eq!(e.code, "bad-params");
        let e = call(&mut h, "file.write", json!({"file": "f1", "text": ""})).unwrap_err();
        assert_eq!(e.code, "bad-params");
        let e = h.call("file.read", &json!({"file": "f0"}), 0, false, false, true);
        assert_eq!(e.unwrap_err().code, "bad-request");
    }

    #[test]
    fn probes_as_given() {
        let mut h = HostFns::default();
        h.probes
            .insert("git".into(), json!({"repo": {"name": "x"}, "stamp": "s"}));
        let g = call(&mut h, "probe.git", json!({})).unwrap();
        assert_eq!(g["repo"]["name"], "x");
        let s = call(&mut h, "probe.system", json!({})).unwrap();
        assert_eq!(s["degree"], "none");
        assert_eq!(h.unanswered, ["system"]);
        let low = h
            .call("probe.git", &json!({}), 0, true, false, true)
            .unwrap();
        assert_eq!(low, json!({"repo": null, "degree": "none"}));
    }
}
