// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev`: the tools of an app's author. See `plan/store-model.md`,
//! "Tools".

use crate::Fail;
use crate::store::io_text;
use clap::Subcommand;
use ptyrc_store::manifest::MANIFEST;
use ptyrc_store::{DirTree, Pointer, check_all, install};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[derive(Subcommand)]
pub enum Dev {
    /// What the install checks, on a directory: every problem, with its
    /// hint; status 1 if any
    Check {
        /// The app's directory [default: .]
        dir: Option<PathBuf>,
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

pub fn dev(verb: Dev) -> Result<ExitCode, Fail> {
    match verb {
        Dev::Check { dir } => check(&dir.unwrap_or_else(|| ".".into())),
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
        Ok(ok) => match install::supported(&ok) {
            Ok(()) => {
                let a = &ok.manifest.app;
                println!(
                    "ok: {} {}: {} files, {} bytes",
                    a.id,
                    a.version,
                    ok.files.len(),
                    ok.size
                );
                return Ok(ExitCode::SUCCESS);
            }
            Err(e) => vec![e],
        },
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
