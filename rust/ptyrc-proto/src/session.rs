// SPDX-License-Identifier: GPL-3.0-or-later
//! Sessions: where the sockets are, which ones are live, which one to use.

use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// Environment variable naming the session; set by the driver for the app.
pub const ENV_SESSION: &str = "PTYRC_SESSION";
/// Environment variable holding the socket path; set by the driver.
pub const ENV_SOCKET: &str = "PTYRC_SOCKET";

const SUFFIX: &str = ".sock";

/// A live session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub path: PathBuf,
}

/// The socket directory: `$XDG_RUNTIME_DIR/ptyrc`, else `/tmp/ptyrc-<uid>`.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("ptyrc"),
        _ => PathBuf::from(format!("/tmp/ptyrc-{}", rustix::process::getuid().as_raw())),
    }
}

/// Create the socket directory, mode 0700. An existing one must be ours
/// and closed to others.
pub fn ensure_runtime_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let dir = runtime_dir();
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || meta.uid() != rustix::process::getuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: not a directory of ours", dir.display()),
        ));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// A session name: `[A-Za-z0-9._-]+`, not starting with a dot.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Socket path of a session.
pub fn socket_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("{name}{SUFFIX}"))
}

/// Whether a driver answers on the socket.
pub fn is_live(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

/// Live sessions, sorted by name. Sockets nobody listens on are removed.
pub fn list() -> Vec<Session> {
    let dir = runtime_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(name) = file.to_str().and_then(|f| f.strip_suffix(SUFFIX)) else {
            continue;
        };
        if !valid_name(name) || !entry.file_type().is_ok_and(|t| t.is_socket()) {
            continue;
        }
        let path = entry.path();
        match UnixStream::connect(&path) {
            Ok(_) => sessions.push(Session { name: name.to_string(), path }),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                let _ = fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    sessions.sort_by(|a, b| a.name.cmp(&b.name));
    sessions
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectError {
    /// No live session.
    None,
    /// The named session is not live.
    NotFound(String),
    /// Several live sessions and none named.
    Ambiguous(Vec<String>),
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SelectError::None => f.write_str("no ptyrc session; start one with `ptyrc run`"),
            SelectError::NotFound(name) => write!(f, "no such session: {name}"),
            SelectError::Ambiguous(names) => {
                write!(f, "several sessions, pick one with --session: {}", names.join(", "))
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// The session to talk to: `explicit`, else `PTYRC_SESSION`, else the only
/// live one.
pub fn select(explicit: Option<&str>) -> Result<Session, SelectError> {
    let env = std::env::var(ENV_SESSION).ok().filter(|s| !s.is_empty());
    if let Some(name) = explicit.map(str::to_string).or(env) {
        let path = socket_path(&name);
        if valid_name(&name) && is_live(&path) {
            return Ok(Session { name, path });
        }
        return Err(SelectError::NotFound(name));
    }
    let mut live = list();
    match live.len() {
        0 => Err(SelectError::None),
        1 => Ok(live.remove(0)),
        _ => Err(SelectError::Ambiguous(live.into_iter().map(|s| s.name).collect())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("t1"));
        assert!(valid_name("a.b_c-9"));
        assert!(!valid_name(""));
        assert!(!valid_name(".hidden"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("a b"));
    }
}
