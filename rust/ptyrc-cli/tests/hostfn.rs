// SPDX-License-Identifier: GPL-3.0-or-later
//! The host functions through a session: `clock.*`, `storage.*`,
//! `app.result`, held, not held, and of the user's connection.

mod common;

use common::{BIN, Sandbox};
use ptyrc_proto::ClientError;
use serde_json::{Value, json};
use std::path::Path;

/// `ptyrc call METHOD PARAMS` lines of a child, each answer to
/// `out/N.json`, each error to `out/N.err`; `out/done` at the end.
fn script(out: &Path, calls: &[(&str, Value)]) -> String {
    let mut s = String::from("set -u\n");
    for (n, (method, params)) in calls.iter().enumerate() {
        s.push_str(&format!(
            "{BIN} --json call {method} '{params}' > {o}/{n}.json 2> {o}/{n}.err\n",
            o = out.display()
        ));
    }
    s.push_str(&format!("touch {}/done\n", out.display()));
    s
}

fn answer(out: &Path, n: usize) -> Result<Value, String> {
    let text = std::fs::read_to_string(out.join(format!("{n}.json"))).unwrap();
    match serde_json::from_str(&text) {
        Ok(value) => Ok(value),
        Err(_) => Err(std::fs::read_to_string(out.join(format!("{n}.err"))).unwrap()),
    }
}

/// Start `calls` as the child `name` declaring `clock` and `storage`;
/// its info once it has ended.
fn run(sb: &Sandbox, name: &str, calls: &[(&str, Value)]) -> (std::path::PathBuf, Value) {
    let out = sb.dir.join(format!("out-{name}"));
    std::fs::create_dir_all(&out).unwrap();
    let mut client = sb.client("h");
    let declares = json!({"permissions": {"clock": "the time", "storage": "the scores"}});
    let started = client
        .call(
            "app.start",
            json!({"argv": ["sh", "-c", script(&out, calls)], "name": name, "declares": declares}),
        )
        .unwrap();
    let id = started["app"]["id"].clone();
    common::wait_for(|| out.join("done").exists(), "the calls");
    let mut info = Value::Null;
    common::wait_for(
        || {
            let apps = client.call("app.list", json!({})).unwrap();
            let app = apps["apps"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id);
            info = app.cloned().unwrap_or_default();
            info["state"] == "exited"
        },
        "the exit",
    );
    (out, info)
}

#[test]
fn held_by_an_app() {
    let sb = Sandbox::new("hostfn-held");
    let _driver = sb.headless("h", "40x4", &["cat"]);
    let calls = [
        ("clock.now", json!({})),
        ("clock.zone", json!({"zone": "UTC", "at": [0, 1000]})),
        ("clock.zone", json!({"zone": "../../etc/passwd"})),
        ("storage.set", json!({"key": "scores", "value": "[3,1]"})),
        ("storage.get", json!({"key": "scores"})),
        ("storage.list", json!({})),
        ("storage.get", json!({"key": ".."})),
        ("app.result", json!({"value": ["a", "b"], "status": 3})),
        (
            "batch",
            json!({"calls": [{"method": "storage.list"}, {"method": "app.result", "params": {"value": "last", "status": 4}}]}),
        ),
    ];
    let (out, info) = run(&sb, "keeper", &calls);
    let now = answer(&out, 0).unwrap();
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let unix_ms = now["unix_ms"].as_i64().unwrap();
    assert!((wall - unix_ms).abs() < 60_000, "{now}");
    assert!(now.get("degree").is_none(), "{now}");
    assert!(now["offset_s"].is_i64() && now["abbr"].is_string(), "{now}");
    let zone = answer(&out, 1).unwrap();
    assert_eq!(
        zone,
        json!({"zone": "UTC", "offsets": [
            {"at": 0, "offset_s": 0, "abbr": "UTC", "next": null},
            {"at": 1000, "offset_s": 0, "abbr": "UTC", "next": null}]})
    );
    assert!(answer(&out, 2).unwrap_err().contains("bad-params"));
    assert_eq!(
        answer(&out, 3).unwrap(),
        json!({"used": 5, "quota": 1048576})
    );
    assert_eq!(answer(&out, 4).unwrap(), json!({"value": "[3,1]"}));
    assert_eq!(
        answer(&out, 5).unwrap()["keys"],
        json!([{"key": "scores", "size": 5}])
    );
    assert!(answer(&out, 6).unwrap_err().contains("bad-params"));
    assert_eq!(answer(&out, 7).unwrap(), json!({}));
    // In a batch: what waits on the worker is refused, the rest answered.
    let batch = answer(&out, 8).unwrap();
    assert_eq!(batch["results"][0]["error"]["code"], "bad-request");
    assert_eq!(batch["results"][1]["result"], json!({}));
    // The last result holds, told with the entry.
    assert_eq!(info["result"], json!({"value": "last", "status": 4}));
    let file = sb.dir.join("state/ptyrc/storage/native%2Fkeeper/scores");
    assert_eq!(std::fs::read_to_string(file).unwrap(), "[3,1]");
}

#[test]
fn not_held_lowered_or_denied() {
    let sb = Sandbox::new("hostfn-denied");
    let dir = sb.config().join("ptyrc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("permissions.toml"),
        "[app.\"native/plain\"]\ndeny = [\"clock\", \"storage\"]\n",
    )
    .unwrap();
    let _driver = sb.headless("h", "40x4", &["cat"]);
    let calls = [
        ("clock.now", json!({})),
        ("clock.zone", json!({"zone": "Asia/Tokyo", "at": [5]})),
        ("storage.get", json!({"key": "scores"})),
    ];
    let (out, info) = run(&sb, "plain", &calls);
    let now = answer(&out, 0).unwrap();
    assert_eq!(
        (&now["zone"], &now["offset_s"], &now["degree"]),
        (&json!("UTC"), &json!(0), &json!("none"))
    );
    assert!(
        now["unix_ms"].as_i64().unwrap() < 60_000,
        "session time: {now}"
    );
    let zone = answer(&out, 1).unwrap();
    assert_eq!(
        zone,
        json!({"zone": "UTC", "degree": "none",
               "offsets": [{"at": 5, "offset_s": 0, "abbr": "UTC", "next": null}]})
    );
    let e = answer(&out, 2).unwrap_err();
    assert!(e.contains("denied") && e.contains("`storage`"), "{e}");
    assert_eq!(info["result"], Value::Null);
}

#[test]
fn the_users_connection() {
    let sb = Sandbox::new("hostfn-user");
    let _driver = sb.headless("h", "40x4", &["cat"]);
    let mut client = sb.client("h");
    let now = client.call("clock.now", json!({})).unwrap();
    assert!(
        now["unix_ms"].as_i64().unwrap() > 1_700_000_000_000,
        "{now}"
    );
    for (method, params) in [
        ("storage.get", json!({"key": "a"})),
        ("app.result", json!({"value": 1})),
    ] {
        let Err(ClientError::Rpc(e)) = client.call(method, params) else {
            panic!("{method}: answered");
        };
        assert_eq!(e.code.as_str(), "not-usable", "{method}");
        assert!(e.message.starts_with("no app"), "{}", e.message);
    }
}

const JS: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";

/// The store app `id` of `manifest` (the `[app]` keys after `script`)
/// and `script`, installed from a repository of `sb`.
fn install(sb: &Sandbox, id: &str, more: &str, script: &str) {
    let repo = sb.dir.join(format!("repo-{id}"));
    std::fs::create_dir_all(&repo).unwrap();
    let manifest = format!(
        "[app]\nid = \"{id}\"\nversion = \"0.1.0\"\nsummary = \"s\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n{more}"
    );
    std::fs::write(repo.join("ptyrc-app.toml"), manifest).unwrap();
    std::fs::write(repo.join("app.js"), script).unwrap();
    std::fs::write(
        repo.join("LICENSE"),
        "GNU GENERAL PUBLIC LICENSE\nVersion 3\n",
    )
    .unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=agent",
            "-c",
            "user.email=agent@example.org",
            "commit",
            "-q",
            "-m",
            "a",
        ],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    let origin = format!("local:{}", repo.display());
    let out = sb.run(&["store", "add", &origin, "--yes"]);
    assert!(out.status.success(), "{out:?}");
}

const ANSWERS: &str = r#"
import { call, exit } from "ptyrc";
export function init(args) {
  let big = "";
  try { call("app.result", { value: "x".repeat(70000) }); }
  catch (e) { big = e.code; }
  call("app.result", { value: "first" });
  call("storage.set", { key: "k", value: "kept" });
  call("app.result", { value: ["one", "\u001b]52;c;aGk=\u0007two", big], status: 7 });
  exit(0);
}
export function tick(now) {}
export function event(topic, data) {}
"#;

const STAYS: &str = r#"
import { period } from "ptyrc";
export function init(args) { period(0); }
export function tick(now) {}
export function event(topic, data) {}
"#;

#[test]
fn a_result_to_the_caller() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS);
    if !js.is_file() {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_JS").is_none(),
            "no interpreter"
        );
        eprintln!("SKIPPED: no interpreter module");
        return;
    }
    let runner = Path::new(BIN).with_file_name("ptyrc-box");
    if !runner.is_file() {
        let built = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
            .status()
            .unwrap();
        assert!(built.success());
    }
    let sb = Sandbox::new("hostfn-result");
    install(
        &sb,
        "answers",
        "wait = true\n[permissions]\nstorage = \"a value\"\n",
        ANSWERS,
    );
    install(&sb, "stays", "", STAYS);
    let mut driver = sb.ptyrc();
    driver
        .env("PTYRC_BOX", &runner)
        .env("PTYRC_JS", &js)
        .args(["run", "--name", "h", "--headless", "--", "cat"])
        .stdin(std::process::Stdio::null());
    let _driver = common::Proc(driver.spawn().unwrap());
    common::wait_for(
        || std::os::unix::net::UnixStream::connect(sb.socket("h")).is_ok(),
        "socket",
    );
    // `wait = true`: waited by default; the last result, escaped.
    let out = sb.run(&["-s", "h", "app", "answers"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(7), "{out:?}");
    assert_eq!(stdout, "one\n\\u{1b}]52;c;aGk=\\u{7}two\nbad-params\n");
    let out = sb.run(&["-s", "h", "--json", "app", "answers"]);
    assert_eq!(out.status.code(), Some(7), "{out:?}");
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(printed[0], "one");
    // Its storage, under its origin; removed by `store rm --purge`.
    let origin = format!("local:{}", sb.dir.join("repo-answers").display());
    let dir: String = (origin.bytes())
        .map(|b| match b.is_ascii_alphanumeric() || b"._-".contains(&b) {
            true => (b as char).to_string(),
            false => format!("%{b:02X}"),
        })
        .collect();
    let kept = sb.dir.join("state/ptyrc/storage").join(dir).join("k");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "kept");
    // Not waited: started, its info.
    let out = sb.run(&["-s", "h", "app", "answers", "--no-wait"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // Stopped without a result: 1, nothing printed.
    let mut waiting = common::Proc(
        sb.ptyrc()
            .args(["-s", "h", "app", "stays", "--wait"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut client = sb.client("h");
    let mut id = Value::Null;
    common::wait_for(
        || {
            let apps = client.call("app.list", json!({})).unwrap();
            let app = apps["apps"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["name"] == "stays");
            id = app.map(|a| a["id"].clone()).unwrap_or_default();
            app.is_some_and(|a| a["state"] == "running")
        },
        "stays",
    );
    client.call("app.stop", json!({"app": id})).unwrap();
    assert_eq!(waiting.wait_exit(), 1);
    let mut printed = String::new();
    std::io::Read::read_to_string(&mut waiting.0.stdout.take().unwrap(), &mut printed).unwrap();
    assert_eq!(printed, "");
    let out = sb.run(&["store", "rm", "answers", "--purge"]);
    assert!(out.status.success(), "{out:?}");
    assert!(!kept.exists() && !kept.parent().unwrap().exists());
}
