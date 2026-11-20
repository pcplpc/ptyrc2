// SPDX-License-Identifier: GPL-3.0-or-later
//! The deck of the script SDK (`"ptyrc/deck"`) against a driver:
//! `sdk/js/examples/clockface` installed, started, drawn in one batch,
//! the variant that fits, a key of its focused widget. Wants the
//! interpreter module (`sdk/js/README.md`): skipped, and said so,
//! without it.

mod common;

use common::{Sandbox, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const JS: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";
const CLOCKFACE: &str = "../../sdk/js/examples/clockface";
/// The first start compiles the interpreter: slow in a debug build.
const SLOW: Duration = Duration::from_secs(180);

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "agent")
        .env("GIT_AUTHOR_EMAIL", "agent@example.org")
        .env("GIT_COMMITTER_NAME", "agent")
        .env("GIT_COMMITTER_EMAIL", "agent@example.org")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn runner() -> PathBuf {
    let path = Path::new(common::BIN).with_file_name("ptyrc-box");
    if !path.is_file() {
        let built = Command::new(env!("CARGO"))
            .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
            .status()
            .unwrap();
        assert!(built.success(), "cargo build -p ptyrc-box");
    }
    path
}

fn wait_slow(mut ready: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + SLOW;
    while !ready() {
        assert!(Instant::now() < deadline, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn the_widget(client: &mut Client) -> Option<Value> {
    let list = client.call("widget.list", json!({})).unwrap();
    (list["widgets"].as_array().unwrap().iter())
        .find(|w| w["tag"] == "clockface" && !w["sprite"].is_null())
        .cloned()
}

/// The rows of the screen, sprites drawn.
fn seen(client: &mut Client) -> String {
    let text = client
        .call("screen.text", json!({"sprites": true}))
        .unwrap();
    let lines = text["lines"].as_array().unwrap().iter();
    lines
        .map(|l| l.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

fn log_of(client: &mut Client, id: u64) -> String {
    let apps = client.call("app.list", json!({})).unwrap();
    let app = (apps["apps"].as_array().unwrap().iter())
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or(Value::Null);
    std::fs::read_to_string(app["log"].as_str().unwrap_or_default()).unwrap_or_default()
}

#[test]
fn a_deck_app_in_a_session() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let js = here.join(JS);
    if !js.is_file() {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_JS").is_none(),
            "no interpreter"
        );
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let sb = Sandbox::new("deck");
    let repo = sb.dir.join("clockface");
    std::fs::create_dir_all(&repo).unwrap();
    for f in [
        "ptyrc-app.toml",
        "app.js",
        "face.js",
        "tests.js",
        "LICENSE.txt",
    ] {
        std::fs::copy(here.join(CLOCKFACE).join(f), repo.join(f)).unwrap();
    }
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "an app"]);
    let out = sb.run(&[
        "store",
        "add",
        &format!("local:{}", repo.display()),
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .env("PTYRC_JS", &js)
        .args(["run", "--name", "d", "--headless", "--size", "60x12"])
        .args(["--", "sh", "-c", "sleep 600"])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("d");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("d");
    let started = client
        .call(
            "app.start",
            json!({"store": "clockface", "args": {"every": true}}),
        )
        .unwrap();
    let id = started["app"]["id"].as_u64().unwrap();

    // The big face, ticking: the driver told the variant (`widgets`).
    let mut widget = Value::Null;
    wait_slow(
        || {
            let log = log_of(&mut client, id);
            assert!(!log.contains("exit:") && !log.contains("Error"), "{log}");
            the_widget(&mut client).map(|w| widget = w).is_some() && seen(&mut client).contains("▪")
        },
        "the big face",
    );
    assert_eq!(widget["variant"], "big", "{widget}");
    assert_eq!(widget["asks"], "quiet", "{widget}");
    wait_slow(|| seen(&mut client).contains("▀▀█"), "a second later");

    // Focused, `space` pauses it: the face stays.
    client
        .call("widget.focus", json!({"id": widget["id"]}))
        .unwrap();
    client
        .call("input.send", json!({"text": " ", "host": true}))
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let paused = seen(&mut client);
    std::thread::sleep(Duration::from_millis(2200));
    assert_eq!(seen(&mut client), paused);
    // `r`: back to 00:00, paused.
    client
        .call("input.send", json!({"text": "r", "host": true}))
        .unwrap();
    wait_for(|| seen(&mut client).matches("█ █").count() == 4, "00:00");
    let log = log_of(&mut client, id);
    assert!(!log.contains("Error"), "{log}");
}
