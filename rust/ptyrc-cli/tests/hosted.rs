// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev push`, `dev build` and `ptyrc store` against a registry
//! server stubbed on 127.0.0.1 (`registry/API.md`), archives kept in
//! memory. Needs `curl`; skipped without. Never the network: git origins
//! from `PTYRC_TEST_GIT_ROOT`, the default registry
//! `PTYRC_TEST_DEFAULT_REGISTRY` (debug builds).
#![cfg(debug_assertions)]

use ptyrc_store::{Dirs, TarTree, check, sha256_hex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const GPL: &str = "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";
const CONFETTI: &str = "../../sdk/rust/examples/confetti";
/// An empty module: no import.
const WASM: &[u8] = b"\0asm\x01\0\0\0";
/// A module importing `env.f`.
const BAD_WASM: &[u8] = b"\0asm\x01\0\0\0\x02\x09\x01\x03env\x01f\x00\x00";

// The stub server.

#[derive(Clone)]
struct Ver {
    rev: String,
    tier: String,
    yanked: bool,
    manifest: Value,
    size: Option<usize>,
    wasm: Option<String>,
}

#[derive(Clone)]
struct App {
    name: String,
    kind: &'static str,
    description: Option<String>,
    url: Option<String>,
    manifest_path: String,
    token: String,
    versions: Vec<Ver>,
}

#[derive(Default)]
struct State {
    apps: Vec<App>,
    blobs: HashMap<String, Vec<u8>>,
    /// Every request: method, path, Authorization.
    seen: Vec<(String, String, Option<String>)>,
    /// The body of the last git push.
    git_body: Option<Value>,
}

struct Stub {
    base: String,
    state: Arc<Mutex<State>>,
}

fn err(code: u16, msg: &str) -> (u16, Vec<u8>) {
    (
        code,
        serde_json::to_vec(&json!({"error": msg, "hint": null})).unwrap(),
    )
}

fn manifest_json(text: &str) -> Value {
    let m = ptyrc_store::manifest::parse_bytes(text.as_bytes()).unwrap();
    let entry = match m.app.entry {
        ptyrc_store::Entry::Rust(_) => "rust",
        ptyrc_store::Entry::Script(_) => "script",
    };
    json!({"id": m.app.id, "version": m.app.version, "summary": m.app.summary,
           "license": m.app.license, "surface": m.app.surface.as_str(), "entry": entry,
           "permissions": []})
}

fn query(path: &str, key: &str) -> Option<String> {
    let (_, q) = path.split_once('?')?;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

impl State {
    fn index(&self) -> Vec<u8> {
        let apps: Vec<Value> = self
            .apps
            .iter()
            .map(|a| {
                let versions: Vec<Value> = a
                    .versions
                    .iter()
                    .map(|v| {
                        let mut o = v.manifest.clone();
                        o["rev"] = json!(v.rev);
                        o["tier"] = json!(v.tier);
                        o["by"] = Value::Null;
                        o["date"] = json!("2026-11-25");
                        o["yanked"] = json!(v.yanked);
                        o["size"] = json!(v.size);
                        o["wasm"] = json!(v.wasm);
                        o
                    })
                    .collect();
                json!({"name": a.name, "kind": a.kind, "yanked": false,
                       "description": a.description, "url": a.url,
                       "manifest_path": a.manifest_path, "versions": versions})
            })
            .collect();
        serde_json::to_vec(&json!({"format": 1, "apps": apps})).unwrap()
    }

    /// A push of `name`: the owner's token, or a new name.
    fn push(&mut self, app: App, ver: Ver, auth: Option<&str>) -> (u16, Vec<u8>) {
        let name = app.name.clone();
        let kind = app.kind;
        let (created_name, a) = match self.apps.iter().position(|a| a.name == name) {
            Some(i) => {
                if auth != Some(&format!("Bearer {}", self.apps[i].token)) {
                    return err(401, "the name is taken");
                }
                (false, &mut self.apps[i])
            }
            None => {
                self.apps.push(app);
                self.apps.sort_by(|a, b| a.name.cmp(&b.name));
                let i = self.apps.iter().position(|a| a.name == name).unwrap();
                (true, &mut self.apps[i])
            }
        };
        let rev = ver.rev.clone();
        let fresh = !a.versions.iter().any(|v| v.rev == rev);
        if fresh {
            a.versions.push(ver);
        }
        let mut answer = json!({"name": name, "kind": kind, "rev": rev, "tier": "untested"});
        if created_name {
            answer["token"] = json!(a.token);
        }
        (
            if fresh { 201 } else { 200 },
            serde_json::to_vec(&answer).unwrap(),
        )
    }

    fn handle(
        &mut self,
        method: &str,
        path: &str,
        auth: Option<&str>,
        body: &[u8],
    ) -> (u16, Vec<u8>) {
        let bare = path.split('?').next().unwrap();
        match (method, bare) {
            ("GET", "/v1/index.json") => (200, self.index()),
            ("GET", p) if p.starts_with("/v1/blob/") => {
                match self.blobs.get(&p["/v1/blob/".len()..]) {
                    Some(b) => (200, b.clone()),
                    None => err(404, "no such file"),
                }
            }
            ("POST", "/v1/push/tar") => {
                let Ok(tree) = TarTree::parse(body) else {
                    return err(400, "not an archive");
                };
                let Ok(checked) = check(&tree) else {
                    return err(400, "a tree refused");
                };
                let text = String::from_utf8(
                    ptyrc_store::Tree::read(
                        &tree,
                        checked
                            .files
                            .iter()
                            .find(|n| n.path == "ptyrc-app.toml")
                            .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let rev = sha256_hex(body);
                let name = checked.manifest.app.id.clone();
                let app = App {
                    name: name.clone(),
                    kind: "tar",
                    description: query(path, "description"),
                    url: None,
                    manifest_path: "ptyrc-app.toml".into(),
                    token: format!("tok-{name}"),
                    versions: Vec::new(),
                };
                let ver = Ver {
                    rev: rev.clone(),
                    tier: "untested".into(),
                    yanked: false,
                    manifest: manifest_json(&text),
                    size: Some(body.len()),
                    wasm: None,
                };
                let out = self.push(app, ver, auth);
                if out.0 < 300 {
                    self.blobs.insert(format!("{rev}.tar.gz"), body.to_vec());
                }
                out
            }
            ("POST", "/v1/push/git") => {
                let b: Value = serde_json::from_slice(body).unwrap();
                self.git_body = Some(b.clone());
                let url = b["origin"].as_str().unwrap();
                let spec = match b["path"].as_str() {
                    Some(p) => format!("{url}#{p}"),
                    None => url.to_string(),
                };
                let name = ptyrc_store::Origin::parse(&spec).unwrap().id();
                let app = App {
                    name: name.clone(),
                    kind: "git",
                    description: b["description"].as_str().map(String::from),
                    url: Some(url.to_string()),
                    manifest_path: b["manifest_path"].as_str().unwrap().to_string(),
                    token: "tok-git".into(),
                    versions: Vec::new(),
                };
                let ver = Ver {
                    rev: b["commit"].as_str().unwrap().to_string(),
                    tier: "untested".into(),
                    yanked: false,
                    manifest: manifest_json(b["manifest"].as_str().unwrap()),
                    size: None,
                    wasm: None,
                };
                self.push(app, ver, auth)
            }
            _ => err(404, "no such route"),
        }
    }

    fn version(&mut self, name: &str, rev: &str) -> &mut Ver {
        let a = self.apps.iter_mut().find(|a| a.name == name).unwrap();
        a.versions.iter_mut().find(|v| v.rev == rev).unwrap()
    }
}

impl Stub {
    fn new() -> Stub {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        std::thread::spawn(move || {
            for conn in l.incoming() {
                let Ok(mut c) = conn else { continue };
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut line = String::new();
                if r.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split(' ');
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut headers: Vec<(String, String)> = Vec::new();
                loop {
                    let mut l = String::new();
                    if r.read_line(&mut l).unwrap_or(0) == 0 {
                        break;
                    }
                    let l = l.trim_end();
                    if l.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = l.split_once(':') {
                        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
                    }
                }
                let h = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                if h("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) {
                    let _ = c.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
                }
                let len: usize = h("content-length").map_or(0, |v| v.parse().unwrap());
                let mut body = vec![0; len];
                if r.read_exact(&mut body).is_err() {
                    continue;
                }
                let auth = h("authorization");
                let (code, out) = {
                    let mut s = st.lock().unwrap();
                    s.seen.push((method.clone(), path.clone(), auth.clone()));
                    s.handle(&method, &path, auth.as_deref(), &body)
                };
                let head = format!(
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    out.len()
                );
                let _ = c.write_all(head.as_bytes());
                let _ = c.write_all(&out);
            }
        });
        Stub { base, state }
    }

    fn host(&self) -> &str {
        self.base.trim_start_matches("http://")
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.state.lock().unwrap())
    }

    fn pushes(&self) -> Vec<Option<String>> {
        self.with(|s| {
            s.seen
                .iter()
                .filter(|(m, ..)| m == "POST")
                .map(|(.., a)| a.clone())
                .collect()
        })
    }
}

fn curl() -> bool {
    let ok = Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("skipped: no curl");
    }
    ok
}

fn has_rustc() -> bool {
    let ok = ptyrc_store::build::Toolchain::find();
    if let Err(e) = &ok {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {e}"
        );
        eprintln!("skipped: {e}");
    }
    ok.is_ok()
}

// The client.

struct Out {
    status: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    #[track_caller]
    fn ok(&self) -> &Out {
        assert_eq!(
            self.status, 0,
            "stdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
        self
    }

    /// Fails with `status`; stderr holds `message` on a `ptyrc: ` line,
    /// `hint` after it.
    #[track_caller]
    fn fails(&self, status: i32, message: &str, hint: &str) {
        assert_eq!(self.status, status, "stderr: {}", self.stderr);
        let lines: Vec<&str> = self.stderr.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("ptyrc: ") && l.contains(message));
        let at = at.unwrap_or_else(|| panic!("{message:?} not in {:?}", self.stderr));
        assert!(
            lines[at + 1..].iter().any(|l| l.contains(hint)),
            "hint {hint:?} not in {:?}",
            self.stderr
        );
    }
}

/// A test root: XDG directories, app trees, repositories.
struct Root {
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Root {
    fn new(tag: &str) -> Root {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("hosted-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Root {
            dir,
            env: Vec::new(),
        }
    }

    fn dirs(&self) -> Dirs {
        Dirs::under(&self.dir.join("xdg"))
    }

    fn set(&mut self, k: &str, v: &str) {
        self.env.push((k.into(), v.into()));
    }

    fn ptyrc(&self, args: &[&str]) -> Out {
        let d = self.dirs();
        let out = Command::new(BIN)
            .args(args)
            .env("HOME", self.dir.join("home"))
            .env("XDG_DATA_HOME", &d.data)
            .env("XDG_STATE_HOME", &d.state)
            .env("XDG_CACHE_HOME", &d.cache)
            .env("XDG_CONFIG_HOME", &d.config)
            .env("XDG_RUNTIME_DIR", self.dir.join("run"))
            .env("PTYRC_STORE_APPS", self.dir.join("shipped"))
            .env("PTYRC_TEST_GIT_ROOT", self.dir.join("hosts"))
            .env("PTYRC_NO_DEFAULT_REGISTRY", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("PTYRC_TEST_DEFAULT_REGISTRY")
            .env_remove("PTYRC_SESSION")
            .env_remove("PTYRC_SOCKET")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        Out {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }

    fn add_registry(&self, stub: &Stub) {
        self.ptyrc(&["store", "registry", "add", "test", &stub.base, "--api"])
            .ok();
    }

    fn lock(&self) -> String {
        std::fs::read_to_string(self.dirs().lock()).unwrap_or_default()
    }
}

fn manifest(id: &str, version: &str) -> String {
    format!(
        "[app]\nid = \"{id}\"\nversion = \"{version}\"\nsummary = \"Sand that piles on your text.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n"
    )
}

fn write_files(dir: &Path, files: &[(&str, String)]) {
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

/// A script app at `dir`.
fn script_app(dir: &Path, id: &str, version: &str) {
    write_files(
        dir,
        &[
            ("ptyrc-app.toml", manifest(id, version)),
            ("LICENSE", GPL.into()),
            (
                "app.js",
                format!("// {version}\nexport function init(args) {{}}\n"),
            ),
        ],
    );
}

/// The Rust example `confetti` at `dir`.
fn rust_app(dir: &Path) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(CONFETTI);
    let read = |n: &str| std::fs::read_to_string(src.join(n)).unwrap();
    write_files(
        dir,
        &[
            ("ptyrc-app.toml", read("ptyrc-app.toml")),
            ("Cargo.toml", read("Cargo.toml")),
            ("src/lib.rs", read("src/lib.rs")),
            ("LICENSE", GPL.into()),
        ],
    );
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The 64-hex revision `dev push` printed.
fn rev_of(out: &Out) -> String {
    let at = out.stdout.find("revision ").expect("a revision") + "revision ".len();
    out.stdout[at..at + 64].to_string()
}

#[test]
fn push_search_add_update() {
    if !curl() {
        return;
    }
    let stub = Stub::new();
    let r = Root::new("trip");
    let app = r.dir.join("sand");
    script_app(&app, "sand", "0.1.0");
    let dir = app.to_str().unwrap();
    let id = format!("{}/sand", stub.host());

    // A dry run: checked and packed, nothing sent, no registry needed.
    let dry = r.ptyrc(&["dev", "push", dir, "--dry-run"]);
    dry.ok();
    assert!(
        dry.stdout.contains("sand 0.1.0: revision "),
        "{}",
        dry.stdout
    );
    assert!(dry.stdout.contains("nothing sent"), "{}", dry.stdout);
    let rev1 = rev_of(&dry);
    assert!(stub.with(|s| s.seen.is_empty()));
    // No registry server: said.
    r.ptyrc(&["dev", "push", dir])
        .fails(2, "no registry server", "--api");

    // A TODO of the template: refused before anything is sent.
    let todo = r.dir.join("todo");
    script_app(&todo, "todo", "0.1.0");
    let m = manifest("todo", "0.1.0").replace("Sand that", "TODO: sand that");
    std::fs::write(todo.join("ptyrc-app.toml"), m).unwrap();
    let out = r.ptyrc(&["dev", "push", todo.to_str().unwrap(), "--dry-run"]);
    assert_eq!(out.status, 1, "{}", out.stderr);
    assert!(out.stderr.contains("TODO"), "{}", out.stderr);

    // A description past 64 characters: refused before anything is sent.
    let long = "x".repeat(65);
    r.ptyrc(&["dev", "push", dir, "--dry-run", "--description", &long])
        .fails(2, "--description: 65 characters", "64 at most");
    r.ptyrc(&[
        "dev",
        "push",
        dir,
        "--dry-run",
        "--description",
        &long[..64],
    ])
    .ok();
    let wordy = r.dir.join("wordy");
    script_app(&wordy, "wordy", "0.1.0");
    let m = manifest("wordy", "0.1.0").replace("Sand that", &format!("{long} Sand that"));
    std::fs::write(wordy.join("ptyrc-app.toml"), m).unwrap();
    r.ptyrc(&["dev", "push", wordy.to_str().unwrap(), "--dry-run"])
        .fails(
            2,
            "the summary, the default description",
            "--description TEXT",
        );

    r.add_registry(&stub);
    let ls = r.ptyrc(&["store", "registry", "ls"]);
    ls.ok();
    assert!(
        ls.stdout
            .contains(&format!("test  api {}  fetched ", stub.base)),
        "{}",
        ls.stdout
    );

    // The first push: the token kept, 0600, not printed.
    let out = r.ptyrc(&["dev", "push", dir, "--description", "Grains of quartz."]);
    out.ok();
    assert!(
        out.stdout
            .contains(&format!("pushed {id} 0.1.0 at {}, untested", &rev1[..12])),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains(&format!("ptyrc store add {id}")),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("tok-sand"), "{}", out.stdout);
    assert!(out.stderr.contains("tokens.toml"), "{}", out.stderr);
    let tokens = r.dirs().tokens();
    let mode = std::fs::metadata(&tokens).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert!(
        std::fs::read_to_string(&tokens)
            .unwrap()
            .contains("tok-sand")
    );
    assert_eq!(stub.pushes(), [None]);

    // Search: by id, summary, description; the line names the origin.
    for word in ["sand", "PILES", "quartz"] {
        let s = r.ptyrc(&["store", "search", word]);
        s.ok();
        assert!(s.stdout.starts_with(&id), "{word}: {}", s.stdout);
        assert_eq!(s.stdout.lines().count(), 1, "{}", s.stdout);
    }
    let j: Value =
        serde_json::from_str(&r.ptyrc(&["--json", "store", "search", "quartz"]).stdout).unwrap();
    assert_eq!(j[0]["origin"], id.as_str());
    assert_eq!(j[0]["kind"], "tar");
    assert_eq!(j[0]["description"], "Grains of quartz.");

    // Show, not installed: from the index.
    let show = r.ptyrc(&["store", "show", &id]);
    show.ok();
    for want in [
        "kind         tar",
        "description  Grains of quartz.",
        "listed in    test",
        &rev1,
    ] {
        assert!(show.stdout.contains(want), "{want}: {}", show.stdout);
    }

    // Installed from a second machine, by its bare name: untested asked.
    let mut other = Root::new("other");
    other.add_registry(&stub);
    other
        .ptyrc(&["store", "add", "sand"])
        .fails(2, "untested; not installed", "--yes");
    other.ptyrc(&["store", "add", "nosuch"]).fails(
        4,
        "listed by no registry server",
        "ptyrc store search WORD",
    );
    let add = other.ptyrc(&["store", "add", "sand", "--yes"]);
    add.ok();
    assert!(
        add.stdout.contains(&format!("added {id}: sand 0.1.0")),
        "{}",
        add.stdout
    );
    let ls = other.ptyrc(&["store", "ls"]);
    ls.ok();
    assert!(
        ls.stdout.contains(&id) && ls.stdout.contains(&rev1[..12]),
        "{}",
        ls.stdout
    );

    // A second push: the token sent again.
    script_app(&app, "sand", "0.2.0");
    let out = r.ptyrc(&["dev", "push", dir]);
    out.ok();
    assert_eq!(stub.pushes()[1].as_deref(), Some("Bearer tok-sand"));
    let rev2 = rev_of(&r.ptyrc(&["dev", "push", dir, "--dry-run"]));
    // The same again: listed already.
    let again = r.ptyrc(&["dev", "push", dir]);
    again.ok();
    assert!(again.stdout.contains("listed already"), "{}", again.stdout);

    // Another's name, or a wrong token: 401, said plainly.
    r.ptyrc(&["dev", "push", dir, "--token", "wrong"])
        .fails(2, "someone else's", "--token");

    // Update: the newer revision, raised to tested.
    stub.with(|s| s.version("sand", &rev2).tier = "tested".into());
    other.ptyrc(&["store", "sync"]).ok();
    let up = other.ptyrc(&["store", "update"]);
    up.ok();
    assert!(
        up.stdout.contains(&format!(
            "updated {id}: sand 0.2.0 at {}, tested",
            &rev2[..12]
        )),
        "{}",
        up.stdout
    );
    assert!(other.lock().contains(&rev2));

    // A yanked revision: refused.
    stub.with(|s| s.version("sand", &rev1).yanked = true);
    let third = Root::new("third");
    third.add_registry(&stub);
    third
        .ptyrc(&["store", "add", &format!("{id}@{rev1}"), "--yes"])
        .fails(2, "yanked", "--force");
    assert!(third.lock().is_empty());

    // An archive whose bytes are not its revision: refused, nothing
    // installed.
    let real = stub.with(|s| {
        let other = s.blobs[&format!("{rev1}.tar.gz")].clone();
        s.blobs.insert(format!("{rev2}.tar.gz"), other).unwrap()
    });
    third
        .ptyrc(&["store", "add", &id, "--yes"])
        .fails(5, "has the sha256", "another archive");
    assert!(third.lock().is_empty());
    // By HOST/NAME, the archive right: the tested revision, not asked.
    stub.with(|s| s.blobs.insert(format!("{rev2}.tar.gz"), real));
    let add = third.ptyrc(&["store", "add", &id]);
    add.ok();
    assert!(
        add.stdout.contains(&format!(
            "added {id}: sand 0.2.0 at {}, tested",
            &rev2[..12]
        )),
        "{}",
        add.stdout
    );
    let _ = &mut other;
}

#[test]
fn hosted_registry_not_fetched() {
    if !curl() {
        return;
    }
    let stub = Stub::new();
    let r = Root::new("absent");
    let app = r.dir.join("sand");
    script_app(&app, "sand", "0.1.0");
    let id = format!("{}/sand", stub.host());
    // Before the stub: a registry server of another host, and a git
    // registry, neither fetched.
    let closed = closed_port();
    r.ptyrc(&["store", "registry", "add", "dead", &closed, "--api"])
        .ok();
    let store = r.dirs().store();
    let mut toml = std::fs::read_to_string(&store).unwrap();
    toml.push_str("[[registry]]\nname = \"gone\"\nurl = \"/nonexistent/registry.git\"\n");
    std::fs::write(&store, &toml).unwrap();
    r.add_registry(&stub);
    r.ptyrc(&["dev", "push", app.to_str().unwrap(), "--registry", "test"])
        .ok();
    // The git registry may list it: in the way, whatever is asked.
    r.ptyrc(&["store", "add", &id, "--yes", "--latest"]).fails(
        5,
        "registry gone: not fetched",
        "ptyrc store sync",
    );
    // The other host's server cannot: passed over.
    r.ptyrc(&["store", "registry", "rm", "gone"]).ok();
    r.ptyrc(&["store", "add", &id, "--yes"]).ok();
}

/// A loopback base that refuses connections.
fn closed_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    drop(l);
    base
}

#[test]
fn default_registry_unreached() {
    if !curl() {
        return;
    }
    let mut r = Root::new("default");
    // A git origin, served from PTYRC_TEST_GIT_ROOT.
    let bare = r.dir.join("hosts/example.org/a/sand");
    std::fs::create_dir_all(&bare).unwrap();
    git(
        &bare,
        &["init", "--bare", "--quiet", "--initial-branch=main"],
    );
    let work = r.dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "--initial-branch=main"]);
    script_app(&work, "sand", "0.1.0");
    git(&work, &["add", "--all"]);
    git(&work, &["commit", "--quiet", "-m", "c"]);
    git(
        &work,
        &["push", "--quiet", bare.to_str().unwrap(), "HEAD:main"],
    );

    let closed = closed_port();
    r.set("PTYRC_TEST_DEFAULT_REGISTRY", &closed);
    // No store.toml: the default, never fetched, passed over.
    let out = r.ptyrc(&["store", "add", "example.org/a/sand", "--yes"]);
    out.ok();
    assert!(out.stderr.contains("(the default)"), "{}", out.stderr);
    assert!(r.lock().contains("example.org/a/sand"));
    // Written to store.toml (by `registry add`): the user's, in the way.
    r.ptyrc(&["store", "rm", "example.org/a/sand"]).ok();
    r.ptyrc(&["store", "registry", "add", "mine", &closed, "--api"])
        .ok();
    let toml = std::fs::read_to_string(r.dirs().store()).unwrap();
    assert!(toml.contains("name = \"ptyrc\""), "{toml}");
    r.ptyrc(&["store", "add", "example.org/a/sand", "--yes"])
        .fails(5, "registry mine: not fetched", "--latest");
}

#[test]
fn prebuilt_module() {
    if !curl() {
        return;
    }
    let stub = Stub::new();
    let r = Root::new("prebuilt");
    let app = r.dir.join("confetti");
    rust_app(&app);
    r.add_registry(&stub);
    let out = r.ptyrc(&["dev", "push", app.to_str().unwrap()]);
    out.ok();
    let rev = rev_of(&r.ptyrc(&["dev", "push", app.to_str().unwrap(), "--dry-run"]));
    let id = format!("{}/confetti", stub.host());

    // No module listed: --prebuilt refused.
    r.ptyrc(&["store", "add", &id, "--yes", "--prebuilt"])
        .fails(4, "lists no module", "without --prebuilt");
    // No rustc, no module: the toolchain's error, the hint extended.
    let mut norust = Root::new("norust");
    norust.set("PTYRC_RUSTC", "/nonexistent/rustc");
    norust.add_registry(&stub);
    let out = norust.ptyrc(&["store", "add", &id, "--yes"]);
    assert_eq!(out.status, 2, "{}", out.stderr);
    assert!(
        out.stderr.contains("lists no module of this version"),
        "{}",
        out.stderr
    );

    stub.with(|s| {
        s.blobs.insert(format!("{rev}.wasm"), WASM.to_vec());
        s.version("confetti", &rev).wasm = Some(sha256_hex(WASM));
    });
    r.ptyrc(&["store", "sync"]).ok();
    norust.ptyrc(&["store", "sync"]).ok();
    let add = r.ptyrc(&["store", "add", &id, "--yes", "--prebuilt"]);
    add.ok();
    assert!(
        add.stderr.contains("the registry's module of"),
        "{}",
        add.stderr
    );
    assert!(r.lock().contains(&rev));
    // No rustc: the module taken by itself.
    let add = norust.ptyrc(&["store", "add", &id, "--yes"]);
    add.ok();
    assert!(add.stderr.contains("(no rustc here)"), "{}", add.stderr);

    // A module that imports `env`: refused, nothing installed.
    stub.with(|s| {
        s.blobs.insert(format!("{rev}.wasm"), BAD_WASM.to_vec());
        s.version("confetti", &rev).wasm = Some(sha256_hex(BAD_WASM));
    });
    let bad = Root::new("badwasm");
    bad.add_registry(&stub);
    bad.ptyrc(&["store", "add", &id, "--yes", "--prebuilt"])
        .fails(2, "imports env.f", "imports of `ptyrc` only");
    assert!(bad.lock().is_empty());
}

#[test]
fn dev_build() {
    let r = Root::new("build");
    let script = r.dir.join("sand");
    script_app(&script, "sand", "0.1.0");
    let out_file = r.dir.join("out/app.wasm");
    std::fs::create_dir_all(out_file.parent().unwrap()).unwrap();
    let out = out_file.to_str().unwrap();
    r.ptyrc(&["dev", "build", script.to_str().unwrap(), "--out", out])
        .fails(2, "nothing to build", "rust");
    assert!(!out_file.exists());
    if !has_rustc() {
        return;
    }
    let app = r.dir.join("confetti");
    rust_app(&app);
    // The tree read-only, as in the builder's container.
    let ro = |mode: u32| {
        for p in [&app, &app.join("src")] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    ro(0o555);
    let mut rr = r;
    rr.set("TMPDIR", rr.dir.join("tmp").to_str().unwrap());
    std::fs::create_dir_all(rr.dir.join("tmp")).unwrap();
    let built = rr.ptyrc(&[
        "dev",
        "build",
        app.to_str().unwrap(),
        "--out",
        out,
        "--force",
    ]);
    ro(0o755);
    built.ok();
    let wasm = std::fs::read(&out_file).unwrap();
    assert!(wasm.starts_with(b"\0asm"), "a module");
    assert!(built.stderr.contains("wrote "), "{}", built.stderr);
    // The work directory removed.
    assert_eq!(std::fs::read_dir(rr.dir.join("tmp")).unwrap().count(), 0);
}

#[test]
fn push_git() {
    if !curl() {
        return;
    }
    let stub = Stub::new();
    let r = Root::new("git");
    r.add_registry(&stub);
    let work = r.dir.join("repo");
    let app = work.join("apps/sand");
    std::fs::create_dir_all(&app).unwrap();
    git(&work, &["init", "--quiet", "--initial-branch=main"]);
    script_app(&app, "sand", "0.1.0");
    git(&work, &["add", "--all"]);
    git(&work, &["commit", "--quiet", "-m", "c"]);
    git(
        &work,
        &["remote", "add", "origin", "git@example.org:a/sand.git"],
    );
    let commit = git(&work, &["rev-parse", "HEAD"]);
    let dir = app.to_str().unwrap();

    // HEAD not on the remote.
    r.ptyrc(&["dev", "push", "--git", dir])
        .fails(2, "not on remote origin", "git push origin");
    git(&work, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    // Uncommitted changes.
    std::fs::write(app.join("app.js"), "// changed\n").unwrap();
    r.ptyrc(&["dev", "push", "--git", dir]).fails(
        2,
        "uncommitted changes",
        "commit and push it first",
    );
    git(&work, &["checkout", "--quiet", "--", "."]);

    let out = r.ptyrc(&["dev", "push", "--git", dir]);
    out.ok();
    let origin = "example.org/a/sand#apps/sand";
    assert!(
        out.stdout
            .contains(&format!("pushed {origin} 0.1.0 at {}", &commit[..12])),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains(&format!("ptyrc store add {origin}")),
        "{}",
        out.stdout
    );
    let body = stub.with(|s| s.git_body.clone()).unwrap();
    assert_eq!(body["origin"], "https://example.org/a/sand");
    assert_eq!(body["path"], "apps/sand");
    assert_eq!(body["commit"], commit.as_str());
    assert_eq!(body["manifest_path"], "apps/sand/ptyrc-app.toml");
    assert_eq!(body["manifest"], manifest("sand", "0.1.0").as_str());
    assert_eq!(body["description"], "Sand that piles on your text.");
    let tokens = std::fs::read_to_string(r.dirs().tokens()).unwrap();
    assert!(
        tokens.contains(origin) && tokens.contains("tok-git"),
        "{tokens}"
    );

    // Listed: the registry's word on the git origin.
    let show = r.ptyrc(&["store", "show", origin]);
    show.ok();
    assert!(show.stdout.contains("kind         git"), "{}", show.stdout);
    assert!(show.stdout.contains(&commit), "{}", show.stdout);
}

/// The archive of `rev` the stub keeps, unpacked at `dir`.
fn unpack(stub: &Stub, rev: &str, dir: &Path) -> Vec<String> {
    let bytes = stub.with(|s| s.blobs[&format!("{rev}.tar.gz")].clone());
    let t = TarTree::parse(&bytes).unwrap();
    let mut names = Vec::new();
    for n in ptyrc_store::Tree::nodes(&t).unwrap() {
        if n.kind == ptyrc_store::Kind::File {
            let p = dir.join(&n.path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, ptyrc_store::Tree::read(&t, &n).unwrap()).unwrap();
            names.push(n.path);
        }
    }
    names.sort();
    names
}

/// `dev push`: the files the app does not use left out, said; the
/// archive stored passes `dev vet`. A binary file: refused.
#[test]
fn push_leaves_out_unused() {
    if !curl() {
        return;
    }
    let stub = Stub::new();
    let r = Root::new("unused");
    r.add_registry(&stub);
    let app = r.dir.join("sand");
    script_app(&app, "sand", "0.1.0");
    write_files(
        &app,
        &[
            ("notes.bin", "plain text, named oddly\n".into()),
            ("old.js", "export const x = 1;\n".into()),
            ("README.md", "# sand\n".into()),
        ],
    );
    let dry = r.ptyrc(&["dev", "push", app.to_str().unwrap(), "--dry-run"]);
    assert!(
        dry.ok().stdout.contains("left out: notes.bin, old.js\n"),
        "{}",
        dry.stdout
    );
    let out = r.ptyrc(&["dev", "push", app.to_str().unwrap()]);
    assert!(out.ok().stdout.contains("left out: notes.bin, old.js\n"));
    let rev = stub.with(|s| {
        let a = s.apps.iter().find(|a| a.name == "sand").unwrap();
        a.versions.last().unwrap().rev.clone()
    });
    let got = r.dir.join("got-sand");
    assert_eq!(
        unpack(&stub, &rev, &got),
        ["LICENSE", "README.md", "app.js", "ptyrc-app.toml"]
    );
    r.ptyrc(&["dev", "vet", got.to_str().unwrap()]).ok();
    std::fs::write(app.join("logo.png"), b"\x89PNG\r\n\x1a\n\0\0").unwrap();
    r.ptyrc(&["dev", "push", app.to_str().unwrap(), "--dry-run"])
        .fails(1, "logo.png: not text", "no image, no binary");
    if !has_rustc() {
        return;
    }
    let crab = r.dir.join("confetti");
    rust_app(&crab);
    write_files(
        &crab,
        &[
            ("notes.bin", "plain text\n".into()),
            ("src/old.rs", "pub fn old() {}\n".into()),
        ],
    );
    let out = r.ptyrc(&["dev", "push", crab.to_str().unwrap()]);
    assert!(
        out.ok()
            .stdout
            .contains("left out: notes.bin, src/old.rs\n"),
        "{}",
        out.stdout
    );
    let rev = stub.with(|s| {
        let a = s.apps.iter().find(|a| a.name == "confetti").unwrap();
        a.versions.last().unwrap().rev.clone()
    });
    let got = r.dir.join("got-confetti");
    assert_eq!(
        unpack(&stub, &rev, &got),
        ["Cargo.toml", "LICENSE", "ptyrc-app.toml", "src/lib.rs"]
    );
    let wasm = r.dir.join("confetti.wasm");
    let vet = ["dev", "vet", got.to_str().unwrap(), "--out"];
    r.ptyrc(&[&vet[..], &[wasm.to_str().unwrap(), "--force"]].concat())
        .ok();
    assert!(std::fs::read(&wasm).unwrap().starts_with(b"\0asm"));
}

/// `PTYRC_HTTP` chooses the transport; with neither `curl` nor
/// `python3` on PATH, said.
#[test]
fn http_transports() {
    let stub = Stub::new();
    let mut r = Root::new("transport");
    r.add_registry(&stub);
    let app = r.dir.join("sand");
    script_app(&app, "sand", "0.1.0");
    let path = app.to_str().unwrap();
    let python = Command::new("python3").arg("--version").output();
    if python.is_ok_and(|o| o.status.success()) {
        let mut p = Root {
            dir: r.dir.clone(),
            env: vec![("PTYRC_HTTP".into(), "python".into())],
        };
        p.ptyrc(&["dev", "push", path]).ok();
        p.set("PTYRC_HTTP", "ftp");
        p.ptyrc(&["dev", "push", path])
            .fails(5, "PTYRC_HTTP=\"ftp\"", "PTYRC_HTTP=curl");
    }
    r.set("PATH", r.dir.join("empty").to_str().unwrap());
    r.ptyrc(&["dev", "push", path]).fails(
        5,
        "neither curl nor python3 on PATH",
        "install curl, or python3",
    );
}
