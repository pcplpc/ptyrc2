// SPDX-License-Identifier: GPL-3.0-or-later
//! `examples/clockface`, the deck of `ptyrc-app`, built as the store
//! builds it, run headless.

use ptyrc_box::headless::{Options, run};
use ptyrc_store::build::{self, Toolchain};
use ptyrc_store::{DirTree, check};
use serde_json::json;
use std::path::Path;

fn build() -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tree = DirTree::new(root.join("../examples/clockface"));
    let checked = check(&tree).expect("the tree check");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("clockface");
    let toolchain = Toolchain::find().expect("rustc and its wasm target");
    let built = build::build(
        &tree,
        &checked,
        &work,
        &toolchain,
        &build::Options::default(),
    );
    built.unwrap_or_else(|e| panic!("clockface: {e}")).wasm
}

#[test]
fn a_widget_kept_drawn_its_keys() {
    let wasm = build();
    for (m, f) in build::imports(&wasm).unwrap() {
        assert_eq!(m, "ptyrc", "{m}.{f}");
    }
    let options = Options {
        size: (40, 8),
        duration_ms: 4000,
        at: vec![2000, 4000],
        // Paused at 2.5 s: 00:02 at 4 s.
        events: vec![(2500, "input".into(), json!({"widget": 1, "key": "space"}))],
        ..Options::default()
    };
    let report = run(&wasm, &options).unwrap();
    assert_eq!(report.end, None, "{report}");
    let text = report.to_string();
    let two = "│█ █ █ █ ▪ █ █ █▀▀│";
    assert_eq!(text.matches(two).count(), 2, "{report}");
    assert!(text.contains("clockface ╯"), "{report}");
    assert!(
        report.requests.iter().all(|r| r.refused.is_none()),
        "{report}"
    );
}
