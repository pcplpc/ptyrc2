// SPDX-License-Identifier: GPL-3.0-or-later
//! The harness of the shipped store apps, `plugins/store-apps/NAME/`
//! (`plugins/store-apps/README.md`). For each tree: `ptyrc dev check`;
//! its id its directory; its `LICENSE` the repository's; each draw of
//! `[app] random` read by its options; each case `tests/CASE.args`, the
//! words after `ptyrc dev test TREE` (run in the tree), its screens
//! against `tests/CASE.txt`; `ptyrc dev test --unit` when `tests.js`
//! exists; a Rust app's `cargo test`.
//!
//! `PTYRC_BLESS=1` writes each `CASE.txt` from the run, in place of the
//! comparison. `PTYRC_STORE_APP=NAME` runs that tree alone. Without the
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

/// `ptyrc ARGS` in `dir`, `ptyrc` first on `PATH`.
fn ptyrc(dir: &Path, args: &[String]) -> Output {
    let bin = Path::new(BIN).parent().unwrap();
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("PTYRC")
        .env_remove("PTYRC_JS")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// The words of a `.args` file: split on white space; `#` to the end
/// of a line is a comment. No quoting.
fn words(text: &str) -> Vec<String> {
    (text.lines())
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(str::split_whitespace)
        .map(str::to_string)
        .collect()
}

fn said(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout\n{}--- stderr\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A first line that differs, and the two sides.
fn differ(want: &str, got: &str) -> String {
    let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let at = (0..w.len().max(g.len()))
        .find(|&i| w.get(i) != g.get(i))
        .unwrap_or(0);
    format!(
        "first difference at line {}:\n  expected: {:?}\n  got:      {:?}\n--- got\n{got}",
        at + 1,
        w.get(at).copied().unwrap_or("<end>"),
        g.get(at).copied().unwrap_or("<end>"),
    )
}

/// One case: `tests/CASE.args`; problems in `fails`.
fn case(tree: &Path, args: &Path, bless: bool) -> Option<String> {
    let name = args.file_stem().unwrap().to_string_lossy().into_owned();
    let expected = args.with_extension("txt");
    let mut argv: Vec<String> = vec!["dev".into(), "test".into(), tree.display().to_string()];
    argv.extend(words(&std::fs::read_to_string(args).unwrap()));
    let out = ptyrc(tree, &argv);
    if !out.status.success() {
        return Some(format!(
            "case {name}: ptyrc {}\n{}",
            argv.join(" "),
            said(&out)
        ));
    }
    let got = String::from_utf8(out.stdout).unwrap();
    if bless {
        std::fs::write(&expected, &got).unwrap();
        return None;
    }
    match std::fs::read_to_string(&expected) {
        Ok(want) if want == got => None,
        Ok(want) => Some(format!(
            "case {name}: the screens differ from {}; PTYRC_BLESS=1 if meant\n{}",
            expected.display(),
            differ(&want, &got)
        )),
        Err(_) => Some(format!(
            "case {name}: no {}; PTYRC_BLESS=1 writes it",
            expected.display()
        )),
    }
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
    for draw in &m.app.random {
        if let Err(e) = ptyrc_store::options::read(&m, draw, dir) {
            fails.push(format!("[app] random: {draw:?}: {e}"));
        }
    }
    let rust = matches!(m.app.entry, Entry::Rust(_));
    if !rust && !js {
        return fails;
    }
    let mut cases: Vec<PathBuf> = (std::fs::read_dir(dir.join("tests")).into_iter())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "args"))
        .collect();
    cases.sort();
    if cases.is_empty() {
        fails.push("no case: tests/CASE.args".into());
    }
    std::thread::scope(|s| {
        let runs: Vec<_> = (cases.iter())
            .map(|c| s.spawn(move || case(dir, c, bless)))
            .collect();
        fails.extend(runs.into_iter().filter_map(|r| r.join().unwrap()));
    });
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

#[test]
fn the_words_of_a_case() {
    assert_eq!(
        words("--size 40x8 # the screen\n# a line\n--at 1s,2s\n"),
        ["--size", "40x8", "--at", "1s,2s"]
    );
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
