// SPDX-License-Identifier: GPL-3.0-or-later
//! A Rust store app from end to end: a local repository, `ptyrc store
//! add` (the build by `rustc` alone), a headless session, `app.start
//! {store}`, the runner, the module. Skipped without `rustc` and its
//! wasm target, unless `PTYRC_REQUIRE_RUSTC=1`.

mod common;

use common::{BIN, Sandbox, wait_for};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";
const CONFETTI: &str = "../../sdk/rust/examples/confetti";

fn has_rustc() -> bool {
    match ptyrc_store::build::Toolchain::find() {
        Ok(_) => true,
        Err(e) if std::env::var_os("PTYRC_REQUIRE_RUSTC").is_some_and(|v| v == "1") => {
            panic!("PTYRC_REQUIRE_RUSTC=1: {e}")
        }
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            false
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "agent")
        .env("GIT_AUTHOR_EMAIL", "agent@example.org")
        .env("GIT_COMMITTER_NAME", "agent")
        .env("GIT_COMMITTER_EMAIL", "agent@example.org")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

/// A bare repository holding `files`, committed.
fn repository(dir: &Path, files: &[(&str, String)]) -> PathBuf {
    let work = dir.join("work");
    for (name, text) in files {
        let p = work.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    git(&work, &["init", "-q", "-b", "main"]);
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "an app"]);
    let bare = dir.join("app.git");
    let out = Command::new("git")
        .args(["clone", "-q", "--bare"])
        .arg(&work)
        .arg(&bare)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap();
    assert!(out.success());
    bare
}

fn runner() -> PathBuf {
    let path = Path::new(BIN).with_file_name("ptyrc-box");
    if !path.is_file() {
        let built = Command::new(env!("CARGO"))
            .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
            .status()
            .unwrap();
        assert!(built.success(), "cargo build -p ptyrc-box");
    }
    path
}

fn confetti() -> Vec<(&'static str, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(CONFETTI);
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    vec![
        ("ptyrc-app.toml", read("ptyrc-app.toml")),
        ("Cargo.toml", read("Cargo.toml")),
        ("src/lib.rs", read("src/lib.rs")),
        ("LICENSE", GPL.to_string()),
    ]
}

#[test]
fn a_rust_app_from_a_repository_to_the_screen() {
    if !has_rustc() {
        return;
    }
    let sb = Sandbox::new("storerust");
    let bare = repository(&sb.dir.join("confetti"), &confetti());
    let origin = format!("local:{}", bare.display());
    let out = sb.run(&["store", "add", &origin, "--yes"]);
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "store add: {said}");
    assert!(said.contains("building confetti with rustc"), "{said}");
    if cfg!(target_os = "linux") {
        assert!(said.contains("built under the profile: landlock"), "{said}");
    }
    // The module beside the sources; the work directory gone.
    let apps = sb.dir.join("data/ptyrc/apps");
    let module = walk(&apps).into_iter().find(|p| p.ends_with("app.wasm"));
    let module = module.expect("app.wasm installed");
    assert!(std::fs::read(&module).unwrap().starts_with(b"\0asm"));
    let build = sb.dir.join("cache/ptyrc/build");
    assert!(std::fs::read_dir(&build).map_or(true, |mut d| d.next().is_none()));

    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .args(["run", "--name", "s", "--headless", "--size", "60x16"])
        .args(["--", "cat"])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("s");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("s");
    let started = client
        .call(
            "app.start",
            json!({"store": "confetti", "args": {"bursts": 3}}),
        )
        .unwrap();
    let id = started["app"]["id"].as_u64().unwrap();
    assert_eq!(started["app"]["name"], "confetti");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let apps = client.call("app.list", json!({})).unwrap();
        let me = (apps["apps"].as_array().unwrap().iter()).find(|a| a["id"] == id);
        let me = me.expect("the app is in the table").clone();
        assert_ne!(me["state"], "exited", "{me}");
        if me["sprites"].as_u64().unwrap_or(0) > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "confetti draws: {me}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The rule, at the install: a dependency refused, the rule named.
#[test]
fn a_rust_app_with_a_dependency_is_refused() {
    if !has_rustc() {
        return;
    }
    let sb = Sandbox::new("storerust-dep");
    let mut files = confetti();
    files[1].1 = files[1].1.replace("[dependencies]\n", "[dependencies]\nserde = \"1\"\n");
    let bare = repository(&sb.dir.join("dep"), &files);
    let out = sb.run(&[
        "store",
        "add",
        &format!("local:{}", bare.display()),
        "--yes",
    ]);
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success());
    assert!(
        said.contains("[dependencies] serde: a dependency"),
        "{said}"
    );
    assert!(said.contains("built by rustc alone"), "{said}");
    assert!(!sb.dir.join("data/ptyrc/apps.lock").exists());
}

/// Without `rustc`: the install says how to get it.
#[test]
fn without_rustc_the_install_says_how() {
    let sb = Sandbox::new("storerust-none");
    let bare = repository(&sb.dir.join("none"), &confetti());
    let out = sb
        .ptyrc()
        .args([
            "store",
            "add",
            &format!("local:{}", bare.display()),
            "--yes",
        ])
        .env("PTYRC_RUSTC", "/nonexistent/rustc")
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success());
    assert!(said.contains("/nonexistent/rustc"), "{said}");
    assert!(said.contains("https://rustup.rs"), "{said}");
}
