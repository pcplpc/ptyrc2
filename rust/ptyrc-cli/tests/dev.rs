// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev new`, `test`, `run`; the examples of `sdk/`. A script app
//! wants the interpreter module (`sdk/js/README.md`), a Rust app the
//! target `wasm32-unknown-unknown`: without them the test passes and
//! says so on the terminal.

mod common;

use common::{BIN, Sandbox, wait_for};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

const JS: &str = "../../sdk/js/interp/target/wasm32-unknown-unknown/release/ptyrc_js.wasm";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The runner beside `ptyrc`, as built now.
fn build_runner() {
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let built = Command::new(env!("CARGO"))
            .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(built.success(), "cargo build -p ptyrc-box");
    });
}

/// Past the capture of the harness.
fn skipped(what: &str) {
    let _ = writeln!(std::io::stderr(), "dev tests SKIPPED: {what}");
}

fn has_js() -> bool {
    let ok = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS).is_file();
    if !ok {
        skipped("no interpreter; cargo build --release --manifest-path sdk/js/interp/Cargo.toml");
    }
    ok
}

fn has_target() -> bool {
    let out = Command::new("rustc")
        .args([
            "--print",
            "target-libdir",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .output();
    let ok = out.is_ok_and(|o| {
        o.status.success() && Path::new(String::from_utf8_lossy(&o.stdout).trim()).is_dir()
    });
    if !ok {
        skipped("no wasm32 target; rustup target add wasm32-unknown-unknown");
    }
    ok
}

fn scratch(tag: &str) -> PathBuf {
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("dev-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `ptyrc ARGS` in `dir`, `ptyrc` first on `PATH`.
fn ptyrc(dir: &Path, args: &[&str]) -> Output {
    build_runner();
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

#[track_caller]
fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "status {:?}\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn sh(dir: &Path, script: &str) -> Output {
    let bin = Path::new(BIN).parent().unwrap().to_path_buf();
    build_runner();
    Command::new(dir.join(script))
        .current_dir(dir)
        .env("PTYRC", bin.join("ptyrc"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// What a new app is: it passes `check` and its own test, untouched.
fn new_app(tag: &str, rust: bool) -> PathBuf {
    let top = scratch(tag);
    let mut args = vec!["dev", "new", "clock"];
    if rust {
        args.push("--rust");
    }
    let said = ok(&ptyrc(&top, &args));
    assert!(said.contains("./test.sh"), "{said}");
    let app = top.join("clock");
    assert!(app.join(".git").is_dir());
    for f in [
        "ptyrc-app.toml",
        "AGENTS.md",
        "LICENSE.txt",
        "test.sh",
        "expected.txt",
    ] {
        assert!(app.join(f).is_file(), "{f}");
    }
    assert!(ok(&ptyrc(&app, &["dev", "check"])).starts_with("ok: clock 0.1.0"));
    ok(&sh(&app, "test.sh"));
    assert!(!app.join("actual.txt").exists());
    app
}

#[test]
fn new_script_app() {
    if !has_js() {
        return;
    }
    let app = new_app("script", false);
    assert!(app.join("ptyrc.d.ts").is_file());
    // test.sh compared a run to the template's: the same text again.
    let expected = std::fs::read_to_string(app.join("expected.txt")).unwrap();
    assert!(expected.contains("t = 0.5 s"), "{expected}");
    let out = ok(&ptyrc(
        &app,
        &[
            "dev",
            "test",
            "--size",
            "40x4",
            "--for",
            "1s",
            "--at",
            "0,500,1000",
        ],
    ));
    assert_eq!(out, expected);
}

#[test]
fn new_rust_app() {
    if !has_target() {
        return;
    }
    let app = new_app("rust", true);
    assert!(app.join("vendor/ptyrc-app/src/lib.rs").is_file());
    let vendored = std::fs::read_to_string(app.join("vendor/ptyrc-app/Cargo.toml")).unwrap();
    assert!(!vendored.contains("[workspace]"));
    // Built: `target/` is not part of the tree.
    assert!(app.join("target/CACHEDIR.TAG").is_file());
    ok(&ptyrc(&app, &["dev", "check"]));
    let args = [
        "dev", "test", "--for", "2s", "--at", "1s,2s", "--arg", "label=x",
    ];
    let a = ok(&ptyrc(&app, &args));
    assert!(a.contains("x = 2.0 s"), "{a}");
    assert_eq!(a, ok(&ptyrc(&app, &args)));
}

#[test]
fn new_refuses() {
    let top = scratch("refuses");
    let out = ptyrc(&top, &["dev", "new", "Bad_Name"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not an app id"));
    std::fs::create_dir_all(top.join("full")).unwrap();
    std::fs::write(top.join("full/x"), "").unwrap();
    let out = ptyrc(&top, &["dev", "new", "full"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not empty"));
}

/// Options are checked against the manifest before anything runs.
#[test]
fn test_options_and_screens() {
    let top = scratch("opts");
    ok(&ptyrc(&top, &["dev", "new", "clock"]));
    let app = top.join("clock");
    let out = ptyrc(&app, &["dev", "test", "--arg", "nope=1"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("nope: no such option") && err.contains("[options]: label"),
        "{err}"
    );
    let out = ptyrc(&app, &["dev", "test", "--tape", "t", "--size", "9x9"]);
    assert_eq!(out.status.code(), Some(2));
    let out = ptyrc(&app, &["dev", "test", "--size", "nine"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("COLSxROWS"));
    if !has_js() {
        return;
    }
    // Over a screen given as text, and over a tape.
    std::fs::write(app.join("screen.txt"), "$ ls\nREADME\n").unwrap();
    let out = ok(&ptyrc(
        &app,
        &[
            "dev",
            "test",
            "--size",
            "30x3",
            "--for",
            "0",
            "--screen",
            "screen.txt",
        ],
    ));
    assert_eq!(out, "--- 0 ms\n$ ls                 t = 0.0 s\nREADME\n");
    let tape =
        "{\"ptyrc\":\"tape\",\"version\":2,\"size\":[30,2]}\n{\"t\":200000,\"o\":\"hello\"}\n";
    std::fs::write(app.join("t.tape"), tape).unwrap();
    let out = ok(&ptyrc(
        &app,
        &[
            "dev", "test", "--tape", "t.tape", "--for", "300ms", "--at", "100,300",
        ],
    ));
    assert_eq!(
        out,
        "--- 100 ms\n                     t = 0.1 s\n--- 300 ms\nhello                t = 0.3 s\n"
    );
}

/// Every example passes `dev check` and `dev test`.
#[test]
fn examples() {
    let mut dirs: Vec<PathBuf> = ["sdk/js/examples", "sdk/rust/examples"]
        .iter()
        .flat_map(|d| std::fs::read_dir(root().join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("ptyrc-app.toml").is_file())
        .collect();
    dirs.sort();
    assert!(dirs.len() >= 2, "{dirs:?}");
    for dir in dirs {
        ok(&ptyrc(&dir, &["dev", "check"]));
        let rust = dir.join("Cargo.toml").is_file();
        if (rust && !has_target()) || (!rust && !has_js()) {
            continue;
        }
        let out = ok(&ptyrc(
            &dir,
            &[
                "dev", "test", "--size", "40x12", "--for", "2s", "--at", "1s,2s",
            ],
        ));
        assert!(out.starts_with("--- 1000 ms\n"), "{}: {out}", dir.display());
        assert!(
            !out.contains("refused") && !out.contains("ended"),
            "{}: {out}",
            dir.display()
        );
    }
}

/// `dev run`: the app in a session, started again when a file changes.
#[test]
fn run_restarts_on_change() {
    if !has_target() {
        return;
    }
    let app = new_app("run", true);
    let sb = Sandbox::new("dev-run");
    let mut driver = sb.headless("s", "40x4", &["sleep", "120"]);
    build_runner();
    let log = sb.dir.join("run.log");
    let mut run = common::Proc(
        sb.ptyrc()
            .args(["-s", "s", "dev", "run"])
            .arg(&app)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut client = sb.client("s");
    let mut shows = |needle: &str| {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let text = client
                .call("screen.text", json!({"sprites": true}))
                .unwrap();
            if text["lines"][0].as_str().unwrap_or("").contains(needle) {
                return;
            }
            assert!(Instant::now() < deadline, "{needle:?} not shown: {text}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    shows("t = ");
    let manifest = app.join("ptyrc-app.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace("default = \"t\"", "default = \"u\""),
    )
    .unwrap();
    shows("u = ");
    let _ = run.0.kill();
    let _ = run.0.wait();
    let said = std::fs::read_to_string(&log).unwrap();
    assert!(
        said.contains("ptyrc-app.toml changed: starting again"),
        "{said}"
    );
    wait_for(
        || sb.client("s").call("draw.list", json!({})).is_ok(),
        "the session",
    );
    let _ = driver.0.kill();
}
