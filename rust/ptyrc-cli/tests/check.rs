// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev check` on the keys of the manifest: a default absent, a
//! default frame, `seed` reserved, a text left `TODO`.

mod common;

use common::Sandbox;
use std::path::Path;

const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";

fn tree(dir: &Path, app: &str, rest: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest = format!(
        "[app]\nid = \"a\"\nversion = \"0.1.0\"\nlicense = \"GPL-3.0-or-later\"\napi = 1\n\
         script = \"app.js\"\n{app}\n{rest}"
    );
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    std::fs::write(dir.join("app.js"), "export function init(args) {}\n").unwrap();
    std::fs::write(dir.join("LICENSE"), GPL).unwrap();
}

/// Status, stdout and stderr of `ptyrc dev check` on `app` and `rest`.
fn check(sb: &Sandbox, name: &str, app: &str, rest: &str) -> (Option<i32>, String) {
    let dir = sb.dir.join(name);
    tree(&dir, app, rest);
    let out = sb.run(&["dev", "check", &dir.to_string_lossy()]);
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code(), text)
}

#[test]
fn the_keys_of_the_manifest() {
    let sb = Sandbox::new("check");
    let summary = "summary = \"An app.\"";
    // No default; a default frame; `random` draws without the option.
    let (status, text) = check(
        &sb,
        "ok",
        &format!("{summary}\nframe = \"largest\"\nrandom = [[], [\"--count\", \"3\"]]"),
        "[options]\ncount = { type = \"int\", min = 1, doc = \"how many\" }\n\
         shy = { type = \"bool\", doc = \"hide\" }\n",
    );
    assert_eq!(status, Some(0), "{text}");
    assert!(!text.contains("note:"), "{text}");
    for (name, app, rest, said) in [
        (
            "all",
            format!("{summary}\nframe = \"all\""),
            "",
            "[app] frame: \"all\": a store app draws on one frame",
        ),
        (
            "seed",
            summary.to_string(),
            "[options]\nseed = { type = \"int\", doc = \"d\" }\n",
            "[options] seed: an option of ptyrc's own",
        ),
    ] {
        let (status, text) = check(&sb, name, &app, rest);
        assert_eq!(status, Some(1), "{text}");
        assert!(text.contains(said), "{text}");
    }
    // `TODO`: a note; the install refuses it.
    let (status, text) = check(
        &sb,
        "todo",
        "summary = \"TODO: what the app does, in one line.\"",
        "[permissions]\nshape = \"TODO: why\"\n",
    );
    assert_eq!(status, Some(0), "{text}");
    for said in [
        "note: [app] summary: starts with TODO",
        "note: [permissions] shape: starts with TODO",
    ] {
        assert!(text.contains(said), "{said}: {text}");
    }
}
