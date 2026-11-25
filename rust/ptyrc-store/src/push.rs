// SPDX-License-Identifier: GPL-3.0-or-later
//! Pushing to a registry server: an archive (`POST /v1/push/tar`), or a
//! commit of a git origin (`POST /v1/push/git`). The first push of a
//! name answers its token ([`crate::tokens`] keeps it).

use serde::Deserialize;

use crate::error::{Error, ErrorKind, Result};
use crate::git::is_oid;
use crate::http::{encode, is_token, post};
use crate::origin::{Origin, Remote};
use crate::registry::Tier;
use crate::tar::sha256_hex;

/// Bound on the answer, bytes.
const MAX_ANSWER: u64 = 64 << 10;

/// What the server answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    pub name: String,
    /// `tar` or `git`.
    pub kind: String,
    /// The sha256 of the archive, or the commit.
    pub rev: String,
    pub tier: Tier,
    /// The name's token, when this push created the name.
    pub token: Option<String>,
    /// 201: the revision is new; 200: listed already.
    pub created: bool,
}

#[derive(Deserialize)]
struct Answer {
    name: String,
    kind: String,
    rev: String,
    tier: Tier,
    #[serde(default)]
    token: Option<String>,
}

fn push_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Fetch, msg)
}

fn answer(base: &str, r: crate::http::Response, kind: &str, rev: &str) -> Result<Pushed> {
    let a: Answer = serde_json::from_slice(&r.body)
        .map_err(|e| push_err(format!("{base}: the answer to a push: {e}")))?;
    let bad = |what: &str| push_err(format!("{base}: the answer to a push: {what}"));
    if a.kind != kind || a.tier == Tier::Shipped {
        return Err(bad("another kind or tier"));
    }
    if a.rev != rev {
        return Err(bad(&format!("revision {:?}, not {rev}", a.rev))
            .with_hint("the registry did not take what was sent"));
    }
    if a.token.as_deref().is_some_and(|t| !is_token(t)) {
        return Err(bad("a token of odd characters"));
    }
    Ok(Pushed {
        name: a.name,
        kind: a.kind,
        rev: a.rev,
        tier: a.tier,
        token: a.token,
        created: r.status == 201,
    })
}

/// `POST /v1/push/tar?description=...`, the body `archive`. The answer's
/// `rev` must be the archive's sha256, its `name` an app id.
pub fn push_tar(
    base: &str,
    archive: &[u8],
    description: Option<&str>,
    token: Option<&str>,
) -> Result<Pushed> {
    let path = match description {
        Some(d) => format!("/v1/push/tar?description={}", encode(d)),
        None => "/v1/push/tar".to_string(),
    };
    let r = post(base, &path, archive, "application/gzip", token, MAX_ANSWER)?;
    let p = answer(base, r, "tar", &sha256_hex(archive))?;
    if !crate::manifest::is_name(&p.name, 64) {
        return Err(push_err(format!(
            "{base}: the answer to a push: name {:?}",
            p.name
        )));
    }
    Ok(p)
}

/// `POST /v1/push/git`: `commit` of `origin` (a git origin, https), the
/// manifest's text and path at that commit, as read by the pusher. The
/// answer's `name` must be the origin's id, its `rev` the commit.
pub fn push_git(
    base: &str,
    origin: &Origin,
    commit: &str,
    manifest_text: &str,
    manifest_path: &str,
    description: Option<&str>,
    token: Option<&str>,
) -> Result<Pushed> {
    let Remote::Https(url) = origin.remote() else {
        return Err(push_err(format!("{origin}: not a git origin over https"))
            .with_hint("host/owner/repo[#path]"));
    };
    if commit.len() != 40 || !is_oid(commit) {
        return Err(push_err(format!("{commit:?}: not a commit id, 40 hex")));
    }
    let body = serde_json::json!({
        "origin": url,
        "path": origin.sub(),
        "commit": commit,
        "manifest": manifest_text,
        "manifest_path": manifest_path,
        "description": description,
    });
    let body = serde_json::to_vec(&body).expect("json serialises");
    let r = post(
        base,
        "/v1/push/git",
        &body,
        "application/json",
        token,
        MAX_ANSWER,
    )?;
    let p = answer(base, r, "git", commit)?;
    if p.name != origin.id() {
        return Err(push_err(format!(
            "{base}: the answer to a push: name {:?}, not {origin}",
            p.name
        )));
    }
    Ok(p)
}
