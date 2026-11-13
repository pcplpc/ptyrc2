// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu of `ptyrc manage` in the session: ctrl+t twice on the host
//! keyboard, the keys of the menu, the way out.

mod common;

use common::{BIN, HostPty, Sandbox, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

/// What the host shows, sprites included.
fn shown(client: &mut Client) -> String {
    let text = client
        .call("screen.text", json!({"sprites": true}))
        .unwrap();
    let lines = text["lines"].as_array().unwrap();
    let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    lines.join("\n")
}

fn apps(client: &mut Client) -> Vec<Value> {
    let apps = client.call("app.list", json!({})).unwrap();
    apps["apps"].as_array().unwrap().clone()
}

fn app(client: &mut Client, name: &str) -> Option<Value> {
    apps(client).into_iter().find(|a| a["name"] == name)
}

fn grabbed(client: &mut Client) -> bool {
    client.call("session.info", json!({})).unwrap()["grabbed"] == true
}

fn widgets(client: &mut Client) -> Vec<Value> {
    let list = client.call("widget.list", json!({})).unwrap();
    list["widgets"].as_array().unwrap().clone()
}

#[test]
fn the_menu_in_the_session() {
    let sb = Sandbox::new("overlay");
    // A `ptyrc-app` with one app, that sleeps.
    let bin = sb.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let catalog = r#"{"apps":[{"name":"nap","help":"sleeps","options":[{"flags":["-l","--long"],"kind":"flag","metavar":null,"choices":null,"default":false,"help":"longer"}],"positionals":[],"random":[[]]}]}"#;
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --catalog ]; then echo '{catalog}'; exit 0; fi\nexec sleep 60\n"
    );
    let program = bin.join("ptyrc-app");
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "PATH={}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let (mut host, _driver) = HostPty::spawn(
        &sb,
        &[
            "env",
            "-u",
            "PTYRC_HOTKEY",
            &path,
            BIN,
            "run",
            "--name",
            "o",
            "cat",
        ],
    );
    wait_for(|| sb.socket("o").exists(), "the session");
    // What the driver draws is read as it comes: a full pty would hold it.
    let drain = host.master.try_clone().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        loop {
            match rustix::io::read(&drain, &mut buf) {
                Ok(0) | Err(rustix::io::Errno::IO) => break,
                Ok(_) => {}
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(2)),
            }
        }
    });
    let mut client = sb.client("o");
    let c = &mut client;

    // ctrl+t twice: the menu, modal, in a widget; the keyboard is its own.
    host.type_in(b"\x14\x14");
    wait_for(|| shown(c).contains("apps of o"), "the menu");
    wait_for(|| grabbed(c), "the grab");
    let screen = shown(c);
    assert!(screen.contains("+ start an app…"), "{screen}");
    assert!(screen.contains("esc leave"), "{screen}");
    let [widget] = &widgets(c)[..] else {
        panic!("one widget: {:?}", widgets(c));
    };
    assert_eq!(
        (&widget["tag"], &widget["modal"]),
        (&json!("manage"), &json!(true))
    );
    assert_eq!(widget["state"], "placed");
    // The manager is not a row of its own list.
    assert!(!screen.contains("● manage"), "{screen}");

    // Start: the list of the catalog, the form, enter.
    host.type_in(b"s");
    wait_for(|| shown(c).contains("start an app"), "the start list");
    assert!(shown(c).contains("nap  sleeps"), "{}", shown(c));
    host.type_in(b"\r");
    wait_for(|| shown(c).contains("▶ start nap"), "the form");
    // A flag, by the arrows and space.
    host.type_in(b"\x1b[B");
    host.type_in(b" ");
    wait_for(|| shown(c).contains("[x] -l, --long"), "the flag");
    host.type_in(b"\x1b[A");
    host.type_in(b"\r");
    wait_for(|| app(c, "nap").is_some(), "the app");
    let nap = app(c, "nap").unwrap();
    assert_eq!(
        (&nap["kind"], &nap["state"]),
        (&json!("child"), &json!("running"))
    );
    let argv: Vec<&str> = nap["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(argv[1..], ["nap", "--long"]);
    wait_for(|| shown(c).contains("● nap --long"), "its row");
    // The apps started here come first: this test, a client that reads
    // the screen, is a row below.
    let screen = shown(c);
    assert!(
        screen.find("● nap").unwrap() < screen.find("(pid ").unwrap(),
        "{screen}"
    );

    // Pause and resume, by the key; then by the actions of the row.
    host.type_in(b"p");
    wait_for(|| app(c, "nap").unwrap()["state"] == "paused", "the pause");
    wait_for(|| shown(c).contains("‖ nap --long"), "its mark");
    host.type_in(b"\r");
    wait_for(|| shown(c).contains("resume"), "the actions");
    host.type_in(b"\r");
    wait_for(
        || app(c, "nap").unwrap()["state"] == "running",
        "the resume",
    );

    // esc: all is as before, but for the app started; `cat` saw no key.
    host.type_in(b"\x1b");
    wait_for(|| widgets(c).is_empty(), "the widget gone");
    wait_for(|| !grabbed(c), "the grab gone");
    wait_for(|| app(c, "manage").is_none(), "the manager gone");
    assert!(app(c, "nap").is_some());
    let text = c.call("screen.text", json!({})).unwrap();
    let typed: String = text["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(typed.trim(), "", "the app saw keys");

    // Again; stop the app from the menu; ctrl+t leaves too.
    host.type_in(b"\x14\x14");
    wait_for(|| shown(c).contains("● nap --long"), "the menu again");
    wait_for(|| grabbed(c), "the grab again");
    host.type_in(b"x");
    wait_for(
        || app(c, "nap").is_none_or(|a| a["state"] == "exited"),
        "the stop",
    );
    host.type_in(b"\x14");
    wait_for(|| widgets(c).is_empty() && !grabbed(c), "the menu left");
    // The keyboard is the app's again.
    host.type_in(b"z\r");
    wait_for(
        || c.call("screen.text", json!({})).unwrap()["lines"][0] == "z",
        "the key",
    );
}
