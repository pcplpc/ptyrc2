// SPDX-License-Identifier: GPL-3.0-or-later
//! Test helpers: temporary directories under the target directory, and
//! bare repositories built object by object.

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

pub const GPL: &[u8] = b"                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";

pub const MANIFEST: &str = r#"
[app]
id = "sand-garden"
version = "0.3.0"
summary = "Sand that piles on your text."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"
assets = "assets/"
surface = "widget"

[permissions]
shape = "the grains rest on the lines"
"#;

/// A fresh empty directory, unique to the call.
pub fn tmp(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("ptyrc-store")
        .join(format!("{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `git` with no user or system configuration.
pub fn git(dir: &Path, args: &[&str], input: Option<&[u8]>) -> String {
    let mut c = Command::new("git");
    c.arg(format!("--git-dir={}", dir.display()))
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "agent")
        .env("GIT_AUTHOR_EMAIL", "agent@example.org")
        .env("GIT_COMMITTER_NAME", "agent")
        .env("GIT_COMMITTER_EMAIL", "agent@example.org")
        .env("GIT_AUTHOR_DATE", "2026-11-19T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-11-19T00:00:00Z")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.unwrap_or(b""))
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// One entry of a tree to build.
pub enum Item<'a> {
    File(&'a str, &'a [u8]),
    Exec(&'a str, &'a [u8]),
    Symlink(&'a str, &'a str),
    /// A gitlink to some commit id.
    Submodule(&'a str),
}

impl Item<'_> {
    fn path(&self) -> &str {
        match self {
            Item::File(p, _) | Item::Exec(p, _) | Item::Symlink(p, _) | Item::Submodule(p) => p,
        }
    }
}

fn hex_to_bytes(h: &str) -> Vec<u8> {
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}

/// Writes a tree object by hand (`hash-object --literally`), so that
/// names git would refuse (`..`) can be made.
fn write_tree(repo: &Path, items: &[(String, &Item)]) -> String {
    use std::collections::BTreeMap;
    let mut here: Vec<(String, &str, String)> = Vec::new();
    let mut sub: BTreeMap<String, Vec<(String, &Item)>> = BTreeMap::new();
    for (path, item) in items {
        match path.split_once('/') {
            Some((d, rest)) => sub
                .entry(d.to_string())
                .or_default()
                .push((rest.to_string(), item)),
            None => {
                let (mode, oid) = match item {
                    Item::File(_, b) => ("100644", blob(repo, b)),
                    Item::Exec(_, b) => ("100755", blob(repo, b)),
                    Item::Symlink(_, t) => ("120000", blob(repo, t.as_bytes())),
                    Item::Submodule(_) => ("160000", "1".repeat(40)),
                };
                here.push((path.clone(), mode, oid));
            }
        }
    }
    for (d, list) in sub {
        let oid = write_tree(repo, &list);
        here.push((d, "40000", oid));
    }
    // git sorts tree entries by name, a directory as if followed by `/`.
    here.sort_by_key(|(n, m, _)| {
        if *m == "40000" {
            format!("{n}/")
        } else {
            n.clone()
        }
    });
    let mut raw = Vec::new();
    for (name, mode, oid) in here {
        raw.extend_from_slice(format!("{mode} {name}\0").as_bytes());
        raw.extend_from_slice(&hex_to_bytes(&oid));
    }
    git(
        repo,
        &["hash-object", "-w", "--literally", "-t", "tree", "--stdin"],
        Some(&raw),
    )
}

fn blob(repo: &Path, bytes: &[u8]) -> String {
    git(repo, &["hash-object", "-w", "--stdin"], Some(bytes))
}

/// A bare repository at `dir`.
pub fn bare(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(
        dir,
        &["init", "--bare", "--quiet", "--initial-branch=main"],
        None,
    );
}

/// Commits a tree on `main` (parent: the current `main`, if any).
pub fn commit(repo: &Path, items: &[Item], msg: &str) -> String {
    let list: Vec<(String, &Item)> = items.iter().map(|i| (i.path().to_string(), i)).collect();
    let tree = write_tree(repo, &list);
    let parent = Command::new("git")
        .arg(format!("--git-dir={}", repo.display()))
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/main"])
        .output()
        .unwrap();
    let parent = String::from_utf8(parent.stdout).unwrap().trim().to_string();
    let mut args = vec!["commit-tree", &tree, "-m", msg];
    if !parent.is_empty() {
        args.push("-p");
        args.push(&parent);
    }
    let c = git(repo, &args, None);
    git(repo, &["update-ref", "refs/heads/main", &c], None);
    c
}

/// The files of a valid script app, beside `extra`.
pub fn app_items<'a>(extra: &[Item<'a>]) -> Vec<Item<'a>> {
    let mut v = vec![
        Item::File("ptyrc-app.toml", MANIFEST.as_bytes()),
        Item::File("LICENSE", GPL),
        Item::File("app.js", b"export function init(args) {}\n"),
        Item::File("assets/grain.txt", b"."),
    ];
    v.retain(|d| !extra.iter().any(|e| e.path() == d.path()));
    for e in extra {
        v.push(match e {
            Item::File(p, b) => Item::File(p, b),
            Item::Exec(p, b) => Item::Exec(p, b),
            Item::Symlink(p, t) => Item::Symlink(p, t),
            Item::Submodule(p) => Item::Submodule(p),
        });
    }
    v
}
