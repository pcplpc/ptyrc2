// SPDX-License-Identifier: GPL-3.0-or-later
//! Fetching a registry named by `url` in `store.toml`: its head, depth
//! 1, with the hardening of an app fetch ([`Repo`]); then its pointer
//! files and `index.json`, read with `git cat-file`, are written under
//! `<cache>/ptyrc/registry/<name>/`. Nothing is checked out; the bare
//! repository is removed after. No clock: `now` is an argument.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, ErrorKind, Result, check_rel_path};
use crate::git::Repo;
use crate::install::{mkdir, write_private};
use crate::origin::Reference;
use crate::registry::Registry;
use crate::tree::{Kind, Tree};

/// A copy older than this is fetched again by the implicit sync.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 3600);
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
/// Name of the stamp in the copy: `SECONDS COMMIT`.
pub const STAMP: &str = ".fetched";

/// What [`sync`] came to.
#[derive(Debug)]
pub enum Synced {
    /// A `path` registry: read as it is.
    Local,
    /// The copy is younger than `max_age`.
    Fresh,
    Fetched {
        commit: String,
    },
    /// The fetch failed; the copy of `since` (seconds) is used.
    Stale {
        error: Error,
        since: Option<u64>,
    },
    /// The fetch failed and there is no copy.
    Missing {
        error: Error,
    },
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

fn fetch(reg: &Registry, remote: &crate::origin::Remote, now: u64) -> Result<String> {
    let git = beside(reg, "git")?;
    let new = beside(reg, "new")?;
    let old = beside(reg, "old")?;
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
        let _ = std::fs::remove_dir_all(&old);
        if reg.dir.exists() {
            std::fs::rename(&reg.dir, &old).map_err(|e| Error::io(reg.dir.display(), e))?;
        }
        std::fs::rename(&new, &reg.dir).map_err(|e| Error::io(reg.dir.display(), e))?;
        let _ = std::fs::remove_dir_all(&old);
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
