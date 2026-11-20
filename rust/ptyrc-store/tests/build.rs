// SPDX-License-Identifier: GPL-3.0-or-later
//! The build by `rustc` alone: the examples, and hostile sources against
//! the rule, the profile and the bounds. Skipped without `rustc` and its
//! wasm target, unless `PTYRC_REQUIRE_RUSTC=1`.

mod common;

use ptyrc_store::build::{self, BOUNDS, Bounds, Options, Toolchain};
use ptyrc_store::{DirTree, ErrorKind, check};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn toolchain() -> Option<Toolchain> {
    match Toolchain::find() {
        Ok(t) => Some(t),
        Err(e) if std::env::var_os("PTYRC_REQUIRE_RUSTC").is_some_and(|v| v == "1") => {
            panic!("PTYRC_REQUIRE_RUSTC=1: {e}")
        }
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/rust/examples")
}

const MANIFEST: &str = r#"
[app]
id = "hostile"
version = "0.1.0"
summary = "Tries."
license = "GPL-3.0-or-later"
api = 1
rust = "src/lib.rs"
"#;

/// An app tree of `lib.rs` (and more files), and its build.
fn build_src(
    name: &str,
    files: &[(&str, &str)],
    options: &Options,
) -> Option<ptyrc_store::Result<build::Built>> {
    let t = toolchain()?;
    let dir = common::tmp(name);
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("src")).unwrap();
    std::fs::write(tree.join("ptyrc-app.toml"), MANIFEST).unwrap();
    std::fs::write(tree.join("LICENSE"), common::GPL).unwrap();
    for (p, text) in files {
        let p = tree.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let dt = DirTree::new(&tree);
    let checked = check(&dt).unwrap();
    Some(build::build(&dt, &checked, &dir.join("work"), &t, options))
}

const APP: &str = "use ptyrc_app::{App, Value};\n\
                   struct A;\n\
                   impl App for A { fn init(_: Value) -> A { A } }\n\
                   ptyrc_app::app!(A);\n";

#[test]
fn the_examples_build_and_import_ptyrc_only() {
    let Some(t) = toolchain() else { return };
    for name in ["confetti", "sand", "snake"] {
        let tree = DirTree::new(examples().join(name));
        let checked = check(&tree).unwrap();
        assert!(build::sources(&tree, &checked).is_empty());
        let work = common::tmp(name).join("work");
        let built = build::build(&tree, &checked, &work, &t, &Options::default()).unwrap();
        assert!(built.wasm.len() < 1 << 20, "{name}: {}", built.wasm.len());
        let imports = build::imports(&built.wasm).unwrap();
        assert!(imports.iter().all(|(m, _)| m == "ptyrc"), "{imports:?}");
        if cfg!(target_os = "linux") {
            assert!(built.missing.is_none(), "{:?}", built.missing);
            assert!(
                built.profile[0].starts_with("landlock ABI"),
                "{:?}",
                built.profile
            );
        }
        // Built again in the same work directory: the same module.
        let again = build::build(&tree, &checked, &work, &t, &Options::default()).unwrap();
        assert_eq!(again.wasm, built.wasm, "{name}");
    }
}

#[test]
fn a_work_directory_not_ours_is_kept() {
    let Some(t) = toolchain() else { return };
    let tree = DirTree::new(examples().join("sand"));
    let checked = check(&tree).unwrap();
    let work = common::tmp("not-ours");
    std::fs::write(work.join("precious"), "x").unwrap();
    let e = build::build(&tree, &checked, &work, &t, &Options::default()).unwrap_err();
    assert!(e.message.contains("not made by a build"), "{e}");
    assert!(work.join("precious").is_file());
}

fn refused(name: &str, files: &[(&str, &str)], options: &Options, want: &str) {
    let Some(got) = build_src(name, files, options) else {
        return;
    };
    let e = got.err().unwrap_or_else(|| panic!("{name}: built"));
    assert_eq!(e.kind, ErrorKind::Build, "{e}");
    assert!(e.message.contains(want), "{name}: {e}");
}

#[test]
fn a_plain_app_builds() {
    let Some(got) = build_src("plain", &[("src/lib.rs", APP)], &Options::default()) else {
        return;
    };
    let built = got.unwrap();
    assert!(
        build::imports(&built.wasm)
            .unwrap()
            .iter()
            .all(|(m, _)| m == "ptyrc")
    );
}

#[test]
fn hostile_sources_refused() {
    let o = Options::default();
    let with = |extra: &str| format!("{APP}{extra}\n");
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "include-out",
            with("pub static S: &str = include_str!(\"/etc/passwd\");"),
            "out of the tree",
        ),
        (
            "path-out",
            with("#[path = \"/etc/passwd\"] mod p;"),
            "#[path]",
        ),
        (
            "link-wasi",
            with(
                "#[link(wasm_import_module = \"wasi_snapshot_preview1\")] unsafe extern \"C\" { fn fd_write(); }",
            ),
            "#[link]",
        ),
        (
            "feature",
            format!("#![feature(rustc_attrs)]\n{APP}"),
            "#![feature]",
        ),
        (
            "extern-crate",
            with("extern crate proc_macro;"),
            "extern crate proc_macro",
        ),
        // No `#[link]`: an import of `env`; rust-lld refuses it undefined.
        (
            "import-env",
            with(
                "unsafe extern \"C\" { fn open(p: u32) -> u32; }\n\
                 #[unsafe(no_mangle)] pub extern \"C\" fn go() -> u32 { unsafe { open(1) } }",
            ),
            "undefined symbol: open",
        ),
    ];
    for (name, src, want) in &cases {
        refused(name, &[("src/lib.rs", src)], &o, want);
    }
    refused(
        "build-rs",
        &[("src/lib.rs", APP), ("build.rs", "fn main() {}")],
        &o,
        "build.rs",
    );
    refused(
        "cargo-deps",
        &[
            ("src/lib.rs", APP),
            (
                "Cargo.toml",
                "[package]\nname = \"h\"\n[dependencies]\nserde = \"1\"\n",
            ),
        ],
        &o,
        "[dependencies] serde",
    );
}

fn quick(time: u64, module: u64) -> Options {
    Options {
        bounds: Bounds {
            time: Duration::from_secs(time),
            module,
            ..BOUNDS
        },
        ..Options::default()
    }
}

#[test]
fn an_endless_const_meets_the_time() {
    let src = format!(
        "#![allow(long_running_const_eval)]\n{APP}\
         const X: u64 = {{ let mut i = 0u64; loop {{ i = i.wrapping_add(1); }} }};\n\
         #[unsafe(no_mangle)] pub static Y: u64 = X;\n"
    );
    let start = std::time::Instant::now();
    refused(
        "const-loop",
        &[("src/lib.rs", &src)],
        &quick(15, BOUNDS.module),
        "past 15 s",
    );
    assert!(start.elapsed() < Duration::from_secs(40));
}

#[test]
fn a_macro_that_doubles_is_ended() {
    let src = format!(
        "#![recursion_limit = \"100000\"]\n{APP}\
         macro_rules! m {{ ($($t:tt)*) => {{ m!($($t)* $($t)*); }} }}\nm!(x);\n"
    );
    // The time, or the memory first.
    let Some(got) = build_src("macro", &[("src/lib.rs", &src)], &quick(15, BOUNDS.module)) else {
        return;
    };
    let e = got.expect_err("built");
    assert!(e.message.starts_with("src/lib.rs: "), "{e}");
}

#[test]
fn a_huge_static_meets_the_size() {
    let src = |mib: u32| {
        format!("{APP}#[unsafe(no_mangle)] pub static BIG: [u8; {mib} << 20] = [1; {mib} << 20];\n")
    };
    // The module past its bound.
    refused(
        "static-2",
        &[("src/lib.rs", &src(2))],
        &quick(120, 1 << 20),
        "more than 1 MiB",
    );
    // A file of the build past four times it: rustc ended.
    refused(
        "static-8",
        &[("src/lib.rs", &src(8))],
        &quick(120, 1 << 20),
        "a file past 4 MiB",
    );
}

#[test]
fn an_old_toolchain_is_named() {
    let Some(t) = toolchain() else { return };
    let tree = DirTree::new(examples().join("sand"));
    let mut checked = check(&tree).unwrap();
    checked.manifest.app.rustc = Some("9.0".into());
    let work = common::tmp("old").join("work");
    let e = build::build(&tree, &checked, &work, &t, &Options::default()).unwrap_err();
    assert!(e.message.contains("is older"), "{e}");
    assert_eq!(e.hint.as_deref(), Some("rustup update"));
}

#[test]
fn no_rustc_says_how_to_get_it() {
    // In a process of its own: the environment is the process's.
    let e = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "probe_no_rustc",
            "--nocapture",
            "--include-ignored",
        ])
        .env("PTYRC_RUSTC", "/nonexistent/rustc")
        .output()
        .unwrap();
    assert!(e.status.success(), "{}", String::from_utf8_lossy(&e.stdout));
}

#[test]
#[ignore = "run by no_rustc_says_how_to_get_it, PTYRC_RUSTC set"]
fn probe_no_rustc() {
    let e = Toolchain::find().unwrap_err();
    assert!(e.message.starts_with("/nonexistent/rustc"), "{e}");
    assert!(e.hint.unwrap().contains("https://rustup.rs"));
}

/// Two builds at once in one work directory, each under `build::lock`:
/// one after the other, the same module.
#[test]
fn two_builds_at_once_in_one_work_directory() {
    let Some(t) = toolchain() else { return };
    let tree = DirTree::new(examples().join("sand"));
    let checked = check(&tree).unwrap();
    let work = common::tmp("twice").join("work");
    let built: Vec<_> = std::thread::scope(|s| {
        let all: Vec<_> = (0..3)
            .map(|_| {
                s.spawn(|| {
                    let _held = build::lock(&work).unwrap();
                    build::build(&tree, &checked, &work, &t, &Options::default())
                })
            })
            .collect();
        all.into_iter()
            .map(|r| r.join().unwrap().unwrap())
            .collect()
    });
    assert!(built.iter().all(|b| b.wasm == built[0].wasm));
}
