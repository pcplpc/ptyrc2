// SPDX-License-Identifier: GPL-3.0-or-later
//! One error type: what was refused, and the cure.

use std::fmt;

/// Where an error arose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// `ptyrc-app.toml`: syntax, a key, a value.
    Manifest,
    /// An origin or a `@REF`.
    Origin,
    /// The tree: a bound, a kind of file, a missing file.
    Tree,
    /// `git`: spawn, fetch, read.
    Fetch,
    /// The installed trees, `apps.lock`.
    Install,
    /// A registry pointer file, `store.toml`, the choice of a version.
    Registry,
    /// Planned, not built: Rust apps.
    Unsupported,
    /// The file system.
    Io,
}

/// What was refused (`message`) and the cure (`hint`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub hint: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Error {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn io(what: impl fmt::Display, e: std::io::Error) -> Error {
        Error::new(ErrorKind::Io, format!("{what}: {e}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {hint})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// A relative path inside an app tree: `/`-separated, no empty, `.`,
/// `..` or `.git` component, no backslash, no control character.
pub(crate) fn check_rel_path(path: &str) -> std::result::Result<(), &'static str> {
    if path.is_empty() {
        return Err("empty");
    }
    if path.len() > 1024 {
        return Err("longer than 1024 bytes");
    }
    if path.starts_with('/') {
        return Err("absolute");
    }
    if path.chars().any(|c| c.is_control() || c == '\\') {
        return Err("a control character or a backslash");
    }
    for part in path.split('/') {
        match part {
            "" => return Err("an empty component"),
            "." | ".." => return Err("a `.` or `..` component"),
            p if p.eq_ignore_ascii_case(".git") => return Err("a `.git` component"),
            _ => {}
        }
    }
    Ok(())
}

/// 1-based line of a byte offset.
pub(crate) fn line_of(src: &str, offset: usize) -> usize {
    let end = offset.min(src.len());
    src.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}
