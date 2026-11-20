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
        "tests/clock.args",
        "tests/clock.txt",
        "tests/label.args",
        "tests/label.txt",
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
    let said = ok(&sh(&app, "test.sh"));
    assert!(
        said.contains("ok   tests/clock\nok   tests/label\n"),
        "{said}"
    );
    // A report that differs, a case that fails: status 1, each named,
    // the difference printed, nothing left in the tree.
    std::fs::write(app.join("tests/label.txt"), "--- 0 ms\n").unwrap();
    std::fs::write(app.join("tests/bad.args"), "--size nine\n").unwrap();
    std::fs::write(app.join("tests/bad.txt"), "").unwrap();
    let out = sh(&app, "test.sh");
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("FAIL tests/bad: ptyrc dev test ended with status 2\n"),
        "{said}"
    );
    assert!(
        said.contains("FAIL tests/label: the report differs from tests/label.txt"),
        "{said}"
    );
    assert!(said.lines().any(|l| l.starts_with('+')), "{said}");
    assert!(said.contains("ok   tests/clock\n"), "{said}");
    let left: Vec<_> = std::fs::read_dir(app.join("tests"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.ends_with(".args") && !n.ends_with(".txt"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    for f in ["tests/bad.args", "tests/bad.txt"] {
        std::fs::remove_file(app.join(f)).unwrap();
    }
    let update = Command::new(app.join("test.sh"))
        .arg("--update")
        .current_dir(&app)
        .env("PTYRC", Path::new(BIN))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(update.status.success());
    ok(&sh(&app, "test.sh"));
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
    let expected = std::fs::read_to_string(app.join("tests/clock.txt")).unwrap();
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
    // The twins have the same cases: `tests/` the same files, byte for
    // byte (each runs its own by its `test.sh`).
    let cases = |d: &Path| -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(d.join("tests"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .map(|p| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read(&p).unwrap())
            })
            .collect();
        out.sort();
        out
    };
    let mut twins = 0;
    for js in dirs.iter().filter(|d| d.starts_with(root().join("sdk/js"))) {
        let rs = root()
            .join("sdk/rust/examples")
            .join(js.file_name().unwrap());
        if !rs.join("ptyrc-app.toml").is_file() {
            continue;
        }
        let (a, b) = (cases(js), cases(&rs));
        let names = |c: &[(String, Vec<u8>)]| c.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&a),
            names(&b),
            "{}: tests/ and its twin's",
            js.display()
        );
        for ((name, x), (_, y)) in a.iter().zip(&b) {
            assert!(x == y, "{}: tests/{name} and its twin's", js.display());
        }
        twins += usize::from(!a.is_empty());
    }
    assert!(twins >= 4, "{twins} twins with cases");
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

/// `dev run` tells the frame a start draws on, as the driver does:
/// `active` by default, its id with `layout`.
#[test]
fn run_tells_the_frame() {
    if !has_js() {
        return;
    }
    let js = r#"
import { log, manifest } from "ptyrc";
export function init(args) { log("frame=" + JSON.stringify(manifest().frame)); }
export function tick(now) {}
export function event(topic, data) {}
"#;
    let app = script_app("run-frame", "[permissions]\nlayout = \"the panes\"\n", js);
    let sb = Sandbox::new("told-frame");
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
    let active = sb
        .client("s")
        .call("frame.get", json!({"frame": "active"}))
        .unwrap();
    let id = active["frame"]["id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("frame=")
    {
        let said = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(Instant::now() < deadline, "not told: {said}");
        std::thread::sleep(Duration::from_millis(100));
    }
    let said = std::fs::read_to_string(&log).unwrap();
    assert!(said.contains(&format!("frame={id}\n")), "{said}");
    let _ = run.0.kill();
    let _ = driver.0.kill();
}

/// A script app over a scratch directory.
fn script_app(tag: &str, permissions: &str, js: &str) -> PathBuf {
    let dir = scratch(tag);
    let manifest = format!(
        "[app]\nid = \"probe\"\nversion = \"0.1.0\"\nsummary = \"A probe.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
         surface = \"frame\"\n[options]\nwide = {{ type = \"bool\", default = false, doc = \"wide\" }}\n\
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

const RUN: &str = r#"
import { call, exit } from "ptyrc";
function t(m, p) { try { return call(m, p); } catch (e) { return e.code; } }
export function init(args) {
  const grab = t("input.grab", { keys: ["up"] });
  const out = [typeof grab === "string" ? grab : "grabbed"];
  if (args.save) out.push(t("file.write", { file: args.save.file, text: "x" }));
  if (args.look) out.push(t("file.write", { file: args.look.file, text: "x" }));
  for (const p of [args.name, args.todo, "hello"]) {
    if (p) out.push(t("screen.find", { pattern: p }).matches.length);
  }
  call("app.result", { value: out });
  exit(0);
}
"#;

/// `dev test` holds the app to what the run asks (`Resolved::asked`):
/// a permission an option needs, only with that option; the user's
/// word and pattern found; `--file NAME=PATH` a handle, written back
/// only when the manifest says `write = true`.
#[test]
fn the_run_asks_what_its_options_need() {
    if !has_js() {
        return;
    }
    let dir = scratch("asked");
    std::fs::write(
        dir.join("ptyrc-app.toml"),
        "[app]\nid = \"asked\"\nversion = \"0.1.0\"\nsummary = \"A probe.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
         [permissions]\nshape = \"walls\"\nkeys-arrows = \"steer\"\n\
         files = \"named\"\nfiles-write = \"kept\"\n\
         [options]\nsteer = { type = \"bool\", default = false, needs = [\"keys-arrows\"], \
         doc = \"steer\" }\n\
         save = { type = \"file\", write = true, doc = \"kept\" }\n\
         look = { type = \"file\", doc = \"read\" }\n\
         name = { type = \"word\", doc = \"a word\" }\n\
         todo = { type = \"pattern\", doc = \"a mark\" }\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.js"), RUN).unwrap();
    std::fs::write(dir.join("screen.txt"), "hello Rose TODO:\n").unwrap();
    std::fs::write(dir.join("a.txt"), "a").unwrap();
    let run = |more: &[&str]| {
        let base = [
            "dev",
            "test",
            "--json",
            "--for",
            "0",
            "--screen",
            "screen.txt",
        ];
        let out = ok(&ptyrc(&dir, &[&base[..], more].concat()));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        v["result"]["value"].clone()
    };
    assert_eq!(run(&[]), json!(["denied", 0]));
    assert_eq!(run(&["--steer"]), json!(["grabbed", 0]));
    assert_eq!(
        run(&["--name", "rose", "--todo", "TODO:"]),
        json!(["denied", 1, 1, 0])
    );
    assert_eq!(
        run(&["--file", "save=a.txt", "--file", "look=a.txt"]),
        json!(["denied", {"size": 1, "mtime_ms": 946_684_800_000_u64}, "denied", 0])
    );
    let out = ptyrc(&dir, &["dev", "test", "--file", "nope=a.txt"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope: not a file or directory option"));
}

/// An option of the app named as a flag of `dev test`: the flag is
/// `dev test`'s before `--`, the app's after; `check` and `test` say so.
#[test]
fn an_option_named_as_a_flag_of_dev_test() {
    let js =
        r#"import { log } from "ptyrc"; export function init(args) { log(`size ${args.size}`); }"#;
    let more = "size = { type = \"int\", default = 0, doc = \"the app's\" }\n";
    let dir = script_app("shadow", more, js);
    std::fs::copy(root().join("LICENSE.txt"), dir.join("LICENSE.txt")).unwrap();
    // `check` runs no `dev test`: nothing to say.
    let out = ptyrc(&dir, &["dev", "check"]);
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!said.contains("--size"), "{said}");
    let out = ptyrc(
        &dir,
        &[
            "dev", "test", "--for", "0", "--size", "40x10", "--", "--size", "7",
        ],
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("note: --size 40x10: taken by dev test, not the app")
            && said.contains("size 7"),
        "{said}"
    );
    // The app's alone, after `--`: nothing to say.
    let out = ptyrc(&dir, &["dev", "test", "--for", "0", "--", "--size", "7"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!said.contains("note") && said.contains("size 7"), "{said}");
}

/// `dev check`: each draw of `[app] random` read as `ptyrc app` reads it.
#[test]
fn check_reads_the_random_draws() {
    let js = "export function init(args) {}\n";
    let dir = script_app("random", "", js);
    std::fs::copy(root().join("LICENSE.txt"), dir.join("LICENSE.txt")).unwrap();
    let toml = std::fs::read_to_string(dir.join("ptyrc-app.toml")).unwrap();
    let with = |draws: &str| {
        let t = toml.replace(
            "script = \"app.js\"\n",
            &format!("script = \"app.js\"\nrandom = {draws}\n"),
        );
        std::fs::write(dir.join("ptyrc-app.toml"), t).unwrap();
        ptyrc(&dir, &["dev", "check"])
    };
    assert!(with("[[], [\"--wide\", \"--n\", \"3\"]]").status.success());
    let out = with("[[\"--n\", \"x\"], [\"--slow\"]]");
    assert_eq!(out.status.code(), Some(1));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("[app] random: [\"--slow\"]") && said.contains("2 problems"),
        "{said}"
    );
    assert!(said.contains("random = [] never drawn"), "{said}");
    // A positional required, `random` absent: the bare draw never
    // drawn, a note; `random = []` says it, silent.
    let args = format!(
        "{toml}[permissions]\nfiles = \"the one named\"\n\
         [args]\nfile = {{ type = \"file\", min = 1, max = 1, doc = \"a file\" }}\n"
    );
    std::fs::write(dir.join("ptyrc-app.toml"), &args).unwrap();
    let out = ptyrc(&dir, &["dev", "check"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(
            "note: [app] random: the bare draw (random absent: [[]]) is not a command line"
        ) && said.contains("random = [] says so"),
        "{said}"
    );
    let never = args.replace(
        "script = \"app.js\"\n",
        "script = \"app.js\"\nrandom = []\n",
    );
    std::fs::write(dir.join("ptyrc-app.toml"), never).unwrap();
    let out = ptyrc(&dir, &["dev", "check"]);
    assert!(out.status.success(), "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("random"),
        "{out:?}"
    );
}

/// `--zone` with the clock held, and with it denied: accepted both
/// times; denied, the zone unused, said.
#[test]
fn a_zone_with_the_clock_denied() {
    if !has_js() {
        return;
    }
    let js = r#"import { now, result } from "ptyrc";
export function init() { const w = now(); result(`${w.zone} ${w.offsetS}`, 0); }
"#;
    let dir = script_app("zone", "[permissions]\nclock = \"the time\"\n", js);
    let run = |zone: &str, more: &[&str]| {
        let base = ["dev", "test", "--for", "0", "--zone", zone];
        ptyrc(&dir, &[&base[..], more].concat())
    };
    let held = ok(&run("Asia/Tokyo", &[]));
    assert!(
        held.contains("result: Asia/Tokyo 32400 (status 0)"),
        "{held}"
    );
    let out = run("Asia/Tokyo", &["--deny", "clock"]);
    let said = ok(&out);
    assert!(said.contains("result: UTC 0 (status 0)"), "{said}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--zone Asia/Tokyo: clock not held"), "{err}");
    let out = run("Nowhere/City", &["--deny", "clock"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

/// Every usage error of `dev test`: a status not 0, the message on
/// stderr, nothing on stdout; the app's options listed with their
/// bounds, the flag named once.
#[test]
fn usage_errors_on_stderr_alone() {
    let more = "m = { type = \"int\", default = 2, min = 1, max = 10, doc = \"many\" }\n";
    let dir = script_app("usage", more, "export function init() {}\n");
    let mut cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["--size", "9x9", "--size", "8x8"], "--size"),
        (
            vec!["--zone", "UTC", "--wide", "--zone", "UTC"],
            "--zone: given twice",
        ),
        (vec!["--size", "nine"], "COLSxROWS"),
        (vec!["--m", "11"], "--m: 11: 1 to 10\n"),
        (vec!["--", "--help"], "--m (int, 1 to 10): many"),
        (vec!["--nope"], "--nope: no such option"),
        (vec!["--at", "x"], "--at x"),
        (vec!["--key", "x"], "--key x"),
        (vec!["--file", "x=y"], "not a file or directory option"),
        (vec!["--deny", "typing"], "not a permission"),
        (vec!["--storage", "/nonexistent-dir"], "--storage"),
    ];
    if has_js() {
        cases.push((vec!["--zone", "Nowhere/City"], "not a zone"));
    }
    for (args, said) in cases {
        let out = ptyrc(&dir, &[&["dev", "test", "--for", "0"][..], &args].concat());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {out:?}");
        assert!(out.stdout.is_empty(), "{args:?}: {out:?}");
        assert!(err.contains(said), "{args:?}: {err}");
        assert!(!err.contains("--m: m:"), "{err}");
    }
}
