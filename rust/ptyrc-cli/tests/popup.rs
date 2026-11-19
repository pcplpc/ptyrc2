// SPDX-License-Identifier: GPL-3.0-or-later
//! The pop-up of the permissions: a start held until the user answers
//! on the host keyboard; the answers in `permissions.toml`.

mod common;

use common::{BIN, HostPty, Sandbox, TIMEOUT, wait_for};
use ptyrc_proto::{Client, ClientError};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const ASKS: &str = "This app asks to:";

/// What the host shows (the driver's model of it).
fn host_text(client: &mut Client) -> String {
    let dump = client.call("debug.dump", json!({})).unwrap();
    let lines = dump["host"].as_array().unwrap();
    let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    lines.join("\n")
}

/// Pump the host while waiting: the driver writes to it.
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

/// A while of pumping, for what must not happen.
fn idle(host: &mut HostPty, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        host.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `method` in a thread of its own: the answer comes when the user
/// answers.
fn ask(sb: &Sandbox, method: &str, params: Value) -> Receiver<Result<Value, ClientError>> {
    let mut client = sb.client("p");
    let method = method.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(client.call(&method, params));
    });
    rx
}

fn names(client: &mut Client) -> Vec<String> {
    let apps = client.call("app.list", json!({})).unwrap();
    (apps["apps"].as_array().unwrap().iter())
        .map(|a| a["name"].as_str().unwrap().to_string())
        .collect()
}

fn flower(more: &[(&str, &str)]) -> Value {
    let mut permissions = json!({
        "shape": "to stand on the text",
        "words": "to bloom on its name",
        "text": "to bloom on every word",
    });
    for (name, why) in more {
        permissions[*name] = json!(why);
    }
    json!({"argv": ["sleep", "60"], "name": "flower",
           "declares": {"permissions": permissions, "words": ["flower"]}})
}

fn session(sb: &Sandbox) -> (HostPty, common::Proc, Client) {
    let (mut host, driver) = HostPty::spawn(sb, &[BIN, "run", "--no-hotkey", "--name", "p", "cat"]);
    until(&mut host, || sb.socket("p").exists(), "the session");
    let client = sb.client("p");
    (host, driver, client)
}

#[test]
fn asked_on_the_host_answered_by_it_alone_and_kept() {
    let sb = Sandbox::new("popup");
    let file = sb.config().join("ptyrc").join("permissions.toml");
    let (mut host, _driver, mut client) = session(&sb);

    let started = ask(&sb, "app.start", flower(&[]));
    until(
        &mut host,
        || host_text(&mut client).contains(ASKS),
        "the pop-up",
    );
    let shown = host_text(&mut client);
    assert!(shown.contains("flower ─ native"), "{shown}");
    assert!(shown.contains("native/flower"), "{shown}");
    assert!(shown.contains("[x] see where the text is, not what it says"));
    assert!(shown.contains("[x] see these words: flower"));
    assert!(shown.contains("[ ] read all the text on the screen"));
    assert!(shown.contains("to bloom on its name"));
    assert!(shown.contains(" ptyrc "));
    // Not started yet.
    assert!(!names(&mut client).contains(&"flower".to_string()));

    // A client does not answer it, as the host keyboard or not.
    for params in [
        json!({"keys": ["space", "enter"], "host": true}),
        json!({"keys": ["esc"], "host": true}),
        // The escape ends as a sequence for `cat`, which echoes it.
        json!({"text": "x"}),
        json!({"keys": ["enter"]}),
    ] {
        client.call("input.send", params).unwrap();
    }
    // Nor covers it: a sprite over the whole screen, on top.
    let rows = vec!["Z".repeat(80); 24];
    client
        .call(
            "draw.put",
            json!({"anchor": {"at": [0, 0], "space": "view"}, "layer": "hud", "z": 1000,
                   "content": {"rows": rows, "opaque": true}}),
        )
        .unwrap();
    idle(&mut host, 300);
    assert!(started.try_recv().is_err(), "answered by a client");
    assert!(!file.exists());
    let shown = host_text(&mut client);
    assert!(shown.contains(ASKS), "{shown}");
    assert!(
        shown.contains("[ ] read all the text"),
        "ticked by a client: {shown}"
    );
    assert!(!shown.contains('Z'), "a sprite with the pop-up: {shown}");

    // The host keyboard: down, untick `words` (and `text` above it),
    // enter.
    host.type_in(b"j");
    host.type_in(b" ");
    until(
        &mut host,
        || host_text(&mut client).contains("[ ] see these words"),
        "the box unticked",
    );
    host.type_in(b"\r");
    let answer = loop {
        host.pump();
        if let Ok(answer) = started.recv_timeout(Duration::from_millis(10)) {
            break answer.unwrap();
        }
    };
    assert_eq!(answer["app"]["name"], "flower");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains(
            "[app.\"native/flower\"]\nallow = [\"shape\"]\ndeny = [\"words\", \"text\"]\n"
        ),
        "{text}"
    );
    // Gone, and the sprite back.
    until(
        &mut host,
        || {
            let shown = host_text(&mut client);
            !shown.contains(ASKS) && shown.contains("ZZZZ")
        },
        "the screen back",
    );

    // Asked again: a yes and a no are kept, nothing is shown.
    let again = ask(&sb, "app.start", flower(&[]));
    let answer = loop {
        host.pump();
        if let Ok(answer) = again.recv_timeout(Duration::from_millis(10)) {
            break answer.unwrap();
        }
    };
    assert_eq!(answer["app"]["name"], "flower");

    // An option that declares more: the difference alone. Esc: nothing
    // written, nothing started.
    let before = names(&mut client).len();
    let more = ask(&sb, "app.start", flower(&[("keys-arrows", "to steer")]));
    until(
        &mut host,
        || host_text(&mut client).contains(ASKS),
        "the pop-up",
    );
    let shown = host_text(&mut client);
    assert!(
        shown.contains("[x] take the arrow and function keys"),
        "{shown}"
    );
    assert!(!shown.contains("see where the text is"), "{shown}");
    host.type_in(b"\x1b");
    let answer = loop {
        host.pump();
        if let Ok(answer) = more.recv_timeout(Duration::from_millis(10)) {
            break answer;
        }
    };
    let Err(ClientError::Rpc(error)) = answer else {
        panic!("not refused: {answer:?}");
    };
    assert_eq!(error.code.as_str(), "cancelled");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    assert_eq!(names(&mut client).len(), before);
}

#[test]
fn one_screen_at_a_time_and_a_screen_too_small() {
    let sb = Sandbox::new("popup-queue");
    let (mut host, _driver, mut client) = session(&sb);
    let app = |name: &str| {
        json!({"argv": ["sleep", "60"], "name": name,
               "declares": {"permissions": {"pointer": "to follow it"}}})
    };
    let first = ask(&sb, "app.start", app("one"));
    until(
        &mut host,
        || host_text(&mut client).contains("one ─ native"),
        "the first",
    );
    let second = ask(&sb, "app.start", app("two"));
    idle(&mut host, 200);
    assert!(!host_text(&mut client).contains("two ─ native"));
    host.type_in(b"\r");
    until(
        &mut host,
        || host_text(&mut client).contains("two ─ native"),
        "the second",
    );
    assert!(first.recv_timeout(TIMEOUT).unwrap().is_ok());
    host.type_in(b"\r");
    let answer = loop {
        host.pump();
        if let Ok(answer) = second.recv_timeout(Duration::from_millis(10)) {
            break answer;
        }
    };
    assert!(answer.is_ok());

    // Smaller than the box: refused, with a hint.
    ptyrc_server::pty::set_winsize(&host.master, 20, 6).unwrap();
    wait_for(
        || client.call("session.info", json!({})).unwrap()["size"] == json!([20, 6]),
        "the resize",
    );
    let error = client.call("app.start", app("three")).unwrap_err();
    let ClientError::Rpc(error) = error else {
        panic!("{error:?}");
    };
    assert_eq!(error.code.as_str(), "not-usable");
    assert!(error.hint.unwrap().contains("enlarge"));
}

#[test]
fn headless_the_declared_or_the_file() {
    let sb = Sandbox::new("popup-headless");
    let _driver = sb.headless("p", "80x24", &["cat"]);
    let mut client = sb.client("p");
    let started = client.call("app.start", flower(&[])).unwrap();
    assert_eq!(started["app"]["name"], "flower");
    assert!(!sb.config().join("ptyrc").exists(), "nothing written");
    // The screen of `ptyrc manage` cannot be shown either.
    let id = started["app"]["id"].clone();
    let error = client
        .call("app.permissions", json!({"app": id}))
        .unwrap_err();
    let ClientError::Rpc(error) = error else {
        panic!("{error:?}");
    };
    assert_eq!(error.code.as_str(), "not-usable");
}

#[test]
fn the_screen_of_manage_changes_the_answers() {
    let sb = Sandbox::new("popup-review");
    let file = sb.config().join("ptyrc").join("permissions.toml");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        "[app.\"native/flower\"]\nallow = [\"shape\", \"words\"]\ndeny = [\"text\"]\n",
    )
    .unwrap();
    let (mut host, _driver, mut client) = session(&sb);
    let started = client.call("app.start", flower(&[])).unwrap();
    let id = started["app"]["id"].clone();
    let review = ask(&sb, "app.permissions", json!({"app": id}));
    until(
        &mut host,
        || host_text(&mut client).contains("This app may:"),
        "the review",
    );
    let shown = host_text(&mut client);
    assert!(shown.contains("[x] see where the text"), "{shown}");
    assert!(shown.contains("[x] see these words"), "{shown}");
    assert!(shown.contains("[ ] read all the text"), "{shown}");
    assert!(shown.contains("enter save"), "{shown}");
    // Text ticked.
    host.type_in(b"jj ");
    host.type_in(b"\r");
    let answer = loop {
        host.pump();
        if let Ok(answer) = review.recv_timeout(Duration::from_millis(10)) {
            break answer.unwrap();
        }
    };
    assert_eq!(answer["app"]["id"], id);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("allow = [\"shape\", \"words\", \"text\"]\ndeny = []\n"),
        "{text}"
    );
    // An app that declares nothing has no screen.
    let bare = client
        .call(
            "app.start",
            json!({"argv": ["sleep", "60"], "name": "bare"}),
        )
        .unwrap();
    let error = client
        .call("app.permissions", json!({"app": bare["app"]["id"]}))
        .unwrap_err();
    assert!(error.to_string().contains("declares nothing"), "{error}");
}

#[test]
fn a_foreground_app_is_asked_and_esc_starts_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::new("popup-enrol");
    let (mut host, _driver, mut client) = session(&sb);
    let bin = sb.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ran = sb.dir.join("ran");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --declares ]; then\n\
         echo '{{\"permissions\": {{\"text\": \"to read\"}}}}'; exit 0; fi\ntouch {}\n",
        ran.display()
    );
    std::fs::write(bin.join("ptyrc-app"), script).unwrap();
    std::fs::set_permissions(
        bin.join("ptyrc-app"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let mut started = sb
        .ptyrc()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .args(["-s", "p", "app", "peek"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    until(
        &mut host,
        || host_text(&mut client).contains(ASKS),
        "the pop-up",
    );
    assert!(host_text(&mut client).contains("peek ─ native"));
    host.type_in(b"\x1b");
    let status = loop {
        host.pump();
        if let Some(status) = started.try_wait().unwrap() {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut started.stderr.take().unwrap(), &mut stderr).unwrap();
    assert!(!status.success());
    assert!(stderr.contains("cancelled"), "{stderr}");
    assert!(!ran.exists());
    assert!(!names(&mut client).contains(&"peek".to_string()));
}
