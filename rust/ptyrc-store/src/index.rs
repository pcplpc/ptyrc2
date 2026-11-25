// SPDX-License-Identifier: GPL-3.0-or-later
//! `index.json`: what a registry lists, with what each listed commit's
//! manifest says; built on merge, read by `store search` and `show` to
//! go fast. Never trusted: an install reads the manifest itself.

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::Repo;
use crate::install::Dirs;
use crate::manifest::{Colour, colour};
use crate::origin::{Origin, Reference, Remote};
use crate::registry::{Pick, Pointer, Registry, Tier, Want, choose};
use crate::tree::{Checked, check};

/// The format written.
pub const FORMAT: u32 = 1;
/// The file's name, at the registry's root.
pub const INDEX: &str = "index.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub format: u32,
    pub apps: Vec<IndexApp>,
}

/// One pointer file. The fields of the manifest are those of the version
/// the client would pick (`choose`); `None` when there is none, or its
/// manifest could not be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexApp {
    /// The identity: `host/owner/repo[#path]`.
    pub origin: String,
    pub url: String,
    pub manifest: String,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub tape: Option<String>,
    /// The commit the client would pick, and its tier.
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub tier: Option<Tier>,
    #[serde(default)]
    pub permissions: Vec<IndexPermission>,
    #[serde(default)]
    pub versions: Vec<IndexVersion>,
    /// A registry server's: `tar` or `git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A registry server's: what the pusher said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexPermission {
    pub name: String,
    pub why: String,
    pub colour: Colour,
    /// `words` only: the words declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexVersion {
    pub commit: String,
    pub tier: Tier,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub yanked: bool,
    /// `[app] version` at that commit.
    #[serde(default)]
    pub version: Option<String>,
    /// Why the manifest could not be read.
    #[serde(default)]
    pub error: Option<String>,
    /// A registry server's: `script` or `rust`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// A registry server's: the sha256 of the module it built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<String>,
    /// A registry server's: the archive's size, bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// Fetches `commit` of `origin` into `<cache>/ptyrc/git/` and checks its
/// tree, as the install does.
pub fn fetch_checked(
    dirs: &Dirs,
    origin: &Origin,
    remote: &Remote,
    commit: &str,
) -> Result<Checked> {
    let repo = Repo::open(&dirs.git(origin))?;
    let got = repo.fetch(remote, &Reference::Commit(commit.to_string()))?;
    check(&repo.tree(&got, origin.sub())?)
}

/// Builds the index of `reg`: every pointer, every listed commit's
/// manifest fetched with [`fetch_checked`]. `remote` says where an
/// origin is fetched from. A commit that cannot be read is an `error` of
/// its version, not a failure.
pub fn build(dirs: &Dirs, reg: &Registry, remote: &dyn Fn(&Origin) -> Remote) -> Result<Index> {
    let mut apps = Vec::new();
    for p in reg.pointers()? {
        apps.push(entry(dirs, &p, remote)?);
    }
    Ok(Index {
        format: FORMAT,
        apps,
    })
}

fn entry(dirs: &Dirs, p: &Pointer, remote: &dyn Fn(&Origin) -> Remote) -> Result<IndexApp> {
    let origin = p.origin()?;
    let r = remote(&origin);
    let mut app = IndexApp {
        origin: origin.id(),
        url: p.origin.clone(),
        manifest: p.manifest.clone(),
        yanked: p.yanked,
        id: None,
        summary: None,
        version: None,
        license: None,
        surface: None,
        tape: None,
        commit: None,
        tier: None,
        permissions: Vec::new(),
        versions: Vec::new(),
        kind: None,
        description: None,
    };
    let pick = choose(
        Some(p),
        &Want {
            force: true,
            ..Want::default()
        },
    )
    .ok();
    if let Some(Pick {
        reference: Reference::Commit(c),
        tier,
    }) = &pick
    {
        app.commit = Some(c.clone());
        app.tier = Some(*tier);
    }
    for v in &p.versions {
        let read = fetch_checked(dirs, &origin, &r, &v.commit);
        let mut iv = IndexVersion {
            commit: v.commit.clone(),
            tier: v.tier,
            by: v.by.clone(),
            date: v.date.map(|d| d.to_string()),
            yanked: v.yanked,
            version: None,
            error: None,
            entry: None,
            wasm: None,
            size: None,
        };
        match read {
            Ok(c) => {
                iv.version = Some(c.manifest.app.version.clone());
                let picked = pick
                    .as_ref()
                    .is_some_and(|k| k.reference == Reference::Commit(v.commit.clone()));
                if picked {
                    let a = &c.manifest.app;
                    app.id = Some(a.id.clone());
                    app.summary = Some(a.summary.clone());
                    app.version = Some(a.version.clone());
                    app.license = Some(a.license.clone());
                    app.surface = Some(a.surface.as_str().to_string());
                    app.tape = a.tape.clone();
                    app.permissions = c
                        .manifest
                        .permissions
                        .iter()
                        .map(|m| IndexPermission {
                            name: m.name.clone(),
                            why: m.why.clone(),
                            colour: colour(&m.name),
                            words: m.words.clone(),
                        })
                        .collect();
                }
            }
            Err(e) => iv.error = Some(e.message),
        }
        app.versions.push(iv);
    }
    Ok(app)
}

impl Index {
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("an index serialises");
        s.push('\n');
        s
    }

    /// Parses `index.json`; another `format`: an error.
    pub fn parse(src: &str) -> Result<Index> {
        let ix: Index = serde_json::from_str(src)
            .map_err(|e| Error::new(ErrorKind::Registry, format!("{INDEX}: {e}")))?;
        if ix.format != FORMAT {
            return Err(Error::new(
                ErrorKind::Registry,
                format!("{INDEX}: format {}", ix.format),
            ));
        }
        Ok(ix)
    }

    /// `<dir>/index.json` when present and readable; else none.
    pub fn load(reg: &Registry) -> Option<Index> {
        let src = std::fs::read_to_string(reg.dir.join(INDEX)).ok()?;
        Index::parse(&src).ok()
    }

    pub fn get(&self, origin: &str) -> Option<&IndexApp> {
        self.apps.iter().find(|a| a.origin == origin)
    }
}
