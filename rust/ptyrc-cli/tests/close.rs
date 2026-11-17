// SPDX-License-Identifier: GPL-3.0-or-later
//! The close key of a focused widget (ctrl+x unless named): the focus is
//! dropped, the widget hidden, its owner told; the key reaches nobody.

mod common;

use common::{Sandbox, TIMEOUT, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};

fn next(client: &mut Client, topic: &str) -> Value {
    loop {
        let event = client.next_event(Some(TIMEOUT)).unwrap().expect("an event");
        if event.event == topic {
            return event.data;
        }
    }
}

/// Declare, show and ask: the id of the widget.
fn asking(client: &mut Client, tag: &str, ask: Value) -> u64 {
    let decl = json!({"widgets": [{"tag": tag, "size": [12, 2]}]});
    client.call("widget.declare", decl).unwrap();
    let shown = client.call("widget.show", json!({"tag": tag})).unwrap();
    let id = shown["widget"]["id"].as_u64().unwrap();
    let mut params = ask;
    params["id"] = json!(id);
    client.call("widget.ask", params).unwrap();
    id
}

/// Type on the host keyboard of a headless session.
fn host(client: &mut Client, bytes: &[u8]) {
    let bytes = ptyrc_server::keys::base64_encode(bytes);
    let params = json!({"bytes": bytes, "host": true});
    client.call("input.send", params).unwrap();
}

fn screen(client: &mut Client) -> String {
    client.call("screen.text", json!({})).unwrap().to_string()
}

fn listed(client: &mut Client, id: u64) -> bool {
    let list = client.call("widget.list", json!({})).unwrap();
    let list = list["widgets"].as_array().unwrap().clone();
    list.iter().any(|w| w["id"] == json!(id))
}

#[test]
fn the_close_key_hides_the_focused_widget() {
    let sb = Sandbox::new("close");
    let _driver = sb.headless("c", "60x16", &["cat"]);
    let mut app = sb.client("c");
    let mut other = sb.client("c");

    // Bad keys; the close key is not the escape key.
    let decl = json!({"widgets": [{"tag": "w", "size": [12, 2]}]});
    app.call("widget.declare", decl).unwrap();
    let shown = app.call("widget.show", json!({"tag": "w"})).unwrap();
    let id = shown["widget"]["id"].as_u64().unwrap();
    for close in [json!("x"), json!(3), json!("esc")] {
        let err = app
            .call("widget.ask", json!({"id": id, "close": close}))
            .unwrap_err();
        assert!(err.to_string().contains("close"), "{err}");
    }
    let err = (app.call(
        "widget.ask",
        json!({"id": id, "escape": "f2", "close": "f2"}),
    ))
    .unwrap_err();
    assert!(err.to_string().contains("bad-params"), "{err}");
    app.call("widget.hide", json!({"id": id})).unwrap();

    // ctrl+x: told `close`, gone; what follows it is the app's.
    let first = asking(&mut app, "first", json!({}));
    app.subscribe(&["widgets"]).unwrap();
    other.call("widget.focus", json!({"id": first})).unwrap();
    host(&mut other, b"q\x18after");
    let data = next(&mut app, "input");
    assert_eq!(
        (&data["key"], &data["widget"]),
        (&json!("q"), &json!(first))
    );
    // Told, and the widget gone, in any order.
    let (mut told, mut gone) = (false, false);
    while !(told && gone) {
        let event = app.next_event(Some(TIMEOUT)).unwrap().expect("an event");
        match event.event.as_str() {
            "input" => {
                assert_eq!(event.data, json!({"widget": first, "released": "close"}));
                told = true;
            }
            "widgets" if event.data["id"] == json!(first) => {
                gone |= event.data["state"] == json!("gone");
            }
            _ => {}
        }
    }
    wait_for(
        || screen(&mut other).contains("after"),
        "the rest of the read",
    );
    let shown = screen(&mut other);
    assert!(!shown.contains('q') && !shown.contains("^X"), "{shown}");
    assert!(!listed(&mut other, first));
    let info = other.call("session.info", json!({})).unwrap();
    assert_eq!(info["focus"], Value::Null);
    // Declared still: shown again.
    app.call("widget.show", json!({"tag": "first"})).unwrap();

    // Named, in the kitty form, alone: its release event reaches nobody.
    let second = asking(&mut app, "second", json!({"close": "f10"}));
    other.call("widget.focus", json!({"id": second})).unwrap();
    host(&mut other, b"\x18");
    let data = next(&mut app, "input");
    assert_eq!(
        (&data["key"], &data["widget"]),
        (&json!("ctrl+x"), &json!(second))
    );
    host(&mut other, b"\x1b[21~");
    assert_eq!(
        next(&mut app, "input"),
        json!({"widget": second, "released": "close"})
    );
    let third = asking(&mut app, "third", json!({}));
    other.call("widget.focus", json!({"id": third})).unwrap();
    host(&mut other, b"\x1b[120;5u");
    assert_eq!(
        next(&mut app, "input"),
        json!({"widget": third, "released": "close"})
    );
    host(&mut other, b"\x1b[120;5:3uz");
    wait_for(|| screen(&mut other).contains("afterz"), "the app's z");
    assert!(!screen(&mut other).contains("120"));

    // false: none, ctrl+x is the widget's.
    let fourth = asking(&mut app, "fourth", json!({"close": false}));
    other.call("widget.focus", json!({"id": fourth})).unwrap();
    host(&mut other, b"\x18");
    assert_eq!(next(&mut app, "input")["key"], json!("ctrl+x"));
    assert!(listed(&mut other, fourth));
}
