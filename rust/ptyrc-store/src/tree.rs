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
    let licence = licence_file(&files);
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

/// The licence text the check reads: the first file of [`LICENSE_FILES`]
/// at the root, in the order of paths.
fn licence_file(files: &[Node]) -> Option<&Node> {
    files.iter().find(|n| {
        !n.path.contains('/')
            && LICENSE_FILES
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&n.path))
    })
}

/// Every file that is not text: valid UTF-8, no NUL byte. A registry
/// server's archive holds text only; a git tree and a shipped app are
/// not bound by it.
pub fn text_only(tree: &dyn Tree, checked: &Checked) -> Vec<Error> {
    let mut bad = Vec::new();
    for f in &checked.files {
        match tree.read(f) {
            Ok(b) if std::str::from_utf8(&b).is_ok() && !b.contains(&0) => {}
            Ok(_) => bad.push(tree_err(
                format!("{}: not text (UTF-8, no NUL byte)", f.path),
                "source and text only: no image, no binary",
            )),
            Err(e) => bad.push(e),
        }
    }
    bad
}

/// Files at the app root counted used: documentation, the tooling of
/// `ptyrc dev new`, cargo's lock.
pub const KEPT: &[&str] = &[
    "README.md",
    "README",
    "AGENTS.md",
    ".gitignore",
    "test.sh",
    "PROTOCOL.md",
    "ptyrc.d.ts",
    "Cargo.lock",
];

/// The files of a checked tree the app uses, sorted: the manifest; the
/// licence text the check reads; [`KEPT`]; under `[app] assets`; the
/// tape; under `tests/`; `vendor/ptyrc-app/` when it is ptyrc's own
/// ([`crate::build::vendored_differs`] empty). A script app: the entry,
/// `tests.js` and every `*.tests.js`, and each script these name,
/// transitively, by a quoted relative path (`"./x.js"`, `'../y.mjs'`).
/// A Rust app: `Cargo.toml`; `built`, the files `rustc` read
/// ([`crate::build::Built::sources`]); without it, every file under the
/// entry's directory (every `.rs` when the entry is at the root).
pub fn used(tree: &dyn Tree, checked: &Checked, built: Option<&[String]>) -> Result<Vec<String>> {
    use std::collections::BTreeSet;
    let files = &checked.files;
    let has = |p: &str| files.iter().any(|f| f.path == p);
    let mut out: BTreeSet<String> = BTreeSet::new();
    out.insert(MANIFEST.to_string());
    if let Some(l) = licence_file(files) {
        out.insert(l.path.clone());
    }
    out.extend(KEPT.iter().filter(|k| has(k)).map(|k| k.to_string()));
    let app = &checked.manifest.app;
    let under = |dir: &str| {
        let d = format!("{}/", dir.trim_end_matches('/'));
        (files.iter())
            .filter(move |f| f.path.starts_with(&d))
            .map(|f| f.path.clone())
    };
    if let Some(a) = &app.assets {
        out.extend(under(a));
    }
    if let Some(t) = &app.tape {
        out.insert(t.clone());
    }
    out.extend(under("tests"));
    if crate::build::vendored_differs(tree, checked).is_empty() {
        out.extend(under(crate::build::VENDORED));
    }
    match &app.entry {
        Entry::Script(main) => {
            let mut queue: Vec<String> = vec![main.clone()];
            queue.extend(
                (files.iter())
                    .map(|f| &f.path)
                    .filter(|p| {
                        let name = p.rsplit('/').next().unwrap_or(p);
                        name == "tests.js" || name.ends_with(".tests.js")
                    })
                    .cloned(),
            );
            let mut seen = BTreeSet::new();
            while let Some(p) = queue.pop() {
                if !seen.insert(p.clone()) {
                    continue;
                }
                let Some(node) = files.iter().find(|f| f.path == p) else {
                    continue;
                };
                let text = tree.read(node)?;
                let base = p.rsplit_once('/').map_or("", |(d, _)| d);
                for name in named_scripts(&String::from_utf8_lossy(&text), base) {
                    if has(&name) {
                        queue.push(name);
                    }
                }
            }
            out.extend(seen.into_iter().filter(|p| has(p)));
        }
        Entry::Rust(entry) => {
            if has("Cargo.toml") {
                out.insert("Cargo.toml".into());
            }
            match built {
                Some(read) => out.extend(read.iter().filter(|p| has(p)).cloned()),
                None => match entry.rsplit_once('/') {
                    Some((dir, _)) => out.extend(under(dir)),
                    None => out.extend(
                        (files.iter())
                            .filter(|f| f.path.ends_with(".rs"))
                            .map(|f| f.path.clone()),
                    ),
                },
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// The files of a checked tree [`used`] leaves out, sorted.
pub fn unused(tree: &dyn Tree, checked: &Checked, built: Option<&[String]>) -> Result<Vec<String>> {
    let used = used(tree, checked, built)?;
    Ok((checked.files.iter())
        .filter(|f| used.binary_search(&f.path).is_err())
        .map(|f| f.path.clone())
        .collect())
}

/// The scripts a text names by a quoted path starting `./` or `../`,
/// resolved from the directory `base`; those outside the tree dropped.
fn named_scripts(text: &str, base: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    for (i, &q) in b.iter().enumerate() {
        if !matches!(q, b'"' | b'\'' | b'`') {
            continue;
        }
        let rest = &text[i + 1..];
        if !(rest.starts_with("./") || rest.starts_with("../")) {
            continue;
        }
        let Some(end) = rest.find([q as char, '\n']) else {
            continue;
        };
        let spec = &rest[..end];
        if !(spec.ends_with(".js") || spec.ends_with(".mjs")) {
            continue;
        }
        let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
        let mut inside = true;
        for c in spec.split('/') {
            match c {
                "." | "" => {}
                ".." => inside &= parts.pop().is_some(),
                c => parts.push(c),
            }
        }
        if inside {
            out.push(parts.join("/"));
        }
    }
    out
}

/// A view of a tree: the files named, the directories named or holding
/// one; nothing else.
pub struct Subset<'a> {
    tree: &'a dyn Tree,
    keep: std::collections::BTreeSet<String>,
}

impl<'a> Subset<'a> {
    pub fn new(tree: &'a dyn Tree, keep: impl IntoIterator<Item = String>) -> Subset<'a> {
        Subset {
            tree,
            keep: keep.into_iter().collect(),
        }
    }
}

impl Tree for Subset<'_> {
    fn nodes(&self) -> Result<Vec<Node>> {
        let mut nodes = self.tree.nodes()?;
        nodes.retain(|n| match n.kind {
            Kind::File => self.keep.contains(&n.path),
            Kind::Dir => {
                let d = format!("{}/", n.path);
                self.keep.contains(&n.path) || self.keep.iter().any(|k| k.starts_with(&d))
            }
            _ => false,
        });
        Ok(nodes)
    }

    fn read(&self, node: &Node) -> Result<Vec<u8>> {
        if !self.keep.contains(&node.path) {
            return Err(tree_err(
                format!("{}: not a file of the view", node.path),
                "a file of the tree",
            ));
        }
        self.tree.read(node)
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
