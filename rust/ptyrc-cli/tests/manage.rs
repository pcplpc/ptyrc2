// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc manage` and `app.*` on a headless session, with a stand-in
//! client app.

mod common;

use common::{Sandbox, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::process::{Command, Output, Stdio};

/// A client app: one sprite, the topic `text` (and `move` when asked),
/// every line it is sent noted in the file `$LOG.<pid>`.
const APP: &str = r#"
import json, os, socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(os.environ["PTYRC_SOCKET"])
topics = ["text"] + (["move"] if "--moves" in sys.argv else [])
def send(n, method, params):
    s.sendall((json.dumps({"id": n, "method": method, "params": params}) + "\n").encode())
send(1, "draw.put", {"content": {"rows": ["HELLO"]}, "layer": "front",
                     "anchor": {"at": [2, 2], "space": "view"}})
send(2, "subscribe", {"topics": topics})
with open(os.environ["LOG"] + "." + str(os.getpid()), "a") as log:
    log.write(" ".join(sys.argv[1:]) + " app=" + os.environ.get("PTYRC_APP", "") + "\n")
    log.flush()
    for line in s.makefile():
        log.write(line)
        log.flush()
"#;

fn out(output: &Output) -> (i32, String, String) {
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn apps(client: &mut Client) -> Vec<Value> {
    let listed = client.call("app.list", json!({})).unwrap();
    let apps = listed["apps"].as_array().unwrap().clone();
    apps.into_iter().collect()
}

fn app(client: &mut Client, id: u64) -> Value {
    let found = apps(client).into_iter().find(|a| a["id"] == id);
    found.unwrap_or(Value::Null)
}

fn shown(client: &mut Client) -> String {
    let text = client
        .call("screen.text", json!({"sprites": true}))
        .unwrap();
    let lines = text["lines"].as_array().unwrap();
    let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    lines.join("\n")
}

#[test]
fn apps_are_started_paused_moved_restarted_stopped() {
    let sb = Sandbox::new("manage");
    let script = sb.dir.join("app.py");
    std::fs::write(&script, APP).unwrap();
    let script = script.to_str().unwrap();
    let log = sb.dir.join("log");
    // The children of the driver inherit its environment.
    let mut driver = sb.ptyrc();
    driver
        .env("LOG", &log)
        .args([
            "run",
            "--name",
            "m",
            "--headless",
            "--size",
            "40x10",
            "--",
            "cat",
        ])
        .stdin(Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    wait_for(|| sb.socket("m").exists(), "the session");
    wait_for(|| Client::connect(&sb.socket("m")).is_ok(), "the socket");
    let mut client = sb.client("m");
    // This test is a client: the app 1, from its first request.
    client.call("session.info", json!({})).unwrap();
    let manage = |args: &[&str]| {
        let mut all = vec!["-s", "m", "manage"];
        all.extend(args);
        out(&sb.run(&all))
    };
    let log_of = |pid: &Value| {
        std::fs::read_to_string(format!("{}.{pid}", log.display())).unwrap_or_default()
    };

    // Nothing yet, but this test. A verb of `manage` is no app.
    let (status, stdout, stderr) = manage(&["ls"]);
    assert_eq!((status, stdout.lines().count()), (0, 2), "{stderr}");
    assert!(
        stdout
            .lines()
            .nth(1)
            .unwrap()
            .starts_with(" 1  client  running")
            || stdout
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("1  client  running")
    );

    // Started: a child, its sprite on the raw frame.
    let (status, stdout, stderr) = manage(&["start", "--cmd", "python3", script, "one"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "2 python3: running\n"),
        "{stderr}"
    );
    wait_for(|| app(&mut client, 2)["sprites"] == 1, "the sprite");
    let info = app(&mut client, 2);
    assert_eq!(info["kind"], "child");
    assert_eq!(info["frames"], json!([0]));
    assert_eq!(info["moves"], false);
    assert_eq!(info["conns"].as_array().unwrap().len(), 1);
    let first = info["pid"].clone();
    wait_for(|| shown(&mut client).contains("HELLO"), "the drawing");
    wait_for(|| log_of(&first).contains("\"id\":2"), "the subscription");
    let (_, stdout, _) = manage(&["ls"]);
    let mut lines = stdout.lines();
    assert_eq!(
        lines.next().unwrap().split_whitespace().collect::<Vec<_>>(),
        ["ID", "KIND", "STATE", "SINCE", "PID", "NAME", "ARGS"]
    );
    let row: Vec<&str> = lines.nth(1).unwrap().split_whitespace().collect();
    assert_eq!(
        (row[0], row[1], row[2], row[5]),
        ("2", "child", "running", "python3")
    );
    assert!(row.last() == Some(&"one") && lines.next().is_none());
    let (_, stdout, _) = manage(&["frames"]);
    let raw = stdout.lines().nth(1).unwrap();
    assert!(
        raw.trim_start().starts_with("0") && raw.ends_with("python3"),
        "{stdout}"
    );

    // Paused: hidden, and no event; resumed: shown, the text told whole.
    client
        .call("subscribe", json!({"topics": ["apps"]}))
        .unwrap();
    let (status, stdout, _) = manage(&["pause", "python3"]);
    assert_eq!((status, stdout.as_str()), (0, "2 python3: paused\n"));
    wait_for(
        || !shown(&mut client).contains("HELLO"),
        "the drawing hidden",
    );
    let before = log_of(&first);
    client
        .call("input.send", json!({"text": "typed\n"}))
        .unwrap();
    wait_for(|| shown(&mut client).contains("typed"), "the echo");
    let (status, stdout, _) = manage(&["unpause", "2"]);
    assert_eq!((status, stdout.as_str()), (0, "2 python3: running\n"));
    wait_for(|| shown(&mut client).contains("HELLO"), "the drawing back");
    wait_for(
        || log_of(&first).len() > before.len(),
        "the events of the resume",
    );
    let after = log_of(&first);
    let told: Vec<Value> = after[before.len()..]
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // One event per frame on screen; nothing of what was typed meanwhile.
    assert!(told.iter().all(|e| e["event"] == "text"), "{told:?}");
    assert_eq!(told[0]["data"]["frame"], 0);
    assert_eq!(
        told[0]["data"]["rows"],
        json!((0..10).collect::<Vec<i64>>())
    );
    // The topic `apps` told both.
    let mut states = Vec::new();
    while states.len() < 2 {
        let event = client.next_event(Some(common::TIMEOUT)).unwrap().unwrap();
        if event.event == "apps" && event.data["id"] == 2 {
            let state = event.data["state"].clone();
            if states.last() != Some(&state) {
                states.push(state);
            }
        }
    }
    assert_eq!(states, ["paused", "running"]);

    // It does not move by itself: moved by a restart, `-f` in its argv.
    let (status, stdout, stderr) = manage(&["move", "2", "raw"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "2 python3: running\n"),
        "{stderr}"
    );
    let info = app(&mut client, 2);
    assert_eq!(info["runs"], 2);
    assert_ne!(info["pid"], first);
    let argv: Vec<&str> = info["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(argv[2..], ["one", "-f", "raw"]);
    // Restarted with other options.
    let (status, _, stderr) = manage(&["restart", "2", script, "two", "--moves"]);
    assert_eq!(status, 0, "{stderr}");
    wait_for(
        || app(&mut client, 2)["moves"] == true,
        "the app that moves",
    );
    let info = app(&mut client, 2);
    assert_eq!(info["runs"], 3);
    let second = info["pid"].clone();
    assert!(log_of(&second).starts_with("two --moves app=2\n"));
    // Now it moves by itself: told, not restarted.
    let (status, stdout, stderr) = manage(&["move", "2", "raw"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "2 python3: frame 0\n"),
        "{stderr}"
    );
    wait_for(
        || {
            let log = log_of(&second);
            let mut events = log
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok());
            events.any(|e| e == json!({"event": "move", "data": {"frame": 0}}))
        },
        "the move",
    );
    assert_eq!(app(&mut client, 2)["runs"], 3);
    let (status, _, stderr) = manage(&["move", "2", "nowhere"]);
    assert_eq!(status, 4, "{stderr}");
    assert!(stderr.contains("no such frame: nowhere"), "{stderr}");

    // A client from elsewhere: listed, paused, stopped.
    let mut outside = Command::new("python3");
    outside
        .args([script, "out"])
        .env("PTYRC_SOCKET", sb.socket("m"))
        .env("LOG", &log)
        .stdin(Stdio::null());
    let mut outside = outside.spawn().unwrap();
    let pid = outside.id();
    wait_for(
        || {
            apps(&mut client)
                .iter()
                .any(|a| a["pid"] == pid && a["sprites"] == 1)
        },
        "the client",
    );
    let info = apps(&mut client)
        .into_iter()
        .find(|a| a["pid"] == pid)
        .unwrap();
    assert_eq!(
        (&info["kind"], &info["log"]),
        (&json!("client"), &Value::Null)
    );
    let id = info["id"].to_string();
    let (status, _, stderr) = manage(&["restart", &id]);
    assert_eq!(status, 4, "{stderr}");
    assert!(stderr.contains("was not started by the driver"), "{stderr}");
    let (status, _, _) = manage(&["pause", &id]);
    assert_eq!(status, 0);
    assert_eq!(
        app(&mut client, info["id"].as_u64().unwrap())["state"],
        "paused"
    );
    let (status, stdout, stderr) = manage(&["stop", &id]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, format!("{id} python3: gone\n").as_str()),
        "{stderr}"
    );
    assert!(!outside.wait().unwrap().success());

    // An app that ends at once with an error: status 1, its last word.
    let (status, _, stderr) = manage(&["start", "--cmd", "sh", "-c", "echo boom >&2; exit 3"]);
    assert_eq!(status, 1, "{stderr}");
    assert!(
        stderr.contains("ended at once, status 3") && stderr.contains("boom"),
        "{stderr}"
    );
    let (status, stdout, _) = manage(&["log", "sh"]);
    assert_eq!((status, stdout.as_str()), (0, "boom\n"));
    let (status, _, stderr) = manage(&["start", "--cmd", "/no/such/program"]);
    assert_eq!(status, 2, "{stderr}");
    assert!(stderr.contains("cannot run /no/such/program"), "{stderr}");

    // Stopped, it stays; forgotten, it goes.
    let (status, stdout, stderr) = manage(&["stop", "python3"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "2 python3: exited 143\n"),
        "{stderr}"
    );
    let (_, stdout, _) = manage(&["--json", "ls"]);
    let listed: Value = serde_json::from_str(&stdout).unwrap();
    let states: Vec<&Value> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|a| &a["state"])
        .collect();
    assert_eq!(states, ["running", "exited", "exited"]);
    let (status, stdout, _) = manage(&["forget", "--all"]);
    assert_eq!(status, 0);
    assert_eq!(stdout.lines().count(), 2);
    assert_eq!(apps(&mut client).len(), 1);
    let (status, _, stderr) = manage(&["stop", "9"]);
    assert_eq!(status, 4, "{stderr}");
    assert!(stderr.contains("no such app: 9"));
    // Without a terminal, the verb alone is the list.
    let (status, stdout, _) = manage(&[]);
    assert_eq!((status, stdout.lines().count()), (0, 2));
}
