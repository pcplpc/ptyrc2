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

/// Three panes, bars at columns 19 and 39.
const PANES: &str = r#"printf '\033[2J\033[H'
i=1
while [ $i -le 16 ]; do
  printf '\033[%d;1Hleft\033[%d;20H│\033[%d;21Hmid\033[%d;40H│\033[%d;41Hright' $i $i $i $i $i
  i=$((i+1))
done
exec sleep 1000
"#;

/// A native app below `layout` (it declares `clock`): one sprite on the
/// frame of its argv; told `move`, one more there. It asks to move
/// itself, and another. Every line it is sent noted in `$LOG.<pid>`.
const BLIND: &str = r#"
import json, os, socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(os.environ["PTYRC_SOCKET"])
def send(n, method, params):
    s.sendall((json.dumps({"id": n, "method": method, "params": params}) + "\n").encode())
me = int(os.environ["PTYRC_APP"])
send(0, "auth", {"token": os.environ["PTYRC_TOKEN"]})
send(1, "draw.put", {"content": {"rows": ["HELLO"]}, "frame": sys.argv[1],
                     "anchor": {"at": [4, 1], "space": "view"}})
send(2, "subscribe", {"topics": ["move"]})
send(3, "app.move", {"app": me, "frame": 0})
send(4, "app.move", {"app": 1, "frame": 0})
with open(os.environ["LOG"] + "." + str(os.getpid()), "a") as log:
    for line in s.makefile():
        log.write(line)
        log.flush()
        if json.loads(line).get("event") == "move":
            send(5, "draw.put", {"content": {"rows": ["AGAIN"]}, "frame": 7,
                                 "anchor": {"at": [4, 3], "space": "view"}})
            send(6, "frame.list", {})
"#;

#[test]
fn a_native_app_below_layout_is_where_the_user_puts_it() {
    let sb = Sandbox::new("blind");
    let script = sb.dir.join("blind.py");
    std::fs::write(&script, BLIND).unwrap();
    let script = script.to_str().unwrap();
    let log = sb.dir.join("log");
    let mut driver = sb.ptyrc();
    driver
        .env("LOG", &log)
        .args(["run", "--name", "b", "--headless", "--size", "60x16"])
        .args(["--", "sh", "-c", PANES])
        .stdin(Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    wait_for(|| Client::connect(&sb.socket("b")).is_ok(), "the socket");
    let mut client = sb.client("b");
    client.call("session.info", json!({})).unwrap();
    let pane = |client: &mut Client, x: u64| {
        let frames = client.call("frame.list", json!({})).unwrap();
        (frames["frames"].as_array().unwrap().iter())
            .find(|f| f["kind"] == "pane" && f["rect"][0] == x && f["state"] == "stable")
            .map(|f| f["id"].as_u64().unwrap())
    };
    let (mut mid, mut right) = (0, 0);
    wait_for(
        || {
            let found = (pane(&mut client, 20), pane(&mut client, 40));
            found
                .0
                .zip(found.1)
                .map(|(m, r)| (mid, right) = (m, r))
                .is_some()
        },
        "the panes",
    );
    let log_of = |pid: &Value| {
        std::fs::read_to_string(format!("{}.{pid}", log.display())).unwrap_or_default()
    };
    let answers = |pid: &Value, id: u64| -> Vec<Value> {
        (log_of(pid).lines())
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["id"] == id)
            .collect()
    };
    let declares = json!({"permissions": {"clock": "the time"}});
    let start = |client: &mut Client, frame: Option<u64>| {
        let mut params = json!({"argv": ["python3", script, "largest"], "declares": declares});
        if let Some(frame) = frame {
            params["frame"] = json!(frame);
        }
        let started = client.call("app.start", params).unwrap();
        started["app"]["id"].as_u64().unwrap()
    };

    // Started with no frame: whatever it names, frame 0.
    let id = start(&mut client, None);
    wait_for(|| app(&mut client, id)["sprites"] == 1, "the sprite");
    assert_eq!(app(&mut client, id)["frames"], json!([0]));
    let pid = app(&mut client, id)["pid"].clone();
    // It may move neither itself nor another.
    wait_for(|| answers(&pid, 4).len() == 1, "its asks answered");
    for asked in [3, 4] {
        let answer = &answers(&pid, asked)[0];
        assert!(answer["error"].is_object(), "{answer}");
    }
    assert_eq!(app(&mut client, 1)["frames"], json!([]));
    assert_eq!(app(&mut client, id)["frames"], json!([0]));

    // Moved by the user: its sprite goes along; what it draws after,
    // on that frame; it is told one frame.
    let moved = client
        .call("app.move", json!({"app": id, "frame": mid}))
        .unwrap();
    assert_eq!(moved["frame"], mid);
    wait_for(|| app(&mut client, id)["sprites"] == 2, "drawn again");
    assert_eq!(app(&mut client, id)["frames"], json!([mid]));
    wait_for(|| answers(&pid, 6).len() == 1, "frame.list");
    let listed = &answers(&pid, 6)[0]["result"]["frames"];
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "{listed}");
    let text = shown(&mut client);
    let line = |y: usize| text.lines().nth(y).unwrap_or_default().to_string();
    assert!(
        line(1).starts_with("left               │mid HELLO"),
        "{text}"
    );
    assert!(
        line(3).starts_with("left               │mid AGAIN"),
        "{text}"
    );

    // Started on a frame: its surface.
    let other = start(&mut client, Some(right));
    wait_for(|| app(&mut client, other)["sprites"] == 1, "the sprite");
    assert_eq!(app(&mut client, other)["frames"], json!([right]));
}
