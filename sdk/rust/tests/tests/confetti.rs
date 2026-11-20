// SPDX-License-Identifier: GPL-3.0-or-later
//! `examples/confetti`, built as the store builds it, run headless.

use ptyrc_box::headless::{Options, Report, run};
use ptyrc_store::build::{self, Toolchain};
use ptyrc_store::{DirTree, check};
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;

/// The module of the example `name`, built once as the store builds it:
/// `rustc` alone, ptyrc's own `ptyrc-app`.
fn build(name: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tree = DirTree::new(root.join("../examples").join(name));
    let checked = check(&tree).expect("the tree check");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let toolchain = Toolchain::find().expect("rustc and its wasm target");
    let built = build::build(
        &tree,
        &checked,
        &work,
        &toolchain,
        &build::Options::default(),
    );
    built.unwrap_or_else(|e| panic!("{name}: {e}")).wasm
}

fn confetti() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| build("confetti"))
}

/// `(module, field)` of the import section.
fn imports(b: &[u8]) -> Vec<(String, String)> {
    build::imports(b).unwrap()
}

#[test]
fn imports_ptyrc_only() {
    let found = imports(confetti());
    assert!(!found.is_empty());
    for (m, f) in &found {
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

/// The bounds in the manifests, refused before the app runs: `bursts`
/// 1 to 10 here, `rate` 1 to 200 in the script's confetti.
#[test]
fn bounds_in_the_manifest() {
    use ptyrc_store::{manifest, options};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (dir, name, max) in [
        ("../examples/confetti", "bursts", 10),
        ("../../js/examples/confetti", "rate", 200),
    ] {
        let dir = root.join(dir);
        let m = std::fs::read(dir.join(manifest::MANIFEST)).unwrap();
        let m = manifest::parse_bytes(&m).unwrap();
        let read = |n: i32| options::read(&m, &[format!("--{name}"), n.to_string()], &dir);
        assert!(read(max).is_ok() && read(1).is_ok(), "{name}");
        assert!(read(0).is_err() && read(max + 1).is_err(), "{name}");
    }
}
