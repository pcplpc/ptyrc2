// SPDX-License-Identifier: GPL-3.0-or-later
//! Interactive widgets: a widget asks, ctrl+t twice on the host keyboard
//! focuses it, its keys are its connection's; the escape key gives the
//! keyboard back; past the last widget the pair is the manager's.

mod common;

use common::{BIN, HostPty, Sandbox, TIMEOUT, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn next_input(client: &mut Client) -> Value {
    loop {
        let event = client.next_event(Some(TIMEOUT)).unwrap().expect("an event");
        if event.event == "input" {
            return event.data;
        }
    }
}

fn seen(host: &HostPty) -> String {
    String::from_utf8_lossy(&host.seen).into_owned()
}

/// Declare, show and ask: the id of the widget.
fn asking(client: &mut Client, tag: &str, ask: Value) -> u64 {
    let decl = json!({"widgets": [{"tag": tag, "size": [12, 2]}]});
    client.call("widget.declare", decl).unwrap();
    let shown = client.call("widget.show", json!({"tag": tag})).unwrap();
    let id = shown["widget"]["id"].as_u64().unwrap();
    let mut params = ask;
    params["id"] = json!(id);
    let asked = client.call("widget.ask", params).unwrap();
    assert_eq!(asked["widget"]["id"], json!(id));
    id
}

/// The raw frame is usable: a widget on it can be focused.
fn settled(client: &mut Client) {
    let usable = |client: &mut Client| {
        let frames = client.call("frame.list", json!({})).unwrap();
        frames["frames"][0]["usable"] == json!(true)
    };
    wait_for(|| usable(client), "a usable screen");
}

fn focus(client: &mut Client) -> Value {
    client.call("session.info", json!({})).unwrap()["focus"].clone()
}

#[test]
fn the_pair_focuses_the_widgets_that_ask_then_the_manager() {
    let sb = Sandbox::new("focus");
    let log = sb.dir.join("log");
    // A stand-in for the manager.
    let app = sb.dir.join("app");
    let script = format!("#!/bin/sh\necho menu >> {}\nexec sleep 60\n", log.display());
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hotkey = format!("PTYRC_HOTKEY={}", app.display());
    let (mut host, _driver) =
        HostPty::spawn(&sb, &["env", &hotkey, BIN, "run", "--name", "d", "cat"]);
    wait_for(|| sb.socket("d").exists(), "the session");

    let mut note = sb.client("d");
    let mut quest = sb.client("d");
    let mut other = sb.client("d");
    settled(&mut other);
    // The quiet one asks first: the loud one is before it all the same.
    let quiet = asking(&mut note, "note", json!({"quiet": true}));
    let loud = asking(&mut quest, "quest", json!({}));
    let info = |client: &mut Client, id: u64| -> Value {
        let list = client.call("widget.list", json!({})).unwrap();
        let list = list["widgets"].as_array().unwrap().clone();
        list.into_iter().find(|w| w["id"] == json!(id)).unwrap()
    };
    assert_eq!(info(&mut other, quiet)["asks"], json!("quiet"));
    assert_eq!(info(&mut other, loud)["asks"], json!("loud"));
    assert_eq!(focus(&mut other), Value::Null);

    // The pair: the loud one. Its keys are its connection's, not the app's.
    host.type_in(b"a");
    host.wait_seen("a");
    host.seen.clear();
    host.type_in(b"\x14\x14");
    wait_for(|| focus(&mut other) == json!(loud), "the focus");
    assert_eq!(info(&mut other, loud)["focused"], json!(true));
    host.type_in(b"x\r\x1b[A");
    for key in ["x", "enter", "up"] {
        let data = next_input(&mut quest);
        assert_eq!((&data["key"], &data["widget"]), (&json!(key), &json!(loud)));
        assert!(data["bytes"].is_string() && data.get("grabbed").is_none());
    }
    // A lone ctrl+t is the widget's, late.
    host.type_in(b"\x14");
    assert_eq!(next_input(&mut quest)["key"], json!("ctrl+t"));

    // No key grab while a widget is focused.
    let err = other.call("input.grab", json!({})).unwrap_err();
    assert!(err.to_string().contains("focused"), "{err}");

    // The pair again: the quiet one; the loud one is told.
    host.type_in(b"\x14\x14");
    let data = next_input(&mut quest);
    assert_eq!(data, json!({"widget": loud, "released": "hotkey"}));
    wait_for(|| focus(&mut other) == json!(quiet), "the next one");
    host.type_in(b"y");
    assert_eq!(next_input(&mut note)["key"], json!("y"));

    // The escape key: the keyboard is the app's again; it still asks.
    host.type_in(b"\x1b");
    let data = next_input(&mut note);
    assert_eq!(data, json!({"widget": quiet, "released": "escape"}));
    assert_eq!(focus(&mut other), Value::Null);
    assert_eq!(info(&mut other, quiet)["asks"], json!("quiet"));
    host.type_in(b"z");
    host.wait_seen("z");
    for typed in ["x", "y", "^T", "^["] {
        assert!(!seen(&host).contains(typed), "{:?}", seen(&host));
    }

    // Down the order, then past the last: the manager, no focus.
    host.type_in(b"\x14\x14");
    wait_for(|| focus(&mut other) == json!(loud), "the first again");
    host.type_in(b"\x14\x14");
    wait_for(|| focus(&mut other) == json!(quiet), "the second");
    assert!(!log.exists());
    host.type_in(b"\x14\x14");
    wait_for(|| log.exists(), "the manager");
    assert_eq!(focus(&mut other), Value::Null);
    assert_eq!(next_input(&mut quest)["released"], json!("hotkey"));
    assert_eq!(next_input(&mut note)["released"], json!("hotkey"));
    // The pair again ends it, as before.
    let managers = |client: &mut Client| -> usize {
        let apps = client.call("app.list", json!({})).unwrap();
        let apps = apps["apps"].as_array().unwrap().clone();
        apps.iter().filter(|a| a["name"] == "manage").count()
    };
    host.type_in(b"\x14\x14");
    wait_for(|| managers(&mut other) == 0, "the end of the manager");
}

#[test]
fn focus_by_request() {
    let sb = Sandbox::new("focus-rpc");
    let _driver = sb.headless("h", "60x16", &["cat"]);
    let mut app = sb.client("h");
    let mut other = sb.client("h");
    settled(&mut other);

    // Nothing asks.
    let none = other.call("widget.focus", json!({})).unwrap();
    assert_eq!(none["widget"], Value::Null);
    let out = sb.run(&["-s", "h", "focus"]);
    assert_eq!(out.status.code(), Some(1));

    // A bad escape key; one that is none.
    let decl = json!({"widgets": [{"tag": "plain", "size": [8, 1]}]});
    app.call("widget.declare", decl).unwrap();
    let plain = app.call("widget.show", json!({"tag": "plain"})).unwrap();
    let plain = plain["widget"]["id"].as_u64().unwrap();
    let err = app
        .call("widget.ask", json!({"id": plain, "escape": "a"}))
        .unwrap_err();
    assert!(err.to_string().contains("not a key"), "{err}");
    let err = other.call("widget.ask", json!({"id": plain})).unwrap_err();
    assert!(err.to_string().contains("no such widget"), "{err}");
    // One that does not ask.
    let err = other
        .call("widget.focus", json!({"id": plain}))
        .unwrap_err();
    assert!(err.to_string().contains("does not ask"), "{err}");

    let first = asking(&mut app, "first", json!({"escape": false}));
    let second = asking(&mut app, "second", json!({"quiet": true, "escape": "f12"}));
    // By any connection: the next, then the one named.
    let got = other.call("widget.focus", json!({})).unwrap();
    assert_eq!(got["widget"]["id"], json!(first));
    assert_eq!(got["widget"]["focused"], json!(true));
    let got = other.call("widget.focus", json!({"id": second})).unwrap();
    assert_eq!(got["widget"]["id"], json!(second));
    let data = next_input(&mut app);
    assert_eq!(data, json!({"widget": first, "released": "blur"}));
    // Past the last: none.
    let got = other.call("widget.focus", json!({})).unwrap();
    assert_eq!(got["widget"], Value::Null);
    assert_eq!(next_input(&mut app)["released"], json!("blur"));

    // The verb: by tag, by id, none.
    let out = sb.run(&["-s", "h", "focus", "second"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let line = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(line, format!("{second} second: focused\n"));
    assert_eq!(focus(&mut other), json!(second));
    let out = sb.run(&["-s", "h", "focus", &first.to_string()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(focus(&mut other), json!(first));
    let data = next_input(&mut app);
    assert_eq!(data, json!({"widget": second, "released": "blur"}));
    assert_eq!(
        sb.run(&["-s", "h", "focus", "plain"]).status.code(),
        Some(4)
    );
    assert_eq!(sb.run(&["-s", "h", "focus", "nope"]).status.code(), Some(4));
    let out = sb.run(&["-s", "h", "focus", "--none"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(focus(&mut other), Value::Null);
    let data = next_input(&mut app);
    assert_eq!(data, json!({"widget": first, "released": "blur"}));

    // Under a key grab the focus is not given.
    other.call("input.grab", json!({})).unwrap();
    let err = app.call("widget.focus", json!({"id": first})).unwrap_err();
    assert!(err.to_string().contains("holds the keys"), "{err}");
    other.call("input.release", json!({})).unwrap();

    // Ended by the app: no word of it; the widget released asks no more.
    app.call("widget.focus", json!({"id": first})).unwrap();
    app.call("widget.release", json!({"id": first})).unwrap();
    assert_eq!(focus(&mut other), Value::Null);
    let got = other.call("widget.focus", json!({})).unwrap();
    assert_eq!(got["widget"]["id"], json!(second));
    app.call("widget.hide", json!({"id": second})).unwrap();
    assert_eq!(focus(&mut other), Value::Null);
    app.subscribe(&["exit"]).unwrap();
    assert!(
        app.next_event(Some(std::time::Duration::from_millis(200)))
            .unwrap()
            .is_none()
    );
}
