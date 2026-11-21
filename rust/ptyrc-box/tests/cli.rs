// SPDX-License-Identifier: GPL-3.0-or-later
//! The program `ptyrc-box`, headless: what its command line tells the
//! guest.

mod common;

/// Not under the OS profile: the test runs the program.
const CONFINE: bool = false;

use common::{EVENT, TICK, guest};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

/// A guest that logs the argument of `init`.
fn echo() -> PathBuf {
    let wasm = guest(&format!(
        r#"
  (func (export "init") (param i32 i32) (call $log (local.get 0) (local.get 1)))
  {TICK} {EVENT}
"#
    ));
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("box-cli-echo.wasm");
    std::fs::write(&path, wasm).unwrap();
    path
}

/// The argument of `init` of a headless run given `manifest`.
fn told(manifest: &Value, grant: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ptyrc-box"))
        .args(["--headless", "--no-profile", "--for", "0", "--grant", grant])
        .arg("--manifest")
        .arg(manifest.to_string())
        .arg(echo())
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{log}");
    let line = log
        .lines()
        .find(|l| l.contains("ptyrc:manifest"))
        .expect("logged");
    let json = &line[line.find('{').unwrap()..];
    serde_json::from_str(json).unwrap()
}

/// Headless, the frame is the run's one frame, raw: whatever
/// `--manifest` says, 0.
#[test]
fn headless_tells_the_raw_frame() {
    let m = json!({"id": "a", "version": "1", "frame": 4});
    let args = told(&m, "layout");
    assert_eq!(args["ptyrc:manifest"]["frame"], json!(0), "{args}");
    let args = told(&json!({"id": "a", "version": "1"}), "shape");
    assert_eq!(args["ptyrc:manifest"]["frame"], json!(0), "{args}");
}

/// `--timing`: a line for each call over `tick-ms` (50 ms), status 1;
/// without it, the report and the status as ever.
#[test]
fn timing_fails_a_slow_tick() {
    let wasm = guest(&format!(
        r#"
  (func (export "init") (param i32 i32) (call $period (i32.const 100)))
  (func (export "tick") (param i64) (local $i i32)
    (loop $l
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 800000000)))))
  {EVENT}
"#
    ));
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("box-cli-slow.wasm");
    std::fs::write(&path, wasm).unwrap();
    let box_ = |flags: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ptyrc-box"))
            .args(["--headless", "--no-profile", "--for", "100"])
            .args(flags)
            .arg(&path)
            .output()
            .unwrap()
    };
    let out = box_(&["--timing"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len();
    assert!(lines[n - 2].starts_with("slow at 100 ms: tick, "), "{text}");
    assert!(lines[n - 1].starts_with("ticks: 1; "), "{text}");
    let plain = box_(&[]);
    assert!(plain.status.success());
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("slow"));
    let json = box_(&["--json", "--timing"]);
    assert_eq!(json.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["slow"][0]["call"], "tick");
    assert_eq!(v["slow"][0]["t"], 100);
}
