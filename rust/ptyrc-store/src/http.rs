// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP to a registry server, by spawning `curl`, as [`crate::git`] runs
//! `git`; without `curl` on `PATH`, `python3` (its standard library).
//! `PTYRC_HTTP=curl|python` chooses one.
//!
//! The environment is cleared but `PATH` and the trust store
//! (`SSL_CERT_FILE`, `SSL_CERT_DIR`): no `HOME`, so no `.curlrc` or
//! `.netrc`, no proxy. One protocol (`https`; `http` for a loopback
//! base), no redirect, timeouts, a bound on the bytes read (the child
//! is killed past it). The URL, the headers and the token go on stdin,
//! never in `argv`. A status other than 2xx is an error: the server's
//! `{"error", "hint"}` as message and hint.
//!
//! `curl`: `-q` first; stdin is its configuration (`--config -`); a
//! body goes through a 0600 file in a 0700 directory of ours, removed
//! after. `python3 -I -S -c SCRIPT`: [`PYTHON`]; stdin is a JSON line,
//! then the body; stdout is `STATUS ETAG\n` (`ETAG` a JSON string or
//! `null`), then the body, `cap + 1` bytes at most.

use std::cell::Cell;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{Error, ErrorKind, Result};
use crate::origin::{is_loopback, parse_base};

/// Bound on the response headers, bytes.
const MAX_HEADERS: u64 = 64 << 10;
/// Seconds to connect, and for the whole exchange.
const CONNECT_TIMEOUT: u32 = 15;
const MAX_TIME: u32 = 300;

/// A 2xx answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
}

fn fetch_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Fetch, msg)
}

/// Percent-encodes every byte but `A-Z a-z 0-9 - . _ ~`.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `GET base+path`; at most `cap` bytes of body.
pub fn get(base: &str, path: &str, cap: u64) -> Result<Response> {
    request(base, path, None, None, cap)
}

/// `POST base+path_and_query`, `body` of `content_type`; `token` as
/// `Authorization: Bearer`.
pub fn post(
    base: &str,
    path_and_query: &str,
    body: &[u8],
    content_type: &str,
    token: Option<&str>,
    cap: u64,
) -> Result<Response> {
    request(base, path_and_query, Some((body, content_type)), token, cap)
}

/// A token as the server gives it: printable ASCII, no space, quote or
/// backslash; 512 bytes at most.
pub fn is_token(t: &str) -> bool {
    (1..=512).contains(&t.len())
        && t.bytes()
            .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
}

/// A file of ours, 0600 in a 0700 directory; removed on drop.
struct Spool(PathBuf);

impl Spool {
    fn new(bytes: &[u8]) -> Result<Spool> {
        static N: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!(
            "ptyrc-http-{}-{}-{nanos}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| Error::io(dir.display(), e))?;
        let spool = Spool(dir);
        let p = spool.0.join("body");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
            .map_err(|e| Error::io(p.display(), e))?;
        f.write_all(bytes).map_err(|e| Error::io(p.display(), e))?;
        Ok(spool)
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What carries a request.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Curl,
    Python,
}

thread_local! {
    static FORCED: Cell<Option<Transport>> = const { Cell::new(None) };
}

/// `f` with every request of this thread by `t`. Tests.
#[doc(hidden)]
pub fn with_transport<T>(t: Transport, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Transport>);
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCED.with(|c| c.set(self.0));
        }
    }
    let _reset = Reset(FORCED.with(|c| c.replace(Some(t))));
    f()
}

/// An executable `name` in a directory of `PATH`.
fn on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|d| {
            std::fs::metadata(d.join(name))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

/// [`with_transport`], else `PTYRC_HTTP`, else `curl` on `PATH`, else
/// `python3`.
fn transport() -> Result<Transport> {
    if let Some(t) = FORCED.with(Cell::get) {
        return Ok(t);
    }
    match std::env::var("PTYRC_HTTP").as_deref() {
        Ok("curl") => return Ok(Transport::Curl),
        Ok("python") => return Ok(Transport::Python),
        Ok("") | Err(std::env::VarError::NotPresent) => {}
        Ok(v) => {
            return Err(
                fetch_err(format!("PTYRC_HTTP={v:?}: neither curl nor python"))
                    .with_hint("PTYRC_HTTP=curl, PTYRC_HTTP=python, or unset"),
            );
        }
        Err(_) => return Err(fetch_err("PTYRC_HTTP: not UTF-8").with_hint("curl or python")),
    }
    if on_path("curl") {
        Ok(Transport::Curl)
    } else if on_path("python3") {
        Ok(Transport::Python)
    } else {
        Err(fetch_err("neither curl nor python3 on PATH").with_hint("install curl, or python3"))
    }
}

/// A request, checked.
struct Req<'a> {
    url: String,
    /// `http` allowed: a loopback base given as `http://`.
    http: bool,
    body: Option<(&'a [u8], &'a str)>,
    token: Option<&'a str>,
    cap: u64,
}

fn too_big(url: &str, cap: u64) -> Error {
    fetch_err(format!("{url}: more than {cap} bytes"))
        .with_hint("the registry answers more than a client takes")
}

fn request(
    base: &str,
    path: &str,
    body: Option<(&[u8], &str)>,
    token: Option<&str>,
    cap: u64,
) -> Result<Response> {
    let host = parse_base(base)?;
    let ok_path = path.starts_with('/')
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-._~%?=&".contains(&b));
    if !ok_path {
        return Err(fetch_err(format!("{path:?}: not an encoded path")));
    }
    if let Some(t) = token
        && !is_token(t)
    {
        return Err(fetch_err("a token of characters curl cannot be given")
            .with_hint("the token as the registry answered it"));
    }
    if let Some((_, ty)) = body
        && !ty
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-.+".contains(&b))
    {
        return Err(fetch_err(format!("{ty:?}: not a media type")));
    }
    let req = Req {
        url: format!("{base}{path}"),
        http: is_loopback(&host) && base.starts_with("http://"),
        body,
        token,
        cap,
    };
    let (code, etag, body) = match transport()? {
        Transport::Curl => curl(&req)?,
        Transport::Python => python(&req)?,
    };
    if body.len() as u64 > cap {
        return Err(too_big(&req.url, cap));
    }
    if !(200..300).contains(&code) {
        return Err(refusal(&req.url, code, &body));
    }
    Ok(Response {
        status: code,
        body,
        etag,
    })
}

/// The environment of a child: `PATH`, the trust store, `LC_ALL=C`.
fn child(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env_clear();
    for k in ["PATH", "SSL_CERT_FILE", "SSL_CERT_DIR"] {
        if let Some(v) = std::env::var_os(k) {
            c.env(k, v);
        }
    }
    c.env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

/// A child run: `input` written to its stdin, at most `limit + 1`
/// bytes of stdout read, killed past `limit`; its status, stdout, the
/// first 16 KiB of stderr and the outcome of the write.
struct Ran {
    status: std::process::ExitStatus,
    out: Vec<u8>,
    errs: Vec<u8>,
    written: std::io::Result<()>,
    read: std::io::Result<usize>,
}

fn run(mut child: std::process::Child, input: &[&[u8]], limit: u64, name: &str) -> Result<Ran> {
    let mut stdin = child.stdin.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let errs = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut stderr).take(16 << 10).read_to_end(&mut v);
        // The rest, dropped, so that the child never blocks.
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        v
    });
    let written = input.iter().try_for_each(|b| stdin.write_all(b));
    drop(stdin);
    let mut out = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped")
        .take(limit + 1)
        .read_to_end(&mut out);
    if out.len() as u64 > limit {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|e| fetch_err(format!("{name}: {e}")))?;
    let errs = errs.join().unwrap_or_default();
    Ok(Ran {
        status,
        out,
        errs,
        written,
        read,
    })
}

type Answer = (u16, Option<String>, Vec<u8>);

fn curl(req: &Req) -> Result<Answer> {
    let (url, cap) = (&req.url, req.cap);
    let proto = if req.http { "=http" } else { "=https" };
    let mut cfg = format!("url = \"{url}\"\nheader = \"Expect:\"\n");
    if let Some(t) = req.token {
        cfg.push_str(&format!("header = \"Authorization: Bearer {t}\"\n"));
    }
    let spool = match req.body {
        Some((bytes, ty)) => {
            let s = Spool::new(bytes)?;
            let p = s.0.join("body");
            let p = p.to_str().filter(|p| !p.contains(['"', '\\', '\n']));
            let p = p.ok_or_else(|| fetch_err("the temporary directory: an odd path"))?;
            cfg.push_str(&format!(
                "header = \"Content-Type: {ty}\"\ndata-binary = \"@{p}\"\n"
            ));
            Some(s)
        }
        None => None,
    };
    let mut c = child("curl");
    c.arg("-q")
        .args(["--silent", "--show-error", "--globoff"])
        .args(["--proto", proto, "--proto-redir", proto])
        .args(["--max-redirs", "0", "--no-keepalive"])
        .arg("--connect-timeout")
        .arg(CONNECT_TIMEOUT.to_string())
        .arg("--max-time")
        .arg(MAX_TIME.to_string())
        .arg("--max-filesize")
        .arg(cap.to_string())
        .args(["--dump-header", "-", "--output", "-", "--config", "-"]);
    let spawned = c.spawn().map_err(|e| {
        fetch_err(format!("curl: {e}")).with_hint("install curl; it is run as `curl` from PATH")
    })?;
    let limit = cap + MAX_HEADERS;
    let ran = run(spawned, &[cfg.as_bytes()], limit, "curl")?;
    drop(spool);
    if ran.out.len() as u64 > limit {
        return Err(too_big(url, cap));
    }
    ran.written.map_err(|e| fetch_err(format!("curl: {e}")))?;
    ran.read.map_err(|e| fetch_err(format!("curl: {e}")))?;
    if !ran.status.success() {
        if ran.status.code() == Some(63) {
            return Err(too_big(url, cap));
        }
        let said = clean(&ran.errs, 300);
        let said = said.strip_prefix("curl: ").unwrap_or(&said);
        return Err(fetch_err(format!("{url}: curl: {said}")));
    }
    let (code, etag, body) = split_headers(&ran.out)
        .ok_or_else(|| fetch_err(format!("{url}: an answer that is not HTTP")))?;
    Ok((code, etag, body.to_vec()))
}

/// The script `python3 -I -S -c` runs: standard library only; no proxy
/// (`http.client` reads none), no redirect followed (a 3xx is answered
/// as it is), certificate and host name checked (`ssl`'s default
/// context), the scheme the one allowed, `SIGALRM` at the total time.
pub const PYTHON: &str = r#"import http.client, json, signal, ssl, sys, urllib.parse
def main():
    inp, out = sys.stdin.buffer, sys.stdout.buffer
    q = json.loads(inp.readline(1 << 16))
    signal.alarm(q["total"])
    n = q["length"]
    body = None
    if n is not None:
        body = inp.read(n)
        if len(body) != n:
            raise OSError("the request body: short")
    u = urllib.parse.urlsplit(q["url"])
    if u.scheme != q["scheme"] or u.username or u.password or u.fragment:
        raise OSError("protocol " + u.scheme + " refused: " + q["scheme"] + " only")
    if u.scheme == "https":
        c = http.client.HTTPSConnection(u.hostname, u.port, timeout=q["connect"],
                                        context=ssl.create_default_context())
    else:
        c = http.client.HTTPConnection(u.hostname, u.port, timeout=q["connect"])
    c.connect()
    c.sock.settimeout(q["total"])
    path = u.path + ("?" + u.query if u.query else "")
    headers = dict(q["headers"])
    headers["Connection"] = "close"
    c.request(q["method"], path, body=body, headers=headers)
    r = c.getresponse()
    out.write(("%d %s\n" % (r.status, json.dumps(r.getheader("ETag")))).encode("ascii"))
    left = q["cap"] + 1
    while left > 0:
        b = r.read(min(left, 1 << 16))
        if not b:
            break
        out.write(b)
        left -= len(b)
    out.flush()
try:
    main()
except Exception as e:
    sys.stderr.write(type(e).__name__ + ": " + str(e) + "\n")
    sys.exit(1)
"#;

fn python(req: &Req) -> Result<Answer> {
    let (url, cap) = (&req.url, req.cap);
    let mut headers = Vec::new();
    if let Some(t) = req.token {
        headers.push(("Authorization".to_string(), format!("Bearer {t}")));
    }
    if let Some((_, ty)) = req.body {
        headers.push(("Content-Type".to_string(), ty.to_string()));
    }
    let line = serde_json::json!({
        "url": url,
        "scheme": if req.http { "http" } else { "https" },
        "method": if req.body.is_some() { "POST" } else { "GET" },
        "headers": headers,
        "length": req.body.map(|(b, _)| b.len()),
        "cap": cap,
        "connect": CONNECT_TIMEOUT,
        "total": MAX_TIME,
    });
    let mut line = line.to_string().into_bytes();
    line.push(b'\n');
    let mut c = child("python3");
    c.args(["-I", "-S", "-c", PYTHON]);
    let spawned = c.spawn().map_err(|e| {
        fetch_err(format!("python3: {e}"))
            .with_hint("install curl, or python3; each is run from PATH")
    })?;
    let limit = cap + MAX_HEADERS;
    let body = req.body.map_or(&[][..], |(b, _)| b);
    let ran = run(spawned, &[&line, body], limit, "python3")?;
    if ran.out.len() as u64 > limit {
        return Err(too_big(url, cap));
    }
    ran.read.map_err(|e| fetch_err(format!("python3: {e}")))?;
    if !ran.status.success() {
        use std::os::unix::process::ExitStatusExt;
        let said = match ran.status.signal() {
            Some(14) => format!("past {MAX_TIME} s"),
            _ => clean(&ran.errs, 300),
        };
        return Err(fetch_err(format!("{url}: python3: {said}")));
    }
    ran.written
        .map_err(|e| fetch_err(format!("python3: {e}")))?;
    let not_http = || fetch_err(format!("{url}: an answer that is not HTTP"));
    let end = ran
        .out
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(not_http)?;
    let head = std::str::from_utf8(&ran.out[..end]).map_err(|_| not_http())?;
    let (code, etag) = head.split_once(' ').ok_or_else(not_http)?;
    let code: u16 = code.parse().map_err(|_| not_http())?;
    let etag: Option<String> = serde_json::from_str(etag).map_err(|_| not_http())?;
    Ok((code, etag, ran.out[end + 1..].to_vec()))
}

/// The status, the `ETag` and the body after the last header block.
fn split_headers(out: &[u8]) -> Option<(u16, Option<String>, &[u8])> {
    let mut rest = out;
    loop {
        let end = rest.windows(4).position(|w| w == b"\r\n\r\n")?;
        let head = std::str::from_utf8(&rest[..end]).ok()?;
        rest = &rest[end + 4..];
        let mut lines = head.split("\r\n");
        let code: u16 = lines.next()?.split(' ').nth(1)?.parse().ok()?;
        if (100..200).contains(&code) {
            continue;
        }
        let etag = lines.find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("etag")
                .then(|| v.trim().to_string())
        });
        return Some((code, etag, rest));
    }
}

/// An error answer: the server's `error` and `hint` when it sent them.
fn refusal(url: &str, code: u16, body: &[u8]) -> Error {
    #[derive(serde::Deserialize)]
    struct Said {
        error: String,
        #[serde(default)]
        hint: Option<String>,
    }
    if (300..400).contains(&code) {
        return fetch_err(format!("{url}: {code}: a redirect, not followed"))
            .with_hint("the registry's own base URL, https://host");
    }
    match serde_json::from_slice::<Said>(body) {
        Ok(s) => {
            let e = fetch_err(format!("{url}: {code}: {}", clean(s.error.as_bytes(), 500)));
            match s.hint {
                Some(h) => e.with_hint(clean(h.as_bytes(), 500)),
                None => e,
            }
        }
        Err(_) => fetch_err(format!("{url}: {code}: {}", clean(body, 200))),
    }
}

/// Printable characters only, at most `max`: a server may send escapes.
fn clean(b: &[u8], max: usize) -> String {
    let s: String = String::from_utf8_lossy(b)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect();
    s.trim().to_string()
}
