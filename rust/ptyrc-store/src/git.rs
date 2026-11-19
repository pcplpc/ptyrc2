// SPDX-License-Identifier: GPL-3.0-or-later
//! Fetch by spawning `git`: one commit, depth 1, into a bare repository
//! of ours; files read with `git cat-file`, nothing checked out.
//!
//! Against a hostile remote or spec: the environment is cleared (no user
//! or system configuration, no credential, no `.netrc`: `HOME` is the
//! bare repository); one protocol is allowed (`https`, or `file` for a
//! local origin) by `protocol.allow` and `GIT_ALLOW_PROTOCOL`; no
//! prompt, hook, submodule, lazy fetch, replace object; objects are
//! checked (`fsckObjects`); every argument from outside is validated and
//! placed after `--` or behind a fixed prefix. A fetch is bounded: the
//! packs it writes grow past [`MAX_FETCH`] (or [`Repo::max_fetch`]), it
//! is killed and they are removed.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::error::{Error, ErrorKind, Result, check_rel_path};
use crate::origin::{Reference, Remote};
use crate::tree::{Kind, Node, Tree};

/// Bound on what one fetch writes, bytes of packs. The tree is 8 MiB at
/// most; a fetch of depth 1 carries the whole commit, other directories
/// of the repository included.
pub const MAX_FETCH: u64 = 64 << 20;

/// The ref our fetch writes.
const WANT: &str = "refs/ptyrc/want";
/// Variables passed through to `git`; every other one is cleared.
const KEEP_ENV: &[&str] = &[
    "PATH",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

fn fetch_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Fetch, msg)
}

/// A bare repository of ours, holding fetched commits.
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
    max_fetch: u64,
}

impl Repo {
    /// Opens `dir`, creating it bare (no template, no hook) when absent.
    pub fn open(dir: &Path) -> Result<Repo> {
        let dir = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| Error::io("current directory", e))?
                .join(dir)
        };
        let repo = Repo {
            dir,
            max_fetch: MAX_FETCH,
        };
        if !repo.dir.join("HEAD").is_file() {
            crate::install::mkdir(&repo.dir)?;
            let empty = repo.dir.join("template");
            crate::install::mkdir(&empty)?;
            repo.run(
                repo.git(None)
                    .args(["-c", "init.defaultBranch=main", "init", "--bare", "--quiet"])
                    .arg(format!("--template={}", empty.display())),
                None,
            )?;
            let _ = std::fs::remove_dir(&empty);
        }
        Ok(repo)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The bound of a fetch, bytes; [`MAX_FETCH`] by default.
    pub fn max_fetch(mut self, bytes: u64) -> Repo {
        self.max_fetch = bytes;
        self
    }

    /// A `git` command: cleared environment, fixed configuration. `proto`
    /// is the one protocol allowed, for a fetch.
    fn git(&self, proto: Option<&str>) -> Command {
        let mut c = Command::new("git");
        c.env_clear();
        for k in KEEP_ENV {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        c.env("HOME", &self.dir)
            .env("XDG_CONFIG_HOME", &self.dir)
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_ALLOW_PROTOCOL", proto.unwrap_or("none"));
        let mut cfg = vec![
            "protocol.allow=never".to_string(),
            "core.hooksPath=/dev/null".into(),
            "core.fsmonitor=false".into(),
            "core.askPass=".into(),
            "credential.helper=".into(),
            "fetch.fsckObjects=true".into(),
            "transfer.fsckObjects=true".into(),
            "fetch.recurseSubmodules=false".into(),
            "submodule.recurse=false".into(),
            "gc.auto=0".into(),
            "maintenance.auto=false".into(),
            "http.followRedirects=initial".into(),
            "http.lowSpeedLimit=1000".into(),
            "http.lowSpeedTime=30".into(),
            "protocol.version=2".into(),
            // A pack is kept whole, so the bound sees one growing file.
            "fetch.unpackLimit=1".into(),
            "transfer.unpackLimit=1".into(),
        ];
        if let Some(p) = proto {
            cfg.push(format!("protocol.{p}.allow=always"));
        }
        for kv in cfg {
            c.arg("-c").arg(kv);
        }
        c.arg(format!("--git-dir={}", self.dir.display()));
        c
    }

    /// Runs, returns stdout; stderr, cleaned, in the error.
    fn run(&self, cmd: &mut Command, what: Option<&str>) -> Result<Vec<u8>> {
        let out = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                fetch_err(format!("git: {e}"))
                    .with_hint("install git; it is run as `git` from PATH")
            })?;
        if out.status.success() {
            return Ok(out.stdout);
        }
        let msg = clean(&out.stderr);
        Err(fetch_err(format!("git {}: {msg}", what.unwrap_or("init"))))
    }

    /// Fetches one commit, depth 1, and returns its id.
    pub fn fetch(&self, remote: &Remote, want: &Reference) -> Result<String> {
        let (proto, url) = match remote {
            Remote::Https(u) => {
                // Built by `Origin`; checked again, as it reaches a command line.
                if !u.starts_with("https://")
                    || u.contains("::")
                    || u.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    return Err(fetch_err(format!("{u:?}: not an https URL")));
                }
                ("https", u.clone())
            }
            Remote::Local(p) => {
                let s = p.to_str().unwrap_or("");
                if !p.is_absolute() || s.contains(':') || s.chars().any(char::is_control) {
                    return Err(fetch_err(format!(
                        "{}: not an absolute local path",
                        p.display()
                    )));
                }
                ("file", format!("file://{s}"))
            }
        };
        let src = match want {
            Reference::Head => "HEAD".to_string(),
            Reference::Name(n) => {
                Reference::parse(n)?;
                n.clone()
            }
            Reference::Commit(c) => {
                if !is_oid(c) {
                    return Err(fetch_err(format!("{c:?}: not a commit id")));
                }
                c.clone()
            }
        };
        let _ = self.run(
            self.git(None).args(["update-ref", "-d", WANT]),
            Some("update-ref"),
        );
        self.run_fetch(
            self.git(Some(proto))
                .args([
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--no-write-fetch-head",
                    "--no-auto-gc",
                    "--no-auto-maintenance",
                    "--depth=1",
                    "--",
                ])
                .arg(&url)
                .arg(format!("+{src}:{WANT}")),
        )
        .map_err(|e| {
            if e.hint.is_some() {
                return e;
            }
            let hint = match want {
                Reference::Commit(_) => {
                    "is the commit there, and is it reachable from a branch or a tag?"
                }
                Reference::Name(_) => "is the branch or tag there?",
                Reference::Head => "is the URL that of a git repository?",
            };
            if e.message.contains("fsck") {
                e.with_hint("the repository holds objects git refuses: a path with `..` or `.git`, a bad mode")
            } else {
                e.with_hint(hint)
            }
        })?;
        let commit = self.resolve(&format!("{WANT}^{{commit}}"))?;
        if let Reference::Commit(c) = want
            && &commit != c
        {
            return Err(fetch_err(format!("asked {c}, got {commit}")));
        }
        Ok(commit)
    }

    /// The files of `objects/pack/` and their sizes.
    fn packs(&self) -> Vec<(OsString, u64)> {
        let Ok(rd) = std::fs::read_dir(self.dir.join("objects/pack")) else {
            return Vec::new();
        };
        rd.flatten()
            .map(|e| (e.file_name(), e.metadata().map_or(0, |m| m.len())))
            .collect()
    }

    /// Bytes of the pack files not in `before`.
    fn grown(&self, before: &HashSet<OsString>) -> u64 {
        let packs = self.packs().into_iter();
        packs
            .filter(|(n, _)| !before.contains(n))
            .map(|(_, s)| s)
            .sum()
    }

    /// Runs a fetch; kills it, and removes what it wrote, past
    /// `max_fetch`.
    fn run_fetch(&self, cmd: &mut Command) -> Result<()> {
        let before: HashSet<OsString> = self.packs().into_iter().map(|(n, _)| n).collect();
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                fetch_err(format!("git: {e}"))
                    .with_hint("install git; it is run as `git` from PATH")
            })?;
        let mut stderr = child.stderr.take().expect("piped");
        let reader = std::thread::spawn(move || {
            // The tail only: a remote may say much.
            let mut tail = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = stderr.read(&mut buf) {
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > 1 << 16 {
                    tail.drain(..tail.len() - (1 << 16));
                }
            }
            tail
        });
        let mut over = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(_) => break None,
            }
            if self.grown(&before) > self.max_fetch {
                over = true;
                kill_tree(&mut child);
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let tail = reader.join().unwrap_or_default();
        if over || self.grown(&before) > self.max_fetch {
            // The locks of the fetch killed.
            for l in ["shallow.lock", "packed-refs.lock", "refs/ptyrc/want.lock"] {
                let _ = std::fs::remove_file(self.dir.join(l));
            }
            for (n, _) in self.packs() {
                if !before.contains(&n) {
                    let _ = std::fs::remove_file(self.dir.join("objects/pack").join(n));
                }
            }
            let _ = self.run(
                self.git(None).args(["update-ref", "-d", WANT]),
                Some("update-ref"),
            );
            return Err(fetch_err(format!(
                "git fetch: more than {} bytes",
                self.max_fetch
            ))
            .with_hint("the commit is too large: a repository of the app alone, or a #path in a smaller one"));
        }
        match status {
            Some(s) if s.success() => Ok(()),
            _ => Err(fetch_err(format!("git fetch: {}", clean(&tail)))),
        }
    }

    fn resolve(&self, rev: &str) -> Result<String> {
        let out = self.run(
            self.git(None)
                .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
                .arg(rev),
            Some("rev-parse"),
        )?;
        let s = String::from_utf8_lossy(&out).trim().to_string();
        if !is_oid(&s) {
            return Err(fetch_err(format!("{rev}: no object id")));
        }
        Ok(s)
    }

    /// The tree of `commit`, or of its directory `sub`.
    pub fn tree(&self, commit: &str, sub: Option<&str>) -> Result<GitTree> {
        if !is_oid(commit) {
            return Err(fetch_err(format!("{commit:?}: not a commit id")));
        }
        let rev = match sub {
            None => format!("{commit}^{{tree}}"),
            Some(s) => {
                check_rel_path(s).map_err(|w| fetch_err(format!("#path {s:?}: {w}")))?;
                format!("{commit}:{s}")
            }
        };
        let no_dir = |e: Error| match sub {
            Some(s) => e.with_hint(format!("no directory {s} at that commit")),
            None => e,
        };
        let tree = self.resolve(&rev).map_err(no_dir)?;
        let ty = self.run(
            self.git(None).args(["cat-file", "-t"]).arg(&tree),
            Some("cat-file"),
        )?;
        if ty.trim_ascii() != b"tree" {
            return Err(no_dir(fetch_err(format!("{rev}: not a directory"))));
        }
        Ok(GitTree {
            repo: self.clone(),
            tree,
        })
    }
}

/// A git tree object, read without checkout.
#[derive(Debug, Clone)]
pub struct GitTree {
    repo: Repo,
    tree: String,
}

impl GitTree {
    pub fn id(&self) -> &str {
        &self.tree
    }
}

impl Tree for GitTree {
    fn nodes(&self) -> Result<Vec<Node>> {
        let out = self.repo.run(
            self.repo
                .git(None)
                .args([
                    "ls-tree",
                    "-r",
                    "-t",
                    "-l",
                    "-z",
                    "--full-tree",
                    "--end-of-options",
                ])
                .arg(&self.tree),
            Some("ls-tree"),
        )?;
        let mut nodes = Vec::new();
        for rec in out.split(|&b| b == 0).filter(|r| !r.is_empty()) {
            let tab = rec
                .iter()
                .position(|&b| b == b'\t')
                .ok_or_else(|| fetch_err("git ls-tree: a record without a tab"))?;
            let head = std::str::from_utf8(&rec[..tab])
                .map_err(|_| fetch_err("git ls-tree: not UTF-8"))?;
            let path = std::str::from_utf8(&rec[tab + 1..]).map_err(|_| {
                Error::new(
                    ErrorKind::Tree,
                    format!("{:?}: not UTF-8", String::from_utf8_lossy(&rec[tab + 1..])),
                )
                .with_hint("file names in UTF-8")
            })?;
            let f: Vec<&str> = head.split_whitespace().collect();
            let [mode, _ty, id, size] = f[..] else {
                return Err(fetch_err(format!("git ls-tree: {head:?}")));
            };
            let kind = match mode {
                "100644" | "100755" => Kind::File,
                "040000" => Kind::Dir,
                "120000" => Kind::Symlink,
                "160000" => Kind::Submodule,
                _ => Kind::Other,
            };
            let size = match kind {
                Kind::File => size
                    .parse()
                    .map_err(|_| fetch_err(format!("git ls-tree: size {size:?}")))?,
                _ => 0,
            };
            nodes.push(Node {
                path: path.to_string(),
                kind,
                size,
                id: id.to_string(),
            });
        }
        Ok(nodes)
    }

    fn read(&self, node: &Node) -> Result<Vec<u8>> {
        if node.kind != Kind::File || !is_oid(&node.id) {
            return Err(fetch_err(format!("{}: not a blob", node.path)));
        }
        let out = self.repo.run(
            self.repo.git(None).args(["cat-file", "blob"]).arg(&node.id),
            Some("cat-file"),
        )?;
        if out.len() as u64 != node.size {
            return Err(fetch_err(format!(
                "{}: size differs from the tree",
                node.path
            )));
        }
        Ok(out)
    }
}

/// Kills `child` and, where `/proc` tells, its descendants: `git fetch`
/// runs the transport and `index-pack` as its children.
fn kill_tree(child: &mut Child) {
    let mut pids = vec![child.id()];
    if let Ok(rd) = std::fs::read_dir("/proc") {
        let parents: Vec<(u32, u32)> = rd
            .flatten()
            .filter_map(|e| {
                let pid: u32 = e.file_name().to_str()?.parse().ok()?;
                let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
                // `pid (comm) state ppid ...`; comm may hold anything.
                let ppid = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?;
                Some((pid, ppid.parse().ok()?))
            })
            .collect();
        let mut i = 0;
        while i < pids.len() {
            let p = pids[i];
            pids.extend(parents.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
            i += 1;
        }
    }
    for pid in pids.iter().skip(1).rev() {
        if let Some(pid) = rustix::process::Pid::from_raw(*pid as i32) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Full lower-case hex object id: SHA-1 or SHA-256.
pub fn is_oid(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64)
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The tail of `git`'s stderr, printable characters only: a remote may
/// send escapes.
fn clean(stderr: &[u8]) -> String {
    let s = String::from_utf8_lossy(stderr);
    let s: String = s
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let s = s.trim();
    match s.char_indices().rev().nth(499) {
        Some((i, _)) => format!("...{}", &s[i..]),
        None => s.to_string(),
    }
}
