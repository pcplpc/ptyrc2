// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev push`: an app to a registry server (`registry/API.md`);
//! `ptyrc dev build`: a Rust app's module, as the store builds it.

use super::{GitApp, dirty, git, git_app, git_raw, is_dir};
use crate::Fail;
use crate::store::io_text;
use clap::Args;
use ptyrc_store::install::StoreLock;
use ptyrc_store::push::{self, Pushed};
use ptyrc_store::registry::Registry;
use ptyrc_store::sync::{self, Synced};
use ptyrc_store::{
    Checked, DirTree, Dirs, Entry, ErrorKind, Origin, Registries, Subset, build, check_all,
    sha256_hex, tar, tokens, tree,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The bound on a description, characters: one line of the registry's
/// page on a 375px screen.
const MAX_DESCRIPTION: usize = 64;

#[derive(Args)]
pub struct PushArgs {
    /// The app's directory [default: .]
    dir: Option<PathBuf>,
    /// The commit at HEAD of the app's repository, pushed there first: its
    /// origin from the git remote, not an archive
    #[arg(long)]
    git: bool,
    /// --git: the remote that names the origin [default: origin, else the
    /// only one]
    #[arg(long, value_name = "NAME", requires = "git")]
    remote: Option<String>,
    /// The registry server of store.toml [default: the first `api`]
    #[arg(long, value_name = "NAME")]
    registry: Option<String>,
    /// What the registry shows [default: the manifest's summary]; 64
    /// characters at most
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// The name's token [default: the one tokens.toml holds]
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,
    /// Print the token the registry answers to a first push
    #[arg(long)]
    show_token: bool,
    /// Check, build and pack, print what is left out, the revision and
    /// the size; send nothing
    #[arg(long)]
    dry_run: bool,
}

/// Every problem of `errors` on stderr, then the count; status 1.
pub(super) fn report(dir: &Path, errors: &[ptyrc_store::Error]) -> ExitCode {
    for e in errors {
        eprintln!("ptyrc: {}", e.message);
        if let Some(h) = &e.hint {
            eprintln!("  {h}");
        }
    }
    let n = errors.len();
    eprintln!(
        "ptyrc: {}: {n} problem{}",
        dir.display(),
        if n == 1 { "" } else { "s" }
    );
    ExitCode::from(1)
}

/// The tree at `dir` as the install checks it; a Rust app, its sources
/// by the rule of the build; `todo`: a `TODO` of the template refused.
fn checked(dir: &Path, todo: bool) -> Result<Checked, ExitCode> {
    let tree = DirTree::new(dir);
    let ok = check_all(&tree).map_err(|v| report(dir, &v))?;
    let mut problems = match ok.manifest.app.entry {
        Entry::Rust(_) => build::sources(&tree, &ok),
        Entry::Script(_) => Vec::new(),
    };
    if todo {
        for t in ptyrc_store::manifest::todos(&ok.manifest) {
            problems.push(
                ptyrc_store::Error::new(
                    ErrorKind::Manifest,
                    format!("{t}: starts with TODO; the install refuses it"),
                )
                .with_hint("replace each TODO of the template"),
            );
        }
    }
    match problems.is_empty() {
        true => Ok(ok),
        false => Err(report(dir, &problems)),
    }
}

/// The registry server pushed to: `name`, else the first `api` of
/// store.toml.
fn server<'a>(
    dirs: &Dirs,
    registries: &'a Registries,
    name: Option<&str>,
) -> Result<&'a Registry, Fail> {
    let store = dirs.store();
    let found = match name {
        Some(n) => {
            let r = registries.list.iter().find(|r| r.name == n);
            let r = r.ok_or_else(|| Fail {
                status: 4,
                message: format!("registry {n}: not in {}", store.display()),
                hint: Some("ptyrc store registry ls".into()),
            })?;
            if r.api.is_none() {
                return Err(Fail::usage(
                    format!("registry {n}: not a registry server"),
                    "one added with --api: ptyrc store registry add NAME URL --api",
                ));
            }
            r
        }
        None => registries
            .list
            .iter()
            .find(|r| r.api.is_some())
            .ok_or_else(|| {
                Fail::usage(
                    format!("no registry server in {}", store.display()),
                    "ptyrc store registry add NAME https://host --api",
                )
            })?,
    };
    Ok(found)
}

/// A refusal of the server, a 401 said plainly.
fn refused(name: &str, e: ptyrc_store::Error) -> Fail {
    // `http` says `URL: STATUS: what the server said`.
    if e.message.contains(": 401: ") {
        return Fail {
            status: 2,
            message: format!(
                "{name}: refused ({}): the name is someone else's, or its token is on another machine",
                e.message
            ),
            hint: Some(
                "another [app] id in ptyrc-app.toml; or --token TOKEN, from the tokens.toml of the first push".into(),
            ),
        };
    }
    Fail::from(e)
}

pub fn push(args: PushArgs) -> Result<ExitCode, Fail> {
    let dir = args.dir.clone().unwrap_or_else(|| ".".into());
    is_dir(&dir)?;
    let ok = match checked(&dir, true) {
        Ok(c) => c,
        Err(status) => return Ok(status),
    };
    let app = &ok.manifest.app;
    let description = args.description.as_deref().unwrap_or(&app.summary);
    let n = description.chars().count();
    if n > MAX_DESCRIPTION {
        return Err(Fail::usage(
            match &args.description {
                Some(_) => format!("--description: {n} characters"),
                None => format!("the summary, the default description: {n} characters"),
            },
            format!("{MAX_DESCRIPTION} at most; --description TEXT"),
        ));
    }
    let dirs = Dirs::from_env()?;
    let registries = Registries::load(&dirs)?;
    let target = match (args.dry_run, &args.registry) {
        // A dry run names a registry only when asked to.
        (true, None) => registries.list.iter().find(|r| r.api.is_some()),
        _ => Some(server(&dirs, &registries, args.registry.as_deref())?),
    };
    if args.git {
        return push_git(&args, &dir, &ok, description, &dirs, target);
    }
    let (packed, left) = match pack_used(&dir, &ok) {
        Ok(p) => p,
        Err(status) => return Ok(status),
    };
    if !left.is_empty() {
        println!("left out: {}", left.join(", "));
    }
    let archive = packed;
    let rev = sha256_hex(&archive);
    let host = target.and_then(Registry::api_host);
    let id = match &host {
        Some(h) => Origin::parse(&format!("{h}/{}", app.id))?.id(),
        None => app.id.clone(),
    };
    if args.dry_run {
        println!(
            "{id} {}: revision {rev}, {} bytes; nothing sent",
            app.version,
            archive.len()
        );
        return Ok(ExitCode::SUCCESS);
    }
    let r = target.expect("a registry when not a dry run");
    let (base, host) = (r.api.as_deref().expect("api"), host.expect("api host"));
    let token = match &args.token {
        Some(t) => Some(t.clone()),
        None => tokens::get(&dirs, &host, &app.id)?,
    };
    let pushed = push::push_tar(base, &archive, Some(description), token.as_deref())
        .map_err(|e| refused(&id, e))?;
    keep_token(&dirs, &host, &pushed, args.show_token)?;
    resync(&dirs, r);
    println!(
        "pushed {id} {} at {}, {}, {} bytes{}",
        app.version,
        short(&pushed.rev),
        pushed.tier,
        archive.len(),
        if pushed.created {
            ""
        } else {
            "; listed already"
        }
    );
    println!("others install it: ptyrc store add {id}");
    Ok(ExitCode::SUCCESS)
}

/// The archive of the files the app uses ([`tree::used`]), and those
/// left out. Text only. A Rust app: built when `rustc` is there (what
/// it read is what is used); else every file under the entry's
/// directory, said. The subset packed passes the check.
fn pack_used(dir: &Path, ok: &Checked) -> Result<(Vec<u8>, Vec<String>), ExitCode> {
    let tree = DirTree::new(dir);
    let fail = |e: ptyrc_store::Error| report(dir, &[e]);
    let bad = tree::text_only(&tree, ok);
    if !bad.is_empty() {
        return Err(report(dir, &bad));
    }
    let mut read = None;
    if matches!(ok.manifest.app.entry, Entry::Rust(_)) {
        match build::Toolchain::find() {
            Ok(t) => read = Some(build_temp(dir, ok, &t, true).map_err(fail)?.sources),
            Err(e) => eprintln!(
                "ptyrc: note: not built ({}): every file under the entry's directory packed; \
                 the registry builds it and refuses a file it does not read",
                e.message
            ),
        }
    }
    let mut keep = tree::used(&tree, ok, read.as_deref()).map_err(fail)?;
    let left: Vec<String> = (ok.files.iter())
        .filter(|f| keep.binary_search(&f.path).is_err())
        .map(|f| f.path.clone())
        .collect();
    keep.extend(ok.manifest.app.assets.clone());
    let view = Subset::new(&tree, keep);
    let packed = check_all(&view).map_err(|v| report(dir, &v))?;
    let archive = tar::pack(&view, &packed).map_err(fail)?;
    Ok((archive, left))
}

fn short(rev: &str) -> &str {
    &rev[..rev.len().min(12)]
}

/// A token answered: written to tokens.toml, said once; printed only
/// with `show`.
fn keep_token(dirs: &Dirs, host: &str, pushed: &Pushed, show: bool) -> Result<(), Fail> {
    let Some(t) = &pushed.token else {
        return Ok(());
    };
    tokens::set(dirs, host, &pushed.name, t)?;
    eprintln!(
        "ptyrc: the token of {} written to {}: every later push wants it",
        pushed.name,
        dirs.tokens().display()
    );
    if show {
        println!("token {t}");
    }
    Ok(())
}

/// The copy of `r` fetched again, the push in it; a failure is a
/// warning.
fn resync(dirs: &Dirs, r: &Registry) {
    let Ok(_lock) = StoreLock::take(dirs, || {
        eprintln!("ptyrc: another ptyrc is installing; waiting")
    }) else {
        return;
    };
    if let Synced::Stale { error, .. } | Synced::Missing { error } =
        sync::sync(r, std::time::SystemTime::now(), None)
    {
        eprintln!("ptyrc: warning: {}", error.message);
        eprintln!("  ptyrc store sync, later");
    }
}

/// `--git`: HEAD of the repository holding `dir`, clean and on its
/// remote.
fn push_git(
    args: &PushArgs,
    dir: &Path,
    ok: &Checked,
    description: &str,
    dirs: &Dirs,
    target: Option<&Registry>,
) -> Result<ExitCode, Fail> {
    let GitApp {
        remote,
        pointer,
        commit,
        manifest,
        ..
    } = git_app(dir, args.remote.clone())?;
    let origin = pointer.origin()?;
    let first = "commit and push it first";
    if dirty(dir)? {
        return Err(Fail::usage(
            format!(
                "{}: uncommitted changes; HEAD is what is pushed",
                dir.display()
            ),
            first,
        ));
    }
    let on = git(dir, &["branch", "-r", "--contains", "HEAD"])?;
    let prefix = format!("{remote}/");
    if !on.lines().any(|l| l.trim().starts_with(&prefix)) {
        return Err(Fail::usage(
            format!(
                "{}: not on remote {remote} (as last fetched)",
                short(&commit)
            ),
            format!("git push {remote}; others fetch it from there"),
        ));
    }
    let text = git_raw(dir, &["show", &format!("{commit}:{manifest}")])?;
    let id = origin.id();
    if args.dry_run {
        println!(
            "{id} {}: commit {commit}; nothing sent",
            ok.manifest.app.version
        );
        return Ok(ExitCode::SUCCESS);
    }
    let r = target.expect("a registry when not a dry run");
    let (base, host) = (
        r.api.as_deref().expect("api"),
        r.api_host().expect("api host"),
    );
    let token = match &args.token {
        Some(t) => Some(t.clone()),
        None => tokens::get(dirs, &host, &id)?,
    };
    let pushed = push::push_git(
        base,
        &origin,
        &commit,
        &text,
        &manifest,
        Some(description),
        token.as_deref(),
    )
    .map_err(|e| refused(&id, e))?;
    keep_token(dirs, &host, &pushed, args.show_token)?;
    resync(dirs, r);
    println!(
        "pushed {id} {} at {}, {}{}",
        ok.manifest.app.version,
        short(&pushed.rev),
        pushed.tier,
        if pushed.created {
            ""
        } else {
            "; listed already"
        }
    );
    println!("others install it: ptyrc store add {id}");
    Ok(ExitCode::SUCCESS)
}

/// The store's build of the checked tree at `dir`, in a new 0700
/// directory of the system's temporary one, removed after; `rustc`'s
/// messages and the profile said on stderr.
pub(super) fn build_temp(
    dir: &Path,
    ok: &Checked,
    toolchain: &build::Toolchain,
    force: bool,
) -> ptyrc_store::Result<build::Built> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let base = std::env::temp_dir().join(format!("ptyrc-build-{}-{nanos}", std::process::id()));
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .map_err(|e| {
                ptyrc_store::Error::new(
                    ErrorKind::Io,
                    format!("{}: {}", base.display(), io_text(&e)),
                )
                .with_hint("a writable TMPDIR for the build")
            })?;
    }
    let options = build::Options {
        unconfined: force,
        ..build::Options::default()
    };
    eprintln!(
        "ptyrc: building {} with rustc {}",
        ok.manifest.app.id, toolchain.release
    );
    let built = build::build(
        &DirTree::new(dir),
        ok,
        &base.join("work"),
        toolchain,
        &options,
    );
    let _ = std::fs::remove_dir_all(&base);
    let built = built?;
    if !built.messages.trim().is_empty() {
        eprintln!("{}", built.messages.trim_end());
    }
    match &built.missing {
        Some(why) => eprintln!("ptyrc: warning: built without an OS profile ({why}), by --force"),
        None => eprintln!(
            "ptyrc: built under the profile: {}",
            built.profile.join("; ")
        ),
    }
    Ok(built)
}

/// `bytes` to `out` by a rename of a file beside it.
pub(super) fn write_out(out: &Path, bytes: &[u8]) -> ptyrc_store::Result<()> {
    let name = out.file_name().ok_or_else(|| {
        ptyrc_store::Error::new(
            ErrorKind::Usage,
            format!("--out {}: not a file name", out.display()),
        )
        .with_hint("--out FILE")
    })?;
    let io = |e: std::io::Error| {
        ptyrc_store::Error::new(ErrorKind::Io, format!("{}: {}", out.display(), io_text(&e)))
            .with_hint("a writable directory for --out")
    };
    let mut next = name.to_os_string();
    next.push(format!(".new-{}", std::process::id()));
    let next = out.with_file_name(next);
    std::fs::write(&next, bytes).map_err(io)?;
    std::fs::rename(&next, out).map_err(|e| {
        let _ = std::fs::remove_file(&next);
        io(e)
    })
}

/// `ptyrc dev build DIR --out FILE`: [`build_temp`], the module written
/// by [`write_out`].
pub fn build_out(dir: &Path, out: &Path, force: bool) -> Result<ExitCode, Fail> {
    is_dir(dir)?;
    let ok = match checked(dir, false) {
        Ok(c) => c,
        Err(status) => return Ok(status),
    };
    if matches!(ok.manifest.app.entry, Entry::Script(_)) {
        return Err(Fail::usage(
            format!("{}: a script app: nothing to build", ok.manifest.app.id),
            "a Rust app: `rust = ...` in [app]",
        ));
    }
    let toolchain = build::Toolchain::find()?;
    let built = build_temp(dir, &ok, &toolchain, force)?;
    write_out(out, &built.wasm)?;
    eprintln!("ptyrc: wrote {}, {} bytes", out.display(), built.wasm.len());
    Ok(ExitCode::SUCCESS)
}
