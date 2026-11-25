// SPDX-License-Identifier: GPL-3.0-or-later
//! Fetching a registry named by `url` in `store.toml`: its head, depth
//! 1, with the hardening of an app fetch ([`Repo`]); then its pointer
//! files and `index.json`, read with `git cat-file`, are written under
//! `<cache>/ptyrc/registry/<name>/`. Nothing is checked out; the bare
//! repository is removed after.
//!
//! A registry server (`api`): `GET /v1/index.json`, written in the same
//! layout: a pointer file per app at its [`Origin::registry_path`] and an
//! `index.json` of [`Index`]. A `tar` app's origin is `HOST/NAME`, HOST
//! the registry's own; a `git` app's, its `url` and `manifest_path`,
//! which must give its `name`. An app or a version that does not parse
//! or agree is left out. No clock: `now` is an argument.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::error::{Error, ErrorKind, Result, check_rel_path};
use crate::git::{Repo, is_oid};
use crate::index::{FORMAT, INDEX, Index, IndexApp, IndexPermission, IndexVersion};
use crate::install::{mkdir, write_private};
use crate::manifest::{MANIFEST, colour, is_name};
use crate::origin::{Origin, Reference, Remote, base_of, parse_base};
use crate::registry::{Pick, Pointer, Registry, Tier, Want, choose};
use crate::tree::{Kind, Tree};

/// A copy older than this is fetched again by the implicit sync.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 3600);
/// [`MAX_AGE`] of a registry server's copy.
pub const MAX_AGE_API: Duration = Duration::from_secs(60);
/// Bound on one registry fetch, bytes of packs.
pub const MAX_REGISTRY_FETCH: u64 = 64 << 20;
/// Bound on the files written, bytes.
pub const MAX_EXPORT: u64 = 64 << 20;
/// Bound on one pointer file, bytes.
pub const MAX_POINTER: u64 = 1 << 20;
/// Bound on `index.json`, bytes; larger, it is left out.
pub const MAX_INDEX: u64 = 32 << 20;
/// Bound on the pointer files of a registry.
pub const MAX_FILES: usize = 100_000;
/// Name of the stamp in the copy: `SECONDS COMMIT`; for a registry
/// server, COMMIT is the sha256 of its `index.json`.
pub const STAMP: &str = ".fetched";

/// What [`sync`] came to.
#[derive(Debug)]
pub enum Synced {
    /// A `path` registry: read as it is.
    Local,
    /// The copy is younger than `max_age`.
    Fresh,
    /// The commit fetched; for a registry server, the sha256 of its
    /// `index.json`.
    Fetched { commit: String },
    /// The fetch failed; the copy of `since` (seconds) is used.
    Stale { error: Error, since: Option<u64> },
    /// The fetch failed and there is no copy.
    Missing { error: Error },
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The stamp of the copy: when it was fetched (seconds), and its commit.
pub fn stamp(reg: &Registry) -> Option<(u64, String)> {
    let s = std::fs::read_to_string(reg.dir.join(STAMP)).ok()?;
    let (at, commit) = s.trim().split_once(' ')?;
    Some((at.parse().ok()?, commit.to_string()))
}

/// Fetches `reg` unless it is a `path` registry or its copy is younger
/// than `max_age` (`None`: always). The caller holds the store lock.
pub fn sync(reg: &Registry, now: SystemTime, max_age: Option<Duration>) -> Synced {
    let Some(remote) = reg.remote() else {
        return Synced::Local;
    };
    let now = secs(now);
    let seen = stamp(reg).filter(|_| reg.dir.is_dir());
    if let (Some(age), Some((at, _))) = (max_age, &seen)
        && now.saturating_sub(*at) < age.as_secs()
    {
        return Synced::Fresh;
    }
    match fetch(reg, &remote, now) {
        Ok(commit) => Synced::Fetched { commit },
        Err(error) if reg.dir.is_dir() => Synced::Stale {
            error,
            since: seen.map(|(at, _)| at),
        },
        Err(error) => Synced::Missing { error },
    }
}

/// `<registry>/.NAME-TAG`: a name never a registry's (names do not
/// begin with `.`).
fn beside(reg: &Registry, tag: &str) -> Result<PathBuf> {
    let parent = reg
        .dir
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::Registry, "registry: no parent directory"))?;
    Ok(parent.join(format!(".{}-{tag}", reg.name)))
}

/// Moves `new` to the registry's place; the old copy goes.
fn swap_in(reg: &Registry, new: &Path) -> Result<()> {
    let old = beside(reg, "old")?;
    let _ = std::fs::remove_dir_all(&old);
    if reg.dir.exists() {
        std::fs::rename(&reg.dir, &old).map_err(|e| Error::io(reg.dir.display(), e))?;
    }
    std::fs::rename(new, &reg.dir).map_err(|e| Error::io(reg.dir.display(), e))?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

fn fetch(reg: &Registry, remote: &Remote, now: u64) -> Result<String> {
    if let Remote::Registry(base) = remote {
        return fetch_api(reg, base, now);
    }
    let git = beside(reg, "git")?;
    let new = beside(reg, "new")?;
    let _ = std::fs::remove_dir_all(&git);
    let res = (|| {
        let repo = Repo::open(&git)?.max_fetch(MAX_REGISTRY_FETCH);
        let commit = repo.fetch(remote, &Reference::Head).map_err(|e| Error {
            message: format!("registry {}: {}", reg.name, e.message),
            ..e
        })?;
        let tree = repo.tree(&commit, None)?;
        export(&tree, &new)?;
        write_private(&new.join(STAMP), format!("{now} {commit}\n").as_bytes())?;
        swap_in(reg, &new)?;
        Ok(commit)
    })();
    let _ = std::fs::remove_dir_all(&git);
    let _ = std::fs::remove_dir_all(&new);
    res
}

/// Whether `path` of a registry is kept: `index.json`, or a `.toml`
/// under `apps/`, no component hidden, 18 deep at most.
fn kept(path: &str) -> bool {
    if path == "index.json" {
        return true;
    }
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 2
        && parts.len() <= 18
        && parts[0] == "apps"
        && path.ends_with(".toml")
        && parts.iter().all(|p| !p.starts_with('.'))
        && check_rel_path(path).is_ok()
}

/// Writes the pointer files and `index.json` of `tree` under `out`:
/// directories 0700, files 0600.
pub fn export(tree: &dyn Tree, out: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(out);
    mkdir(out)?;
    let mut nodes = tree.nodes()?;
    nodes.retain(|n| n.kind == Kind::File && kept(&n.path));
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    let too = |what: String| {
        Err(Error::new(ErrorKind::Registry, what).with_hint("a registry of pointer files only"))
    };
    if nodes.len() > MAX_FILES + 1 {
        return too(format!("more than {MAX_FILES} pointer files"));
    }
    let mut total = 0u64;
    for n in nodes {
        let max = match n.path.as_str() {
            "index.json" => MAX_INDEX,
            _ => MAX_POINTER,
        };
        if n.size > max {
            if n.path == "index.json" {
                continue;
            }
            return too(format!("{}: more than {max} bytes", n.path));
        }
        total += n.size;
        if total > MAX_EXPORT {
            return too(format!("pointer files: more than {MAX_EXPORT} bytes"));
        }
        let bytes = tree.read(&n)?;
        write_private(&out.join(&n.path), &bytes)?;
    }
    Ok(())
}

fn fetch_api(reg: &Registry, base: &str, now: u64) -> Result<String> {
    let host = parse_base(base)?;
    let new = beside(reg, "new")?;
    let res = (|| {
        let r = crate::http::get(base, "/v1/index.json", MAX_INDEX).map_err(|e| Error {
            message: format!("registry {}: {}", reg.name, e.message),
            ..e
        })?;
        let tag = crate::tar::sha256_hex(&r.body);
        let files = from_api(&host, &r.body).map_err(|e| Error {
            message: format!("registry {}: {}", reg.name, e.message),
            ..e
        })?;
        let _ = std::fs::remove_dir_all(&new);
        mkdir(&new)?;
        for (rel, bytes) in files {
            write_private(&new.join(rel), &bytes)?;
        }
        write_private(&new.join(STAMP), format!("{now} {tag}\n").as_bytes())?;
        swap_in(reg, &new)?;
        Ok(tag)
    })();
    let _ = std::fs::remove_dir_all(&new);
    res
}

#[derive(Deserialize)]
struct ApiIndex {
    format: u32,
    #[serde(default)]
    apps: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ApiApp {
    name: String,
    kind: String,
    #[serde(default)]
    yanked: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    manifest_path: Option<String>,
    #[serde(default)]
    versions: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ApiVersion {
    rev: String,
    tier: Tier,
    #[serde(default)]
    by: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    yanked: bool,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    permissions: Vec<ApiPermission>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    wasm: Option<String>,
}

#[derive(Deserialize)]
struct ApiPermission {
    name: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    words: Vec<String>,
}

/// Printable, at most `max` characters: for display only.
fn text(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect()
}

fn opt(s: &Option<String>, max: usize) -> Option<String> {
    s.as_deref().map(|s| text(s, max))
}

/// The files of a copy from a server's `index.json`: `(path, bytes)`.
fn from_api(host: &str, body: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let ix: ApiIndex = serde_json::from_slice(body)
        .map_err(|e| Error::new(ErrorKind::Registry, format!("{INDEX}: {e}")))?;
    if ix.format != 1 {
        return Err(Error::new(
            ErrorKind::Registry,
            format!("{INDEX}: format {}", ix.format),
        )
        .with_hint("a newer ptyrc reads it"));
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut apps = Vec::new();
    for v in ix.apps.into_iter().take(MAX_FILES) {
        let Ok(a) = serde_json::from_value::<ApiApp>(v) else {
            continue;
        };
        let Some((pointer, versions)) = api_pointer(host, &a) else {
            continue;
        };
        let Ok(origin) = pointer.0.origin() else {
            continue;
        };
        let Some(rel) = origin.registry_path() else {
            continue;
        };
        if pointer.1.len() as u64 > MAX_POINTER || !seen.insert(rel.clone()) {
            continue;
        }
        apps.push(index_app(&origin, &pointer.0, &a, &versions));
        out.push((rel, pointer.1.into_bytes()));
    }
    let index = Index {
        format: FORMAT,
        apps,
    }
    .to_json();
    if index.len() as u64 <= MAX_INDEX {
        out.push((INDEX.to_string(), index.into_bytes()));
    }
    Ok(out)
}

/// The pointer of an app of the index, and its text; its versions kept.
/// None: left out.
fn api_pointer(host: &str, a: &ApiApp) -> Option<((Pointer, String), Vec<ApiVersion>)> {
    let (url, manifest, oid) = match a.kind.as_str() {
        "tar" => {
            if !is_name(&a.name, 64) {
                return None;
            }
            let url = format!("{}/{}", base_of(host), a.name);
            if a.url.as_ref().is_some_and(|u| u != &url) {
                return None;
            }
            (url, MANIFEST.to_string(), 64)
        }
        "git" => (
            a.url.clone()?,
            a.manifest_path.clone().unwrap_or(MANIFEST.into()),
            40,
        ),
        _ => return None,
    };
    let versions: Vec<ApiVersion> = a
        .versions
        .iter()
        .filter_map(|v| serde_json::from_value::<ApiVersion>(v.clone()).ok())
        .filter(|v| v.rev.len() == oid && is_oid(&v.rev) && v.tier != Tier::Shipped)
        .collect();
    let q = |s: &str| toml::Value::String(s.to_string()).to_string();
    let mut src = format!("origin = {}\nmanifest = {}\n", q(&url), q(&manifest));
    if a.yanked {
        src.push_str("yanked = true\n");
    }
    for v in &versions {
        src.push_str(&format!(
            "\n[[version]]\ncommit = {}\ntier = {}\n",
            q(&v.rev),
            q(v.tier.as_str())
        ));
        if let Some(b) = &v.by {
            src.push_str(&format!("by = {}\n", q(&text(b, 64))));
        }
        if let Some(d) = v
            .date
            .as_deref()
            .and_then(|d| d.parse::<toml::value::Datetime>().ok())
        {
            src.push_str(&format!("date = {d}\n"));
        }
        if v.yanked {
            src.push_str("yanked = true\n");
        }
    }
    let pointer = Pointer::parse("index.json", &src).ok()?;
    let origin = pointer.origin().ok()?;
    let ok = match a.kind.as_str() {
        "tar" => origin.is_hosted() && origin.host() == host,
        _ => !origin.is_hosted() && !origin.is_local() && origin.id() == a.name,
    };
    ok.then_some(((pointer, src), versions))
}

/// The entry of `index.json`: the fields of the version `choose` picks.
fn index_app(origin: &Origin, p: &Pointer, a: &ApiApp, versions: &[ApiVersion]) -> IndexApp {
    let pick = choose(
        Some(p),
        &Want {
            force: true,
            ..Want::default()
        },
    )
    .ok();
    let picked = match &pick {
        Some(Pick {
            reference: Reference::Commit(c),
            ..
        }) => versions.iter().find(|v| &v.rev == c),
        _ => None,
    };
    let s = |f: &Option<String>| opt(f, 256);
    IndexApp {
        origin: origin.id(),
        url: p.origin.clone(),
        manifest: p.manifest.clone(),
        yanked: p.yanked,
        id: picked.and_then(|v| s(&v.id)),
        summary: picked.and_then(|v| s(&v.summary)),
        version: picked.and_then(|v| s(&v.version)),
        license: picked.and_then(|v| s(&v.license)),
        surface: picked.and_then(|v| s(&v.surface)),
        tape: None,
        commit: picked.map(|v| v.rev.clone()),
        tier: picked.map(|v| v.tier),
        permissions: picked
            .map(|v| {
                v.permissions
                    .iter()
                    .take(64)
                    .map(|m| IndexPermission {
                        name: text(&m.name, 64),
                        why: text(&m.why, 512),
                        colour: colour(&m.name),
                        words: m.words.iter().take(32).map(|w| text(w, 32)).collect(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        versions: versions
            .iter()
            .map(|v| IndexVersion {
                commit: v.rev.clone(),
                tier: v.tier,
                by: opt(&v.by, 64),
                date: v
                    .date
                    .as_deref()
                    .and_then(|d| d.parse::<toml::value::Datetime>().ok())
                    .map(|d| d.to_string()),
                yanked: v.yanked,
                version: s(&v.version),
                error: None,
                entry: v.entry.clone().filter(|e| e == "script" || e == "rust"),
                wasm: v.wasm.clone().filter(|w| w.len() == 64 && is_oid(w)),
                size: v.size,
            })
            .collect(),
        kind: Some(a.kind.clone()),
        description: opt(&a.description, 2000),
    }
}
