// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc store` and `ptyrc dev` against local bare repositories. The
//! four XDG directories and HOME are the test's; https origins are read
//! from local repositories (`PTYRC_TEST_GIT_ROOT`, debug builds), never
//! the network.
#![cfg(debug_assertions)]

mod common;

use ptyrc_store::{Dirs, Origin, Pointer, Tier};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");

const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";

fn manifest(id: &str, version: &str) -> String {
    format!(
        "[app]\nid = \"{id}\"\nversion = \"{version}\"\nsummary = \"Sand that piles on your text.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\n\
         [permissions]\nshape = \"the grains rest on the lines\"\n"
    )
}

/// A test root: the XDG directories, the repositories.
struct Root {
    dir: PathBuf,
}

/// The outcome of a verb.
struct Out {
    status: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    #[track_caller]
    fn ok(&self) -> &Out {
        assert_eq!(self.status, 0, "stderr: {}", self.stderr);
        self
    }

    /// Fails with `status`; stderr holds `message` on a `ptyrc: ` line
    /// and `hint` after it.
    #[track_caller]
    fn fails(&self, status: i32, message: &str, hint: &str) {
        assert_eq!(self.status, status, "stderr: {}", self.stderr);
        let lines: Vec<&str> = self.stderr.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("ptyrc: ") && l.contains(message));
        let at = at.unwrap_or_else(|| panic!("{message:?} not in {:?}", self.stderr));
        assert!(
            lines[at + 1..].iter().any(|l| l.contains(hint)),
            "hint {hint:?} not in {:?}",
            self.stderr
        );
        assert!(!self.stderr.contains("os error"), "{}", self.stderr);
    }
}

impl Root {
    fn new(tag: &str) -> Root {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("store-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Root { dir }
    }

    fn dirs(&self) -> Dirs {
        Dirs::under(&self.dir.join("xdg"))
    }

    /// The environment of `ptyrc`: every directory the test's.
    fn env(&self) -> Vec<(String, String)> {
        let d = self.dirs();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        [
            ("HOME", s(&self.dir.join("home"))),
            ("XDG_DATA_HOME", s(&d.data)),
            ("XDG_STATE_HOME", s(&d.state)),
            ("XDG_CACHE_HOME", s(&d.cache)),
            ("XDG_CONFIG_HOME", s(&d.config)),
            ("XDG_RUNTIME_DIR", s(&self.dir.join("run"))),
            ("PTYRC_TEST_GIT_ROOT", s(&self.dir.join("hosts"))),
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    fn ptyrc(&self, args: &[&str]) -> Out {
        let out = Command::new(BIN)
            .args(args)
            .envs(self.env())
            .env_remove("PTYRC_SESSION")
            .env_remove("PTYRC_SOCKET")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        Out {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }

    /// A work tree at `rel` with `files`, committed; returns its path.
    fn work(&self, rel: &str, files: &[(&str, &str)]) -> PathBuf {
        let w = self.dir.join(rel);
        if !w.join(".git").exists() {
            std::fs::create_dir_all(&w).unwrap();
            git(&w, &["init", "--quiet", "--initial-branch=main"]);
        }
        write_files(&w, files);
        git(&w, &["add", "--all"]);
        git(&w, &["commit", "--quiet", "--allow-empty", "-m", "c"]);
        w
    }

    /// A bare repository at `rel` holding a commit of `files` on top of
    /// the last; returns its path and the commit.
    fn bare(&self, rel: &str, files: &[(&str, &str)]) -> (PathBuf, String) {
        let b = self.dir.join(rel);
        if !b.exists() {
            std::fs::create_dir_all(&b).unwrap();
            git(&b, &["init", "--bare", "--quiet", "--initial-branch=main"]);
        }
        let w = self.work(&format!("work/{rel}"), files);
        git(
            &w,
            &[
                "push",
                "--quiet",
                "--force",
                b.to_str().unwrap(),
                "HEAD:main",
            ],
        );
        (b, git(&w, &["rev-parse", "HEAD"]))
    }

    /// `host/owner/repo`, served from `PTYRC_TEST_GIT_ROOT`.
    fn hosted(&self, origin: &str, files: &[(&str, &str)]) -> String {
        self.bare(&format!("hosts/{origin}"), files).1
    }

    /// A local registry at `reg/`, named in `store.toml`.
    fn registry(&self, pointers: &[(&str, &str)]) {
        let reg = self.dir.join("reg");
        for (rel, text) in pointers {
            let p = reg.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        std::fs::create_dir_all(&reg).unwrap();
        let store = self.dirs().store();
        std::fs::create_dir_all(store.parent().unwrap()).unwrap();
        std::fs::write(
            store,
            format!(
                "[[registry]]\nname = \"test\"\npath = \"{}\"\n",
                reg.display()
            ),
        )
        .unwrap();
    }

    fn lock(&self) -> String {
        std::fs::read_to_string(self.dirs().lock()).unwrap_or_default()
    }
}

fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "agent")
        .env("GIT_AUTHOR_EMAIL", "agent@example.org")
        .env("GIT_COMMITTER_NAME", "agent")
        .env("GIT_COMMITTER_EMAIL", "agent@example.org")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn app_files(version: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ptyrc-app.toml", manifest("sand", version)),
        ("LICENSE", GPL.to_string()),
        ("app.js", format!("// {version}\n")),
    ]
}

fn files<'a>(v: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    v.iter().map(|(p, t)| (*p, t.as_str())).collect()
}

#[test]
fn add_ls_update_rm() {
    let r = Root::new("trip");
    let one = app_files("0.1.0");
    let (remote, c1) = r.bare("remote.git", &files(&one));
    let spec = remote.to_str().unwrap();
    let origin = Origin::local(&remote, None).unwrap();

    // Untested, no terminal: refused without --yes.
    r.ptyrc(&["store", "add", spec])
        .fails(2, "untested; not installed", "--yes");
    assert!(r.lock().is_empty());
    let out = r.ptyrc(&["store", "add", "--yes", spec]);
    out.ok();
    assert!(out.stdout.contains("added local:"), "{}", out.stdout);
    assert!(out.stderr.contains("untested"), "{}", out.stderr);
    assert!(r.lock().contains(&c1));
    assert!(r.dirs().tree(&origin, &c1).join("app.js").is_file());

    let ls = r.ptyrc(&["store", "ls"]);
    ls.ok();
    let row = ls.stdout.lines().nth(1).unwrap().to_string();
    assert!(row.contains("sand") && row.contains("0.1.0") && row.contains(&c1[..12]));
    assert!(row.ends_with("untested"), "no colour off a terminal: {row}");
    let json: Value = serde_json::from_str(&r.ptyrc(&["--json", "store", "ls"]).stdout).unwrap();
    assert_eq!(json[0]["origin"], origin.id());
    assert_eq!(json[0]["commit"], c1);

    let show = r.ptyrc(&["store", "show", "sand"]);
    show.ok();
    assert!(show.stdout.contains("shape: the grains rest on the lines"));
    assert!(show.stdout.contains(&c1));

    // Again: nothing to do.
    let again = r.ptyrc(&["store", "add", "--yes", spec]);
    assert!(again.ok().stdout.contains("already"), "{}", again.stdout);
    assert!(
        r.ptyrc(&["store", "update"])
            .ok()
            .stdout
            .contains("up to date")
    );

    // A new head: update takes it, untested, with --yes.
    let two = app_files("0.2.0");
    let (_, c2) = r.bare("remote.git", &files(&two));
    r.ptyrc(&["store", "update", "sand"])
        .fails(2, "untested; not installed", "--yes");
    let up = r.ptyrc(&["store", "update", "--yes", "sand"]);
    assert!(up.ok().stdout.contains("updated"), "{}", up.stdout);
    assert!(r.lock().contains(&c2) && !r.lock().contains(&c1));
    // c1 never started: not kept.
    assert!(!r.dirs().tree(&origin, &c1).exists());
    assert!(r.dirs().tree(&origin, &c2).exists());

    let rm = r.ptyrc(&["store", "rm", "sand"]);
    assert!(rm.ok().stdout.contains("removed"));
    assert!(!r.dirs().app(&origin).exists());
    assert!(r.ptyrc(&["store", "ls"]).ok().stdout.is_empty());
    r.ptyrc(&["store", "rm", "sand"])
        .fails(4, "not installed", "ptyrc store ls");
    r.ptyrc(&["store", "show", "sand"])
        .fails(4, "not installed", "ptyrc store ls");
}

#[test]
fn previous_tree_kept_until_started() {
    let r = Root::new("previous");
    let (remote, c1) = r.bare("remote.git", &files(&app_files("0.1.0")));
    let spec = remote.to_str().unwrap();
    let origin = Origin::local(&remote, None).unwrap();
    r.ptyrc(&["store", "add", "--yes", spec]).ok();
    // What a start does, later: the driver's.
    ptyrc_store::install::started(&r.dirs(), &origin).unwrap();
    let (_, c2) = r.bare("remote.git", &files(&app_files("0.2.0")));
    r.ptyrc(&["store", "update", "--yes"]).ok();
    assert!(
        r.dirs().tree(&origin, &c1).exists(),
        "kept: c2 has not started"
    );
    assert!(r.dirs().tree(&origin, &c2).exists());
    let show = r.ptyrc(&["store", "show", "sand"]);
    assert!(show.ok().stdout.contains(&format!("previous     {c1}")));
}

#[test]
fn registry_tiers_and_update_rule() {
    let r = Root::new("tiers");
    let site = "example.org/someone/sand";
    let c1 = r.hosted(site, &files(&app_files("0.1.0")));
    let c2 = r.hosted(site, &files(&app_files("0.2.0")));
    let pointer = |versions: &str| {
        format!("origin = \"https://{site}\"\nmanifest = \"ptyrc-app.toml\"\n{versions}")
    };
    let rel = "apps/example.org/someone/sand.toml";
    r.registry(&[(
        rel,
        &pointer(&format!(
            "[[version]]\ncommit = \"{c1}\"\ntier = \"approved\"\nby = \"maintainer\"\n"
        )),
    )]);

    // Approved: installed without asking, though the head is newer.
    let add = r.ptyrc(&["store", "add", site]);
    assert!(add.ok().stdout.contains("approved"), "{}", add.stdout);
    assert!(r.lock().contains(&c1));
    assert!(
        r.ptyrc(&["store", "update"])
            .ok()
            .stdout
            .contains("up to date")
    );

    // The registry drops c1: the rule picks the untested head; held.
    r.registry(&[(rel, &pointer(""))]);
    let held = r.ptyrc(&["store", "update", "--yes"]);
    assert!(held.ok().stdout.contains("kept at"), "{}", held.stdout);
    assert!(held.stdout.contains("--latest"), "{}", held.stdout);
    assert!(r.lock().contains(&c1));
    let latest = r.ptyrc(&["store", "update", "--latest", "--yes"]);
    assert!(latest.ok().stdout.contains("updated"), "{}", latest.stdout);
    assert!(r.lock().contains(&c2));
    assert!(r.lock().contains("tier = \"untested\""));

    // show: as installed and as listed.
    r.registry(&[(
        rel,
        &pointer(&format!(
            "[[version]]\ncommit = \"{c1}\"\ntier = \"approved\"\n\n\
             [[version]]\ncommit = \"{c2}\"\ntier = \"tested\"\n"
        )),
    )]);
    let show = r.ptyrc(&["store", "show", site]);
    assert!(
        show.ok().stdout.contains("listed in    test"),
        "{}",
        show.stdout
    );
    assert!(show.stdout.contains(&format!("{c2} tested")));

    // search, over the registry present.
    let found = r.ptyrc(&["store", "search", "SAND"]);
    assert!(found.ok().stdout.contains(site), "{}", found.stdout);
    assert!(found.stdout.contains("approved") && found.stdout.contains("installed"));
    let none = r.ptyrc(&["store", "search", "nothing"]);
    assert_eq!(none.status, 1);
    assert!(none.stderr.contains("no app matches"));
    let json: Value =
        serde_json::from_str(&r.ptyrc(&["--json", "store", "search", "sand"]).stdout).unwrap();
    assert_eq!(json[0]["origin"], site);
    assert_eq!(json[0]["tier"], "approved");
}

#[test]
fn yanked() {
    let r = Root::new("yanked");
    let site = "example.org/someone/sand";
    let c1 = r.hosted(site, &files(&app_files("0.1.0")));
    let rel = "apps/example.org/someone/sand.toml";
    let pointer = |app: &str, version: &str| {
        format!(
            "origin = \"https://{site}\"\n{app}\n[[version]]\ncommit = \"{c1}\"\ntier = \"approved\"\n{version}"
        )
    };
    r.registry(&[(rel, &pointer("", "yanked = true\n"))]);
    let at = format!("{site}@{c1}");
    r.ptyrc(&["store", "add", &at])
        .fails(2, "yanked", "--force");
    // Not listed otherwise: the head is that commit, yanked.
    r.ptyrc(&["store", "add", "--yes", site])
        .fails(2, "yanked", "--force");
    assert!(r.lock().is_empty());
    let forced = r.ptyrc(&["store", "add", "--force", &at]);
    assert!(forced.ok().stdout.contains("approved"), "{}", forced.stdout);
    r.ptyrc(&["store", "rm", site]).ok();

    r.registry(&[(rel, &pointer("yanked = true", ""))]);
    r.ptyrc(&["store", "add", site])
        .fails(2, "yanked", "--force");
    r.ptyrc(&["store", "add", "--force", site]).ok();
}

#[test]
fn rm_purge() {
    let r = Root::new("purge");
    let (remote, _) = r.bare("remote.git", &files(&app_files("0.1.0")));
    let spec = remote.to_str().unwrap();
    let origin = Origin::local(&remote, None).unwrap();
    r.ptyrc(&["store", "add", "--yes", spec]).ok();
    let d = r.dirs();
    std::fs::create_dir_all(d.storage(&origin)).unwrap();
    std::fs::write(d.storage(&origin).join("save"), "x").unwrap();
    std::fs::create_dir_all(d.permissions().parent().unwrap()).unwrap();
    std::fs::write(
        d.permissions(),
        format!(
            "[app.\"{}\"]\nallow = [\"shape\"]\ndeny = []\n\n\
             [app.\"example.org/other/app\"]\nallow = [\"shape\"]\ndeny = []\n",
            origin.id()
        ),
    )
    .unwrap();

    // Without --purge: kept.
    r.ptyrc(&["store", "rm", "sand"]).ok();
    assert!(d.storage(&origin).exists());
    r.ptyrc(&["store", "add", "--yes", spec]).ok();
    let out = r.ptyrc(&["store", "rm", "--purge", "sand"]);
    assert!(
        out.ok().stdout.contains("its storage and its grants"),
        "{}",
        out.stdout
    );
    assert!(!d.storage(&origin).exists());
    let grants = std::fs::read_to_string(d.permissions()).unwrap();
    assert!(!grants.contains(&origin.id()), "{grants}");
    assert!(grants.contains("example.org/other/app"), "{grants}");
    assert!(!d.app(&origin).exists());
}

#[test]
fn rust_app_not_built_yet() {
    let r = Root::new("rust");
    let m = manifest("crab", "0.1.0").replace(
        "script = \"app.js\"",
        "rust = \"src/lib.rs\"\nrustc = \"1.85\"",
    );
    let (remote, _) = r.bare(
        "remote.git",
        &[("ptyrc-app.toml", &m), ("LICENSE", GPL), ("src/lib.rs", "")],
    );
    r.ptyrc(&["store", "add", "--yes", remote.to_str().unwrap()])
        .fails(2, "Rust apps are not built yet", "script");
    assert!(r.lock().is_empty());
}

#[test]
fn search_without_registry() {
    let r = Root::new("noreg");
    r.ptyrc(&["store", "search", "sand"])
        .fails(2, "no registry", "[[registry]]");
}

#[test]
fn bad_origins() {
    let r = Root::new("origins");
    r.ptyrc(&["store", "add", "ssh://example.org/a/b"]).fails(
        2,
        "https only",
        "https://host/owner/repo",
    );
    r.ptyrc(&["store", "add", "./no-such-dir"])
        .fails(2, "no-such-dir", "git repository");
    let empty = r.dir.join("empty.git");
    std::fs::create_dir_all(&empty).unwrap();
    git(&empty, &["init", "--bare", "--quiet"]);
    let out = r.ptyrc(&["store", "add", "--yes", empty.to_str().unwrap()]);
    assert_eq!(out.status, 5, "{}", out.stderr);
}

#[test]
fn dev_check() {
    let r = Root::new("check");
    let good = r.dir.join("good");
    write_files(&good, &files(&app_files("0.1.0")));
    let out = r.ptyrc(&["dev", "check", good.to_str().unwrap()]);
    assert!(
        out.ok().stdout.starts_with("ok: sand 0.1.0"),
        "{}",
        out.stdout
    );

    let bad = |name: &str, edit: &dyn Fn(&Path)| {
        let d = r.dir.join(name);
        write_files(&d, &files(&app_files("0.1.0")));
        edit(&d);
        r.ptyrc(&["dev", "check", d.to_str().unwrap()])
    };
    bad("nolicence", &|d| {
        std::fs::remove_file(d.join("LICENSE")).unwrap()
    })
    .fails(1, "GPL version 3", "LICENSE or COPYING");
    bad("noscript", &|d| {
        std::fs::remove_file(d.join("app.js")).unwrap()
    })
    .fails(1, "app.js: named by [app] script", "add app.js");
    bad("badtoml", &|d| {
        std::fs::write(d.join("ptyrc-app.toml"), "[app]\nid = \"x\"\nwho = 1\n").unwrap()
    })
    .fails(1, "ptyrc-app.toml:", "");
    bad("bigjs", &|d| {
        std::fs::write(d.join("big.js"), vec![b' '; (1 << 20) + 1]).unwrap()
    })
    .fails(1, "big.js", "1 MiB");
    // Every problem, each with its hint.
    let many = bad("many", &|d| {
        std::fs::remove_file(d.join("LICENSE")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", d.join("link")).unwrap();
        std::fs::remove_file(d.join("app.js")).unwrap();
    });
    many.fails(1, "link: a symlink", "regular files only");
    many.fails(1, "GPL version 3", "LICENSE or COPYING");
    many.fails(1, "app.js: named by", "add app.js");
    assert!(
        many.stderr.ends_with("many: 3 problems\n"),
        "{}",
        many.stderr
    );
    r.ptyrc(&["dev", "check", "/no/such/dir"])
        .fails(4, "/no/such/dir", "ptyrc-app.toml");
}

#[test]
fn dev_entry() {
    let r = Root::new("entry");
    let app = app_files("0.1.0");
    let mut nested: Vec<(String, String)> = Vec::new();
    for (p, t) in &app {
        nested.push((format!("apps/sand/{p}"), t.clone()));
    }
    let nested: Vec<(&str, &str)> = nested
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let w = r.work("repo", &nested);
    git(
        &w,
        &[
            "remote",
            "add",
            "origin",
            "git@Example.org:someone/garden.git",
        ],
    );
    let head = git(&w, &["rev-parse", "HEAD"]);
    let dir = w.join("apps/sand");
    let out = r.ptyrc(&["dev", "entry", dir.to_str().unwrap()]);
    out.ok();
    let (first, body) = out.stdout.split_once('\n').unwrap();
    assert_eq!(first, "# apps/example.org/someone/garden/apps/sand.toml");
    let p = Pointer::parse("entry", body).unwrap();
    assert_eq!(p.origin, "https://example.org/someone/garden");
    assert_eq!(p.manifest, "apps/sand/ptyrc-app.toml");
    assert_eq!(p.versions.len(), 1);
    assert_eq!(p.versions[0].commit, head);
    assert_eq!(p.versions[0].tier, Tier::Untested);
    assert_eq!(
        p.origin().unwrap(),
        Origin::parse("example.org/someone/garden#apps/sand").unwrap()
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);

    // Uncommitted changes: said.
    std::fs::write(dir.join("app.js"), "// changed\n").unwrap();
    let out = r.ptyrc(&["dev", "entry", dir.to_str().unwrap()]);
    assert!(out.ok().stderr.contains("uncommitted"), "{}", out.stderr);

    // No remote; not the app's directory.
    let lone = r.work("lone", &files(&app));
    r.ptyrc(&["dev", "entry", lone.to_str().unwrap()])
        .fails(2, "no git remote", "git remote add");
    r.ptyrc(&["dev", "entry", w.to_str().unwrap()])
        .fails(4, "no ptyrc-app.toml", "dev check");
}

#[test]
fn untested_asks_on_a_terminal() {
    let r = Root::new("ask");
    let (remote, c1) = r.bare("remote.git", &files(&app_files("0.1.0")));
    let argv = [BIN, "store", "add", remote.to_str().unwrap()].map(str::to_string);
    let answer = |text: &[u8]| {
        let pty = ptyrc_server::pty::spawn(&argv, &r.env(), 80, 24).unwrap();
        let mut host = common::HostPty {
            master: pty.master,
            seen: Vec::new(),
        };
        let mut proc = common::Proc(pty.child);
        host.wait_seen("install it? [y/N] ");
        let seen = String::from_utf8_lossy(&host.seen).into_owned();
        assert!(seen.contains("\x1b[31muntested\x1b[0m"), "in red: {seen:?}");
        host.type_in(text);
        (
            host.wait_exit(&mut proc),
            String::from_utf8_lossy(&host.seen).into_owned(),
        )
    };
    let (status, seen) = answer(b"n\r");
    assert_eq!(status, 1, "{seen}");
    assert!(seen.contains("not installed"), "{seen}");
    assert!(r.lock().is_empty());
    let (status, seen) = answer(b"y\r");
    assert_eq!(status, 0, "{seen}");
    assert!(seen.contains("added"), "{seen}");
    assert!(r.lock().contains(&c1));
}
