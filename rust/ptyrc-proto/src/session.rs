// SPDX-License-Identifier: GPL-3.0-or-later
//! Sessions: where the sockets are, which ones are live, which one to use.

use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Environment variable naming the session; set by the driver for the app.
pub const ENV_SESSION: &str = "PTYRC_SESSION";
/// Environment variable holding the socket path; set by the driver.
pub const ENV_SOCKET: &str = "PTYRC_SOCKET";

/// In place of a session name: the session the user typed in last.
pub const AUTO: &str = "auto";

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

/// Create the socket directory, mode 0700. An existing one must be ours;
/// one open to others is closed.
pub fn ensure_runtime_dir() -> io::Result<PathBuf> {
    let dir = runtime_dir();
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    match check_dir(&dir)? {
        Ok(()) => {}
        Err(Unsafe::Open) => fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?,
        Err(e) => return Err(e.error(&dir)),
    }
    Ok(dir)
}

/// Why a socket directory is not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unsafe {
    /// Not a directory (a symlink is none), or of another user.
    Foreign,
    /// Ours, with a mode bit for the group or the others.
    Open,
}

impl Unsafe {
    fn error(self, dir: &Path) -> io::Error {
        let what = match self {
            Unsafe::Foreign => "not a directory of ours",
            Unsafe::Open => "open to other users",
        };
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {what}", dir.display()),
        )
    }
}

/// Whether `dir` is a directory of this user, mode 0700 at most. The
/// symlink is looked at, not its target.
fn check_dir(dir: &Path) -> io::Result<Result<(), Unsafe>> {
    let meta = fs::symlink_metadata(dir)?;
    Ok(
        if !meta.is_dir() || meta.uid() != rustix::process::getuid().as_raw() {
            Err(Unsafe::Foreign)
        } else if meta.permissions().mode() & 0o077 != 0 {
            Err(Unsafe::Open)
        } else {
            Ok(())
        },
    )
}

/// The socket directory as a client takes it: `None` when there is
/// none. One that is not ours, or open to others, is refused: whoever
/// made it may listen there in a driver's place.
pub fn trusted_dir() -> io::Result<Option<PathBuf>> {
    let dir = runtime_dir();
    match check_dir(&dir) {
        Ok(Ok(())) => Ok(Some(dir)),
        Ok(Err(e)) => Err(e.error(&dir)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Connect to a driver's socket. The listener must run as this user:
/// a socket of another is closed at once (`PermissionDenied`).
pub fn connect(path: &Path) -> io::Result<UnixStream> {
    let stream = UnixStream::connect(path)?;
    let peer = rustix::net::sockopt::socket_peercred(&stream)?;
    if peer.uid.as_raw() != rustix::process::getuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: the listener is another user", path.display()),
        ));
    }
    Ok(stream)
}

/// A session name: `[A-Za-z0-9._-]+`, not starting with a dot, not
/// `auto`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != AUTO
        && !name.starts_with('.')
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Socket path of a session.
pub fn socket_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("{name}{SUFFIX}"))
}

/// Whether a driver answers on the socket.
pub fn is_live(path: &Path) -> bool {
    connect(path).is_ok()
}

/// Live sessions, sorted by name. Sockets nobody listens on are removed.
/// None in a directory that is not trusted (`trusted_dir`).
pub fn list() -> Vec<Session> {
    let Ok(Some(dir)) = trusted_dir() else {
        return Vec::new();
    };
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
        match connect(&path) {
            Ok(_) => sessions.push(Session {
                name: name.to_string(),
                path,
            }),
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
    /// Not a session name.
    BadName(String),
    /// The named session is not live; the live ones.
    NotFound(String, Vec<String>),
    /// The named session left its socket behind: the driver is gone.
    Dead(String),
    /// Several live sessions and none named.
    Ambiguous(Vec<String>),
    /// `auto`, several live sessions, and the user typed in none.
    Untouched(Vec<String>),
    /// The socket directory is not ours, or open to others: why.
    Unsafe(String),
}

impl SelectError {
    /// What to try.
    pub fn hint(&self) -> String {
        match self {
            SelectError::None => "start one: ptyrc run -- CMD".into(),
            SelectError::BadName(_) => {
                "a name is made of letters, digits, `.`, `_`, `-`; list sessions: ptyrc ls".into()
            }
            SelectError::NotFound(name, live) if live.is_empty() => {
                format!("no session is running; start it: ptyrc run --name {name} -- CMD")
            }
            SelectError::NotFound(name, live) => format!(
                "{}running: {}",
                crate::suggest::did_you_mean(name, live.iter().map(String::as_str)),
                live.join(", ")
            ),
            SelectError::Dead(name) => {
                format!("start it again: ptyrc run --name {name} -- CMD")
            }
            SelectError::Ambiguous(names) => format!(
                "pick one with -s NAME or {ENV_SESSION}: {}",
                names.join(", ")
            ),
            SelectError::Untouched(names) => {
                format!("pick one with -s NAME: {}", names.join(", "))
            }
            SelectError::Unsafe(_) => {
                "the socket directory must be yours, mode 0700; remove it, or set XDG_RUNTIME_DIR"
                    .into()
            }
        }
    }
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SelectError::None => f.write_str("no session"),
            SelectError::BadName(name) => write!(f, "bad session name: {name}"),
            SelectError::NotFound(name, _) => write!(f, "no such session: {name}"),
            SelectError::Dead(name) => write!(
                f,
                "session {name} is dead: its driver is gone, the stale socket was removed"
            ),
            SelectError::Ambiguous(names) => write!(f, "{} sessions, none chosen", names.len()),
            SelectError::Untouched(names) => write!(
                f,
                "{} sessions, the user typed in none: auto has no choice",
                names.len()
            ),
            SelectError::Unsafe(why) => write!(f, "socket directory {why}"),
        }
    }
}

impl std::error::Error for SelectError {}

/// The session a tmux session names in its option `@ptyrc` (set by
/// `ptyrc tmux attach`), for a process in a pane of it: `$TMUX_PANE` is
/// asked of the server of `$TMUX`.
fn of_tmux() -> Option<String> {
    let set = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    set("TMUX")?;
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &set("TMUX_PANE")?,
            "#{@ptyrc}",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && valid_name(&name)).then_some(name)
}

/// When the user last typed on the host keyboard of the session at
/// `path`: `active_at` of its `hello`, milliseconds since the epoch.
fn active_at(path: &Path) -> Option<u64> {
    let stream = connect(path).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    let hello: serde_json::Value = serde_json::from_str(&line).ok()?;
    hello["data"]["active_at"].as_u64()
}

/// What `auto` chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auto {
    pub session: Session,
    /// The live sessions it chose among.
    pub among: usize,
    /// When the user typed there last, milliseconds since the epoch;
    /// `None`: never, the session is the only one.
    pub active_at: Option<u64>,
}

/// The session the user typed in last, on its host keyboard; the only
/// live one, typed in or not. Sessions without a host (headless, a
/// replay) are typed in by nobody. The same date: the last by name.
pub fn auto() -> Result<Auto, SelectError> {
    if let Err(e) = trusted_dir() {
        return Err(SelectError::Unsafe(e.to_string()));
    }
    let mut live = list();
    let among = live.len();
    let dates: Vec<Option<u64>> = live.iter().map(|s| active_at(&s.path)).collect();
    let best = (0..among)
        .filter(|i| dates[*i].is_some())
        .max_by_key(|i| dates[*i]);
    match (best, among) {
        (_, 0) => Err(SelectError::None),
        (Some(i), _) => Ok(Auto {
            session: live.swap_remove(i),
            among,
            active_at: dates[i],
        }),
        (None, 1) => Ok(Auto {
            session: live.remove(0),
            among,
            active_at: None,
        }),
        (None, _) => Err(SelectError::Untouched(
            live.into_iter().map(|s| s.name).collect(),
        )),
    }
}

/// The session to talk to: `explicit`, else `PTYRC_SESSION`, else the one
/// over the tmux session this process is in, else the only live one.
/// The name `auto`: `auto()`.
pub fn select(explicit: Option<&str>) -> Result<Session, SelectError> {
    if let Err(e) = trusted_dir() {
        return Err(SelectError::Unsafe(e.to_string()));
    }
    let env = std::env::var(ENV_SESSION).ok().filter(|s| !s.is_empty());
    if let Some(name) = explicit.map(str::to_string).or(env) {
        if name == AUTO {
            return auto().map(|auto| auto.session);
        }
        if !valid_name(&name) {
            return Err(SelectError::BadName(name));
        }
        let path = socket_path(&name);
        return match connect(&path) {
            Ok(_) => Ok(Session { name, path }),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                let _ = fs::remove_file(&path);
                Err(SelectError::Dead(name))
            }
            Err(_) => {
                let live = list().into_iter().map(|s| s.name).collect();
                Err(SelectError::NotFound(name, live))
            }
        };
    }
    if let Some(name) = of_tmux() {
        let path = socket_path(&name);
        if is_live(&path) {
            return Ok(Session { name, path });
        }
    }
    let mut live = list();
    match live.len() {
        0 => Err(SelectError::None),
        1 => Ok(live.remove(0)),
        _ => Err(SelectError::Ambiguous(
            live.into_iter().map(|s| s.name).collect(),
        )),
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
        assert!(!valid_name(AUTO));
    }

    #[test]
    fn a_directory_is_checked() {
        let dir = std::env::temp_dir().join(format!("ptyrc-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(check_dir(&dir).is_err());
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        assert_eq!(check_dir(&dir).unwrap(), Ok(()));
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(check_dir(&dir).unwrap(), Err(Unsafe::Open));
        // A symlink to a good directory is no directory.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert_eq!(check_dir(&link).unwrap(), Err(Unsafe::Foreign));
        assert_eq!(check_dir(Path::new("/")).unwrap(), Err(Unsafe::Foreign));
        fs::remove_dir_all(&dir).unwrap();
    }
}
