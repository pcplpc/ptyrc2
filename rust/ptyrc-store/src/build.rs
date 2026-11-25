// SPDX-License-Identifier: GPL-3.0-or-later
//! The build of a Rust app by `rustc` alone (decision 20): `src/lib.rs`
//! of the tree and the vendored `ptyrc-app` into `app.wasm`. No cargo,
//! no other crate, no build script, no macro crate.
//!
//! In a fresh work directory: `app/`, the tree (less `vendor/ptyrc-app/`);
//! `ptyrc-app/`, the sources shipped with ptyrc; `out/`, `tmp/`. Three
//! runs of the toolchain's own `rustc` (not the rustup proxy), the
//! environment cleared: `ptyrc-app` as an rlib; the app as a `cdylib`,
//! `--extern ptyrc_app`, its dependency information beside; that
//! information alone under `--test`. Each under the profile (Linux,
//! Landlock): read and execute the sysroot and the system's libraries;
//! read the two sources; write `out/`, `tmp/`; no TCP, no abstract
//! socket, no signal out. Bounds: the time of both runs, the address
//! space and the size of a file written by each, the module. The
//! module imports from `ptyrc` only. One build at a time in a work
//! directory: the caller's, by [`lock`] (`ptyrc dev`, a shipped app)
//! or the store's lock (an install).

pub mod scan;

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::error::{Error, ErrorKind, Result};
use crate::manifest::Entry;
use crate::tree::{Checked, Tree};

pub use scan::{MODULE, RULE, VENDORED, sources};

/// The target of every Rust store app.
pub const TARGET: &str = "wasm32-unknown-unknown";

/// The sources of `ptyrc-app` shipped with ptyrc: `(path, text)`.
pub const PTYRC_APP: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/lib.rs"),
    ),
    (
        "src/canvas.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/canvas.rs"),
    ),
    (
        "src/deck.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/deck.rs"),
    ),
    (
        "src/dice.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/dice.rs"),
    ),
    (
        "src/ffi.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/ffi.rs"),
    ),
    (
        "src/host.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/host.rs"),
    ),
    (
        "src/json.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/json.rs"),
    ),
    (
        "src/sight.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/sight.rs"),
    ),
    (
        "src/native.rs",
        include_str!("../../../sdk/rust/ptyrc-app/src/native.rs"),
    ),
];

/// A work directory this module made, and may empty.
const MARK: &str = ".ptyrc-build";

fn err(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Build, message)
}

/// The builder's fault, not the app's.
fn tool_err(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Toolchain, message)
}

/// How to get the toolchain.
const GET: &str = "rustc and its target: https://rustup.rs, then rustup target add \
                   wasm32-unknown-unknown (or the rust and rust-wasm packages of the system)";

/// Bounds of one build (plan, Rust).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The runs of `rustc`, together.
    pub time: Duration,
    /// Address space of each process.
    pub memory: u64,
    /// The module, bytes; a file written by the build: four times this.
    pub module: u64,
}

pub const BOUNDS: Bounds = Bounds {
    time: Duration::from_secs(300),
    memory: 4 << 30,
    module: 16 << 20,
};

/// How to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub bounds: Bounds,
    /// Build without the profile where there is none, in place of a
    /// refusal. The author's own tree (`ptyrc dev`), or `--force`.
    pub unconfined: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            bounds: BOUNDS,
            unconfined: false,
        }
    }
}

/// A module built.
#[derive(Debug, Clone)]
pub struct Built {
    pub wasm: Vec<u8>,
    /// The profile applied, a line each; empty: none (`unconfined`).
    pub profile: Vec<String>,
    /// Why there was none, when there was none.
    pub missing: Option<String>,
    /// `rustc -vV`, its `release`.
    pub rustc: String,
    /// What `rustc` said of the app (warnings).
    pub messages: String,
    /// The files of the tree `rustc` read for the module, or would for
    /// `--test` (`mod`, `include_str!`, ...), by [`dep_info`]; sorted.
    pub sources: Vec<String>,
}

/// The toolchain found.
#[derive(Debug, Clone)]
pub struct Toolchain {
    /// `<sysroot>/bin/rustc` when it is there, else the program found.
    pub rustc: PathBuf,
    pub sysroot: PathBuf,
    /// `1.96.0`, `1.97.0-nightly`.
    pub release: String,
}

impl Toolchain {
    /// `PTYRC_RUSTC`, else `rustc` on `PATH`, asked for its sysroot from
    /// `/` (a `rust-toolchain.toml` of a tree chooses nothing); the
    /// target's standard library there.
    pub fn find() -> Result<Toolchain> {
        let given = std::env::var_os("PTYRC_RUSTC")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from("rustc"), PathBuf::from);
        let ask = |program: &Path, args: &[&str], clear: bool| -> Result<String> {
            let mut cmd = Command::new(program);
            cmd.args(args).current_dir("/").stdin(Stdio::null());
            if clear {
                cmd.env_clear();
            }
            let out = cmd.output().map_err(|e| {
                tool_err(format!("{}: {e}", program.display())).with_hint(format!("install {GET}"))
            })?;
            if !out.status.success() {
                let text = String::from_utf8_lossy(&out.stderr);
                return Err(tool_err(format!(
                    "{} {}: {}",
                    program.display(),
                    args.join(" "),
                    text.trim()
                ))
                .with_hint(GET));
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let sysroot = PathBuf::from(ask(&given, &["--print", "sysroot"], false)?);
        let own = sysroot.join("bin/rustc");
        let rustc = if own.is_file() { own } else { given };
        let vv = ask(&rustc, &["-vV"], true)?;
        let release = vv
            .lines()
            .find_map(|l| l.strip_prefix("release: "))
            .unwrap_or("")
            .to_string();
        if !sysroot
            .join("lib/rustlib")
            .join(TARGET)
            .join("lib")
            .is_dir()
        {
            return Err(tool_err(format!(
                "rustc {release} ({}): no standard library for {TARGET}",
                sysroot.display()
            ))
            .with_hint(format!(
                "rustup target add {TARGET} (or the rust-wasm package of the system)"
            )));
        }
        Ok(Toolchain {
            rustc,
            sysroot,
            release,
        })
    }

    /// `(major, minor, patch)` of `release`.
    pub fn version(&self) -> Option<(u32, u32, u32)> {
        version(&self.release)
    }
}

fn version(text: &str) -> Option<(u32, u32, u32)> {
    let core = text.split(['-', ' ']).next()?;
    let mut n = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((n.next()??, n.next()??, n.next().flatten().unwrap_or(0)))
}

/// Builds the Rust app of a checked tree in `work` (made, or emptied if
/// this module made it). [`sources`] first: its first problem refuses.
pub fn build(
    tree: &dyn Tree,
    checked: &Checked,
    work: &Path,
    toolchain: &Toolchain,
    options: &Options,
) -> Result<Built> {
    let Entry::Rust(entry) = &checked.manifest.app.entry else {
        return Err(err("not a Rust app: nothing to build"));
    };
    if let Some(e) = sources(tree, checked).into_iter().next() {
        return Err(e);
    }
    if let Some(want) = &checked.manifest.app.rustc
        && let (Some(w), Some(have)) = (version(want), toolchain.version())
        && have < w
    {
        return Err(tool_err(format!(
            "[app] rustc = {want:?}: rustc {} is older",
            toolchain.release
        ))
        .with_hint("rustup update"));
    }
    let start = Instant::now();
    fresh(work)?;
    let (app, lib, out, tmp) = (
        work.join("app"),
        work.join("ptyrc-app"),
        work.join("out"),
        work.join("tmp"),
    );
    for d in &checked.dirs {
        if !format!("{}/", d.path).starts_with(VENDORED) {
            mkdir(&app.join(&d.path))?;
        }
    }
    mkdir(&app)?;
    for f in checked
        .files
        .iter()
        .filter(|f| !f.path.starts_with(VENDORED))
    {
        write(&app.join(&f.path), &tree.read(f)?)?;
    }
    for (path, text) in PTYRC_APP {
        write(&lib.join(path), text.as_bytes())?;
    }
    mkdir(&out)?;
    mkdir(&tmp)?;
    let common = [
        "--edition",
        "2024",
        "--target",
        TARGET,
        "-C",
        "opt-level=s",
        "-C",
        "panic=abort",
        "-C",
        "codegen-units=1",
        "--color",
        "never",
    ];
    let rules = Rules {
        exec: exec(&toolchain.sysroot),
        read: vec!["/etc/ld.so.cache".into(), app.clone(), lib.clone()],
        write: vec![out.clone(), tmp.clone(), "/dev/null".into()],
    };
    let deadline = start + options.bounds.time;
    let run = |cwd: &Path, args: Vec<OsString>| {
        let mut all: Vec<OsString> = common.iter().map(OsString::from).collect();
        all.extend(args);
        let env = [("TMPDIR", tmp.as_os_str())];
        confined(&toolchain.rustc, &all, cwd, &env, &rules, deadline, options)
    };
    let rlib = run(
        &lib,
        ["--crate-type", "rlib", "--crate-name", "ptyrc_app"]
            .iter()
            .map(OsString::from)
            .chain(
                [
                    "--cap-lints",
                    "allow",
                    "-o",
                    "../out/libptyrc_app.rlib",
                    "src/lib.rs",
                ]
                .map(OsString::from),
            )
            .collect(),
    )?;
    if !rlib.status.success() {
        let hint = match toolchain.version().is_some_and(|v| v < EDITION_2024) {
            true => "the toolchain is older than ptyrc-app needs (edition 2024: rustc 1.85); \
                     rustup update"
                .to_string(),
            false => format!(
                "rustc {} on ptyrc's own ptyrc-app: the messages above; not the app's sources",
                toolchain.release
            ),
        };
        let e = failed("ptyrc-app", &rlib, options);
        return Err(tool_err(e.message).with_hint(hint));
    }
    let made = run(
        &app,
        [
            "--crate-type",
            "cdylib",
            "--crate-name",
            "app",
            "-C",
            "lto",
            "-C",
            "strip=symbols",
            "--extern",
            "ptyrc_app=../out/libptyrc_app.rlib",
            "-L",
            "dependency=../out",
            "--out-dir",
            "../out",
            "--emit",
            "link=../out/app.wasm,dep-info=../out/app.d",
        ]
        .iter()
        .map(OsString::from)
        .chain([OsString::from(entry)])
        .collect(),
    )?;
    if !made.status.success() {
        return Err(failed(entry, &made, options).with_hint(
            "the errors of rustc above, paths from the app's root; dev check, dev test the same",
        ));
    }
    // The files of the tests too (`--test`: `#[cfg(test)]`, `#[test]`):
    // the dependency information alone, whatever the outcome (written
    // before name resolution).
    run(
        &app,
        [
            "--test",
            "--crate-name",
            "app",
            "--cap-lints",
            "allow",
            "--extern",
            "ptyrc_app=../out/libptyrc_app.rlib",
            "-L",
            "dependency=../out",
            "--out-dir",
            "../out",
            "--emit",
            "dep-info=../out/test.d",
        ]
        .iter()
        .map(OsString::from)
        .chain([OsString::from(entry)])
        .collect(),
    )?;
    let mut read: Vec<String> = Vec::new();
    for d in ["app.d", "test.d"] {
        let text = std::fs::read_to_string(out.join(d)).unwrap_or_default();
        read.extend(dep_info(&text).into_iter().filter_map(|p| {
            let p = match Path::new(&p).strip_prefix(&app) {
                Ok(rel) => rel.to_str()?.to_string(),
                Err(_) => p,
            };
            let p = p.strip_prefix("./").unwrap_or(&p).to_string();
            checked.files.iter().any(|f| f.path == p).then_some(p)
        }));
    }
    read.sort();
    read.dedup();
    let module = out.join("app.wasm");
    let size = std::fs::metadata(&module)
        .map_err(|e| Error::io(module.display(), e))?
        .len();
    if size > options.bounds.module {
        return Err(err(format!(
            "{MODULE}: {size} bytes, more than {} MiB",
            options.bounds.module >> 20
        ))
        .with_hint("a smaller module: less static data; assets/ for files"));
    }
    let wasm = std::fs::read(&module).map_err(|e| Error::io(module.display(), e))?;
    if let Some(bad) = imports(&wasm)?.into_iter().find(|(m, _)| m != "ptyrc") {
        return Err(err(format!(
            "{MODULE}: imports {}.{}: from a module other than ptyrc",
            bad.0, bad.1
        ))
        .with_hint("no extern block of your own; ptyrc-app's functions only"));
    }
    Ok(Built {
        wasm,
        profile: made.profile,
        missing: made.missing,
        rustc: toolchain.release.clone(),
        messages: made.stderr,
        sources: read,
    })
}

/// The prerequisites of the first rule of a dependency file of `rustc`
/// (`--emit dep-info`): `TARGET: A B\ C` gives `A`, `B C`.
pub fn dep_info(text: &str) -> Vec<String> {
    let Some(line) = text
        .lines()
        .find(|l| !l.starts_with('#') && l.contains(": "))
    else {
        return Vec::new();
    };
    let rest = &line[line.find(": ").map_or(line.len(), |i| i + 2)..];
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                cur.push(' ');
                chars.next();
            }
            ' ' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The sources of `vendor/ptyrc-app/` (the copy a template keeps for
/// cargo) that are not ptyrc's own, which the build uses: changed,
/// missing or added. Empty without that directory.
pub fn vendored_differs(tree: &dyn Tree, checked: &Checked) -> Vec<String> {
    let mine: Vec<_> = (checked.files.iter())
        .filter(|f| f.path.starts_with(VENDORED) && f.path.ends_with(".rs"))
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (path, text) in PTYRC_APP {
        let full = format!("{VENDORED}{path}");
        match mine.iter().find(|f| f.path == full) {
            Some(f) if tree.read(f).is_ok_and(|b| b == text.as_bytes()) => {}
            _ => out.push(full),
        }
    }
    for f in mine {
        if !PTYRC_APP
            .iter()
            .any(|(p, _)| f.path == format!("{VENDORED}{p}"))
        {
            out.push(f.path.clone());
        }
    }
    out
}

/// The first `rustc` of edition 2024, which `ptyrc-app` is in.
const EDITION_2024: (u32, u32, u32) = (1, 85, 0);

/// An exclusive lock on `PATH.lock` (made, its directory too), held
/// until the file is dropped; waits for another holder, of this
/// process or another. Around [`build`] and what uses its work
/// directory; the file is left.
pub fn lock(path: &Path) -> Result<std::fs::File> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    let file = PathBuf::from(name);
    if let Some(parent) = file.parent() {
        mkdir(parent)?;
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&file)
        .map_err(|e| Error::io(file.display(), e))?;
    f.lock().map_err(|e| Error::io(file.display(), e))?;
    Ok(f)
}

/// `work`, empty: made, or emptied when it holds the mark.
fn fresh(work: &Path) -> Result<()> {
    if work.exists() {
        if !work.join(MARK).is_file() {
            return Err(tool_err(format!(
                "{}: exists, and not made by a build",
                work.display()
            ))
            .with_hint("another directory, or remove it"));
        }
        std::fs::remove_dir_all(work).map_err(|e| Error::io(work.display(), e))?;
    }
    mkdir(work)?;
    write(&work.join(MARK), b"")
}

fn mkdir(p: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
        .map_err(|e| Error::io(p.display(), e))
}

fn write(p: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = p.parent() {
        mkdir(parent)?;
    }
    std::fs::write(p, bytes).map_err(|e| Error::io(p.display(), e))
}

/// Read and execute: the sysroot (`rustc`, `rust-lld`, the standard
/// library) and the system's shared libraries.
fn exec(sysroot: &Path) -> Vec<PathBuf> {
    let mut v = vec![sysroot.to_path_buf()];
    v.extend(["/usr/lib", "/usr/lib64", "/lib", "/lib64"].map(PathBuf::from));
    v
}

/// The paths of the profile.
struct Rules {
    exec: Vec<PathBuf>,
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
}

struct Ran {
    status: ExitStatus,
    stderr: String,
    profile: Vec<String>,
    missing: Option<String>,
    /// Ended by the bound of time.
    late: bool,
}

/// Bound on what is kept of `rustc`'s messages.
const MAX_MESSAGES: u64 = 1 << 20;

fn failed(what: &str, ran: &Ran, options: &Options) -> Error {
    use std::os::unix::process::ExitStatusExt;
    let why = match ran.status.signal() {
        _ if ran.late => format!("past {} s, ended", options.bounds.time.as_secs()),
        Some(25) => format!(
            "a file past {} MiB, ended",
            (4 * options.bounds.module) >> 20
        ),
        Some(s) => format!("signal {s} (memory: {} GiB)", options.bounds.memory >> 30),
        None => "the build failed".into(),
    };
    let text = ran.stderr.trim_end();
    let message = match text.is_empty() {
        true => format!("{what}: {why}"),
        false => format!("{what}: {why}:\n{text}"),
    };
    err(message)
}

/// `rustc` with `args` in `cwd`, the environment cleared but `env`,
/// under the profile, in its own process group, bounded. From a thread
/// of its own: Landlock applies to the thread that asks and what it
/// spawns, the rest of the process is left as it was.
fn confined(
    rustc: &Path,
    args: &[OsString],
    cwd: &Path,
    env: &[(&str, &std::ffi::OsStr)],
    rules: &Rules,
    deadline: Instant,
    options: &Options,
) -> Result<Ran> {
    std::thread::scope(|s| {
        s.spawn(|| -> Result<Ran> {
            let (profile, missing) = match confine::apply(rules) {
                Ok(lines) => (lines, None),
                Err(why) if options.unconfined => (Vec::new(), Some(why)),
                Err(why) => {
                    return Err(tool_err(format!("no OS profile for the build: {why}"))
                        .with_hint("--force builds it without one: its sources may then read any file of yours into the module"));
                }
            };
            use std::os::unix::process::CommandExt;
            let mut child = Command::new(rustc)
                .args(args)
                .current_dir(cwd)
                .env_clear()
                .envs(env.iter().copied())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()
                .map_err(|e| tool_err(format!("{}: {e}", rustc.display())).with_hint(GET))?;
            let pid = rustix::process::Pid::from_child(&child);
            limits(pid, &options.bounds);
            let mut pipe = child.stderr.take();
            let reader = std::thread::spawn(move || {
                let mut text = String::new();
                if let Some(p) = pipe.as_mut() {
                    let _ = p.take(MAX_MESSAGES).read_to_string(&mut text);
                    // The rest, dropped, so that rustc never blocks.
                    let _ = std::io::copy(p, &mut std::io::sink());
                }
                text
            });
            let mut late = false;
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) if Instant::now() >= deadline => {
                        late = true;
                        let _ = rustix::process::kill_process_group(
                            pid,
                            rustix::process::Signal::KILL,
                        );
                        break child.wait().map_err(|e| tool_err(format!("rustc: {e}")))?;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(e) => return Err(tool_err(format!("rustc: {e}"))),
                }
            };
            // A linker left behind, if any: the group ends with rustc.
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            let stderr = reader.join().unwrap_or_default();
            Ok(Ran {
                status,
                stderr,
                profile,
                missing,
                late,
            })
        })
        .join()
        .unwrap_or_else(|_| Err(tool_err("the build: a thread panicked")))
    })
}

/// The address space, a file written, no core: set on the child once
/// spawned, before it has done much.
fn limits(pid: rustix::process::Pid, bounds: &Bounds) {
    use rustix::process::{Resource, Rlimit, prlimit};
    let set = |r, n: u64| {
        let _ = prlimit(
            Some(pid),
            r,
            Rlimit {
                current: Some(n),
                maximum: Some(n),
            },
        );
    };
    set(Resource::As, bounds.memory);
    set(Resource::Fsize, 4 * bounds.module);
    set(Resource::Core, 0);
}

#[cfg(target_os = "linux")]
mod confine {
    use super::Rules;
    use landlock::{
        ABI, Access, AccessFs, AccessNet, LandlockStatus, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus, Scope, path_beneath_rules,
    };

    /// The newest ABI this build knows.
    const LATEST: ABI = ABI::V9;

    /// Restricts the calling thread; what it spawns inherits it. `Err`:
    /// why it could not.
    pub fn apply(rules: &Rules) -> Result<Vec<String>, String> {
        let read = AccessFs::ReadFile | AccessFs::ReadDir;
        let done = Ruleset::default()
            .handle_access(AccessFs::from_all(LATEST))
            .and_then(|r| r.handle_access(AccessNet::from_all(LATEST)))
            .and_then(|r| r.scope(Scope::from_all(LATEST)))
            .and_then(|r| r.create())
            .and_then(|r| r.add_rules(path_beneath_rules(&rules.exec, AccessFs::from_read(LATEST))))
            .and_then(|r| r.add_rules(path_beneath_rules(&rules.read, read)))
            .and_then(|r| r.add_rules(path_beneath_rules(&rules.write, AccessFs::from_all(LATEST))))
            .and_then(|r| r.restrict_self())
            .map_err(|e| format!("landlock: {e}"))?;
        let abi = match done.landlock {
            LandlockStatus::Available { effective_abi, .. } => effective_abi,
            LandlockStatus::NotEnabled => return Err("landlock: not enabled at boot".into()),
            LandlockStatus::NotImplemented => return Err("landlock: not in this kernel".into()),
        };
        if done.ruleset == RulesetStatus::NotEnforced {
            return Err("landlock: not enforced".into());
        }
        let shown = |v: &[std::path::PathBuf]| {
            v.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut lines = vec![
            format!(
                "landlock ABI {}: read and execute {}",
                abi as i32,
                shown(&rules.exec)
            ),
            format!("read {}", shown(&rules.read)),
            format!("write {}", shown(&rules.write)),
        ];
        lines.push(match abi >= ABI::V4 {
            true => "no TCP bind or connect".into(),
            false => "MISSING: TCP (landlock ABI 4)".into(),
        });
        lines.push(match abi >= ABI::V6 {
            true => "no abstract unix socket, no signal out".into(),
            false => "MISSING: abstract unix sockets, signals (landlock ABI 6)".into(),
        });
        Ok(lines)
    }
}

#[cfg(not(target_os = "linux"))]
mod confine {
    use super::Rules;

    pub fn apply(_: &Rules) -> Result<Vec<String>, String> {
        Err(format!("no OS profile on {}", std::env::consts::OS))
    }
}

fn leb(b: &[u8], i: &mut usize) -> Result<u64> {
    let (mut n, mut shift) = (0u64, 0);
    loop {
        let x = *b
            .get(*i)
            .ok_or_else(|| err(format!("{MODULE}: cut short")))?;
        *i += 1;
        if shift < 64 {
            n |= u64::from(x & 0x7f) << shift;
        }
        shift += 7;
        if x < 0x80 {
            return Ok(n);
        }
        if shift > 70 {
            return Err(err(format!("{MODULE}: a number too long")));
        }
    }
}

fn name(b: &[u8], i: &mut usize) -> Result<String> {
    let n = leb(b, i)? as usize;
    let end = i.checked_add(n).filter(|e| *e <= b.len());
    let end = end.ok_or_else(|| err(format!("{MODULE}: cut short")))?;
    let s = String::from_utf8_lossy(&b[*i..end]).into_owned();
    *i = end;
    Ok(s)
}

/// `(module, name)` of each import of a WebAssembly module.
pub fn imports(b: &[u8]) -> Result<Vec<(String, String)>> {
    if b.len() < 8 || &b[..8] != b"\0asm\x01\0\0\0" {
        return Err(err(format!("{MODULE}: not a WebAssembly module")));
    }
    let (mut i, mut out) = (8, Vec::new());
    while i < b.len() {
        let id = b[i];
        i += 1;
        let size = leb(b, &mut i)? as usize;
        let end = i
            .checked_add(size)
            .filter(|e| *e <= b.len())
            .ok_or_else(|| err(format!("{MODULE}: cut short")))?;
        if id == 2 {
            let mut j = i;
            for _ in 0..leb(b, &mut j)? {
                let (m, f) = (name(b, &mut j)?, name(b, &mut j)?);
                let kind = *b
                    .get(j)
                    .ok_or_else(|| err(format!("{MODULE}: cut short")))?;
                j += 1;
                match kind {
                    // A function, a tag: an index (a tag: its attribute first).
                    0 => {
                        leb(b, &mut j)?;
                    }
                    4 => {
                        j += 1;
                        leb(b, &mut j)?;
                    }
                    // A table: its type, then limits; a memory: limits.
                    1 | 2 => {
                        if kind == 1 {
                            j += 1;
                        }
                        let flags = *b
                            .get(j)
                            .ok_or_else(|| err(format!("{MODULE}: cut short")))?;
                        j += 1;
                        leb(b, &mut j)?;
                        if flags & 1 != 0 {
                            leb(b, &mut j)?;
                        }
                    }
                    // A global: its type, mutable or not.
                    3 => j += 2,
                    _ => return Err(err(format!("{MODULE}: an import of kind {kind}"))),
                }
                out.push((m, f));
            }
        }
        i = end;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wall without the scan: `rustc` itself refused a file out of
    /// the work directory, and the network.
    #[test]
    #[cfg(target_os = "linux")]
    fn the_profile_holds_rustc() {
        let t = match Toolchain::find() {
            Ok(t) => t,
            Err(e) if std::env::var_os("PTYRC_REQUIRE_RUSTC").is_some_and(|v| v == "1") => {
                panic!("PTYRC_REQUIRE_RUSTC=1: {e}")
            }
            Err(_) => return,
        };
        let root = std::env::temp_dir().join(format!("profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (src, out, tmp) = (root.join("src"), root.join("out"), root.join("tmp"));
        for d in [&src, &out, &tmp] {
            std::fs::create_dir_all(d).unwrap();
        }
        let secret = root.join("secret.txt");
        std::fs::write(&secret, "the user's").unwrap();
        let lib = format!(
            "pub static S: &str = include_str!({:?});\n",
            secret.display().to_string()
        );
        std::fs::write(src.join("lib.rs"), lib).unwrap();
        let rules = Rules {
            exec: exec(&t.sysroot),
            read: vec!["/etc/ld.so.cache".into(), src.clone()],
            write: vec![out.clone(), tmp.clone(), "/dev/null".into()],
        };
        let args: Vec<OsString> = [
            "--edition",
            "2024",
            "--target",
            TARGET,
            "--crate-type",
            "rlib",
            "-o",
            "../out/x.rlib",
            "lib.rs",
        ]
        .map(OsString::from)
        .into();
        let deadline = Instant::now() + Duration::from_secs(60);
        let ran = confined(
            &t.rustc,
            &args,
            &src,
            &[],
            &rules,
            deadline,
            &Options::default(),
        );
        let ran = ran.unwrap();
        assert!(!ran.status.success());
        assert!(ran.stderr.contains("Permission denied"), "{}", ran.stderr);
        assert!(
            ran.profile.iter().any(|l| l == "no TCP bind or connect"),
            "{:?}",
            ran.profile
        );
        // The same, the file inside: built.
        std::fs::write(
            src.join("lib.rs"),
            "pub static S: &str = include_str!(\"lib.rs\");\n",
        )
        .unwrap();
        let ran = confined(
            &t.rustc,
            &args,
            &src,
            &[],
            &rules,
            deadline,
            &Options::default(),
        );
        assert!(ran.unwrap().status.success());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dep_info_read() {
        let d =
            "out/app.d: src/lib.rs src/a\\ b.rs ../x.rs\n\napp.wasm: src/lib.rs\n\nsrc/lib.rs:\n";
        assert_eq!(dep_info(d), ["src/lib.rs", "src/a b.rs", "../x.rs"]);
        assert!(dep_info("").is_empty());
    }

    #[test]
    fn versions() {
        assert_eq!(version("1.96.0"), Some((1, 96, 0)));
        assert_eq!(version("1.97.0-nightly"), Some((1, 97, 0)));
        assert_eq!(version("1.85"), Some((1, 85, 0)));
        assert_eq!(version("x"), None);
    }

    #[test]
    fn imports_read() {
        // (import "ptyrc" "log" (func)) (import "env" "m" (memory 1 2))
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        m.extend([1, 4, 1, 0x60, 0, 0]);
        let body = [
            &[2u8][..],
            &[5],
            b"ptyrc",
            &[3],
            b"log",
            &[0, 0],
            &[3],
            b"env",
            &[1],
            b"m",
            &[2, 1, 1, 2],
        ]
        .concat();
        m.push(2);
        m.push(body.len() as u8);
        m.extend(body);
        let got = imports(&m).unwrap();
        assert_eq!(
            got,
            [("ptyrc".into(), "log".into()), ("env".into(), "m".into())]
        );
        assert!(imports(b"\0asm\x01\0\0\0\x02\x05").is_err());
        assert!(imports(b"junk").is_err());
    }
}
