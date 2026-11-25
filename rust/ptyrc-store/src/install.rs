// SPDX-License-Identifier: GPL-3.0-or-later
//! Installed trees and `apps.lock`. Directories 0700, files 0400 (the
//! lock 0600). See `plan/store-model.md`, "Install".

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::is_oid;
use crate::origin::Origin;
use crate::registry::Tier;
use crate::shipped::Shipped;
use crate::tree::{Checked, Tree};

/// The base directories; each path below appends `ptyrc/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub config: PathBuf,
    /// The shipped trees ([`crate::shipped::root`]); none: no shipped app.
    pub shipped: Option<PathBuf>,
}

impl Dirs {
    /// From `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`,
    /// `XDG_CONFIG_HOME` (absolute values only), else under `HOME`.
    pub fn from_env() -> Result<Dirs> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let pick = |var: &str, under: &str| -> Result<PathBuf> {
            if let Some(v) = std::env::var_os(var).map(PathBuf::from)
                && v.is_absolute()
            {
                return Ok(v);
            }
            home.as_ref().map(|h| h.join(under)).ok_or_else(|| {
                Error::new(
                    ErrorKind::Install,
                    format!("neither {var} nor HOME is an absolute path"),
                )
                .with_hint("set HOME")
            })
        };
        let data = pick("XDG_DATA_HOME", ".local/share")?;
        Ok(Dirs {
            shipped: crate::shipped::root(&data),
            data,
            state: pick("XDG_STATE_HOME", ".local/state")?,
            cache: pick("XDG_CACHE_HOME", ".cache")?,
            config: pick("XDG_CONFIG_HOME", ".config")?,
        })
    }

    /// All under one root: `root/{data,state,cache,config,shipped}`.
    pub fn under(root: &Path) -> Dirs {
        Dirs {
            data: root.join("data"),
            state: root.join("state"),
            cache: root.join("cache"),
            config: root.join("config"),
            shipped: Some(root.join("shipped")),
        }
    }

    /// `<data>/ptyrc/apps/`.
    pub fn apps(&self) -> PathBuf {
        self.data.join("ptyrc/apps")
    }

    /// `<data>/ptyrc/apps/<origin>/`: every installed commit of it.
    pub fn app(&self, origin: &Origin) -> PathBuf {
        self.apps().join(origin.dir_name())
    }

    /// `<data>/ptyrc/apps/<origin>/<commit>/`.
    pub fn tree(&self, origin: &Origin, commit: &str) -> PathBuf {
        self.app(origin).join(commit)
    }

    /// `<data>/ptyrc/apps.lock`.
    pub fn lock(&self) -> PathBuf {
        self.data.join("ptyrc/apps.lock")
    }

    /// `<data>/ptyrc/store.flock`: held by [`StoreLock`].
    pub fn store_lock(&self) -> PathBuf {
        self.data.join("ptyrc/store.flock")
    }

    /// `<state>/ptyrc/storage/<origin>/`.
    pub fn storage(&self, origin: &Origin) -> PathBuf {
        self.state.join("ptyrc/storage").join(origin.dir_name())
    }

    /// `<cache>/ptyrc/registry/`.
    pub fn registry(&self) -> PathBuf {
        self.cache.join("ptyrc/registry")
    }

    /// `<cache>/ptyrc/git/<origin>.git`: the bare repository of fetches.
    pub fn git(&self, origin: &Origin) -> PathBuf {
        self.cache
            .join("ptyrc/git")
            .join(format!("{}.git", origin.dir_name()))
    }

    /// `<config>/ptyrc/permissions.toml`: the grants.
    pub fn permissions(&self) -> PathBuf {
        self.config.join("ptyrc/permissions.toml")
    }

    /// `<config>/ptyrc/tokens.toml`: the tokens of pushed names.
    pub fn tokens(&self) -> PathBuf {
        self.config.join("ptyrc/tokens.toml")
    }

    /// `<config>/ptyrc/store.toml`: the registries, in order.
    pub fn store(&self) -> PathBuf {
        self.config.join("ptyrc/store.toml")
    }
}

/// Creates a directory and its missing parents, 0700.
pub(crate) fn mkdir(p: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
        .map_err(|e| Error::io(p.display(), e))
}

/// One writer at a time of the trees and `apps.lock`: an exclusive
/// `flock` on [`Dirs::store_lock`], released on drop. Every call of this
/// module that writes expects its caller to hold it. Not reentrant: a
/// second [`StoreLock::take`] in the same process waits for the first.
#[derive(Debug)]
pub struct StoreLock {
    _file: std::fs::File,
}

impl StoreLock {
    /// Takes the lock; `on_wait` is called once when another holds it,
    /// before waiting for it.
    pub fn take(dirs: &Dirs, on_wait: impl FnOnce()) -> Result<StoreLock> {
        let p = dirs.store_lock();
        mkdir(p.parent().expect("lock has a parent"))?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&p)
            .map_err(|e| Error::io(p.display(), e))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                on_wait();
                file.lock().map_err(|e| Error::io(p.display(), e))?;
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(Error::io(p.display(), e)),
        }
        Ok(StoreLock { _file: file })
    }
}

/// Writes `bytes` to `p`, 0600, by a rename.
pub(crate) fn write_private(p: &Path, bytes: &[u8]) -> Result<()> {
    let dir = p.parent().expect("a file has a parent");
    mkdir(dir)?;
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.tmp"));
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| Error::io(tmp.display(), e))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| Error::io(tmp.display(), e))?;
    std::fs::rename(&tmp, p).map_err(|e| Error::io(p.display(), e))
}

/// One installed app: the commit in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installed {
    /// [`Origin::id`].
    pub origin: String,
    pub id: String,
    pub version: String,
    pub commit: String,
    pub tier: Tier,
    /// `YYYY-MM-DD`, given at install.
    pub date: String,
    /// The commit in use before, kept on disk until this one has
    /// started once ([`started`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// This commit has started once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub started: bool,
}

/// `apps.lock`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    #[serde(default, rename = "app")]
    pub apps: Vec<Installed>,
}

impl Lock {
    /// Reads the lock; absent: empty.
    pub fn load(dirs: &Dirs) -> Result<Lock> {
        let p = dirs.lock();
        let src = match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Lock::default()),
            Err(e) => return Err(Error::io(p.display(), e)),
        };
        toml::from_str(&src).map_err(|e| {
            Error::new(
                ErrorKind::Install,
                format!("{}: {}", p.display(), e.message()),
            )
            .with_hint("written by ptyrc alone; remove it to start over, then add the apps again")
        })
    }

    /// Writes the lock, 0600, by a rename.
    pub fn save(&self, dirs: &Dirs) -> Result<()> {
        let text = toml::to_string(self)
            .map_err(|e| Error::new(ErrorKind::Install, format!("apps.lock: {e}")))?;
        write_private(&dirs.lock(), text.as_bytes())
    }

    pub fn get(&self, origin: &str) -> Option<&Installed> {
        self.apps.iter().find(|a| a.origin == origin)
    }

    /// An origin, else the one app of that `id`.
    pub fn resolve(&self, name: &str) -> Result<&Installed> {
        if let Some(a) = self.get(name) {
            return Ok(a);
        }
        if let Ok(o) = Origin::parse(name)
            && let Some(a) = self.get(&o.id())
        {
            return Ok(a);
        }
        let by_id: Vec<&Installed> = self.apps.iter().filter(|a| a.id == name).collect();
        match by_id[..] {
            [a] => Ok(a),
            [] => Err(
                Error::new(ErrorKind::Install, format!("{name}: not installed"))
                    .with_hint("ptyrc store ls"),
            ),
            _ => Err(Error::new(
                ErrorKind::Install,
                format!("{name}: the id of several apps"),
            )
            .with_hint(format!(
                "name the origin: {}",
                by_id
                    .iter()
                    .map(|a| a.origin.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}

/// `YYYY-MM-DD`.
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// Copies a checked tree to `<data>/ptyrc/apps/<origin>/<commit>/` and
/// records it in the lock as the commit in use. A tree already there is
/// kept; earlier commits stay until [`prune`]. The commit in use before,
/// if it had started, becomes `previous`; if not, `previous` is kept.
/// A script app; a Rust app: [`install_built`].
pub fn install(
    dirs: &Dirs,
    origin: &Origin,
    tree: &dyn Tree,
    checked: &Checked,
    commit: &str,
    tier: Tier,
    date: &str,
) -> Result<Installed> {
    install_built(dirs, origin, tree, checked, None, commit, tier, date)
}

/// [`install`], and for a Rust app its module (`build::build`), written
/// as `app.wasm` at the root of the tree.
#[allow(clippy::too_many_arguments)]
pub fn install_built(
    dirs: &Dirs,
    origin: &Origin,
    tree: &dyn Tree,
    checked: &Checked,
    module: Option<&[u8]>,
    commit: &str,
    tier: Tier,
    date: &str,
) -> Result<Installed> {
    if !is_oid(commit) {
        return Err(Error::new(
            ErrorKind::Install,
            format!("{commit:?}: not a commit id"),
        ));
    }
    if !is_date(date) {
        return Err(
            Error::new(ErrorKind::Install, format!("{date:?}: not a date")).with_hint("YYYY-MM-DD"),
        );
    }
    let todos = crate::manifest::todos(&checked.manifest);
    if !todos.is_empty() {
        return Err(Error::new(
            ErrorKind::Install,
            format!("ptyrc-app.toml: TODO left in {}", todos.join(", ")),
        )
        .with_hint("the author writes them; ptyrc dev check names them"));
    }
    let rust = matches!(checked.manifest.app.entry, crate::manifest::Entry::Rust(_));
    match (rust, module) {
        (true, None) => {
            return Err(Error::new(
                ErrorKind::Install,
                "a Rust app is installed with its module",
            )
            .with_hint("build::build, then install_built"));
        }
        (false, Some(_)) => {
            return Err(Error::new(
                ErrorKind::Install,
                "a module given for a script app",
            ));
        }
        _ => {}
    }
    let app = dirs.app(origin);
    mkdir(&app)?;
    let dest = app.join(commit);
    if !dest.is_dir() {
        let tmp = app.join(format!(".tmp-{commit}"));
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).map_err(|e| Error::io(tmp.display(), e))?;
        }
        mkdir(&tmp)?;
        for d in &checked.dirs {
            mkdir(&tmp.join(&d.path))?;
        }
        let built = module.map(|m| (crate::build::MODULE.to_string(), m.to_vec()));
        let files = checked
            .files
            .iter()
            .map(|f| Ok((f.path.clone(), tree.read(f)?)));
        for file in files.chain(built.map(Ok)) {
            let (path, bytes): (String, Vec<u8>) = file?;
            let p = tmp.join(&path);
            if let Some(parent) = p.parent() {
                mkdir(parent)?;
            }
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&p)
                .map_err(|e| Error::io(p.display(), e))?;
            out.write_all(&bytes)
                .map_err(|e| Error::io(p.display(), e))?;
            drop(out);
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o400))
                .map_err(|e| Error::io(p.display(), e))?;
        }
        std::fs::rename(&tmp, &dest).map_err(|e| Error::io(dest.display(), e))?;
    }
    let mut lock = Lock::load(dirs)?;
    let (previous, started) = match lock.get(&origin.id()) {
        None => (None, false),
        Some(old) if old.commit == commit => (old.previous.clone(), old.started),
        Some(old) if old.started => (Some(old.commit.clone()), false),
        Some(old) => (old.previous.clone(), false),
    };
    let entry = Installed {
        origin: origin.id(),
        id: checked.manifest.app.id.clone(),
        version: checked.manifest.app.version.clone(),
        commit: commit.to_string(),
        tier,
        date: date.to_string(),
        previous,
        started,
    };
    lock.apps.retain(|a| a.origin != entry.origin);
    lock.apps.push(entry.clone());
    lock.apps.sort_by(|a, b| a.origin.cmp(&b.origin));
    lock.save(dirs)?;
    Ok(entry)
}

/// Every installed app, sorted by origin.
pub fn list(dirs: &Dirs) -> Result<Vec<Installed>> {
    Ok(Lock::load(dirs)?.apps)
}

/// Commits of `origin` present on disk, sorted.
pub fn trees(dirs: &Dirs, origin: &Origin) -> Result<Vec<String>> {
    let app = dirs.app(origin);
    let rd = match std::fs::read_dir(&app) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io(app.display(), e)),
    };
    let mut out = Vec::new();
    for ent in rd {
        let ent = ent.map_err(|e| Error::io(app.display(), e))?;
        if let Some(n) = ent.file_name().to_str()
            && is_oid(n)
        {
            out.push(n.to_string());
        }
    }
    out.sort();
    Ok(out)
}

/// Removes the trees of `origin` other than the commit in use and
/// `previous`; returns those removed.
pub fn prune(dirs: &Dirs, origin: &Origin) -> Result<Vec<String>> {
    let lock = Lock::load(dirs)?;
    let entry = lock.get(&origin.id());
    let keep = |c: &String| entry.is_some_and(|a| &a.commit == c || a.previous.as_ref() == Some(c));
    let mut removed = Vec::new();
    for c in trees(dirs, origin)? {
        if !keep(&c) {
            let p = dirs.tree(origin, &c);
            std::fs::remove_dir_all(&p).map_err(|e| Error::io(p.display(), e))?;
            removed.push(c);
        }
    }
    Ok(removed)
}

/// Every installed app, sorted by origin, then the shipped ones
/// ([`crate::shipped`]) whose manifest reads.
pub fn all(dirs: &Dirs) -> Result<Vec<Installed>> {
    let mut apps = list(dirs)?;
    let shipped = crate::shipped::list(dirs).into_iter().flatten();
    apps.extend(shipped.map(|s| s.installed()));
    Ok(apps)
}

/// An installed app ([`Lock::resolve`]) or a shipped one: `ptyrc/NAME`,
/// or its id when no installed app has it (an installed app hides a
/// shipped one of its id, which then goes by its origin).
pub fn resolve(dirs: &Dirs, name: &str) -> Result<(Installed, Option<Shipped>)> {
    if crate::shipped::name_of(name).is_some() {
        let s = crate::shipped::get(dirs, name).ok_or_else(|| {
            Error::new(
                ErrorKind::Install,
                format!("{name}: not shipped with this ptyrc"),
            )
            .with_hint("ptyrc store ls")
        })??;
        return Ok((s.installed(), Some(s)));
    }
    match Lock::load(dirs)?.resolve(name) {
        Ok(e) => Ok((e.clone(), None)),
        Err(e) => match crate::shipped::get(dirs, name) {
            Some(s) => {
                let s = s?;
                Ok((s.installed(), Some(s)))
            }
            None => Err(e),
        },
    }
}

/// The tree of an installed or shipped app, by origin or `id`
/// ([`resolve`]): what `ptyrc app ID|ORIGIN` starts. A shipped Rust app:
/// [`Shipped::tree`].
pub fn locate(dirs: &Dirs, name: &str) -> Result<(Installed, PathBuf)> {
    let (entry, shipped) = resolve(dirs, name)?;
    if let Some(s) = shipped {
        let dir = s.tree(dirs)?;
        return Ok((entry, dir));
    }
    let origin = Origin::parse(&entry.origin)?;
    let dir = dirs.tree(&origin, &entry.commit);
    if !dir.is_dir() {
        return Err(Error::new(
            ErrorKind::Install,
            format!("{}: tree {} absent", entry.origin, dir.display()),
        )
        .with_hint(format!("ptyrc store add {}", entry.origin)));
    }
    Ok((entry, dir))
}

/// Records that the commit in use of `origin` has started once: clears
/// `previous` and prunes. Returns the trees removed.
pub fn started(dirs: &Dirs, origin: &Origin) -> Result<Vec<String>> {
    let mut lock = Lock::load(dirs)?;
    let Some(a) = lock.apps.iter_mut().find(|a| a.origin == origin.id()) else {
        return Err(Error::new(
            ErrorKind::Install,
            format!("{origin}: not installed"),
        ));
    };
    if !a.started || a.previous.is_some() {
        a.started = true;
        a.previous = None;
        lock.save(dirs)?;
    }
    prune(dirs, origin)
}

/// Removes an app: its entry in the lock and every tree of it. Its
/// storage and grants are kept: see [`purge_storage`] and
/// [`Dirs::permissions`].
pub fn remove(dirs: &Dirs, name: &str) -> Result<Installed> {
    let mut lock = Lock::load(dirs)?;
    let entry = lock.resolve(name)?.clone();
    let origin = Origin::parse(&entry.origin)?;
    let app = dirs.app(&origin);
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| Error::io(app.display(), e))?;
    }
    lock.apps.retain(|a| a.origin != entry.origin);
    lock.save(dirs)?;
    Ok(entry)
}

/// Removes `<state>/ptyrc/storage/<origin>/`; false when absent.
pub fn purge_storage(dirs: &Dirs, origin: &Origin) -> Result<bool> {
    let p = dirs.storage(origin);
    match std::fs::remove_dir_all(&p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::io(p.display(), e)),
    }
}

/// Removes the answers of `origin` from [`Dirs::permissions`]: the
/// table `[app."<id>"]`, [`Origin::id`] being the key the driver writes
/// (`Identity::key`). The other lines are kept as they are. False when
/// there was none.
pub fn purge_grants(dirs: &Dirs, origin: &Origin) -> Result<bool> {
    let p = dirs.permissions();
    let src = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(Error::io(p.display(), e)),
    };
    let table: toml::Table = src.parse().map_err(|e: toml::de::Error| {
        Error::new(
            ErrorKind::Install,
            format!("{}: {}", p.display(), e.message()),
        )
        .with_hint("written by ptyrc alone; fix or remove it")
    })?;
    let id = origin.id();
    let apps = table.get("app").and_then(toml::Value::as_table);
    if !apps.is_some_and(|apps| apps.contains_key(&id)) {
        return Ok(false);
    }
    // A header names the table of `id` when it parses to it alone.
    let names_it = |line: &str| {
        let header: Option<toml::Table> = format!("{line}\n").parse().ok();
        let app = header
            .as_ref()
            .and_then(|h| h.get("app")?.as_table().cloned());
        app.is_some_and(|app| app.len() == 1 && app.contains_key(&id))
    };
    let mut text = String::new();
    let mut inside = false;
    for line in src.lines() {
        if line.trim_start().starts_with('[') {
            inside = names_it(line.trim());
        }
        if !inside {
            text.push_str(line);
            text.push('\n');
        }
    }
    write_private(&p, text.as_bytes())?;
    Ok(true)
}
