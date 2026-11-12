// SPDX-License-Identifier: GPL-3.0-or-later
//! Why a session did not start, in words a user can act on.

use ptyrc_proto::suggest;
use std::fmt;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Kind of failure, and the exit status `ptyrc run` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunErrorKind {
    /// Bad arguments: 2.
    Usage,
    /// The session name is taken, or the socket cannot be made: 3.
    Session,
    /// Anything else of the system: 5.
    Io,
    /// The command is there and cannot be run: 126, as a shell.
    NotExecutable,
    /// The command is not there: 127, as a shell.
    NotFound,
}

/// A session that did not start, or broke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError {
    pub kind: RunErrorKind,
    pub message: String,
    /// What to try.
    pub hint: Option<String>,
}

impl RunError {
    pub fn new(kind: RunErrorKind, message: impl Into<String>) -> RunError {
        RunError {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> RunError {
        self.hint = Some(hint.into());
        self
    }

    pub fn exit_status(&self) -> u8 {
        match self.kind {
            RunErrorKind::Usage => 2,
            RunErrorKind::Session => 3,
            RunErrorKind::Io => 5,
            RunErrorKind::NotExecutable => 126,
            RunErrorKind::NotFound => 127,
        }
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RunError {}

impl From<io::Error> for RunError {
    fn from(e: io::Error) -> RunError {
        RunError::new(RunErrorKind::Io, describe(&e))
    }
}

/// An I/O error without the `(os error N)` tail.
pub(crate) fn describe(e: &io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

/// A command of `PATH` whose name is close to `program`.
fn similar_command(program: &str) -> Option<String> {
    let mut names = Vec::new();
    for dir in path_dirs() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string()
                && name.len().abs_diff(program.len()) <= 2
                && suggest::distance(&name.to_lowercase(), &program.to_lowercase()) <= 2
            {
                names.push(name);
            }
        }
    }
    suggest::nearest(program, names.iter().map(String::as_str)).map(str::to_string)
}

/// Check that `program` can be run, as `execvp` will look for it.
pub fn check_command(program: &str) -> Result<(), RunError> {
    let not_found = |m: String| RunError::new(RunErrorKind::NotFound, m);
    let not_exec = |m: String| RunError::new(RunErrorKind::NotExecutable, m);
    if program.is_empty() {
        return Err(RunError::new(RunErrorKind::Usage, "the command is empty")
            .with_hint("ptyrc run -- CMD [ARG...]"));
    }
    if program.contains('/') {
        let path = Path::new(program);
        return match path.metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
                let hint = match parent {
                    Some(dir) if !dir.exists() => {
                        format!("the directory {} does not exist", dir.display())
                    }
                    _ => "check the path; a command of PATH is given without a `/`".into(),
                };
                Err(not_found(format!("{program}: no such file")).with_hint(hint))
            }
            Err(e) => Err(not_exec(format!("{program}: {}", describe(&e)))),
            Ok(m) if m.is_dir() => Err(not_exec(format!("{program}: is a directory"))
                .with_hint("give a program to run, or none for your shell")),
            Ok(m) if m.permissions().mode() & 0o111 == 0 => Err(not_exec(format!(
                "{program}: not executable"
            ))
            .with_hint(format!(
                "chmod +x {program}, or run it through its interpreter"
            ))),
            Ok(_) => Ok(()),
        };
    }
    let dirs = path_dirs();
    if dirs.iter().any(|d| executable(&d.join(program))) {
        return Ok(());
    }
    if let Some(dir) = dirs.iter().find(|d| d.join(program).is_file()) {
        let path = dir.join(program);
        return Err(not_exec(format!("{}: not executable", path.display()))
            .with_hint(format!("chmod +x {}", path.display())));
    }
    let hint = if Path::new(program).is_file() {
        format!("a file of that name is here: ./{program}")
    } else {
        match similar_command(program) {
            Some(name) => format!("did you mean {name}?"),
            None => "check the spelling, or give a path".into(),
        }
    };
    Err(not_found(format!("{program}: command not found in PATH")).with_hint(hint))
}

/// The error of an `exec` that failed after `check_command` passed.
pub fn exec_error(program: &str, e: &io::Error) -> RunError {
    match e.raw_os_error() {
        Some(libc::ENOEXEC) => RunError::new(
            RunErrorKind::NotExecutable,
            format!("{program}: not a program: neither a binary nor a script with a `#!` line"),
        )
        .with_hint(format!(
            "run it through its interpreter: ptyrc run -- sh {program}"
        )),
        Some(libc::ENOENT) => RunError::new(
            RunErrorKind::NotExecutable,
            format!("{program}: its interpreter is missing"),
        )
        .with_hint("look at the `#!` line of the script, or at the loader of the binary"),
        Some(libc::EACCES) => RunError::new(
            RunErrorKind::NotExecutable,
            format!("{program}: permission denied"),
        ),
        _ => RunError::new(RunErrorKind::Io, format!("{program}: {}", describe(e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert!(check_command("sh").is_ok());
        assert!(check_command("/bin/sh").is_ok());
        let e = check_command("no-such-command-zz").unwrap_err();
        assert_eq!(e.kind, RunErrorKind::NotFound);
        assert_eq!(e.exit_status(), 127);
        let e = check_command("/no/such/dir/x").unwrap_err();
        assert_eq!(e.kind, RunErrorKind::NotFound);
        assert!(e.hint.unwrap().contains("/no/such/dir does not exist"));
        let e = check_command("/tmp").unwrap_err();
        assert_eq!(e.kind, RunErrorKind::NotExecutable);
        assert_eq!(e.message, "/tmp: is a directory");
        let e = check_command("/etc/passwd").unwrap_err();
        assert_eq!(e.exit_status(), 126);
        assert_eq!(e.message, "/etc/passwd: not executable");
    }

    #[test]
    fn io_errors_lose_their_number() {
        let e = io::Error::from_raw_os_error(libc::ENOENT);
        assert_eq!(describe(&e), "No such file or directory");
    }
}
