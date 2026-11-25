// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev`: the tools of an app's author. See `plan/store-model.md`,
//! "Tools".

mod app;
pub mod cases;
mod new;
mod probe;
mod push;
mod registry;
mod vet;

use crate::store::io_text;
use crate::{Common, Fail};
use clap::Subcommand;
use ptyrc_store::manifest::MANIFEST;
use ptyrc_store::{DirTree, Pointer, build, check_all};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Dev {
    /// Write a new app at NAME: the manifest, the app, a test, the
    /// licence, AGENTS.md; a git repository
    New {
        /// The directory to create; its last part is the app's id
        name: PathBuf,
        /// A Rust app, `ptyrc-app` vendored, in place of a script
        #[arg(long)]
        rust: bool,
    },
    /// What the install checks, on a directory: every problem, with its
    /// hint; status 1 if any
    Check {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
    },
    /// Run the app headless: the screen at each time as text, then the
    /// requests refused or lowered, the permissions lacking, the limits
    /// met, the end; the log on stderr. The same run every time. A Rust
    /// app is built first (rustc, as the store builds it). It holds what
    /// its manifest declares for the run: a permission an option needs,
    /// only with that option given. --file NAME=PATH: the file option
    /// NAME (ARG#N: the Nth value of [args], from 0), as --NAME PATH;
    /// [args] todo = { type = "file", min = 1, max = 1, ... }: --file
    /// ARG#0=todo.md. Also, as ptyrc-box takes them: --clock ISO,
    /// --zone NAME, --storage DIR (a file a key: its name the key, its
    /// content the value; read once; --storage-out writes the end),
    /// --probe
    /// git=FILE.json (written by `ptyrc dev probe git`), --pointer T:X,Y, --click T:X,Y, --press T:X,Y,
    /// --release T:X,Y, --focus T:TAG, --paste T:TEXT, --resize T:CxR.
    /// A flag of dev test is its own before `--`; an app's option of the
    /// same name goes after
    #[command(allow_hyphen_values = true)]
    Test {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
        /// Screen size, COLSxROWS [default: 80x24]
        #[arg(long, value_name = "CxR")]
        size: Option<String>,
        /// Session time the run lasts: 1500, 1500ms, 1.5s [default: 1s]
        #[arg(long = "for", value_name = "T")]
        duration: Option<String>,
        /// Times of the screens shown, comma-separated [default: the end]
        #[arg(long, value_name = "T,...")]
        at: Option<String>,
        /// Seed of the random numbers [default: 1]
        #[arg(long, value_name = "N")]
        seed: Option<u64>,
        /// A tape (`ptyrc run --record`) played under the app, in session
        /// time; its size is the screen's
        #[arg(long, value_name = "FILE", conflicts_with_all = ["screen", "size"])]
        tape: Option<PathBuf>,
        /// A text file as the program's screen under the app, from
        /// session time T on [default T: 0]; again for each change
        #[arg(long, value_name = "[T:]FILE")]
        screen: Vec<String>,
        /// Under each screen, a grid of its styles: a letter per style
        /// that is not the default, and the legend
        #[arg(long)]
        styles: bool,
        /// The wall time of each tick and event, its host calls counted
        /// in: a line for each over tick-ms (50 ms), `slow at T ms: tick,
        /// X ms`, `slow at T ms: event TOPIC, X ms`; a last line, `ticks:
        /// N; a tick, wall time: mean X ms, max X ms; events: N, max X
        /// ms` (no event: no `events`). A slow call: status 1. Not in the report by default: it
        /// is not the same twice. --json: always, `ticks`, `tick_ms`,
        /// `events`, `event_ms`, `slow`. Run alone, on a quiet machine,
        /// with the release runner
        #[arg(long)]
        timing: bool,
        /// The storage at the end into DIR, as --storage reads it (a file
        /// a key); DIR then holds that and no other key. A second run:
        /// --storage DIR. May be the DIR of --storage
        #[arg(long, value_name = "DIR")]
        storage_out: Option<PathBuf>,
        /// The unit tests of a script: the exports `test_*` of FILE, a
        /// script of the app [default: tests.js]; no screen, no session;
        /// `asset.*` from `[app] assets`. Status 1 when one fails
        #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "tests.js")]
        unit: Option<String>,
        /// The cases of the tree: each tests/CASE.args, the words after
        /// `ptyrc dev test .` (white space, `#` comments, run in the
        /// tree), its report against tests/CASE.txt; those NAMEd alone.
        /// A line a case, a diff for a failure; status 1 when one fails.
        /// A value of an option refused (status 2, `ptyrc: --NAME: WHY`
        /// on stderr): its report is that line.
        /// The cases run at once, a Rust app built once before. The
        /// canonical runner: the harness and a tree's test.sh call it
        #[arg(long, value_name = "NAME", num_args = 0.., conflicts_with = "unit")]
        cases: Option<Vec<String>>,
        /// With --cases: write each CASE.txt from the run. Read the diff
        /// first
        #[arg(long, requires = "cases")]
        bless: bool,
        /// An option of the manifest's [options]; again for each
        #[arg(long = "arg", value_name = "NAME=VALUE")]
        args: Vec<String>,
        /// Permissions declared and denied: answered as the driver
        /// answers a no (lowered, or `denied`)
        #[arg(long, value_name = "NAME,...")]
        deny: Vec<String>,
        /// An event at session time T, to a topic subscribed and held.
        /// `mouse` names its sprite: `sprite`, the id `draw.put` answered
        /// or the `sprite` of a widget's info (`widget.list`); headless,
        /// ids count from 1 in the order drawn, a widget's sprite with
        /// them. --click T:X,Y needs no id (no wheel)
        #[arg(long = "event", value_name = "T:TOPIC:JSON")]
        events: Vec<String>,
        /// A key of the keyboard at session time T: to the app when it
        /// grabbed it; else to the program, `input {bytes}` to the app
        /// with keys-read
        #[arg(long = "key", value_name = "T:KEY")]
        keys: Vec<String>,
        /// Options of the manifest's [options], as `ptyrc app` takes
        /// them: --NAME VALUE, --NAME, --no-NAME; after `--`, or here
        #[arg(
            value_name = "OPTION",
            trailing_var_arg = true,
            allow_hyphen_values = true
        )]
        options: Vec<String>,
    },
    /// Run the app in a live session, as the user's own client; start it
    /// again when a file of DIR changes. The log on stderr; Ctrl-C ends
    /// it, the app stopped first. The frame told at `init`: --frame,
    /// else `[app] frame`, else `active`, resolved by the session
    Run {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
        /// Options of the manifest's [options], as `ptyrc app` takes
        /// them: --NAME VALUE, --NAME, --no-NAME; NAME=VALUE
        #[arg(
            value_name = "OPTION",
            trailing_var_arg = true,
            allow_hyphen_values = true
        )]
        args: Vec<String>,
    },
    /// Print the registry's pointer file for the app at DIR: the origin
    /// from the git remote, the manifest's path, the head commit as
    /// `untested`
    Entry {
        /// The app's directory, in a git repository [default: .]
        dir: Option<PathBuf>,
        /// The remote that names the origin [default: origin, else the
        /// only one]
        #[arg(long, value_name = "NAME")]
        remote: Option<String>,
    },
    /// Publish the app at DIR to a registry server, no git, no account:
    /// checked, text only, a Rust app built (when rustc is there), the
    /// files the app uses packed (`left out: ...` names the others),
    /// pushed; then `ptyrc store add HOST/NAME` installs it. The first
    /// push of a name answers its token, kept in tokens.toml
    Push(push::PushArgs),
    /// Build the Rust app at DIR as the store does; the module to FILE.
    /// A script app: status 2
    Build {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
        /// The module written
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Built where the build has no OS profile, as `store add --force`
        #[arg(long)]
        force: bool,
    },
    /// What a registry server's builder runs on an archive: the check of
    /// dev check, text only, no file the app does not use, a Rust app
    /// built (the module to FILE). Status 0: valid; 1: the app's fault,
    /// a line each on stderr; 2: usage; 3: not vetted here (no rustc or
    /// its target, no OS profile without --force, an I/O error)
    Vet {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
        /// A Rust app's module, written by a rename; required for one
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Built where the build has no OS profile, as dev build --force
        #[arg(long)]
        force: bool,
    },
    /// Print what the driver's probe answers now, as JSON: a fixture for
    /// `dev test --probe git=FILE.json` (or system=). git: the
    /// repository holding DIR, `days` in UTC (headless --zone by
    /// default); system: a second sample a second after the first
    Probe {
        /// git or system
        #[arg(value_parser = ["git", "system"])]
        what: String,
        /// git: a directory of the repository [default: .]
        dir: Option<PathBuf>,
        /// The parts asked, comma-separated [default: all]
        #[arg(long, value_name = "NAME,...")]
        parts: Option<String>,
        /// git: commits of `log` [default: 30]
        #[arg(long, value_name = "N")]
        log: Option<u64>,
        /// system: processes of `glance` [default: 5]
        #[arg(long, value_name = "N")]
        top: Option<u64>,
    },
    #[command(flatten)]
    Registry(registry::RegistryDev),
}

pub fn dev(common: &Common, verb: Dev) -> Result<ExitCode, Fail> {
    let here = |dir: Option<PathBuf>| dir.unwrap_or_else(|| ".".into());
    match verb {
        Dev::New { name, rust } => new::new(&name, rust),
        Dev::Check { dir } => check(&here(dir)),
        Dev::Registry(v) => registry::registry_dev(common.json, v),
        Dev::Test {
            dir,
            size,
            duration,
            at,
            seed,
            tape,
            screen,
            styles,
            timing,
            mut storage_out,
            mut args,
            mut deny,
            events,
            keys,
            unit,
            cases,
            bless,
            options,
        } => {
            let (dir, options) = split_dir(dir, options);
            if let Some(mut names) = cases {
                // The command takes hyphens as values: `--cases --bless`.
                let bless = bless || names.iter().any(|n| n == "--bless");
                names.retain(|n| n != "--bless");
                let given = names.iter().any(|n| n.starts_with('-'))
                    || size.is_some()
                    || duration.is_some()
                    || at.is_some()
                    || seed.is_some()
                    || tape.is_some()
                    || !screen.is_empty()
                    || styles
                    || timing
                    || storage_out.is_some()
                    || !args.is_empty()
                    || !deny.is_empty()
                    || !events.is_empty()
                    || !keys.is_empty()
                    || !options.is_empty();
                if given {
                    return Err(Fail::usage(
                        "--cases: no other flag, no option",
                        "the words of a case are in its tests/CASE.args",
                    ));
                }
                let dir = here(dir);
                is_dir(&dir)?;
                return cases::all(&dir, &names, bless);
            }
            if let Some(file) = unit {
                let dir = here(dir);
                is_dir(&dir)?;
                return app::unit(&dir, &file);
            }
            // Flags of `dev test` after the first option of the app,
            // before a `--`: ours.
            let mut flags: Vec<OsString> = Vec::new();
            let mut files: Vec<String> = Vec::new();
            let mut json = common.json;
            // The words after a `--` of the command line are the app's,
            // though the parser took the `--` itself.
            let mut options = options;
            let after = std::env::args_os()
                .skip_while(|a| a != "--")
                .skip(1)
                .count();
            if after > 0 && after <= options.len() && !options.contains(&"--".to_string()) {
                options.insert(options.len() - after, "--".into());
            }
            let mut words = options.into_iter();
            while let Some(w) = words.next() {
                let (name, inline) = match w.split_once('=') {
                    Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                    _ => (w.clone(), None),
                };
                let mut value = || inline.clone().or_else(|| words.next()).unwrap_or_default();
                match name.as_str() {
                    "--" => {
                        args.extend(words.by_ref());
                    }
                    "--json" if inline.is_none() => json = true,
                    "--styles" if inline.is_none() => flags.push("--styles".into()),
                    "--timing" if inline.is_none() => flags.push("--timing".into()),
                    "--arg" => args.push(value()),
                    "--storage-out" => {
                        if storage_out.replace(value().into()).is_some() {
                            return Err(Fail::usage(
                                "--storage-out: given twice",
                                "--storage-out once; the app's own after `--`",
                            ));
                        }
                    }
                    "--deny" => deny.push(value()),
                    "--file" => files.push(value()),
                    "--size" | "--for" | "--at" | "--seed" | "--tape" | "--screen" | "--event"
                    | "--key" | "--clock" | "--zone" | "--storage" | "--probe" | "--pointer"
                    | "--click" | "--press" | "--release" | "--focus" | "--paste" | "--resize" => {
                        flags.extend([name.clone().into(), value().into()])
                    }
                    _ => args.push(w),
                }
            }
            let dir = here(dir);
            is_dir(&dir)?;
            let mut flag = |name: &str, v: Option<OsString>| {
                if let Some(v) = v {
                    flags.extend([name.into(), v]);
                }
            };
            flag("--size", size.map(Into::into));
            flag("--for", duration.map(Into::into));
            flag("--at", at.map(Into::into));
            flag("--seed", seed.map(|s| s.to_string().into()));
            flag("--tape", tape.map(Into::into));
            for s in screen {
                flag("--screen", Some(s.into()));
            }

            for e in events {
                flag("--event", Some(e.into()));
            }
            for k in keys {
                flag("--key", Some(k.into()));
            }
            if styles {
                flags.push("--styles".into());
            }
            if timing {
                flags.push("--timing".into());
            }
            if json {
                flags.push("--json".into());
            }
            // A flag of one value given twice: refused, as clap refuses
            // its own (here, or after the app's first option).
            for once in [
                "--size",
                "--for",
                "--at",
                "--seed",
                "--tape",
                "--clock",
                "--zone",
                "--storage",
            ] {
                if flags.iter().filter(|f| *f == once).count() > 1 {
                    return Err(Fail::usage(
                        format!("{once}: given twice"),
                        format!("{once} once; the app's own after `--`"),
                    ));
                }
            }
            let deny = deny.iter().flat_map(|d| d.split(',')).map(str::trim);
            let deny = deny.filter(|d| !d.is_empty()).map(str::to_owned).collect();
            app::test(app::TestOpts {
                dir,
                args,
                deny,
                files,
                flags,
                storage_out,
            })
        }
        Dev::Run { dir, args } => {
            let (dir, args) = split_dir(dir, args);
            let dir = here(dir);
            is_dir(&dir)?;
            app::run(common, &dir, &args)
        }
        Dev::Push(args) => push::push(args),
        Dev::Build { dir, out, force } => push::build_out(&here(dir), &out, force),
        Dev::Vet { dir, out, force } => vet::vet(&here(dir), out.as_deref(), force),
        Dev::Entry { dir, remote } => entry(&dir.unwrap_or_else(|| ".".into()), remote),
        Dev::Probe {
            what,
            dir,
            parts,
            log,
            top,
        } => match what.as_str() {
            "git" if top.is_none() => probe::git(&here(dir), parts.as_deref(), log),
            "system" if dir.is_none() && log.is_none() => probe::system(parts.as_deref(), top),
            _ => Err(Fail::usage(
                format!("dev probe {what}: DIR and --log are of git, --top of system"),
                "ptyrc dev probe git [DIR] [--parts P] [--log N]; ptyrc dev probe system \
                 [--parts P] [--top N]",
            )),
        },
    }
}

/// The directory and the options: a first word that is an option
/// (`--rate`, `rate=1`) is not the directory.
fn split_dir(dir: Option<PathBuf>, mut words: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    // `--` ends the flags of ptyrc; what follows is the app's.
    if words.first().is_some_and(|w| w == "--") {
        words.remove(0);
    }
    match dir {
        Some(d)
            if d.to_str()
                .is_some_and(|d| d.starts_with('-') || d.contains('=')) =>
        {
            words.insert(0, d.to_string_lossy().into_owned());
            (None, words)
        }
        d => (d, words),
    }
}

fn is_dir(dir: &Path) -> Result<(), Fail> {
    match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(Fail::usage(
            format!("{}: not a directory", dir.display()),
            "the app's directory, holding ptyrc-app.toml",
        )),
        Err(e) => Err(Fail {
            status: 4,
            message: format!("{}: {}", dir.display(), io_text(&e)),
            hint: Some("the app's directory, holding ptyrc-app.toml".into()),
        }),
    }
}

/// `[app] random`: each draw read as `ptyrc app` reads it; a problem
/// each that does not. The bare draw (`random` absent: `[[]]`) that
/// does not read is never drawn (the menu skips it): a note.
fn draws(ok: &ptyrc_store::Checked, dir: &Path) -> (Vec<ptyrc_store::Error>, Vec<String>) {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let bare = ok.manifest.app.random == [Vec::<String>::new()];
    for draw in &ok.manifest.app.random {
        let read = ptyrc_store::options::read(&ok.manifest, draw, dir).and_then(|l| {
            let l = l.unwrap_or_default();
            let args = serde_json::Value::Object(l.args);
            ptyrc_store::options::resolve(&ok.manifest, Some(&args), &l.files).map(drop)
        });
        let Err(e) = read else { continue };
        if bare {
            notes.push(format!(
                "[app] random: the bare draw (random absent: [[]]) is not a command \
                 line ({}): never drawn; random = [] says so, or draws that read",
                e.message
            ));
        } else {
            let message = format!("[app] random: {draw:?}: {}", e.message);
            problems.push(ptyrc_store::Error {
                message,
                hint: Some(e.hint.unwrap_or_else(|| {
                    "each draw: words of the app's options, as ptyrc app takes them; \
                     random = [] never drawn"
                        .into()
                })),
                ..e
            });
        }
    }
    (problems, notes)
}

fn check(dir: &Path) -> Result<ExitCode, Fail> {
    is_dir(dir)?;
    let problems = match check_all(&DirTree::new(dir)) {
        Ok(ok) => {
            let tree = DirTree::new(dir);
            let rust = matches!(ok.manifest.app.entry, ptyrc_store::Entry::Rust(_));
            // A Rust app: the rule, then the build of the store.
            let mut problems = match rust {
                true => build::sources(&tree, &ok),
                false => Vec::new(),
            };
            let todos = ptyrc_store::manifest::todos(&ok.manifest);
            let mut notes: Vec<String> = (todos.iter())
                .map(|t| format!("{t}: starts with TODO; the install refuses it"))
                .collect();
            let (bad, said) = draws(&ok, dir);
            problems.extend(bad);
            notes.extend(said);
            let mut built = None;
            if rust && problems.is_empty() {
                match app::build_checked(dir, &tree, &ok) {
                    Ok(m) => built = Some(m),
                    Err(e) => problems.push(e),
                }
            }
            if problems.is_empty() {
                let a = &ok.manifest.app;
                // A permission tied to an option: asked only with it.
                let asks: Vec<String> = (ok.manifest.permissions.iter())
                    .map(|p| {
                        let with = ptyrc_store::options::asked_with(&ok.manifest, &p.name);
                        match with.is_empty() {
                            true => p.name.clone(),
                            false => format!("{} (with {})", p.name, with.join(" or ")),
                        }
                    })
                    .collect();
                let asks = match asks.is_empty() {
                    true => "asks no permission".to_string(),
                    false => format!("asks {}", asks.join(", ")),
                };
                // A note, status 0; the install refuses it: said here.
                let todo = match todos.is_empty() {
                    true => String::new(),
                    false => format!("; the install refuses it: TODO in {}", todos.join(", ")),
                };
                println!(
                    "ok: {} {}: {} files, {} bytes{todo}; {asks}",
                    a.id,
                    a.version,
                    ok.files.len(),
                    ok.size
                );
                for n in &notes {
                    eprintln!("ptyrc: note: {n}");
                }
                if let Some(m) = built {
                    let size = std::fs::metadata(&m).map_or(0, |m| m.len());
                    eprintln!("ptyrc: built {}, {size} bytes", m.display());
                }
                return Ok(ExitCode::SUCCESS);
            }
            for n in &notes {
                eprintln!("ptyrc: note: {n}");
            }
            problems
        }
        Err(v) => v,
    };
    for e in &problems {
        eprintln!("ptyrc: {}", e.message);
        if let Some(h) = &e.hint {
            eprintln!("  {h}");
        }
    }
    let n = problems.len();
    eprintln!(
        "ptyrc: {}: {n} problem{}",
        dir.display(),
        if n == 1 { "" } else { "s" }
    );
    Ok(ExitCode::from(1))
}

/// `git -C dir ARGS`, its stdout trimmed; the user's own repository and
/// configuration.
fn git(dir: &Path, args: &[&str]) -> Result<String, Fail> {
    Ok(git_raw(dir, args)?.trim().to_string())
}

/// `git -C dir ARGS`, its stdout as it is.
fn git_raw(dir: &Path, args: &[&str]) -> Result<String, Fail> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Fail {
            status: 127,
            message: format!("git: {}", io_text(&e)),
            hint: Some("install git".into()),
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err: String = err.chars().filter(|c| !c.is_control()).collect();
        return Err(Fail {
            status: 2,
            message: format!("git {}: {}", args[0], err.trim()),
            hint: Some(format!("{} in a git repository", dir.display())),
        });
    }
    String::from_utf8(out.stdout).map_err(|_| Fail::from(format!("git {}: not UTF-8", args[0])))
}

/// A remote URL as the https URL of the origin: `https://`, `http://`,
/// `ssh://`, `git://` and `user@host:path`; `.git` and a port dropped.
pub fn https_url(url: &str) -> Option<String> {
    let rest = ["https://", "http://", "ssh://", "git+ssh://", "git://"]
        .iter()
        .find_map(|p| url.strip_prefix(p));
    let (host, path) = match rest {
        Some(r) => {
            let (auth, path) = r.split_once('/')?;
            let host = auth.rsplit_once('@').map_or(auth, |(_, h)| h);
            let host = host.split_once(':').map_or(host, |(h, _)| h);
            (host, path)
        }
        None => {
            // scp form: `[user@]host:path`, no `/` before the `:`.
            let (auth, path) = url.split_once(':')?;
            if auth.contains('/') {
                return None;
            }
            (auth.rsplit_once('@').map_or(auth, |(_, h)| h), path)
        }
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("https://{}/{path}", host.to_ascii_lowercase()))
}

/// The app at `dir` as its repository names it.
pub(crate) struct GitApp {
    /// The remote that names the origin.
    remote: String,
    /// The pointer file of HEAD, `untested`, and its path in a registry.
    pointer: Pointer,
    body: String,
    rel: String,
    commit: String,
    /// The manifest's path in the repository.
    manifest: String,
}

/// The origin from the git remote (`remote`, else `origin`, else the
/// only one), the head commit, the manifest's path.
fn git_app(dir: &Path, remote: Option<String>) -> Result<GitApp, Fail> {
    is_dir(dir)?;
    if !dir.join(MANIFEST).is_file() {
        return Err(Fail {
            status: 4,
            message: format!("{}: no {MANIFEST}", dir.display()),
            hint: Some("the app's directory; `ptyrc dev check DIR`".into()),
        });
    }
    let top = PathBuf::from(git(dir, &["rev-parse", "--show-toplevel"])?);
    let commit = git(dir, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let name = match remote {
        Some(r) => r,
        None => {
            let names = git(dir, &["remote"])?;
            let names: Vec<&str> = names.lines().collect();
            match names[..] {
                _ if names.contains(&"origin") => "origin".into(),
                [one] => one.into(),
                [] => {
                    return Err(Fail::usage(
                        "no git remote",
                        "the repository others fetch: git remote add origin https://...",
                    ));
                }
                _ => {
                    return Err(Fail::usage(
                        format!("several remotes: {}", names.join(", ")),
                        "--remote NAME",
                    ));
                }
            }
        }
    };
    let url = git(dir, &["remote", "get-url", "--", &name])?;
    let origin = https_url(&url).ok_or_else(|| {
        Fail::usage(
            format!("remote {name}: {url}: not a URL of a host"),
            "https://host/owner/repo, or git@host:owner/repo",
        )
    })?;
    let abs = |p: &Path| std::fs::canonicalize(p).map_err(|e| Fail::from(io_text(&e)));
    let (top, here) = (abs(&top)?, abs(dir)?);
    let sub = here.strip_prefix(&top).map_err(|_| {
        Fail::from(format!(
            "{}: outside its repository {}",
            here.display(),
            top.display()
        ))
    })?;
    let manifest = match sub.to_str() {
        Some("") => MANIFEST.to_string(),
        Some(s) => format!("{s}/{MANIFEST}"),
        None => return Err(Fail::from(format!("{}: not UTF-8", sub.display()))),
    };
    let body = format!(
        "origin = \"{origin}\"\nmanifest = \"{manifest}\"\n\n[[version]]\ncommit = \"{commit}\"\ntier = \"untested\"\n"
    );
    let pointer = Pointer::parse("entry", &body)?;
    let rel = pointer.origin()?.registry_path().expect("an https origin");
    Ok(GitApp {
        remote: name,
        pointer,
        body,
        rel,
        commit,
        manifest,
    })
}

/// The uncommitted changes under `dir`, tracked files only.
fn dirty(dir: &Path) -> Result<bool, Fail> {
    let out = git(
        dir,
        &["status", "--porcelain", "--untracked-files=no", "--", "."],
    )?;
    Ok(!out.is_empty())
}

fn entry(dir: &Path, remote: Option<String>) -> Result<ExitCode, Fail> {
    let GitApp { body, rel, .. } = git_app(dir, remote)?;
    if dirty(dir)? {
        eprintln!(
            "ptyrc: uncommitted changes in {}: the entry names HEAD",
            dir.display()
        );
    }
    print!("# {rel}\n{body}");
    Ok(ExitCode::SUCCESS)
}
