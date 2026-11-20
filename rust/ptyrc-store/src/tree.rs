// SPDX-License-Identifier: GPL-3.0-or-later
//! An app tree, from git or a directory, and the check run by
//! `ptyrc dev check` and the install.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind, Result, check_rel_path};
use crate::manifest::{self, Entry, MANIFEST, MAX_MANIFEST, Manifest};

/// Bound on the tree, bytes of regular files.
pub const MAX_TREE: u64 = 8 << 20;
/// Bound on the number of regular files.
pub const MAX_FILES: usize = 512;
/// Bound on one script (`.js`), bytes.
pub const MAX_SCRIPT: u64 = 1 << 20;
/// File names taken as the licence text, case ignored.
pub const LICENSE_FILES: &[&str] = &[
    "COPYING",
    "COPYING.md",
    "COPYING.txt",
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Submodule,
    /// A device, a socket, a fifo, an unknown git mode.
    Other,
}

/// One entry of a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Relative to the app root, `/`-separated.
    pub path: String,
    pub kind: Kind,
    /// Bytes; 0 but for files.
    pub size: u64,
    /// The git object id; empty for a directory tree.
    pub id: String,
}

/// A source of an app tree.
pub trait Tree {
    /// Every entry, directories included, in no set order.
    fn nodes(&self) -> Result<Vec<Node>>;
    /// The content of a file node; at most `node.size` bytes are accepted.
    fn read(&self, node: &Node) -> Result<Vec<u8>>;
}

/// A tree that passed [`check`].
#[derive(Debug, Clone)]
pub struct Checked {
    pub manifest: Manifest,
    /// Regular files, sorted by path.
    pub files: Vec<Node>,
    /// Directories, sorted by path.
    pub dirs: Vec<Node>,
    /// Sum of file sizes.
    pub size: u64,
}

fn tree_err(msg: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::new(ErrorKind::Tree, msg).with_hint(hint)
}

/// Checks a tree: names, kinds, bounds, the manifest, the licence text,
/// the entry, `assets` and `tape`. Rust apps pass; their build is not
/// checked here. The first problem found; [`check_all`] for every one.
pub fn check(tree: &dyn Tree) -> Result<Checked> {
    check_all(tree).map_err(|mut v| v.remove(0))
}

/// [`check`], every problem: none is skipped for an earlier one, but
/// what needs the manifest is not checked without it. The list is never
/// empty.
pub fn check_all(tree: &dyn Tree) -> std::result::Result<Checked, Vec<Error>> {
    let mut nodes = tree.nodes().map_err(|e| vec![e])?;
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    let mut bad = Vec::new();
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut size = 0u64;
    for n in nodes {
        if let Err(why) = check_rel_path(&n.path) {
            bad.push(tree_err(
                format!("{:?}: a path with {why}", n.path),
                "plain relative names; no `..`, `.git`, backslash or control character",
            ));
            continue;
        }
        match n.kind {
            Kind::File => {
                size += n.size;
                if n.path.ends_with(".js") && n.size > MAX_SCRIPT {
                    bad.push(tree_err(
                        format!("{}: {} bytes, more than {MAX_SCRIPT}", n.path, n.size),
                        "a script of 1 MiB at most; split it into modules",
                    ));
                }
                files.push(n);
            }
            Kind::Dir => dirs.push(n),
            Kind::Symlink => bad.push(tree_err(
                format!("{}: a symlink", n.path),
                "regular files only: put a copy in place of the link",
            )),
            Kind::Submodule => bad.push(tree_err(
                format!("{}: a submodule", n.path),
                "regular files only: copy the files into the tree",
            )),
            Kind::Other => bad.push(tree_err(
                format!("{}: not a regular file", n.path),
                "regular files and directories only",
            )),
        }
    }
    if files.len() > MAX_FILES {
        bad.push(tree_err(
            format!("{} files, more than {MAX_FILES}", files.len()),
            "512 files at most in an app tree",
        ));
    }
    if size > MAX_TREE {
        bad.push(tree_err(
            format!("{size} bytes, more than {MAX_TREE}"),
            "8 MiB at most in an app tree",
        ));
    }
    let file = |p: &str| files.iter().find(|n| n.path == p);
    let manifest = match file(MANIFEST) {
        None => {
            bad.push(tree_err(
                format!("no {MANIFEST} at the app root"),
                "the manifest at the root, or the origin says where: host/owner/repo#path",
            ));
            None
        }
        Some(m) if m.size > MAX_MANIFEST as u64 => {
            bad.push(tree_err(
                format!("{MANIFEST}: {} bytes, more than {MAX_MANIFEST}", m.size),
                "a manifest of 64 KiB at most",
            ));
            None
        }
        Some(m) => match tree.read(m).and_then(|b| manifest::parse_bytes(&b)) {
            Ok(m) => Some(m),
            Err(e) => {
                bad.push(e);
                None
            }
        },
    };
    let licence = files.iter().find(|n| {
        !n.path.contains('/')
            && LICENSE_FILES
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&n.path))
    });
    let gpl = |text: &[u8]| {
        let t = String::from_utf8_lossy(text);
        t.contains("GNU GENERAL PUBLIC LICENSE") && t.contains("Version 3")
    };
    match licence.map(|n| tree.read(n)) {
        Some(Ok(text)) if gpl(&text) => {}
        Some(Err(e)) => bad.push(e),
        _ => bad.push(tree_err(
            "no text of the GPL version 3 at the app root",
            "the text of https://www.gnu.org/licenses/gpl-3.0.txt as LICENSE or COPYING",
        )),
    }
    if let Some(manifest) = &manifest {
        let entry = manifest.app.entry.path();
        if file(entry).is_none() {
            let key = match manifest.app.entry {
                Entry::Script(_) => "script",
                Entry::Rust(_) => "rust",
            };
            bad.push(tree_err(
                format!("{entry}: named by [app] {key}, not a file of the tree"),
                format!("add {entry}, or name the file that is: {key} = \"...\""),
            ));
        }
        if let Some(a) = &manifest.app.assets
            && !dirs.iter().any(|d| &d.path == a)
        {
            bad.push(tree_err(
                format!("{a}/: named by [app] assets, not a directory of the tree"),
                "add the directory, or remove assets",
            ));
        }
        if let Some(a) = &manifest.app.assets {
            let under = format!("{a}/");
            for n in files.iter().filter(|n| n.path.starts_with(&under)) {
                let name = &n.path[under.len()..];
                if !asset_name(name) {
                    bad.push(tree_err(
                        format!("{}: not a name of an asset, never answered", n.path),
                        ASSET_RULE,
                    ));
                }
            }
        }
        if let Some(t) = &manifest.app.tape
            && file(t).is_none()
        {
            bad.push(tree_err(
                format!("{t}: named by [app] tape, not a file of the tree"),
                "add the tape, or remove tape",
            ));
        }
    }
    match manifest {
        Some(manifest) if bad.is_empty() => Ok(Checked {
            manifest,
            files,
            dirs,
            size,
        }),
        _ => Err(bad),
    }
}

/// The rule of an asset's name, as the runner keeps it (`ptyrc-box`,
/// `assets::valid_name`).
pub const ASSET_RULE: &str = "under [app] assets: components of [A-Za-z0-9._-], 1 to 64 bytes, \
    none starting with `.`, `/` between them, 4 at most";

/// A name under `[app] assets` the runner answers.
pub fn asset_name(name: &str) -> bool {
    let part = |c: &str| {
        (1..=64).contains(&c.len())
            && !c.starts_with('.')
            && c.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    name.split('/').count() <= 4 && name.split('/').all(part)
}

/// A plain directory (`ptyrc dev check DIR`). Skipped at its root: `.git`,
/// and a cache directory (one holding `CACHEDIR.TAG`, as cargo's
/// `target/`): neither is committed.
#[derive(Debug, Clone)]
pub struct DirTree {
    root: PathBuf,
}

impl DirTree {
    pub fn new(root: impl Into<PathBuf>) -> DirTree {
        DirTree { root: root.into() }
    }

    fn walk(&self, rel: &str, dir: &Path, out: &mut Vec<Node>) -> Result<()> {
        let rd = std::fs::read_dir(dir).map_err(|e| Error::io(dir.display(), e))?;
        for ent in rd {
            let ent = ent.map_err(|e| Error::io(dir.display(), e))?;
            let name = ent.file_name();
            let Some(name) = name.to_str() else {
                return Err(tree_err(
                    format!("{rel}{}: not UTF-8", name.to_string_lossy()),
                    "file names in UTF-8",
                ));
            };
            if rel.is_empty() && (name == ".git" || ent.path().join("CACHEDIR.TAG").is_file()) {
                continue;
            }
            let path = format!("{rel}{name}");
            if out.len() > 4 * MAX_FILES {
                return Err(tree_err(
                    format!("more than {} entries", 4 * MAX_FILES),
                    "512 files at most in an app tree",
                ));
            }
            let meta = ent
                .metadata()
                .map_err(|e| Error::io(ent.path().display(), e))?;
            let ft = meta.file_type();
            let (kind, size) = if ft.is_symlink() {
                (Kind::Symlink, 0)
            } else if ft.is_dir() {
                (Kind::Dir, 0)
            } else if ft.is_file() {
                (Kind::File, meta.len())
            } else {
                (Kind::Other, 0)
            };
            out.push(Node {
                path: path.clone(),
                kind,
                size,
                id: String::new(),
            });
            if kind == Kind::Dir {
                self.walk(&format!("{path}/"), &ent.path(), out)?;
            }
        }
        Ok(())
    }
}

impl Tree for DirTree {
    fn nodes(&self) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        self.walk("", &self.root, &mut out)?;
        Ok(out)
    }

    fn read(&self, node: &Node) -> Result<Vec<u8>> {
        check_rel_path(&node.path)
            .map_err(|w| tree_err(format!("{:?}: {w}", node.path), "a relative path"))?;
        let p = self.root.join(&node.path);
        let meta = std::fs::symlink_metadata(&p).map_err(|e| Error::io(p.display(), e))?;
        if !meta.file_type().is_file() {
            return Err(tree_err(
                format!("{}: not a regular file", node.path),
                "regular files only",
            ));
        }
        let f = std::fs::File::open(&p).map_err(|e| Error::io(p.display(), e))?;
        let mut buf = Vec::new();
        f.take(node.size + 1)
            .read_to_end(&mut buf)
            .map_err(|e| Error::io(p.display(), e))?;
        if buf.len() as u64 != node.size {
            return Err(tree_err(
                format!("{}: changed while read", node.path),
                "check again",
            ));
        }
        Ok(buf)
    }
}
