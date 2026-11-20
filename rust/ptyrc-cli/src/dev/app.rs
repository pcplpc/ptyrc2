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

/// The program that runs apps: `$PTYRC_BOX`, as the driver; else
/// beside this one; else on `PATH`.
fn runner() -> Result<PathBuf, Fail> {
    let named = std::env::var_os("PTYRC_BOX")
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    if let Some(p) = named {
        return Ok(p);
    }
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
            let each = (m.options.iter()).map(|o| {
                let range = o.range().map(|r| format!(", {r}")).unwrap_or_default();
                let choices = match o.choices.is_empty() {
                    true => String::new(),
                    false => format!(", one of {}", o.choices.join(", ")),
                };
                format!(
                    "{} ({}{range}{choices}): {}",
                    o.flag(),
                    o.type_name(),
                    o.doc
                )
            });
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

/// The flags of `dev test` that take a value or stand alone.
pub const TEST_FLAGS: &[&str] = &[
    "size",
    "for",
    "at",
    "seed",
    "tape",
    "screen",
    "styles",
    "timing",
    "unit",
    "arg",
    "deny",
    "event",
    "key",
    "clock",
    "zone",
    "storage",
    "storage-out",
    "file",
    "probe",
    "pointer",
    "click",
    "focus",
    "paste",
    "resize",
    "json",
];

/// The options of the app named as a flag of `dev test`: given before
/// `--`, the flag is `dev test`'s.
pub fn shadowed(m: &Manifest) -> Vec<&str> {
    (m.options.iter())
        .map(|o| o.name.as_str())
        .filter(|n| TEST_FLAGS.contains(n))
        .collect()
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
/// `ptyrc-app`) in `DIR/target/ptyrc/work/`, under the profile where
/// there is one, else without (the author's own tree; said). `rustc`'s
/// messages on stderr. The module: `DIR/target/ptyrc/app.wasm`, put by
/// a rename, kept with `app.stamp` (the sources, `ptyrc-app`, `rustc`):
/// not built again while they are the same. Under a lock
/// (`target/ptyrc/app.lock`): two at once in a tree build once.
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
    let base = target.join("ptyrc");
    let module = base.join(build::MODULE);
    let _held = build::lock(&base.join("app"))?;
    let stamp = stamp(tree, checked, &toolchain)?;
    let stamp_file = base.join("app.stamp");
    let same = std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp);
    if same && module.is_file() {
        return Ok(module);
    }
    let options = build::Options {
        unconfined: true,
        ..build::Options::default()
    };
    let built = build::build(tree, checked, &base.join("work"), &toolchain, &options)?;
    if !built.messages.trim().is_empty() {
        eprintln!("{}", built.messages.trim_end());
    }
    if let Some(why) = &built.missing {
        eprintln!("ptyrc: note: built without an OS profile: {why}");
    }
    let _ = std::fs::remove_file(&stamp_file);
    let next = base.join("app.wasm.new");
    std::fs::write(&next, &built.wasm).map_err(|e| io(&next, e))?;
    std::fs::rename(&next, &module).map_err(|e| io(&module, e))?;
    std::fs::write(&stamp_file, &stamp).map_err(|e| io(&stamp_file, e))?;
    Ok(module)
}

/// What a module is of: the files of the tree, ptyrc's `ptyrc-app`,
/// `rustc`, this program; a hash, as text.
fn stamp(tree: &DirTree, checked: &Checked, t: &build::Toolchain) -> ptyrc_store::Result<String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    build::PTYRC_APP.hash(&mut h);
    (&t.rustc, &t.release).hash(&mut h);
    for f in &checked.files {
        f.path.hash(&mut h);
        ptyrc_store::Tree::read(tree, f)?.hash(&mut h);
    }
    Ok(format!("{:016x}\n", h.finish()))
}

/// [`build_checked`] of the tree at `dir`, checked first.
pub(super) fn build(dir: &Path) -> Result<PathBuf, Fail> {
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

/// The module of a Rust app at `dir`, built if it is not; a script
/// app: nothing. `dev test --cases` before its cases.
pub(super) fn built(dir: &Path) -> Result<(), Fail> {
    match manifest_of(dir)?.app.entry {
        Entry::Rust(_) => build(dir).map(drop),
        Entry::Script(_) => Ok(()),
    }
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
    /// `--storage-out DIR`.
    pub storage_out: Option<PathBuf>,
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
    put("--seed", l.seed.map(|s| s.to_string()));
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
    // A flag of `dev test` given that an option of the app shares:
    // ours, said with the value it took.
    let shadowed = shadowed(&m);
    for (i, f) in o.flags.iter().enumerate() {
        let Some(name) = f.to_str().and_then(|f| f.strip_prefix("--")) else {
            continue;
        };
        if shadowed.contains(&name) {
            let value = match name {
                "styles" | "timing" | "json" => String::new(),
                _ => format!(
                    " {}",
                    o.flags
                        .get(i + 1)
                        .map_or("".into(), |v| v.to_string_lossy())
                ),
            };
            eprintln!(
                "ptyrc: note: --{name}{value}: taken by dev test, not the app; the app's \
                 --{name} goes after `--` (ptyrc dev test DIR -- --{name} VALUE)"
            );
        }
    }
    let mut words = o.args.clone();
    file_words(&m, &o.files, &mut words)?;
    let (launch, resolved) = read_args(&m, &words)?;
    if launch.launcher.seed.is_some() && o.flags.iter().any(|f| f == "--seed") {
        return Err(Fail::usage(
            "--seed: given twice",
            "--seed once: dev test's, or the launcher's after `--`, the same seed",
        ));
    }
    let grants = grants(&m, &o.deny, &launch, &resolved)?;
    let args = Value::Object(resolved.init.clone());
    let program = runner()?;
    // The runner writes a file opened before its profile; the directory
    // is written here.
    let out_file = o.storage_out.as_ref().map(|_| {
        std::env::temp_dir().join(format!("ptyrc-storage-out-{}.json", std::process::id()))
    });
    let mut out_flags: Vec<OsString> = Vec::new();
    if let Some(f) = &out_file {
        out_flags.extend(["--storage-out".into(), f.into()]);
    }
    let status = Command::new(&program)
        .arg("--headless")
        .arg("--manifest")
        .arg(given(&m).to_string())
        .args(["--surface", m.app.surface.as_str()])
        .args(guest(&o.dir, &m)?)
        .arg(args.to_string())
        .args(&o.flags)
        .args(grants)
        .args(out_flags)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| Fail::from(format!("{}: {}", program.display(), io_text(&e))));
    if let (Some(dir), Some(file)) = (&o.storage_out, &out_file) {
        let text = std::fs::read_to_string(file);
        let _ = std::fs::remove_file(file);
        if status.is_ok() {
            let text = text.unwrap_or_default();
            match serde_json::from_str::<BTreeMap<String, String>>(&text) {
                Ok(storage) => storage_into(dir, &storage)?,
                Err(_) => eprintln!(
                    "ptyrc: --storage-out {}: the runner gave no storage; not written",
                    dir.display()
                ),
            }
        }
    }
    Ok(status_of(status?))
}

/// A key as `--storage` reads it: a file name of `[A-Za-z0-9._-]`, not
/// `.` nor `..`.
fn is_key(k: &str) -> bool {
    !k.is_empty()
        && k != "."
        && k != ".."
        && (k.bytes()).all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `--storage-out DIR`: a file a key; a file of DIR named as a key and
/// not in `storage` removed: DIR holds the storage, no more.
fn storage_into(dir: &Path, storage: &BTreeMap<String, String>) -> Result<(), Fail> {
    let fail = |p: &Path, e: std::io::Error| {
        Fail::from(format!("--storage-out {}: {}", p.display(), io_text(&e)))
    };
    std::fs::create_dir_all(dir).map_err(|e| fail(dir, e))?;
    for e in std::fs::read_dir(dir).map_err(|e| fail(dir, e))?.flatten() {
        let name = e.file_name();
        let Some(k) = name.to_str() else { continue };
        if is_key(k) && !storage.contains_key(k) && e.file_type().is_ok_and(|t| t.is_file()) {
            std::fs::remove_file(e.path()).map_err(|x| fail(&e.path(), x))?;
        }
    }
    for (k, v) in storage.iter().filter(|(k, _)| is_key(k)) {
        let p = dir.join(k);
        std::fs::write(&p, v).map_err(|e| fail(&p, e))?;
    }
    eprintln!(
        "ptyrc: storage: {} keys written to {}",
        storage.len(),
        dir.display()
    );
    Ok(())
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

/// The frame a start draws on, as the driver resolves it for
/// `app.start`: `--frame`, else `[app] frame`, else `active`.
fn frame_at_start(socket: &Path, m: &Manifest, launch: &options::Launch) -> Result<u64, Fail> {
    let name = (launch.launcher.frame.clone())
        .or_else(|| m.app.frame.clone())
        .unwrap_or_else(|| "active".into());
    let frame = name
        .parse::<u64>()
        .map_or_else(|_| json!(name), |id| json!(id));
    let mut client = ptyrc_proto::Client::connect_as(socket, None)?;
    let got = client.call("frame.get", json!({ "frame": frame }))?;
    (got["frame"]["id"].as_u64()).ok_or_else(|| Fail::from(format!("frame.get {name}: no id")))
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
            let (launch, resolved) = read_args(&m, given)?;
            let args = Value::Object(resolved.init);
            // The author's own run: all it declares, its limits too.
            let mut told = self::given(&m);
            told["granted"] = json!(declared(&m));
            told["frame"] = json!(frame_at_start(&socket, &m, &launch)?);
            let mut guest = guest(dir, &m)?;
            guest.splice(0..0, ["--manifest".into(), told.to_string().into()]);
            if let Some(seed) = launch.launcher.seed {
                guest.splice(0..0, ["--seed".into(), seed.to_string().into()]);
            }
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_list_names_a_bool_on_by_default_by_its_no_flag() {
        let m = super::manifest::parse(
            "[app]\nid = \"a\"\nversion = \"0.1.0\"\nsummary = \"A.\"\n\
             license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
             [options]\nseconds = { type = \"bool\", default = true, doc = \"with seconds\" }\n",
        )
        .unwrap();
        let Err(e) = super::read_args(&m, &["--help".to_string()]) else {
            panic!("--help")
        };
        let hint = e.hint.unwrap_or_default();
        assert!(hint.contains("--no-seconds (bool): with seconds"), "{hint}");
        let (launch, _) = super::read_args(&m, &["--no-seconds".to_string()]).unwrap();
        assert_eq!(launch.args["seconds"], false);
    }
}
