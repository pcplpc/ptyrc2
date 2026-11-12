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
    assert_eq!(got, json!({"escape": "ctrl+t"}));
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
    host.type_in(b"\x14z");
    assert_eq!(next_input(&mut client), json!({"released": "escape"}));
    host.wait_seen("z");
    assert!(!seen(&host).contains('x') && !seen(&host).contains("^T"));

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
    let _driver = sb.headless("c", "200x6", &["sh", "-c", &script]);
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
    // Modifiers, a double click; a drag is press and release for an app
    // that did not ask for the moves.
    let out = sb.run(&[
        "-s", "c", "click", "7,3", "--mod", "alt", "--count", "2", "--to", "9,4",
    ]);
    assert!(out.status.success());
    wait(&mut client, "^[[<8;8;4M^[[<8;10;5m^[[<8;8;4M^[[<8;10;5m");
    let out = sb.run(&["-s", "c", "click", "1,1", "--mod", "ctl"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("did you mean ctrl?"), "{err}");
    let e = client
        .call("input.click", json!({"x": 200, "y": 0}))
        .unwrap_err();
    assert!(e.to_string().contains("off the screen (200x6)"), "{e}");
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
fn host_clicks_go_to_a_sprite_or_to_the_app() {
    let sb = Sandbox::new("mouse");
    // The app asks for clicks in the X10 form.
    let script = format!("printf '\\033[?1000h'; {SHOW_KEYS}");
    let (mut host, mut driver) =
        HostPty::spawn(&sb, &[BIN, "run", "--name", "m", "--", "sh", "-c", &script]);
    wait_for(|| sb.socket("m").exists(), "socket");
    let mut client = sb.client("m");
    client
        .call("screen.wait", json!({"stable_ms": 200, "timeout_ms": 5000}))
        .unwrap();
    let sprite = client
        .call(
            "draw.put",
            json!({"content": {"rows": ["[-ok-]"]}, "anchor": {"at": [10, 3], "space": "view"},
                   "layer": "front", "persist": true}),
        )
        .unwrap()["id"]
        .clone();
    host.wait_seen("[-ok-]");

    // A listener: the host is asked for reports, in the SGR form.
    client.subscribe(&["mouse"]).unwrap();
    host.wait_seen("\x1b[?1002h\x1b[?1006h");
    let next = |client: &mut Client| loop {
        let event = client.next_event(Some(TIMEOUT)).unwrap().expect("an event");
        if event.event == "mouse" {
            return event.data;
        }
    };

    // On the sprite: an event, press and release; the app sees nothing.
    host.type_in(b"\x1b[<0;13;4M\x1b[<0;13;4m");
    let press = next(&mut client);
    assert_eq!(
        press,
        json!({"sprite": sprite, "at": [2, 0], "x": 12, "y": 3, "frame": press["frame"],
               "pos": press["pos"], "button": "left", "action": "press", "mods": []})
    );
    assert!(press["frame"].is_number() && press["pos"].is_array());
    assert_eq!(next(&mut client)["action"], "release");

    // A drag that leaves the sprite stays with it, to the release.
    host.type_in(b"\x1b[<16;11;4M\x1b[<48;30;5M\x1b[<16;30;5m");
    assert_eq!(next(&mut client)["mods"], json!(["ctrl"]));
    let drag = next(&mut client);
    assert_eq!(
        (
            drag["action"].clone(),
            drag["sprite"].clone(),
            drag["at"].clone()
        ),
        (json!("drag"), sprite.clone(), Value::Null)
    );
    assert_eq!(
        (drag["x"].clone(), drag["y"].clone()),
        (json!(29), json!(4))
    );
    assert_eq!(next(&mut client)["action"], "release");

    // Elsewhere: the app's, written again in its own form; a drag it
    // did not ask for is dropped.
    host.type_in(b"a\x1b[<0;2;6M\x1b[<32;3;6M\x1b[<0;3;6mb");
    host.wait_seen("a^[[M \"&^[[M##&b");
    assert!(!seen(&host).contains("^[[<"), "{:?}", seen(&host));

    // The listener gone, the host has the app's mode again, and the
    // reports are the app's business.
    drop(client);
    host.wait_seen("\x1b[?1002l\x1b[?1003l\x1b[?1000l\x1b[?9l\x1b[?1006l\x1b[?1000h\x1b[?1006l");
    host.type_in(b"\x1b[M !!c");
    host.wait_seen("^[[M !!c");

    drop(driver.0.kill());
    host.wait_exit(&mut driver);
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
    // The cursor, after `bar`: an inverse cell.
    let cursor = "<rect x=\"25.2\" y=\"18.0\" width=\"8.4\" height=\"18.0\" fill=\"#e5e5e5\"/>";
    assert!(svg.contains(cursor), "{svg}");
    assert!(!shot(&["--no-cursor"]).contains(cursor));
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
    host.type_in(b"x\x14y");
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
fn attach_takes_the_modes_and_follows_the_size() {
    use std::os::fd::AsFd;
    let sb = Sandbox::new("attach2");
    let script = format!("printf '\\033[?1h\\033[?1000h\\033[?1006hready'; {SHOW_KEYS}");
    let _driver = sb.headless("b", "40x6", &["sh", "-c", &script]);
    let mut client = sb.client("b");
    let size =
        |client: &mut Client| client.call("session.info", json!({})).unwrap()["size"].clone();
    client
        .call(
            "screen.wait",
            json!({"pattern": "ready", "timeout_ms": 5000}),
        )
        .unwrap();

    // --resize: the session takes the size of this terminal.
    let (mut host, mut attach) = HostPty::spawn(&sb, &[BIN, "-s", "b", "attach", "--resize"]);
    host.wait_seen("ready");
    assert_eq!(size(&mut client), json!([80, 24]));
    // The modes of the app are this terminal's: application cursor
    // keys, the mouse in the SGR form; the others are off.
    host.wait_seen("\x1b[?1h\x1b[?9l\x1b[?1000h\x1b[?1002l\x1b[?1003l\x1b[?1006h\x1b[?2004l");
    // So an arrow and a click are typed as the app reads them.
    host.type_in(b"\x1bOA\x1b[<0;3;2M");
    client
        .call(
            "screen.wait",
            json!({"pattern": "^[OA^[[<0;3;2M", "timeout_ms": 5000}),
        )
        .unwrap();

    // Resized after the start: followed.
    ptyrc_server::pty::set_winsize(host.master.as_fd(), 60, 10).unwrap();
    wait_for(|| size(&mut client) == json!([60, 10]), "the new size");

    // Written there, shown here, at the new size.
    client.call("input.send", json!({"text": "x"})).unwrap();
    host.wait_seen("^[[<0;3;2Mx");
    host.seen.clear();
    host.type_in(b"\x14");
    assert_eq!(host.wait_exit(&mut attach), 0);
    // Left: the modes are given back.
    assert!(
        seen(&host).contains("\x1b[?1l\x1b[?9l\x1b[?1000l"),
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
