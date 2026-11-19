// SPDX-License-Identifier: GPL-3.0-or-later
//! A guest in a live, headless session. Needs the `ptyrc` binary
//! (`PTYRC_BIN`, else `target/release/ptyrc`), so ignored by default:
//!
//!   cargo build --release -p ptyrc-cli
//!   cargo test --release -p ptyrc-box --test live -- --ignored --nocapture
//!
//! Prints the cost of a frame for the driver; asserts that 30 Hz is
//! held, the picture whole, and, one colour, the bound of the plan:
//! 2 ms a frame of the driver's processor time (measured 1.4 to 1.8 ms).
//! `PTYRC_SPEED_SLACK`: a factor on it, for a loaded machine.

mod common;

/// Not under the OS profile: the test starts the driver.
const CONFINE: bool = false;

use common::{EVENT, guest};
use ptyrc_box::{Limits, live};
use ptyrc_proto::{Client, session};
use serde_json::json;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const FRAMES: u32 = 90;
/// The bound of the plan, one colour: ms of the driver a frame.
const BOUND_MS: f64 = 2.0;

/// A 200x60 canvas repainted whole at every tick, 30 Hz, `FRAMES`
/// times. `vary`: 1, every cell its own colour (12000 cells sent a
/// frame); 0, one colour (a cell and its repeats a row).
fn repaint(vary: u32) -> Vec<u8> {
    guest(&format!(
        r#"
  (global $id (mut i32) (i32.const 0))
  (global $n (mut i32) (i32.const 0))
  (func (export "init") (param i32 i32)
    (call $period (i32.const 33))
    (global.set $id (call $canvas (i32.const 200) (i32.const 60))))
  (func (export "tick") (param i64) (local $p i32) (local $i i32)
    (global.set $n (i32.add (global.get $n) (i32.const 1)))
    (local.set $p (call $cells (global.get $id)))
    (loop $l
      (i32.store (local.get $p)
        (i32.add (i32.const 0x41) (i32.rem_u (global.get $n) (i32.const 26))))
      (i32.store (i32.add (local.get $p) (i32.const 4))
        (i32.or (i32.const 3)
          (i32.shl (i32.mul (i32.const {vary}) (i32.add (local.get $i) (global.get $n)))
                   (i32.const 8))))
      (local.set $p (i32.add (local.get $p) (i32.const 16)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 12000))))
    (drop (call $present (global.get $id) (i32.const 0) (i32.const 0)))
    (if (i32.ge_u (global.get $n) (i32.const {FRAMES})) (then (call $exit (i32.const 0)))))
  {EVENT}
"#
    ))
}

struct Driver(Child, PathBuf);

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn driver(name: &str) -> Driver {
    let bin = std::env::var_os("PTYRC_BIN").map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/release/ptyrc"
            ))
        },
        PathBuf::from,
    );
    let child = Command::new(&bin)
        .args([
            "run",
            "--headless",
            "--size",
            "200x60",
            "--name",
            name,
            "--",
            "sleep",
            "60",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("{}: {e}", bin.display()));
    let path = session::socket_path(name);
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < Duration::from_secs(5), "no socket");
        std::thread::sleep(Duration::from_millis(10));
    }
    Driver(child, path)
}

/// Processor time of a process so far (user and system), ms.
fn cpu_ms(pid: u32) -> f64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let rest = stat.rsplit_once(") ").unwrap().1;
    let field = |i: usize| rest.split(' ').nth(i).unwrap().parse::<f64>().unwrap();
    // Fields 14 and 15, in ticks of 10 ms.
    (field(11) + field(12)) * 10.0
}

#[test]
#[ignore = "needs the ptyrc binary"]
fn repaint_200x60_at_30_hz() {
    for (vary, what) in [(0, "one colour"), (1, "a colour a cell")] {
        let name = format!("box-live-{}-{vary}", std::process::id());
        let driver = driver(&name);
        let pid = driver.0.id();
        let client = Client::connect(&driver.1).unwrap();
        let host = live::Live::new(client, Box::new(std::io::sink()));
        let term = Arc::new(AtomicBool::new(false));
        let path = driver.1.clone();
        let seen = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let mut check = Client::connect(&path).unwrap();
            let text = check.call("screen.text", json!({"sprites": true})).unwrap();
            text["lines"][59].as_str().unwrap_or("").to_string()
        });
        let (before, start) = (cpu_ms(pid), Instant::now());
        let own = cpu_ms(std::process::id());
        let code = live::run(
            &repaint(vary),
            host,
            &json!({}),
            Limits::default(),
            1.0,
            term,
        );
        let wall = start.elapsed();
        let cost = (cpu_ms(pid) - before) / f64::from(FRAMES);
        let own = (cpu_ms(std::process::id()) - own) / f64::from(FRAMES);
        assert_eq!(code, Ok(0));
        // The picture, as the driver composed it midway.
        let row = seen.join().unwrap();
        let first = row.chars().next().unwrap_or(' ');
        assert!(first.is_ascii_uppercase(), "{row:?}");
        assert_eq!(row, first.to_string().repeat(200));
        println!(
            "  {what}: driver {cost:.2} ms a frame, runner {own:.2} ms a frame, \
             {FRAMES} frames in {:.2} s",
            wall.as_secs_f64()
        );
        assert!(wall < Duration::from_secs(4), "30 Hz not held: {wall:?}");
        if vary == 0 {
            let slack = std::env::var("PTYRC_SPEED_SLACK")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.0);
            assert!(
                cost < BOUND_MS * slack,
                "{what}: {cost:.2} ms a frame, over {BOUND_MS} ms (PTYRC_SPEED_SLACK {slack})"
            );
        }
    }
}
