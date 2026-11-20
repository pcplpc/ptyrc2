// SPDX-License-Identifier: GPL-3.0-or-later
//! `file.*`: the files the user named at the start, by handle. The
//! params are checked on the driver's loop (`request`); the disk is
//! read on the worker (`Request::run`). The directory holding the name
//! is opened and its real path compared with the one named; the name
//! is opened under it with `O_NOFOLLOW`: a regular file or nothing.

use crate::keys::{base64_decode, base64_encode};
use crate::storage::valid_key;
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_store::FileKind;
use rustix::fd::{AsFd, AsRawFd, OwnedFd};
use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags};
use rustix::io::Errno;
use serde_json::{Value, json};
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// One answer of `file.read`, at most.
pub(crate) const MAX_READ: u64 = 1 << 20;
/// A file read, at most (by pieces of `MAX_READ`).
pub(crate) const MAX_FILE: u64 = 16 << 20;
/// `file.write`, at most.
pub(crate) const MAX_WRITE: usize = 1 << 20;
/// Entries of `file.list`, at most.
pub(crate) const MAX_ENTRIES: usize = 1024;

pub(crate) const FILE_HINT: &str =
    "a file is named by the user at the start: [options] x = { type = \"file\" }";
const NAME_RULE: &str = "a name: 1 to 64 bytes of [A-Za-z0-9._-], not starting with `.`";

/// The `no-file` of a handle not given to the app.
pub(crate) fn no_handle(params: &Value) -> RpcError {
    let handle = params.get("file").and_then(Value::as_str).unwrap_or("");
    RpcError::new(ErrorCode::NoFile, format!("no file {handle:?}")).with_hint(FILE_HINT)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Stat,
    Read {
        offset: u64,
        len: u64,
        base64: bool,
    },
    List,
    Write {
        bytes: Vec<u8>,
        if_mtime: Option<i64>,
    },
}

/// A request checked: what the worker does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Request {
    pub method: &'static str,
    /// The path named, canonical.
    pub path: PathBuf,
    pub kind: FileKind,
    /// A file of the directory `path`.
    pub child: Option<String>,
    pub op: Op,
}

fn bad(message: impl Into<String>, hint: &str) -> RpcError {
    RpcError::bad_params(message).with_hint(hint)
}

fn not_usable(message: impl Into<String>, hint: &str) -> RpcError {
    RpcError::new(ErrorCode::NotUsable, message).with_hint(hint)
}

/// The params of `method` on the handle `(path, kind, writable)`, in the
/// order of the headless runner (`ptyrc-box`, `hostfn.rs`).
pub(crate) fn request(
    method: &'static str,
    params: &Value,
    path: PathBuf,
    kind: FileKind,
    writable: bool,
) -> Result<Request, RpcError> {
    let child = match params.get("name") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_str().filter(|n| valid_key(n)) {
            Some(n) => Some(n.to_owned()),
            None => return Err(bad(format!("{method}: name"), NAME_RULE)),
        },
    };
    let shown = last(&path);
    let dir = kind == FileKind::Dir;
    if !dir && (child.is_some() || method == "file.list") {
        return Err(bad(
            format!("{method}: {shown} is a file, not a directory"),
            "name: a file of a directory handle",
        ));
    }
    let op = match method {
        "file.write" => {
            if dir {
                return Err(bad(
                    "file.write: a directory is never written",
                    "a file handle",
                ));
            }
            if !writable {
                return Err(
                    RpcError::new(ErrorCode::Denied, "file.write: denied").with_hint(
                        "a box ticked is written back: the manifest's `write = true` and \
                     `files-write`",
                    ),
                );
            }
            let bytes = match (params.get("text"), params.get("base64")) {
                (Some(Value::String(t)), None) => t.as_bytes().to_vec(),
                (None, Some(Value::String(b))) => base64_decode(b)
                    .ok_or_else(|| bad("file.write: base64", "the content, base64"))?,
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
            let if_mtime = params.get("if_mtime_ms").filter(|v| !v.is_null());
            // Not a number: no mtime is it.
            let if_mtime = if_mtime.map(|v| v.as_i64().unwrap_or(i64::MIN));
            Op::Write { bytes, if_mtime }
        }
        "file.list" if child.is_none() => Op::List,
        _ if dir && child.is_none() => {
            return Err(bad(
                format!("{method}: name: a file of the directory {shown}"),
                "file.list names them",
            ));
        }
        "file.read" => {
            let num = |k: &str, default: u64| match params.get(k) {
                None | Some(Value::Null) => Ok(default),
                Some(v) => v.as_u64().ok_or_else(|| {
                    RpcError::bad_params(format!("{method}: {k}: a count of bytes"))
                }),
            };
            let offset = num("offset", 0)?;
            let len = num("len", MAX_READ)?.min(MAX_READ);
            let base64 = match params.get("as") {
                None | Some(Value::Null) => false,
                Some(v) if v == "text" => false,
                Some(v) if v == "base64" => true,
                Some(_) => {
                    return Err(bad(
                        format!("{method}: as"),
                        "\"text\" (UTF-8, invalid bytes replaced) or \"base64\"",
                    ));
                }
            };
            Op::Read {
                offset,
                len,
                base64,
            }
        }
        // `file.list {name}`: a file of it is no directory.
        "file.list" => {
            return Err(bad(
                format!("{method}: name: a directory is listed, never deeper"),
                "file.list {file}",
            ));
        }
        _ => Op::Stat,
    };
    Ok(Request {
        method,
        path,
        kind,
        child,
        op,
    })
}

/// The last component, as shown in a message.
fn last(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn io_error(method: &str, what: &str, e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::NotUsable, format!("{method}: {what}: {e}"))
}

fn mtime_ms(stat: &rustix::fs::Stat) -> i64 {
    let secs: i64 = stat.st_mtime;
    let nsec: i64 = stat.st_mtime_nsec as i64;
    secs.saturating_mul(1000).saturating_add(nsec / 1_000_000)
}

/// The directory `dir`, opened; refused when its real path is not
/// `dir` (a component replaced by a link since it was named).
fn open_dir(method: &str, dir: &Path) -> Result<OwnedFd, RpcError> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let moved = || {
        not_usable(
            format!("{method}: {}: moved since it was named", dir.display()),
            "the user names it again at the start",
        )
    };
    let fd = match rustix::fs::open(dir, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return Err(moved()),
        Err(e) => return Err(io_error(method, &dir.display().to_string(), e)),
    };
    let real = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()));
    match real {
        Ok(real) if real == dir => Ok(fd),
        _ => Err(moved()),
    }
}

/// `name` under `dir`, not followed: a regular file, or `None`.
fn open_leaf(
    method: &str,
    dir: &impl AsFd,
    name: &str,
) -> Result<Option<(std::fs::File, rustix::fs::Stat)>, RpcError> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::openat(dir, name, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::NOENT | Errno::LOOP | Errno::NXIO) => return Ok(None),
        Err(e) => return Err(io_error(method, name, e)),
    };
    let stat = rustix::fs::fstat(&fd).map_err(|e| io_error(method, name, e))?;
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => Ok(Some((std::fs::File::from(fd), stat))),
        _ => Ok(None),
    }
}

impl Request {
    /// The directory and the name under it.
    fn place(&self) -> Result<(OwnedFd, String), RpcError> {
        let m = self.method;
        match (&self.kind, &self.child) {
            (FileKind::Dir, child) => {
                let fd = open_dir(m, &self.path)?;
                Ok((fd, child.clone().unwrap_or_default()))
            }
            (FileKind::File, _) => {
                let parent = self.path.parent().unwrap_or(Path::new("/"));
                let fd = open_dir(m, parent)?;
                Ok((fd, last(&self.path)))
            }
        }
    }

    /// Done on the worker.
    pub fn run(self) -> Result<Value, RpcError> {
        let (dir, name) = self.place()?;
        let m = self.method;
        match self.op {
            Op::List => list(m, &dir),
            Op::Stat => Ok(match open_leaf(m, &dir, &name)? {
                None => json!({"exists": false}),
                Some((_, st)) => {
                    json!({"exists": true, "size": st.st_size, "mtime_ms": mtime_ms(&st)})
                }
            }),
            Op::Read {
                offset,
                len,
                base64,
            } => {
                let Some((file, st)) = open_leaf(m, &dir, &name)? else {
                    return Err(RpcError::new(
                        ErrorCode::NoFile,
                        format!("{m}: {name} does not exist"),
                    ));
                };
                let size = st.st_size as u64;
                if size > MAX_FILE {
                    return Err(not_usable(
                        format!("{m}: {name}: more than {MAX_FILE} bytes"),
                        "a file named is read whole, 16 MiB at most, by pieces of 1 MiB",
                    ));
                }
                let start = offset.min(size);
                let end = start.saturating_add(len).min(size);
                let mut piece = vec![0; (end - start) as usize];
                let mut got = 0;
                while got < piece.len() {
                    match file.read_at(&mut piece[got..], start + got as u64) {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(e) => return Err(io_error(m, &name, e)),
                    }
                }
                piece.truncate(got);
                let mut out = json!({
                    "size": size,
                    "eof": start + got as u64 >= size,
                    "mtime_ms": mtime_ms(&st),
                });
                match base64 {
                    true => out["base64"] = json!(base64_encode(&piece)),
                    false => out["text"] = json!(String::from_utf8_lossy(&piece)),
                }
                Ok(out)
            }
            Op::Write { bytes, if_mtime } => write(m, &dir, &name, &bytes, if_mtime),
        }
    }
}

/// The regular files of `dir` of a name that keeps the rule, by name.
fn list(method: &str, dir: &OwnedFd) -> Result<Value, RpcError> {
    let entries = Dir::read_from(dir).map_err(|e| io_error(method, "list", e))?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_error(method, "list", e))?;
        let Ok(name) = entry.file_name().to_str() else {
            continue;
        };
        if !valid_key(name) {
            continue;
        }
        let Ok(st) = rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) else {
            continue;
        };
        if FileType::from_raw_mode(st.st_mode) == FileType::RegularFile {
            out.push((name.to_owned(), st.st_size as u64));
        }
    }
    out.sort();
    out.truncate(MAX_ENTRIES);
    let entries: Vec<Value> = out
        .into_iter()
        .map(|(name, size)| json!({"name": name, "size": size}))
        .collect();
    Ok(json!({"entries": entries}))
}

/// The whole of `name` under `dir`: a temp file beside it, the mode of
/// the file it replaces (0600 for a new one), a rename.
fn write(
    method: &str,
    dir: &OwnedFd,
    name: &str,
    bytes: &[u8],
    if_mtime: Option<i64>,
) -> Result<Value, RpcError> {
    let old = match rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) => Some(st),
        Err(Errno::NOENT) => None,
        Err(e) => return Err(io_error(method, name, e)),
    };
    let regular = old.filter(|st| FileType::from_raw_mode(st.st_mode) == FileType::RegularFile);
    if let Some(want) = if_mtime
        && regular.is_none_or(|st| mtime_ms(&st) != want)
    {
        return Err(not_usable(
            "file.write: changed since it was read",
            "read it again, then write",
        ));
    }
    if old.is_some_and(|st| FileType::from_raw_mode(st.st_mode) == FileType::Directory) {
        return Err(io_error(method, name, "a directory holds the name"));
    }
    let mode = regular.map_or(0o600, |st| st.st_mode & 0o7777);
    // Hidden, and not a name of the rule: never listed.
    let temp = format!(".{}.ptyrc-new", std::process::id());
    let _ = rustix::fs::unlinkat(dir, &temp, AtFlags::empty());
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let fd = rustix::fs::openat(dir, &temp, flags, Mode::from_raw_mode(0o600))
        .map_err(|e| io_error(method, name, e))?;
    let done = (|| {
        rustix::fs::fchmod(&fd, Mode::from_raw_mode(mode)).map_err(std::io::Error::from)?;
        let mut file = std::fs::File::from(fd.try_clone()?);
        file.write_all(bytes)?;
        file.sync_all()?;
        let st = rustix::fs::fstat(&fd)?;
        rustix::fs::renameat(dir, &temp, dir, name)?;
        Ok::<_, std::io::Error>(st)
    })();
    match done {
        Ok(st) => Ok(json!({"size": bytes.len(), "mtime_ms": mtime_ms(&st)})),
        Err(e) => {
            let _ = rustix::fs::unlinkat(dir, &temp, AtFlags::empty());
            Err(io_error(method, name, e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ptyrc-files-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn ask(
        method: &'static str,
        path: &Path,
        kind: FileKind,
        writable: bool,
        params: Value,
    ) -> Result<Value, RpcError> {
        request(method, &params, path.to_path_buf(), kind, writable)?.run()
    }

    fn file(method: &'static str, path: &Path, params: Value) -> Result<Value, RpcError> {
        ask(method, path, FileKind::File, true, params)
    }

    #[test]
    fn read_stat_write() {
        let d = tmp("rsw");
        let f = d.join("TODO.md");
        assert_eq!(
            file("file.stat", &f, json!({})).unwrap(),
            json!({"exists": false})
        );
        let e = file("file.read", &f, json!({})).unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (ErrorCode::NoFile, "file.read: TODO.md does not exist")
        );
        let w = file("file.write", &f, json!({"text": "- a\n"})).unwrap();
        assert_eq!(w["size"], 4);
        let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let s = file("file.stat", &f, json!({})).unwrap();
        assert_eq!(
            (s["exists"].clone(), s["size"].clone()),
            (json!(true), json!(4))
        );
        assert_eq!(s["mtime_ms"], w["mtime_ms"]);
        let r = file("file.read", &f, json!({})).unwrap();
        assert_eq!(
            r,
            json!({"text": "- a\n", "size": 4, "eof": true, "mtime_ms": w["mtime_ms"]})
        );
        let r = file(
            "file.read",
            &f,
            json!({"as": "base64", "offset": 2, "len": 1}),
        )
        .unwrap();
        assert_eq!(
            (r["base64"].clone(), r["eof"].clone()),
            (json!("YQ=="), json!(false))
        );
        // Past the end: nothing, eof.
        let r = file("file.read", &f, json!({"offset": 99})).unwrap();
        assert_eq!(
            (r["text"].clone(), r["eof"].clone()),
            (json!(""), json!(true))
        );
        // The mode kept.
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
        let m = file("file.stat", &f, json!({})).unwrap()["mtime_ms"].clone();
        file(
            "file.write",
            &f,
            json!({"base64": "eHk=", "if_mtime_ms": m}),
        )
        .unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"xy");
        let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        // Not the mtime of now: nothing written.
        let e = file("file.write", &f, json!({"text": "z", "if_mtime_ms": 1})).unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (
                ErrorCode::NotUsable,
                "file.write: changed since it was read"
            )
        );
        assert_eq!(std::fs::read(&f).unwrap(), b"xy");
        // No temp file left.
        let names: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["TODO.md"]);
    }

    #[test]
    fn the_checks_of_the_params() {
        let d = tmp("params");
        let f = d.join("a");
        std::fs::write(&f, "x").unwrap();
        let code = |m, path: &Path, kind, w, p| ask(m, path, kind, w, p).unwrap_err().code;
        use FileKind::{Dir as D, File as F};
        for name in ["../x", "a/b", ".x", "", "..", "a\0b"] {
            assert_eq!(
                code("file.read", &d, D, false, json!({"name": name})),
                ErrorCode::BadParams,
                "{name:?}"
            );
        }
        assert_eq!(
            code("file.read", &f, F, false, json!({"name": "b"})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.list", &f, F, false, json!({})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.read", &d, D, false, json!({})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.stat", &d, D, false, json!({})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.write", &d, D, true, json!({"text": ""})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.write", &f, F, false, json!({"text": ""})),
            ErrorCode::Denied
        );
        assert_eq!(
            code("file.write", &f, F, true, json!({})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.write", &f, F, true, json!({"text": "", "base64": ""})),
            ErrorCode::BadParams
        );
        let big = "x".repeat(MAX_WRITE + 1);
        assert_eq!(
            code("file.write", &f, F, true, json!({"text": big})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.read", &f, F, false, json!({"as": "hex"})),
            ErrorCode::BadParams
        );
        assert_eq!(
            code("file.read", &f, F, false, json!({"offset": -1})),
            ErrorCode::BadParams
        );
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
    }

    #[test]
    fn a_directory() {
        let d = tmp("dir");
        std::fs::write(d.join("ship.pic"), "abc").unwrap();
        std::fs::write(d.join(".hidden"), "h").unwrap();
        std::fs::write(d.join("a b"), "out of the rule").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/deep"), "d").unwrap();
        let outside = tmp("dir-outside");
        std::fs::write(outside.join("secret"), "s").unwrap();
        symlink(outside.join("secret"), d.join("link")).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            d.join("fifo").as_path(),
            FileType::Fifo,
            Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        let dir = |m, p| ask(m, &d, FileKind::Dir, false, p);
        let l = dir("file.list", json!({})).unwrap();
        assert_eq!(l, json!({"entries": [{"name": "ship.pic", "size": 3}]}));
        let r = dir("file.read", json!({"name": "ship.pic"})).unwrap();
        assert_eq!(r["text"], "abc");
        // A link, a FIFO, a directory: nothing (and no hang).
        for name in ["link", "fifo", "sub", "nope"] {
            let e = dir("file.read", json!({"name": name})).unwrap_err();
            assert_eq!(e.code, ErrorCode::NoFile, "{name}");
            assert_eq!(
                dir("file.stat", json!({"name": name})).unwrap(),
                json!({"exists": false})
            );
        }
        assert_eq!(
            dir("file.list", json!({"name": "sub"})).unwrap_err().code,
            ErrorCode::BadParams
        );
    }

    #[test]
    fn a_link_planted_after_the_naming() {
        let d = tmp("planted");
        let outside = tmp("planted-outside");
        std::fs::write(outside.join("secret"), "s").unwrap();
        let f = d.join("TODO.md");
        std::fs::write(&f, "mine").unwrap();
        assert_eq!(file("file.read", &f, json!({})).unwrap()["text"], "mine");
        // The file replaced by a link: not followed.
        std::fs::remove_file(&f).unwrap();
        symlink(outside.join("secret"), &f).unwrap();
        assert_eq!(
            file("file.read", &f, json!({})).unwrap_err().code,
            ErrorCode::NoFile
        );
        assert_eq!(
            file("file.stat", &f, json!({})).unwrap(),
            json!({"exists": false})
        );
        // A write replaces the link, the target untouched.
        file("file.write", &f, json!({"text": "new"})).unwrap();
        assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"s");
        assert!(std::fs::symlink_metadata(&f).unwrap().is_file());
        // The directory replaced by a link: refused, read nor written.
        let moved = d.with_extension("old");
        std::fs::rename(&d, &moved).unwrap();
        symlink(&outside, &d).unwrap();
        std::fs::write(outside.join("TODO.md"), "theirs").unwrap();
        let e = file("file.read", &f, json!({})).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotUsable);
        assert!(
            e.message.contains("moved since it was named"),
            "{}",
            e.message
        );
        assert_eq!(
            file("file.write", &f, json!({"text": "x"}))
                .unwrap_err()
                .code,
            ErrorCode::NotUsable
        );
        assert_eq!(std::fs::read(outside.join("TODO.md")).unwrap(), b"theirs");
        let e = ask("file.list", &d, FileKind::Dir, false, json!({})).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotUsable);
    }

    #[test]
    fn bounds() {
        let d = tmp("bounds");
        let f = d.join("big");
        std::fs::File::create(&f)
            .unwrap()
            .set_len(MAX_FILE + 1)
            .unwrap();
        assert_eq!(
            file("file.read", &f, json!({})).unwrap_err().code,
            ErrorCode::NotUsable
        );
        std::fs::File::create(&f)
            .unwrap()
            .set_len(3 * MAX_READ)
            .unwrap();
        let r = file(
            "file.read",
            &f,
            json!({"len": 5 * MAX_READ, "offset": MAX_READ}),
        )
        .unwrap();
        assert_eq!(r["text"].as_str().unwrap().len() as u64, MAX_READ);
        assert_eq!(r["eof"], false);
        for i in 0..(MAX_ENTRIES + 5) {
            std::fs::write(d.join(format!("e{i:05}")), "").unwrap();
        }
        let l = ask("file.list", &d, FileKind::Dir, false, json!({})).unwrap();
        assert_eq!(l["entries"].as_array().unwrap().len(), MAX_ENTRIES);
        assert_eq!(l["entries"][0]["name"], "big");
    }
}
