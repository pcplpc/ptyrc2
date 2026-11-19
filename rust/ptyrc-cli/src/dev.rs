// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev`: the tools of an app's author. See `plan/store-model.md`,
//! "Tools".

mod app;
mod new;

use crate::store::io_text;
use crate::{Common, Fail};
use clap::Subcommand;
use ptyrc_store::manifest::MANIFEST;
use ptyrc_store::{DirTree, Pointer, check_all, install};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[derive(Subcommand)]
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
    /// requests refused, the limits met, the end; the log on stderr. The
    /// same run every time. A Rust app is built first (cargo)
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
        /// A text file as the program's screen under the app
        #[arg(long, value_name = "FILE")]
        screen: Option<PathBuf>,
        /// An option of the manifest's [options]; again for each
        #[arg(long = "arg", value_name = "NAME=VALUE")]
        args: Vec<String>,
    },
    /// Run the app in a live session, as the user's own client; start it
    /// again when a file of DIR changes. The log on stderr; Ctrl-C ends
    Run {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
        /// Options of the manifest's [options]
        #[arg(value_name = "NAME=VALUE")]
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
}

pub fn dev(common: &Common, verb: Dev) -> Result<ExitCode, Fail> {
    let here = |dir: Option<PathBuf>| dir.unwrap_or_else(|| ".".into());
    match verb {
        Dev::New { name, rust } => new::new(&name, rust),
        Dev::Check { dir } => check(&here(dir)),
        Dev::Test {
            dir,
            size,
            duration,
            at,
            seed,
            tape,
            screen,
            args,
        } => {
            let dir = here(dir);
            is_dir(&dir)?;
            let mut flags: Vec<OsString> = Vec::new();
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
            flag("--screen", screen.map(Into::into));
            app::test(app::TestOpts { dir, args, flags })
        }
        Dev::Run { dir, args } => {
            let dir = here(dir);
            is_dir(&dir)?;
            app::run(common, &dir, &args)
        }
        Dev::Entry { dir, remote } => entry(&dir.unwrap_or_else(|| ".".into()), remote),
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
            let supported = install::supported(&ok);
            let rust = matches!(ok.manifest.app.entry, ptyrc_store::Entry::Rust(_));
            match supported {
                // A Rust app: `dev test` builds it here; the store, not yet.
                Err(e) if !rust => vec![e],
                _ => {
                    let a = &ok.manifest.app;
                    println!(
                        "ok: {} {}: {} files, {} bytes",
                        a.id,
                        a.version,
                        ok.files.len(),
                        ok.size
                    );
                    if let Err(e) = supported {
                        eprintln!("ptyrc: note: {}: `ptyrc store add` refuses it", e.message);
                    }
                    return Ok(ExitCode::SUCCESS);
                }
            }
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
