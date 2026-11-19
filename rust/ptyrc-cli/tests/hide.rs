// SPDX-License-Identifier: GPL-3.0-or-later
//! The key that hides all: ctrl+t, then `h`, on the host keyboard.
//! Hidden, told apart, shown again; no client sees the key, nor
//! covers the names.

mod common;

use common::{BIN, HostPty, Sandbox, TIMEOUT};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const HIDE: &[u8] = b"\x14h";

/// What the host shows (the driver's model of it).
fn host_text(client: &mut Client) -> String {
    let dump = client.call("debug.dump", json!({})).unwrap();
    let lines = dump["host"].as_array().unwrap();
    let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    lines.join("\n")
}

fn until(host: &mut HostPty, mut ready: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        host.pump();
        if ready() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn hidden_told_apart_shown_and_never_an_apps() {
    let sb = Sandbox::new("hide");
    let (mut host, _driver) = HostPty::spawn(&sb, &[BIN, "run", "--name", "h", "cat"]);
    until(&mut host, || sb.socket("h").exists(), "the session");
    let mut client = sb.client("h");
    client
        .call("input.send", json!({"text": "plain text\n"}))
        .unwrap();
    // A sprite on top, and a fake name of the driver beside it.
    let at = |x: i64, y: i64| json!({"at": [x, y], "space": "view"});
    client
        .call(
            "draw.put",
            json!({"anchor": at(5, 3), "layer": "hud", "z": 1000,
                   "content": {"rows": ["ZZZZZZZZZZ"], "opaque": true}}),
        )
        .unwrap();
    until(
        &mut host,
        || host_text(&mut client).contains("ZZZZZZZZZZ"),
        "the sprite",
    );
    // Whoever has all the keys does not see this one.
    let mut grabber = sb.client("h");
    grabber.call("input.grab", json!({})).unwrap();

    // Hidden: the text alone.
    host.type_in(HIDE);
    until(
        &mut host,
        || !host_text(&mut client).contains('Z'),
        "hidden",
    );
    assert!(host_text(&mut client).contains("plain text"));
    // Drawn meanwhile: hidden too.
    client
        .call(
            "draw.put",
            json!({"anchor": at(5, 5), "layer": "hud", "content": {"rows": ["YYYY"]}}),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    host.pump();
    assert!(!host_text(&mut client).contains("YYYY"));

    // Told apart: the owner's name over what it drew, the driver's:
    // the name of the client, an app of its own.
    host.type_in(HIDE);
    until(
        &mut host,
        || host_text(&mut client).contains(" hide-"),
        "the names",
    );
    let shown = host_text(&mut client);
    let row: Vec<&str> = shown.lines().collect();
    assert!(row[3].starts_with("      hide-"), "{shown}");
    // A sprite over the name does not cover it.
    client
        .call(
            "draw.put",
            json!({"anchor": at(5, 3), "layer": "hud", "z": 2000,
                   "content": {"rows": ["XXXXXX"], "opaque": true}}),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    host.pump();
    let shown = host_text(&mut client);
    assert!(
        shown.lines().nth(3).unwrap().starts_with("      hide-"),
        "{shown}"
    );

    // Shown again.
    host.type_in(HIDE);
    until(
        &mut host,
        || host_text(&mut client).contains("XXXXXXZZZZ"),
        "shown again",
    );
    assert!(host_text(&mut client).contains("YYYY"));

    // The grab had none of it; it has what follows.
    host.type_in(b"q");
    let mut keys = Vec::new();
    while let Ok(Some(event)) = grabber.next_event(Some(Duration::from_millis(500))) {
        if event.event == "input" {
            keys.push(event.data["key"].as_str().unwrap_or_default().to_string());
        }
        if keys.contains(&"q".to_string()) {
            break;
        }
    }
    assert_eq!(keys, ["q"]);
    grabber.call("input.release", json!({})).unwrap();

    // `input.send {host}` is not the host keyboard: typed, as keys.
    client
        .call("input.send", json!({"text": "\u{14}h", "host": true}))
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    host.pump();
    assert!(host_text(&mut client).contains("XXXXXXZZZZ"));
    // `cat` had the client's alone.
    let text = client.call("screen.text", json!({})).unwrap();
    assert_eq!(text.to_string().matches("^Th").count(), 1, "{text}");
}
