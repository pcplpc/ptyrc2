// SPDX-License-Identifier: GPL-3.0-or-later
//! Tapes: record, replay, and the recorded sessions of real TUIs held to
//! their snapshots (`scripts/record-tapes.sh` writes both).

mod common;

use common::*;
use serde_json::{Value, json};
use std::path::Path;

fn tape(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/tapes")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn check(sb: &Sandbox, name: &str) {
    let out = sb.run(&[
        "replay",
        &tape(&format!("{name}.tape")),
        "--check",
        &tape(&format!("{name}.snap")),
    ]);
    assert!(
        out.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `less`: pages, lines, a page back, the end, the top.
#[test]
fn pager_tape_keeps_its_frames() {
    let sb = Sandbox::new("tape-less");
    check(&sb, "less");
    // What the snapshots say, in words.
    let shots: Value =
        serde_json::from_str(&std::fs::read_to_string(tape("less.snap")).unwrap()).unwrap();
    let pane = |i: usize| {
        shots[i]["frames"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone()
    };
    let origins: Vec<i64> = (0..7)
        .map(|i| pane(i)["origin"][1].as_i64().unwrap())
        .collect();
    assert_eq!(
        origins[..5],
        [0, 15, 30, 33, 18],
        "pages and lines are scrolls"
    );
    assert!(origins[5] > 1000, "G is a jump: an island");
    assert_eq!(origins[6], 0, "g: the first page is found again");
    // The rows of the file; the prompt row is chrome.
    assert_eq!(pane(6)["extent"], json!([0, 47]));
}

/// tmux: a split, a second window, back to the first.
#[test]
fn tmux_tape_keeps_its_frames() {
    let sb = Sandbox::new("tape-tmux");
    check(&sb, "tmux");
    let shots: Value =
        serde_json::from_str(&std::fs::read_to_string(tape("tmux.snap")).unwrap()).unwrap();
    let usable = |i: usize| -> Vec<Value> {
        let frames = shots[i]["frames"].as_array().unwrap();
        let panes = frames
            .iter()
            .filter(|f| f["kind"] == "pane" && f["usable"] == true);
        panes
            .map(|f| json!([f["rect"], f["origin"], f["key"]]))
            .collect()
    };
    let split = json!([
        [[0, 0, 40, 23], [0, 19], "tmux:tape:0"],
        [[41, 0, 39, 23], [0, 0], "tmux:tape:0"]
    ]);
    assert_eq!(json!(usable(3)), split);
    assert_eq!(
        json!(usable(4)),
        json!([[[0, 0, 80, 23], [0, 0], "tmux:tape:1"]])
    );
    assert_eq!(
        json!(usable(6)),
        split,
        "the first window comes back as it was"
    );
}

#[test]
fn a_changed_snapshot_is_told() {
    let sb = Sandbox::new("tape-diff");
    let text = std::fs::read_to_string(tape("less.snap")).unwrap();
    let changed = sb.dir.join("changed.snap");
    std::fs::write(
        &changed,
        text.replace("\"line number 31\"", "\"line number 13\""),
    )
    .unwrap();
    let out = sb.run(&[
        "replay",
        &tape("less.tape"),
        "--check",
        changed.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("text[0]: expected \"line number 13\", got \"line number 31\""),
        "{err}"
    );

    let out = sb.run(&["replay", "/no/such.tape"]);
    assert_eq!(out.status.code(), Some(2));
    let out = sb.run(&["replay", &changed.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("not a tape") && err.contains("ptyrc run --record"),
        "{err}"
    );
}

#[test]
fn record_then_replay() {
    let sb = Sandbox::new("tape-rec");
    let file = sb.dir.join("s.tape");
    let file = file.to_str().unwrap();
    let script =
        "printf 'he\\342'; sleep 0.1; printf '\\224\\202llo\\n'; read x; echo got $x; exit 3";
    let mut driver = sb.spawn(
        "r",
        &[
            "run",
            "--name",
            "r",
            "--headless",
            "--size",
            "40x5",
            "--record",
            file,
            "--record-input",
            "--",
            "sh",
            "-c",
            script,
        ],
    );
    let mut client = sb.client("r");
    client
        .call("screen.wait", json!({"pattern": "llo", "timeout_ms": 5000}))
        .unwrap();
    client
        .call("session.resize", json!({"size": [30, 4]}))
        .unwrap();
    client.call("input.send", json!({"text": "key\r"})).unwrap();
    assert_eq!(driver.wait_exit(), 3);

    let text = std::fs::read_to_string(file).unwrap();
    assert!(
        text.lines().next().unwrap().contains("\"size\":[40,5]"),
        "{text}"
    );
    assert!(
        text.contains("\"size\":[30,4]") && text.contains("\"i\":\"key\\r\""),
        "{text}"
    );
    assert!(
        text.contains("\"ob\":") && text.contains("\"exit\":3"),
        "{text}"
    );

    // The tape as a session: the same screen, the same end.
    let out = sb.run(&["replay", file, "--snap", "--at", "end"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shots: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(shots[0]["size"], json!([30, 4]));
    assert_eq!(shots[0]["text"][0], "he│llo");
    let out = sb.run(&["replay", file, "--speed", "0"]);
    assert_eq!(out.status.code(), Some(3), "the status of the tape");
}

#[test]
fn a_replay_is_piloted() {
    let sb = Sandbox::new("tape-ctl");
    let mut replay = sb.spawn(
        "p",
        &["replay", &tape("less.tape"), "--name", "p", "--step"],
    );
    let mut client = sb.client("p");
    let status = client.call("tape.status", json!({})).unwrap();
    assert_eq!(
        (status["paused"].clone(), status["next"].clone()),
        (json!(true), json!(0))
    );
    assert_eq!(
        client.call("screen.text", json!({})).unwrap()["lines"][0],
        ""
    );

    // One record: the first page. Client verbs work as on a live session.
    let status = client.call("tape.step", json!({})).unwrap();
    assert_eq!(status["next"], 1);
    let found = client
        .call("screen.find", json!({"pattern": "number 7"}))
        .unwrap();
    assert_eq!(found["matches"][0]["screen"], json!([5, 6]));
    let info = client.call("session.info", json!({})).unwrap();
    assert_eq!(info["tape"]["next"], 1);
    assert_eq!(info["argv"], json!(["less", "numbers.txt"]));

    // To a time: the clock stops there, exactly.
    let status = client.call("tape.seek", json!({"t": 6.0})).unwrap();
    assert_eq!(
        (status["t"].clone(), status["paused"].clone()),
        (json!(6.0), json!(true))
    );
    assert_eq!(
        client.call("screen.text", json!({})).unwrap()["lines"][0],
        "line number 31"
    );
    let e = client.call("tape.seek", json!({"t": 1.0})).unwrap_err();
    assert!(e.to_string().contains("a tape goes forward"), "{e}");
    let e = client.call("input.send", json!({"text": "x"})).unwrap_err();
    assert!(e.to_string().contains("a replay takes no input"), "{e}");
    let e = client.call("tape.rewind", json!({})).unwrap_err();
    assert!(e.hint().unwrap().contains("tape.seek"), "{e}");

    // The CLI.
    let out = sb.run(&["-s", "p", "tape"]);
    let line = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        line.starts_with("6.000 / ") && line.trim_end().ends_with("paused"),
        "{line}"
    );
    let out = sb.run(&["-s", "p", "tape", "seek", "end+1"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("record 21/21"));

    // Played to its end, the replay leaves with the tape's status.
    client.subscribe(&["exit"]).unwrap();
    client.call("tape.play", json!({"speed": 0})).unwrap();
    let event = client.next_event(Some(TIMEOUT)).unwrap().unwrap();
    assert_eq!(event.event, "exit");
    assert_eq!(replay.wait_exit(), 0);
    assert!(!sb.socket("p").exists());

    // A live session has no tape.
    let _live = sb.headless("l", "20x4", &["cat"]);
    let e = sb.client("l").call("tape.status", json!({})).unwrap_err();
    assert!(e.to_string().contains("not a replay"), "{e}");
}
