// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev test` and `ptyrc dev run`: the app at a directory, run by
//! the `ptyrc-box` program (this one does not link the engine).

use crate::store::io_text;
use crate::{Common, Fail};
use ptyrc_proto::session;
use ptyrc_store::build;
use ptyrc_store::manifest::{self, MANIFEST};
use ptyrc_store::options::Resolved;
use ptyrc_store::{Checked, DirTree, Entry, ErrorKind, Manifest, check, options};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

const BOX: &str = "ptyrc-box";

/// The program that runs apps: beside this one, else on `PATH`.
fn runner() -> Result<PathBuf, Fail> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join(BOX)))
        .filter(|p| p.is_file());
    let on_path = || {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|d| d.join(BOX))
            .find(|p| p.is_file())
    };
    beside.or_else(on_path).ok_or_else(|| Fail {
        status: 127,
        message: format!("{BOX}: not found beside ptyrc nor on PATH"),
        hint: Some(format!(
            "install {BOX} with ptyrc; in the source tree: cargo build -p {BOX}"
        )),
    })
}

/// The manifest of the app at `dir`.
fn manifest_of(dir: &Path) -> Result<Manifest, Fail> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| Fail {
        status: 4,
        message: format!("{}: {}", path.display(), io_text(&e)),
        hint: Some("the app's directory; a new one: ptyrc dev new NAME".into()),
    })?;
    Ok(manifest::parse(&text)?)
}

/// The words given read as `ptyrc app` reads them
/// (`ptyrc_store::options::read`), or as `NAME=VALUE`, and checked as the
/// driver checks them (`options::resolve`): `init` holds the defaults of
/// `[options]` filled in.
fn read_args(m: &Manifest, given: &[String]) -> Result<(options::Launch, Resolved), Fail> {
    let names = match m.options.is_empty() {
        true => "the manifest has no [options]".to_string(),
        false => {
            let each =
                (m.options.iter()).map(|o| format!("--{} ({}): {}", o.name, o.type_name(), o.doc));
            format!("[options]: {}", each.collect::<Vec<_>>().join("; "))
        }
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let read = options::read(m, given, &cwd)
        .map_err(|e| Fail::usage(e.message, e.hint.unwrap_or(names.clone())))?;
    let Some(launch) = read else {
        return Err(Fail::usage("--help: the options of the manifest", names));
    };
    let args = Value::Object(launch.args.clone());
    let resolved = options::resolve(m, Some(&args), &launch.files)?;
    Ok((launch, resolved))
}

/// The argument of `init` (`read_args`).
fn init_args(m: &Manifest, given: &[String]) -> Result<Value, Fail> {
    Ok(Value::Object(read_args(m, given)?.1.init))
}

/// `--file NAME=PATH` of `dev test` as the words of the app: `--NAME
/// PATH` for a file or directory option; `ARG#N` the `N`th value of
/// `[args]`, from 0, after the positionals typed.
fn file_words(m: &Manifest, files: &[String], words: &mut Vec<String>) -> Result<(), Fail> {
    let named: Vec<&str> = (m.options.iter().chain(m.args.as_ref().map(|a| &a.option)))
        .filter(|o| o.ty.named())
        .map(|o| o.name.as_str())
        .collect();
    let hint = || match named.is_empty() {
        true => "the manifest has no file or directory option".to_string(),
        false => format!(
            "NAME: a file or directory option ({}); ARG#N: a value of [args]",
            named.join(", ")
        ),
    };
    let mut positional: Vec<(usize, &str)> = Vec::new();
    for f in files {
        let Some((name, path)) = f
            .split_once('=')
            .filter(|(n, p)| !n.is_empty() && !p.is_empty())
        else {
            return Err(Fail::usage(format!("--file {f}: NAME=PATH"), hint()));
        };
        if let Some(n) = name.strip_prefix("ARG#") {
            let args = m.args.as_ref().filter(|a| a.option.ty.named());
            match (n.parse::<usize>(), args) {
                (Ok(n), Some(_)) => positional.push((n, path)),
                _ => {
                    return Err(Fail::usage(
                        format!("--file {f}: no such value of [args]"),
                        hint(),
                    ));
                }
            }
        } else if m.options.iter().any(|o| o.name == name && o.ty.named()) {
            words.extend([format!("--{name}"), path.to_string()]);
        } else {
            return Err(Fail::usage(
                format!("--file {f}: {name}: not a file or directory option"),
                hint(),
            ));
        }
    }
    positional.sort();
    for (i, (n, _)) in positional.iter().enumerate() {
        if *n != i {
            return Err(Fail::usage(
                format!("--file ARG#{n}: ARG#{i} is not given"),
                "ARG#0, ARG#1, ...: in order, none left out",
            ));
        }
    }
    if !positional.is_empty() && !words.iter().any(|w| w == "--") {
        words.push("--".into());
    }
    words.extend(positional.into_iter().map(|(_, p)| p.to_string()));
    Ok(())
}

/// The arguments of `ptyrc-box` that name the guest: `--script APP.js`,
/// or the module, built.
fn guest(dir: &Path, m: &Manifest) -> Result<Vec<OsString>, Fail> {
    // `[app] assets`: read by the runner, answered by it (`asset.*`).
    let mut out: Vec<OsString> = match &m.app.assets {
        Some(a) => vec!["--assets".into(), dir.join(a).into()],
        None => Vec::new(),
    };
    match &m.app.entry {
        Entry::Script(main) => out.extend(["--script".into(), dir.join(main).into()]),
        Entry::Rust(_) => out.push(build(dir)?.into()),
    }
    Ok(out)
}

/// The build of the store (`build::build`: `rustc` alone, ptyrc's own
/// `ptyrc-app`) in `DIR/target/ptyrc/`, under the profile where there is
/// one, else without (the author's own tree; said). `rustc`'s messages
/// on stderr. The module.
pub(super) fn build_checked(
    dir: &Path,
    tree: &DirTree,
    checked: &Checked,
) -> ptyrc_store::Result<PathBuf> {
    let toolchain = build::Toolchain::find()?;
    let target = dir.join("target");
    let io = |p: &Path, e: std::io::Error| {
        ptyrc_store::Error::new(ErrorKind::Io, format!("{}: {}", p.display(), io_text(&e)))
    };
    std::fs::create_dir_all(&target).map_err(|e| io(&target, e))?;
    let tag = target.join("CACHEDIR.TAG");
    if !tag.exists() {
        // Skipped by the tree check and `dev run`, as cargo's.
        std::fs::write(&tag, "Signature: 8a477f597d28d172789f06886806bc55\n")
            .map_err(|e| io(&tag, e))?;
    }
    for p in build::vendored_differs(tree, checked) {
        eprintln!("ptyrc: note: {p}: not ptyrc's own ptyrc-app, which the build uses");
    }
    let options = build::Options {
        unconfined: true,
        ..build::Options::default()
    };
    let work = target.join("ptyrc");
    let built = build::build(tree, checked, &work, &toolchain, &options)?;
    if !built.messages.trim().is_empty() {
        eprintln!("{}", built.messages.trim_end());
    }
    if let Some(why) = &built.missing {
        eprintln!("ptyrc: note: built without an OS profile: {why}");
    }
    Ok(work.join("out").join(build::MODULE))
}

/// [`build_checked`] of the tree at `dir`, checked first.
fn build(dir: &Path) -> Result<PathBuf, Fail> {
    let tree = DirTree::new(dir);
    let checked = check(&tree).map_err(|e| Fail {
        hint: Some(match &e.hint {
            Some(h) => format!("{h}; `ptyrc dev check` lists every problem"),
            None => "`ptyrc dev check` lists every problem".into(),
        }),
        ..Fail::from(e)
    })?;
    Ok(build_checked(dir, &tree, &checked)?)
}

fn status_of(status: std::process::ExitStatus) -> ExitCode {
    match status.code() {
        Some(c) => ExitCode::from(c.clamp(0, 255) as u8),
        None => ExitCode::from(70),
    }
}

/// How `dev test` runs the app.
pub struct TestOpts {
    pub dir: PathBuf,
    pub args: Vec<String>,
    /// Permissions declared and denied.
    pub deny: Vec<String>,
    /// `--file NAME=PATH`, as given.
    pub files: Vec<String>,
    /// `ptyrc-box --headless` flags, as given.
    pub flags: Vec<OsString>,
}

/// The flags of `ptyrc-box` that hold the app to what its manifest
/// declares for this run (`Resolved::asked`: a permission an option
/// needs only when that option is given), less `deny`; the user's words
/// and patterns; the launcher's options; the files named.
fn grants(
    m: &Manifest,
    deny: &[String],
    launch: &options::Launch,
    resolved: &Resolved,
) -> Result<Vec<OsString>, Fail> {
    let declared = declared(m);
    for d in deny {
        if !manifest::PERMISSIONS.contains(&d.as_str()) {
            return Err(Fail::usage(
                format!("--deny {d}: not a permission"),
                format!("permissions: {}", manifest::PERMISSIONS.join(", ")),
            ));
        }
        if !declared.contains(&d.as_str()) {
            eprintln!("ptyrc: note: --deny {d}: not declared, never held");
        }
    }
    let mut asked = resolved.asked(m);
    for l in &m.limits {
        let p = manifest::limit_permission(&l.name);
        if !asked.contains(&p) {
            asked.push(p);
        }
    }
    let words = m.permissions.iter().flat_map(|p| p.words.iter());
    let words: Vec<&str> = words.map(String::as_str).collect();
    let mut flags: Vec<OsString> = vec!["--grant".into(), asked.join(",").into()];
    if !deny.is_empty() {
        flags.extend(["--deny".into(), deny.join(",").into()]);
    }
    if !words.is_empty() {
        flags.extend(["--words".into(), words.join(",").into()]);
    }
    for w in &resolved.words {
        flags.extend(["--user-word".into(), w.into()]);
    }
    for p in &resolved.patterns {
        let flag = if p.regex {
            "--user-regex"
        } else {
            "--user-pattern"
        };
        flags.extend([flag.into(), (&p.pattern).into()]);
    }
    let l = &launch.launcher;
    match l.front {
        Some(true) => flags.push("--front".into()),
        Some(false) => flags.push("--back".into()),
        None => {}
    }
    let mut put = |flag: &str, v: Option<String>| {
        if let Some(v) = v {
            flags.extend([flag.into(), v.into()]);
        }
    };
    put("--prefer", l.prefer.clone());
    put("--alpha", l.alpha.map(|a| a.to_string()));
    put("--border", l.border.clone());
    // The handles in order (`f0`, ...), each under its option: the
    // runner numbers them as given.
    let mut handles: Vec<_> = resolved.files.iter().collect();
    handles.sort_by_key(|h| h.handle[1..].parse::<usize>().unwrap_or(usize::MAX));
    for h in handles {
        let holds = |v: &Value| v.get("file").and_then(Value::as_str) == Some(&h.handle);
        let option = (resolved.init.iter())
            .find(|(_, v)| match v {
                Value::Array(list) => list.iter().any(holds),
                v => holds(v),
            })
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        let flag = if h.write { "--write-file" } else { "--file" };
        let pair = format!("{option}={}", h.path.display());
        flags.extend([flag.into(), pair.into()]);
    }
    Ok(flags)
}

/// The permissions of `[permissions]`, and those `[limits]` asks
/// (`limit-memory`, `limit-time`; plan, Mechanism).
fn declared(m: &Manifest) -> Vec<&str> {
    let mut declared: Vec<&str> = m.permissions.iter().map(|p| p.name.as_str()).collect();
    for l in &m.limits {
        let p = manifest::limit_permission(&l.name);
        if !declared.contains(&p) {
            declared.push(p);
        }
    }
    declared
}

/// The manifest as `ptyrc-box --manifest` takes it: what the app may
/// know of itself, and `[limits]`.
pub fn given(m: &Manifest) -> Value {
    let permissions: Vec<Value> = (m.permissions.iter())
        .map(|p| match p.words.is_empty() {
            true => json!({"name": p.name}),
            false => json!({"name": p.name, "words": p.words}),
        })
        .collect();
    let limits: Map<String, Value> = (m.limits.iter())
        .map(|l| (l.name.clone(), json!(l.value)))
        .collect();
    json!({"id": m.app.id, "version": m.app.version, "permissions": permissions,
           "limits": limits})
}

pub fn test(o: TestOpts) -> Result<ExitCode, Fail> {
    let m = manifest_of(&o.dir)?;
    let mut words = o.args.clone();
    file_words(&m, &o.files, &mut words)?;
    let (launch, resolved) = read_args(&m, &words)?;
    let grants = grants(&m, &o.deny, &launch, &resolved)?;
    let args = Value::Object(resolved.init.clone());
    let program = runner()?;
    let status = Command::new(&program)
        .arg("--headless")
        .arg("--manifest")
        .arg(given(&m).to_string())
        .args(guest(&o.dir, &m)?)
        .arg(args.to_string())
        .args(&o.flags)
        .args(grants)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| Fail::from(format!("{}: {}", program.display(), io_text(&e))))?;
    Ok(status_of(status))
}

/// `dev test --unit FILE`: the unit tests of a script app, under the
/// limits its manifest declares.
pub fn unit(dir: &Path, file: &str) -> Result<ExitCode, Fail> {
    let m = manifest_of(dir)?;
    let Entry::Script(main) = &m.app.entry else {
        return Err(Fail::usage(
            "--unit: the unit tests of a script app",
            "a Rust app: cargo test",
        ));
    };
    let program = runner()?;
    let status = Command::new(&program)
        .args(["--unit", file])
        .arg("--manifest")
        .arg(given(&m).to_string())
        .arg("--script")
        .arg(dir.join(main))
        .stdin(Stdio::null())
        .status()
        .map_err(|e| Fail::from(format!("{}: {}", program.display(), io_text(&e))))?;
    Ok(status_of(status))
}

/// Files of the tree by path: `(mtime, size)`. Skipped: `.git`, a cache
/// directory (`CACHEDIR.TAG`, as `target/`), `node_modules`.
fn snapshot(dir: &Path, out: &mut BTreeMap<PathBuf, (SystemTime, u64)>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            let name = e.file_name();
            let skip = name == ".git"
                || name == "node_modules"
                || path.join("CACHEDIR.TAG").is_file()
                || depth >= 16;
            if !skip {
                snapshot(&path, out, depth + 1);
            }
        } else if let Ok(meta) = e.metadata() {
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.insert(path, (mtime, meta.len()));
        }
    }
}

/// The socket of the session chosen as every verb chooses it.
fn socket(common: &Common) -> Result<PathBuf, Fail> {
    if common.session.is_none()
        && let Some(s) = std::env::var_os(session::ENV_SOCKET).filter(|s| !s.is_empty())
    {
        return Ok(PathBuf::from(s));
    }
    match session::select(common.session.as_deref()) {
        Ok(s) => Ok(s.path),
        Err(e) => Err(Fail {
            status: 3,
            message: e.to_string(),
            hint: Some(e.hint()),
        }),
    }
}

fn start(program: &Path, socket: &Path, guest: &[OsString], args: &Value) -> Result<Child, Fail> {
    Command::new(program)
        .args(guest)
        .arg(args.to_string())
        .env(session::ENV_SOCKET, socket)
        .env_remove(session::ENV_TOKEN)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| Fail::from(format!("{}: {}", program.display(), io_text(&e))))
}

/// SIGTERM to the runner: the guest's `stop` (500 ms), then the end;
/// killed past a second.
fn stop(child: &mut Child) {
    if let Ok(None) = child.try_wait() {
        let pid = rustix::process::Pid::from_raw(child.id() as i32);
        if let Some(pid) = pid {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `dev run`: the app in the session; started again when a file of
/// the tree changes. Ends with SIGINT (Ctrl-C) or SIGTERM, the app
/// stopped first.
pub fn run(common: &Common, dir: &Path, given: &[String]) -> Result<ExitCode, Fail> {
    let program = runner()?;
    let socket = socket(common)?;
    let term = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, term.clone())
            .map_err(|e| Fail::from(format!("signals: {}", io_text(&e))))?;
    }
    let poll = Duration::from_millis(300);
    let step = Duration::from_millis(50);
    let mut seen = BTreeMap::new();
    snapshot(dir, &mut seen, 0);
    loop {
        // A failed start waits for a change, as an exit does.
        let mut child = match manifest_of(dir).and_then(|m| {
            let args = init_args(&m, given)?;
            // The author's own run: all it declares, its limits too.
            let mut told = self::given(&m);
            told["granted"] = json!(declared(&m));
            let mut guest = guest(dir, &m)?;
            guest.splice(0..0, ["--manifest".into(), told.to_string().into()]);
            eprintln!("ptyrc: starting {} {}", m.app.id, m.app.version);
            start(&program, &socket, &guest, &args)
        }) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("ptyrc: {}", e.message);
                if let Some(h) = &e.hint {
                    eprintln!("  {h}");
                }
                None
            }
        };
        loop {
            let slept = std::time::Instant::now();
            while slept.elapsed() < poll && !term.load(Ordering::Relaxed) {
                std::thread::sleep(step);
            }
            if term.load(Ordering::Relaxed) {
                if let Some(c) = &mut child {
                    stop(c);
                }
                eprintln!("ptyrc: stopped");
                return Ok(ExitCode::SUCCESS);
            }
            if let Some(c) = &mut child
                && let Ok(Some(status)) = c.try_wait()
            {
                eprintln!("ptyrc: the app ended ({status}); waiting for a change");
                child = None;
            }
            let mut now = BTreeMap::new();
            snapshot(dir, &mut now, 0);
            if now != seen {
                let changed = now
                    .iter()
                    .find(|(p, v)| seen.get(*p) != Some(*v))
                    .or_else(|| seen.iter().find(|(p, _)| !now.contains_key(*p)))
                    .map(|(p, _)| p.strip_prefix(dir).unwrap_or(p).display().to_string());
                eprintln!(
                    "ptyrc: {} changed: starting again",
                    changed.unwrap_or_default()
                );
                seen = now;
                break;
            }
        }
        if let Some(mut c) = child {
            // The connection closes: the driver removes what it drew.
            stop(&mut c);
        }
    }
}
