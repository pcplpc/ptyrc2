// SPDX-License-Identifier: GPL-3.0-or-later
//! The `git` runner: the commands of `Cmd` and no other, hardened
//! against the repository (README, "Audit").

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::parse;

/// The verbs ever run.
pub const COMMANDS: [&str; 8] = [
    "rev-parse",
    "status",
    "diff-files",
    "diff-index",
    "log",
    "stash",
    "for-each-ref",
    "worktree",
];
/// Wall time of one command; past it, its process group is killed.
pub const TIMEOUT: Duration = Duration::from_secs(2);
/// Output of one command; past it, the same.
pub const LIMIT: usize = 4 << 20;
/// Names of filter drivers neutralised by name (`info/attributes`);
/// more: the commands that read the work tree are not run.
pub const FILTERS: usize = 32;

/// A command. Every argument is fixed but `Log`'s count and `Days`'
/// first second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Locate,
    Status,
    DiffFiles,
    DiffIndex,
    Log(u32),
    Days(i64),
    Stashes,
    Branches,
    Worktrees,
}

impl Cmd {
    /// Reads the work tree (and so may meet a filter).
    fn reads_tree(self) -> bool {
        matches!(self, Cmd::Status | Cmd::DiffFiles | Cmd::DiffIndex)
    }

    pub fn args(self) -> Vec<String> {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let diff = ["--numstat", "-z", "--no-ext-diff", "--no-textconv"];
        let mut out = match self {
            Cmd::Locate => s(&[
                "rev-parse",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
                "--show-object-format",
            ]),
            Cmd::Status => s(&["status", "--porcelain=v2", "--branch", "-z"]),
            Cmd::DiffFiles => s(&[&["diff-files"][..], &diff].concat()),
            Cmd::DiffIndex => s(&[&["diff-index", "--cached"][..], &diff].concat()),
            Cmd::Log(n) => {
                let mut v = s(&["log", "--graph", "--no-color", "--no-show-signature"]);
                v.push(format!("--format={}", parse::log_format()));
                v.extend(["-n".to_string(), n.to_string()]);
                v
            }
            Cmd::Days(since) => {
                let mut v = s(&["log", "--no-show-signature", "--format=%ct"]);
                v.push(format!("--since=@{since}"));
                v
            }
            Cmd::Stashes => vec![
                "stash".into(),
                "list".into(),
                format!("--format={}", parse::stash_format()),
            ],
            Cmd::Branches => vec![
                "for-each-ref".into(),
                "--sort=-committerdate".into(),
                format!("--format={}", parse::branch_format()),
                "refs/heads".into(),
            ],
            Cmd::Worktrees => s(&["worktree", "list", "--porcelain", "-z"]),
        };
        if self.reads_tree() {
            out.push("--ignore-submodules=all".into());
        }
        if self == Cmd::DiffIndex {
            out.push("HEAD".into());
        }
        out
    }
}

/// Configuration given on the command line: it wins over the
/// repository's.
const CONFIG: [&str; 9] = [
    "core.fsmonitor=false",
    "core.hooksPath=/dev/null",
    "core.pager=cat",
    "core.quotePath=false",
    "core.attributesFile=/dev/null",
    "log.showSignature=false",
    "color.ui=false",
    "protocol.allow=never",
    "submodule.recurse=false",
];

/// The empty tree of each object format: the source of attributes.
fn empty_tree(format: &str) -> Option<&'static str> {
    match format {
        "sha1" => Some("4b825dc642cb6eb9a060e54bf8d69288fbee4904"),
        "sha256" => Some("6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321"),
        _ => None,
    }
}

/// Names of the filter drivers an attributes file sets (`filter=NAME`,
/// in macros too); none when more than `FILTERS`.
pub fn filters(text: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_ascii_whitespace() {
        if let Some(name) = word.strip_prefix("filter=")
            && !out.iter().any(|n| n == name)
        {
            out.push(name.to_string());
        }
    }
    (out.len() <= FILTERS).then_some(out)
}

/// Runs the commands of `Cmd` in one directory.
#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    dir: PathBuf,
    /// `--attr-source`; none until `locate` told the object format.
    attr_source: Option<&'static str>,
    /// The filter drivers of `info/attributes`; none: unknown.
    filters: Option<Vec<String>>,
}

impl Git {
    /// `program`: `git` (from `PATH`), or another for tests.
    pub fn new(program: impl Into<PathBuf>, dir: &Path) -> Git {
        Git {
            program: program.into(),
            dir: dir.to_path_buf(),
            attr_source: None,
            filters: None,
        }
    }

    /// Once `locate` ran: the object format, and the filter drivers of
    /// `info/attributes` (none: unknown).
    pub fn arm(&mut self, format: &str, filters: Option<Vec<String>>) {
        self.attr_source = empty_tree(format);
        self.filters = filters;
    }

    /// The full argv after the program; none when `cmd` may not run yet.
    pub fn argv(&self, cmd: Cmd) -> Option<Vec<String>> {
        let mut argv = vec![
            "--no-pager".into(),
            "--no-optional-locks".into(),
            "-C".into(),
        ];
        argv.push(self.dir.to_string_lossy().into_owned());
        if cmd != Cmd::Locate {
            argv.push(format!("--attr-source={}", self.attr_source?));
        }
        for c in CONFIG {
            argv.extend(["-c".into(), c.into()]);
        }
        if cmd.reads_tree() {
            for name in self.filters.as_ref()? {
                for key in ["clean=", "smudge=", "process=", "required=false"] {
                    argv.extend(["-c".into(), format!("filter.{name}.{key}")]);
                }
            }
        }
        argv.extend(cmd.args());
        Some(argv)
    }

    /// The output of `cmd`; none when it may not run, fails, outlives
    /// `TIMEOUT` or writes more than `LIMIT`.
    pub fn run(&self, cmd: Cmd) -> Option<String> {
        let argv = self.argv(cmd)?;
        let mut command = Command::new(&self.program);
        command.args(&argv);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(&key);
            }
        }
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env_remove("SSH_ASKPASS")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let child = command.spawn().ok()?;
        bounded(child, TIMEOUT, LIMIT).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

fn kill(child: &mut Child) {
    let pid = rustix::process::Pid::from_child(child);
    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    let _ = child.kill();
    let _ = child.wait();
}

/// The standard output of `child` when it exits with success within
/// `timeout`, `limit` bytes at most; else it is killed, with its group.
fn bounded(mut child: Child, timeout: Duration, limit: usize) -> Option<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let read = (&mut stdout).take(limit as u64 + 1).read_to_end(&mut out);
        let _ = tx.send(read.ok().map(|_| out));
    });
    let left = deadline.saturating_duration_since(Instant::now());
    let out = match rx.recv_timeout(left) {
        Ok(Some(out)) if out.len() <= limit => out,
        _ => {
            kill(&mut child);
            return None;
        }
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success().then_some(out),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
            _ => {
                kill(&mut child);
                return None;
            }
        }
    }
}
