// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev`: the tools of an app's author. See `plan/store-model.md`,
//! "Tools".

mod app;
mod new;
mod registry;

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
    /// its manifest declares. Also, as ptyrc-box takes them: --clock ISO,
    /// --zone NAME, --storage DIR, --file NAME=PATH, --probe
    /// git=FILE.json, --pointer T:X,Y, --click T:X,Y, --focus T:TAG,
    /// --paste T:TEXT
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
        /// The unit tests of a script: the exports `test_*` of FILE, a
        /// script of the app [default: tests.js]; no screen, no session.
        /// Status 1 when one fails
        #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "tests.js")]
        unit: Option<String>,
        /// An option of the manifest's [options]; again for each
        #[arg(long = "arg", value_name = "NAME=VALUE")]
        args: Vec<String>,
        /// Permissions declared and denied: answered as the driver
        /// answers a no (lowered, or `denied`)
        #[arg(long, value_name = "NAME,...")]
        deny: Vec<String>,
        /// An event at session time T, to a topic subscribed and held
        #[arg(long = "event", value_name = "T:TOPIC:JSON")]
        events: Vec<String>,
        /// A key of the keyboard at session time T: to the app when it
        /// grabbed it
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
            mut args,
            mut deny,
            events,
            keys,
            unit,
            options,
        } => {
            let (dir, options) = split_dir(dir, options);
            if let Some(file) = unit {
                let dir = here(dir);
                is_dir(&dir)?;
                return app::unit(&dir, &file);
            }
            // Flags of `dev test` after the first option of the app,
            // before a `--`: ours.
            let mut flags: Vec<OsString> = Vec::new();
            let mut json = common.json;
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
                    "--arg" => args.push(value()),
                    "--deny" => deny.push(value()),
                    "--size" | "--for" | "--at" | "--seed" | "--tape" | "--screen" | "--event"
                    | "--key" | "--clock" | "--zone" | "--storage" | "--file" | "--probe"
                    | "--pointer" | "--click" | "--focus" | "--paste" => {
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
            if json {
                flags.push("--json".into());
            }
            let deny = deny.iter().flat_map(|d| d.split(',')).map(str::trim);
            let deny = deny.filter(|d| !d.is_empty()).map(str::to_owned).collect();
            app::test(app::TestOpts {
                dir,
                args,
                deny,
                flags,
            })
        }
        Dev::Run { dir, args } => {
            let (dir, args) = split_dir(dir, args);
            let dir = here(dir);
            is_dir(&dir)?;
            app::run(common, &dir, &args)
        }
        Dev::Entry { dir, remote } => entry(&dir.unwrap_or_else(|| ".".into()), remote),
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
            let mut built = None;
            if rust && problems.is_empty() {
                match app::build_checked(dir, &tree, &ok) {
                    Ok(m) => built = Some(m),
                    Err(e) => problems.push(e),
                }
            }
            if problems.is_empty() {
                let a = &ok.manifest.app;
                let asks: Vec<&str> = (ok.manifest.permissions.iter())
                    .map(|p| p.name.as_str())
                    .collect();
                let asks = match asks.is_empty() {
                    true => "asks no permission".to_string(),
                    false => format!("asks {}", asks.join(", ")),
                };
                println!(
                    "ok: {} {}: {} files, {} bytes; {asks}",
                    a.id,
                    a.version,
                    ok.files.len(),
                    ok.size
                );
                if let Some(m) = built {
                    let size = std::fs::metadata(&m).map_or(0, |m| m.len());
                    eprintln!("ptyrc: built {}, {size} bytes", m.display());
                }
                return Ok(ExitCode::SUCCESS);
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
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
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

fn entry(dir: &Path, remote: Option<String>) -> Result<ExitCode, Fail> {
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
    let dirty = git(
        dir,
        &["status", "--porcelain", "--untracked-files=no", "--", "."],
    )?;
    if !dirty.is_empty() {
        eprintln!(
            "ptyrc: uncommitted changes in {}: the entry names HEAD",
            dir.display()
        );
    }
    print!("# {rel}\n{body}");
    Ok(ExitCode::SUCCESS)
}
