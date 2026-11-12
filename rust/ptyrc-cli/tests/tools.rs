// SPDX-License-Identifier: GPL-3.0-or-later
//! Hands on a session: grabbed keys, clicks, pictures, `attach`.

mod common;

use common::*;
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::time::Duration;

/// An app that shows the bytes it reads: `^[[A` for the up arrow.
const SHOW_KEYS: &str = "stty raw -echo; exec cat -v";

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

#[test]
fn grabbed_keys_do_not_reach_the_app() {
    let sb = Sandbox::new("grab");
    let (mut host, mut driver) = HostPty::spawn(
        &sb,
        &[BIN, "run", "--name", "g", "--", "sh", "-c", SHOW_KEYS],
    );
    wait_for(|| sb.socket("g").exists(), "socket");
    let mut client = sb.client("g");
    // Raw mode on both sides before a key is typed.
    client
        .call("screen.wait", json!({"stable_ms": 200, "timeout_ms": 5000}))
        .unwrap();
    host.type_in(b"a");
    host.wait_seen("a");

    // Two keys taken: the arrows, in either cursor mode.
    let got = client
        .call("input.grab", json!({"keys": ["up", "down"]}))
        .unwrap();
    assert_eq!(got, json!({"escape": "ctrl+]"}));
    host.type_in(b"\x1b[Ab\x1bOBc\x1b[C");
    host.wait_seen("c^[[C");
    assert_eq!(next_input(&mut client)["key"], "up");
    let down = next_input(&mut client);
    assert_eq!(
        down,
        json!({"key": "down", "bytes": "G09C", "grabbed": true})
    );
    assert!(!seen(&host).contains("^[[A") && !seen(&host).contains("^[OB"));

    // Another client waits its turn.
    let mut other = sb.client("g");
    let e = other.call("input.grab", json!({})).unwrap_err();
    assert!(
        e.to_string().contains("another client holds the keys"),
        "{e}"
    );
    for bad in [
        json!({"keys": ["ctrl+xx"]}),
        json!({"escape": "q"}),
        json!({"keys": "up"}),
    ] {
        assert!(client.call("input.grab", bad.clone()).is_err(), "{bad}");
    }

    // Everything taken, until the escape key: it reaches nobody.
    client.call("input.grab", json!({})).unwrap();
    host.type_in("xé".as_bytes());
    assert_eq!(next_input(&mut client)["key"], "x");
    assert_eq!(next_input(&mut client)["key"], "é");
    host.type_in(b"\x1dz");
    assert_eq!(next_input(&mut client), json!({"released": "escape"}));
    host.wait_seen("z");
    assert!(!seen(&host).contains('x') && !seen(&host).contains("^]"));

    // A grab goes with its client; `input.release` gives it back.
    other.call("input.grab", json!({})).unwrap();
    other.call("input.release", json!({})).unwrap();
    host.type_in(b"m");
    host.wait_seen("m");
    other.call("input.grab", json!({})).unwrap();
    drop(other);
    wait_for(
        || {
            host.type_in(b"w");
            std::thread::sleep(Duration::from_millis(20));
            host.pump();
            seen(&host).contains('w')
        },
        "the keys back",
    );

    // The CLI: one line per key, until the escape.
    let grab = sb
        .ptyrc()
        .args(["-s", "g", "grab", "--escape", "f12"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(
        || client.call("session.info", json!({})).unwrap()["grabbed"] == true,
        "the grab",
    );
    host.type_in(b"k\x1b[24~");
    let out = grab.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "{\"bytes\":\"aw==\",\"grabbed\":true,\"key\":\"k\"}\n{\"released\":\"escape\"}\n"
    );

    client
        .call("input.send", json!({"keys": ["ctrl+c"]}))
        .unwrap();
    drop(driver.0.kill());
    host.wait_exit(&mut driver);
}

#[test]
fn clicks_are_told_as_the_app_asked() {
    let sb = Sandbox::new("click");
    let script = format!("printf 'one two\\r\\n\\033[?1000h\\033[?1006h'; {SHOW_KEYS}");
    let _driver = sb.headless("c", "100x6", &["sh", "-c", &script]);
    let mut client = sb.client("c");
    let wait = |client: &mut Client, pattern: &str| {
        let got = client
            .call(
                "screen.wait",
                json!({"pattern": pattern, "timeout_ms": 5000}),
            )
            .unwrap();
        assert_eq!(got["matched"], true, "{pattern}");
    };
    wait(&mut client, "one two");
    wait_for(
        || client.call("input.click", json!({"x": 4, "y": 2})).is_ok(),
        "the mouse mode",
    );
    wait(&mut client, "^[[<0;5;3M^[[<0;5;3m");
    client
        .call(
            "input.click",
            json!({"x": 0, "y": 0, "button": "wheel-down"}),
        )
        .unwrap();
    wait(&mut client, "^[[<65;1;1M");
    let e = client
        .call("input.click", json!({"x": 100, "y": 0}))
        .unwrap_err();
    assert!(e.to_string().contains("off the screen (100x6)"), "{e}");
    let e = client
        .call("input.click", json!({"x": 0, "y": 0, "button": "fourth"}))
        .unwrap_err();
    assert!(e.hint().unwrap().contains("wheel-up"), "{e}");

    // On text, from the CLI.
    let out = sb.run(&["-s", "c", "--json", "click", "--text", "two", "-b", "right"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        r#"{"x":4,"y":0}"#
    );
    wait(&mut client, "^[[<2;5;1M^[[<2;5;1m");
    assert_eq!(
        sb.run(&["-s", "c", "click", "--text", "three"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        sb.run(&["-s", "c", "click", "nowhere"]).status.code(),
        Some(2)
    );

    // An app that did not ask is not clicked.
    let _quiet = sb.headless("q", "40x6", &["cat"]);
    let out = sb.run(&["-s", "q", "click", "1,1"]);
    assert_eq!(out.status.code(), Some(4));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("does not listen to the mouse"), "{err}");
}

#[test]
fn pictures_of_the_screen() {
    let sb = Sandbox::new("shot");
    let script = "printf '\\033[31mred\\033[0m <plain>\\r\\n\\033[1;44mbar\\033[0m'; sleep 30";
    let _driver = sb.headless("s", "20x4", &["sh", "-c", script]);
    let mut client = sb.client("s");
    client
        .call("screen.wait", json!({"pattern": "bar", "timeout_ms": 5000}))
        .unwrap();
    let shot = |args: &[&str]| {
        let mut all = vec!["-s", "s", "shot"];
        all.extend(args);
        let out = sb.run(&all);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let svg = shot(&[]);
    assert!(
        svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"168.0\" height=\"72.0\""
        )
    );
    assert!(
        svg.contains("fill=\"#cd0000\" xml:space=\"preserve\">red</text>"),
        "{svg}"
    );
    assert!(svg.contains(">&lt;plain&gt;</text>"));
    assert!(
        svg.contains(
            "<rect x=\"0.0\" y=\"18.0\" width=\"25.2\" height=\"18.0\" fill=\"#0000ee\"/>"
        )
    );
    let html = shot(&["--html"]);
    assert!(
        html.contains("<span style=\"color:#cd0000;\">red</span> &lt;plain&gt;\n"),
        "{html}"
    );
    assert!(html.contains("<span style=\"background:#0000ee;font-weight:bold;\">bar</span>"));
    let ansi = shot(&["--ansi"]);
    assert!(
        ansi.contains("31mred") && ansi.lines().count() == 4,
        "{ansi:?}"
    );

    // The sprites are in the picture when asked.
    client
        .call(
            "draw.put",
            json!({"content": {"rows": ["SPR"]}, "anchor": {"at": [5, 3], "space": "view"},
                   "layer": "front", "persist": true}),
        )
        .unwrap();
    wait_for(
        || shot(&["--html", "--sprites"]).contains("SPR"),
        "the sprite",
    );
    assert!(!shot(&["--html"]).contains("SPR"));
    assert!(shot(&["--sprites"]).contains(">SPR</text>"));
    let file = sb.dir.join("shot.svg");
    assert_eq!(shot(&["-o", file.to_str().unwrap()]), "");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), svg);
    // One frame, by name.
    assert!(shot(&["-f", "raw", "--ansi"]).contains("red"));
    assert_eq!(
        sb.run(&["-s", "s", "shot", "-f", "nope"]).status.code(),
        Some(4)
    );

    let snap: Value = serde_json::from_slice(&sb.run(&["-s", "s", "snap"]).stdout).unwrap();
    assert_eq!(snap["size"], json!([20, 4]));
    assert_eq!(snap["text"][0], "red <plain>");
    assert_eq!(snap["frames"][0]["kind"], "raw");
}

#[test]
fn attach_mirrors_and_types() {
    let sb = Sandbox::new("attach");
    let _driver = sb.headless("a", "40x6", &["cat"]);
    let out = sb.run(&["-s", "a", "attach"]);
    assert_eq!(out.status.code(), Some(2), "no terminal: a usage error");
    assert!(String::from_utf8_lossy(&out.stderr).contains("ptyrc show"));
    let out = sb.run(&["-s", "a", "attach", "--detach", "q"]);
    assert_eq!(out.status.code(), Some(2));

    let (mut host, mut attach) = HostPty::spawn(&sb, &[BIN, "-s", "a", "attach"]);
    host.wait_seen("\x1b[?1049h");
    // Typed here, read there, shown here.
    host.type_in(b"mar");
    host.type_in(b"co\r");
    host.wait_seen("marco");
    let mut client = sb.client("a");
    let got = client
        .call(
            "screen.wait",
            json!({"pattern": "marco", "timeout_ms": 5000}),
        )
        .unwrap();
    assert_eq!(got["matched"], true);
    // Written there by another, shown here; the cursor follows.
    client
        .call("input.send", json!({"text": "polo\r"}))
        .unwrap();
    host.wait_seen("polo");
    host.wait_seen("\x1b[5;1H\x1b[?25h");

    // The detach key leaves; it is not typed; the session lives.
    host.type_in(b"x\x1dy");
    assert_eq!(host.wait_exit(&mut attach), 0);
    assert!(seen(&host).contains("detached from a"), "{:?}", seen(&host));
    assert!(seen(&host).contains("\x1b[?1049l"));
    client
        .call("screen.wait", json!({"pattern": "x", "timeout_ms": 5000}))
        .unwrap();
    let text = client.call("screen.text", json!({})).unwrap();
    assert_eq!(
        text["lines"][4], "x",
        "what follows the detach key is dropped"
    );

    // A session that ends says so.
    let (mut host, mut attach) =
        HostPty::spawn(&sb, &[BIN, "-s", "a", "attach", "--detach", "f12"]);
    host.wait_seen("polo");
    host.type_in(b"\r\x04");
    assert_eq!(host.wait_exit(&mut attach), 0);
    assert!(
        seen(&host).contains("the session ended"),
        "{:?}",
        seen(&host)
    );
}

#[test]
fn events_start_with_the_state_when_asked() {
    use std::io::{BufRead, BufReader};
    let sb = Sandbox::new("evsnap");
    let _driver = sb.headless("e", "30x5", &["cat"]);
    let mut events = Proc(
        sb.ptyrc()
            .args(["-s", "e", "events", "size", "cursor", "--snapshot"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut lines = BufReader::new(events.0.stdout.take().unwrap()).lines();
    let first: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(first, json!({"event": "size", "data": {"size": [30, 5]}}));
    let second: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(second["event"], "cursor");
    assert_eq!(
        (&second["data"]["x"], &second["data"]["y"]),
        (&json!(0), &json!(0))
    );
}
