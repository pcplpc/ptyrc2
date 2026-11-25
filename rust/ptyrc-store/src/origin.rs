// SPDX-License-Identifier: GPL-3.0-or-later
//! The identity of an app: where its source is. `host/owner/repo[#path]`,
//! `https://host/owner/repo[.git][#path]`, or `local:/abs/dir[#path]`
//! (dev and tests); each `[@REF]` in a spec. Hosted on a registry server:
//! `host/NAME`, `https://host/NAME`; `http://` for a loopback host only.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind, Result, check_rel_path};

/// An origin, normalised. Two spellings of one repository compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    /// `host[:port]`, lower case; empty for a local origin.
    host: String,
    /// `owner/.../repo`, no `.git`; for a local origin, the absolute path.
    path: String,
    /// The app's directory inside the repository.
    sub: Option<String>,
    local: bool,
    /// `host/NAME`: an archive on a registry server; `path` is NAME.
    hosted: bool,
}

/// What to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// The remote's `HEAD`.
    Head,
    /// A branch or a tag, as the remote names it.
    Name(String),
    /// A full commit id, lower-case hex (40 or 64 digits).
    Commit(String),
}

/// Where the source is fetched from: by `git`, or from a registry server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remote {
    Https(String),
    Local(PathBuf),
    /// The base URL of a registry server: `https://host`, or
    /// `http://host` for a loopback host.
    Registry(String),
}

/// Bound on the identity, bytes; its directory name stays below 255.
const MAX_ID: usize = 200;

fn origin_err(spec: &str, why: impl fmt::Display) -> Error {
    Error::new(ErrorKind::Origin, format!("origin {spec:?}: {why}")).with_hint(
        "host/owner/repo[#path][@REF], https://host/owner/repo[#path][@REF], or host/NAME[@REV]",
    )
}

/// `127.0.0.1[:port]` or `localhost[:port]`.
pub(crate) fn is_loopback(host: &str) -> bool {
    let name = host.split_once(':').map_or(host, |(n, _)| n);
    name == "127.0.0.1" || name == "localhost"
}

/// The base URL of a registry server for `host`.
pub(crate) fn base_of(host: &str) -> String {
    match is_loopback(host) {
        true => format!("http://{host}"),
        false => format!("https://{host}"),
    }
}

/// The host of a base URL: `https://HOST`, or `http://HOST` for a
/// loopback host; nothing after the host.
pub fn parse_base(base: &str) -> Result<String> {
    let bad = || {
        Error::new(
            ErrorKind::Registry,
            format!("registry {base:?}: not a base URL"),
        )
        .with_hint("https://host, nothing after the host; http:// for 127.0.0.1 or localhost")
    };
    let (http, rest) = match (base.strip_prefix("https://"), base.strip_prefix("http://")) {
        (Some(r), _) => (false, r),
        (None, Some(r)) => (true, r),
        _ => return Err(bad()),
    };
    if !is_host(rest) || (http && !is_loopback(rest)) {
        return Err(bad());
    }
    Ok(rest.to_string())
}

impl Origin {
    /// Parses `ORIGIN[@REF]`.
    pub fn parse_spec(spec: &str) -> Result<(Origin, Option<Reference>)> {
        let (head, reference) = match spec.rsplit_once('@') {
            Some((h, r)) => (h, Some(Reference::parse(r)?)),
            None => (spec, None),
        };
        Ok((Origin::parse(head)?, reference))
    }

    /// Parses an origin without `@REF`.
    pub fn parse(spec: &str) -> Result<Origin> {
        if spec.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(origin_err(spec, "a space or a control character"));
        }
        if spec.contains("::") {
            return Err(origin_err(spec, "a transport (`::`) is not accepted"));
        }
        let (main, sub) = match spec.split_once('#') {
            Some((m, s)) => (m, Some(s)),
            None => (spec, None),
        };
        let sub = match sub {
            None => None,
            Some(s) => {
                let s = s.trim_end_matches('/');
                check_rel_path(s).map_err(|w| origin_err(spec, format!("#path: {w}")))?;
                if !s.split('/').all(is_segment) {
                    return Err(origin_err(
                        spec,
                        "#path: letters, digits, `.`, `_`, `-` per component",
                    ));
                }
                Some(s.to_string())
            }
        };
        if let Some(dir) = main.strip_prefix("local:") {
            return Origin::local(Path::new(dir), sub.as_deref());
        }
        let (http, rest) = if let Some(r) = main.strip_prefix("https://") {
            (false, r)
        } else if let Some(r) = main.strip_prefix("http://") {
            (true, r)
        } else if main.contains("://") {
            return Err(origin_err(spec, "https only").with_hint(
                "https://host/owner/repo, or host/owner/repo; no ssh, http, git, file",
            ));
        } else {
            (false, main)
        };
        if rest.contains(['?', '@', '%']) {
            return Err(origin_err(spec, "a query, a user or an escape"));
        }
        let rest = rest.trim_end_matches('/');
        let mut parts = rest.split('/');
        let host = parts.next().unwrap_or("").to_ascii_lowercase();
        if !is_host(&host) {
            return Err(origin_err(spec, format!("host {host:?}")));
        }
        let mut segs: Vec<&str> = parts.collect();
        if http && !(segs.len() == 1 && is_loopback(&host)) {
            return Err(origin_err(spec, "https only")
                .with_hint("http:// for an app of a registry on 127.0.0.1 or localhost alone"));
        }
        if segs.len() == 1 {
            return Origin::hosted(spec, &host, segs[0], sub.is_some());
        }
        if let Some(last) = segs.last_mut() {
            *last = last.strip_suffix(".git").unwrap_or(last);
        }
        if segs.len() < 2 || !segs.iter().all(|s| is_segment(s)) {
            return Err(origin_err(
                spec,
                "owner/repo: letters, digits, `.`, `_`, `-` per component",
            ));
        }
        Origin::finish(Origin {
            host,
            path: segs.join("/"),
            sub,
            local: false,
            hosted: false,
        })
    }

    fn hosted(spec: &str, host: &str, name: &str, sub: bool) -> Result<Origin> {
        if sub {
            return Err(origin_err(spec, "no #path for an app of a registry"));
        }
        if !host.contains('.') && !is_loopback(host) {
            return Err(origin_err(spec, format!("host {host:?}: not a domain")));
        }
        if !crate::manifest::is_name(name, 64) {
            return Err(origin_err(
                spec,
                format!("{name:?}: lower-case letters, digits and `-`, a letter first, 64 at most"),
            ));
        }
        Origin::finish(Origin {
            host: host.to_string(),
            path: name.to_string(),
            sub: None,
            local: false,
            hosted: true,
        })
    }

    /// A local repository or directory, for dev and tests.
    pub fn local(dir: &Path, sub: Option<&str>) -> Result<Origin> {
        let s = dir
            .to_str()
            .ok_or_else(|| origin_err("local:", "not UTF-8"))?;
        if !dir.is_absolute() {
            return Err(origin_err(s, "a local origin is an absolute path"));
        }
        if s.contains(':') || s.chars().any(char::is_control) {
            return Err(origin_err(s, "`:` or a control character in a local path"));
        }
        let path = s.trim_end_matches('/');
        if path.split('/').any(|p| p == ".." || p == ".") {
            return Err(origin_err(s, "`.` or `..` in a local path"));
        }
        if let Some(sub) = sub {
            check_rel_path(sub).map_err(|w| origin_err(s, format!("#path: {w}")))?;
        }
        Origin::finish(Origin {
            host: String::new(),
            path: path.to_string(),
            sub: sub.map(str::to_string),
            local: true,
            hosted: false,
        })
    }

    fn finish(o: Origin) -> Result<Origin> {
        let id = o.id();
        if id.len() > MAX_ID {
            return Err(origin_err(&id, format!("longer than {MAX_ID} bytes")));
        }
        Ok(o)
    }

    /// The normalised identity: `host/owner/repo[#path]`, or
    /// `local:/dir[#path]`.
    pub fn id(&self) -> String {
        let mut s = if self.local {
            format!("local:{}", self.path)
        } else {
            format!("{}/{}", self.host, self.path)
        };
        if let Some(sub) = &self.sub {
            s.push('#');
            s.push_str(sub);
        }
        s
    }

    pub fn is_local(&self) -> bool {
        self.local
    }

    /// An app of a registry server: `host/NAME`.
    pub fn is_hosted(&self) -> bool {
        self.hosted
    }

    /// NAME of a hosted origin.
    pub fn name(&self) -> Option<&str> {
        self.hosted.then_some(self.path.as_str())
    }

    /// `host[:port]`; empty for a local origin.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The app's directory inside the repository.
    pub fn sub(&self) -> Option<&str> {
        self.sub.as_deref()
    }

    /// The repository to fetch.
    pub fn remote(&self) -> Remote {
        if self.local {
            Remote::Local(PathBuf::from(&self.path))
        } else if self.hosted {
            Remote::Registry(base_of(&self.host))
        } else {
            Remote::Https(format!("https://{}/{}", self.host, self.path))
        }
    }

    /// The identity as one file name: bytes outside `[A-Za-z0-9._-]` as
    /// `%XX`. Never `.`, `..`, nor holding `/`.
    pub fn dir_name(&self) -> String {
        let mut out = String::new();
        for b in self.id().bytes() {
            if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    /// The pointer file in a registry:
    /// `apps/<host>/<owner>/<repo>[/<path>].toml`, `apps/<host>/<NAME>.toml`;
    /// `:` as `_`. None for a local origin.
    pub fn registry_path(&self) -> Option<String> {
        if self.local {
            return None;
        }
        let mut s = format!("apps/{}/{}", self.host.replace(':', "_"), self.path);
        if let Some(sub) = &self.sub {
            s.push('/');
            s.push_str(sub);
        }
        s.push_str(".toml");
        Some(s)
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id())
    }
}

impl Reference {
    /// `HEAD`, a full commit id, or a ref name as `git check-ref-format`
    /// would accept, never starting with `-`.
    pub fn parse(s: &str) -> Result<Reference> {
        if s == "HEAD" {
            return Ok(Reference::Head);
        }
        if (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(Reference::Commit(s.to_ascii_lowercase()));
        }
        let bad = s.is_empty()
            || s.len() > 255
            || s.starts_with(['-', '/', '.'])
            || s.ends_with(['/', '.'])
            || s.ends_with(".lock")
            || s.contains("..")
            || s.contains("//")
            || s.contains("@{")
            || s.contains("/.")
            || s.chars().any(|c| {
                !c.is_ascii_graphic() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '@')
            });
        if bad {
            return Err(
                Error::new(ErrorKind::Origin, format!("@REF {s:?}: not a ref"))
                    .with_hint("a branch, a tag, HEAD, or a full commit id"),
            );
        }
        Ok(Reference::Name(s.to_string()))
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Head => f.write_str("HEAD"),
            Reference::Name(s) | Reference::Commit(s) => f.write_str(s),
        }
    }
}

fn is_host(h: &str) -> bool {
    let (name, port) = match h.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (h, None),
    };
    if let Some(p) = port
        && (p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn is_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && !s.starts_with(['.', '-'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
