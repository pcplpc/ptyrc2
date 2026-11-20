// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc store`: install, update, list and remove store apps. The work
//! is `ptyrc-store`'s; see `plan/store-model.md`, "Install".

use crate::{Common, Fail, print_json};
use clap::{Args, Subcommand};
use ptyrc_store::build;
use ptyrc_store::index::Index;
use ptyrc_store::install::{self, StoreLock};
use ptyrc_store::registry::{self, Registry};
use ptyrc_store::sync::{self, MAX_AGE, Synced};
use ptyrc_store::{
    Dirs, ErrorKind, Installed, Lock, Origin, Pointer, Reference, Registries, Remote, Repo, Tier,
    Want, check, choose,
};
use serde_json::Value;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;
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
    /// The apps listed whose origin, id or summary holds WORD
    Search { word: String },
    /// Fetch the registries of store.toml named by `url`, now; `add`,
    /// `update` and `search` do it when a copy is a day old
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
    /// (fetched), or with --path a directory read as it is
    Add {
        name: String,
        #[arg(value_name = "URL|DIR")]
        source: String,
        /// Read DIR as it is, not fetched
        #[arg(long)]
        path: bool,
    },
    /// Remove a registry and its copy
    Rm { name: String },
}

#[derive(Args)]
pub struct Add {
    /// `host/owner/repo[#path][@REF]`, `https://...`, or a local
    /// directory (`/dir`, `./dir`, `local:/dir`) holding a git repository
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

/// Picks, fetches, checks, asks, installs. `held`: the entry in use;
/// `hold` (`update`): not for a lower tier unless `--latest`. The caller
/// holds the store lock.
fn obtain(
    dirs: &Dirs,
    origin: &Origin,
    pointer: Option<&Pointer>,
    want: &Want,
    yes: bool,
    held: Option<&Installed>,
    hold: bool,
) -> Result<Outcome, Fail> {
    let pick = choose(pointer, want)?;
    let current = |c: &str| held.is_some_and(|h| h.commit == c);
    // The commit in use, yanked since: not held to, not kept as current.
    let yanked = held.is_some_and(|h| listed_now(pointer, &h.commit) == Some(Now::Yanked));
    if let Reference::Commit(c) = &pick.reference
        && current(c)
    {
        return Ok(Outcome::Current);
    }
    let repo = Repo::open(&dirs.git(origin))?;
    let commit = repo.fetch(&remote_of(origin), &pick.reference)?;
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
    let tree = repo.tree(&commit, origin.sub())?;
    let checked = check(&tree).map_err(|e| {
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
        confirm(origin, &commit, &checked.manifest.app.id, yes)?;
    }
    let module = match checked.manifest.app.entry {
        ptyrc_store::Entry::Rust(_) => Some(build_rust(dirs, origin, &tree, &checked, want.force)?),
        ptyrc_store::Entry::Script(_) => None,
    };
    let entry = install::install_built(
        dirs,
        origin,
        &tree,
        &checked,
        module.as_deref(),
        &commit,
        tier,
        &today(),
    )?;
    install::prune(dirs, origin)?;
    Ok(Outcome::Installed(entry))
}

/// Step 3 of the install for a Rust app: the build, in
/// `<cache>/ptyrc/build/<origin>/`, removed after. `force`: without the
/// OS profile where there is none, said on stderr.
fn build_rust(
    dirs: &Dirs,
    origin: &Origin,
    tree: &dyn ptyrc_store::Tree,
    checked: &ptyrc_store::Checked,
    force: bool,
) -> Result<Vec<u8>, Fail> {
    let toolchain = build::Toolchain::find()?;
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

fn add(dirs: &Dirs, args: Add) -> Result<ExitCode, Fail> {
    let (origin, reference) = parse_spec(&args.spec)?;
    let _lock = lock(dirs)?;
    let registries = Registries::load(dirs)?;
    refresh(&registries, Some(MAX_AGE));
    let want = Want {
        reference,
        latest: args.pick.latest,
        force: args.pick.force,
    };
    let found = lookup(&registries, &origin, &want)?;
    let pointer = found.as_ref().map(|(_, p)| p);
    let held = Lock::load(dirs)?.get(&origin.id()).cloned();
    match obtain(
        dirs,
        &origin,
        pointer,
        &want,
        args.pick.yes,
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

fn rm(dirs: &Dirs, args: Rm) -> Result<ExitCode, Fail> {
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
            v.push(installed(dirs, n)?);
        }
        v
    };
    if entries.is_empty() {
        eprintln!("ptyrc: no store app\n  ptyrc store add ORIGIN");
        return Ok(ExitCode::SUCCESS);
    }
    let registries = Registries::load(dirs)?;
    refresh(&registries, Some(MAX_AGE));
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
            let pointer = found.as_ref().map(|(_, p)| p);
            obtain(
                dirs,
                &origin,
                pointer,
                &want,
                args.pick.yes,
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

/// Fetches the registries by `url` whose copy is older than `max_age`
/// (`None`: all); a failure is a warning. The caller holds the store
/// lock. Returns the number that failed.
fn refresh(registries: &Registries, max_age: Option<Duration>) -> usize {
    let mut failed = 0;
    for r in &registries.list {
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

/// The pointer of the first registry that lists `origin`. A registry
/// with no copy here, before it, may have spoken: refused, unless the
/// version is named (`@REF`, `--latest`).
fn lookup<'a>(
    registries: &'a Registries,
    origin: &Origin,
    want: &Want,
) -> Result<Option<(&'a Registry, Pointer)>, Fail> {
    let present = registries.lookup_present(origin)?;
    if let Some(r) = present.absent.first()
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
    let apps = install::list(dirs)?;
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
    let entry = match installed(dirs, name) {
        Ok(e) => Some(e),
        Err(f) if f.status == 4 => None,
        Err(f) => return Err(f),
    };
    let origin = match &entry {
        Some(e) => Origin::parse(&e.origin)?,
        None => match parse_spec(name) {
            Ok((o, _)) => o,
            Err(_) => {
                return Err(Fail {
                    status: 4,
                    message: format!("{name}: not installed"),
                    hint: Some("ptyrc store ls; or name an origin".into()),
                });
            }
        },
    };
    let registries = Registries::load(dirs)?;
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
                println!("  {} ({}): {}", o.name, o.default.type_name(), o.doc);
            }
        }
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
        refresh(&registries, Some(MAX_AGE));
    }
    let word = word.to_lowercase();
    let lock = Lock::load(dirs)?;
    // origin, best tier, registry, summary
    let mut rows: Vec<(String, Option<Tier>, String, Option<String>)> = Vec::new();
    for r in &registries.list {
        if !r.dir.is_dir() {
            eprintln!("ptyrc: registry {}: no copy here", r.name);
            continue;
        }
        let index = Index::load(r);
        for p in r.pointers()? {
            let origin = p.origin()?.id();
            if rows.iter().any(|row| row.0 == origin) {
                continue;
            }
            let ix = index.as_ref().and_then(|ix| ix.get(&origin));
            let summary = ix.and_then(|a| a.summary.as_deref()).map(plain);
            let id = ix.and_then(|a| a.id.as_deref()).unwrap_or("");
            let hay = format!("{origin}\n{id}\n{}", summary.as_deref().unwrap_or(""));
            if !hay.to_lowercase().contains(&word) {
                continue;
            }
            let best = p
                .versions
                .iter()
                .filter(|v| !v.yanked && !p.yanked)
                .map(|v| v.tier)
                .max();
            rows.push((origin, best, r.name.clone(), summary));
        }
    }
    if json {
        let rows: Vec<Value> = rows
            .iter()
            .map(|(o, t, r, s)| {
                serde_json::json!({
                    "origin": o,
                    "tier": t.map(Tier::as_str),
                    "registry": r,
                    "summary": s,
                    "installed": lock.get(o).is_some(),
                })
            })
            .collect();
        print_json(&Value::Array(rows), false);
    } else {
        let tty = std::io::stdout().is_terminal();
        let wo = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
        for (o, t, r, s) in &rows {
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
                    error
                        .hint
                        .as_deref()
                        .unwrap_or("is the URL that of a git repository?")
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
                let (kind, src) = match &r.url {
                    Some(u) => ("url", u.clone()),
                    None => ("path", r.dir.display().to_string()),
                };
                let state = match (&r.url, sync::stamp(r)) {
                    (None, _) => String::new(),
                    (Some(_), Some((at, c))) if r.dir.is_dir() => {
                        format!("  fetched {} at {}", date_of(at), short(&c))
                    }
                    (Some(_), _) => "  not fetched".into(),
                };
                println!("{}  {kind} {src}{state}", r.name);
            }
            Ok(ExitCode::SUCCESS)
        }
        RegistryVerb::Add { name, source, path } => {
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
                let r = registries.list.last().expect("just added");
                match sync::sync(r, SystemTime::now(), None) {
                    Synced::Fetched { commit } => println!("{name}: fetched {}", short(&commit)),
                    Synced::Stale { error, .. } | Synced::Missing { error } => {
                        eprintln!("ptyrc: warning: {}", error.message);
                        eprintln!("  ptyrc store sync, later");
                    }
                    _ => {}
                }
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
            if r.url.is_some() && r.dir.starts_with(dirs.registry()) {
                let _ = std::fs::remove_dir_all(&r.dir);
            }
            println!("removed registry {name}");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// A git repository: a work tree (`.git`) or bare (`HEAD`, `objects`).
fn is_git(p: &Path) -> bool {
    p.join(".git").exists() || (p.join("HEAD").is_file() && p.join("objects").is_dir())
}
