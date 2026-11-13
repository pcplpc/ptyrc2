// SPDX-License-Identifier: GPL-3.0-or-later
//! What `ptyrc` says, and exits with, when it cannot do what was asked.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");

/// A private runtime directory.
struct Sandbox {
    dir: PathBuf,
    drivers: Vec<Child>,
}

/// The outcome of a verb.
struct Out {
    status: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    /// Fails with `status`; stderr is `ptyrc: <message>` and a hint line,
    /// three lines at most, and names no internal.
    #[track_caller]
    fn fails(&self, status: i32, message: &str, hint: &str) {
        assert_eq!(self.status, status, "status; stderr: {}", self.stderr);
        let mut lines = self.stderr.lines();
        let first = lines.next().unwrap_or("");
        assert!(
            first.starts_with("ptyrc: ") && first.contains(message),
            "message {message:?} not in {first:?}"
        );
        let rest: Vec<&str> = lines.collect();
        assert!(
            rest.iter().any(|l| l.contains(hint)),
            "hint {hint:?} not in {rest:?}"
        );
        assert!(rest.len() <= 5, "too long: {}", self.stderr);
        for internal in [
            "os error",
            "Os {",
            "panicked",
            "untagged enum",
            "RUST_BACKTRACE",
        ] {
            assert!(!self.stderr.contains(internal), "{}", self.stderr);
        }
    }
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("err-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Sandbox {
            dir,
            drivers: Vec::new(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env("XDG_RUNTIME_DIR", &self.dir)
            .env_remove("PTYRC_SESSION")
            .env_remove("PTYRC_SOCKET")
            .stdin(Stdio::null());
        command
    }

    fn socket(&self, name: &str) -> PathBuf {
        self.dir.join("ptyrc").join(format!("{name}.sock"))
    }

    /// A headless session running `cat`.
    fn session(&mut self, name: &str) {
        let child = self
            .command()
            .args([
                "run",
                "--name",
                name,
                "--headless",
                "--size",
                "40x8",
                "--",
                "cat",
            ])
            .spawn()
            .unwrap();
        self.drivers.push(child);
        let path = self.socket(name);
        let deadline = Instant::now() + Duration::from_secs(10);
        while UnixStream::connect(&path).is_err() {
            assert!(Instant::now() < deadline, "no socket");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn ptyrc(&self, args: &[&str]) -> Out {
        let out = self.command().args(args).output().unwrap();
        Out {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn file(&self, name: &str, content: &str, mode: u32) -> String {
        let path = self.dir.join(name);
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path.to_string_lossy().into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for child in &mut self.drivers {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn run_command_that_cannot_run() {
    let sb = Sandbox::new("run");
    let run = |program: &str| sb.ptyrc(&["run", "--headless", "--", program]);

    run("no-such-command-zz").fails(
        127,
        "no-such-command-zz: command not found in PATH",
        "spelling",
    );
    run("/no/such/dir/tool").fails(
        127,
        "/no/such/dir/tool: no such file",
        "/no/such/dir does not exist",
    );
    run("./absent").fails(127, "./absent: no such file", "check the path");
    run("/tmp").fails(126, "/tmp: is a directory", "program");

    let plain = sb.file("plain.txt", "text\n", 0o644);
    run(&plain).fails(126, "plain.txt: not executable", "chmod +x");
    let script = sb.file("script", "#!/no/such/interpreter\n", 0o755);
    run(&script).fails(126, "script: its interpreter is missing", "#!");
    let binary = sb.file("binary", "\x7fELF garbage", 0o755);
    let out = run(&binary);
    assert_eq!(out.status, 126, "{}", out.stderr);
    assert!(!out.stderr.contains("os error"), "{}", out.stderr);

    // A near miss names the command that exists.
    let out = sb
        .command()
        .env("PATH", "/usr/bin:/bin")
        .args(["run", "--headless", "--", "sleeep"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&out.stderr).contains("did you mean sleep?"));

    // No socket is left behind by a session that did not start.
    let left = std::fs::read_dir(sb.dir.join("ptyrc")).map_or(0, |d| d.count());
    assert_eq!(left, 0);
}

#[test]
fn run_session_names() {
    let mut sb = Sandbox::new("names");
    sb.ptyrc(&["run", "--headless", "--name", "a/b", "--", "true"])
        .fails(2, "bad session name: a/b", "letters, digits");
    sb.session("taken");
    sb.ptyrc(&["run", "--headless", "--name", "taken", "--", "true"])
        .fails(3, "session taken is already running", "ptyrc -s taken");
    let out = sb
        .command()
        .env("XDG_RUNTIME_DIR", "/proc/no-such-dir")
        .args(["run", "--headless", "--", "true"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no socket directory /proc/no-such-dir/ptyrc"),
        "{stderr}"
    );
    assert!(!stderr.contains("os error"), "{stderr}");
}

#[test]
fn no_session_several_sessions_dead_session() {
    let mut sb = Sandbox::new("sessions");
    sb.ptyrc(&["text"])
        .fails(3, "no session", "ptyrc run -- CMD");
    sb.ptyrc(&["-s", "ghost", "text"])
        .fails(3, "no such session: ghost", "ptyrc run --name ghost");
    sb.ptyrc(&["-s", "a/b", "text"])
        .fails(3, "bad session name: a/b", "ptyrc ls");
    let out = sb.ptyrc(&["ls"]);
    assert_eq!(out.status, 0);
    assert!(out.stdout.is_empty() && out.stderr.contains("no session"));

    sb.session("alpha");
    assert_eq!(sb.ptyrc(&["text"]).status, 0);
    sb.ptyrc(&["-s", "alpah", "text"]).fails(
        3,
        "no such session: alpah",
        "did you mean alpha? running: alpha",
    );

    sb.session("beta");
    sb.ptyrc(&["text"]).fails(
        3,
        "2 sessions, none chosen",
        "-s NAME or PTYRC_SESSION: alpha, beta",
    );
    assert_eq!(sb.ptyrc(&["-s", "beta", "text"]).status, 0);

    // A socket nobody listens on: said once, removed.
    let stale = sb.socket("stale");
    drop(UnixListener::bind(&stale).unwrap());
    assert!(stale.exists());
    sb.ptyrc(&["-s", "stale", "text"])
        .fails(3, "session stale is dead", "ptyrc run --name stale");
    assert!(!stale.exists());
    sb.ptyrc(&["-s", "stale", "text"])
        .fails(3, "no such session: stale", "running: alpha, beta");
}

#[test]
fn bad_frame_key_colour_anchor_json() {
    let mut sb = Sandbox::new("params");
    sb.session("one");

    sb.ptyrc(&["text", "-f", "activ"]).fails(
        4,
        "no such frame: activ",
        "did you mean active? frames: 0",
    );
    sb.ptyrc(&["text", "-f", "99"]).fails(
        4,
        "no such frame: 99",
        "names: raw, active, largest, quietest",
    );
    sb.ptyrc(&["find", "-f", "99", "x"])
        .fails(4, "no such frame: 99", "frames: 0");

    sb.ptyrc(&["keys", "Enter"])
        .fails(2, "not a key: Enter", "did you mean enter?");
    sb.ptyrc(&["keys", "return"])
        .fails(2, "not a key: return", "did you mean enter?");
    sb.ptyrc(&["keys", "ctrl+shift+x"])
        .fails(2, "not a key: ctrl+shift+x", "keys: enter tab esc");
    sb.ptyrc(&["keys", "f13"])
        .fails(2, "not a key: f13", "f1..f12");
    // Nothing of a refused list is typed; a valid one is.
    sb.ptyrc(&["keys", "a", "pgdn"])
        .fails(2, "not a key: pgdn", "did you mean pagedown?");
    assert_eq!(sb.ptyrc(&["keys", "ok", "enter"]).status, 0);
    assert_eq!(sb.ptyrc(&["wait", "ok", "--timeout", "3000"]).status, 0);
    assert!(!sb.ptyrc(&["text"]).stdout.contains('a'));

    sb.ptyrc(&["draw", "text", "--fg", "gren", "hi"]).fails(
        2,
        "bad colour: gren",
        "did you mean green? a colour is a name",
    );
    sb.ptyrc(&["draw", "text", "--bg", "#12", "hi"])
        .fails(2, "bad colour: #12", "#rrggbb");
    sb.ptyrc(&["draw", "text", "--at", "5", "hi"])
        .fails(2, "not a position: 5", "X,Y as in 10,2");
    sb.ptyrc(&["draw", "text", "--near", "x", "--side", "diagonal", "hi"])
        .fails(2, "bad anchor", "\"side\":\"right\"|\"left\"");
    sb.ptyrc(&["draw", "text", "--auto", "nowhere", "hi"])
        .fails(2, "bad anchor", "\"prefer\":\"top-left\"");
    sb.ptyrc(&["draw", "rm", "42"])
        .fails(4, "no such sprite: 42", "no sprite is up");
    sb.ptyrc(&["draw", "ext", "shape", "circle"]).fails(
        2,
        "no draw extension shape",
        "did you mean shapes? extensions:",
    );
    sb.ptyrc(&["draw", "ext", "shapes", "circl"]).fails(
        2,
        "no function circl",
        "did you mean circle?",
    );
    sb.ptyrc(&["draw", "ext", "shapes", "circle", "{r: 4}"])
        .fails(2, "params is not JSON", "quote it for the shell");
    sb.ptyrc(&["draw", "effect", "confeti"]).fails(
        2,
        "unknown effect: confeti",
        "did you mean confetti? effects:",
    );
    sb.ptyrc(&["debug", "overlay", "heap"]).fails(
        2,
        "no such overlay: heap",
        "overlays: frames, heat, off",
    );

    sb.ptyrc(&["call", "screen.txt"]).fails(
        2,
        "unknown method: screen.txt",
        "did you mean screen.text?",
    );
    sb.ptyrc(&["call", "screen.text", "{bad"])
        .fails(2, "params is not JSON", "got: {bad");
    sb.ptyrc(&["call", "screen.text", "[1]"])
        .fails(2, "bad-params", "params is a JSON object");
    sb.ptyrc(&["events", "screens"]).fails(
        2,
        "no such topic: screens",
        "did you mean screen? topics: screen, cursor",
    );
    sb.ptyrc(&["find", "-E", "(["])
        .fails(2, "regex parse error", "without `regex`");
}

#[test]
fn widget_errors() {
    let mut sb = Sandbox::new("widgets");
    sb.session("one");
    let call = |method: &str, params: &str| sb.ptyrc(&["call", method, params]);
    call("widget.show", r#"{"tag": "clock"}"#).fails(
        2,
        "no such tag: clock",
        "this connection declared no tag: `widget.declare` first",
    );
    call("widget.hide", r#"{"id": 3}"#).fails(
        4,
        "no such widget: 3",
        "this connection shows no widget",
    );
    call("widget.declare", r#"{"border": "paintng"}"#).fails(
        2,
        "no such border: paintng",
        "did you mean painting? borders: plain, round, heavy, double, ascii, none, painting",
    );
    call("widget.declare", r#"{"border": {"top": "日"}}"#).fails(
        2,
        "border top",
        "glyphs of one cell",
    );
    call("widget.declare", r#"{"border": 3}"#).fails(2, "bad border", "a preset name (plain");
    call(
        "widget.declare",
        r#"{"widgets": [{"tag": "a", "size": [0, 1]}]}"#,
    )
    .fails(
        2,
        "from 1x1 to 200x60",
        "16 variants a tag, 64 tags a connection",
    );
    // In one connection: a batch.
    let out = call(
        "batch",
        r#"{"calls": [
            {"method": "widget.declare", "params": {"widgets": [{"tag": "t", "size": [4, 1]}]}},
            {"method": "widget.show", "params": {"tag": "t", "border": "painting"}},
            {"method": "widget.set", "params": {"id": 1, "variant": "4x2", "title": "x"}}
        ]}"#,
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert!(out.stdout.contains(r#""tags""#), "{}", out.stdout);
    assert!(
        out.stdout.contains(r#""border": "painting""#),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("did you mean 4x1? variants: 4x1"),
        "{}",
        out.stdout
    );
    let out = sb.ptyrc(&["widgets"]);
    assert_eq!(out.status, 0);
    assert!(out.stdout.starts_with("  ID  OWNER  TAG"), "{}", out.stdout);
    assert_eq!(
        out.stdout.lines().count(),
        1,
        "the batch's widget died with it"
    );
    let out = sb.ptyrc(&["widgets", "--json"]);
    assert_eq!(out.stdout.trim(), r#"{"widgets":[]}"#);
}

#[test]
fn wait_timeout_and_usage() {
    let mut sb = Sandbox::new("wait");
    sb.session("one");
    let started = Instant::now();
    sb.ptyrc(&["wait", "never shown", "--timeout", "150"])
        .fails(
            1,
            "wait: timed out after 150 ms: \"never shown\" is not shown",
            "ptyrc text",
        );
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(sb.ptyrc(&["send", "here"]).status, 0);
    assert_eq!(sb.ptyrc(&["wait", "here"]).status, 0);
    sb.ptyrc(&["wait", "here", "--gone", "--timeout", "100"])
        .fails(1, "\"here\" is still shown", "--timeout MS");
    sb.ptyrc(&["wait"])
        .fails(2, "wait: nothing to wait for", "--stable MS");
    // `find` without a match: status 1, as grep, and silent.
    let out = sb.ptyrc(&["find", "absent"]);
    assert_eq!((out.status, out.stderr.as_str()), (1, ""));
}

#[test]
fn session_that_ends_under_a_client() {
    let mut sb = Sandbox::new("gone");
    sb.session("one");
    let mut events = sb.command().args(["events", "exit"]).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let mut driver = sb.drivers.pop().unwrap();
    driver.kill().unwrap();
    driver.wait().unwrap();
    // The listener ends quietly; a new verb says the session is dead.
    assert!(events.wait().unwrap().success());
    sb.ptyrc(&["-s", "one", "text"])
        .fails(3, "session one is dead", "ptyrc run --name one");
}

#[test]
fn manage_errors() {
    let mut sb = Sandbox::new("manage");
    sb.session("one");
    let manage = |args: &[&str]| sb.ptyrc(&[&["manage"], args].concat());
    manage(&["stop", "7"]).fails(
        4,
        "no such app: 7",
        "no app; start one: ptyrc manage start APP",
    );
    manage(&["pause", "bricks"]).fails(4, "no such app: bricks", "no app; start one");
    manage(&["log", "bricks"]).fails(4, "no such app: bricks", "no app");
    manage(&["start", "--cmd", "/no/such/program"]).fails(
        2,
        "cannot run /no/such/program",
        "a program of the driver's PATH",
    );
    manage(&["start", "--cmd", "sh", "-c", "echo broken >&2; exit 4"]).fails(
        1,
        "app 1 (sh) ended at once, status 4",
        "broken",
    );
    let started = manage(&["start", "--cmd", "sleep", "60"]);
    assert_eq!(
        (started.status, started.stdout.as_str()),
        (0, "2 sleep: running\n")
    );
    manage(&["start", "--cmd", "sleep", "61"]);
    manage(&["move", "2", "nowhere"]).fails(4, "no such frame: nowhere", "names: raw, active");
    manage(&["restart", "sleep", "5"]).fails(2, "sleep: 2 apps of that name", "ids: 2, 3");
    manage(&["forget", "2"]).fails(4, "app 2 has not exited", "stop it first");
    manage(&["move", "--restart", "sleep", "raw"]).fails(2, "2 apps of that name", "name it by id");
    let call = |method: &str, params: &str| sb.ptyrc(&["call", method, params]);
    call("app.move", r#"{"app": 2, "frame": "raw"}"#).fails(
        4,
        "app 2 does not move by itself",
        "restart it there: ptyrc manage move --restart 2 FRAME",
    );
    call("app.stop", r#"{"app": 2, "now": true}"#).fails(2, "no member `now`", "members: app");
    call("app.start", r#"{"argv": []}"#).fails(2, "argv: [program, arg...]", "as in");
    call("app.kill", "{}").fails(2, "app.kill", "app methods: app.list");
    let stopped = manage(&["stop", "--all"]);
    assert_eq!(stopped.status, 0, "{}", stopped.stderr);
    assert_eq!(stopped.stdout.lines().count(), 2);
}
