// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the `ptyrc` binary, headless and hosted in a pty.

use ptyrc_proto::{Client, ClientError, ErrorCode};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const TIMEOUT: Duration = Duration::from_secs(10);

/// A private runtime directory, so tests do not see each other or the
/// user's sessions.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        let dir =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Sandbox { dir }
    }

    fn socket(&self, name: &str) -> PathBuf {
        self.dir.join("ptyrc").join(format!("{name}.sock"))
    }

    fn log(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.log"))
    }

    fn ptyrc(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env("XDG_RUNTIME_DIR", &self.dir)
            .env_remove("PTYRC_SESSION")
            .env_remove("PTYRC_SOCKET");
        command
    }

    /// A headless driver named `name` running `argv`, its host stream in
    /// `log(name)`.
    fn headless(&self, name: &str, argv: &[&str]) -> Proc {
        let child = Proc(
            self.ptyrc()
                .args([
                    "run",
                    "--name",
                    name,
                    "--headless",
                    "--size",
                    "60x10",
                    "--host-log",
                ])
                .arg(self.log(name))
                .arg("--")
                .args(argv)
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        self.wait_socket(name);
        child
    }

    fn wait_socket(&self, name: &str) {
        let path = self.socket(name);
        wait_for(|| UnixStream::connect(&path).is_ok(), "socket");
    }

    fn client(&self, name: &str) -> Client {
        Client::connect(&self.socket(name)).unwrap()
    }

    fn host_log(&self, name: &str) -> String {
        String::from_utf8_lossy(&std::fs::read(self.log(name)).unwrap_or_default()).into_owned()
    }

    fn wait_log(&self, name: &str, needle: &str) {
        wait_for(|| self.host_log(name).contains(needle), needle);
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait_for(mut ready: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A driver process, killed when the test leaves it behind.
struct Proc(Child);

impl Proc {
    fn id(&self) -> u32 {
        self.0.id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.0.try_wait()
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_exit(child: &mut Proc) -> i32 {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().unwrap_or(-1);
        }
        assert!(Instant::now() < deadline, "driver did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn headless_echo_and_exit() {
    let sb = Sandbox::new("echo");
    let mut driver = sb.headless("cat", &["cat"]);
    let mut client = sb.client("cat");
    assert_eq!(client.hello()["protocol"], 1);
    assert_eq!(client.hello()["session"], "cat");
    assert_eq!(client.hello()["size"], json!([60, 10]));
    assert_eq!(client.hello()["argv"], json!(["cat"]));

    client
        .call("input.send", json!({"text": "hi there"}))
        .unwrap();
    client
        .call("input.send", json!({"keys": ["enter"]}))
        .unwrap();
    // Once by the tty echo, once by cat.
    sb.wait_log("cat", "hi there\r\nhi there\r\n");
    // "QUJD" is "ABC". In one request: text, then keys, then bytes.
    client
        .call(
            "input.send",
            json!({"bytes": "QUJDDQ==", "text": "x", "keys": ["y"]}),
        )
        .unwrap();
    sb.wait_log("cat", "xyABC\r\nxyABC\r\n");

    let info = client.call("session.info", json!({})).unwrap();
    assert_eq!(info["exited"], false);
    assert_eq!(info["headless"], true);

    client
        .call("input.send", json!({"keys": ["ctrl+d"]}))
        .unwrap();
    assert_eq!(wait_exit(&mut driver), 0);
    assert!(!sb.socket("cat").exists(), "socket removed at exit");
}

#[test]
fn keys_follow_the_table() {
    let sb = Sandbox::new("keys");
    let mut driver = sb.headless("k", &["cat", "-v"]);
    let mut client = sb.client("k");
    client
        .call(
            "input.send",
            json!({"keys": ["up", "f5", "alt+x", "shift+tab", "plain", "enter"]}),
        )
        .unwrap();
    sb.wait_log("k", "^[[A^[[15~^[x^[[Zplain\r\n");
    match client.call("input.send", json!({})) {
        Err(ClientError::Rpc(e)) => assert_eq!(e.code, ErrorCode::BadParams),
        other => panic!("{other:?}"),
    }
    match client.call("input.send", json!({"bytes": "***"})) {
        Err(ClientError::Rpc(e)) => assert_eq!(e.code, ErrorCode::BadParams),
        other => panic!("{other:?}"),
    }
    client
        .call("input.send", json!({"keys": ["ctrl+d"]}))
        .unwrap();
    assert_eq!(wait_exit(&mut driver), 0);
}

#[test]
fn exit_status_is_the_apps() {
    let sb = Sandbox::new("status");
    let run = |argv: &[&str]| {
        sb.ptyrc()
            .args(["run", "--headless", "--"])
            .args(argv)
            .stdin(Stdio::null())
            .status()
            .unwrap()
            .code()
    };
    assert_eq!(run(&["sh", "-c", "exit 7"]), Some(7));
    assert_eq!(run(&["true"]), Some(0));
    // Killed by SIGTERM: 128 + 15.
    assert_eq!(run(&["sh", "-c", "kill -TERM $$"]), Some(143));
    // Not found.
    assert_eq!(run(&["/nonexistent/program"]), Some(2));
}

#[test]
fn output_before_exit_is_not_lost() {
    let sb = Sandbox::new("drain");
    let mut driver = sb.headless("d", &["sh", "-c", "sleep 0.2; seq 1 20000; echo the-end"]);
    assert_eq!(wait_exit(&mut driver), 0);
    let log = sb.host_log("d");
    assert!(
        log.contains("\r\n20000\r\nthe-end\r\n"),
        "tail: {:?}",
        &log[log.len().saturating_sub(40)..]
    );
    assert_eq!(log.lines().count(), 20001);
}

#[test]
fn app_environment() {
    let sb = Sandbox::new("env");
    let mut driver = sb.headless(
        "e1",
        &[
            "sh",
            "-c",
            "sleep 0.2; echo s=$PTYRC_SESSION; echo k=$PTYRC_SOCKET; stty size",
        ],
    );
    assert_eq!(wait_exit(&mut driver), 0);
    let log = sb.host_log("e1");
    assert!(log.contains("s=e1\r\n"), "{log}");
    assert!(
        log.contains(&format!("k={}\r\n", sb.socket("e1").display())),
        "{log}"
    );
    assert!(log.contains("10 60\r\n"), "{log}");
}

#[test]
fn session_discovery() {
    let sb = Sandbox::new("disc");
    let mut a = sb.headless("a", &["cat"]);
    // One session: found without a name.
    let out = sb
        .ptyrc()
        .args(["call", "session.info", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let info: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(info["session"], "a");

    let mut b = sb.headless("b", &["cat"]);
    // A socket nobody listens on.
    drop(UnixListener::bind(sb.socket("stale")).unwrap());
    assert!(sb.socket("stale").exists());

    let out = sb.ptyrc().args(["ls", "--json"]).output().unwrap();
    let list: Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["session"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["a", "b"]);
    assert!(!sb.socket("stale").exists(), "stale socket pruned");

    // Two sessions and no name: refused, both listed.
    let out = sb.ptyrc().args(["call", "session.info"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("a, b"), "{err}");

    let out = sb
        .ptyrc()
        .args(["-s", "b", "call", "session.info", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["session"],
        "b"
    );
    let out = sb
        .ptyrc()
        .env("PTYRC_SESSION", "a")
        .args(["call", "session.info", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["session"],
        "a"
    );
    let out = sb
        .ptyrc()
        .args(["-s", "nope", "call", "session.info"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));

    // A second driver under a live name is refused.
    let out = sb
        .ptyrc()
        .args(["run", "--name", "a", "--headless", "--", "true"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(sb.socket("a").exists());

    for name in ["a", "b"] {
        assert!(
            sb.ptyrc()
                .args(["-s", name, "keys", "ctrl+d"])
                .status()
                .unwrap()
                .success()
        );
    }
    assert_eq!(wait_exit(&mut a), 0);
    assert_eq!(wait_exit(&mut b), 0);
    let out = sb.ptyrc().args(["ls", "--json"]).output().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!([])
    );
}

#[test]
fn events_reach_every_subscriber() {
    let sb = Sandbox::new("events");
    let mut driver = sb.headless("ev", &["sh"]);
    let mut one = sb.client("ev");
    let mut two = sb.client("ev");
    let mut deaf = sb.client("ev");
    one.subscribe(&["exit", "size"]).unwrap();
    two.subscribe(&["exit"]).unwrap();

    one.call("session.resize", json!({"size": [50, 12]}))
        .unwrap();
    let event = one.next_event(Some(TIMEOUT)).unwrap().unwrap();
    assert_eq!(
        (event.event.as_str(), &event.data),
        ("size", &json!({"size": [50, 12]}))
    );
    assert_eq!(
        two.call("session.info", json!({})).unwrap()["size"],
        json!([50, 12])
    );
    // The app sees the new size.
    two.call("input.send", json!({"text": "stty size\r"}))
        .unwrap();
    sb.wait_log("ev", "12 50\r\n");
    assert!(matches!(
        one.call("session.resize", json!({"size": [0, 1]})),
        Err(ClientError::Rpc(_))
    ));

    deaf.call("input.send", json!({"text": "exit 5\r"}))
        .unwrap();
    for client in [&mut one, &mut two] {
        let event = client.next_event(Some(TIMEOUT)).unwrap().unwrap();
        assert_eq!(
            (event.event.as_str(), &event.data),
            ("exit", &json!({"status": 5}))
        );
    }
    assert!(matches!(
        deaf.next_event(Some(TIMEOUT)),
        Err(ClientError::Closed)
    ));
    assert_eq!(wait_exit(&mut driver), 5);
}

#[test]
fn batch_and_errors() {
    let sb = Sandbox::new("batch");
    let mut driver = sb.headless("b", &["cat"]);
    let mut client = sb.client("b");
    let result = client
        .call(
            "batch",
            json!({"calls": [
                {"method": "input.send", "params": {"text": "one "}},
                {"method": "no.such"},
                {"method": "input.send", "params": {"text": "two\r"}},
                {"method": "batch", "params": {"calls": []}},
                {"nomethod": 1},
            ]}),
        )
        .unwrap();
    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), 5);
    assert_eq!(results[0], json!({"result": {}}));
    assert_eq!(results[1]["error"]["code"], "unknown-method");
    assert_eq!(results[2], json!({"result": {}}));
    assert_eq!(results[3]["error"]["code"], "bad-request");
    assert_eq!(results[4]["error"]["code"], "bad-request");
    sb.wait_log("b", "one two\r\none two\r\n");

    match client.call("no.such", json!({})) {
        Err(ClientError::Rpc(e)) => assert_eq!(e.code, ErrorCode::UnknownMethod),
        other => panic!("{other:?}"),
    }
    match client.call("batch", json!({})) {
        Err(ClientError::Rpc(e)) => assert_eq!(e.code, ErrorCode::BadParams),
        other => panic!("{other:?}"),
    }

    // Raw lines: not JSON, split writes, two requests in one write.
    let mut raw = UnixStream::connect(sb.socket("b")).unwrap();
    let mut lines = BufReader::new(raw.try_clone().unwrap()).lines();
    let mut next = || serde_json::from_str::<Value>(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(next()["event"], "hello");
    raw.write_all(b"this is not json\n").unwrap();
    let reply = next();
    assert_eq!(
        (reply["id"].clone(), reply["error"]["code"].clone()),
        (Value::Null, json!("bad-request"))
    );
    raw.write_all(b"{\"id\":\"x\",\"meth").unwrap();
    raw.flush().unwrap();
    std::thread::sleep(Duration::from_millis(30));
    raw.write_all(b"od\":\"session.info\"}\n{\"id\":2,\"method\":\"subscribe\",\"params\":{\"topics\":[\"exit\"]}}\n").unwrap();
    let reply = next();
    assert_eq!(
        (reply["id"].clone(), reply["result"]["session"].clone()),
        (json!("x"), json!("b"))
    );
    assert_eq!(next(), json!({"id": 2, "result": {}}));
    raw.write_all(b"{\"id\":3}\n").unwrap();
    assert_eq!(next()["error"]["code"], "bad-request");

    client
        .call("input.send", json!({"keys": ["ctrl+d"]}))
        .unwrap();
    assert_eq!(next(), json!({"event": "exit", "data": {"status": 0}}));
    assert_eq!(wait_exit(&mut driver), 0);
}

#[test]
fn sigterm_cleans_up() {
    let sb = Sandbox::new("term");
    let mut driver = sb.headless("t", &["cat"]);
    let mut client = sb.client("t");
    client.subscribe(&["exit"]).unwrap();
    let pid = client.hello()["driver_pid"].as_u64().unwrap();
    assert_eq!(pid, driver.id() as u64);
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let event = client.next_event(Some(TIMEOUT)).unwrap().unwrap();
    assert_eq!(event.data, json!({"status": 143}));
    assert_eq!(wait_exit(&mut driver), 143);
    assert!(!sb.socket("t").exists());
}

/// The test is the host: it owns a pty and runs the driver on it.
mod hosted {
    use super::*;
    use rustix::termios::{LocalModes, tcgetattr};
    use std::os::fd::OwnedFd;

    struct HostPty {
        master: OwnedFd,
        seen: Vec<u8>,
    }

    impl HostPty {
        fn pump(&mut self) {
            let mut buf = [0u8; 65536];
            while let Ok(n) = rustix::io::read(&self.master, &mut buf) {
                if n == 0 {
                    break;
                }
                self.seen.extend_from_slice(&buf[..n]);
            }
        }

        fn wait_seen(&mut self, needle: &str) {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                self.pump();
                if String::from_utf8_lossy(&self.seen).contains(needle) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for {needle:?} in {:?}",
                    String::from_utf8_lossy(&self.seen)
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn type_in(&mut self, text: &str) {
            let mut rest = text.as_bytes();
            while !rest.is_empty() {
                match rustix::io::write(&self.master, rest) {
                    Ok(n) => rest = &rest[n..],
                    Err(_) => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        }

        fn canonical(&self) -> bool {
            let modes = tcgetattr(&self.master).unwrap().local_modes;
            modes.contains(LocalModes::ICANON) && modes.contains(LocalModes::ECHO)
        }
    }

    #[test]
    fn raw_passthrough_resize_restore() {
        let sb = Sandbox::new("hosted");
        let argv: Vec<String> = [BIN, "run", "--name", "h", "--", "sh"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let env = [
            (
                "XDG_RUNTIME_DIR".to_string(),
                sb.dir.to_string_lossy().into_owned(),
            ),
            ("PS1".to_string(), "$ ".to_string()),
            ("ENV".to_string(), String::new()),
        ];
        let pty = ptyrc_server::pty::spawn(&argv, &env, 80, 24).unwrap();
        let mut driver = Proc(pty.child);
        let mut host = HostPty {
            master: pty.master,
            seen: Vec::new(),
        };
        assert!(host.canonical(), "a fresh pty is in canonical mode");
        sb.wait_socket("h");
        wait_for(|| !host.canonical(), "raw mode");

        let mut client = sb.client("h");
        assert_eq!(client.hello()["size"], json!([80, 24]));
        client.subscribe(&["size", "input"]).unwrap();

        // Keys typed on the host reach the app; its output reaches the host.
        host.type_in("echo ma$((1+1))rk; stty size\r");
        host.wait_seen("ma2rk\r\n24 80\r\n");
        let event = client.next_event(Some(TIMEOUT)).unwrap().unwrap();
        assert_eq!(event.event, "input");

        // A control character is data, not a signal to the driver: the
        // inner shell gets ^C and gives a new prompt.
        host.type_in("sleep 100\r");
        std::thread::sleep(Duration::from_millis(100));
        host.type_in("\x03");
        host.type_in("echo ali$((1+1))ve\r");
        host.wait_seen("ali2ve\r\n");

        // Resize of the host reaches the app.
        ptyrc_server::pty::set_winsize(&host.master, 50, 12).unwrap();
        loop {
            let event = client.next_event(Some(TIMEOUT)).unwrap().unwrap();
            if event.event == "size" {
                assert_eq!(event.data, json!({"size": [50, 12]}));
                break;
            }
        }
        host.type_in("stty size\r");
        host.wait_seen("12 50\r\n");

        // Typed from a client, shown on the host.
        client
            .call("input.send", json!({"text": "echo pi$((1+1))lot\r"}))
            .unwrap();
        host.wait_seen("pi2lot\r\n");

        host.type_in("exit 3\r");
        let status = {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                host.pump();
                if let Some(status) = driver.try_wait().unwrap() {
                    break status.code();
                }
                assert!(Instant::now() < deadline, "driver did not exit");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        assert_eq!(status, Some(3));
        assert!(host.canonical(), "termios restored at exit");
        assert!(!sb.socket("h").exists());
    }

    #[test]
    fn sigterm_restores_the_terminal() {
        let sb = Sandbox::new("hterm");
        let argv: Vec<String> = [BIN, "run", "--name", "h", "--", "cat"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let env = [(
            "XDG_RUNTIME_DIR".to_string(),
            sb.dir.to_string_lossy().into_owned(),
        )];
        let pty = ptyrc_server::pty::spawn(&argv, &env, 80, 24).unwrap();
        let mut driver = Proc(pty.child);
        let host = HostPty {
            master: pty.master,
            seen: Vec::new(),
        };
        sb.wait_socket("h");
        wait_for(|| !host.canonical(), "raw mode");
        assert!(
            Command::new("kill")
                .args(["-HUP", &driver.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(wait_exit(&mut driver), 129);
        assert!(host.canonical(), "termios restored after SIGHUP");
        assert!(!sb.socket("h").exists());
    }
}
