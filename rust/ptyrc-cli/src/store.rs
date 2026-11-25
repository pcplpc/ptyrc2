// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc store`: install, update, list and remove store apps. The work
//! is `ptyrc-store`'s; see `plan/store-model.md`, "Install".

use crate::{Common, Fail, print_json};
use clap::{Args, Subcommand};
use ptyrc_store::build;
use ptyrc_store::index::Index;
use ptyrc_store::install::{self, StoreLock};
use ptyrc_store::registry::{self, Registry};
use ptyrc_store::sync::{self, Synced};
use ptyrc_store::{
    Dirs, ErrorKind, Installed, Lock, Origin, Pointer, Reference, Registries, Remote, Repo, Tier,
    Tree, Want, check, choose, hosted,
};
use serde_json::Value;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Subcommand)]
pub enum Store {
    /// Install an app: the version its registry vouches for best, else
    /// the head
    Add(Add),
    /// Remove an app and its trees
    Rm(Rm),
    /// Take the version the same rule picks now: the apps named, else
    /// all
    Update(Update),
    /// The installed apps: origin, id, version, commit, tier
    Ls,
    /// One app: as installed, as listed
    Show {
        /// An installed id, or an origin
        #[arg(value_name = "ID|ORIGIN")]
        name: String,
    },
    /// The apps listed whose origin, id, summary or description holds
    /// WORD: a line each, the origin to add first
    Search { word: String },
    /// Fetch the registries of store.toml (`url`, `api`) now; `add`,
    /// `update` and `search` do it when a copy is a day old (`api`: a
    /// minute)
    Sync,
    /// The registries of store.toml: ls, add, rm
    #[command(subcommand)]
    Registry(RegistryVerb),
}

#[derive(Subcommand)]
pub enum RegistryVerb {
    /// The registries, in order: name, source, when fetched
    Ls,
    /// Add a registry, last: https URL or a git repository here
    /// (fetched), with --path a directory read as it is, with --api a
    /// registry server
    Add {
        name: String,
        #[arg(value_name = "URL|DIR")]
        source: String,
        /// Read DIR as it is, not fetched
        #[arg(long, conflicts_with = "api")]
        path: bool,
        /// URL is a registry server (`https://host`), apps pushed to it
        #[arg(long)]
        api: bool,
    },
    /// Remove a registry and its copy
    Rm { name: String },
}

#[derive(Args)]
pub struct Add {
    /// `host/owner/repo[#path][@REF]`, `https://...`, a local directory
    /// (`/dir`, `./dir`, `local:/dir`) holding a git repository; an app
    /// of a registry server: `HOST/NAME`, or NAME, the first that lists it
    #[arg(value_name = "ORIGIN[@REF]")]
    spec: String,
    #[command(flatten)]
    pick: PickArgs,
}

#[derive(Args)]
pub struct PickArgs {
    /// The head, whatever the registry lists
    #[arg(long)]
    latest: bool,
    /// Take a yanked version; build a Rust app where the build has no
    /// OS profile (its sources may then read any file of yours)
    #[arg(long)]
    force: bool,
    /// Take an untested version without asking
    #[arg(short, long)]
    yes: bool,
    /// A Rust app of a registry server: the module the registry built,
    /// not a build here (taken by itself when there is no rustc)
    #[arg(long)]
    prebuilt: bool,
}

#[derive(Args)]
pub struct Rm {
    #[arg(value_name = "ID|ORIGIN")]
    name: String,
    /// Its storage and its grants too
    #[arg(long)]
    purge: bool,
}

#[derive(Args)]
pub struct Update {
    #[arg(value_name = "ID|ORIGIN")]
    names: Vec<String>,
    #[command(flatten)]
    pick: PickArgs,
}

impl From<ptyrc_store::Error> for Fail {
    fn from(e: ptyrc_store::Error) -> Fail {
        let status = match e.kind {
            ErrorKind::Fetch | ErrorKind::Io => 5,
            _ => 2,
        };
        Fail {
            status,
            message: e.message,
            hint: e.hint,
        }
    }
}

pub fn store(common: &Common, verb: Store) -> Result<ExitCode, Fail> {
    let dirs = Dirs::from_env()?;
    match verb {
        Store::Add(args) => add(&dirs, args),
        Store::Rm(args) => rm(&dirs, args),
        Store::Update(args) => update(&dirs, args),
        Store::Ls => ls(&dirs, common.json),
        Store::Show { name } => show(&dirs, &name),
        Store::Search { word } => search(&dirs, &word, common.json),
        Store::Sync => sync_verb(&dirs),
        Store::Registry(v) => registry_verb(&dirs, v),
    }
}

/// The store lock; told when another `ptyrc` holds it.
fn lock(dirs: &Dirs) -> Result<StoreLock, Fail> {
    Ok(StoreLock::take(dirs, || {
        eprintln!("ptyrc: another ptyrc is installing; waiting")
    })?)
}

/// `ORIGIN[@REF]`; a path (`/`, `.` first) is a local origin.
pub fn parse_spec(spec: &str) -> Result<(Origin, Option<Reference>), Fail> {
    if !spec.starts_with(['/', '.']) {
        return Ok(Origin::parse_spec(spec)?);
    }
    let (head, at) = match spec.rsplit_once('@') {
        Some((h, r)) => (h, Some(r)),
        None => (spec, None),
    };
    let (path, sub) = match head.split_once('#') {
        Some((p, s)) => (p, Some(s)),
        None => (head, None),
    };
    let abs = std::fs::canonicalize(path).map_err(|e| {
        Fail::usage(
            format!("{path}: {}", io_text(&e)),
            "a local origin is a directory holding a git repository",
        )
    })?;
    let mut text = format!("local:{}", abs.display());
    if let Some(s) = sub {
        text.push('#');
        text.push_str(s);
    }
    if let Some(r) = at {
        text.push('@');
        text.push_str(r);
    }
    Ok(Origin::parse_spec(&text)?)
}

/// An `io::Error` without its `(os error N)`.
pub fn io_text(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Where `origin` is fetched from. Debug builds only: with
/// `PTYRC_TEST_GIT_ROOT`, an https origin `host/owner/repo` is the local
/// repository `$PTYRC_TEST_GIT_ROOT/host/owner/repo`, for tests without
/// the network.
pub(crate) fn remote_of(origin: &Origin) -> Remote {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("PTYRC_TEST_GIT_ROOT")
        && !origin.is_local()
        && let Remote::Https(url) = origin.remote()
    {
        let rest = url.trim_start_matches("https://");
        return Remote::Local(std::path::Path::new(&root).join(rest));
    }
    origin.remote()
}

/// Today, UTC, `YYYY-MM-DD`.
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of(secs)
}

/// Seconds since 1970 as a UTC date, `YYYY-MM-DD`.
fn date_of(secs: u64) -> String {
    // Days to civil date (H. Hinnant).
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// A tier, `untested` in red on a terminal.
fn tier_word(tier: Tier, tty: bool) -> String {
    if tier == Tier::Untested && tty {
        format!("\x1b[31m{tier}\x1b[0m")
    } else {
        tier.to_string()
    }
}

/// An untested version wants `y`; without a terminal, `--yes`.
fn confirm(origin: &Origin, commit: &str, id: &str, yes: bool) -> Result<(), Fail> {
    let tty = std::io::stderr().is_terminal();
    eprintln!(
        "ptyrc: {origin} ({id}) at {}: {}: nobody vouches for this version",
        short(commit),
        tier_word(Tier::Untested, tty)
    );
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(Fail::usage(
            format!("{origin}: untested; not installed"),
            "--yes to take it without asking",
        ));
    }
    eprint!("install it? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    match line.trim() {
        "y" | "Y" | "yes" => Ok(()),
        _ => Err(Fail {
            status: 1,
            message: format!("{origin}: not installed"),
            hint: None,
        }),
    }
}

/// What a fetch came to.
enum Outcome {
    Installed(Installed),
    /// The commit in use is the one picked.
    Current,
    /// An update to a lower tier, refused without `--latest`.
    Held {
        found: Tier,
    },
}

/// Picks, fetches, checks, asks, installs. `found`: the registry that
/// lists `origin`, and its pointer; `held`: the entry in use; `hold`
/// (`update`): not for a lower tier unless `--latest`. The caller holds
/// the store lock.
fn obtain(
    dirs: &Dirs,
    origin: &Origin,
    found: Option<(&Registry, &Pointer)>,
    want: &Want,
    args: &PickArgs,
    held: Option<&Installed>,
    hold: bool,
) -> Result<Outcome, Fail> {
    let pointer = found.map(|(_, p)| p);
    let pick = match origin.is_hosted() {
        true => hosted::pick(origin, pointer, want)?,
        false => choose(pointer, want)?,
    };
    let current = |c: &str| held.is_some_and(|h| h.commit == c);
    // The commit in use, yanked since: not held to, not kept as current.
    let yanked = held.is_some_and(|h| listed_now(pointer, &h.commit) == Some(Now::Yanked));
    if let Reference::Commit(c) = &pick.reference
        && current(c)
    {
        return Ok(Outcome::Current);
    }
    // A hosted app: the archive of the revision picked; else the commit
    // fetched, its tree read once it is not the one in use.
    let (commit, got) = match origin.remote() {
        Remote::Registry(base) => {
            let Reference::Commit(rev) = pick.reference else {
                return Err(Fail::from("a hosted app is named by its revision"));
            };
            let tree = hosted::fetch_archive(&base, &rev)?;
            (rev, Err(tree))
        }
        _ => {
            let repo = Repo::open(&dirs.git(origin))?;
            let commit = repo.fetch(&remote_of(origin), &pick.reference)?;
            (commit, Ok(repo))
        }
    };
    if current(&commit) && yanked && !want.force {
        return Err(Fail::usage(
            format!(
                "{origin}: {} is yanked, and no other version is listed",
                short(&commit)
            ),
            "ptyrc store rm ORIGIN; or --force to keep it",
        ));
    }
    if current(&commit) {
        return Ok(Outcome::Current);
    }
    let tree: Box<dyn Tree> = match got {
        Ok(repo) => Box::new(repo.tree(&commit, origin.sub())?),
        Err(archive) => Box::new(archive),
    };
    let tree = tree.as_ref();
    let checked = check(tree).map_err(|e| {
        let hint = match &e.hint {
            Some(h) => format!("{h}; `ptyrc dev check` lists every problem"),
            None => "`ptyrc dev check` lists every problem".into(),
        };
        Fail {
            hint: Some(hint),
            ..Fail::from(e)
        }
    })?;
    let tier = registry::tier_of(pointer, &commit, want.force)?;
    if let Some(h) = held
        && hold
        && !yanked
        && tier < h.tier
        && !want.latest
    {
        return Ok(Outcome::Held { found: tier });
    }
    if tier == Tier::Untested {
        confirm(origin, &commit, &checked.manifest.app.id, args.yes)?;
    }
    let module = match checked.manifest.app.entry {
        ptyrc_store::Entry::Rust(_) => Some(module_of(
            dirs,
            origin,
            found.map(|(r, _)| r),
            &commit,
            tree,
            &checked,
            args,
        )?),
        ptyrc_store::Entry::Script(_) => None,
    };
    let entry = install::install_built(
        dirs,
        origin,
        tree,
        &checked,
        module.as_deref(),
        &commit,
        tier,
        &today(),
    )?;
    install::prune(dirs, origin)?;
    Ok(Outcome::Installed(entry))
}

/// The sha256 of the module the registry lists for `rev`, from its
/// index.
fn listed_module(registry: Option<&Registry>, origin: &Origin, rev: &str) -> Option<String> {
    let ix = Index::load(registry?)?;
    let app = ix.get(&origin.id())?;
    let v = app.versions.iter().find(|v| v.commit == rev)?;
    v.wasm.clone()
}

/// The module of a Rust app: built here when `rustc` is found and not
/// `--prebuilt`; else, for an app of a registry server, the module it
/// lists for `rev`.
fn module_of(
    dirs: &Dirs,
    origin: &Origin,
    registry: Option<&Registry>,
    rev: &str,
    tree: &dyn Tree,
    checked: &ptyrc_store::Checked,
    args: &PickArgs,
) -> Result<Vec<u8>, Fail> {
    let base = match origin.remote() {
        Remote::Registry(b) => Some(b),
        _ => None,
    };
    if args.prebuilt && base.is_none() {
        return Err(Fail::usage(
            format!("{origin}: --prebuilt: not an app of a registry server; built here"),
            "drop --prebuilt",
        ));
    }
    let listed = base
        .as_ref()
        .and_then(|_| listed_module(registry, origin, rev));
    let toolchain = match args.prebuilt {
        true => None,
        false => Some(build::Toolchain::find()),
    };
    let missing = match toolchain {
        Some(Ok(t)) => return build_rust(dirs, origin, tree, checked, t, args.force),
        Some(Err(e)) => Some(e),
        None => None,
    };
    let (Some(base), Some(sha)) = (base, listed) else {
        return Err(match missing {
            Some(e) if origin.is_hosted() => {
                let hint = match &e.hint {
                    Some(h) => format!("{h}; the registry lists no module of this version"),
                    None => "the registry lists no module of this version".into(),
                };
                Fail {
                    hint: Some(hint),
                    ..Fail::from(e)
                }
            }
            Some(e) => Fail::from(e),
            None => Fail {
                status: 4,
                message: format!(
                    "{origin}: --prebuilt: the registry lists no module of {}",
                    short(rev)
                ),
                hint: Some(
                    "ptyrc store sync, if built since; or without --prebuilt, built here with rustc"
                        .into(),
                ),
            },
        });
    };
    let why = match missing {
        Some(_) => "no rustc here",
        None => "--prebuilt",
    };
    eprintln!(
        "ptyrc: {}: the registry's module of {} ({why}); it runs in the sandbox under the same grants; that it is this source's build is the registry's word",
        checked.manifest.app.id,
        short(rev)
    );
    Ok(hosted::fetch_module(&base, rev, &sha)?)
}

/// Step 3 of the install for a Rust app: the build, in
/// `<cache>/ptyrc/build/<origin>/`, removed after. `force`: without the
/// OS profile where there is none, said on stderr.
fn build_rust(
    dirs: &Dirs,
    origin: &Origin,
    tree: &dyn Tree,
    checked: &ptyrc_store::Checked,
    toolchain: build::Toolchain,
    force: bool,
) -> Result<Vec<u8>, Fail> {
    let work = dirs.cache.join("ptyrc/build").join(origin.dir_name());
    let options = build::Options {
        unconfined: force,
        ..build::Options::default()
    };
    eprintln!(
        "ptyrc: building {} with rustc {}",
        checked.manifest.app.id, toolchain.release
    );
    let built = build::build(tree, checked, &work, &toolchain, &options);
    let _ = std::fs::remove_dir_all(&work);
    let built = built?;
    match &built.missing {
        Some(why) => eprintln!(
            "ptyrc: warning: built without an OS profile ({why}), by --force: its sources could read any file of yours into the module"
        ),
        None => eprintln!(
            "ptyrc: built under the profile: {}",
            built.profile.join("; ")
        ),
    }
    Ok(built.wasm)
}

fn said(e: &Installed) -> String {
    format!(
        "{} {} at {}, {}",
        e.id,
        e.version,
        short(&e.commit),
        tier_word(e.tier, std::io::stdout().is_terminal())
    )
}

/// `NAME[@REV]`: no `/`, no `:`, not a path, not a shipped app's name.
fn bare_name<'a>(dirs: &Dirs, spec: &'a str) -> Option<(&'a str, Option<&'a str>)> {
    let (name, at) = match spec.split_once('@') {
        Some((n, r)) => (n, Some(r)),
        None => (spec, None),
    };
    let bare = !name.is_empty() && !name.contains(['/', ':', '#']) && !name.starts_with('.');
    (bare && shipped_named(dirs, name).is_none()).then_some((name, at))
}

/// The hosted origin of NAME: the first registry server that lists it.
fn resolve_bare(registries: &Registries, name: &str) -> Result<Origin, Fail> {
    hosted::resolve_name(registries, name)?.ok_or_else(|| Fail {
        status: 4,
        message: format!("{name}: listed by no registry server here"),
        hint: Some("ptyrc store search WORD; or name its origin, host/owner/repo".into()),
    })
}

fn add(dirs: &Dirs, args: Add) -> Result<ExitCode, Fail> {
    let bare = bare_name(dirs, &args.spec);
    let word = args.spec.split('@').next().unwrap_or("");
    if bare.is_none()
        && !word.contains(['/', ':', '#'])
        && let Some(origin) = shipped_named(dirs, word)
    {
        return Err(Fail::usage(
            format!("{word}: {origin}, shipped with ptyrc"),
            "an app a registry server lists under that name: ptyrc store add HOST/NAME",
        ));
    }
    let parsed = match bare {
        Some(_) => None,
        None => Some(parse_spec(&args.spec)?),
    };
    let _lock = lock(dirs)?;
    let registries = Registries::load(dirs)?;
    refresh(&registries, false);
    let (origin, reference) = match (parsed, bare) {
        (Some(p), _) => p,
        (None, Some((name, at))) => {
            let origin = match hosted::resolve_name(&registries, name)? {
                Some(o) => o,
                None => {
                    // Pushed since the copy: fetched again, once.
                    refresh_api(&registries);
                    resolve_bare(&registries, name)?
                }
            };
            (origin, at.map(Reference::parse).transpose()?)
        }
        (None, None) => unreachable!("parsed or bare"),
    };
    let want = Want {
        reference,
        latest: args.pick.latest,
        force: args.pick.force,
    };
    let mut found = lookup(&registries, &origin, &want)?;
    if found.is_none() && origin.is_hosted() {
        refresh_api(&registries);
        found = lookup(&registries, &origin, &want)?;
    }
    let held = Lock::load(dirs)?.get(&origin.id()).cloned();
    match obtain(
        dirs,
        &origin,
        found.as_ref().map(|(r, p)| (*r, p)),
        &want,
        &args.pick,
        held.as_ref(),
        false,
    )? {
        Outcome::Installed(e) => println!("added {origin}: {}", said(&e)),
        Outcome::Current | Outcome::Held { .. } => {
            let e = held.expect("held when current");
            println!("{origin}: already {}", said(&e));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The entry of `name`; not installed: status 4.
fn installed(dirs: &Dirs, name: &str) -> Result<Installed, Fail> {
    let lock = Lock::load(dirs)?;
    let e = lock.resolve(name).map_err(|e| Fail {
        status: if e.message.ends_with("not installed") {
            4
        } else {
            2
        },
        ..Fail::from(e)
    })?;
    Ok(e.clone())
}

/// A shipped app named (`ptyrc/NAME`, or an id no installed app has):
/// it goes with ptyrc, never by the store.
fn shipped_named(dirs: &Dirs, name: &str) -> Option<String> {
    match install::resolve(dirs, name) {
        Ok((e, Some(_))) => Some(e.origin),
        _ => None,
    }
}

fn rm(dirs: &Dirs, args: Rm) -> Result<ExitCode, Fail> {
    if let Some(origin) = shipped_named(dirs, &args.name) {
        return Err(Fail::usage(
            format!("{origin}: shipped with ptyrc: removed with it, not by the store"),
            "its grants: ptyrc manage, or the table of permissions.toml",
        ));
    }
    let _lock = lock(dirs)?;
    let gone = match installed(dirs, &args.name) {
        Ok(e) => Some(install::remove(dirs, &e.origin)?),
        // Purged after its removal: an origin no longer installed.
        Err(f) if f.status == 4 && args.purge && Origin::parse(&args.name).is_ok() => None,
        Err(f) => return Err(f),
    };
    let origin = match &gone {
        Some(e) => Origin::parse(&e.origin)?,
        None => parse_spec(&args.name)?.0,
    };
    if let Some(e) = &gone {
        println!("removed {origin}: {} {}", e.id, e.version);
    }
    if args.purge {
        let storage = install::purge_storage(dirs, &origin)?;
        let grants = install::purge_grants(dirs, &origin)?;
        let what = match (storage, grants) {
            (true, true) => "its storage and its grants",
            (true, false) => "its storage",
            (false, true) => "its grants",
            (false, false) => "nothing to purge",
        };
        println!("purged {origin}: {what}");
    }
    Ok(ExitCode::SUCCESS)
}

fn update(dirs: &Dirs, args: Update) -> Result<ExitCode, Fail> {
    let _lock = lock(dirs)?;
    let lock = Lock::load(dirs)?;
    let entries: Vec<Installed> = if args.names.is_empty() {
        lock.apps.clone()
    } else {
        let mut v = Vec::new();
        for n in &args.names {
            match shipped_named(dirs, n) {
                Some(origin) => println!("{origin}: shipped with ptyrc: updated with it"),
                None => v.push(installed(dirs, n)?),
            }
        }
        if v.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }
        v
    };
    if entries.is_empty() {
        eprintln!("ptyrc: no store app\n  ptyrc store add ORIGIN");
        return Ok(ExitCode::SUCCESS);
    }
    let registries = Registries::load(dirs)?;
    refresh(&registries, false);
    let mut failed = None;
    for held in &entries {
        let origin = Origin::parse(&held.origin)?;
        let one = || -> Result<Outcome, Fail> {
            let want = Want {
                reference: None,
                latest: args.pick.latest,
                force: args.pick.force,
            };
            let found = lookup(&registries, &origin, &want)?;
            obtain(
                dirs,
                &origin,
                found.as_ref().map(|(r, p)| (*r, p)),
                &want,
                &args.pick,
                Some(held),
                true,
            )
        };
        match one() {
            Ok(Outcome::Installed(e)) => println!("updated {origin}: {}", said(&e)),
            Ok(Outcome::Current) => println!("{origin}: up to date, {}", said(held)),
            Ok(Outcome::Held { found }) => println!(
                "{origin}: kept at {}; the version found is {found} (--latest takes it)",
                said(held)
            ),
            Err(f) if entries.len() == 1 => return Err(f),
            Err(f) => {
                eprintln!("ptyrc: {}", f.message);
                if let Some(h) = &f.hint {
                    eprintln!("  {h}");
                }
                failed = Some(f.status);
            }
        }
    }
    Ok(failed.map_or(ExitCode::SUCCESS, ExitCode::from))
}

/// Fetches the registries whose copy is older than their
/// [`Registry::max_age`] (`all`: every one); a failure is a warning. The
/// caller holds the store lock. Returns the number that failed.
fn refresh(registries: &Registries, all: bool) -> usize {
    let mut failed = 0;
    for r in &registries.list {
        let max_age: Option<Duration> = (!all).then(|| r.max_age());
        match sync::sync(r, SystemTime::now(), max_age) {
            Synced::Local | Synced::Fresh | Synced::Fetched { .. } => {}
            Synced::Stale { error, since } => {
                failed += 1;
                let when = since.map_or("an unknown date".to_string(), date_of);
                eprintln!("ptyrc: warning: {}", error.message);
                eprintln!("  the copy of {when} is used");
            }
            Synced::Missing { error } => {
                failed += 1;
                eprintln!("ptyrc: warning: {}", error.message);
                eprintln!("  registry {}: no copy here", r.name);
            }
        }
    }
    failed
}

/// The default registry, not reached, said once a process.
static DEFAULT_WARNED: AtomicBool = AtomicBool::new(false);

/// The default registry, as vendored: [`registry::default_api`].
fn is_default(r: &Registry) -> bool {
    registry::default_api().is_some_and(|(name, base)| {
        r.name == name && r.api.as_deref().map(|a| a.trim_end_matches('/')) == Some(base)
    })
}

/// Fetches every registry server now, whatever the age of its copy; a
/// failure is a warning.
fn refresh_api(registries: &Registries) {
    let api = Registries {
        list: registries
            .list
            .iter()
            .filter(|r| r.api.is_some())
            .cloned()
            .collect(),
    };
    refresh(&api, true);
}

/// The pointer of the first registry that lists `origin`. A registry
/// with no copy here, before it, may have spoken: refused, unless the
/// version is named (`@REF`, `--latest`; never for a hosted origin). Not
/// in the way: for a hosted origin, a registry server of another host;
/// for a git origin, the default registry (`registry::default_api`, its
/// name and URL) with no copy, a warning.
fn lookup<'a>(
    registries: &'a Registries,
    origin: &Origin,
    want: &Want,
) -> Result<Option<(&'a Registry, Pointer)>, Fail> {
    let present = registries.lookup_present(origin)?;
    let mut absent = present
        .absent
        .iter()
        .copied()
        .filter(|r| match origin.is_hosted() {
            true => r.api.is_none() || r.api_host().as_deref() == Some(origin.host()),
            false => !is_default(r),
        });
    let absent = absent.next();
    if let Some(r) = present.absent.iter().find(|r| is_default(r))
        && !origin.is_hosted()
        && !DEFAULT_WARNED.swap(true, Ordering::Relaxed)
    {
        eprintln!(
            "ptyrc: warning: registry {} (the default): no copy here; passed over for {origin}",
            r.name
        );
    }
    if let Some(r) = absent
        && origin.is_hosted()
    {
        return Err(Fail {
            status: 5,
            message: format!(
                "registry {}: not fetched; its word on {origin} is unknown",
                r.name
            ),
            hint: Some("ptyrc store sync".into()),
        });
    }
    if let Some(r) = absent
        && want.reference.is_none()
        && !want.latest
    {
        return Err(Fail {
            status: 5,
            message: format!("registry {}: not fetched; its word on {origin} is unknown", r.name),
            hint: Some(
                "ptyrc store sync; or --latest, or ORIGIN@REF, to install without a registry's word (untested)"
                    .into(),
            ),
        });
    }
    Ok(present.found)
}

/// What the registries say now of an installed commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Now {
    Tier(Tier),
    Yanked,
}

/// The word of `pointer` on `commit`; no pointer: none.
fn listed_now(pointer: Option<&Pointer>, commit: &str) -> Option<Now> {
    let p = pointer?;
    Some(match p.tier_of(commit, false) {
        Ok(t) => Now::Tier(t),
        Err(_) => Now::Yanked,
    })
}

/// The word now on an installed app, from the copies here; none when
/// no registry lists it or one cannot be read.
fn now_of(registries: &Registries, e: &Installed) -> Option<Now> {
    let origin = Origin::parse(&e.origin).ok()?;
    let present = registries.lookup_present(&origin).ok()?;
    listed_now(present.found.as_ref().map(|(_, p)| p), &e.commit)
}

/// `s` without control characters: registry text reaches a terminal.
fn plain(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn red(s: &str, tty: bool) -> String {
    match tty {
        true => format!("\x1b[31m{s}\x1b[0m"),
        false => s.to_string(),
    }
}

fn yanked_line(origin: &str, tty: bool) {
    eprintln!(
        "ptyrc: {}",
        red(&format!("{origin}: the version installed is yanked"), tty)
    );
    eprintln!("  ptyrc store update {origin}");
}

fn ls(dirs: &Dirs, json: bool) -> Result<ExitCode, Fail> {
    let apps = install::all(dirs)?;
    let registries = Registries::load(dirs).unwrap_or_default();
    let now: Vec<Option<Now>> = apps.iter().map(|a| now_of(&registries, a)).collect();
    if json {
        let rows: Vec<Value> = apps
            .iter()
            .zip(&now)
            .map(|(a, n)| {
                let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
                v["listed"] = match n {
                    None => Value::Null,
                    Some(Now::Yanked) => "yanked".into(),
                    Some(Now::Tier(t)) => t.as_str().into(),
                };
                v
            })
            .collect();
        print_json(&Value::Array(rows), false);
        return Ok(ExitCode::SUCCESS);
    }
    if apps.is_empty() {
        eprintln!("ptyrc: no store app\n  ptyrc store add ORIGIN");
        return Ok(ExitCode::SUCCESS);
    }
    let width = |f: fn(&Installed) -> &str, head: &str| {
        apps.iter()
            .map(|a| f(a).len())
            .max()
            .unwrap_or(0)
            .max(head.len())
    };
    let wo = width(|a| &a.origin, "ORIGIN");
    let wi = width(|a| &a.id, "ID");
    let wv = width(|a| &a.version, "VERSION");
    let tty = std::io::stdout().is_terminal();
    println!(
        "{:wo$}  {:wi$}  {:wv$}  {:12}  TIER",
        "ORIGIN", "ID", "VERSION", "COMMIT"
    );
    for (a, n) in apps.iter().zip(&now) {
        let tier = match n {
            Some(Now::Yanked) => red("yanked", tty),
            Some(Now::Tier(t)) => tier_word(*t, tty),
            None => tier_word(a.tier, tty),
        };
        println!(
            "{:wo$}  {:wi$}  {:wv$}  {:12}  {tier}",
            a.origin,
            a.id,
            a.version,
            short(&a.commit),
        );
    }
    let etty = std::io::stderr().is_terminal();
    for (a, n) in apps.iter().zip(&now) {
        if *n == Some(Now::Yanked) {
            yanked_line(&a.origin, etty);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn show(dirs: &Dirs, name: &str) -> Result<ExitCode, Fail> {
    if let Ok((e, Some(s))) = install::resolve(dirs, name) {
        let tty = std::io::stdout().is_terminal();
        println!("origin       {}", e.origin);
        println!("id           {}", s.manifest.app.id);
        println!("version      {}", s.manifest.app.version);
        println!("summary      {}", s.manifest.app.summary);
        println!(
            "commit       ptyrc {} ({})",
            e.commit,
            tier_word(e.tier, tty)
        );
        return Ok(describe(&s.manifest, &s.dir));
    }
    let entry = match installed(dirs, name) {
        Ok(e) => Some(e),
        Err(f) if f.status == 4 => None,
        Err(f) => return Err(f),
    };
    let registries = Registries::load(dirs)?;
    let origin = match &entry {
        Some(e) => Origin::parse(&e.origin)?,
        None => match parse_spec(name) {
            Ok((o, _)) => o,
            Err(_) => match bare_name(dirs, name)
                .and_then(|(n, _)| hosted::resolve_name(&registries, n).ok().flatten())
            {
                Some(o) => o,
                None => {
                    return Err(Fail {
                        status: 4,
                        message: format!("{name}: not installed"),
                        hint: Some("ptyrc store ls; or name an origin".into()),
                    });
                }
            },
        },
    };
    let present = registries.lookup_present(&origin)?;
    let found = present.found;
    if entry.is_none() && found.is_none() {
        return Err(Fail {
            status: 4,
            message: format!("{origin}: neither installed nor listed"),
            hint: Some("ptyrc store search WORD".into()),
        });
    }
    let tty = std::io::stdout().is_terminal();
    println!("origin       {origin}");
    if let Some(e) = &entry {
        let (_, dir) = install::locate(dirs, &e.origin)?;
        let text = std::fs::read(dir.join(ptyrc_store::manifest::MANIFEST))?;
        let m = ptyrc_store::manifest::parse_bytes(&text)?;
        println!("id           {}", m.app.id);
        println!("version      {}", m.app.version);
        println!("summary      {}", m.app.summary);
        println!(
            "commit       {} ({}, {})",
            e.commit,
            tier_word(e.tier, tty),
            e.date
        );
        match listed_now(found.as_ref().map(|(_, p)| p), &e.commit) {
            Some(Now::Yanked) => {
                println!("listed now   {}", red("yanked", tty));
                yanked_line(&e.origin, std::io::stderr().is_terminal());
            }
            Some(Now::Tier(t)) if t != e.tier => println!("listed now   {}", tier_word(t, tty)),
            _ => {}
        }
        if let Some(p) = &e.previous {
            println!("previous     {p} (kept until the new one has started)");
        }
        describe(&m, &dir);
    }
    if let Some((registry, p)) = &found {
        if entry.is_none()
            && let Some(a) = Index::load(registry)
                .as_ref()
                .and_then(|ix| ix.get(&origin.id()))
        {
            // The index's word, for the eye; the install reads the manifest.
            for (k, v) in [
                ("id", &a.id),
                ("version", &a.version),
                ("summary", &a.summary),
                ("kind", &a.kind),
                ("description", &a.description),
            ] {
                if let Some(v) = v {
                    println!("{k:12} {}", plain(v));
                }
            }
            if !a.permissions.is_empty() {
                println!("permissions");
                for p in &a.permissions {
                    let line = format!("  {}: {}", plain(&p.name), plain(&p.why));
                    match p.colour {
                        ptyrc_store::manifest::Colour::Red => println!("{}", red(&line, tty)),
                        _ => println!("{line}"),
                    }
                }
            }
        }
        let yanked = if p.yanked { ", yanked" } else { "" };
        println!("listed in    {}{yanked}", registry.name);
        for v in &p.versions {
            let date = v.date.map(|d| format!(" {d}")).unwrap_or_default();
            let yanked = if v.yanked { " yanked" } else { "" };
            println!("  {} {}{date}{yanked}", v.commit, tier_word(v.tier, tty));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The lines of `show` from the manifest: surface, tree, permissions,
/// options.
fn describe(m: &ptyrc_store::Manifest, dir: &Path) -> ExitCode {
    println!("surface      {}", m.app.surface.as_str());
    println!("tree         {}", dir.display());
    if !m.permissions.is_empty() {
        println!("permissions");
        for p in &m.permissions {
            println!("  {}: {}", p.name, p.why);
        }
    }
    if !m.options.is_empty() {
        println!("options");
        for o in &m.options {
            println!("  {} ({}): {}", o.name, o.type_name(), o.doc);
        }
    }
    ExitCode::SUCCESS
}

/// A line of `search`.
struct Row {
    origin: String,
    /// The best tier listed, yanked versions aside.
    tier: Option<Tier>,
    registry: String,
    summary: Option<String>,
    /// A registry server's: `tar` or `git`.
    kind: Option<String>,
    description: Option<String>,
}

fn search(dirs: &Dirs, word: &str, json: bool) -> Result<ExitCode, Fail> {
    let registries = Registries::load(dirs)?;
    if registries.list.is_empty() {
        return Err(Fail::usage(
            format!("no registry: {} names none", dirs.store().display()),
            "ptyrc store registry add NAME URL",
        ));
    }
    {
        let _lock = lock(dirs)?;
        refresh(&registries, false);
    }
    let word = word.to_lowercase();
    let lock = Lock::load(dirs)?;
    let mut rows: Vec<Row> = Vec::new();
    for r in &registries.list {
        if !r.dir.is_dir() {
            eprintln!("ptyrc: registry {}: no copy here", r.name);
            continue;
        }
        let index = Index::load(r);
        for p in r.pointers()? {
            let origin = p.origin()?.id();
            if rows.iter().any(|row| row.origin == origin) {
                continue;
            }
            let ix = index.as_ref().and_then(|ix| ix.get(&origin));
            let summary = ix.and_then(|a| a.summary.as_deref()).map(plain);
            let id = ix.and_then(|a| a.id.as_deref()).unwrap_or("");
            let description = ix.and_then(|a| a.description.as_deref()).map(plain);
            let kind = ix.and_then(|a| a.kind.clone());
            let hay = format!(
                "{origin}\n{id}\n{}\n{}",
                summary.as_deref().unwrap_or(""),
                description.as_deref().unwrap_or("")
            );
            if !hay.to_lowercase().contains(&word) {
                continue;
            }
            let best = p
                .versions
                .iter()
                .filter(|v| !v.yanked && !p.yanked)
                .map(|v| v.tier)
                .max();
            rows.push(Row {
                origin,
                tier: best,
                registry: r.name.clone(),
                summary,
                kind,
                description,
            });
        }
    }
    if json {
        let rows: Vec<Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "origin": r.origin,
                    "tier": r.tier.map(Tier::as_str),
                    "registry": r.registry,
                    "summary": r.summary,
                    "kind": r.kind,
                    "description": r.description,
                    "installed": lock.get(&r.origin).is_some(),
                })
            })
            .collect();
        print_json(&Value::Array(rows), false);
    } else {
        let tty = std::io::stdout().is_terminal();
        let wo = rows.iter().map(|r| r.origin.len()).max().unwrap_or(0);
        for row in &rows {
            let (o, t, r, s) = (&row.origin, &row.tier, &row.registry, &row.summary);
            let cell = format!("{:8}", t.map_or("-", Tier::as_str));
            let cell = match t {
                Some(Tier::Untested) => red(&cell, tty),
                _ => cell,
            };
            let mark = if lock.get(o).is_some() {
                "  installed"
            } else {
                ""
            };
            let s = s.as_deref().map(|s| format!("  {s}")).unwrap_or_default();
            println!("{o:wo$}  {cell}  {r}{mark}{s}");
        }
    }
    if rows.is_empty() {
        eprintln!("ptyrc: no app matches {word:?}");
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn sync_verb(dirs: &Dirs) -> Result<ExitCode, Fail> {
    let registries = Registries::load(dirs)?;
    if registries.list.is_empty() {
        return Err(Fail::usage(
            format!("no registry: {} names none", dirs.store().display()),
            "ptyrc store registry add NAME URL",
        ));
    }
    let _lock = lock(dirs)?;
    let mut failed = false;
    for r in &registries.list {
        match sync::sync(r, SystemTime::now(), None) {
            Synced::Local => println!("{}: a directory, read as it is", r.name),
            Synced::Fresh => {}
            Synced::Fetched { commit } => println!("{}: fetched {}", r.name, short(&commit)),
            Synced::Stale { error, since } => {
                failed = true;
                let when = since.map_or("an unknown date".to_string(), date_of);
                eprintln!("ptyrc: {}", error.message);
                eprintln!("  the copy of {when} is kept");
            }
            Synced::Missing { error } => {
                failed = true;
                eprintln!("ptyrc: {}", error.message);
                eprintln!(
                    "  {}",
                    error.hint.as_deref().unwrap_or(match r.api {
                        Some(_) => "is the URL that of a registry server?",
                        None => "is the URL that of a git repository?",
                    })
                );
            }
        }
    }
    Ok(match failed {
        true => ExitCode::from(5),
        false => ExitCode::SUCCESS,
    })
}

fn registry_verb(dirs: &Dirs, verb: RegistryVerb) -> Result<ExitCode, Fail> {
    match verb {
        RegistryVerb::Ls => {
            let registries = Registries::load(dirs)?;
            if registries.list.is_empty() {
                eprintln!(
                    "ptyrc: no registry in {}\n  ptyrc store registry add NAME URL",
                    dirs.store().display()
                );
            }
            for r in &registries.list {
                let (kind, src) = match (&r.url, &r.api) {
                    (_, Some(a)) => ("api", a.clone()),
                    (Some(u), None) => ("url", u.clone()),
                    (None, None) => ("path", r.dir.display().to_string()),
                };
                let fetched = r.url.is_some() || r.api.is_some();
                let state = match (fetched, sync::stamp(r)) {
                    (false, _) => String::new(),
                    (true, Some((at, c))) if r.dir.is_dir() => {
                        format!("  fetched {} at {}", date_of(at), short(&c))
                    }
                    (true, _) => "  not fetched".into(),
                };
                println!("{}  {kind} {src}{state}", r.name);
            }
            Ok(ExitCode::SUCCESS)
        }
        RegistryVerb::Add {
            name,
            source,
            path: _,
            api: true,
        } => {
            let _lock = lock(dirs)?;
            let mut registries = Registries::load(dirs)?;
            registries.add_api(dirs, &name, &source)?;
            registries.save(dirs)?;
            println!("added registry {name}: api {source}");
            let r = registries.list.last().expect("just added");
            fetch_new(r);
            Ok(ExitCode::SUCCESS)
        }
        RegistryVerb::Add {
            name,
            source,
            path,
            api: false,
        } => {
            let _lock = lock(dirs)?;
            let mut registries = Registries::load(dirs)?;
            let local = !source.starts_with("https://");
            let src = match local {
                true => std::fs::canonicalize(&source)
                    .map_err(|e| {
                        Fail::usage(
                            format!("{source}: {}", io_text(&e)),
                            "an https URL, or a directory here",
                        )
                    })?
                    .display()
                    .to_string(),
                false => source.clone(),
            };
            if local && !path && !is_git(Path::new(&src)) {
                return Err(Fail::usage(
                    format!("{src}: not a git repository"),
                    "--path to read a directory as it is",
                ));
            }
            if path && !local {
                return Err(Fail::usage("--path: a directory, not a URL", "drop --path"));
            }
            let key = if path { "path" } else { "url" };
            registries.add(dirs, &name, &src, path)?;
            registries.save(dirs)?;
            println!("added registry {name}: {key} {src}");
            if !path {
                fetch_new(registries.list.last().expect("just added"));
            }
            Ok(ExitCode::SUCCESS)
        }
        RegistryVerb::Rm { name } => {
            let _lock = lock(dirs)?;
            let mut registries = Registries::load(dirs)?;
            let Some(i) = registries.list.iter().position(|r| r.name == name) else {
                return Err(Fail {
                    status: 4,
                    message: format!("registry {name}: not in {}", dirs.store().display()),
                    hint: Some("ptyrc store registry ls".into()),
                });
            };
            let r = registries.list.remove(i);
            registries.save(dirs)?;
            if (r.url.is_some() || r.api.is_some()) && r.dir.starts_with(dirs.registry()) {
                let _ = std::fs::remove_dir_all(&r.dir);
            }
            println!("removed registry {name}");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// The first fetch of a registry just added; a failure is a warning.
fn fetch_new(r: &Registry) {
    match sync::sync(r, SystemTime::now(), None) {
        Synced::Fetched { commit } => println!("{}: fetched {}", r.name, short(&commit)),
        Synced::Stale { error, .. } | Synced::Missing { error } => {
            eprintln!("ptyrc: warning: {}", error.message);
            if let Some(h) = &error.hint {
                eprintln!("  {h}");
            }
            eprintln!("  ptyrc store sync, later");
        }
        _ => {}
    }
}

/// A git repository: a work tree (`.git`) or bare (`HEAD`, `objects`).
fn is_git(p: &Path) -> bool {
    p.join(".git").exists() || (p.join("HEAD").is_file() && p.join("objects").is_dir())
}
