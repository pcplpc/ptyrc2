// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP to a registry server, by spawning `curl`, as [`crate::git`] runs
//! `git`.
//!
//! The environment is cleared but `PATH` and the trust store
//! (`SSL_CERT_FILE`, `SSL_CERT_DIR`): no `HOME`, so no `.curlrc` or
//! `.netrc`, no proxy. `-q` first. One protocol (`https`; `http` for a
//! loopback base), no redirect, timeouts, a bound on the bytes read
//! (curl is killed past it). The URL, the headers and the token go on
//! stdin as curl's configuration (`--config -`), never in `argv`; a body
//! goes through a 0600 file in a 0700 directory of ours, removed after.
//! A status other than 2xx is an error: the server's `{"error", "hint"}`
//! as message and hint.

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
    let url = format!("{base}{path}");
    let proto = if is_loopback(&host) && base.starts_with("http://") {
        "=http"
    } else {
        "=https"
    };
    let mut cfg = format!("url = \"{url}\"\nheader = \"Expect:\"\n");
    if let Some(t) = token {
        if !is_token(t) {
            return Err(fetch_err("a token of characters curl cannot be given")
                .with_hint("the token as the registry answered it"));
        }
        cfg.push_str(&format!("header = \"Authorization: Bearer {t}\"\n"));
    }
    let spool = match body {
        Some((bytes, ty)) => {
            if !ty
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-.+".contains(&b))
            {
                return Err(fetch_err(format!("{ty:?}: not a media type")));
            }
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
    let mut c = Command::new("curl");
    c.env_clear();
    for k in ["PATH", "SSL_CERT_FILE", "SSL_CERT_DIR"] {
        if let Some(v) = std::env::var_os(k) {
            c.env(k, v);
        }
    }
    c.env("LC_ALL", "C")
        .arg("-q")
        .args(["--silent", "--show-error", "--globoff"])
        .args(["--proto", proto, "--proto-redir", proto])
        .args(["--max-redirs", "0", "--no-keepalive"])
        .arg("--connect-timeout")
        .arg(CONNECT_TIMEOUT.to_string())
        .arg("--max-time")
        .arg(MAX_TIME.to_string())
        .arg("--max-filesize")
        .arg(cap.to_string())
        .args(["--dump-header", "-", "--output", "-", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| {
        fetch_err(format!("curl: {e}")).with_hint("install curl; it is run as `curl` from PATH")
    })?;
    let mut stdin = child.stdin.take().expect("piped");
    let written = stdin.write_all(cfg.as_bytes());
    drop(stdin);
    let mut stderr = child.stderr.take().expect("piped");
    let errs = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut stderr).take(16 << 10).read_to_end(&mut v);
        v
    });
    let limit = cap + MAX_HEADERS;
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
    let status = child.wait().map_err(|e| fetch_err(format!("curl: {e}")))?;
    let errs = errs.join().unwrap_or_default();
    drop(spool);
    let too_big = || {
        fetch_err(format!("{url}: more than {cap} bytes"))
            .with_hint("the registry answers more than a client takes")
    };
    if out.len() as u64 > limit {
        return Err(too_big());
    }
    written.map_err(|e| fetch_err(format!("curl: {e}")))?;
    read.map_err(|e| fetch_err(format!("curl: {e}")))?;
    if !status.success() {
        if status.code() == Some(63) {
            return Err(too_big());
        }
        let said = clean(&errs, 300);
        let said = said.strip_prefix("curl: ").unwrap_or(&said);
        return Err(fetch_err(format!("{url}: curl: {said}")));
    }
    let (code, etag, body) = split_headers(&out)
        .ok_or_else(|| fetch_err(format!("{url}: an answer that is not HTTP")))?;
    if body.len() as u64 > cap {
        return Err(too_big());
    }
    if !(200..300).contains(&code) {
        return Err(refusal(&url, code, body));
    }
    Ok(Response {
        status: code,
        body: body.to_vec(),
        etag,
    })
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
