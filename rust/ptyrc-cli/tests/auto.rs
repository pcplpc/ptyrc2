// SPDX-License-Identifier: GPL-3.0-or-later
//! `-s auto`: the session the user typed in last.

mod common;

use common::{BIN, HostPty, Sandbox, wait_for};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::process::Output;

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The session `-s auto` talks to.
fn chosen(sb: &Sandbox) -> String {
    let out = sb.run(&["-s", "auto", "--json", "call", "session.info"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let info: Value = serde_json::from_slice(&out.stdout).unwrap();
    info["session"].as_str().unwrap().to_string()
}

fn active_at(sb: &Sandbox, name: &str) -> Option<u64> {
    sb.client(name).hello()["active_at"].as_u64()
}

/// Type on the host keyboard of `name`, until the driver has read it.
fn type_in(sb: &Sandbox, host: &mut HostPty, name: &str, bytes: &[u8]) {
    let before = active_at(sb, name);
    // A date of its own: a millisecond after the last one.
    std::thread::sleep(std::time::Duration::from_millis(3));
    host.type_in(bytes);
    wait_for(|| active_at(sb, name) != before, "the keys read");
    host.pump();
}

#[test]
fn none_one_and_untouched_sessions() {
    let sb = Sandbox::new("auto-none");
    let out = sb.run(&["-s", "auto", "text"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).contains("no session"), "{}", stderr(&out));

    // The only one, typed in or not.
    let _h = sb.headless("h", "40x10", &["cat"]);
    assert_eq!(chosen(&sb), "h");
    assert_eq!(sb.client("h").hello()["active_at"], json!(null));

    // Several, nobody at any keyboard: no choice.
    let _g = sb.headless("g", "40x10", &["cat"]);
    let out = sb.run(&["-s", "auto", "text"]);
    assert_eq!(out.status.code(), Some(3));
    let said = stderr(&out);
    assert!(
        said.contains("2 sessions, the user typed in none"),
        "{said}"
    );
    assert!(said.contains("pick one with -s NAME: g, h"), "{said}");
}

#[test]
fn the_session_typed_in_last() {
    let sb = Sandbox::new("auto-typed");
    let (mut a, _da) = HostPty::spawn(&sb, &[BIN, "run", "--name", "a", "cat"]);
    let (mut b, _db) = HostPty::spawn(&sb, &[BIN, "run", "--name", "b", "cat"]);
    let _h = sb.headless("h", "40x10", &["cat"]);
    for name in ["a", "b"] {
        wait_for(|| sb.socket(name).exists(), "the session");
    }

    // An answer of the terminal is not the user.
    a.type_in(b"\x1b[12;40R\x1b[?62;22c\x1b[I");
    a.wait_seen("62;22c");
    assert_eq!(active_at(&sb, "a"), None);
    assert_eq!(sb.run(&["-s", "auto", "text"]).status.code(), Some(3));

    type_in(&sb, &mut a, "a", b"x");
    assert_eq!(chosen(&sb), "a");
    type_in(&sb, &mut b, "b", b"y");
    assert_eq!(chosen(&sb), "b");
    type_in(&sb, &mut a, "a", b"\x1b[A");
    assert_eq!(chosen(&sb), "a");

    // A client that types is not the user.
    sb.client("b")
        .call("input.send", json!({"text": "z"}))
        .unwrap();
    sb.client("h")
        .call("input.send", json!({"text": "z", "host": true}))
        .unwrap();
    assert_eq!(chosen(&sb), "a");

    // The name in the environment.
    let out = sb
        .ptyrc()
        .env("PTYRC_SESSION", "auto")
        .args(["--json", "call", "session.info"])
        .output()
        .unwrap();
    let info: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(info["session"], "a");

    // `ls`: the date, and the mark.
    let out = sb.run(&["ls"]);
    let table = String::from_utf8_lossy(&out.stdout).into_owned();
    let marked: Vec<&str> = table.lines().filter(|l| l.starts_with('*')).collect();
    assert_eq!(marked.len(), 1, "{table}");
    assert!(marked[0].starts_with("* a "), "{table}");
    assert!(table.lines().next().unwrap().contains("ACTIVE"), "{table}");
}

#[test]
fn an_app_keeps_the_session_chosen() {
    let sb = Sandbox::new("auto-app");
    let (mut a, _da) = HostPty::spawn(&sb, &[BIN, "run", "--name", "a", "cat"]);
    let _h = sb.headless("h", "40x10", &["cat"]);
    wait_for(|| sb.socket("a").exists(), "the session");

    // A stand-in for `ptyrc-app`: it tells its session.
    let bin = sb.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let app = bin.join("ptyrc-app");
    std::fs::write(&app, "#!/bin/sh\necho \"${PTYRC_SESSION-none} $*\"\n").unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let told = |args: &[&str], env: &[(&str, &str)]| {
        let mut ptyrc = sb.ptyrc();
        ptyrc.env("PATH", &path).args(args);
        ptyrc.envs(env.iter().copied());
        String::from_utf8_lossy(&ptyrc.output().unwrap().stdout).into_owned()
    };

    // Typed in by nobody: the program chooses, as before.
    assert_eq!(told(&["app", "bricks"], &[]), "none bricks\n");
    type_in(&sb, &mut a, "a", b"x");
    assert_eq!(told(&["app", "-s", "auto", "bricks"], &[]), "a bricks\n");
    // No session chosen, several: an app goes where the user typed.
    assert_eq!(told(&["app", "bricks", "-x"], &[]), "a bricks -x\n");
    // A choice stands; no app named: nothing is chosen.
    assert_eq!(told(&["app", "-s", "h", "bricks"], &[]), "h bricks\n");
    let env = [("PTYRC_SESSION", "h")];
    assert_eq!(told(&["app", "bricks"], &env), "h bricks\n");
    let env = [("PTYRC_SOCKET", "/nowhere")];
    assert_eq!(told(&["app", "bricks"], &env), "none bricks\n");
    assert_eq!(told(&["app"], &[]), "none \n");
    assert_eq!(told(&["app", "--help"], &[]), "none --help\n");

    // The other verbs keep to a name.
    let out = sb.run(&["text"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).contains("2 sessions, none chosen"));
}

#[test]
fn auto_is_no_name_of_a_session() {
    let sb = Sandbox::new("auto-name");
    let out = sb.run(&[
        "run",
        "--name",
        "auto",
        "--headless",
        "--size",
        "40x10",
        "cat",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("bad session name: auto"));
    assert!(!sb.socket("auto").exists());
}
