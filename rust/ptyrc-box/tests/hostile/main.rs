// SPDX-License-Identifier: GPL-3.0-or-later
//! Hostile guests. Each ends contained, with a named error or a named
//! limit; the runner goes on. A hole found later becomes a guest here.
//!
//! The guests are the `.wat` files beside this one: fragments, put
//! after the prelude of `tests/common` (`guest`) or alone (`bare`).

#[path = "../common/mod.rs"]
mod common;

/// Every guest under the OS profile (`common::confine`).
const CONFINE: bool = true;

use common::{bare, guest};
use ptyrc_box::headless::{Options, Report, run};
use ptyrc_box::{Error, Limits};
use std::time::{Duration, Instant};

/// Bounds of time short enough for a test.
fn quick() -> Options {
    Options {
        limits: Limits {
            hard: Duration::from_millis(300),
            init: Duration::from_millis(300),
            ..Limits::default()
        },
        ..Options::default()
    }
}

fn met(report: &Report, limit: &str) -> bool {
    report.met.iter().any(|m| m == limit)
}

#[test]
fn loop_in_tick() {
    let start = Instant::now();
    let report = run(&guest(include_str!("loop_tick.wat")), &quick()).unwrap();
    assert_eq!(report.end, Some(Error::Time("tick")));
    assert_eq!(report.end.unwrap().name(), "time");
    assert_eq!(report.ticks, 1);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn loop_in_init() {
    let report = run(&guest(include_str!("loop_init.wat")), &quick()).unwrap();
    assert_eq!(report.end, Some(Error::Time("init")));
    assert_eq!(report.ticks, 0);
}

#[test]
fn loop_in_start() {
    let wasm = guest(include_str!("loop_start.wat"));
    assert_eq!(run(&wasm, &quick()).err(), Some(Error::Time("start")));
}

/// Memory grown without end: the growth is refused at the limit, the
/// guest goes on with what it has.
#[test]
fn memory_grown_without_end() {
    let report = run(&guest(include_str!("memory_grow.wat")), &quick()).unwrap();
    assert_eq!(report.end, None);
    assert!(met(&report, "memory"));
    // Pages held when the growth was refused: 64 MiB.
    assert_eq!(report.log, ["   100 limit: memory", "   100 pages 1024"]);
}

#[test]
fn memory_too_large_at_the_start() {
    let wasm = bare(include_str!("memory_start.wat"));
    let err = run(&wasm, &quick()).err().unwrap();
    assert_eq!((err.name(), err), ("memory", Error::Memory));
}

#[test]
fn import_outside_ptyrc() {
    let err = run(&bare(include_str!("import_env.wat")), &quick()).err();
    assert_eq!(err, Some(Error::Import("env.open".into())));
    assert_eq!(err.unwrap().name(), "bad-import");
}

#[test]
fn import_of_wasi() {
    let err = run(&bare(include_str!("import_wasi.wat")), &quick()).err();
    assert_eq!(
        err,
        Some(Error::Import("wasi_snapshot_preview1.fd_write".into()))
    );
}

/// A name `ptyrc` does not have; a name it has, of another type; a
/// memory in place of a function.
#[test]
fn import_ptyrc_does_not_have() {
    let err = run(&bare(include_str!("import_unknown.wat")), &quick()).err();
    assert_eq!(err, Some(Error::Import("ptyrc.open".into())));
    let err = run(&bare(include_str!("import_type.wat")), &quick()).err();
    assert!(matches!(err, Some(Error::Import(_))), "{err:?}");
    let err = run(&bare(include_str!("import_memory.wat")), &quick()).err();
    assert_eq!(err, Some(Error::Import("ptyrc.log".into())));
}

/// A flood of `call`: 1000 reach the host, the rest are answered
/// `limit`; the loop then meets the bound of time.
#[test]
fn flood_of_call() {
    let report = run(&guest(include_str!("call_flood.wat")), &quick()).unwrap();
    assert!(met(&report, "call"));
    assert_eq!(report.requests.len(), 1000);
    assert_eq!(report.end, Some(Error::Time("tick")));
}

/// A flood that returns: the next second has its 1000 again.
#[test]
fn call_rate_is_per_second() {
    let options = Options {
        duration_ms: 1500,
        ..quick()
    };
    let report = run(&guest(include_str!("call_burst.wat")), &options).unwrap();
    assert_eq!(report.end, None);
    assert!(met(&report, "call"));
    // 1500 at 500 ms, 1500 at 1000 ms (still that second), 1500 at 1500 ms.
    assert_eq!(report.requests.len(), 2000);
}

#[test]
fn canvas_of_two_to_the_32_cells() {
    let report = run(&guest(include_str!("canvas_huge.wat")), &quick()).unwrap();
    assert_eq!(report.end, None);
    assert!(met(&report, "canvas"));
    // 80x24, four screens: 7680 cells. 96x80 is taken, one cell more is not.
    assert_eq!(report.log.last().unwrap(), "     0 ids 0 0 0 1 0");
    assert!(report.requests.is_empty());
}

#[test]
fn pointer_out_of_memory() {
    for (file, what) in [
        (include_str!("pointer_call.wat"), "call: method"),
        (include_str!("pointer_params.wat"), "call: params"),
        (include_str!("pointer_log.wat"), "log"),
        (include_str!("pointer_place.wat"), "present: place"),
        (include_str!("pointer_random.wat"), "random"),
    ] {
        let report = run(&guest(file), &quick()).unwrap();
        match report.end {
            Some(Error::Pointer(m)) => assert!(m.starts_with(what), "{m}"),
            other => panic!("{what}: {other:?}"),
        }
        assert!(report.requests.is_empty());
    }
}

/// An `alloc` that answers a pointer outside the memory.
#[test]
fn alloc_that_lies() {
    let report = run(&bare(include_str!("alloc_lies.wat")), &quick()).unwrap();
    assert!(matches!(report.end, Some(Error::Pointer(_))));
}

#[test]
fn recursion_to_the_end_of_the_stack() {
    let report = run(&guest(include_str!("recursion.wat")), &quick()).unwrap();
    assert_eq!(report.end, Some(Error::Stack));
    assert_eq!(report.end.unwrap().name(), "stack");
}

/// The host calls `alloc` from inside `call`; an `alloc` that calls
/// `call` again recurses through the host.
#[test]
fn recursion_through_the_host() {
    let report = run(&bare(include_str!("recursion_host.wat")), &quick()).unwrap();
    let end = report.end.expect("ended");
    assert!(matches!(end, Error::Stack | Error::Time(_)), "{end:?}");
}

#[test]
fn trap() {
    let report = run(&guest(include_str!("unreachable.wat")), &quick()).unwrap();
    assert!(matches!(report.end, Some(Error::Trap(_))));
}

/// More than 1 MiB of log: the rest is dropped.
#[test]
fn flood_of_log() {
    let report = run(&guest(include_str!("log_flood.wat")), &quick()).unwrap();
    assert_eq!(report.end, None);
    assert!(met(&report, "log"));
    let bytes: usize = report.log.iter().map(String::len).sum();
    assert!(bytes < (1 << 20) + (1 << 16), "{bytes}");
}

/// Escape bytes and control characters, in a cell and in the log:
/// spaces.
#[test]
fn escapes_in_text() {
    let report = run(&guest(include_str!("escapes.wat")), &quick()).unwrap();
    assert_eq!(report.end, None);
    assert_eq!(report.screens[0].1[0], "a   b");
    assert_eq!(report.log, ["     0 a ]52;c b"]);
    let all = format!("{report}{:?}", report.requests);
    assert!(!all.chars().any(|c| c.is_control() && c != '\n'));
}

/// A host of no session, for the runner alone.
struct Bare;

impl ptyrc_box::Host for Bare {
    fn call(
        &mut self,
        m: &str,
        _: serde_json::Value,
    ) -> Result<serde_json::Value, ptyrc_box::CallError> {
        Err(ptyrc_box::CallError::new("not-usable", m))
    }
    fn size(&self) -> (u32, u32) {
        (80, 24)
    }
    fn log(&mut self, _: &str) {}
    fn random(&mut self, buf: &mut [u8]) {
        buf.fill(0);
    }
}

fn spin_limits() -> Limits {
    Limits {
        late: Duration::from_millis(5),
        ..Limits::default()
    }
}

/// Live: three late in a row, paused; no tick entered after.
#[test]
fn late_ticks_pause() {
    let wasm = guest(include_str!("late.wat"));
    let mut runner = ptyrc_box::Runner::load(&wasm, Bare, spin_limits()).unwrap();
    runner.init(&serde_json::json!({}), 0).unwrap();
    for t in 1..=5 {
        runner.tick(t * 100).unwrap();
    }
    assert!(runner.paused());
    assert_eq!(runner.timing().ticks, 3);
    assert_eq!(runner.met(), ["late", "paused"]);
}

/// Headless: lateness in session time, which a call does not take: the
/// same ticks spinning past the bound are neither late nor paused.
#[test]
fn headless_is_never_late() {
    let options = Options {
        limits: spin_limits(),
        duration_ms: 500,
        ..Options::default()
    };
    let report = run(&guest(include_str!("late.wat")), &options).unwrap();
    assert_eq!(report.end, None);
    assert_eq!(report.ticks, 5);
    assert!(report.met.is_empty(), "{:?}", report.met);
}

/// Headless: each call over `tick-ms` kept with its session time, for
/// `--timing`; out of the text report.
#[test]
fn headless_names_the_slow_calls() {
    let spin = "(func (export \"event\") (param i32 i32 i32 i32) (local $i i32)
      (loop $l
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br_if $l (i32.lt_u (local.get $i) (i32.const 30000000)))))";
    let wat =
        include_str!("late.wat").replace("(func (export \"event\") (param i32 i32 i32 i32))", spin);
    let options = Options {
        limits: spin_limits(),
        duration_ms: 300,
        events: vec![(150, "go".into(), serde_json::json!({}))],
        ..Options::default()
    };
    let report = run(&guest(&wat), &options).unwrap();
    let slow: Vec<(u64, Option<&str>)> = (report.timing.slow.iter())
        .map(|c| (c.now_ms, c.topic.as_deref()))
        .collect();
    assert_eq!(
        slow,
        [(100, None), (150, Some("go")), (200, None), (300, None)]
    );
    assert!(
        report
            .timing
            .slow
            .iter()
            .all(|c| c.took > Duration::from_millis(5))
    );
    let lines = report.slow_lines();
    assert!(
        lines[1].starts_with("slow at 150 ms: event go, "),
        "{lines:?}"
    );
    assert!(lines[0].starts_with("slow at 100 ms: tick, "), "{lines:?}");
    assert!(report.timing_line().contains("; events: 1, max "));
    assert!(!report.to_string().contains("slow"));
    let j = report.to_json();
    assert_eq!(j["slow"][1]["call"], "event");
    assert_eq!(j["slow"][1]["topic"], "go");
    assert_eq!(j["slow"][0]["t"], 100);
    assert!(j["slow"][0]["ms"].as_f64().unwrap() > 5.0);
    assert_eq!(j["events"], 1);
    // Under the bound: none.
    let options = Options {
        duration_ms: 300,
        ..Options::default()
    };
    let quick = run(&guest(include_str!("late.wat")), &options).unwrap();
    assert!(quick.timing.slow.is_empty() || quick.timing.max > Duration::from_millis(50));
}

/// `present` on canvases that are not; `release` twice; `cells` of
/// nothing.
#[test]
fn canvases_that_are_not() {
    let report = run(&guest(include_str!("canvas_ids.wat")), &quick()).unwrap();
    assert_eq!(report.end, None);
    assert!(report.requests.is_empty());
}
