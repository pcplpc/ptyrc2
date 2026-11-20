// SPDX-License-Identifier: GPL-3.0-or-later
//! The harness of the shipped store apps, `plugins/store-apps/NAME/`
//! (`plugins/store-apps/README.md`). For each tree: `ptyrc dev check`;
//! its id its directory; its `LICENSE` the repository's; each draw of
//! `[app] random` read by its options; its cases, `ptyrc dev test TREE
//! --cases` (each `tests/CASE.args` against `tests/CASE.txt`); `ptyrc
//! dev test --unit` when `tests.js` exists; a Rust app's `cargo test`;
//! each file named as one of `_shared/js/` that file, byte for byte.
//! `_shared/js/`: its `*.tests.js` run once, in a tree of their own.
//!
//! `PTYRC_BLESS=1`: `--cases --bless`, each `CASE.txt` written from the
//! run. `PTYRC_SYNC=1`: each copy of `_shared/js/` written from it. `PTYRC_STORE_APP=NAME` runs that tree alone. Without the
//! interpreter (`sdk/js/interp/build.sh`) the script apps are skipped,
//! unless `PTYRC_REQUIRE_JS=1`.

use ptyrc_store::manifest::{self, Entry};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const JS: &str = "sdk/js/interp/target/ptyrc-js.wasm";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| v == "1")
}

/// The runner beside `ptyrc`, as built now.
fn build_runner() {
    let built = Command::new(env!("CARGO"))
        .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(built.success(), "cargo build -p ptyrc-box");
}

/// The runner the cases use: `target/release/ptyrc-box` when it was
/// built after the debug one (a start compiles the interpreter: some
/// 2 s of processor time against 7 s), else the debug one beside
/// `ptyrc`. `PTYRC_BOX` set: that one.
fn runner() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("PTYRC_BOX") {
        return Some(named.into());
    }
    let debug = Path::new(BIN).with_file_name("ptyrc-box");
    let release = debug.parent()?.parent()?.join("release/ptyrc-box");
    let built = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (built(&release)? > built(&debug)?).then_some(release)
}

/// `ptyrc ARGS` in `dir`, `ptyrc` first on `PATH`.
fn ptyrc(dir: &Path, args: &[String]) -> Output {
    let bin = Path::new(BIN).parent().unwrap();
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let mut c = Command::new(BIN);
    if let Some(r) = runner() {
        c.env("PTYRC_BOX", r);
    }
    c.args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("PTYRC")
        .env_remove("PTYRC_JS")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn said(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout\n{}--- stderr\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The files of `_shared/js/` that trees copy: name, bytes. Not its
/// `*.tests.js`.
fn shared() -> Vec<(String, Vec<u8>)> {
    let dir = root().join("plugins/store-apps/_shared/js");
    let mut out: Vec<(String, Vec<u8>)> = (std::fs::read_dir(dir).unwrap().flatten())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), p))
        .filter(|(n, _)| n.ends_with(".js") && !n.ends_with(".tests.js"))
        .map(|(n, p)| (n, std::fs::read(p).unwrap()))
        .collect();
    out.sort();
    out
}

/// The files of `dir` and below, `target/` and `tests/` left out.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() && name != "target" && name != "tests" && name != ".git" {
            out.extend(files(&p));
        } else if p.is_file() {
            out.push(p);
        }
    }
    out
}

/// Each copy of a file of `_shared/js/` in `dir`: that file, or written
/// from it with `sync`.
fn copies(dir: &Path, shared: &[(String, Vec<u8>)], sync: bool) -> Vec<String> {
    let mut fails = Vec::new();
    for f in files(dir) {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let Some((_, source)) = shared.iter().find(|(n, _)| *n == name) else {
            continue;
        };
        if std::fs::read(&f).ok().as_ref() == Some(source) {
            continue;
        }
        let at = f.strip_prefix(dir).unwrap().display().to_string();
        if sync {
            std::fs::write(&f, source).unwrap();
            eprintln!("wrote {at} from _shared/js/{name}");
        } else {
            fails.push(format!(
                "{at}: not a copy of _shared/js/{name} (PTYRC_SYNC=1 copies it)"
            ));
        }
    }
    fails
}

/// The unit tests of `_shared/js/`, once: its files and a `tests.js`
/// exporting every `*.tests.js`, as an app of a scratch tree.
#[test]
fn shared_unit_tests() {
    if !root().join(JS).is_file() {
        assert!(!flag("PTYRC_REQUIRE_JS"), "PTYRC_REQUIRE_JS=1: no {JS}");
        eprintln!("store apps: _shared/js unit tests SKIPPED: no interpreter");
        return;
    }
    build_runner();
    let src = root().join("plugins/store-apps/_shared/js");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-apps-shared");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut tests = String::from("// SPDX-License-Identifier: GPL-3.0-or-later\n");
    let mut names: Vec<String> = (std::fs::read_dir(&src).unwrap().flatten())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".js"))
        .collect();
    names.sort();
    for n in &names {
        std::fs::copy(src.join(n), dir.join(n)).unwrap();
        if n.ends_with(".tests.js") {
            tests.push_str(&format!("export * from \"./{n}\";\n"));
        }
    }
    std::fs::write(dir.join("tests.js"), tests).unwrap();
    std::fs::write(dir.join("app.js"), "export function init() {}\n").unwrap();
    std::fs::write(
        dir.join(manifest::MANIFEST),
        "[app]\nid = \"shared\"\nversion = \"0.1.0\"\nsummary = \"_shared/js\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n",
    )
    .unwrap();
    let unit = ptyrc(
        &dir,
        &["dev".into(), "test".into(), ".".into(), "--unit".into()],
    );
    assert!(
        unit.status.success(),
        "_shared/js: dev test --unit\n{}",
        said(&unit)
    );
}

/// Every problem of the tree `dir`.
fn tree(dir: &Path, js: bool, bless: bool) -> Vec<String> {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let mut fails = Vec::new();
    let check = ptyrc(dir, &["dev".into(), "check".into(), ".".into()]);
    if !check.status.success() {
        fails.push(format!("dev check\n{}", said(&check)));
    }
    let Ok(m) = manifest::parse_bytes(&std::fs::read(dir.join(manifest::MANIFEST)).unwrap()) else {
        return fails;
    };
    if m.app.id != name {
        fails.push(format!("[app] id = {:?}: not its directory", m.app.id));
    }
    let license = std::fs::read(root().join("LICENSE.txt")).unwrap();
    if std::fs::read(dir.join("LICENSE")).ok() != Some(license) {
        fails.push("LICENSE: not a copy of the repository's LICENSE.txt".into());
    }
    fails.extend(copies(dir, &shared(), flag("PTYRC_SYNC")));
    for draw in &m.app.random {
        if let Err(e) = ptyrc_store::options::read(&m, draw, dir) {
            fails.push(format!("[app] random: {draw:?}: {e}"));
        }
    }
    let rust = matches!(m.app.entry, Entry::Rust(_));
    if !rust && !js {
        return fails;
    }
    let mut argv: Vec<String> = vec!["dev".into(), "test".into(), ".".into(), "--cases".into()];
    if bless {
        argv.push("--bless".into());
    }
    let cases = ptyrc(dir, &argv);
    if !cases.status.success() {
        fails.push(format!("dev test --cases\n{}", said(&cases)));
    }
    if !rust && dir.join("tests.js").is_file() {
        let unit = ptyrc(
            dir,
            &["dev".into(), "test".into(), ".".into(), "--unit".into()],
        );
        if !unit.status.success() {
            fails.push(format!("dev test --unit\n{}", said(&unit)));
        }
    }
    if rust {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("store-apps")
            .join(&name);
        let unit = Command::new(env!("CARGO"))
            .args(["test", "--quiet", "--manifest-path"])
            .arg(dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", target)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        if !unit.status.success() {
            fails.push(format!("cargo test\n{}", said(&unit)));
        }
    }
    fails.into_iter().map(|f| format!("{name}: {f}")).collect()
}

#[test]
fn every_shipped_tree() {
    let apps = root().join("plugins/store-apps");
    let only = std::env::var("PTYRC_STORE_APP").ok();
    let mut trees: Vec<PathBuf> = (std::fs::read_dir(&apps).unwrap().flatten())
        .map(|e| e.path())
        .filter(|p| p.join(manifest::MANIFEST).is_file())
        .filter(|p| only.as_deref().is_none_or(|o| p.file_name().unwrap() == o))
        .collect();
    trees.sort();
    assert!(!trees.is_empty(), "no tree under {}", apps.display());
    let js = root().join(JS).is_file();
    if !js {
        assert!(!flag("PTYRC_REQUIRE_JS"), "PTYRC_REQUIRE_JS=1: no {JS}");
        eprintln!("store apps: script cases SKIPPED: no interpreter; sdk/js/interp/build.sh");
    }
    build_runner();
    let bless = flag("PTYRC_BLESS");
    let fails: Vec<String> = std::thread::scope(|s| {
        let runs: Vec<_> = (trees.iter())
            .map(|t| s.spawn(move || tree(t, js, bless)))
            .collect();
        runs.into_iter().flat_map(|r| r.join().unwrap()).collect()
    });
    assert!(fails.is_empty(), "\n{}", fails.join("\n\n"));
}

/// `ptyrc ARGS` with the shipped trees of the repository and empty
/// XDG directories.
fn store(home: &Path, args: &[&str]) -> Output {
    let mut c = Command::new(BIN);
    c.args(args)
        .env("PTYRC_STORE_APPS", root().join("plugins/store-apps"))
        .env("HOME", home)
        .stdin(Stdio::null());
    for (var, dir) in [
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_CONFIG_HOME", "config"),
    ] {
        c.env(var, home.join(dir));
    }
    c.output().unwrap()
}

#[test]
fn the_store_and_a_shipped_app() {
    let home =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("shipped-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();
    let ls = store(&home, &["store", "ls"]);
    assert!(ls.status.success(), "{}", said(&ls));
    let line = text(&ls)
        .lines()
        .find(|l| l.starts_with("ptyrc/sand"))
        .map(str::to_string);
    let line = line.unwrap_or_else(|| panic!("{}", said(&ls)));
    assert!(
        line.ends_with("shipped") && line.contains(" sand "),
        "{line}"
    );
    let show = store(&home, &["store", "show", "ptyrc/confetti"]);
    assert!(show.status.success(), "{}", said(&show));
    let shown = text(&show);
    assert!(
        shown.contains("(shipped)") && shown.contains("plugins/store-apps/confetti"),
        "{shown}"
    );
    let rm = store(&home, &["store", "rm", "sand"]);
    assert_eq!(rm.status.code(), Some(2), "{}", said(&rm));
    assert!(String::from_utf8_lossy(&rm.stderr).contains("ptyrc/sand: shipped with ptyrc"));
    let up = store(&home, &["store", "update", "ptyrc/sand"]);
    assert!(
        up.status.success() && text(&up).contains("updated with it"),
        "{}",
        said(&up)
    );
    let help = store(&home, &["app", "ptyrc/sand", "--help"]);
    assert!(
        text(&help).contains("ptyrc/sand 0.2.0, shipped"),
        "{}",
        said(&help)
    );
}
