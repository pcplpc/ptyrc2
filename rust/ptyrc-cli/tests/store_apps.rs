// SPDX-License-Identifier: GPL-3.0-or-later
//! The harness of the shipped store apps, `plugins/store-apps/NAME/`
//! (`plugins/store-apps/README.md`). For each tree: `ptyrc dev check`;
//! its id its directory; its `LICENSE.txt` the repository's; each draw of
//! `[app] random` read by its options; its cases, `ptyrc dev test TREE
//! --cases` (each `tests/CASE.args` against `tests/CASE.txt`); `ptyrc
//! dev test --unit` when `tests.js` exists; a Rust app's `cargo test`;
//! each file named as one of `_shared/js/`, `_shared/rust/` that file,
//! byte for byte. `_shared/js/`: its `*.tests.js` run once, in a tree
//! of their own; `_shared/rust/`: its unit tests, once, natively.
//!
//! `PTYRC_BLESS=1`: `--cases --bless`, each `CASE.txt` written from the
//! run. `PTYRC_SYNC=1`: each copy of `_shared/` written from it. `PTYRC_STORE_APP=NAME` runs that tree alone. Without the
//! interpreter (`sdk/js/interp/build.sh`) the script apps are skipped,
//! unless `PTYRC_REQUIRE_JS=1`; without `rustc` and its wasm target the
//! Rust apps, unless `PTYRC_REQUIRE_RUSTC=1`.

use ptyrc_store::manifest::{self, Entry};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const JS: &str = "sdk/js/interp/target/ptyrc-js.wasm";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| v == "1")
}

/// The runner beside `ptyrc`, as built now.
fn build_runner() {
    let built = Command::new(env!("CARGO"))
        .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(built.success(), "cargo build -p ptyrc-box");
}

/// The runner the cases use: `target/release/ptyrc-box` when it was
/// built after the debug one (a start compiles the interpreter: some
/// 2 s of processor time against 7 s), else the debug one beside
/// `ptyrc`. `PTYRC_BOX` set: that one.
fn runner() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("PTYRC_BOX") {
        return Some(named.into());
    }
    let debug = Path::new(BIN).with_file_name("ptyrc-box");
    let release = debug.parent()?.parent()?.join("release/ptyrc-box");
    let built = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (built(&release)? > built(&debug)?).then_some(release)
}

/// `ptyrc ARGS` in `dir`, `ptyrc` first on `PATH`.
fn ptyrc(dir: &Path, args: &[String]) -> Output {
    let bin = Path::new(BIN).parent().unwrap();
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let mut c = Command::new(BIN);
    if let Some(r) = runner() {
        c.env("PTYRC_BOX", r);
    }
    c.args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("PTYRC")
        .env_remove("PTYRC_JS")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn said(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout\n{}--- stderr\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// What a tree copies from `_shared/KIND/`: `_shared/js/*.js` but its
/// `*.tests.js`, `_shared/rust/*.rs` but `lib.rs`, `main.rs` (their unit
/// tests are inside). `(KIND/NAME, bytes)`.
fn shared_in(base: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for kind in ["js", "rust"] {
        let Ok(dir) = std::fs::read_dir(base.join(kind)) else {
            continue;
        };
        for p in dir.flatten().map(|e| e.path()).filter(|p| p.is_file()) {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            let copied = match kind {
                "js" => n.ends_with(".js") && !n.ends_with(".tests.js"),
                _ => n.ends_with(".rs") && n != "lib.rs" && n != "main.rs",
            };
            if copied {
                out.push((format!("{kind}/{n}"), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn shared() -> Vec<(String, Vec<u8>)> {
    shared_in(&root().join("plugins/store-apps/_shared"))
}

/// The files of `dir` and below, `target/` and `tests/` left out.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() && name != "target" && name != "tests" && name != ".git" {
            out.extend(files(&p));
        } else if p.is_file() {
            out.push(p);
        }
    }
    out
}

/// Each copy in `dir` of a file of `_shared/` (a file of its name and
/// tier): that file, or written from it with `sync`.
fn copies(dir: &Path, shared: &[(String, Vec<u8>)], sync: bool) -> Vec<String> {
    let mut fails = Vec::new();
    for f in files(dir) {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let Some((from, source)) = shared.iter().find(|(n, _)| {
            n.strip_prefix("js/").or_else(|| n.strip_prefix("rust/")) == Some(name.as_str())
        }) else {
            continue;
        };
        if std::fs::read(&f).ok().as_ref() == Some(source) {
            continue;
        }
        let at = f.strip_prefix(dir).unwrap().display().to_string();
        if sync {
            std::fs::write(&f, source).unwrap();
            eprintln!("wrote {at} from _shared/{from}");
        } else {
            fails.push(format!(
                "{at}: not a copy of _shared/{from} (PTYRC_SYNC=1 copies it)"
            ));
        }
    }
    fails
}

#[test]
fn a_copy_is_its_source_byte_for_byte() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-apps-copies");
    let _ = std::fs::remove_dir_all(&base);
    for d in [
        "_shared/js",
        "_shared/rust",
        "app/src",
        "app/lib",
        "app/tests",
    ] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    let put = |p: &str, t: &str| std::fs::write(base.join(p), t).unwrap();
    put("_shared/js/sky.js", "sky\n");
    put("_shared/js/sky.tests.js", "tests\n");
    put("_shared/rust/grid.rs", "grid\n");
    put("_shared/rust/lib.rs", "lib\n");
    put("app/src/grid.rs", "grid, changed\n");
    put("app/src/lib.rs", "the app's own\n");
    put("app/lib/sky.js", "sky\n");
    put("app/tests/grid.rs", "not looked at\n");
    let shared = shared_in(&base.join("_shared"));
    let names: Vec<&str> = shared.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["js/sky.js", "rust/grid.rs"]);
    let app = base.join("app");
    assert_eq!(
        copies(&app, &shared, false),
        ["src/grid.rs: not a copy of _shared/rust/grid.rs (PTYRC_SYNC=1 copies it)"]
    );
    assert!(copies(&app, &shared, true).is_empty());
    assert_eq!(
        std::fs::read_to_string(app.join("src/grid.rs")).unwrap(),
        "grid\n"
    );
    assert!(copies(&app, &shared, false).is_empty());
    assert_eq!(
        std::fs::read_to_string(app.join("src/lib.rs")).unwrap(),
        "the app's own\n"
    );
}

/// The unit tests of the `*.rs` of `src`, natively: each a module of a
/// scratch crate `dir` with `ptyrc-app`, `cargo test`. `None`: no file.
fn rust_unit_tests(src: &Path, dir: &Path) -> Option<Output> {
    let mut names: Vec<String> = (std::fs::read_dir(src).ok()?.flatten())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rs") && n != "lib.rs" && n != "main.rs")
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort();
    let _ = std::fs::remove_dir_all(dir.join("src"));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut lib = String::from("// SPDX-License-Identifier: GPL-3.0-or-later\n");
    for n in &names {
        std::fs::copy(src.join(n), dir.join("src").join(n)).unwrap();
        lib.push_str(&format!("pub mod {};\n", n.trim_end_matches(".rs")));
    }
    std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
    let sdk = root().join("sdk/rust/ptyrc-app");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
             publish = false\n\n[workspace]\n\n[dependencies]\nptyrc-app = {{ path = {:?} }}\n",
            sdk.display().to_string()
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO"))
        .args(["test", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Some(out)
}

/// `_shared/rust/`: its files' unit tests, once.
#[test]
fn shared_rust_unit_tests() {
    let src = root().join("plugins/store-apps/_shared/rust");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-apps-shared-rust");
    match rust_unit_tests(&src, &dir) {
        None => eprintln!("store apps: _shared/rust: no file"),
        Some(out) => assert!(
            out.status.success(),
            "_shared/rust: cargo test\n{}",
            said(&out)
        ),
    }
}

/// The runner of `shared_rust_unit_tests` on a file of its own: a test
/// that passes, then one that fails.
#[test]
fn a_shared_rust_file_is_unit_tested() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-apps-rust-runner");
    let src = base.join("src-in");
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();
    assert!(rust_unit_tests(&src, &base.join("crate")).is_none());
    let file = |ok: bool| {
        format!(
            "// SPDX-License-Identifier: GPL-3.0-or-later\n\
             pub fn two() -> u32 {{ ptyrc_app::cells(\"ab\") }}\n\
             #[cfg(test)]\n#[test]\nfn it() {{ assert_eq!(super::two::two(), {}); }}\n",
            if ok { 2 } else { 3 }
        )
    };
    std::fs::write(src.join("two.rs"), file(true)).unwrap();
    let out = rust_unit_tests(&src, &base.join("crate")).unwrap();
    assert!(out.status.success(), "{}", said(&out));
    std::fs::write(src.join("two.rs"), file(false)).unwrap();
    let out = rust_unit_tests(&src, &base.join("crate")).unwrap();
    assert!(!out.status.success(), "{}", said(&out));
}

/// The unit tests of `_shared/js/`, once: its files and a `tests.js`
/// exporting every `*.tests.js`, as an app of a scratch tree.
#[test]
fn shared_unit_tests() {
    if !root().join(JS).is_file() {
        assert!(!flag("PTYRC_REQUIRE_JS"), "PTYRC_REQUIRE_JS=1: no {JS}");
        eprintln!("store apps: _shared/js unit tests SKIPPED: no interpreter");
        return;
    }
    build_runner();
    let src = root().join("plugins/store-apps/_shared/js");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-apps-shared");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut tests = String::from("// SPDX-License-Identifier: GPL-3.0-or-later\n");
    let mut names: Vec<String> = (std::fs::read_dir(&src).unwrap().flatten())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".js"))
        .collect();
    names.sort();
    for n in &names {
        std::fs::copy(src.join(n), dir.join(n)).unwrap();
        if n.ends_with(".tests.js") {
            tests.push_str(&format!("export * from \"./{n}\";\n"));
        }
    }
    std::fs::write(dir.join("tests.js"), tests).unwrap();
    std::fs::write(dir.join("app.js"), "export function init() {}\n").unwrap();
    std::fs::write(
        dir.join(manifest::MANIFEST),
        "[app]\nid = \"shared\"\nversion = \"0.1.0\"\nsummary = \"_shared/js\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n",
    )
    .unwrap();
    let unit = ptyrc(
        &dir,
        &["dev".into(), "test".into(), ".".into(), "--unit".into()],
    );
    assert!(
        unit.status.success(),
        "_shared/js: dev test --unit\n{}",
        said(&unit)
    );
}

/// Every problem of the tree `dir`.
fn tree(dir: &Path, js: bool, bless: bool) -> Vec<String> {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let mut fails = Vec::new();
    let check = ptyrc(dir, &["dev".into(), "check".into(), ".".into()]);
    if !check.status.success() {
        fails.push(format!("dev check\n{}", said(&check)));
    }
    let Ok(m) = manifest::parse_bytes(&std::fs::read(dir.join(manifest::MANIFEST)).unwrap()) else {
        return fails;
    };
    if m.app.id != name {
        fails.push(format!("[app] id = {:?}: not its directory", m.app.id));
    }
    let license = std::fs::read(root().join("LICENSE.txt")).unwrap();
    if std::fs::read(dir.join("LICENSE.txt")).ok() != Some(license) {
        fails.push("LICENSE.txt: not a copy of the repository's".into());
    }
    fails.extend(copies(dir, &shared(), flag("PTYRC_SYNC")));
    for draw in &m.app.random {
        if let Err(e) = ptyrc_store::options::read(&m, draw, dir) {
            fails.push(format!("[app] random: {draw:?}: {e}"));
        }
    }
    let rust = matches!(m.app.entry, Entry::Rust(_));
    if !rust && !js {
        return fails;
    }
    let mut argv: Vec<String> = vec!["dev".into(), "test".into(), ".".into(), "--cases".into()];
    if bless {
        argv.push("--bless".into());
    }
    let cases = ptyrc(dir, &argv);
    if !cases.status.success() {
        fails.push(format!("dev test --cases\n{}", said(&cases)));
    }
    if !rust && dir.join("tests.js").is_file() {
        let unit = ptyrc(
            dir,
            &["dev".into(), "test".into(), ".".into(), "--unit".into()],
        );
        if !unit.status.success() {
            fails.push(format!("dev test --unit\n{}", said(&unit)));
        }
    }
    if rust {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("store-apps")
            .join(&name);
        let unit = Command::new(env!("CARGO"))
            .args(["test", "--quiet", "--manifest-path"])
            .arg(dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", target)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        if !unit.status.success() {
            fails.push(format!("cargo test\n{}", said(&unit)));
        }
    }
    fails.into_iter().map(|f| format!("{name}: {f}")).collect()
}

#[test]
fn every_shipped_tree() {
    let apps = root().join("plugins/store-apps");
    let only = std::env::var("PTYRC_STORE_APP").ok();
    let mut trees: Vec<PathBuf> = (std::fs::read_dir(&apps).unwrap().flatten())
        .map(|e| e.path())
        .filter(|p| p.join(manifest::MANIFEST).is_file())
        .filter(|p| only.as_deref().is_none_or(|o| p.file_name().unwrap() == o))
        .collect();
    trees.sort();
    assert!(!trees.is_empty(), "no tree under {}", apps.display());
    let js = root().join(JS).is_file();
    if !js {
        assert!(!flag("PTYRC_REQUIRE_JS"), "PTYRC_REQUIRE_JS=1: no {JS}");
        eprintln!("store apps: script cases SKIPPED: no interpreter; sdk/js/interp/build.sh");
    }
    if let Err(e) = ptyrc_store::build::Toolchain::find() {
        assert!(!flag("PTYRC_REQUIRE_RUSTC"), "PTYRC_REQUIRE_RUSTC=1: {e}");
        eprintln!("store apps: Rust apps SKIPPED: {e}");
        trees.retain(|t| !t.join("Cargo.toml").is_file());
    }
    build_runner();
    let bless = flag("PTYRC_BLESS");
    let fails: Vec<String> = std::thread::scope(|s| {
        let runs: Vec<_> = (trees.iter())
            .map(|t| s.spawn(move || tree(t, js, bless)))
            .collect();
        runs.into_iter().flat_map(|r| r.join().unwrap()).collect()
    });
    assert!(fails.is_empty(), "\n{}", fails.join("\n\n"));
}

/// `ptyrc ARGS` with the shipped trees of the repository and empty
/// XDG directories.
fn store(home: &Path, args: &[&str]) -> Output {
    let mut c = Command::new(BIN);
    c.args(args)
        .env("PTYRC_STORE_APPS", root().join("plugins/store-apps"))
        .env("HOME", home)
        .stdin(Stdio::null());
    for (var, dir) in [
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_CONFIG_HOME", "config"),
    ] {
        c.env(var, home.join(dir));
    }
    c.output().unwrap()
}

#[test]
fn the_store_and_a_shipped_app() {
    let home =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("shipped-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();
    let ls = store(&home, &["store", "ls"]);
    assert!(ls.status.success(), "{}", said(&ls));
    let line = text(&ls)
        .lines()
        .find(|l| l.starts_with("ptyrc/sand"))
        .map(str::to_string);
    let line = line.unwrap_or_else(|| panic!("{}", said(&ls)));
    assert!(
        line.ends_with("shipped") && line.contains(" sand "),
        "{line}"
    );
    let show = store(&home, &["store", "show", "ptyrc/confetti"]);
    assert!(show.status.success(), "{}", said(&show));
    let shown = text(&show);
    assert!(
        shown.contains("(shipped)") && shown.contains("plugins/store-apps/confetti"),
        "{shown}"
    );
    let rm = store(&home, &["store", "rm", "sand"]);
    assert_eq!(rm.status.code(), Some(2), "{}", said(&rm));
    assert!(String::from_utf8_lossy(&rm.stderr).contains("ptyrc/sand: shipped with ptyrc"));
    let up = store(&home, &["store", "update", "ptyrc/sand"]);
    assert!(
        up.status.success() && text(&up).contains("updated with it"),
        "{}",
        said(&up)
    );
    let help = store(&home, &["app", "ptyrc/sand", "--help"]);
    assert!(
        text(&help).contains("ptyrc/sand 0.2.0, shipped"),
        "{}",
        said(&help)
    );
}
