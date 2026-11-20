// SPDX-License-Identifier: GPL-3.0-or-later
//! The ticks of one guest, live (a driver of the test's own) and
//! headless: the same times for a script of periods asked in `init`, in
//! `tick`, in `event`. A timing test: it may fail under load.

mod common;

use ptyrc_box::Limits;
use ptyrc_box::headless::{self, Options};
use ptyrc_box::live::{self, Live};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CONFINE: bool = false;

/// `init` asks 100; the ticks 250, 40, 400, 0; `call("t")` in each.
/// An event asks 60; the next tick 120; the one after exits.
const GUEST: &str = r#"
  (data (i32.const 0) "\fa\00\00\00\28\00\00\00\90\01\00\00\00\00\00\00\78\00\00\00")
  (data (i32.const 100) "t")
  (global $k (mut i32) (i32.const 0))
  (func $say (param $v i64)
    (local $p i32)
    (local.set $p (i32.const 2040))
    (loop $d
      (local.set $p (i32.sub (local.get $p) (i32.const 1)))
      (i32.store8 (local.get $p)
        (i32.add (i32.const 48) (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const 10)))))
      (local.set $v (i64.div_u (local.get $v) (i64.const 10)))
      (br_if $d (i64.ne (local.get $v) (i64.const 0))))
    (local.set $p (i32.sub (local.get $p) (i32.const 2)))
    (i32.store16 (local.get $p) (i32.const 0x2074))
    (call $log (local.get $p) (i32.sub (i32.const 2040) (local.get $p))))
  (func (export "init") (param i32 i32) (call $period (i32.const 100)))
  (func (export "tick") (param $now i64)
    (call $say (local.get $now))
    (if (i32.eq (global.get $k) (i32.const 5)) (then (call $exit (i32.const 0))))
    (drop (call $call (i32.const 100) (i32.const 1) (i32.const 0) (i32.const 0)))
    (call $period (i32.load (i32.mul (global.get $k) (i32.const 4))))
    (global.set $k (i32.add (global.get $k) (i32.const 1))))
  (func (export "event") (param i32 i32 i32 i32) (call $period (i32.const 60)))
"#;

/// After the fourth tick, the event this much later.
const EVENT_AFTER: u64 = 300;

struct Log(Arc<Mutex<Vec<u8>>>);

impl Write for Log {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The lines ending `t N` of a log, N.
fn times<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<u64> {
    lines
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w[..] {
                [.., "t", n] => n.parse().ok(),
                _ => None,
            }
        })
        .collect()
}

/// A driver: `hello`, `{}` to every request, the event `go` once the
/// fourth `t` is `EVENT_AFTER` old.
fn driver(listener: UnixListener) {
    let (stream, _) = listener.accept().unwrap();
    let mut out = stream.try_clone().unwrap();
    let hello = json!({"event": "hello", "data": {"protocol": 1, "size": [80, 24]}});
    writeln!(out, "{hello}").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(2)))
        .unwrap();
    let mut read = BufReader::new(stream);
    let (mut seen, mut go) = (0, None::<Instant>);
    let mut line = String::new();
    loop {
        if go.is_some_and(|t| t.elapsed() >= Duration::from_millis(EVENT_AFTER)) {
            go = None;
            writeln!(out, "{}", json!({"event": "go", "data": {}})).unwrap();
        }
        match read.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) if line.ends_with('\n') => {
                let v: Value = serde_json::from_str(&line).unwrap();
                line.clear();
                if v["method"] == "t" {
                    seen += 1;
                    if seen == 4 {
                        go = Some(Instant::now());
                    }
                }
                if writeln!(out, "{}", json!({"id": v["id"], "result": {}})).is_err() {
                    return;
                }
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
}

#[test]
fn live_ticks_as_headless() {
    let wasm = common::guest(GUEST);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(format!("ticks-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let serve = std::thread::spawn(move || driver(listener));
    let client = Client::connect_as(&path, None).unwrap();
    let buf = Arc::new(Mutex::new(Vec::new()));
    let host = Live::new(client, Box::new(Log(buf.clone())));
    let term = Arc::new(AtomicBool::new(false));
    let status = live::run(&wasm, host, &json!({}), Limits::default(), 1.0, term).unwrap();
    assert_eq!(status, 0);
    drop(serve);
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    let live = times(text.lines());
    assert_eq!(live.len(), 6, "{text}");

    // Headless, the event where it fell live.
    let go = live[3] + EVENT_AFTER;
    let options = Options {
        duration_ms: 5000,
        events: vec![(go, "go".into(), json!({}))],
        ..Options::default()
    };
    let report = headless::run(&wasm, &options).unwrap();
    let headless = times(report.log.iter().map(String::as_str));
    let want = [100, 350, 390, 790, go + 60, go + 180];
    assert_eq!(headless, want, "live {live:?}");
    // Live is late by its wake-ups alone, never early.
    for (l, h) in live.iter().zip(&headless) {
        assert!(
            *h <= *l && *l <= *h + 40,
            "live {live:?}, headless {headless:?}"
        );
    }
}
