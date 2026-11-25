// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev vet`: its status is a contract with a registry's builder.
//! 1 is the app's fault alone; a builder that cannot build is 3. No
//! HOME. The Rust cases skipped without `rustc` and its wasm target,
//! unless `PTYRC_REQUIRE_RUSTC=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";

fn has_rustc() -> bool {
    let ok = ptyrc_store::build::Toolchain::find();
    if let Err(e) = &ok {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {e}"
        );
        eprintln!("skipped: {e}");
    }
    ok.is_ok()
}

fn tmp(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vet-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    dir
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

struct Out {
    status: i32,
    stdout: String,
    stderr: String,
}

/// `ptyrc ARGS`, no HOME, no XDG directory; TMPDIR under `root`.
fn ptyrc(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Out {
    let mut c = Command::new(BIN);
    c.args(args)
        .env_remove("HOME")
        .env("TMPDIR", root.join("tmp"))
        .stdin(Stdio::null());
    for k in [
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
        "PTYRC",
    ] {
        c.env_remove(k);
    }
    c.envs(env.iter().copied());
    let o = c.output().unwrap();
    Out {
        status: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

#[track_caller]
fn status(o: &Out, want: i32) {
    assert_eq!(o.status, want, "stdout: {}\nstderr: {}", o.stdout, o.stderr);
}

fn vet(root: &Path, dir: &Path, out: Option<&Path>, env: &[(&str, &str)]) -> Out {
    let mut args = vec!["dev", "vet", dir.to_str().unwrap()];
    if let Some(o) = out {
        args.extend(["--out", o.to_str().unwrap(), "--force"]);
    }
    ptyrc(root, &args, env)
}

fn write(dir: &Path, files: &[(&str, &[u8])]) {
    for (rel, b) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    }
}

/// A script app: valid, and nothing else.
fn script_app(dir: &Path) {
    write(
        dir,
        &[
            (
                "ptyrc-app.toml",
                b"[app]\nid = \"sand\"\nversion = \"0.1.0\"\nsummary = \"Sand.\"\n\
                  license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n",
            ),
            ("LICENSE", GPL.as_bytes()),
            (
                "app.js",
                b"import { x } from \"./lib/x.js\";\nexport function init() {}\n",
            ),
            ("lib/x.js", b"export const x = 1;\n"),
        ],
    );
}

/// The Rust example `confetti`, copied to `dir`.
fn rust_app(dir: &Path) {
    let src = repo("sdk/rust/examples/confetti");
    for f in [
        "ptyrc-app.toml",
        "Cargo.toml",
        "Cargo.lock",
        "LICENSE.txt",
        "src/lib.rs",
    ] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(src.join(f), to).unwrap();
    }
}

#[test]
fn script_examples_pass() {
    let root = tmp("js");
    for e in std::fs::read_dir(repo("sdk/js/examples")).unwrap() {
        let dir = e.unwrap().path();
        let o = vet(&root, &dir, None, &[]);
        status(&o, 0);
        assert!(o.stdout.contains(": vetted"), "{}", o.stdout);
    }
}

#[test]
fn the_app_s_fault_is_1() {
    let root = tmp("fault");
    let ok = root.join("ok");
    script_app(&ok);
    status(&vet(&root, &ok, None, &[]), 0);
    // A manifest problem.
    let m = root.join("manifest");
    script_app(&m);
    write(
        &m,
        &[(
            "ptyrc-app.toml",
            b"[app]\nid = \"sand\"\nscript = \"app.js\"\n",
        )],
    );
    status(&vet(&root, &m, None, &[]), 1);
    // A file that is not text.
    let b = root.join("binary");
    script_app(&b);
    write(&b, &[("logo.png", b"\x89PNG\r\n\x1a\n\0\0")]);
    let o = vet(&root, &b, None, &[]);
    status(&o, 1);
    assert!(
        o.stderr.contains("ptyrc: logo.png: not text"),
        "{}",
        o.stderr
    );
    assert!(o.stderr.contains("no image, no binary"), "{}", o.stderr);
    // A file the app does not use; one under tests/, and docs, are.
    let u = root.join("unused");
    script_app(&u);
    write(
        &u,
        &[
            ("old.js", b"export const y = 2;\n"),
            ("tests/a.args", b"--size 10x2\n"),
            ("README.md", b"# sand\n"),
        ],
    );
    let o = vet(&root, &u, None, &[]);
    status(&o, 1);
    assert!(
        o.stderr.contains("ptyrc: old.js: not used by the app\n"),
        "{}",
        o.stderr
    );
    assert!(o.stderr.contains("ptyrc dev push` leaves it out"));
    assert!(o.stderr.contains(": 1 problem"), "{}", o.stderr);
}

#[test]
fn usage_and_unreadable_are_not_1() {
    let root = tmp("usage");
    let app = root.join("crab");
    rust_app(&app);
    // A Rust app: --out required.
    status(&vet(&root, &app, None, &[]), 2);
    // No directory.
    status(&vet(&root, &root.join("nowhere"), None, &[]), 3);
    // No rustc: the builder's fault.
    let o = vet(
        &root,
        &app,
        Some(&root.join("app.wasm")),
        &[("PTYRC_RUSTC", "/nonexistent/rustc")],
    );
    status(&o, 3);
    assert!(
        o.stderr.contains("not vetted: /nonexistent/rustc"),
        "{}",
        o.stderr
    );
    assert!(!root.join("app.wasm").exists());
    // A rustc without the wasm target; one that builds nothing.
    let sysroot = root.join("sysroot");
    let fake = sysroot.join("bin/rustc");
    write(
        &sysroot,
        &[(
            "bin/rustc",
            format!(
                "#!/bin/sh\ncase \"$1\" in\n--print) echo {} ;;\n-vV) echo 'release: 1.96.0' ;;\n\
                 *) exit 1 ;;\nesac\n",
                sysroot.display()
            )
            .as_bytes(),
        )],
    );
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let rustc = [("PTYRC_RUSTC", fake.to_str().unwrap())];
    let o = vet(&root, &app, Some(&root.join("app.wasm")), &rustc);
    status(&o, 3);
    assert!(o.stderr.contains("no standard library for"), "{}", o.stderr);
    std::fs::create_dir_all(sysroot.join("lib/rustlib/wasm32-unknown-unknown/lib")).unwrap();
    let o = vet(&root, &app, Some(&root.join("app.wasm")), &rustc);
    status(&o, 3);
    assert!(o.stderr.contains("not vetted: "), "{}", o.stderr);
    assert!(!root.join("app.wasm").exists());
    if !has_rustc() {
        return;
    }
    // --out where nothing can be written.
    let o = vet(&root, &app, Some(&root.join("none/app.wasm")), &[]);
    status(&o, 3);
}

#[test]
fn rust_apps() {
    if !has_rustc() {
        return;
    }
    let root = tmp("rust");
    for e in std::fs::read_dir(repo("sdk/rust/examples")).unwrap() {
        let dir = e.unwrap().path();
        let o = vet(&root, &dir, Some(&root.join("m.wasm")), &[]);
        status(&o, 0);
    }
    // The module dev build writes.
    let app = root.join("crab");
    rust_app(&app);
    let (a, b) = (root.join("vet.wasm"), root.join("build.wasm"));
    status(&vet(&root, &app, Some(&a), &[]), 0);
    let built = ptyrc(
        &root,
        &[
            "dev",
            "build",
            app.to_str().unwrap(),
            "--out",
            b.to_str().unwrap(),
            "--force",
        ],
        &[],
    );
    status(&built, 0);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    // A file of `mod` and one of `include_str!` are used, under
    // `#[cfg(test)]` too; a stray `.rs` is not.
    let lib = std::fs::read_to_string(app.join("src/lib.rs")).unwrap();
    write(
        &app,
        &[
            (
                "src/lib.rs",
                format!("{lib}\nmod more;\n#[cfg(test)]\nmod tests;\n").as_bytes(),
            ),
            (
                "src/more.rs",
                b"pub const W: &str = include_str!(\"w.txt\");\n",
            ),
            ("src/w.txt", b"words\n"),
            (
                "src/tests.rs",
                b"#[test]\nfn t() { assert!(include_str!(\"t.txt\").len() > 0); }\n",
            ),
            ("src/t.txt", b"case\n"),
            ("src/old.rs", b"pub fn old() {}\n"),
        ],
    );
    let o = vet(&root, &app, Some(&a), &[]);
    status(&o, 1);
    assert!(
        o.stderr
            .contains("ptyrc: src/old.rs: not used by the app\n"),
        "{}",
        o.stderr
    );
    assert!(o.stderr.contains(": 1 problem"), "{}", o.stderr);
    std::fs::remove_file(app.join("src/old.rs")).unwrap();
    status(&vet(&root, &app, Some(&a), &[]), 0);
    // A compile error: the app's.
    write(&app, &[("src/more.rs", b"pub fn broken( {}\n")]);
    let o = vet(&root, &app, Some(&root.join("bad.wasm")), &[]);
    status(&o, 1);
    assert!(o.stderr.contains("the build failed"), "{}", o.stderr);
    assert!(!root.join("bad.wasm").exists());
}
