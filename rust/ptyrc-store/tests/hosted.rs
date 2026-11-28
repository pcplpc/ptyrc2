// SPDX-License-Identifier: GPL-3.0-or-later
//! A registry server, stubbed on 127.0.0.1: sync, fetch, push, tokens.
//! Each test by `curl` and by `python3`; a transport not installed is
//! skipped.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use common::{GPL, MANIFEST, tmp};
use ptyrc_store::http::{self, Transport};
use ptyrc_store::index::Index;
use ptyrc_store::registry::{Registries, latest};
use ptyrc_store::sync::{self, MAX_AGE_API, Synced};
use ptyrc_store::{
    DirTree, Dirs, ErrorKind, Origin, Reference, Remote, Tier, Want, check, choose, hosted, push,
    sha256_hex, tar, tokens,
};

#[derive(Debug, Clone)]
struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Req {
    fn header(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
}

type Answer = (u16, Vec<(&'static str, String)>, Vec<u8>);
type Handler = Arc<Mutex<Box<dyn FnMut(&Req) -> Answer + Send>>>;

/// One request per connection, `Connection: close`.
struct Stub {
    base: String,
    seen: Arc<Mutex<Vec<Req>>>,
    handler: Handler,
}

impl Stub {
    fn new() -> Stub {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(Mutex::new(Box::new(|_: &Req| (404, vec![], vec![]))));
        let (s, h) = (seen.clone(), handler.clone());
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
                let mut headers = Vec::new();
                loop {
                    let mut l = String::new();
                    r.read_line(&mut l).unwrap();
                    let l = l.trim_end();
                    if l.is_empty() {
                        break;
                    }
                    let (k, v) = l.split_once(':').unwrap();
                    headers.push((k.to_string(), v.trim().to_string()));
                }
                let len: usize = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.parse().unwrap());
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let req = Req {
                    method,
                    path,
                    headers,
                    body,
                };
                s.lock().unwrap().push(req.clone());
                let (code, extra, body) = (h.lock().unwrap())(&req);
                let mut head = format!(
                    "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (k, v) in extra {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                let _ = c.write_all(head.as_bytes());
                let _ = c.write_all(&body);
            }
        });
        Stub {
            base,
            seen,
            handler,
        }
    }

    fn answer(&self, f: impl FnMut(&Req) -> Answer + Send + 'static) {
        *self.handler.lock().unwrap() = Box::new(f);
    }

    fn last(&self) -> Req {
        self.seen.lock().unwrap().last().unwrap().clone()
    }

    fn host(&self) -> &str {
        self.base.trim_start_matches("http://")
    }
}

/// The transports installed; a note for each that is not.
fn transports() -> Vec<Transport> {
    let mut out = Vec::new();
    for (t, program) in [(Transport::Curl, "curl"), (Transport::Python, "python3")] {
        let ok = std::process::Command::new(program)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        match ok {
            true => out.push(t),
            false => eprintln!("note: {program} absent; hosted tests by it skipped"),
        }
    }
    out
}

/// `f` by each transport installed.
fn each(f: impl Fn(Transport)) {
    for t in transports() {
        eprintln!("by {t:?}");
        http::with_transport(t, || f(t));
    }
}

/// The command lines of this process's children, NUL-separated.
fn children_argv() -> Vec<String> {
    let me = std::process::id().to_string();
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").unwrap().flatten() {
        let stat = std::fs::read_to_string(e.path().join("stat")).unwrap_or_default();
        // `PID (COMM) STATE PPID ...`; COMM may hold spaces.
        let ppid = stat
            .rsplit_once(") ")
            .and_then(|(_, r)| r.split(' ').nth(1));
        if ppid == Some(me.as_str()) {
            let argv = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
            out.push(String::from_utf8_lossy(&argv).into_owned());
        }
    }
    out
}

fn json(v: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&v).unwrap()
}

/// A packed app tree, and its sha256.
fn archive(root: &Path) -> (Vec<u8>, String) {
    let dir = root.join("app");
    for (p, b) in [
        ("ptyrc-app.toml", MANIFEST.as_bytes()),
        ("LICENSE", GPL),
        ("app.js", b"export function init(args) {}\n"),
        ("assets/grain.txt", b"."),
    ] {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    }
    let t = DirTree::new(&dir);
    let bytes = tar::pack(&t, &check(&t).unwrap()).unwrap();
    let sha = sha256_hex(&bytes);
    (bytes, sha)
}

fn version(rev: &str, tier: &str, date: &str) -> serde_json::Value {
    serde_json::json!({
        "rev": rev, "tier": tier, "by": null, "date": date, "yanked": false,
        "id": "sand-garden", "version": "0.3.0", "summary": "Sand.\u{1b}[31m",
        "license": "GPL-3.0-or-later", "surface": "widget", "entry": "script",
        "permissions": [{"name": "shape", "why": "the grains rest", "words": []}],
        "size": 5120, "wasm": null, "future": {"ignored": true}
    })
}

fn index(rev: &str) -> Vec<u8> {
    let old = "a".repeat(64);
    let git = "b".repeat(40);
    json(serde_json::json!({"format": 1, "extra": 1, "apps": [
        {"name": "../x", "kind": "tar", "versions": [version(&old, "untested", "2026-11-20")]},
        {"name": "confetti", "kind": "tar", "yanked": false,
         "description": "Paper falls behind your text.", "manifest_path": "ptyrc-app.toml",
         "versions": [version(&old, "untested", "2026-11-20"), version(rev, "tested", "2026-11-21"),
                      version("not-a-rev", "approved", "2026-11-22"),
                      {"rev": "c".repeat(64), "tier": "shipped"},
                      {"rev": 3}]},
        {"name": "elsewhere", "kind": "tar", "url": "https://evil.example/elsewhere",
         "versions": [version(&old, "untested", "2026-11-20")]},
        {"name": "evil.example/x", "kind": "tar", "versions": []},
        {"name": "example.org/a/sand#apps/sand", "kind": "git",
         "url": "https://example.org/a/sand", "manifest_path": "apps/sand/ptyrc-app.toml",
         "versions": [version(&git, "approved", "2026-11-19")]},
        {"name": "example.org/a/liar", "kind": "git",
         "url": "https://example.org/a/sand", "manifest_path": "ptyrc-app.toml", "versions": []},
        {"name": "example.org/a/up", "kind": "git",
         "url": "https://example.org/a/up/../../x", "versions": []},
        {"name": "zipped", "kind": "zip", "versions": []},
        {"name": 7},
        "junk"
    ]}))
}

/// Every file under `dir`, relative, sorted.
fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                let rel = e.path().strip_prefix(dir).unwrap().display().to_string();
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

fn setup(name: &str, stub: &Stub) -> (std::path::PathBuf, Dirs, Registries) {
    let root = tmp(name);
    let dirs = Dirs::under(&root);
    let mut regs = Registries::default();
    regs.add_api(&dirs, "hub", &stub.base).unwrap();
    regs.save(&dirs).unwrap();
    let regs = Registries::load(&dirs).unwrap();
    (root, dirs, regs)
}

#[test]
fn sync_api_registry() {
    each(sync_api_registry_by);
}

fn sync_api_registry_by(by: Transport) {
    let stub = Stub::new();
    let (root, dirs, regs) = setup(&format!("hosted-sync-{by:?}"), &stub);
    assert_eq!(regs.list[0].api.as_deref(), Some(stub.base.as_str()));
    assert_eq!(regs.list[0].max_age(), MAX_AGE_API);
    assert_eq!(
        regs.list[0].remote(),
        Some(Remote::Registry(stub.base.clone()))
    );
    let (_, rev) = archive(&root);
    let body = index(&rev);
    let b = body.clone();
    stub.answer(move |r| match r.path.as_str() {
        "/v1/index.json" => (200, vec![], b.clone()),
        _ => (
            404,
            vec![],
            json(serde_json::json!({"error": "no such file", "hint": null})),
        ),
    });
    let reg = &regs.list[0];
    let now = SystemTime::now();
    match sync::sync(reg, now, Some(MAX_AGE_API)) {
        Synced::Fetched { commit } => assert_eq!(commit, sha256_hex(&body)),
        s => panic!("{s:?}"),
    }
    assert_eq!(stub.last().method, "GET");
    let host = stub.host().replace(':', "_");
    assert_eq!(
        files(&reg.dir),
        [
            ".fetched".to_string(),
            format!("apps/{host}/confetti.toml"),
            "apps/example.org/a/sand/apps/sand.toml".into(),
            "index.json".into(),
        ]
    );
    // Nothing beside the copy.
    let mut beside: Vec<String> = std::fs::read_dir(dirs.registry())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    beside.sort();
    assert_eq!(beside, ["hub"]);
    // The readers of a git registry, unchanged.
    let o = Origin::parse(&format!("{}/confetti", stub.host())).unwrap();
    assert!(o.is_hosted());
    assert_eq!(o.remote(), Remote::Registry(stub.base.clone()));
    let (_, p) = regs.lookup(&o).unwrap().unwrap();
    assert_eq!(p.origin, format!("{}/confetti", stub.base));
    assert_eq!(p.versions.len(), 2);
    let pick = choose(Some(&p), &Want::default()).unwrap();
    assert_eq!(pick.reference, Reference::Commit(rev.clone()));
    assert_eq!(pick.tier, Tier::Tested);
    assert_eq!(latest(&p).unwrap().commit, rev);
    let ix = Index::load(reg).unwrap();
    assert_eq!(ix.format, 1);
    let a = ix.get(&o.id()).unwrap();
    assert_eq!(a.kind.as_deref(), Some("tar"));
    assert_eq!(
        a.description.as_deref(),
        Some("Paper falls behind your text.")
    );
    assert_eq!(a.commit.as_deref(), Some(rev.as_str()));
    assert_eq!(a.summary.as_deref(), Some("Sand. [31m"));
    assert_eq!(a.permissions[0].name, "shape");
    assert_eq!(a.versions[1].entry.as_deref(), Some("script"));
    assert_eq!(a.versions[1].size, Some(5120));
    let sand = Origin::parse("example.org/a/sand#apps/sand").unwrap();
    let (_, g) = regs.lookup(&sand).unwrap().unwrap();
    assert_eq!(g.manifest, "apps/sand/ptyrc-app.toml");
    assert_eq!(ix.get(&sand.id()).unwrap().tier, Some(Tier::Approved));
    // Young: not fetched again.
    assert!(matches!(
        sync::sync(reg, now, Some(MAX_AGE_API)),
        Synced::Fresh
    ));
    // A format of the future: refused, the copy kept.
    stub.answer(|_| {
        (
            200,
            vec![],
            json(serde_json::json!({"format": 2, "apps": []})),
        )
    });
    match sync::sync(reg, now, None) {
        Synced::Stale { error, .. } => assert!(error.message.contains("format 2"), "{error}"),
        s => panic!("{s:?}"),
    }
    assert!(regs.lookup(&o).unwrap().is_some());
    // resolve_name: the bare NAME of a registry server.
    assert_eq!(
        hosted::resolve_name(&regs, "confetti").unwrap(),
        Some(o.clone())
    );
    assert_eq!(hosted::resolve_name(&regs, "elsewhere").unwrap(), None);
    assert_eq!(hosted::resolve_name(&regs, "../x").unwrap(), None);
    // Hosted: no head, no branch; listed or refused.
    let named = Want {
        reference: Some(Reference::Name("main".into())),
        ..Want::default()
    };
    assert_eq!(
        choose(Some(&p), &named).unwrap_err().kind,
        ErrorKind::Registry
    );
    let head = Want {
        latest: true,
        ..Want::default()
    };
    assert_eq!(
        choose(Some(&p), &head).unwrap().reference,
        Reference::Commit(rev)
    );
    let unknown = Origin::parse(&format!("{}/nope", stub.host())).unwrap();
    assert!(hosted::pick(&unknown, None, &Want::default()).is_err());
}

#[test]
fn fetch_archive_and_module() {
    each(fetch_archive_and_module_by);
}

fn fetch_archive_and_module_by(by: Transport) {
    let stub = Stub::new();
    let root = tmp(&format!("hosted-fetch-{by:?}"));
    let (bytes, rev) = archive(&root);
    let wasm = b"\0asm\x01\0\0\0".to_vec();
    let bad_wasm = b"\0asm\x01\0\0\0\x02\x09\x01\x03env\x01f\x00\x00".to_vec();
    let (b, w, bw) = (bytes.clone(), wasm.clone(), bad_wasm.clone());
    let r = rev.clone();
    stub.answer(move |q| {
        let p = q.path.as_str();
        if p == format!("/v1/blob/{r}.tar.gz") {
            (200, vec![("ETag", format!("\"{r}\""))], b.clone())
        } else if p == format!("/v1/blob/{r}.wasm") {
            (200, vec![], w.clone())
        } else if p == format!("/v1/blob/{}.wasm", "e".repeat(64)) {
            (200, vec![], bw.clone())
        } else if p == format!("/v1/blob/{}.tar.gz", "d".repeat(64)) {
            (200, vec![], b.clone())
        } else if p == format!("/v1/blob/{}.tar.gz", "9".repeat(64)) {
            (200, vec![], vec![0; 9 << 20])
        } else if p == format!("/v1/blob/{}.tar.gz", "8".repeat(64)) {
            (
                302,
                vec![("Location", "http://127.0.0.1:1/x".into())],
                vec![],
            )
        } else {
            let e = serde_json::json!({"error": "no such revision", "hint": "ptyrc store sync"});
            (404, vec![], json(e))
        }
    });
    let t = hosted::fetch_archive(&stub.base, &rev).unwrap();
    assert_eq!(check(&t).unwrap().manifest.app.id, "sand-garden");
    let e = hosted::fetch_archive(&stub.base, &"d".repeat(64)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
    assert!(e.message.contains("sha256"), "{e}");
    let e = hosted::fetch_archive(&stub.base, &"9".repeat(64)).unwrap_err();
    assert!(e.message.contains("more than"), "{e}");
    let e = hosted::fetch_archive(&stub.base, &"8".repeat(64)).unwrap_err();
    assert!(e.message.contains("redirect"), "{e}");
    let e = hosted::fetch_archive(&stub.base, &"7".repeat(64)).unwrap_err();
    assert!(e.message.contains("404: no such revision"), "{e}");
    assert_eq!(e.hint.as_deref(), Some("ptyrc store sync"));
    assert!(hosted::fetch_archive(&stub.base, "../../etc/passwd").is_err());
    assert_eq!(
        hosted::fetch_module(&stub.base, &rev, &sha256_hex(&wasm)).unwrap(),
        wasm
    );
    let e = hosted::fetch_module(&stub.base, &rev, &"0".repeat(64)).unwrap_err();
    assert!(e.message.contains("sha256"), "{e}");
    let e = hosted::fetch_module(&stub.base, &"e".repeat(64), &sha256_hex(&bad_wasm)).unwrap_err();
    assert!(e.message.contains("env.f"), "{e}");
    // https only, but for a loopback host; nothing after the host.
    for base in [
        "http://example.org",
        "https://example.org/x",
        "ftp://example.org",
    ] {
        assert!(
            ptyrc_store::http::get(base, "/v1/index.json", 10).is_err(),
            "{base}"
        );
    }
    // No server there.
    let e = hosted::fetch_archive("http://127.0.0.1:1", &rev).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
    // https asked of a server of plain http: refused, never downgraded.
    let tls = stub.base.replace("http://", "https://");
    let e = hosted::fetch_archive(&tls, &rev).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch, "{by:?}");
    let by = if by == Transport::Curl {
        "curl"
    } else {
        "python3"
    };
    assert!(e.message.contains(&format!(": {by}: ")), "{e}");
}

/// The script of `python3`, alone: the scheme is the one allowed; a
/// redirect is answered as it is.
#[test]
fn python_script_holds() {
    if !transports().contains(&Transport::Python) {
        return;
    }
    let stub = Stub::new();
    stub.answer(|_| {
        (
            301,
            vec![("Location", "http://127.0.0.1:1/".into())],
            vec![],
        )
    });
    let run = |scheme: &str| {
        let mut c = std::process::Command::new("python3")
            .args(["-I", "-S", "-c", http::PYTHON])
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let line = serde_json::json!({"url": format!("{}/x", stub.base), "scheme": scheme,
            "method": "GET", "headers": [], "length": null, "cap": 10, "connect": 5, "total": 20});
        c.stdin
            .take()
            .unwrap()
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        c.wait_with_output().unwrap()
    };
    let o = run("https");
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("protocol http refused: https only"), "{err}");
    let o = run("http");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(o.stdout, b"301 null\n");
    assert_eq!(stub.seen.lock().unwrap().len(), 1);
}

#[test]
fn push_and_tokens() {
    each(push_and_tokens_by);
}

#[test]
fn push_solves_registry_work() {
    each(|by| {
        let stub = Stub::new();
        let root = tmp(&format!("hosted-work-{by:?}"));
        let (bytes, rev) = archive(&root);
        let answer_rev = rev.clone();
        let mut n = 0;
        stub.answer(move |q| {
            n += 1;
            if n == 1 {
                assert_eq!(q.header("x-ptyrc-work"), None);
                return (
                    429,
                    vec![],
                    json(serde_json::json!({
                        "error": "proof of work required",
                        "hint": "automatic",
                        "work": {"challenge": "abc.DEF-_9", "difficulty": 8}
                    })),
                );
            }
            let proof = q.header("x-ptyrc-work").expect("work header");
            let (challenge, counter) = proof.rsplit_once('.').unwrap();
            assert_eq!(challenge, "abc.DEF-_9");
            assert!(sha256_hex(format!("{challenge}:{counter}").as_bytes()).starts_with("00"));
            (
                201,
                vec![],
                json(serde_json::json!({
                    "name": "sand-garden", "kind": "tar", "rev": answer_rev,
                    "tier": "untested", "token": "tok"
                })),
            )
        });
        let pushed = push::push_tar(&stub.base, &bytes, None, None).unwrap();
        assert_eq!(pushed.rev, rev);
        assert_eq!(stub.seen.lock().unwrap().len(), 2);
    });
}

fn push_and_tokens_by(by: Transport) {
    let stub = Stub::new();
    let root = tmp(&format!("hosted-push-{by:?}"));
    let dirs = Dirs::under(&root);
    let (bytes, rev) = archive(&root);
    let r = rev.clone();
    let mut n = 0;
    let argv: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = argv.clone();
    stub.answer(move |q| {
        n += 1;
        if q.header("authorization").is_some() {
            seen.lock().unwrap().extend(children_argv());
        }
        match (q.path.as_str(), q.header("authorization")) {
            (p, None) if p.starts_with("/v1/push/tar") && n == 1 => (
                201,
                vec![],
                json(
                    serde_json::json!({"name": "sand-garden", "kind": "tar", "rev": r,
                    "tier": "untested", "token": "tok-1.x"}),
                ),
            ),
            (p, Some("Bearer tok-1.x")) if p.starts_with("/v1/push/tar") => (
                200,
                vec![],
                json(
                    serde_json::json!({"name": "sand-garden", "kind": "tar", "rev": r,
                    "tier": "untested"}),
                ),
            ),
            ("/v1/push/git", Some(_)) => {
                let b: serde_json::Value = serde_json::from_slice(&q.body).unwrap();
                (
                    201,
                    vec![],
                    json(
                        serde_json::json!({"name": "example.org/a/sand#apps/sand", "kind": "git",
                        "rev": b["commit"], "tier": "untested"}),
                    ),
                )
            }
            ("/v1/push/tar", None) => (
                200,
                vec![],
                json(serde_json::json!({"name": "sand-garden", "kind": "tar",
                    "rev": "0".repeat(64), "tier": "untested"})),
            ),
            _ => (
                401,
                vec![],
                json(
                    serde_json::json!({"error": "sand-garden: the name is taken",
                    "hint": "the name's token: Authorization: Bearer TOKEN"}),
                ),
            ),
        }
    });
    let p = push::push_tar(&stub.base, &bytes, Some("Paper falls & más"), None).unwrap();
    assert_eq!(
        (
            p.name.as_str(),
            p.kind.as_str(),
            p.rev.as_str(),
            p.tier,
            p.created
        ),
        ("sand-garden", "tar", rev.as_str(), Tier::Untested, true)
    );
    let q = stub.last();
    assert_eq!(q.method, "POST");
    assert_eq!(
        q.path,
        "/v1/push/tar?description=Paper%20falls%20%26%20m%C3%A1s"
    );
    assert_eq!(q.header("content-type"), Some("application/gzip"));
    assert_eq!(q.body, bytes);
    assert_eq!(q.header("authorization"), None);
    let token = p.token.unwrap();
    tokens::set(&dirs, stub.host(), &p.name, &token).unwrap();
    tokens::set(&dirs, stub.host(), "example.org/a/sand#apps/sand", "tok-2").unwrap();
    let mode = std::fs::metadata(dirs.tokens())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        tokens::get(&dirs, stub.host(), "sand-garden")
            .unwrap()
            .as_deref(),
        Some("tok-1.x")
    );
    assert_eq!(
        tokens::get(&dirs, "other.example", "sand-garden").unwrap(),
        None
    );
    // Again, the token sent: in the header, never in argv.
    let t = tokens::get(&dirs, stub.host(), "sand-garden").unwrap();
    let again = push::push_tar(&stub.base, &bytes, None, t.as_deref()).unwrap();
    assert!(!again.created && again.token.is_none());
    let q = stub.last();
    assert_eq!(q.header("authorization"), Some("Bearer tok-1.x"));
    assert_eq!(q.path, "/v1/push/tar");
    let argv = argv.lock().unwrap().clone();
    let by = if by == Transport::Curl {
        "curl"
    } else {
        "python3"
    };
    assert!(argv.iter().any(|a| a.starts_with(by)), "{argv:?}");
    assert!(!argv.iter().any(|a| a.contains("tok-1")), "{argv:?}");
    // A wrong token: the server's error and hint.
    let e = push::push_tar(&stub.base, &bytes, None, Some("wrong")).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
    assert!(
        e.message.contains("401: sand-garden: the name is taken"),
        "{e}"
    );
    assert!(e.hint.unwrap().contains("Bearer"));
    // Another revision answered: refused.
    let e = push::push_tar(&stub.base, &bytes, None, None).unwrap_err();
    assert!(e.message.contains("revision"), "{e}");
    // A token curl's configuration cannot carry.
    assert!(push::push_tar(&stub.base, &bytes, None, Some("a\"b\nurl = x")).is_err());
    // A git push: the pusher's word, as JSON.
    let o = Origin::parse("example.org/a/sand#apps/sand").unwrap();
    let c = "f".repeat(40);
    let g = push::push_git(
        &stub.base,
        &o,
        &c,
        MANIFEST,
        "apps/sand/ptyrc-app.toml",
        None,
        Some("tok-2"),
    )
    .unwrap();
    assert_eq!((g.kind.as_str(), g.rev.as_str()), ("git", c.as_str()));
    let b: serde_json::Value = serde_json::from_slice(&stub.last().body).unwrap();
    assert_eq!(b["origin"], "https://example.org/a/sand");
    assert_eq!(b["path"], "apps/sand");
    assert_eq!(b["manifest"], MANIFEST);
    assert_eq!(b["description"], serde_json::Value::Null);
    let hosted = Origin::parse(&format!("{}/sand-garden", stub.host())).unwrap();
    assert!(push::push_git(&stub.base, &hosted, &c, MANIFEST, "x", None, None).is_err());
}

#[test]
fn hosted_origins() {
    let o = Origin::parse("https://registry.example.org/confetti").unwrap();
    assert!(o.is_hosted());
    assert_eq!(o.id(), "registry.example.org/confetti");
    assert_eq!(o.name(), Some("confetti"));
    assert_eq!(
        o.registry_path().unwrap(),
        "apps/registry.example.org/confetti.toml"
    );
    assert_eq!(
        o.remote(),
        Remote::Registry("https://registry.example.org".into())
    );
    assert_eq!(Origin::parse(&o.id()).unwrap(), o);
    let l = Origin::parse("http://127.0.0.1:8080/confetti").unwrap();
    assert_eq!(l.id(), "127.0.0.1:8080/confetti");
    assert_eq!(
        l.registry_path().unwrap(),
        "apps/127.0.0.1_8080/confetti.toml"
    );
    assert_eq!(l.remote(), Remote::Registry("http://127.0.0.1:8080".into()));
    assert_eq!(Origin::parse(&l.id()).unwrap(), l);
    assert_eq!(
        Origin::parse("localhost/x").unwrap().remote(),
        Remote::Registry("http://localhost".into())
    );
    let g = Origin::parse("https://git.example.org:8443/a/b").unwrap();
    assert!(!g.is_hosted() && g.name().is_none());
    assert_eq!(Origin::parse(&g.id()).unwrap(), g);
    let (h, r) = Origin::parse_spec(&format!("example.org/x@{}", "a".repeat(64))).unwrap();
    assert!(h.is_hosted());
    assert_eq!(r, Some(Reference::Commit("a".repeat(64))));
    // The registries: `api`, https or loopback http, nothing after the host.
    let dirs = Dirs::under(&tmp("hosted-store"));
    let mut regs = Registries::default();
    regs.add_api(&dirs, "main", "https://registry.example.org")
        .unwrap();
    regs.add(&dirs, "git", "https://example.org/reg", false)
        .unwrap();
    for bad in [
        "http://registry.example.org",
        "https://registry.example.org/",
        "https://registry.example.org/v1",
        "https://user@registry.example.org",
        "registry.example.org",
    ] {
        assert!(regs.clone().add_api(&dirs, "x", bad).is_err(), "{bad}");
    }
    let text = regs.to_toml();
    assert!(
        text.contains("api = \"https://registry.example.org\""),
        "{text}"
    );
    assert_eq!(Registries::parse(&dirs, &text).unwrap(), regs);
    let both =
        "[[registry]]\nname = \"x\"\napi = \"https://a.example\"\nurl = \"https://b.example/r\"\n";
    assert!(Registries::parse(&dirs, both).is_err());
    // No store.toml: the default registry server, if any.
    let d = Registries::defaults(&dirs, Some(("main", "https://registry.example.org"))).unwrap();
    assert_eq!(d.list[0].dir, dirs.registry().join("main"));
    assert!(Registries::defaults(&dirs, None).unwrap().list.is_empty());
}
