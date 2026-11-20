// SPDX-License-Identifier: GPL-3.0-or-later
//! ABI 1 against small guests, headless.

mod common;

/// Every test under the OS profile (`common::confine`).
const CONFINE: bool = true;

use common::{EVENT, INIT, TICK, guest};
use ptyrc_box::Error;
use ptyrc_box::headless::{Options, Tape, Under, run};
use serde_json::json;

/// A canvas of 5x1 at (2, 1): `hi`, then the count of ticks in place
/// of `h`. Events are logged by topic.
fn counter() -> Vec<u8> {
    guest(
        r#"
  (data (i32.const 16) "{\"anchor\":{\"at\":[2,1]}}")
  (global $id (mut i32) (i32.const 0))
  (global $n (mut i32) (i32.const 0))
  (func $show (drop (call $present (global.get $id) (i32.const 16) (i32.const 23))))
  (func (export "init") (param i32 i32)
    (call $period (i32.const 100))
    (global.set $id (call $canvas (i32.const 5) (i32.const 1)))
    (i32.store (call $cells (global.get $id)) (i32.const 0x68))
    (i32.store (i32.add (call $cells (global.get $id)) (i32.const 16)) (i32.const 0x69))
    (call $show))
  (func (export "tick") (param i64)
    (global.set $n (i32.add (global.get $n) (i32.const 1)))
    (i32.store (call $cells (global.get $id)) (i32.add (i32.const 0x30) (global.get $n)))
    (call $show)
    ;; Presented twice, sent once.
    (call $show))
  (func (export "event") (param i32 i32 i32 i32)
    (call $log (local.get 0) (local.get 1))
    (call $log (local.get 2) (local.get 3)))
"#,
    )
}

#[test]
fn canvas_ticks_events() {
    let options = Options {
        duration_ms: 350,
        at: vec![0, 100, 350],
        events: vec![(150, "size".into(), json!({"size": [80, 24]}))],
        ..Options::default()
    };
    let report = run(&counter(), &options).unwrap();
    assert_eq!(report.end, None);
    assert_eq!(report.ticks, 3);
    let row = |i: usize| report.screens[i].1[1].as_str();
    assert_eq!((row(0), row(1), row(2)), ("  hi", "  1i", "  3i"));
    let methods: Vec<(u64, &str)> = report
        .requests
        .iter()
        .map(|r| (r.t, r.method.as_str()))
        .collect();
    assert_eq!(
        methods,
        [
            (0, "draw.put"),
            (100, "draw.update"),
            (200, "draw.update"),
            (300, "draw.update")
        ]
    );
    assert_eq!(report.requests[0].params, json!({"anchor": {"at": [2, 1]}}));
    // The place did not move: the update bears the content alone.
    assert_eq!(report.requests[1].params, json!({"id": 1}));
    assert_eq!(report.log, ["   150 size", "   150 {\"size\":[80,24]}"]);
    assert!(report.met.is_empty());
}

#[test]
fn the_same_run_twice() {
    let options = Options {
        duration_ms: 1000,
        at: vec![0, 500, 1000],
        ..Options::default()
    };
    let a = run(&counter(), &options).unwrap();
    let b = run(&counter(), &options).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.to_string(), b.to_string());
    assert!(
        a.to_string()
            .starts_with("--- 0 ms\n\n  hi\n--- 500 ms\n\n  5i\n")
    );
}

/// `call` answers in memory the guest allocated: `(ptr << 32) | len`.
/// The wall time of the ticks: counted, in `--json`, out of the text.
#[test]
fn the_ticks_timed() {
    let options = Options {
        duration_ms: 1000,
        ..Options::default()
    };
    let r = run(&counter(), &options).unwrap();
    assert_eq!((r.ticks, r.timing.ticks), (10, 10));
    let mean = r.timing.mean().unwrap();
    assert!(mean <= r.timing.max && r.timing.max <= r.timing.total);
    assert!(!r.to_string().contains("ticks"));
    let line = r.timing_line();
    assert!(
        line.starts_with("ticks: 10; a tick, wall time: mean "),
        "{line}"
    );
    let j = r.to_json();
    assert_eq!(j["ticks"], 10);
    assert!(j["tick_ms"]["mean"].as_f64().unwrap() <= j["tick_ms"]["max"].as_f64().unwrap());
    // No tick: none timed.
    let quiet = run(&guest(&format!("{INIT}{TICK}{EVENT}")), &options).unwrap();
    assert_eq!(quiet.timing_line(), "ticks: 0");
    assert!(quiet.to_json()["tick_ms"].is_null());
}

#[test]
fn call_answers() {
    let wasm = guest(&format!(
        r#"
  (data (i32.const 16) "session.info")
  (data (i32.const 32) "input.send")
  (data (i32.const 48) "{{\"text\":\"x\"}}")
  (func $say (param $r i64)
    (call $log (i32.wrap_i64 (i64.shr_u (local.get $r) (i64.const 32)))
               (i32.wrap_i64 (local.get $r))))
  (func (export "init") (param i32 i32)
    (call $log (local.get 0) (local.get 1))
    (call $say (call $call (i32.const 16) (i32.const 12) (i32.const 0) (i32.const 0)))
    (call $say (call $call (i32.const 32) (i32.const 10) (i32.const 48) (i32.const 12))))
  {TICK} {EVENT}
"#
    ));
    let options = Options {
        args: json!({"rate": 20}),
        size: (40, 10),
        ..Options::default()
    };
    let report = run(&wasm, &options).unwrap();
    assert_eq!(report.end, None);
    assert_eq!(
        report.log,
        [
            r#"     0 {"rate":20}"#,
            r#"     0 {"result":{"headless":true,"protocol":1,"server":"ptyrc-box","size":[40,10]}}"#,
            r#"     0 {"error":{"code":"headless","hint":"a method of the driver, answered in a session (ptyrc dev run), not in a test: PROTOCOL.md, Headless","message":"input.send: not answered without a session"}}"#,
        ]
    );
    assert_eq!(report.requests[1].params, json!({"text": "x"}));
    assert_eq!(report.requests[1].refused.as_deref(), Some("headless"));
    assert!(
        report
            .to_string()
            .contains("refused at 0 ms: input.send (headless)")
    );
}

#[test]
fn exit_ends_the_run() {
    let wasm = guest(&format!(
        r#"
  (func (export "init") (param i32 i32) (call $period (i32.const 50)))
  (func (export "tick") (param $now i64)
    (if (i64.ge_u (local.get $now) (i64.const 100)) (then (call $exit (i32.const 3)))))
  {EVENT}
"#
    ));
    let report = run(&wasm, &Options::default()).unwrap();
    assert_eq!(report.end, Some(Error::Exit(3)));
    assert_eq!(report.ticks, 2);
}

/// `period` under 33 ms is 33 ms; 0 stops the ticks.
#[test]
fn period_bounds() {
    let wasm = guest(&format!(
        r#"
  (func (export "init") (param i32 i32) (call $period (i32.const 1)))
  (func (export "tick") (param $now i64)
    (if (i64.ge_u (local.get $now) (i64.const 99)) (then (call $period (i32.const 0)))))
  {EVENT}
"#
    ));
    let report = run(&wasm, &Options::default()).unwrap();
    assert_eq!(report.ticks, 3);
}

/// The seed gives the bytes; a released canvas leaves the screen.
#[test]
fn random_and_release() {
    let wasm = guest(&format!(
        r#"
  (global $id (mut i32) (i32.const 0))
  (func (export "init") (param i32 i32)
    (call $random (i32.const 64) (i32.const 4))
    (global.set $id (call $canvas (i32.const 1) (i32.const 1)))
    (i32.store (call $cells (global.get $id))
      (i32.add (i32.const 0x41) (i32.rem_u (i32.load8_u (i32.const 64)) (i32.const 26))))
    (drop (call $present (global.get $id) (i32.const 0) (i32.const 0))))
  {TICK}
  (func (export "event") (param i32 i32 i32 i32) (call $release (global.get $id)))
"#
    ));
    let mut options = Options {
        duration_ms: 20,
        at: vec![0, 20],
        events: vec![(10, "x".into(), json!(null))],
        ..Options::default()
    };
    let a = run(&wasm, &options).unwrap();
    assert_eq!(a.screens[0].1[0].len(), 1);
    assert_eq!(a.screens[1].1[0], "");
    assert_eq!(a.requests.last().unwrap().method, "draw.remove");
    assert_eq!(a, run(&wasm, &options).unwrap());
    let letters: std::collections::BTreeSet<String> = (1..20)
        .map(|seed| {
            options.seed = seed * 7919;
            run(&wasm, &options).unwrap().screens[0].1[0].clone()
        })
        .collect();
    assert!(letters.len() > 3);
}

#[test]
fn missing_exports() {
    let no_tick = guest(&format!("{INIT} {EVENT}"));
    assert!(matches!(
        run(&no_tick, &Options::default()),
        Err(Error::Export(e)) if e.starts_with("tick")
    ));
    let wrong = guest(&format!(
        r#"{INIT} {EVENT} (func (export "tick") (param i32))"#
    ));
    assert!(matches!(
        run(&wrong, &Options::default()),
        Err(Error::Export(_))
    ));
}

/// `init` sends its argument as `draw.put` and subscribes to `size`;
/// events are logged, topic then data.
fn putter() -> Vec<u8> {
    guest(&format!(
        r#"
  (data (i32.const 16) "draw.put")
  (data (i32.const 32) "subscribe")
  (data (i32.const 48) "{{\"topics\":[\"size\"]}}")
  (func (export "init") (param i32 i32)
    (drop (call $call (i32.const 16) (i32.const 8) (local.get 0) (local.get 1)))
    (drop (call $call (i32.const 32) (i32.const 9) (i32.const 48) (i32.const 19))))
  (func (export "event") (param i32 i32 i32 i32)
    (call $log (local.get 0) (local.get 1))
    (call $log (local.get 2) (local.get 3)))
  {TICK}
"#
    ))
}

/// A text sprite on a screen given as text: behind the text, the
/// default layer.
#[test]
fn text_sprite_over_a_screen() {
    let options = Options {
        size: (20, 4),
        under: Under::Text("hello world\nline two\n".into()),
        args: json!({"anchor": {"at": [6, 1]}, "content": {"rows": ["TWO"]}}),
        at: vec![0],
        ..Options::default()
    };
    let report = run(&putter(), &options).unwrap();
    assert_eq!(report.end, None, "{report}");
    assert_eq!(report.screens[0].1, ["hello world", "line twoO", "", ""]);
    assert_eq!(report.requests[0].params, json!({"anchor": {"at": [6, 1]}}));
}

/// A tape is played in session time; a resize is a `size` event.
#[test]
fn over_a_tape() {
    let tape = Tape::parse(concat!(
        r#"{"ptyrc":"tape","version":2,"size":[20,3]}"#,
        "\n",
        r#"{"t":0,"o":"abc"}"#,
        "\n",
        r#"{"t":300000,"ob":"DQp4eXo="}"#,
        "\n",
        r#"{"t":400000,"size":[30,5]}"#,
        "\n",
    ))
    .unwrap();
    let options = Options {
        under: Under::Tape(tape),
        args: json!({"anchor": {"at": [-1, -1], "space": "view"}, "content": {"rows": ["#"]}}),
        at: vec![0, 350, 500],
        duration_ms: 500,
        ..Options::default()
    };
    let report = run(&putter(), &options).unwrap();
    assert_eq!(report.end, None, "{report}");
    let screen = |i: usize| report.screens[i].1.clone();
    assert_eq!(screen(0), ["abc", "", "                   #"]);
    assert_eq!(screen(1), ["abc", "xyz", "                   #"]);
    assert_eq!(screen(2)[4], format!("{:>30}", "#"));
    assert_eq!(report.log, ["   400 size", r#"   400 {"size":[30,5]}"#]);
    assert_eq!(
        report.to_string(),
        run(&putter(), &options).unwrap().to_string()
    );
}
