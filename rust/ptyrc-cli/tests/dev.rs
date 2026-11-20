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

const JS: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";

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
        skipped("no interpreter; sdk/js/interp/build.sh");
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
        let what = "no wasm32 target; rustup target add wasm32-unknown-unknown";
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {what}"
        );
        skipped(what);
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
        "PROTOCOL.md",
    ] {
        assert!(app.join(f).is_file(), "{f}");
    }
    let checked = ok(&ptyrc(&app, &["dev", "check"]));
    assert!(checked.starts_with("ok: clock 0.1.0"), "{checked}");
    assert!(
        checked.trim_end().ends_with("; asks no permission"),
        "{checked}"
    );
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
    let unit = ok(&ptyrc(&app, &["dev", "test", "--unit"]));
    assert_eq!(unit, "ok   test_sum\n1 passed, 0 failed\n");
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

/// `dev check` of a Rust app: the rule named; a vendored copy that is
/// not ptyrc's said.
#[test]
fn check_rust_rule() {
    if !has_target() {
        return;
    }
    let top = scratch("rule");
    ok(&ptyrc(&top, &["dev", "new", "clock", "--rust"]));
    let app = top.join("clock");
    let lib = app.join("vendor/ptyrc-app/src/lib.rs");
    let text = std::fs::read_to_string(&lib).unwrap();
    std::fs::write(&lib, format!("{text}\n// mine\n")).unwrap();
    let out = ptyrc(&app, &["dev", "check"]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{err}");
    assert!(
        err.contains("vendor/ptyrc-app/src/lib.rs: not ptyrc's own ptyrc-app"),
        "{err}"
    );
    std::fs::write(app.join("build.rs"), "fn main() {}\n").unwrap();
    let out = ptyrc(&app, &["dev", "check"]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("build.rs: a build script"), "{err}");
    assert!(err.contains("built by rustc alone"), "{err}");
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
        err.contains("nope: no such option") && err.contains("[options]: --label"),
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
    // The twins print the same text.
    for js in dirs.iter().filter(|d| d.starts_with(root().join("sdk/js"))) {
        let rs = root()
            .join("sdk/rust/examples")
            .join(js.file_name().unwrap());
        let read = |d: &Path| std::fs::read_to_string(d.join("expected.txt")).ok();
        if let (Some(a), Some(b)) = (read(js), read(&rs)) {
            assert_eq!(a, b, "{} and its twin", js.display());
        }
    }
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
        if dir.join("tests.js").is_file() {
            let unit = ok(&ptyrc(&dir, &["dev", "test", "--unit"]));
            assert!(unit.trim_end().ends_with(" passed, 0 failed"), "{unit}");
        }
        // A method headless does not serve yet is the bench's gap.
        let refused = (out.lines()).any(|l| l.contains("refused") && !l.ends_with("(headless)"));
        assert!(
            !refused && !out.contains("ended"),
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
    // SIGTERM: the runner stopped too, what it drew removed.
    let pid = rustix::process::Pid::from_raw(run.0.id() as i32).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    let status = run.0.wait().unwrap();
    assert!(status.success(), "{status}");
    wait_for(
        || {
            let list = sb.client("s").call("draw.list", json!({})).unwrap();
            list["sprites"].as_array().is_some_and(|s| s.is_empty())
        },
        "the app's sprites removed",
    );
    let said = std::fs::read_to_string(&log).unwrap();
    assert!(
        said.contains("ptyrc-app.toml changed: starting again") && said.contains("ptyrc: stopped"),
        "{said}"
    );
    wait_for(
        || sb.client("s").call("draw.list", json!({})).is_ok(),
        "the session",
    );
    let _ = driver.0.kill();
}

/// A script app over a scratch directory.
fn script_app(tag: &str, permissions: &str, js: &str) -> PathBuf {
    let dir = scratch(tag);
    let manifest = format!(
        "[app]\nid = \"probe\"\nversion = \"0.1.0\"\nsummary = \"A probe.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
         [options]\nwide = {{ type = \"bool\", default = false, doc = \"wide\" }}\n\
         n = {{ type = \"int\", default = 1, doc = \"a number\" }}\n{permissions}"
    );
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    std::fs::write(dir.join("app.js"), js).unwrap();
    dir
}

const PROBE: &str = r#"
import { call, Canvas, style } from "ptyrc";
const S = style({});
let c;
const lines = [];
function show(t) {
  lines.push(t);
  c.clear();
  lines.forEach((l, i) => c.text(0, i, l, S));
  c.present({ anchor: { at: [0, 0], space: "view" } });
}
export function init(args) {
  c = new Canvas(30, 6);
  show(`args ${args.wide} ${args.n}`);
  show(`text ${call("screen.text").degree ?? "whole"}`);
  call("subscribe", { topics: ["size", "input"] });
  try { call("input.grab", { keys: ["up"] }); show("grab"); }
  catch (e) { show(`grab ${e.code}`); }
}
export function event(topic, data) { show(`${topic} ${data.key ?? data.size}`); }
"#;

/// The app holds what it declares; `--deny` answers a no; keys and
/// events at a session time; `--json`.
#[test]
fn grants_keys_events() {
    if !has_js() {
        return;
    }
    let perms = "[permissions]\nshape = \"walls\"\nkeys-arrows = \"steer\"\n";
    let app = script_app("grants", perms, PROBE);
    let base = ["dev", "test", "--size", "30x6", "--for", "300ms"];
    let run = |more: &[&str]| ok(&ptyrc(&app, &[&base[..], more].concat()));
    let out = run(&[
        "--wide",
        "--n",
        "3",
        "--key",
        "200:up",
        "--event",
        "100:size:{\"size\":[9,9]}",
    ]);
    assert_eq!(
        out,
        "--- 300 ms\nargs true 3\ntext shape\ngrab\nsize 9,9\ninput up\n\
         lowered at 0 ms: screen.text, to the degree shape\n\
         lowered: text, not declared: fine if intended; else the manifest would ask for \
         it: [permissions] text = \"...\"\n\
         lowered: keys-read, not declared: fine if intended; else the manifest would ask \
         for it: [permissions] keys-read = \"...\"\n\
         note: declaring less is good: the user is asked less, and a lowered answer is \
         an answer\n"
    );
    // The old forms, and `--` before the options.
    let old = run(&["--arg", "n=3", "--", "--wide"]);
    assert!(old.starts_with("--- 300 ms\nargs true 3\n"), "{old}");
    // A no, as JSON.
    let out = run(&["--json", "--deny", "shape,keys-arrows", "--key", "200:up"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let rows = &v["screens"][0]["rows"];
    assert_eq!(
        rows,
        &json!(["args false 1", "text none", "grab denied"]),
        "{v}"
    );
    assert_eq!(
        v["refused"],
        json!([{"t": 0, "method": "input.grab", "code": "denied"}])
    );
    assert_eq!(v["lowered"][0]["degree"], "none");
    assert_eq!(
        v["dropped"][0]["what"],
        "key up: to the program, not grabbed"
    );
    let lacks: Vec<&str> = (v["lacks"].as_array().unwrap().iter())
        .map(|l| l["line"].as_str().unwrap())
        .collect();
    for line in [
        "lowered: shape, declared and answered no",
        "refused: keys-arrows, declared and answered no",
    ] {
        assert!(lacks.contains(&line), "{line}: {v}");
    }
    assert!(v["end"].is_null() && v["limits"] == json!([]));
    let out = ptyrc(&app, &["dev", "test", "--deny", "typing"]);
    assert_eq!(out.status.code(), Some(2));
    let out = ptyrc(&app, &["dev", "test", "--key", "5:nokey"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a key"));
    // Nothing declared: nothing held, and `check` says so.
    let app = script_app("none", "", PROBE);
    std::fs::copy(root().join("LICENSE.txt"), app.join("LICENSE.txt")).unwrap();
    let out = ok(&ptyrc(&app, &["dev", "test", "--for", "0"]));
    assert!(out.contains("text none\ngrab denied\n"), "{out}");
    assert!(out.contains("refused: keys-arrows, not declared"), "{out}");
    let said = ok(&ptyrc(&app, &["dev", "check"]));
    assert!(said.contains("asks no permission"), "{said}");
}

/// `dev check`: a limit past the runner's ceiling, named.
#[test]
fn check_names_the_ceiling() {
    let js = "export function init(args) {}\n";
    let dir = script_app("ceiling", "[limits]\nmemory-mib = 4096\n", js);
    let out = ptyrc(&dir, &["dev", "check"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert!(
        err.contains("memory-mib: 4096 above the ceiling 512"),
        "{err}"
    );
    assert!(err.contains("memory-mib = 512 at most"), "{err}");
}
