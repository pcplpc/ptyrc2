// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev index`, `ptyrc dev registry-check`: the tools of a
//! registry's maintainers, run by its CI. See `plan/store-model.md`,
//! "Registry".

use crate::store::{io_text, remote_of};
use crate::{Fail, print_json};
use clap::Subcommand;
use ptyrc_store::index;
use ptyrc_store::review::{Maintainers, review};
use ptyrc_store::{Dirs, Origin, Reference, Registry, Repo, check_all, page};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Subcommand)]
pub enum RegistryDev {
    /// Build a registry's index.json: each pointer of DIR/apps, each
    /// listed commit fetched and checked, its manifest read; with
    /// --html, the static page from it
    Index {
        /// The registry's directory, holding apps/
        dir: PathBuf,
        /// Where index.json goes [default: stdout]
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Where the page goes
        #[arg(long, value_name = "FILE")]
        html: Option<PathBuf>,
        /// The page's title
        #[arg(long, default_value = "ptyrc store")]
        title: String,
    },
    /// The registry's check of a change, BASE to HEAD: pointer files
    /// only, each where its origin says; each new commit passes `dev
    /// check`; a tier above untested set by one who may. Status 1 if a
    /// problem
    RegistryCheck {
        /// The commit merged into, as git names it
        base: String,
        /// The commit proposed
        head: String,
        /// Who proposes the change
        #[arg(long)]
        author: String,
        /// Who may set a tier: `NAME [tested|approved]` per line
        #[arg(long, value_name = "FILE")]
        maintainers: Option<PathBuf>,
        /// The registry's git repository [default: .]
        #[arg(long, value_name = "DIR")]
        repo: Option<PathBuf>,
    },
}

pub fn registry_dev(json: bool, verb: RegistryDev) -> Result<ExitCode, Fail> {
    match verb {
        RegistryDev::Index {
            dir,
            out,
            html,
            title,
        } => build_index(&dir, out.as_deref(), html.as_deref(), &title),
        RegistryDev::RegistryCheck {
            base,
            head,
            author,
            maintainers,
            repo,
        } => registry_check(
            json,
            &base,
            &head,
            &author,
            maintainers.as_deref(),
            repo.as_deref().unwrap_or(Path::new(".")),
        ),
    }
}

fn write(p: &Path, text: &str) -> Result<(), Fail> {
    std::fs::write(p, text).map_err(|e| Fail::from(format!("{}: {}", p.display(), io_text(&e))))
}

fn build_index(
    dir: &Path,
    out: Option<&Path>,
    html: Option<&Path>,
    title: &str,
) -> Result<ExitCode, Fail> {
    if !dir.join("apps").is_dir() {
        return Err(Fail {
            status: 4,
            message: format!("{}: no apps/", dir.display()),
            hint: Some("the registry's directory".into()),
        });
    }
    let dirs = Dirs::from_env()?;
    let reg = Registry {
        name: "index".into(),
        dir: dir.to_path_buf(),
        url: None,
        api: None,
    };
    let ix = index::build(&dirs, &reg, &remote_of)?;
    for a in &ix.apps {
        for v in &a.versions {
            if let Some(e) = &v.error {
                eprintln!("ptyrc: warning: {} at {}: {e}", a.origin, v.commit);
            }
        }
    }
    let json = ix.to_json();
    match out {
        Some(p) => write(p, &json)?,
        None => print!("{json}"),
    }
    if let Some(p) = html {
        write(p, &page::render(&ix, title))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn registry_check(
    json: bool,
    base: &str,
    head: &str,
    author: &str,
    maintainers: Option<&Path>,
    repo: &Path,
) -> Result<ExitCode, Fail> {
    let git_dir = match repo.join(".git") {
        g if g.is_dir() => g,
        _ if repo.join("HEAD").is_file() => repo.to_path_buf(),
        _ => {
            return Err(Fail {
                status: 4,
                message: format!("{}: not a git repository", repo.display()),
                hint: Some("--repo DIR: the registry's repository".into()),
            });
        }
    };
    let maintainers = match maintainers {
        Some(p) => {
            let src = std::fs::read_to_string(p)
                .map_err(|e| Fail::from(format!("{}: {}", p.display(), io_text(&e))))?;
            Maintainers::parse(&src)?
        }
        None => Maintainers::default(),
    };
    let reg = Repo::open(&git_dir)?;
    let (b, h) = (reg.commit(base)?, reg.commit(head)?);
    let (bt, ht) = (reg.tree(&b, None)?, reg.tree(&h, None)?);
    let dirs = Dirs::from_env()?;
    let mut fetch = |o: &Origin, c: &str| -> Result<(), Vec<ptyrc_store::Error>> {
        let one = || -> ptyrc_store::Result<ptyrc_store::GitTree> {
            let r = Repo::open(&dirs.git(o))?;
            let got = r.fetch(&remote_of(o), &Reference::Commit(c.to_string()))?;
            r.tree(&got, o.sub())
        };
        let tree = one().map_err(|e| vec![e])?;
        check_all(&tree).map(|_| ())
    };
    let r = review(&bt, &ht, author, &maintainers, &mut fetch)?;
    if json {
        let problems: Vec<_> = r
            .problems
            .iter()
            .map(|e| serde_json::json!({"message": e.message, "hint": e.hint}))
            .collect();
        let checked: Vec<_> = r
            .checked
            .iter()
            .map(|(o, c)| serde_json::json!({"origin": o, "commit": c}))
            .collect();
        print_json(
            &serde_json::json!({"ok": r.problems.is_empty(), "checked": checked, "problems": problems}),
            false,
        );
    } else {
        for (o, c) in &r.checked {
            println!("checked {o} at {c}");
        }
        for e in &r.problems {
            eprintln!("ptyrc: {}", e.message);
            if let Some(h) = &e.hint {
                eprintln!("  {h}");
            }
        }
    }
    let n = r.problems.len();
    if n == 0 {
        if !json {
            println!("ok: {} to {}", &b[..12], &h[..12]);
        }
        return Ok(ExitCode::SUCCESS);
    }
    if !json {
        eprintln!("ptyrc: {n} problem{}", if n == 1 { "" } else { "s" });
    }
    Ok(ExitCode::from(1))
}
