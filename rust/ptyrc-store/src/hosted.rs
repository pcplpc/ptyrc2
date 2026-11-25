// SPDX-License-Identifier: GPL-3.0-or-later
//! An app of a registry server (`HOST/NAME`): the choice of a revision,
//! the archive and the module fetched and verified. The index is the
//! server's word, used to choose a revision; the archive is checked by
//! its sha256 (the revision), then by [`crate::tree::check`] (the
//! caller's), as a git tree.

use crate::build::{BOUNDS, imports};
use crate::error::{Error, ErrorKind, Result};
use crate::git::is_oid;
use crate::origin::Origin;
use crate::registry::{Pick, Pointer, Registries, Want, choose, read_pointer};
use crate::tar::{TarTree, sha256_hex};

/// Bound on an archive, bytes.
pub const MAX_ARCHIVE: u64 = 8 << 20;

fn fetch_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Fetch, msg)
}

fn sha256_arg(what: &str, s: &str) -> Result<()> {
    match s.len() == 64 && is_oid(s) {
        true => Ok(()),
        false => Err(fetch_err(format!("{what} {s:?}: not a sha256, 64 hex"))),
    }
}

/// `GET /v1/blob/<rev>.tar.gz`, 8 MiB at most, its sha256 `rev`; parsed.
pub fn fetch_archive(base: &str, rev: &str) -> Result<TarTree> {
    sha256_arg("revision", rev)?;
    let r = crate::http::get(base, &format!("/v1/blob/{rev}.tar.gz"), MAX_ARCHIVE)?;
    let got = sha256_hex(&r.body);
    if got != rev {
        return Err(fetch_err(format!(
            "{base}: the archive of {rev} has the sha256 {got}"
        ))
        .with_hint("the registry answers another archive than the one named; try later, or tell its owner"));
    }
    TarTree::parse(&r.body)
}

/// `GET /v1/blob/<rev>.wasm`, the build's module bound at most; its
/// sha256 `sha256` (the index's), every import of `ptyrc`.
pub fn fetch_module(base: &str, rev: &str, sha256: &str) -> Result<Vec<u8>> {
    sha256_arg("revision", rev)?;
    sha256_arg("module", sha256)?;
    let r = crate::http::get(base, &format!("/v1/blob/{rev}.wasm"), BOUNDS.module)?;
    let got = sha256_hex(&r.body);
    if got != sha256 {
        return Err(fetch_err(format!(
            "{base}: the module of {rev} has the sha256 {got}, the index says {sha256}"
        ))
        .with_hint("ptyrc store sync, then again; or build it here, with rustc"));
    }
    if let Some((m, n)) = imports(&r.body)?.into_iter().find(|(m, _)| m != "ptyrc") {
        return Err(Error::new(
            ErrorKind::Build,
            format!("{base}: the module of {rev} imports {m}.{n}"),
        )
        .with_hint("imports of `ptyrc` only; build it here, with rustc"));
    }
    Ok(r.body)
}

/// [`choose`] for a hosted origin: a pointer wanted (a server's app is
/// listed, or it is not known here).
pub fn pick(origin: &Origin, pointer: Option<&Pointer>, want: &Want) -> Result<Pick> {
    match pointer {
        Some(p) => choose(Some(p), want),
        None => Err(Error::new(
            ErrorKind::Registry,
            format!("{origin}: listed by no registry here"),
        )
        .with_hint("ptyrc store sync; the registry of that host, in store.toml (api)")),
    }
}

/// The hosted origin of the first registry server, its copy present,
/// that lists `name`.
pub fn resolve_name(registries: &Registries, name: &str) -> Result<Option<Origin>> {
    if !crate::manifest::is_name(name, 64) {
        return Ok(None);
    }
    for r in &registries.list {
        let Some(host) = r.api_host() else {
            continue;
        };
        if !r.dir.is_dir() {
            continue;
        }
        let o = Origin::parse(&format!("{host}/{name}"))?;
        let Some(rel) = o.registry_path() else {
            continue;
        };
        if read_pointer(&r.dir, &rel, &o)?.is_some() {
            return Ok(Some(o));
        }
    }
    Ok(None)
}
