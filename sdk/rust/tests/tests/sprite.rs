// SPDX-License-Identifier: GPL-3.0-or-later
//! `sprite()` of a canvas, both SDKs, headless: none before `present`,
//! then the `sprite` of a `hover` on it.

use ptyrc_box::headless::{Inject, Options, run};
use ptyrc_box::script;
use ptyrc_store::build::{self, Toolchain};
use ptyrc_store::{DirTree, check};
use std::path::Path;

const SCRIPT: &str = r#"
import { Canvas, call } from "ptyrc";
let c;
export function init() {
  c = new Canvas(3, 1);
  const before = c.sprite();
  c.text(0, 0, "abc");
  c.present({ anchor: { at: [0, 0] }, layer: "front" });
  call("subscribe", { topics: ["hover"] });
  call("draw.put", { anchor: { at: [0, 1] }, content: { rows: [`${before} ${c.sprite() !== null}`] } });
}
export function tick() {}
export function event(topic, data) {
  if (topic === "hover" && data.sprite !== null)
    call("draw.put", { anchor: { at: [0, 2] }, content: { rows: [`hover ${data.sprite === c.sprite()}`] } });
}
"#;

const RUST: &str = r#"// SPDX-License-Identifier: GPL-3.0-or-later
use ptyrc_app::{App, Canvas, Value, call, json, text};
struct Spr(Canvas);
impl App for Spr {
    fn init(_: Value) -> Spr {
        let mut c = Canvas::new(3, 1).expect("canvas");
        let before = format!("{:?}", c.sprite());
        c.text(0, 0, "abc");
        c.present(&json!({"anchor": {"at": [0, 0]}, "layer": "front"}));
        call("subscribe", json!({"topics": ["hover"]})).expect("subscribed");
        let line = format!("{before} {}", c.sprite().is_some());
        text(0, 1, &[line.as_str()], Value::Null).expect("drawn");
        Spr(c)
    }
    fn event(&mut self, topic: &str, data: Value) {
        if topic == "hover" && !data["sprite"].is_null() {
            let same = data["sprite"].as_u64() == self.0.sprite();
            text(0, 2, &[format!("hover {same}").as_str()], Value::Null).expect("drawn");
        }
    }
}
ptyrc_app::app!(Spr);
"#;

const MANIFEST: &str = r#"# SPDX-License-Identifier: GPL-3.0-or-later
[app]
id = "spr"
version = "0.1.0"
summary = "The sprite of a canvas."
license = "GPL-3.0-or-later"
api = 1
rust = "src/lib.rs"
rustc = "1.88"
surface = "screen"
"#;

fn options(args: serde_json::Value) -> Options {
    Options {
        size: (20, 3),
        args,
        duration_ms: 100,
        at: vec![100],
        inject: vec![(50, Inject::Pointer(1, 0))],
        ..Options::default()
    }
}

fn rows(r: &ptyrc_box::headless::Report) -> Vec<String> {
    let rows = r.screens[0].1.iter().map(|l| l.trim_end().to_string());
    rows.collect()
}

#[test]
fn script_canvas_sprite() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../js/interp/target/ptyrc-js.wasm");
    let path = std::env::var_os(script::ENV_JS).map_or(path, Into::into);
    let Ok(wasm) = std::fs::read(path) else {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_JS").is_none(),
            "no interpreter"
        );
        eprintln!("script test SKIPPED: no interpreter (sdk/js/interp/build.sh)");
        return;
    };
    let sources = [("app.js".to_string(), SCRIPT.to_string())].into();
    let o = options(script::init_args("app.js", &sources, serde_json::json!({})));
    let r = run(&wasm, &o).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert_eq!(rows(&r), ["abc", "null true", "hover true"], "{r}");
}

#[test]
fn rust_canvas_sprite() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("spr");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::copy(root.join("LICENSE.txt"), dir.join("LICENSE.txt")).unwrap();
    std::fs::write(dir.join("ptyrc-app.toml"), MANIFEST).unwrap();
    std::fs::write(dir.join("src/lib.rs"), RUST).unwrap();
    let tree = DirTree::new(&dir);
    let checked = check(&tree).expect("the tree check");
    let toolchain = Toolchain::find().expect("rustc and its wasm target");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("spr-work");
    let built = build::build(
        &tree,
        &checked,
        &work,
        &toolchain,
        &build::Options::default(),
    );
    let wasm = built.unwrap_or_else(|e| panic!("spr: {e}")).wasm;
    let r = run(&wasm, &options(serde_json::json!({}))).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert_eq!(rows(&r), ["abc", "None true", "hover true"], "{r}");
}
