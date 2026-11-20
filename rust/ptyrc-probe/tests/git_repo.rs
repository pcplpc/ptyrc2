// SPDX-License-Identifier: GPL-3.0-or-later
//! `git::probe` on real repositories: the answer, the stamp, nothing
//! written, nothing of the repository run, a `git` that hangs cut.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ptyrc_probe::git::run::{Cmd, Git};
use ptyrc_probe::git::{self, Ask, Now, Part};
use serde_json::Value;

/// A fresh directory of its own for test `name`, and a `git` that
/// reads no global config and stops at it.
struct Base {
    dir: PathBuf,
    git: PathBuf,
}

fn base(name: &str) -> Base {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("probe-git")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let git = dir.join("git-wrap");
    script(
        &git,
        &format!(
            "export GIT_CONFIG_GLOBAL=/dev/null GIT_CEILING_DIRECTORIES='{}'\nexec git \"$@\"\n",
            dir.display()
        ),
    );
    Base { dir, git }
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn sh(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Git as a user runs it, its success not asked.
fn plain(cwd: &Path, args: &[&str]) {
    let _ = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(std::process::Stdio::null())
        .output();
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

/// Two commits on `main`, a stash, a branch, a file staged, one changed,
/// one untracked.
fn work(b: &Base) -> PathBuf {
    let root = b.dir.join("work");
    std::fs::create_dir(&root).unwrap();
    sh(&root, &["init", "-q", "-b", "main"]);
    write(&root.join("a.txt"), "one\n");
    sh(&root, &["add", "a.txt"]);
    sh(&root, &["commit", "-qm", "first"]);
    write(&root.join("a.txt"), "one\ntwo\n");
    sh(&root, &["commit", "-qam", "second"]);
    sh(&root, &["branch", "side"]);
    write(&root.join("a.txt"), "one\ntwo\nstashed\n");
    sh(&root, &["stash", "-q"]);
    write(&root.join("a.txt"), "one\nthree\nfour\n");
    write(&root.join("b.txt"), "b\n");
    sh(&root, &["add", "b.txt"]);
    write(&root.join("c.txt"), "c\n");
    root
}

fn now() -> Now {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    Now { at, offset: 0 }
}

fn probe(b: &Base, dir: &Path, ask: &Ask) -> Value {
    git::probe_with(&b.git, dir, ask, now())
}

/// Every string of `v`.
fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|v| strings(v, out)),
        Value::Object(o) => o.iter().for_each(|(k, v)| {
            out.push(k);
            strings(v, out)
        }),
        _ => {}
    }
}

fn no_path(v: &Value, base: &Path) {
    let mut all = Vec::new();
    strings(v, &mut all);
    let base = base.to_string_lossy();
    for s in all {
        assert!(
            !s.contains(&*base) && !s.starts_with('/'),
            "a path in the answer: {s}"
        );
    }
}

#[test]
fn read() {
    let b = base("read");
    let root = work(&b);
    let got = probe(&b, &root, &Ask::default());
    let r = &got["repo"];
    assert_eq!(r["name"], "work");
    let head = &r["head"];
    assert_eq!(
        (head["branch"].as_str(), head["upstream"].is_null()),
        (Some("main"), true)
    );
    assert_eq!(head["initial"], false);
    let mut changes: Vec<String> = r["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.to_string())
        .collect();
    changes.sort();
    assert_eq!(
        changes,
        [
            r#"{"added":1,"code":"A","deleted":0,"kind":"staged","path":"b.txt"}"#,
            r#"{"added":2,"code":"M","deleted":1,"kind":"unstaged","path":"a.txt"}"#,
            r#"{"added":null,"code":"?","deleted":null,"kind":"untracked","path":"c.txt"}"#,
        ]
    );
    assert_eq!(
        r["counts"],
        serde_json::json!({"staged": 1, "unstaged": 1, "untracked": 1, "conflict": 0})
    );
    let subjects: Vec<_> = r["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["subject"].as_str().unwrap())
        .collect();
    assert_eq!(subjects, ["second", "first"]);
    assert_eq!(
        r["log"][0]["refs"],
        serde_json::json!(["HEAD -> main", "side"])
    );
    let oid = head["oid"].as_str().unwrap();
    assert_eq!(
        r["stashes"][0]["subject"],
        format!("WIP on main: {oid} second")
    );
    let branches: Vec<_> = r["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["name"].as_str().unwrap(), b["current"].as_bool().unwrap()))
        .collect();
    assert_eq!(branches, [("main", true), ("side", false)]);
    assert_eq!(
        r["worktrees"],
        serde_json::json!([{"name": "work", "oid": oid, "branch": "main", "flags": [], "current": true}])
    );
    let days: Vec<u64> = serde_json::from_value(r["days"].clone()).unwrap();
    assert_eq!(
        (days.len(), days.iter().sum::<u64>(), days[111]),
        (112, 2, 2)
    );
    assert_eq!(r["state"], "");
    assert!(r["state_since"].is_null() && r["dirty_since"].is_f64());
    assert!(got["stamp"].as_str().unwrap().len() == 16);
    no_path(&got, &b.dir);
    // Parts: only those asked.
    let ask = Ask {
        parts: vec![Part::Head],
        ..Ask::default()
    };
    let got = probe(&b, &root, &ask);
    let keys: Vec<_> = got["repo"].as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["head", "name", "now"]);
    let ask = Ask {
        parts: vec![Part::Log],
        log: 1,
        ..Ask::default()
    };
    assert_eq!(
        probe(&b, &root, &ask)["repo"]["log"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn awkward() {
    let b = base("awkward");
    let root = work(&b);
    let null = serde_json::json!({"repo": null, "stamp": ""});
    let plain = b.dir.join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(probe(&b, &plain, &Ask::default()), null);
    assert_eq!(probe(&b, &b.dir.join("missing"), &Ask::default()), null);
    assert_eq!(
        probe(&b, &root.join("a.txt"), &Ask::default()),
        null,
        "a file"
    );
    let no_git = git::probe_with(Path::new("/nonexistent/git"), &root, &Ask::default(), now());
    assert_eq!(no_git, null);
    let empty = b.dir.join("empty");
    std::fs::create_dir(&empty).unwrap();
    sh(&empty, &["init", "-q", "-b", "trunk"]);
    let r = &probe(&b, &empty, &Ask::default())["repo"];
    assert_eq!(
        (r["head"]["initial"].as_bool(), r["head"]["branch"].as_str()),
        (Some(true), Some("trunk"))
    );
    assert_eq!(r["log"], serde_json::json!([]));
    assert_eq!(
        r["days"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_u64)
            .sum::<u64>(),
        0
    );
    sh(&root, &["stash", "-q"]);
    sh(&root, &["checkout", "-q", "--detach", "HEAD~1"]);
    let r = &probe(&b, &root, &Ask::default())["repo"];
    assert!(r["head"]["branch"].is_null() && !r["head"]["oid"].as_str().unwrap().is_empty());
    // A linked work tree: its own git directory, the shared one found.
    let linked = b.dir.join("linked");
    sh(
        &root,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "side"],
    );
    let got = probe(&b, &linked, &Ask::default());
    let r = &got["repo"];
    assert_eq!(
        (r["name"].as_str(), r["head"]["branch"].as_str()),
        (Some("linked"), Some("side"))
    );
    let trees: Vec<_> = r["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| (w["name"].as_str().unwrap(), w["current"].as_bool().unwrap()))
        .collect();
    assert_eq!(trees, [("work", false), ("linked", true)]);
    no_path(&got, &b.dir);
    let (gitdir, common) = git::discover(&linked).unwrap();
    assert!(gitdir != common && common == root.join(".git") && gitdir.join("HEAD").is_file());
    // Unchanged through the linked tree too.
    let stamp = got["stamp"].as_str().unwrap().to_string();
    let again = Ask {
        since: Some(stamp.clone()),
        ..Ask::default()
    };
    assert_eq!(
        probe(&b, &linked, &again),
        serde_json::json!({"unchanged": true, "stamp": stamp})
    );
}

#[test]
fn sha256() {
    let b = base("sha256");
    let root = b.dir.join("r");
    std::fs::create_dir(&root).unwrap();
    sh(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    write(&root.join("a"), "a\n");
    sh(&root, &["add", "a"]);
    sh(&root, &["commit", "-qm", "one"]);
    write(&root.join("a"), "b\n");
    let r = &probe(&b, &root, &Ask::default())["repo"];
    assert_eq!(r["log"][0]["subject"], "one");
    assert_eq!(r["changes"][0]["added"], 1);
}

/// Every file of the repository: its mtime and its content.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (SystemTime, Vec<u8>)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let m = e.metadata().unwrap();
            if m.is_dir() {
                todo.push(e.path());
            } else {
                out.insert(
                    e.path(),
                    (m.modified().unwrap(), std::fs::read(e.path()).unwrap()),
                );
            }
        }
    }
    out
}

#[test]
fn read_only() {
    let b = base("read_only");
    let root = work(&b);
    // The index is stale now: `git status` would refresh it.
    let f = std::fs::File::options()
        .write(true)
        .open(root.join("b.txt"))
        .unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(1)).unwrap();
    let before = snapshot(&root);
    for _ in 0..3 {
        assert!(probe(&b, &root, &Ask::default())["repo"].is_object());
    }
    assert_eq!(snapshot(&root), before);
    assert!(
        !before
            .keys()
            .any(|p| p.extension().is_some_and(|e| e == "lock"))
    );
}

#[test]
fn stamp() {
    let b = base("stamp");
    let root = work(&b);
    let first = probe(&b, &root, &Ask::default());
    let stamp = |v: &Value| v["stamp"].as_str().unwrap().to_string();
    let since = |s: String| Ask {
        since: Some(s),
        ..Ask::default()
    };
    let s = stamp(&first);
    assert_eq!(probe(&b, &root, &since(s.clone()))["unchanged"], true);
    write(&root.join("c.txt"), "the work tree is not watched\n");
    assert_eq!(probe(&b, &root, &since(s.clone()))["unchanged"], true);
    let other = Ask {
        parts: vec![Part::Head],
        since: Some(s.clone()),
        ..Ask::default()
    };
    assert!(
        probe(&b, &root, &other)["repo"].is_object(),
        "other parts, other stamp"
    );
    sh(&root, &["add", "c.txt"]);
    let second = probe(&b, &root, &since(s.clone()));
    assert!(second["repo"].is_object() && stamp(&second) != s);
    let s = stamp(&second);
    assert_eq!(probe(&b, &root, &since(s.clone()))["unchanged"], true);
    sh(&root, &["commit", "-qm", "third"]);
    let third = probe(&b, &root, &since(s.clone()));
    assert!(third["repo"].is_object());
    let s = stamp(&third);
    sh(&root, &["stash", "drop", "-q"]);
    let fourth = probe(&b, &root, &since(s));
    assert!(fourth["repo"].is_object(), "a stash dropped");
    let s = stamp(&fourth);
    sh(&root, &["branch", "fresh"]);
    assert!(
        probe(&b, &root, &since(s))["repo"].is_object(),
        "a branch made"
    );
}

/// A command that touches `marks/NAME`.
fn marker(b: &Base, name: &str) -> String {
    let path = b.dir.join(format!("mark-{name}"));
    script(
        &path,
        &format!("touch '{}/marks/{name}'\nexit 0\n", b.dir.display()),
    );
    path.display().to_string()
}

fn marks(b: &Base) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(b.dir.join("marks"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// A repository whose config and attributes name a command at every
/// place we know of: none runs.
#[test]
fn hostile() {
    let b = base("hostile");
    std::fs::create_dir(b.dir.join("marks")).unwrap();
    let root = b.dir.join("evil");
    std::fs::create_dir(&root).unwrap();
    sh(&root, &["init", "-q", "-b", "main"]);
    write(&root.join(".gitattributes"), "* filter=tree diff=tree\n");
    write(&root.join("a"), "a\n");
    sh(&root, &["add", "."]);
    sh(&root, &["commit", "-qm", "one"]);
    // A commit with a signature: `log.showSignature` would run gpg.
    let tree = sh(&root, &["rev-parse", "HEAD^{tree}"]);
    let parent = sh(&root, &["rev-parse", "HEAD"]);
    let body = format!(
        "tree {}\nparent {}\nauthor t <t@t> 1 +0000\ncommitter t <t@t> 1 +0000\n\
         gpgsig -----BEGIN PGP SIGNATURE-----\n x\n -----END PGP SIGNATURE-----\n\nsigned\n",
        tree.trim(),
        parent.trim()
    );
    write(&b.dir.join("commit"), &body);
    let oid = sh(
        &root,
        &[
            "hash-object",
            "-t",
            "commit",
            "-w",
            b.dir.join("commit").to_str().unwrap(),
        ],
    );
    sh(&root, &["update-ref", "refs/heads/main", oid.trim()]);
    let info = root.join(".git/info");
    std::fs::create_dir_all(&info).unwrap();
    write(&info.join("attributes"), "[attr]m filter=info\n* m\n");
    write(&b.dir.join("attrs"), "* filter=file\n");
    let hooks = b.dir.join("hooks");
    std::fs::create_dir(&hooks).unwrap();
    for hook in [
        "post-index-change",
        "reference-transaction",
        "post-checkout",
        "pre-auto-gc",
    ] {
        std::fs::copy(marker(&b, &format!("hook-{hook}")), hooks.join(hook)).unwrap();
    }
    write(
        &b.dir.join("included"),
        &format!("[filter \"tree\"]\n\tsmudge = {}\n", marker(&b, "include")),
    );
    let config = [
        ("core.fsmonitor", marker(&b, "fsmonitor")),
        ("core.pager", marker(&b, "pager")),
        ("pager.status", marker(&b, "pager-status")),
        ("pager.log", marker(&b, "pager-log")),
        ("core.hooksPath", hooks.display().to_string()),
        (
            "core.attributesFile",
            b.dir.join("attrs").display().to_string(),
        ),
        ("core.sshCommand", marker(&b, "ssh")),
        ("diff.external", marker(&b, "external")),
        ("diff.tree.textconv", marker(&b, "textconv")),
        ("diff.tree.command", marker(&b, "diff-command")),
        ("filter.tree.clean", marker(&b, "filter-clean")),
        ("filter.tree.process", marker(&b, "filter-process")),
        ("filter.info.clean", marker(&b, "info-clean")),
        ("filter.file.clean", marker(&b, "file-clean")),
        ("log.showSignature", "true".into()),
        ("gpg.program", marker(&b, "gpg")),
        ("alias.status", marker(&b, "alias")),
        ("color.ui", "always".into()),
        ("include.path", b.dir.join("included").display().to_string()),
    ];
    for (key, value) in &config {
        sh(&root, &["config", key, value]);
    }
    // Stat-dirty, the same content: status must read the file.
    let f = std::fs::File::options()
        .write(true)
        .open(root.join("a"))
        .unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(1000))
        .unwrap();
    let got = probe(&b, &root, &Ask::default());
    assert_eq!(marks(&b), Vec::<String>::new());
    let r = &got["repo"];
    assert_eq!(r["log"][0]["subject"], "signed");
    assert_eq!(r["log"][0]["graph"][0], "*", "no colour");
    assert!(r["head"]["oid"].as_str().is_some_and(|o| o.len() == 7));
    // The control: plain git runs them.
    plain(&root, &["status", "--porcelain"]);
    plain(&root, &["log", "-1", "--format=%h"]);
    let ran = marks(&b);
    for want in ["fsmonitor", "gpg", "hook-post-index-change", "info-clean"] {
        assert!(ran.iter().any(|m| m == want), "{want} not live: {ran:?}");
    }
    // `.gitattributes` alone: its filter is live too.
    std::fs::remove_file(info.join("attributes")).unwrap();
    let f = std::fs::File::options()
        .write(true)
        .open(root.join("a"))
        .unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(2000))
        .unwrap();
    assert_eq!(
        probe(&b, &root, &Ask::default())["repo"]["head"]["branch"],
        "main"
    );
    assert_eq!(marks(&b), ran);
    plain(&root, &["status", "--porcelain"]);
    assert!(
        marks(&b).iter().any(|m| m == "filter-process"),
        "{:?}",
        marks(&b)
    );
}

/// `info/attributes` naming a filter: neutralised by name.
#[test]
fn info_attributes() {
    let b = base("info_attributes");
    std::fs::create_dir(b.dir.join("marks")).unwrap();
    let root = b.dir.join("r");
    std::fs::create_dir(&root).unwrap();
    sh(&root, &["init", "-q", "-b", "main"]);
    write(&root.join("a"), "a\n");
    sh(&root, &["add", "a"]);
    sh(&root, &["commit", "-qm", "one"]);
    write(&root.join(".git/info/attributes"), "* filter=x\n");
    sh(&root, &["config", "filter.x.clean", &marker(&b, "clean")]);
    sh(&root, &["config", "filter.x.required", "true"]);
    let f = std::fs::File::options()
        .write(true)
        .open(root.join("a"))
        .unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(1000))
        .unwrap();
    let r = &probe(&b, &root, &Ask::default())["repo"];
    assert_eq!(marks(&b), Vec::<String>::new());
    assert_eq!(r["head"]["branch"], "main");
    plain(&root, &["status", "--porcelain"]);
    assert_eq!(marks(&b), ["clean"], "the control");
}

/// A partial clone missing an object: the lazy fetch would run the
/// remote's `uploadpack`.
#[test]
fn no_lazy_fetch() {
    let b = base("no_lazy_fetch");
    std::fs::create_dir(b.dir.join("marks")).unwrap();
    let root = b.dir.join("r");
    std::fs::create_dir(&root).unwrap();
    sh(&root, &["init", "-q", "-b", "main"]);
    write(&root.join("a"), "a\n");
    sh(&root, &["add", "a"]);
    sh(&root, &["commit", "-qm", "one"]);
    let tree = sh(&root, &["rev-parse", "HEAD^{tree}"]);
    let tree = tree.trim();
    std::fs::remove_file(root.join(".git/objects").join(&tree[..2]).join(&tree[2..])).unwrap();
    for (key, value) in [
        ("core.repositoryformatversion", "1".to_string()),
        ("extensions.partialClone", "origin".into()),
        ("remote.origin.promisor", "true".into()),
        ("remote.origin.url", b.dir.display().to_string()),
        ("remote.origin.uploadpack", marker(&b, "uploadpack")),
    ] {
        sh(&root, &["config", key, &value]);
    }
    let _ = probe(&b, &root, &Ask::default());
    assert_eq!(marks(&b), Vec::<String>::new());
    plain(&root, &["status"]);
    assert_eq!(marks(&b), ["uploadpack"], "the control");
}

#[test]
fn hang_cut() {
    let b = base("hang");
    let pid = b.dir.join("pid");
    let hang = b.dir.join("hang");
    script(
        &hang,
        &format!("sleep 30 &\necho $! > '{}'\nwait\n", pid.display()),
    );
    let t = Instant::now();
    let got = git::probe_with(&hang, &b.dir, &Ask::default(), now());
    let took = t.elapsed();
    assert_eq!(got["repo"], Value::Null);
    assert!(
        took >= Duration::from_millis(1900) && took < Duration::from_secs(3),
        "{took:?}"
    );
    // The group is killed: the child of the child too.
    let pid = std::fs::read_to_string(&pid).unwrap();
    let proc = PathBuf::from(format!("/proc/{}", pid.trim()));
    let t = Instant::now();
    while proc.exists() && t.elapsed() < Duration::from_secs(1) {
        let stat = std::fs::read_to_string(proc.join("stat")).unwrap_or_default();
        if stat.contains(") Z ") {
            break; // dead, its parent not yet reaped by init
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let stat = std::fs::read_to_string(proc.join("stat")).unwrap_or_default();
    assert!(
        stat.is_empty() || stat.contains(") Z "),
        "still alive: {stat}"
    );
}

#[test]
fn output_bounded() {
    let b = base("bounded");
    let big = b.dir.join("big");
    script(&big, "head -c 5000000 /dev/zero\n");
    assert_eq!(Git::new(&big, &b.dir).run(Cmd::Locate), None);
    let small = b.dir.join("small");
    script(&small, "printf 'x\\n'\n");
    assert_eq!(
        Git::new(&small, &b.dir).run(Cmd::Locate).as_deref(),
        Some("x\n")
    );
    let fails = b.dir.join("fails");
    script(&fails, "printf 'x\\n'\nexit 1\n");
    assert_eq!(Git::new(&fails, &b.dir).run(Cmd::Locate), None);
}
