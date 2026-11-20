// SPDX-License-Identifier: GPL-3.0-or-later
//! Registries: pointer files, `store.toml`, the choice of a version. See
//! `plan/store-model.md`, "Registry".

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::is_oid;
use crate::install::Dirs;
use crate::manifest::MANIFEST;
use crate::origin::{Origin, Reference};

/// A word on a commit. Ordered: `Untested < Tested < Approved <
/// Shipped`. `Shipped`: an app of ptyrc itself (`crate::shipped`),
/// never in a registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Untested,
    Tested,
    Approved,
    Shipped,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Untested => "untested",
            Tier::Tested => "tested",
            Tier::Approved => "approved",
            Tier::Shipped => "shipped",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One file `apps/<host>/<owner>/<repo>[/<path>].toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pointer {
    /// `https://host/owner/repo`.
    pub origin: String,
    /// The manifest's path in the repository.
    #[serde(default = "default_manifest")]
    pub manifest: String,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default, rename = "version")]
    pub versions: Vec<Version>,
}

fn default_manifest() -> String {
    MANIFEST.to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub commit: String,
    pub tier: Tier,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub date: Option<toml::value::Datetime>,
    #[serde(default)]
    pub yanked: bool,
}

fn reg_err(msg: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::new(ErrorKind::Registry, msg).with_hint(hint)
}

impl Pointer {
    /// Parses and validates a pointer file; `name` is for errors.
    pub fn parse(name: &str, src: &str) -> Result<Pointer> {
        let p: Pointer = toml::from_str(src).map_err(|e| {
            let line = e
                .span()
                .map(|s| crate::error::line_of(src, s.start))
                .unwrap_or(1);
            reg_err(
                format!("{name}:{line}: {}", e.message().trim()),
                "origin = \"https://...\", manifest = \"ptyrc-app.toml\", [[version]] commit, tier",
            )
        })?;
        p.origin()?;
        for v in &p.versions {
            if !is_oid(&v.commit) {
                return Err(reg_err(
                    format!("{name}: commit {:?}", v.commit),
                    "a full commit id, lower-case hex",
                ));
            }
            if v.tier == Tier::Shipped {
                return Err(reg_err(
                    format!("{name}: commit {}: tier shipped", v.commit),
                    "untested, tested or approved; shipped is ptyrc's own apps",
                ));
            }
        }
        Ok(p)
    }

    /// The origin it speaks for: `origin` and the directory of `manifest`.
    pub fn origin(&self) -> Result<Origin> {
        if !self.origin.starts_with("https://") || self.origin.contains('#') {
            return Err(reg_err(
                format!("origin {:?}: not an https URL", self.origin),
                "origin = \"https://host/owner/repo\"",
            ));
        }
        let sub = match self.manifest.strip_suffix(MANIFEST) {
            Some("") => None,
            Some(dir) if dir.ends_with('/') => Some(dir.trim_end_matches('/')),
            _ => {
                return Err(reg_err(
                    format!("manifest {:?}", self.manifest),
                    format!("a path ending in {MANIFEST}"),
                ));
            }
        };
        let spec = match sub {
            None => self.origin.clone(),
            Some(s) => format!("{}#{s}", self.origin),
        };
        Origin::parse(&spec).map_err(|e| Error {
            kind: ErrorKind::Registry,
            ..e
        })
    }

    /// The tier of `commit`; not listed: `Untested`. Yanked: refused
    /// unless `force`.
    pub fn tier_of(&self, commit: &str, force: bool) -> Result<Tier> {
        let v = self.versions.iter().find(|v| v.commit == commit);
        if (self.yanked || v.is_some_and(|v| v.yanked)) && !force {
            return Err(reg_err(
                format!("{commit}: yanked"),
                "another version, or --force",
            ));
        }
        Ok(v.map(|v| v.tier).unwrap_or(Tier::Untested))
    }
}

/// What the user asked of `store add` / `update`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Want {
    /// `@REF`.
    pub reference: Option<Reference>,
    /// `--latest`: the head, whatever is listed.
    pub latest: bool,
    /// `--force`: a yanked version is taken.
    pub force: bool,
}

/// What to fetch, and the tier known before the fetch. For a
/// [`Reference::Name`] or [`Reference::Head`], the tier of the commit
/// fetched is [`tier_of`]'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub reference: Reference,
    pub tier: Tier,
}

/// Chooses a version: `@REF` as given; else the listed version of the
/// best tier, the newest (by `date`, then the later in the file) among
/// equals; not listed, nothing but yanked versions, or `--latest`: the
/// head, `untested`.
pub fn choose(pointer: Option<&Pointer>, want: &Want) -> Result<Pick> {
    if let Some(p) = pointer
        && p.yanked
        && !want.force
    {
        return Err(reg_err(
            format!("{}: yanked", p.origin),
            "--force to install it anyway",
        ));
    }
    match &want.reference {
        Some(Reference::Commit(c)) => {
            let tier = tier_of(pointer, c, want.force)?;
            return Ok(Pick {
                reference: Reference::Commit(c.clone()),
                tier,
            });
        }
        Some(r) => {
            return Ok(Pick {
                reference: r.clone(),
                tier: Tier::Untested,
            });
        }
        None => {}
    }
    let head = Pick {
        reference: Reference::Head,
        tier: Tier::Untested,
    };
    if want.latest {
        return Ok(head);
    }
    let Some(p) = pointer else {
        return Ok(head);
    };
    let date = |v: &Version| v.date.map(|d| d.to_string()).unwrap_or_default();
    let best = p
        .versions
        .iter()
        .enumerate()
        .filter(|(_, v)| !v.yanked)
        .max_by(|(i, a), (j, b)| {
            a.tier
                .cmp(&b.tier)
                .then_with(|| date(a).cmp(&date(b)))
                .then_with(|| i.cmp(j))
        });
    Ok(match best {
        Some((_, v)) => Pick {
            reference: Reference::Commit(v.commit.clone()),
            tier: v.tier,
        },
        None => head,
    })
}

/// The tier of a fetched commit; no pointer: `Untested`.
pub fn tier_of(pointer: Option<&Pointer>, commit: &str, force: bool) -> Result<Tier> {
    match pointer {
        Some(p) => p.tier_of(commit, force),
        None => Ok(Tier::Untested),
    }
}

/// One registry: a directory holding `apps/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    pub name: String,
    pub dir: PathBuf,
    /// Where it is fetched from; none for a local directory.
    pub url: Option<String>,
}

/// What [`Registries::lookup_present`] found.
#[derive(Debug, Default)]
pub struct Present<'a> {
    pub found: Option<(&'a Registry, Pointer)>,
    pub absent: Vec<&'a Registry>,
}

/// Bound on the pointer files read by [`Registry::pointers`].
const MAX_POINTERS: usize = 100_000;

impl Registry {
    /// Where a registry by `url` is fetched from; none for a `path`.
    pub fn remote(&self) -> Option<crate::origin::Remote> {
        let u = self.url.as_deref()?;
        Some(match u.starts_with('/') {
            true => crate::origin::Remote::Local(PathBuf::from(u)),
            false => crate::origin::Remote::Https(u.to_string()),
        })
    }

    /// Every pointer file under `apps/`, sorted by path; symlinks are not
    /// followed. A file that does not parse, or does not speak for its
    /// place, is an error.
    pub fn pointers(&self) -> Result<Vec<Pointer>> {
        let mut rels = Vec::new();
        walk(&self.dir, "apps", 0, &mut rels)?;
        rels.sort();
        let mut out = Vec::new();
        for rel in rels {
            let p = self.dir.join(&rel);
            let src = std::fs::read_to_string(&p).map_err(|e| Error::io(p.display(), e))?;
            let pointer = Pointer::parse(&rel, &src)?;
            let o = pointer.origin()?;
            if o.registry_path().as_deref() != Some(rel.as_str()) {
                return Err(reg_err(
                    format!("registry {}: {rel}: speaks for {o}", self.name),
                    "the file's place and its origin and manifest must agree",
                ));
            }
            out.push(pointer);
        }
        Ok(out)
    }
}

/// The `.toml` files under `dir/rel`, as paths relative to `dir`.
fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<String>) -> Result<()> {
    let p = dir.join(rel);
    let rd = match std::fs::read_dir(&p) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && depth == 0 => return Ok(()),
        Err(e) => return Err(Error::io(p.display(), e)),
    };
    for ent in rd {
        let ent = ent.map_err(|e| Error::io(p.display(), e))?;
        let Some(name) = ent.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let ft = ent.file_type().map_err(|e| Error::io(p.display(), e))?;
        let r = format!("{rel}/{name}");
        if ft.is_dir() && depth < 16 && !name.starts_with('.') {
            walk(dir, &r, depth + 1, out)?;
        } else if ft.is_file() && name.ends_with(".toml") {
            if out.len() >= MAX_POINTERS {
                return Err(reg_err(
                    format!("{}: more than {MAX_POINTERS} pointer files", dir.display()),
                    "a registry of pointer files only",
                ));
            }
            out.push(r);
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    #[serde(default)]
    registry: Vec<RegistryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryEntry {
    name: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
}

/// The registries of `store.toml`, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registries {
    pub list: Vec<Registry>,
}

impl Registries {
    /// Reads `<config>/ptyrc/store.toml`; absent: no registry.
    pub fn load(dirs: &Dirs) -> Result<Registries> {
        let p = dirs.store();
        match std::fs::read_to_string(&p) {
            Ok(s) => Registries::parse(dirs, &s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Registries::default()),
            Err(e) => Err(Error::io(p.display(), e)),
        }
    }

    /// Parses `store.toml`: `[[registry]]` with `name` and one of `url`
    /// (https, or the absolute path of a git repository; fetched into
    /// `<cache>/ptyrc/registry/<name>/`) or `path` (a directory, read as
    /// it is).
    pub fn parse(dirs: &Dirs, src: &str) -> Result<Registries> {
        let hint = "[[registry]] name = \"main\", url = \"https://...\" or path = \"/dir\"";
        let f: StoreFile = toml::from_str(src).map_err(|e| {
            let line = e
                .span()
                .map(|s| crate::error::line_of(src, s.start))
                .unwrap_or(1);
            reg_err(format!("store.toml:{line}: {}", e.message().trim()), hint)
        })?;
        let mut list: Vec<Registry> = Vec::new();
        for r in f.registry {
            let ok = !r.name.is_empty()
                && r.name.len() <= 64
                && !r.name.starts_with(['.', '-'])
                && r.name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
            if !ok || list.iter().any(|l| l.name == r.name) {
                return Err(reg_err(
                    format!("store.toml: name {:?}", r.name),
                    "a unique name: letters, digits, `.`, `_`, `-`",
                ));
            }
            let (dir, url) = match (r.url, r.path) {
                (Some(u), None)
                    if (u.starts_with("https://") && !u.contains("::"))
                        || (u.starts_with('/') && !u.contains(':')) =>
                {
                    (dirs.registry().join(&r.name), Some(u))
                }
                (None, Some(p)) if p.is_absolute() => (p, None),
                _ => {
                    return Err(reg_err(
                        format!("store.toml: registry {}: url or path", r.name),
                        hint,
                    ));
                }
            };
            list.push(Registry {
                name: r.name,
                dir,
                url,
            });
        }
        Ok(Registries { list })
    }

    /// `store.toml` for these registries.
    pub fn to_toml(&self) -> String {
        let q = |s: &str| toml::Value::String(s.to_string()).to_string();
        let mut out = String::from(
            "# The registries, in order: the first that lists an origin speaks for it.\n\
             # url: https, or a git repository here (fetched); path: a directory (read as it is).\n",
        );
        for r in &self.list {
            out.push_str(&format!("\n[[registry]]\nname = {}\n", q(&r.name)));
            match &r.url {
                Some(u) => out.push_str(&format!("url = {}\n", q(u))),
                None => out.push_str(&format!("path = {}\n", q(&r.dir.to_string_lossy()))),
            }
        }
        out
    }

    /// Appends a registry: `url` (`path` false) or `path`, validated as
    /// `store.toml` would be.
    pub fn add(&mut self, dirs: &Dirs, name: &str, source: &str, path: bool) -> Result<()> {
        let q = |s: &str| toml::Value::String(s.to_string()).to_string();
        let key = if path { "path" } else { "url" };
        let one = format!("[[registry]]\nname = {}\n{key} = {}\n", q(name), q(source));
        let new = Registries::parse(dirs, &one)?;
        let mut all = self.clone();
        all.list.extend(new.list);
        Registries::parse(dirs, &all.to_toml())?;
        *self = all;
        Ok(())
    }

    /// Writes `<config>/ptyrc/store.toml`, 0600.
    pub fn save(&self, dirs: &Dirs) -> Result<()> {
        let text = self.to_toml();
        Registries::parse(dirs, &text)?;
        crate::install::write_private(&dirs.store(), text.as_bytes())
    }

    /// The first registry present here that lists `origin`, and its
    /// pointer; and the registries absent here before it, which might
    /// have spoken first.
    pub fn lookup_present(&self, origin: &Origin) -> Result<Present<'_>> {
        let mut out = Present::default();
        let Some(rel) = origin.registry_path() else {
            return Ok(out);
        };
        for r in &self.list {
            if !r.dir.is_dir() {
                out.absent.push(r);
                continue;
            }
            if let Some(p) = read_pointer(&r.dir, &rel, origin)? {
                out.found = Some((r, p));
                break;
            }
        }
        Ok(out)
    }

    /// The first registry that lists `origin`, and its pointer; a
    /// registry absent here is an error.
    pub fn lookup(&self, origin: &Origin) -> Result<Option<(&Registry, Pointer)>> {
        let Some(rel) = origin.registry_path() else {
            return Ok(None);
        };
        for r in &self.list {
            if !r.dir.is_dir() {
                return Err(reg_err(
                    format!("registry {}: {} absent", r.name, r.dir.display()),
                    "fetch the registry first, or remove it from store.toml",
                ));
            }
            if let Some(p) = read_pointer(&r.dir, &rel, origin)? {
                return Ok(Some((r, p)));
            }
        }
        Ok(None)
    }
}

/// Reads `dir/rel` when present; its origin must be `origin`.
pub fn read_pointer(dir: &Path, rel: &str, origin: &Origin) -> Result<Option<Pointer>> {
    let p = dir.join(rel);
    let meta = match std::fs::symlink_metadata(&p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::io(p.display(), e)),
    };
    if !meta.is_file() || meta.len() > 1 << 20 {
        return Err(reg_err(
            format!("{rel}: not a regular file of 1 MiB at most"),
            "a pointer file",
        ));
    }
    let src = std::fs::read_to_string(&p).map_err(|e| Error::io(p.display(), e))?;
    let pointer = Pointer::parse(rel, &src)?;
    let o = pointer.origin()?;
    if &o != origin {
        return Err(reg_err(
            format!("{rel}: speaks for {o}, not {origin}"),
            "the file's place and its origin and manifest must agree",
        ));
    }
    Ok(Some(pointer))
}
