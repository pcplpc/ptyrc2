// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc store`: install, update, list and remove store apps. The work
//! is `ptyrc-store`'s; see `plan/store-model.md`, "Install".

use crate::{Common, Fail, print_json};
use clap::{Args, Subcommand};
use ptyrc_store::install::{self, StoreLock};
use ptyrc_store::registry::{self, read_pointer};
use ptyrc_store::{
    Dirs, ErrorKind, Installed, Lock, Origin, Pointer, Reference, Registries, Remote, Repo, Tier,
    Want, check, choose,
};
use serde_json::Value;
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// The apps of the registries present here whose origin holds WORD
    Search { word: String },
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
    /// Take a yanked version
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
fn remote_of(origin: &Origin) -> Remote {
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
    if let Reference::Commit(c) = &pick.reference
        && current(c)
    {
        return Ok(Outcome::Current);
    }
    let repo = Repo::open(&dirs.git(origin))?;
    let commit = repo.fetch(&remote_of(origin), &pick.reference)?;
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
        && tier < h.tier
        && !want.latest
    {
        return Ok(Outcome::Held { found: tier });
    }
    install::supported(&checked)?;
    if tier == Tier::Untested {
        confirm(origin, &commit, &checked.manifest.app.id, yes)?;
    }
    let entry = install::install(dirs, origin, &tree, &checked, &commit, tier, &today())?;
    install::prune(dirs, origin)?;
    Ok(Outcome::Installed(entry))
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
    let found = registries.lookup(&origin)?;
    let pointer = found.as_ref().map(|(_, p)| p);
    let want = Want {
        reference,
        latest: args.pick.latest,
        force: args.pick.force,
    };
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
    let mut failed = None;
    for held in &entries {
        let origin = Origin::parse(&held.origin)?;
        let one = || -> Result<Outcome, Fail> {
            let found = registries.lookup(&origin)?;
            let want = Want {
                reference: None,
                latest: args.pick.latest,
                force: args.pick.force,
            };
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

fn ls(dirs: &Dirs, json: bool) -> Result<ExitCode, Fail> {
    let apps = install::list(dirs)?;
    if json {
        let rows = serde_json::to_value(&apps).unwrap_or(Value::Null);
        print_json(&rows, false);
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
    for a in &apps {
        println!(
            "{:wo$}  {:wi$}  {:wv$}  {:12}  {}",
            a.origin,
            a.id,
            a.version,
            short(&a.commit),
            tier_word(a.tier, tty)
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The first registry present here that lists `origin`; those not
/// fetched are passed over.
fn listed(dirs: &Dirs, origin: &Origin) -> Result<Option<(String, Pointer)>, Fail> {
    let Some(rel) = origin.registry_path() else {
        return Ok(None);
    };
    for r in Registries::load(dirs)?.list {
        if r.dir.is_dir()
            && let Some(p) = read_pointer(&r.dir, &rel, origin)?
        {
            return Ok(Some((r.name, p)));
        }
    }
    Ok(None)
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
    let found = listed(dirs, &origin)?;
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
        let yanked = if p.yanked { ", yanked" } else { "" };
        println!("listed in    {registry}{yanked}");
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
            "[[registry]] name = \"main\", path = \"/dir\" (or url = \"https://...\")",
        ));
    }
    let word = word.to_lowercase();
    let lock = Lock::load(dirs)?;
    let mut rows = Vec::new();
    for r in &registries.list {
        if !r.dir.is_dir() {
            eprintln!(
                "ptyrc: registry {}: not fetched ({})",
                r.name,
                r.dir.display()
            );
            continue;
        }
        for p in r.pointers()? {
            let origin = p.origin()?.id();
            if !origin.to_lowercase().contains(&word) {
                continue;
            }
            let best = p
                .versions
                .iter()
                .filter(|v| !v.yanked && !p.yanked)
                .map(|v| v.tier)
                .max();
            rows.push((origin, best, r.name.clone()));
        }
    }
    if json {
        let rows: Vec<Value> = rows
            .iter()
            .map(|(o, t, r)| {
                serde_json::json!({
                    "origin": o,
                    "tier": t.map(Tier::as_str),
                    "registry": r,
                    "installed": lock.get(o).is_some(),
                })
            })
            .collect();
        print_json(&Value::Array(rows), false);
    } else {
        let tty = std::io::stdout().is_terminal();
        let wo = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
        for (o, t, r) in &rows {
            let cell = format!("{:8}", t.map_or("-", Tier::as_str));
            let cell = match t {
                Some(Tier::Untested) if tty => format!("\x1b[31m{cell}\x1b[0m"),
                _ => cell,
            };
            let mark = if lock.get(o).is_some() {
                "  installed"
            } else {
                ""
            };
            println!("{o:wo$}  {cell}  {r}{mark}");
        }
    }
    if rows.is_empty() {
        eprintln!("ptyrc: no app matches {word:?}");
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
