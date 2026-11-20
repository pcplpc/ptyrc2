// SPDX-License-Identifier: GPL-3.0-or-later
//! Host functions of both SDKs, run headless. The no-clobber recipe: a
//! file read with its mtime, written guarded by it; a second write
//! guarded by the same mtime refused (`not-usable`). `until`.

use ptyrc_box::headless::{Options, run};
use ptyrc_box::hostfn::{Content, Named};
use ptyrc_box::script;
use ptyrc_store::build::{self, Toolchain};
use ptyrc_store::{DirTree, check};
use serde_json::json;
use std::path::Path;

const SCRIPT: &str = r#"
import { call, file, until } from "ptyrc";
export function init(args) {
  if (args.until) {
    const rows = [[1000, 1000], [1400, 1000], [1967, 1000], [1968, 1000], [-1, 1000], [5, 10]];
    const shown = rows.map(([at, every]) => until(at, every)).join(" ");
    return call("draw.put", { anchor: { at: [0, 0] }, content: { rows: [shown] } });
  }
  const f = file(args.notes);
  const { text, mtimeMs } = f.textWithMtime();
  const out = [text, mtimeMs];
  out.push(f.write(text + "!", { ifMtime: mtimeMs }).mtimeMs);
  try {
    f.write("lost", { ifMtime: mtimeMs });
    out.push("clobbered");
  } catch (e) {
    out.push(e.code);
  }
  out.push(f.text());
  call("draw.put", { anchor: { at: [0, 0] }, content: { rows: [out.join(" ")] } });
}
"#;

const RUST: &str = r#"// SPDX-License-Identifier: GPL-3.0-or-later
use ptyrc_app::{App, File, Value, text};
struct Notes;
impl App for Notes {
    fn init(args: Value) -> Notes {
        let f = File::of(&args["notes"]).expect("notes");
        let (old, mtime) = f.text_with_mtime().expect("read");
        let mut out = vec![old.clone(), mtime.to_string()];
        let new = f.write(&format!("{old}!"), Some(mtime)).expect("write");
        out.push(new.mtime_ms.to_string());
        out.push(match f.write("lost", Some(mtime)) {
            Ok(_) => "clobbered".into(),
            Err(e) => e.code,
        });
        out.push(f.text().expect("read"));
        text(0, 0, &[out.join(" ").as_str()], Value::Null).expect("drawn");
        Notes
    }
}
ptyrc_app::app!(Notes);
"#;

const MANIFEST: &str = r#"# SPDX-License-Identifier: GPL-3.0-or-later
[app]
id = "notes"
version = "0.1.0"
summary = "A file read and written back."
license = "GPL-3.0-or-later"
api = 1
rust = "src/lib.rs"
rustc = "1.85"
surface = "screen"

[options]
notes = { type = "file", write = true, doc = "the file" }

[permissions]
files = "reads the file named"
files-write = "writes it back"
"#;

/// `notes`: `todo`, mtime 5; the session's wall clock from 1000000.
fn options(args: serde_json::Value) -> Options {
    let mut o = Options {
        size: (40, 2),
        args,
        duration_ms: 0,
        at: vec![0],
        ..Options::default()
    };
    o.host.wall0 = 1_000_000;
    o.host.files.push(Named {
        option: "notes".into(),
        name: "TODO.md".into(),
        entries: None,
        content: Content {
            bytes: Some(b"todo".to_vec()),
            mtime_ms: 5,
        },
        written: false,
        write: true,
    });
    o
}

const SHOWN: &str = "todo 5 1000000 not-usable todo!";

#[test]
fn script_reads_with_mtime_and_writes_guarded() {
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
    let args = |a| options(script::init_args("app.js", &sources, a));
    let r = run(&wasm, &args(json!({}))).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert_eq!(r.screens[0].1[0].trim_end(), SHOWN, "{r}");
    // As `clock::until` of `ptyrc-app` (its unit test).
    let r = run(&wasm, &args(json!({"until": true}))).unwrap();
    assert_eq!(
        r.screens[0].1[0].trim_end(),
        "1000 600 33 1032 1001 35",
        "{r}"
    );
}

#[test]
fn rust_reads_with_mtime_and_writes_guarded() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("notes");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::copy(root.join("LICENSE.txt"), dir.join("LICENSE.txt")).unwrap();
    std::fs::write(dir.join("ptyrc-app.toml"), MANIFEST).unwrap();
    std::fs::write(dir.join("src/lib.rs"), RUST).unwrap();
    let tree = DirTree::new(&dir);
    let checked = check(&tree).expect("the tree check");
    let toolchain = Toolchain::find().expect("rustc and its wasm target");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("notes-work");
    let built = build::build(
        &tree,
        &checked,
        &work,
        &toolchain,
        &build::Options::default(),
    );
    let wasm = built.unwrap_or_else(|e| panic!("notes: {e}")).wasm;
    let r = run(&wasm, &options(json!({}))).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert_eq!(r.screens[0].1[0].trim_end(), SHOWN, "{r}");
}
