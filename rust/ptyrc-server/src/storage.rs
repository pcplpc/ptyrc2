// SPDX-License-Identifier: GPL-3.0-or-later
//! The storage of an app: a table of named values, a file each, in
//! `<state>/ptyrc/storage/<identity>/`. Reached by name, never by path:
//! the directory is opened without following a link, each value by
//! `openat` under it. Runs on the worker (`hostio`).

use ptyrc_proto::{ErrorCode, RpcError};
use rustix::fd::OwnedFd;
use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags};
use rustix::io::Errno;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

/// A value, at most.
pub(crate) const MAX_VALUE: usize = 256 * 1024;
/// The values of an app, at most.
pub(crate) const MAX_KEYS: usize = 64;
/// All the values of an app, at most.
pub(crate) const QUOTA: u64 = 1024 * 1024;

const KEY_RULE: &str = "a key: 1 to 64 bytes of [A-Za-z0-9._-], not starting with `.`";

/// The directory of an identity (`Identity::key`): bytes outside
/// `[A-Za-z0-9._-]` as `%XX`, as `ptyrc_store::Origin::dir_name`.
pub(crate) fn dir_name(identity: &str) -> String {
    let mut out = String::new();
    for b in identity.bytes() {
        match b.is_ascii_alphanumeric() || b"._-".contains(&b) {
            true => out.push(b as char),
            false => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A key: 1 to 64 bytes of `[A-Za-z0-9._-]`, not starting with `.`
/// (so neither `.` nor `..`).
pub(crate) fn valid_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && !key.starts_with('.')
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `key` of the params, checked.
pub(crate) fn key_of(method: &str, params: &Value) -> Result<String, RpcError> {
    let key = params.get("key").and_then(Value::as_str);
    match key.filter(|k| valid_key(k)) {
        Some(key) => Ok(key.to_string()),
        None => Err(RpcError::bad_params(format!("{method}: key")).with_hint(KEY_RULE)),
    }
}

/// The storage root, `<state>/ptyrc/storage`.
pub(crate) fn root() -> Option<PathBuf> {
    ptyrc_store::Dirs::from_env()
        .ok()
        .map(|dirs| dirs.state.join("ptyrc/storage"))
}

fn io_error(what: &str, e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::NotUsable, format!("storage: {what}: {e}"))
}

fn full(used: u64) -> RpcError {
    RpcError::new(ErrorCode::NotUsable, "storage: full").with_hint(format!(
        "storage: {QUOTA} bytes at most, {used} used; {MAX_KEYS} keys, {} bytes a value",
        MAX_VALUE
    ))
}

/// The directory of one app.
pub(crate) struct Table {
    dir: OwnedFd,
}

impl Table {
    /// The directory `name` under `root`, made 0700 when absent. A link
    /// in its place is not followed.
    pub fn open(root: &Path, name: &str, create: bool) -> Result<Option<Table>, RpcError> {
        if create {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root)
                .map_err(|e| io_error("the directory", e))?;
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let root = match rustix::fs::open(root, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT) => return Ok(None),
            Err(e) => return Err(io_error("the directory", e)),
        };
        if create {
            match rustix::fs::mkdirat(&root, name, Mode::from_raw_mode(0o700)) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(e) => return Err(io_error("the directory", e)),
            }
        }
        match rustix::fs::openat(&root, name, flags, Mode::empty()) {
            Ok(dir) => Ok(Some(Table { dir })),
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(io_error("the directory", e)),
        }
    }

    /// The keys and their sizes: the regular files of a valid name.
    pub fn keys(&self) -> Result<Vec<(String, u64)>, RpcError> {
        let dir = Dir::read_from(&self.dir).map_err(|e| io_error("list", e))?;
        let mut keys = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| io_error("list", e))?;
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            if !valid_key(name) {
                continue;
            }
            let Ok(stat) = rustix::fs::statat(&self.dir, name, AtFlags::SYMLINK_NOFOLLOW) else {
                continue;
            };
            if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile {
                keys.push((name.to_string(), stat.st_size as u64));
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// The value of `key`; None when absent, or not a regular file.
    pub fn get(&self, key: &str) -> Result<Option<String>, RpcError> {
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&self.dir, key, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT | Errno::LOOP) => return Ok(None),
            Err(e) => return Err(io_error(key, e)),
        };
        let stat = rustix::fs::fstat(&fd).map_err(|e| io_error(key, e))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        std::fs::File::from(fd)
            .take(MAX_VALUE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(key, e))?;
        if bytes.len() > MAX_VALUE {
            return Err(io_error(key, "larger than a value"));
        }
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// Write `value` under `key`: a temp file and a rename. Refused,
    /// nothing written, past a bound. The bytes used after.
    pub fn set(&self, key: &str, value: &str) -> Result<u64, RpcError> {
        let keys = self.keys()?;
        let used: u64 = keys.iter().map(|k| k.1).sum();
        let old = keys.iter().find(|k| k.0 == key).map(|k| k.1);
        let after = used - old.unwrap_or(0) + value.len() as u64;
        let count = keys.len() + usize::from(old.is_none());
        if value.len() > MAX_VALUE || after > QUOTA || count > MAX_KEYS {
            return Err(full(used));
        }
        // Not a key: a name starting with `.`.
        let temp = format!(".{key}.new");
        let _ = rustix::fs::unlinkat(&self.dir, &temp, AtFlags::empty());
        let flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = rustix::fs::openat(&self.dir, &temp, flags, Mode::from_raw_mode(0o600))
            .map_err(|e| io_error(key, e))?;
        let mut file = std::fs::File::from(fd);
        let written = file
            .write_all(value.as_bytes())
            .and_then(|()| file.sync_all());
        if let Err(e) = written {
            let _ = rustix::fs::unlinkat(&self.dir, &temp, AtFlags::empty());
            return Err(io_error(key, e));
        }
        // A rename replaces what holds the name, a link too, unfollowed.
        rustix::fs::renameat(&self.dir, &temp, &self.dir, key).map_err(|e| {
            let _ = rustix::fs::unlinkat(&self.dir, &temp, AtFlags::empty());
            io_error(key, e)
        })?;
        Ok(after)
    }

    pub fn remove(&self, key: &str) -> Result<(), RpcError> {
        match rustix::fs::unlinkat(&self.dir, key, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            // A directory planted under the name is not ours to remove.
            Err(Errno::ISDIR | Errno::PERM) => Ok(()),
            Err(e) => Err(io_error(key, e)),
        }
    }
}

/// `storage.get`, `.set`, `.remove`, `.list` of the identity `who`
/// under `root`.
pub(crate) fn call(
    root: &Path,
    who: &str,
    method: &str,
    params: &Value,
) -> Result<Value, RpcError> {
    let name = dir_name(who);
    let quota = |used: u64| json!({"used": used, "quota": QUOTA});
    match method {
        "storage.get" => {
            let key = key_of(method, params)?;
            let table = Table::open(root, &name, false)?;
            let value = match table {
                Some(table) => table.get(&key)?,
                None => None,
            };
            Ok(json!({"value": value}))
        }
        "storage.set" => {
            let key = key_of(method, params)?;
            let Some(value) = params.get("value").and_then(Value::as_str) else {
                return Err(RpcError::bad_params("storage.set: value")
                    .with_hint("a string: JSON, or base64, made by the app"));
            };
            let table = Table::open(root, &name, true)?.expect("made");
            Ok(quota(table.set(&key, value)?))
        }
        "storage.remove" => {
            let key = key_of(method, params)?;
            if let Some(table) = Table::open(root, &name, false)? {
                table.remove(&key)?;
            }
            Ok(json!({}))
        }
        "storage.list" => {
            let keys = match Table::open(root, &name, false)? {
                Some(table) => table.keys()?,
                None => Vec::new(),
            };
            let used: u64 = keys.iter().map(|k| k.1).sum();
            let list: Vec<Value> = (keys.iter())
                .map(|(key, size)| json!({"key": key, "size": size}))
                .collect();
            let mut answer = quota(used);
            answer["keys"] = json!(list);
            Ok(answer)
        }
        _ => Err(RpcError::unknown_method(method)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ptyrc-storage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("ptyrc/storage")
    }

    fn ask(root: &Path, who: &str, method: &str, params: Value) -> Result<Value, RpcError> {
        call(root, who, method, &params)
    }

    #[test]
    fn set_get_remove_list() {
        let r = root("basic");
        let who = "example.org/a/b";
        assert_eq!(
            ask(&r, who, "storage.get", json!({"key": "x"})).unwrap(),
            json!({"value": null})
        );
        assert_eq!(
            ask(&r, who, "storage.list", json!({})).unwrap()["keys"],
            json!([])
        );
        let set = ask(
            &r,
            who,
            "storage.set",
            json!({"key": "scores", "value": "[1,2]"}),
        );
        assert_eq!(set.unwrap(), json!({"used": 5, "quota": QUOTA}));
        let got = ask(&r, who, "storage.get", json!({"key": "scores"})).unwrap();
        assert_eq!(got, json!({"value": "[1,2]"}));
        ask(
            &r,
            who,
            "storage.set",
            json!({"key": "scores", "value": "[1]"}),
        )
        .unwrap();
        let list = ask(&r, who, "storage.list", json!({})).unwrap();
        assert_eq!(
            list,
            json!({"keys": [{"key": "scores", "size": 3}], "used": 3, "quota": QUOTA})
        );
        let dir = r.join("example.org%2Fa%2Fb");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&dir), mode(&dir.join("scores"))), (0o700, 0o600));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temp file left"
        );
        ask(&r, who, "storage.remove", json!({"key": "scores"})).unwrap();
        ask(&r, who, "storage.remove", json!({"key": "scores"})).unwrap();
        assert_eq!(
            ask(&r, who, "storage.get", json!({"key": "scores"})).unwrap()["value"],
            json!(null)
        );
    }

    #[test]
    fn the_directory_agrees_with_the_store() {
        let origin = ptyrc_store::Origin::parse("example.org/someone/sand-garden").unwrap();
        assert_eq!(dir_name(&origin.id()), origin.dir_name());
        let local = ptyrc_store::Origin::parse("local:/home/u/app#sub").unwrap();
        assert_eq!(dir_name(&local.id()), local.dir_name());
        assert_eq!(dir_name("native/pace"), "native%2Fpace");
    }

    #[test]
    fn bad_keys() {
        let r = root("keys");
        let long = "k".repeat(65);
        for key in [
            json!(".."),
            json!("."),
            json!("a/b"),
            json!(".x"),
            json!(""),
            json!(long),
            json!("a\0b"),
            json!(3),
        ] {
            for method in ["storage.get", "storage.set", "storage.remove"] {
                let e = ask(&r, "native/x", method, json!({"key": key, "value": "v"})).unwrap_err();
                assert_eq!(e.code, ErrorCode::BadParams, "{key} {method}");
            }
        }
        assert!(!r.exists(), "nothing made");
        assert!(valid_key(&"k".repeat(64)));
    }

    #[test]
    fn the_quota() {
        let r = root("quota");
        let who = "native/q";
        let big = "x".repeat(MAX_VALUE + 1);
        let e = ask(&r, who, "storage.set", json!({"key": "a", "value": big})).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotUsable);
        assert!(e.hint.unwrap().contains("1048576 bytes at most, 0 used"));
        // By the sum: four of 256 KiB fill it.
        let quarter = "x".repeat(MAX_VALUE);
        for k in ["a", "b", "c", "d"] {
            ask(&r, who, "storage.set", json!({"key": k, "value": quarter})).unwrap();
        }
        let e = ask(&r, who, "storage.set", json!({"key": "e", "value": "1"})).unwrap_err();
        assert!(e.hint.unwrap().contains("1048576 used"));
        // Replacing a value counts the new size alone.
        ask(&r, who, "storage.set", json!({"key": "a", "value": "1"})).unwrap();
        for k in ["b", "c", "d"] {
            ask(&r, who, "storage.remove", json!({"key": k})).unwrap();
        }
        // By the count.
        for i in 1..MAX_KEYS {
            ask(
                &r,
                who,
                "storage.set",
                json!({"key": format!("k{i}"), "value": ""}),
            )
            .unwrap();
        }
        let e = ask(&r, who, "storage.set", json!({"key": "over", "value": ""})).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotUsable);
        assert_eq!(
            ask(&r, who, "storage.get", json!({"key": "over"})).unwrap()["value"],
            json!(null)
        );
        ask(
            &r,
            who,
            "storage.set",
            json!({"key": "k1", "value": "again"}),
        )
        .unwrap();
    }

    #[test]
    fn origins_do_not_meet() {
        let r = root("apart");
        ask(
            &r,
            "example.org/a/one",
            "storage.set",
            json!({"key": "k", "value": "1"}),
        )
        .unwrap();
        ask(
            &r,
            "native/one",
            "storage.set",
            json!({"key": "k", "value": "2"}),
        )
        .unwrap();
        let get =
            |who: &str| ask(&r, who, "storage.get", json!({"key": "k"})).unwrap()["value"].clone();
        assert_eq!(get("example.org/a/one"), json!("1"));
        assert_eq!(get("native/one"), json!("2"));
        assert_eq!(get("example.org/a/two"), json!(null));
    }

    #[test]
    fn links_are_not_followed() {
        let r = root("links");
        let who = "native/l";
        ask(&r, who, "storage.set", json!({"key": "a", "value": "1"})).unwrap();
        let dir = r.join("native%2Fl");
        let outside = r.parent().unwrap().join("outside");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("evil")).unwrap();
        assert_eq!(
            ask(&r, who, "storage.get", json!({"key": "evil"})).unwrap()["value"],
            json!(null)
        );
        let list = ask(&r, who, "storage.list", json!({})).unwrap();
        assert_eq!(list["keys"], json!([{"key": "a", "size": 1}]));
        // Written over: the link replaced, the file outside untouched.
        ask(
            &r,
            who,
            "storage.set",
            json!({"key": "evil", "value": "mine"}),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
        assert!(
            !std::fs::symlink_metadata(dir.join("evil"))
                .unwrap()
                .is_symlink()
        );
        // A planted temp name does not divert the write.
        std::os::unix::fs::symlink(&outside, dir.join(".b.new")).unwrap();
        ask(&r, who, "storage.set", json!({"key": "b", "value": "2"})).unwrap();
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
        // The app's directory itself a link: refused, not followed.
        let other = r.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&other).unwrap();
        std::os::unix::fs::symlink(&other, r.join("native%2Fm")).unwrap();
        let e = ask(
            &r,
            "native/m",
            "storage.set",
            json!({"key": "a", "value": "1"}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::NotUsable);
        assert_eq!(std::fs::read_dir(&other).unwrap().count(), 0);
    }
}
