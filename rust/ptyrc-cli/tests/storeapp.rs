// SPDX-License-Identifier: GPL-3.0-or-later
//! A store app from end to end: a local repository, `ptyrc store add`,
//! a headless session, `app.start {store}`, the runner, the script.
//! Wants the interpreter module (`sdk/js/README.md`): skipped, and said
//! so, without it.

mod common;

use common::{BIN, Sandbox, wait_for};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";
const JS: &str = "../../sdk/js/interp/target/wasm32-unknown-unknown/release/ptyrc_js.wasm";
const CONFETTI: &str = "../../sdk/js/examples/confetti";
/// The first start compiles the interpreter: slow in a debug build.
const SLOW: Duration = Duration::from_secs(180);

/// A script that asks what a store app may not, and logs the answers.
const PROBE: &str = r#"
import { call, log, period } from "ptyrc";
function ask(method, params) {
  try { call(method, params); log(method + ": allowed"); }
  catch (e) { log(method + ": " + e.code); }
}
export function init(args) {
  ask("input.send", { text: "echo pwned\n" });
  ask("app.list", {});
  ask("app.start", { argv: ["true"] });
  ask("tape.start", { path: "/tmp/x" });
  ask("draw.put", { content: { rows: ["probe"] }, at: [0, 0] });
  log("apps=" + call("app.list", {}).apps.length);
  log("sprites=" + call("draw.list", {}).sprites.length);
  const hud = call("draw.put", { content: { rows: ["h"] }, at: [0, 1], layer: "hud" });
  const mine = call("draw.list", {}).sprites.find((s) => s.id == hud.id);
  log("layer=" + mine.layer + " app=" + call("app.list", {}).apps[0].id);
  let put = 2, error = "";
  for (; put < 300; put++) {
    try { call("draw.put", { content: { rows: ["."] }, at: [1, 1] }); }
    catch (e) { error = e.code + ": " + e.message; break; }
  }
  log("put=" + put + " " + error);
  log("word=" + args.word);
  period(0);
}
export function tick(now) {}
export function event(topic, data) {}
"#;

const PROBE_MANIFEST: &str = r#"[app]
id = "probe"
version = "0.1.0"
summary = "Asks what it may not."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"

[options]
word = { type = "string", default = "none", doc = "a word" }
"#;

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

/// A repository of `files`, committed.
fn repository(dir: &Path, files: &[(&str, String)]) {
    std::fs::create_dir_all(dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "an app"]);
}

/// The runner beside `ptyrc`, built when it is not there.
fn runner() -> PathBuf {
    let path = Path::new(BIN).with_file_name("ptyrc-box");
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

fn log_of(client: &mut Client, id: u64) -> String {
    let apps = client.call("app.list", json!({})).unwrap();
    let app = (apps["apps"].as_array().unwrap().iter())
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or(Value::Null);
    let path = app["log"].as_str().unwrap_or_default().to_string();
    std::fs::read_to_string(path).unwrap_or_default()
}

fn code(result: Result<Value, ptyrc_proto::ClientError>) -> String {
    match result {
        Ok(v) => format!("ok {v}"),
        Err(ptyrc_proto::ClientError::Rpc(e)) => e.code.as_str().to_string(),
        Err(e) => format!("{e}"),
    }
}

#[test]
fn a_store_app_from_a_repository_to_the_screen() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        eprintln!(
            "SKIPPED: no interpreter module; build it: \
             cargo build --release --manifest-path sdk/js/interp/Cargo.toml"
        );
        return;
    }
    let sb = Sandbox::new("storeapp");
    let read = |name: &str| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(CONFETTI);
        std::fs::read_to_string(dir.join(name)).unwrap()
    };
    let confetti = sb.dir.join("repo-confetti");
    repository(
        &confetti,
        &[
            ("ptyrc-app.toml", read("ptyrc-app.toml")),
            ("app.js", read("app.js")),
            ("LICENSE", GPL.to_string()),
        ],
    );
    let probe = sb.dir.join("repo-probe");
    repository(
        &probe,
        &[
            ("ptyrc-app.toml", PROBE_MANIFEST.to_string()),
            ("app.js", PROBE.to_string()),
            ("LICENSE", GPL.to_string()),
        ],
    );
    for repo in [&confetti, &probe] {
        let origin = format!("local:{}", repo.display());
        let out = sb.run(&["store", "add", &origin, "--yes"]);
        assert!(
            out.status.success(),
            "store add: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // The driver: the runner and the interpreter in its environment.
    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .env("PTYRC_JS", &js)
        .args([
            "run",
            "--name",
            "s",
            "--headless",
            "--size",
            "60x16",
            "--",
            "cat",
        ])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("s");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("s");

    // A client names an origin or an id: never a program beside it.
    let both = client.call(
        "app.start",
        json!({"store": "confetti", "argv": ["sh", "-c", "true"]}),
    );
    assert_eq!(code(both), "bad-params");
    let named = client.call("app.start", json!({"store": "confetti", "name": "x"}));
    assert_eq!(code(named), "bad-params");
    let program = client.call("app.start", json!({"store": "/bin/sh"}));
    assert_eq!(code(program), "not-usable");
    let option = client.call("app.start", json!({"store": "confetti", "args": {"no": 1}}));
    assert_eq!(code(option), "bad-params");

    // It starts, as its origin, and draws.
    let started = client
        .call(
            "app.start",
            json!({"store": "confetti", "args": {"rate": 30}}),
        )
        .unwrap();
    let app = &started["app"];
    let id = app["id"].as_u64().unwrap();
    let origin = format!("local:{}", confetti.display());
    assert_eq!(app["store"], origin.as_str(), "{app}");
    assert_eq!(app["name"], "confetti");
    assert_eq!(app["declares"], true);
    wait_slow(
        || {
            let apps = client.call("app.list", json!({})).unwrap();
            let me = (apps["apps"].as_array().unwrap().iter()).find(|a| a["id"] == id);
            let me = me.expect("the app is in the table");
            assert_ne!(
                me["state"],
                "exited",
                "log: {}",
                log_of(&mut sb.client("s"), id)
            );
            me["sprites"].as_u64().unwrap_or(0) > 0
        },
        "confetti draws",
    );

    // Its token, read by another process, is of no use there.
    let pid = app["pid"].as_u64().unwrap();
    let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap();
    let token = (environ.split(|b| *b == 0))
        .find_map(|v| v.strip_prefix(b"PTYRC_TOKEN="))
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .expect("the runner has a token");
    assert!(Client::connect_as(&socket, Some(&token)).is_err());

    // Restarted as it is; not as another program.
    let other = client.call(
        "app.restart",
        json!({"app": id, "argv": ["sh", "-c", "true"]}),
    );
    assert_eq!(code(other), "not-usable");
    let renamed = client.call("app.restart", json!({"app": id, "name": "clock"}));
    assert_eq!(code(renamed), "not-usable");

    // What a store app may not: `denied`, whatever it declares.
    let started = client
        .call(
            "app.start",
            json!({"store": "probe", "args": {"word": "fish"}}),
        )
        .unwrap();
    let probe_id = started["app"]["id"].as_u64().unwrap();
    wait_slow(
        || log_of(&mut client, probe_id).contains("word="),
        "the probe has asked",
    );
    let log = log_of(&mut client, probe_id);
    for method in ["input.send", "app.start", "tape.start"] {
        assert!(
            log.contains(&format!("{method}: denied")),
            "{method}: {log}"
        );
    }
    assert!(log.contains("draw.put: allowed"), "{log}");
    // Its own entry, its own sprite: not the others'.
    assert!(
        log.contains("app.list: allowed") && log.contains("apps=1"),
        "{log}"
    );
    assert!(log.contains("sprites=1"), "{log}");
    // Its own band, behind: `hud` is not its; the limit of sprites.
    let layer = format!("layer={} app={probe_id}", -(2 + (probe_id % 97) as i64));
    assert!(log.contains(&layer), "{log}");
    assert!(
        log.contains("put=256 not-usable: draw.put: 256 sprites at most"),
        "{log}"
    );
    assert!(log.contains("word=fish"), "{log}");
    let screen = client.call("screen.text", json!({})).unwrap();
    assert!(!screen.to_string().contains("pwned"), "{screen}");

    // The commit has started once: the lock says so.
    wait_for(
        || {
            let out = sb.run(&["store", "ls", "--json"]);
            let lock = std::fs::read_to_string(sb.dir.join("data/ptyrc/apps.lock"));
            out.status.success() && lock.is_ok_and(|l| l.contains("started = true"))
        },
        "started, in the lock",
    );

    // `ptyrc app`: the options of the manifest, the driver starts it.
    // A native app has the name `confetti`: the origin names this one.
    let help = sb.run(&["app", &origin, "--help"]);
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(
        help.contains("--rate N") && help.contains("pieces a second"),
        "{help}"
    );
    let bad = sb.run(&["app", "probe", "--speed", "3"]);
    assert_eq!(bad.status.code(), Some(2));
    let run = sb.run(&["-s", "s", "app", "probe", "--word", "toy"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(said.contains("probe"), "{said}");
    let ls = sb.run(&["-s", "s", "manage", "ls"]);
    let ls = String::from_utf8_lossy(&ls.stdout).into_owned();
    assert!(ls.contains("confetti") && ls.contains("probe"), "{ls}");
}
