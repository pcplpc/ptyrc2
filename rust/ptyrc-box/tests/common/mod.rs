// SPDX-License-Identifier: GPL-3.0-or-later
//! Guests for the tests, written in the text format.

#![allow(dead_code)]

/// Every import of ABI 1, a memory of two pages, a bump allocator
/// (`alloc` answers 0 when the memory does not grow) and a `free` that
/// does nothing. Data may sit under 4096.
pub const PRELUDE: &str = r#"
  (import "ptyrc" "call" (func $call (param i32 i32 i32 i32) (result i64)))
  (import "ptyrc" "canvas" (func $canvas (param i32 i32) (result i32)))
  (import "ptyrc" "cells" (func $cells (param i32) (result i32)))
  (import "ptyrc" "present" (func $present (param i32 i32 i32) (result i32)))
  (import "ptyrc" "release" (func $release (param i32)))
  (import "ptyrc" "period" (func $period (param i32)))
  (import "ptyrc" "log" (func $log (param i32 i32)))
  (import "ptyrc" "random" (func $random (param i32 i32)))
  (import "ptyrc" "exit" (func $exit (param i32)))
  (memory (export "memory") 2)
  (global $top (mut i32) (i32.const 4096))
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $top))
    (global.set $top (i32.add (global.get $top)
      (i32.and (i32.add (local.get $n) (i32.const 15)) (i32.const -16))))
    (if (i32.gt_u (global.get $top) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (if (i32.eq (i32.const -1)
              (memory.grow (i32.add (i32.shr_u (local.get $n) (i32.const 16)) (i32.const 1))))
          (then (global.set $top (local.get $p)) (return (i32.const 0))))))
    (local.get $p))
  (func (export "free") (param i32 i32))
"#;

/// Exports a guest must have and a test may not care for.
pub const INIT: &str = r#"(func (export "init") (param i32 i32))"#;
pub const TICK: &str = r#"(func (export "tick") (param i64))"#;
pub const EVENT: &str = r#"(func (export "event") (param i32 i32 i32 i32))"#;

/// The OS profile, once a process, where the test file asks for it
/// (`const CONFINE: bool`): what follows runs under it.
pub fn confine() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = ptyrc_box::profile::apply();
        // Past the capture of the test harness.
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{}", status.lines().join("\n"));
        if cfg!(target_os = "linux") {
            assert!(!status.applied.is_empty(), "{status:?}");
        }
    });
}

/// A module: the prelude and `body`. Under the profile when the test
/// file asks for it.
pub fn guest(body: &str) -> Vec<u8> {
    if crate::CONFINE {
        confine();
    }
    wat::parse_str(format!("(module {PRELUDE} {body})")).expect("wat")
}

/// A module of `body` alone.
pub fn bare(body: &str) -> Vec<u8> {
    if crate::CONFINE {
        confine();
    }
    wat::parse_str(format!("(module {body})")).expect("wat")
}
