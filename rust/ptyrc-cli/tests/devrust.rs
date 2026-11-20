// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev test` of a Rust app: one build for the cases, two runs at
//! once on one tree, the module kept while the sources are the same.
//! Skipped without `rustc` and its wasm target, unless
//! `PTYRC_REQUIRE_RUSTC=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");

fn has_rustc() -> bool {
    let ok = ptyrc_store::build::Toolchain::find();
    if let Err(e) = &ok {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {e}"
        );
        eprintln!("skipped: {e}");
    }
    ok.is_ok()
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

fn ptyrc(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env_remove("PTYRC")
        .env_remove("PTYRC_BOX")
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

const LIB: &str = r#"use ptyrc_app::{App, Canvas, Value, period};
struct A(Canvas, u32);
impl App for A {
    fn init(_: Value) -> A {
        period(50);
        A(Canvas::new(10, 1).unwrap(), 0)
    }
    fn tick(&mut self, _: u64) {
        self.1 += 1;
        self.0.text(0, 0, &format!("tick {}", self.1));
        self.0.present(&Value::Null);
    }
}
ptyrc_app::app!(A);
"#;

/// A Rust app of `lib` and `cases` (`NAME`, its words).
fn tree(tag: &str, lib: &str, cases: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("devrust-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    let manifest = "[app]\nid = \"ticker\"\nversion = \"0.1.0\"\nsummary = \"Counts ticks.\"\n\
                    license = \"GPL-3.0-or-later\"\napi = 1\nrust = \"src/lib.rs\"\nsurface = \"frame\"\n";
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    let gpl = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE.txt");
    std::fs::copy(gpl, dir.join("LICENSE")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
    for (name, words) in cases {
        std::fs::write(dir.join("tests").join(format!("{name}.args")), words).unwrap();
    }
    dir
}

#[test]
fn the_cases_of_a_rust_app_build_once() {
    if !has_rustc() {
        return;
    }
    build_runner();
    let cases: Vec<(String, String)> = (0..6)
        .map(|i| {
            (
                format!("c{i}"),
                format!("--size 20x2 --for {}ms\n", 100 + 50 * i),
            )
        })
        .collect();
    let cases: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (&a[..], &b[..])).collect();
    let dir = tree("cases", LIB, &cases);
    let blessed = ptyrc(&dir, &["dev", "test", ".", "--cases", "--bless"]);
    assert!(blessed.status.success(), "{}", said(&blessed));
    let module = dir.join("target/ptyrc/app.wasm");
    let when = || std::fs::metadata(&module).unwrap().modified().unwrap();
    // `--bless` wrote `tests/*.txt`, files of the tree: built again once.
    let out = ptyrc(&dir, &["dev", "test", ".", "--cases"]);
    assert!(out.status.success(), "{}", said(&out));
    let first = when();
    for _ in 0..3 {
        let out = ptyrc(&dir, &["dev", "test", ".", "--cases"]);
        assert!(out.status.success(), "{}", said(&out));
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(text.lines().filter(|l| l.starts_with("ok   ")).count(), 6);
    }
    assert_eq!(when(), first, "built again, the sources the same");
    // A change of the sources: built again.
    std::fs::write(dir.join("src/lib.rs"), format!("{LIB}// changed\n")).unwrap();
    let out = ptyrc(&dir, &["dev", "test", ".", "--cases"]);
    assert!(out.status.success(), "{}", said(&out));
    assert_ne!(when(), first);
}

#[test]
fn two_runs_at_once_on_one_tree() {
    if !has_rustc() {
        return;
    }
    build_runner();
    let dir = tree("twice", LIB, &[]);
    let runs: Vec<Output> = std::thread::scope(|s| {
        let all: Vec<_> = (0..3)
            .map(|_| s.spawn(|| ptyrc(&dir, &["dev", "test", ".", "--size", "20x2"])))
            .collect();
        all.into_iter().map(|r| r.join().unwrap()).collect()
    });
    for out in &runs {
        assert!(out.status.success(), "{}", said(out));
        assert!(String::from_utf8_lossy(&out.stdout).contains("tick"));
    }
}

#[test]
fn a_failed_build_of_the_app_blames_its_sources() {
    if !has_rustc() {
        return;
    }
    build_runner();
    let dir = tree("broken", "fn broken( {}\n", &[("a", "--size 20x2\n")]);
    let out = ptyrc(&dir, &["dev", "test", ".", "--cases"]);
    let text = said(&out);
    assert!(!out.status.success(), "{text}");
    assert!(!text.contains("rustup update"), "{text}");
    // Said once, before any case.
    assert!(text.contains("src/lib.rs"), "{text}");
    assert!(!text.contains("tests/a"), "{text}");
}

/// `--timing`: a last line; `--json`: `ticks`, `tick_ms`; the cases
/// never see it.
#[test]
fn the_time_of_a_tick() {
    if !has_rustc() {
        return;
    }
    build_runner();
    let dir = tree("timing", LIB, &[("a", "--size 20x2 --for 1s\n")]);
    let out = ptyrc(&dir, &["dev", "test", ".", "--size", "20x2", "--timing"]);
    assert!(out.status.success(), "{}", said(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().last().unwrap();
    assert!(
        last.starts_with("ticks: 20; a tick, wall time: mean "),
        "{text}"
    );
    let plain = ptyrc(&dir, &["dev", "test", ".", "--size", "20x2"]);
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("ticks"));
    let json = ptyrc(&dir, &["dev", "test", ".", "--size", "20x2", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["ticks"], 20);
    assert!(v["tick_ms"]["max"].as_f64().unwrap() >= v["tick_ms"]["mean"].as_f64().unwrap());
    let cases = ptyrc(&dir, &["dev", "test", ".", "--cases", "--timing"]);
    assert_eq!(cases.status.code(), Some(2), "{}", said(&cases));
}
