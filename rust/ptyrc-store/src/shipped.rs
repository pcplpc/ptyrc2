// SPDX-License-Identifier: GPL-3.0-or-later
//! The store apps shipped with ptyrc: a tree each, `NAME/` of one
//! directory ([`root`]), origin `ptyrc/NAME`, tier `shipped`. Found
//! without `store add`; never fetched, updated or removed by the store.
//! A Rust app runs its `app.wasm`: beside the tree when installed, else
//! built from the tree into `<cache>/ptyrc/shipped/NAME/` ([`build`]).

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::build;
use crate::error::{Error, ErrorKind, Result};
use crate::install::{Dirs, Installed, mkdir, write_private};
use crate::manifest::{self, Entry, MANIFEST, Manifest};
use crate::registry::Tier;
use crate::tree::{DirTree, Tree, check};

/// The origin of a shipped app: `ptyrc/NAME`. Not an origin of
/// [`crate::Origin`]: none can be added, nor collide with one.
pub const PREFIX: &str = "ptyrc/";
/// Names the directory of the trees, in place of [`root`]'s search.
pub const ENV: &str = "PTYRC_STORE_APPS";
/// The trees in a checkout of ptyrc.
pub const CHECKOUT: &str = "plugins/store-apps";
/// The stamp of a build: what it was built from.
const STAMP: &str = ".ptyrc-source";

/// A shipped tree.
#[derive(Debug, Clone)]
pub struct Shipped {
    /// The directory name, `[app] id`.
    pub name: String,
    /// The tree.
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// `ptyrc/NAME`.
pub fn origin(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// `NAME` of `ptyrc/NAME`.
pub fn name_of(origin: &str) -> Option<&str> {
    origin.strip_prefix(PREFIX).filter(|n| !n.is_empty())
}

/// The directory of the shipped trees ([`Dirs::shipped`]), the first
/// of: `$`[`ENV`] (absolute; not a directory: none); [`CHECKOUT`] in the
/// first ancestor of this program's directory that has it (a checkout:
/// `target/debug/ptyrc`); `<data>/ptyrc/shipped/` (an installed ptyrc).
pub fn root(data: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV).map(PathBuf::from) {
        return (p.is_absolute() && p.is_dir()).then_some(p);
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let checkout = exe.ancestors().skip(1).map(|a| a.join(CHECKOUT));
    checkout
        .chain([data.join("ptyrc/shipped")])
        .find(|p| p.is_dir())
}

fn is_name(n: &str) -> bool {
    n.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && n.len() <= 64
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The tree `root/NAME`: its manifest, its id `NAME`.
pub fn read(root: &Path, name: &str) -> Result<Shipped> {
    let dir = root.join(name);
    let path = dir.join(MANIFEST);
    let bytes = std::fs::read(&path).map_err(|e| Error::io(path.display(), e))?;
    let manifest = manifest::parse_bytes(&bytes).map_err(|e| Error {
        message: format!("{}: {}", origin(name), e.message),
        ..e
    })?;
    if manifest.app.id != name {
        return Err(Error::new(
            ErrorKind::Install,
            format!("{}: [app] id = {:?}", origin(name), manifest.app.id),
        )
        .with_hint(format!(
            "a shipped app's id is its directory: id = {name:?}"
        )));
    }
    Ok(Shipped {
        name: name.to_string(),
        dir,
        manifest,
    })
}

/// Every shipped tree, sorted; a directory without a manifest is not
/// one; a manifest that does not read: an error of the list.
pub fn list(dirs: &Dirs) -> Vec<Result<Shipped>> {
    let Some(root) = &dirs.shipped else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = (rd.flatten())
        .filter(|e| e.path().join(MANIFEST).is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_name(n))
        .collect();
    names.sort();
    names.iter().map(|n| read(root, n)).collect()
}

/// The shipped app `name` (`NAME` or `ptyrc/NAME`); `None`: none such.
pub fn get(dirs: &Dirs, name: &str) -> Option<Result<Shipped>> {
    let name = name_of(name).unwrap_or(name);
    let root = dirs.shipped.as_ref()?;
    (is_name(name) && root.join(name).join(MANIFEST).is_file()).then(|| read(root, name))
}

impl Shipped {
    pub fn origin(&self) -> String {
        origin(&self.name)
    }

    /// As an entry of `apps.lock`: tier `shipped`, the commit ptyrc's
    /// version, started already (nothing to prune).
    pub fn installed(&self) -> Installed {
        Installed {
            origin: self.origin(),
            id: self.name.clone(),
            version: self.manifest.app.version.clone(),
            commit: env!("CARGO_PKG_VERSION").to_string(),
            tier: Tier::Shipped,
            date: String::new(),
            previous: None,
            started: true,
        }
    }

    pub fn is_rust(&self) -> bool {
        matches!(self.manifest.app.entry, Entry::Rust(_))
    }

    /// `<cache>/ptyrc/shipped/NAME/`: a Rust app built here.
    pub fn cache(&self, dirs: &Dirs) -> PathBuf {
        dirs.cache.join("ptyrc/shipped").join(&self.name)
    }

    /// The directory to start: the tree of a script; of a Rust app, the
    /// tree when `app.wasm` is in it, else its build when it is of the
    /// tree as it is ([`build`]).
    pub fn tree(&self, dirs: &Dirs) -> Result<PathBuf> {
        if !self.is_rust() || self.dir.join(build::MODULE).is_file() {
            return Ok(self.dir.clone());
        }
        let built = self.cache(dirs).join("tree");
        let fresh = (std::fs::read_to_string(self.cache(dirs).join(STAMP)).ok())
            .is_some_and(|s| Some(s) == self.source().ok());
        match fresh && built.join(build::MODULE).is_file() {
            true => Ok(built),
            false => Err(
                Error::new(ErrorKind::Build, format!("{}: not built", self.origin()))
                    .with_hint(format!("ptyrc app {} builds it", self.origin())),
            ),
        }
    }

    /// What a build is of: the files of the tree, `ptyrc-app`, ptyrc's
    /// version; a hash, as text.
    fn source(&self) -> Result<String> {
        let tree = DirTree::new(&self.dir);
        let checked = check(&tree)?;
        let mut h = std::hash::DefaultHasher::new();
        env!("CARGO_PKG_VERSION").hash(&mut h);
        build::PTYRC_APP.hash(&mut h);
        for f in &checked.files {
            f.path.hash(&mut h);
            tree.read(f)?.hash(&mut h);
        }
        Ok(format!("{:016x}\n", h.finish()))
    }

    /// A Rust app not built beside its tree: built by `rustc`
    /// ([`build::build`], `unconfined`: ptyrc's own sources) into
    /// [`Shipped::cache`] with a copy of the tree, unless built already
    /// from the tree as it is. Returns the directory to start. `on_build`
    /// is called before a build.
    pub fn build(&self, dirs: &Dirs, on_build: impl FnOnce(&build::Toolchain)) -> Result<PathBuf> {
        if let Ok(dir) = self.tree(dirs) {
            return Ok(dir);
        }
        let tree = DirTree::new(&self.dir);
        let checked = check(&tree)?;
        let source = self.source()?;
        let toolchain = build::Toolchain::find()?;
        on_build(&toolchain);
        let cache = self.cache(dirs);
        let work = cache.join("work");
        let options = build::Options {
            unconfined: true,
            ..build::Options::default()
        };
        let built = build::build(&tree, &checked, &work, &toolchain, &options);
        let _ = std::fs::remove_dir_all(&work);
        let built = built?;
        let next = cache.join("tree.new");
        let _ = std::fs::remove_dir_all(&next);
        mkdir(&next)?;
        for f in &checked.files {
            write_private(&next.join(&f.path), &tree.read(f)?)?;
        }
        write_private(&next.join(build::MODULE), &built.wasm)?;
        let dest = cache.join("tree");
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&next, &dest).map_err(|e| Error::io(dest.display(), e))?;
        write_private(&cache.join(STAMP), source.as_bytes())?;
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_origins() {
        assert_eq!(origin("sand"), "ptyrc/sand");
        assert_eq!(name_of("ptyrc/sand"), Some("sand"));
        assert_eq!(name_of("ptyrc/"), None);
        assert_eq!(name_of("example.org/a/sand"), None);
        // Never a store origin: none can be added under that name.
        assert!(crate::Origin::parse("ptyrc/sand").is_err());
        assert!(is_name("sand-2") && !is_name("Sand") && !is_name("2d") && !is_name(".x"));
    }
}
