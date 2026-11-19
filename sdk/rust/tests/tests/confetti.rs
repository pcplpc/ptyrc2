// SPDX-License-Identifier: GPL-3.0-or-later
//! `examples/confetti`, built for `wasm32-unknown-unknown`, run headless.

use ptyrc_box::headless::{Options, Report, run};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The release module of the example crate `name`, built once.
fn build(name: &str) -> Vec<u8> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(name);
    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "--release",
            "--locked",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .current_dir(&dir)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("cargo");
    assert!(status.success(), "the build of {name} failed");
    let wasm: PathBuf = dir.join(format!("target/wasm32-unknown-unknown/release/{name}.wasm"));
    std::fs::read(&wasm).expect("the module")
}

fn confetti() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| build("confetti"))
}

fn leb(b: &[u8], i: &mut usize) -> usize {
    let (mut n, mut shift) = (0, 0);
    loop {
        let x = b[*i];
        *i += 1;
        n |= ((x & 0x7f) as usize) << shift;
        shift += 7;
        if x < 0x80 {
            return n;
        }
    }
}

fn name(b: &[u8], i: &mut usize) -> String {
    let n = leb(b, i);
    let s = String::from_utf8(b[*i..*i + n].to_vec()).unwrap();
    *i += n;
    s
}

/// `(module, field, kind)` of the import section.
fn imports(b: &[u8]) -> Vec<(String, String, u8)> {
    assert_eq!(&b[..8], b"\0asm\x01\0\0\0");
    let (mut i, mut out) = (8, Vec::new());
    while i < b.len() {
        let id = b[i];
        i += 1;
        let size = leb(b, &mut i);
        let end = i + size;
        if id == 2 {
            for _ in 0..leb(b, &mut i) {
                let (m, f) = (name(b, &mut i), name(b, &mut i));
                let kind = b[i];
                i += 1;
                // A function: its type; the rest is refused below.
                assert_eq!(kind, 0, "{m}.{f}: not a function");
                leb(b, &mut i);
                out.push((m, f, kind));
            }
        }
        i = end;
    }
    out
}

#[test]
fn imports_ptyrc_only() {
    let found = imports(confetti());
    assert!(!found.is_empty());
    for (m, f, _) in &found {
        assert_eq!(m, "ptyrc", "{m}.{f}");
    }
}

fn options() -> Options {
    Options {
        size: (40, 12),
        args: json!({"bursts": 2}),
        duration_ms: 6500,
        at: (0..=65).map(|i| i * 100).collect(),
        seed: 7,
        ..Options::default()
    }
}

fn drawn(report: &Report) -> usize {
    report
        .screens
        .iter()
        .filter(|(_, rows)| rows.iter().any(|r| r.contains('o')))
        .count()
}

#[test]
fn the_same_text_twice() {
    let a = run(confetti(), &options()).unwrap();
    let b = run(confetti(), &options()).unwrap();
    assert_eq!(a.to_string(), b.to_string());
    assert_eq!(a.end, None, "{a}");
    assert!(a.met.is_empty(), "{a}");
    assert!(a.requests.iter().all(|r| r.refused.is_none()), "{a}");
    // Two rounds of eight pops: some frames show the third picture.
    assert!(drawn(&a) >= 2, "{a}");
    let other = run(
        confetti(),
        &Options {
            seed: 8,
            ..options()
        },
    )
    .unwrap();
    assert_ne!(a.to_string(), other.to_string());
}

#[test]
fn one_shot_exits() {
    let options = Options {
        args: json!({"one-shot": true}),
        ..options()
    };
    let report = run(confetti(), &options).unwrap();
    assert_eq!(report.end, Some(ptyrc_box::Error::Exit(0)), "{report}");
    let last = &report.screens.last().unwrap().1;
    assert!(last.iter().all(|r| r.is_empty()), "{report}");
}
