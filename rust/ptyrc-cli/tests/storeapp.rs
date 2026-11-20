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
const JS: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";
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
             sdk/js/interp/build.sh"
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

/// Draws a row of `=` across the screen, on `raw`, whatever its
/// surface; with `widget`, a widget that asks for the keys.
const WIDE: &str = r#"
import { call, log, period } from "ptyrc";
export function init(args) {
  const row = "=".repeat(60);
  call("draw.put", { content: { rows: [row] }, frame: "raw", anchor: { at: [0, args.row], space: "view" } });
  if (args.widget) {
    call("widget.declare", { widgets: [{ tag: "w", size: [4, 1], content: { rows: ["hi"] } }] });
    const w = call("widget.show", { tag: "w", frame: "raw" }).widget;
    call("widget.ask", { id: w.id });
  }
  log("drawn");
  period(0);
}
export function tick(now) {}
export function event(topic, data) {}
"#;

/// Two panes side by side, a bar at column 29.
const SPLIT: &str = r#"printf '\033[2J\033[H'
i=1
while [ $i -le 16 ]; do
  printf '\033[%d;1Hleft %d\033[%d;30H│\033[%d;31Hright %d' $i $i $i $i $i
  i=$((i+1))
done
exec sleep 1000
"#;

fn wide_manifest(id: &str, surface: &str, permissions: &str) -> String {
    format!(
        "[app]\nid = \"{id}\"\nversion = \"0.1.0\"\nsummary = \"A row.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
         surface = \"{surface}\"\n\n[permissions]\n{permissions}\n\n[options]\n\
         row = {{ type = \"int\", default = 0, doc = \"the row\" }}\n\
         widget = {{ type = \"bool\", default = false, doc = \"a widget\" }}\n"
    )
}

#[test]
fn a_store_app_is_clipped_to_its_surface() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let sb = Sandbox::new("surface");
    let wide = "screen-wide = \"anywhere\"";
    let apps = [
        ("onframe", "frame", ""),
        ("inwidget", "widget", ""),
        ("narrow", "screen", ""),
        ("wide", "screen", wide),
    ];
    for (id, surface, permissions) in apps {
        let repo = sb.dir.join(format!("repo-{id}"));
        repository(
            &repo,
            &[
                ("ptyrc-app.toml", wide_manifest(id, surface, permissions)),
                ("app.js", WIDE.to_string()),
                ("LICENSE", GPL.to_string()),
            ],
        );
        let origin = format!("local:{}", repo.display());
        let out = sb.run(&["store", "add", &origin, "--yes"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .env("PTYRC_JS", &js)
        .args(["run", "--name", "s", "--headless", "--size", "60x16"])
        .args(["--", "sh", "-c", SPLIT])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("s");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("s");
    // The left pane.
    let mut left = Value::Null;
    wait_for(
        || {
            let frames = client.call("frame.list", json!({})).unwrap();
            let found = (frames["frames"].as_array().unwrap().iter())
                .find(|f| f["kind"] == "pane" && f["rect"][0] == 0 && f["state"] == "stable")
                .cloned();
            found.map(|f| left = f["id"].clone()).is_some()
        },
        "the panes",
    );
    let start = |client: &mut Client, store: &str, row: i64, widget: bool| {
        let args = json!({"row": row, "widget": widget});
        let started = client
            .call(
                "app.start",
                json!({"store": store, "args": args, "frame": left}),
            )
            .unwrap();
        started["app"]["id"].as_u64().unwrap()
    };
    // The first compiles the interpreter; the others find it cached.
    let mut ids = vec![start(&mut client, "onframe", 0, false)];
    let drawn = |client: &mut Client, id: u64| {
        let log = log_of(client, id);
        assert!(!log.contains("exit:"), "{log}");
        log.contains("drawn")
    };
    wait_slow(|| drawn(&mut client, ids[0]), "drawn");
    ids.push(start(&mut client, "inwidget", 1, true));
    ids.push(start(&mut client, "narrow", 2, false));
    ids.push(start(&mut client, "wide", 3, false));
    for id in ids.clone() {
        wait_slow(|| drawn(&mut client, id), "drawn");
    }
    let text = client
        .call("screen.text", json!({"sprites": true}))
        .unwrap();
    let row = |y: usize| text["lines"][y].as_str().unwrap_or_default().to_string();
    // Behind the text: `=` in its blank cells. Left of the bar, right.
    let sides = |y: usize| {
        let row: Vec<char> = row(y).chars().collect();
        let side = |r: &[char]| r.contains(&'=');
        (
            side(&row[..29.min(row.len())]),
            side(&row[29.min(row.len())..]),
        )
    };
    // Its frame: the left pane alone.
    assert_eq!(sides(0), (true, false), "{text}");
    // `widget`: a bare sprite is shown nowhere.
    assert_eq!(sides(1), (false, false), "{text}");
    // `screen` without `screen-wide`: its frame.
    assert_eq!(sides(2), (true, false), "{text}");
    // With it: anywhere.
    assert_eq!(sides(3), (true, true), "{text}");

    // Its widget, shown without a layer, stays behind when focused.
    let mut widget = Value::Null;
    wait_slow(
        || {
            let list = client.call("widget.list", json!({})).unwrap();
            let found = (list["widgets"].as_array().unwrap().iter())
                .find(|w| w["tag"] == "w" && !w["sprite"].is_null())
                .cloned();
            found.map(|w| widget = w).is_some()
        },
        "the widget shown",
    );
    client
        .call("widget.focus", json!({"id": widget["id"]}))
        .unwrap();
    let layer = |client: &mut Client| {
        let list = client.call("draw.list", json!({})).unwrap();
        let sprite = (list["sprites"].as_array().unwrap().iter())
            .find(|s| s["id"] == widget["sprite"])
            .cloned()
            .unwrap_or(Value::Null);
        sprite["layer"].as_i64()
    };
    let behind = -(2 + (ids[1] % 97) as i64);
    wait_for(|| layer(&mut client) == Some(behind), "behind when focused");
}

/// A look-alike of the driver's pop-up: a box of double lines, white on
/// blue, ` ptyrc ` at its foot, asked on `hud`; a widget with `alert`.
const FAKE: &str = r#"
import { call, log, period } from "ptyrc";
export function init(args) {
  const rows = ["╔═ cat ─ native ═══════╗", "║ This app asks to:    ║", "╚═════════════ ptyrc ═╝"];
  const style = { fg: 15, bg: 4, bold: true };
  call("draw.put", { content: { rows, style }, layer: "hud", z: 1000, anchor: { at: [2, 2], space: "view" } });
  call("widget.declare", { widgets: [{ tag: "w", size: [6, 1] }], border: "alert" });
  call("widget.show", { tag: "w", layer: "hud", border: { base: "double", style } });
  log("drawn");
  period(0);
}
export function tick(now) {}
export function event(topic, data) {}
"#;

/// The store app `id` in a repository of `sb`, installed.
fn install(sb: &Sandbox, id: &str, manifest: String, script: &str) -> String {
    let repo = sb.dir.join(format!("repo-{id}"));
    repository(
        &repo,
        &[
            ("ptyrc-app.toml", manifest),
            ("app.js", script.to_string()),
            ("LICENSE", GPL.to_string()),
        ],
    );
    let origin = format!("local:{}", repo.display());
    let out = sb.run(&["store", "add", &origin, "--yes"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    origin
}

#[test]
fn a_look_alike_of_the_pop_up() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let sb = Sandbox::new("lookalike");
    let wide = "screen-wide = \"anywhere\"";
    let front = format!("{wide}\nfront = \"over the text\"");
    install(&sb, "fake", wide_manifest("fake", "screen", wide), FAKE);
    let origin = install(
        &sb,
        "fakefront",
        wide_manifest("fakefront", "screen", &front),
        FAKE,
    );
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
    let mut placed = |store: &str| {
        let started = client.call("app.start", json!({"store": store})).unwrap();
        let id = started["app"]["id"].as_u64().unwrap();
        wait_slow(
            || {
                let log = log_of(&mut client, id);
                assert!(!log.contains("exit:"), "{log}");
                log.contains("drawn")
            },
            "drawn",
        );
        let conns = &client.call("app.list", json!({})).unwrap()["apps"];
        let conns = (conns.as_array().unwrap().iter())
            .find(|a| a["id"] == id)
            .map(|a| a["conns"].clone())
            .unwrap();
        let mine = |item: &&Value| conns.as_array().unwrap().contains(&item["owner"]);
        let sprites = client.call("draw.list", json!({})).unwrap();
        let layers: Vec<i64> = (sprites["sprites"].as_array().unwrap().iter())
            .filter(mine)
            .filter_map(|s| s["layer"].as_i64())
            .collect();
        let widgets = client.call("widget.list", json!({})).unwrap();
        let borders: Vec<Value> = (widgets["widgets"].as_array().unwrap().iter())
            .filter(mine)
            .map(|w| w["border"].clone())
            .collect();
        (id, layers, borders)
    };
    // Without `front`: behind the text, in its band; the looks plain.
    let (id, layers, borders) = placed("fake");
    assert_eq!(layers, [-(2 + (id % 97) as i64); 2], "the box, the widget");
    assert_eq!(borders, [json!("plain")]);
    // With it: in front, below the driver's top band.
    let (id, layers, borders) = placed("fakefront");
    assert_eq!(layers, [2 + (id % 97) as i64; 2]);
    assert_eq!(borders, [json!("plain")]);
    drop(_driver);

    // On a terminal: the key that hides all tells it apart.
    let config = sb.config().join("ptyrc");
    std::fs::create_dir_all(&config).unwrap();
    let allowed = format!("[app.\"{origin}\"]\nallow = [\"screen-wide\", \"front\"]\n");
    std::fs::write(config.join("permissions.toml"), allowed).unwrap();
    let env = |name: &str, path: &Path| format!("{name}={}", path.display());
    let (box_, js_) = (env("PTYRC_BOX", &runner()), env("PTYRC_JS", &js));
    let data = env("XDG_DATA_HOME", &sb.dir.join("data"));
    let state = env("XDG_STATE_HOME", &sb.dir.join("state"));
    let cache = env("XDG_CACHE_HOME", &sb.dir.join("cache"));
    let argv = [
        "env", &box_, &js_, &data, &state, &cache, BIN, "run", "--name", "t", "cat",
    ];
    let (mut host, _pty) = common::HostPty::spawn(&sb, &argv);
    let host_text = |client: &mut Client| {
        let dump = client.call("debug.dump", json!({})).unwrap();
        let lines = dump["host"].as_array().unwrap().iter();
        lines
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let until = |host: &mut common::HostPty, ready: &mut dyn FnMut() -> bool, what: &str| {
        wait_slow(
            || {
                host.pump();
                ready()
            },
            what,
        )
    };
    until(&mut host, &mut || sb.socket("t").exists(), "the session");
    let mut client = sb.client("t");
    client
        .call("app.start", json!({"store": "fakefront"}))
        .unwrap();
    let fake = "This app asks to:";
    until(
        &mut host,
        &mut || host_text(&mut client).contains(fake),
        "the fake",
    );
    host.type_in(b"\x14h");
    until(
        &mut host,
        &mut || !host_text(&mut client).contains(fake),
        "hidden",
    );
    host.type_in(b"\x14h");
    until(
        &mut host,
        &mut || host_text(&mut client).contains(" local:"),
        "told",
    );
}

/// Holds `shape` and asks two limits; stays.
const LIMITED: &str = r#"[app]
id = "limited"
version = "0.1.0"
summary = "Asks for more."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"

[permissions]
shape = "to see"

[limits]
memory-mib = 128
tick-ms = 100
"#;

const STAYS: &str = r#"
import { log, period } from "ptyrc";
export function init(args) { log("up"); period(0); }
export function tick(now) {}
export function event(topic, data) {}
"#;

/// `--manifest` of the runner's command line of `app`: `granted`, and
/// the limits.
fn told(app: &Value) -> Value {
    let argv = app["argv"].as_array().unwrap();
    assert_eq!(argv[1], "--manifest", "{app}");
    serde_json::from_str(argv[2].as_str().unwrap()).unwrap()
}

#[test]
fn limits_granted_told_to_the_runner_and_anew_at_a_restart() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let sb = Sandbox::new("limits");
    let origin = install(&sb, "limited", LIMITED.to_string(), STAYS);
    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .env("PTYRC_JS", &js)
        .args(["run", "--name", "s", "--headless", "--", "cat"])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("s");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("s");

    // Headless: granted as declared, each named in a warning.
    let started = client
        .call("app.start", json!({"store": "limited"}))
        .unwrap();
    let warned = "no screen to ask: granted as declared: shape, limit-memory, limit-time";
    assert_eq!(started["warnings"], json!([warned]), "{started}");
    let app = &started["app"];
    let id = app["id"].as_u64().unwrap();
    let given = told(app);
    assert_eq!(
        given["granted"],
        json!(["shape", "limit-memory", "limit-time"])
    );
    assert_eq!(given["limits"], json!({"memory-mib": 128, "tick-ms": 100}));
    assert_eq!(given["id"], "limited");
    wait_slow(|| log_of(&mut client, id).contains("up"), "started");
    assert!(log_of(&mut client, id).contains(warned));

    // A no to a limit, then a restart: the runner is told anew.
    let config = sb.config().join("ptyrc");
    std::fs::create_dir_all(&config).unwrap();
    let answers = format!(
        "[app.\"{origin}\"]\nallow = [\"shape\", \"limit-time\"]\ndeny = [\"limit-memory\"]\n"
    );
    std::fs::write(config.join("permissions.toml"), answers).unwrap();
    let restarted = client.call("app.restart", json!({"app": id})).unwrap();
    assert!(restarted.get("warnings").is_none(), "{restarted}");
    assert_eq!(
        told(&restarted["app"])["granted"],
        json!(["shape", "limit-time"])
    );
    let runs = |client: &mut Client| {
        let apps = client.call("app.list", json!({})).unwrap();
        let app = (apps["apps"].as_array().unwrap().iter()).find(|a| a["id"] == id);
        app.map(|a| {
            (
                a["runs"].as_u64().unwrap_or(0),
                a["pid"].as_u64().unwrap_or(0),
            )
        })
    };
    wait_slow(
        || runs(&mut client).is_some_and(|(r, _)| r == 2),
        "run again",
    );
    let pid = runs(&mut client).unwrap().1;
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
    let cmdline = String::from_utf8_lossy(&cmdline);
    assert!(
        cmdline.contains(r#""granted":["shape","limit-time"]"#),
        "{cmdline}"
    );

    // `ptyrc app`: the warning on its stderr.
    std::fs::remove_file(config.join("permissions.toml")).unwrap();
    let run = sb.run(&["-s", "s", "app", "limited"]);
    let err = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{err}");
    assert!(err.contains(&format!("ptyrc: warning: {warned}")), "{err}");
}

#[test]
fn the_pop_up_of_a_store_app_shows_its_limits() {
    let sb = Sandbox::new("limitsasked");
    install(&sb, "limited", LIMITED.to_string(), STAYS);
    let env = |name: &str, path: &Path| format!("{name}={}", path.display());
    let box_ = env("PTYRC_BOX", &runner());
    let data = env("XDG_DATA_HOME", &sb.dir.join("data"));
    let state = env("XDG_STATE_HOME", &sb.dir.join("state"));
    let cache = env("XDG_CACHE_HOME", &sb.dir.join("cache"));
    let argv = [
        "env",
        &box_,
        &data,
        &state,
        &cache,
        BIN,
        "run",
        "--no-hotkey",
        "--name",
        "t",
        "cat",
    ];
    let (mut host, _pty) = common::HostPty::spawn(&sb, &argv);
    let host_text = |client: &mut Client| {
        let dump = client.call("debug.dump", json!({})).unwrap();
        let lines = dump["host"].as_array().unwrap().iter();
        lines
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let until = |host: &mut common::HostPty, ready: &mut dyn FnMut() -> bool, what: &str| {
        wait_slow(
            || {
                host.pump();
                ready()
            },
            what,
        )
    };
    until(&mut host, &mut || sb.socket("t").exists(), "the session");
    let mut asker = sb.client("t");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(code(asker.call("app.start", json!({"store": "limited"}))));
    });
    let mut client = sb.client("t");
    until(
        &mut host,
        &mut || host_text(&mut client).contains("This app asks to:"),
        "the pop-up",
    );
    let shown = host_text(&mut client);
    assert!(shown.contains("[x] see where the text is"), "{shown}");
    assert!(shown.contains("[x] use up to 128 MiB of memory"), "{shown}");
    assert!(shown.contains("[x] use up to 100 ms a tick"), "{shown}");
    host.type_in(b"\x1b");
    until(
        &mut host,
        &mut || !host_text(&mut client).contains("asks to"),
        "closed",
    );
    let answer = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(answer, "cancelled");
}

const OPTIONS_MANIFEST: &str = r#"[app]
id = "opts"
version = "0.1.0"
summary = "Logs what it was given."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"

[permissions]
shape = "the walls"
keys-arrows = "steering"
files = "the files named"

[options]
grab = { type = "bool", default = false, short = "g", needs = ["keys-arrows"], doc = "steer" }
word = { type = "word", doc = "a word" }
mark = { type = "pattern", list = true, doc = "marked" }
stop = { type = "file", doc = "words left out" }
"#;

const OPTIONS: &str = r#"
import { call, log, period } from "ptyrc";
let n = 0;
export function init(args) {
  log("args=" + JSON.stringify(args));
  const s = call("draw.put", { content: { rows: ["x"] }, at: [0, 0] });
  const mine = call("draw.list", {}).sprites.find((x) => x.id == s.id);
  log("sprite=" + JSON.stringify(mine));
  period(100);
}
export function tick(now) {
  if (++n != 5) return;
  const find = (p) => call("screen.find", p).matches.length;
  log("pattern=" + find({ pattern: "fish xyz" }) + " other=" + find({ pattern: "abc" })
      + " regex=" + find({ pattern: "fish xyz", regex: true }) + " word=" + find({ pattern: "rose" }));
  period(0);
}
export function event(topic, data) {}
"#;

#[test]
fn the_options_of_a_store_app() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let sb = Sandbox::new("options");
    install(&sb, "opts", OPTIONS_MANIFEST.to_string(), OPTIONS);
    let stop = sb.dir.join("stop.txt");
    std::fs::write(&stop, "the\n").unwrap();
    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", runner())
        .env("PTYRC_JS", &js)
        .args(["run", "--name", "s", "--headless", "--size", "60x16", "--"])
        .args(["sh", "-c", "printf 'abc fish xyz Rose\\n'; exec cat"])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    let socket = sb.socket("s");
    wait_for(
        || std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "socket",
    );
    let mut client = sb.client("s");

    // `--help`: from the manifest, the launcher's options beside.
    let help = sb.run(&["-s", "s", "app", "opts", "--help"]);
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(
        help.contains("-g, --grab") && help.contains("[asks: keys-arrows]"),
        "{help}"
    );
    assert!(help.contains("--front, --back"), "{help}");

    // Refused by the launcher: a word outside the rule.
    let bad = sb.run(&["-s", "s", "app", "opts", "--word", "a b", "--no-wait"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("a word is"));

    // Given: what `--grab` needs is asked, the rest is the user's hand.
    let stop_arg = stop.to_string_lossy().into_owned();
    let run = sb.run(&[
        "-s",
        "s",
        "app",
        "opts",
        "-g",
        "--word",
        "Rose",
        "--mark",
        "fish xyz",
        "--stop",
        &stop_arg,
        "--front",
        "--alpha",
        "0.5",
        "--no-wait",
    ]);
    let err = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{err}");
    assert!(
        err.contains("granted as declared: shape, keys-arrows"),
        "{err}"
    );
    let apps = client.call("app.list", json!({})).unwrap();
    let app = apps["apps"][0].clone();
    let id = app["id"].as_u64().unwrap();
    assert_eq!(told(&app)["granted"], json!(["shape", "keys-arrows"]));
    wait_slow(|| log_of(&mut client, id).contains("pattern="), "the finds");
    let log = log_of(&mut client, id);
    assert!(
        log.contains(r#""stop":{"file":"f0","name":"stop.txt"}"#),
        "{log}"
    );
    assert!(!log.contains(&stop_arg), "a path reached the guest: {log}");
    assert!(
        log.contains(r#""word":"Rose""#) && log.contains(r#""grab":true"#),
        "{log}"
    );
    assert!(log.contains(r#""alpha":0.5"#), "{log}");
    let layer = log.split(r#""layer":"#).nth(1).unwrap_or_default();
    assert!(
        layer.starts_with(|c: char| c.is_ascii_digit()),
        "in front: {log}"
    );
    // The user's pattern, exactly, and the user's word; nothing else.
    assert!(log.contains("pattern=1 other=0 regex=0 word=1"), "{log}");

    // Restarted: asked anew from the same options, the files kept.
    let again = client.call("app.restart", json!({"app": id})).unwrap();
    assert_eq!(
        told(&again["app"])["granted"],
        json!(["shape", "keys-arrows"])
    );

    // Not given: nothing more than the rest.
    let plain = client.call("app.start", json!({"store": "opts"})).unwrap();
    assert_eq!(told(&plain["app"])["granted"], json!(["shape"]));

    // The driver's checks.
    for params in [
        json!({"argv": ["true"], "front": true}),
        json!({"store": "opts", "alpha": 2}),
        json!({"store": "opts", "prefer": "middle"}),
        json!({"store": "opts", "border": "nope"}),
        json!({"store": "opts", "args": {"word": "a b"}}),
        json!({"store": "opts", "args": {"stop": {"file": "f0", "name": "x"}}}),
        json!({"store": "opts", "args": {"stop": {"file": "f0", "name": "x"}},
               "files": [{"handle": "f0", "path": "stop.txt", "kind": "file"}]}),
        json!({"store": "opts", "args": {"stop": {"file": "f0", "name": "x"}},
               "files": [{"handle": "f0", "path": "/dev/null", "kind": "file"}]}),
    ] {
        assert_eq!(
            code(client.call("app.start", params.clone())),
            "bad-params",
            "{params}"
        );
    }
}
