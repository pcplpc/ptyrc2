// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev test` and `ptyrc dev run`: the app at a directory, run by
//! the `ptyrc-box` program (this one does not link the engine).

use crate::store::io_text;
use crate::{Common, Fail};
use ptyrc_proto::session;
use ptyrc_store::manifest::{self, MANIFEST};
use ptyrc_store::{Entry, Manifest, OptionValue};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime};

const BOX: &str = "ptyrc-box";
const TARGET: &str = "wasm32-unknown-unknown";

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

fn option_json(v: &OptionValue) -> Value {
    match v {
        OptionValue::Int(n) => json!(n),
        OptionValue::Float(f) => json!(f),
        OptionValue::Bool(b) => json!(b),
        OptionValue::Str(s) => json!(s),
    }
}

/// The argument of `init`: the defaults of `[options]`, then each
/// `NAME=VALUE`, of the type of its default.
fn init_args(m: &Manifest, given: &[String]) -> Result<Value, Fail> {
    let mut args: Map<String, Value> = m
        .options
        .iter()
        .map(|o| (o.name.clone(), option_json(&o.default)))
        .collect();
    let names = || {
        let n: Vec<&str> = m.options.iter().map(|o| o.name.as_str()).collect();
        if n.is_empty() {
            "the manifest has no [options]".to_string()
        } else {
            format!("[options]: {}", n.join(", "))
        }
    };
    for arg in given {
        let Some((name, text)) = arg.split_once('=') else {
            return Err(Fail::usage(format!("{arg}: not NAME=VALUE"), names()));
        };
        let Some(opt) = m.options.iter().find(|o| o.name == name) else {
            return Err(Fail::usage(format!("{name}: no such option"), names()));
        };
        let value = match &opt.default {
            OptionValue::Int(_) => text.parse::<i64>().ok().map(Value::from),
            OptionValue::Float(_) => text.parse::<f64>().ok().map(Value::from),
            OptionValue::Bool(_) => text.parse::<bool>().ok().map(Value::from),
            OptionValue::Str(_) => Some(Value::from(text)),
        };
        let value = value.ok_or_else(|| {
            Fail::usage(
                format!("{name}={text}: not a {}", opt.default.type_name()),
                format!("{name}: {}", opt.doc),
            )
        })?;
        args.insert(name.to_string(), value);
    }
    Ok(Value::Object(args))
}

/// The arguments of `ptyrc-box` that name the guest: `--script APP.js`,
/// or the module, built.
fn guest(dir: &Path, m: &Manifest) -> Result<Vec<OsString>, Fail> {
    match &m.app.entry {
        Entry::Script(main) => Ok(vec!["--script".into(), dir.join(main).into()]),
        Entry::Rust(_) => Ok(vec![build(dir)?.into()]),
    }
}

/// `cargo build --release --target wasm32-unknown-unknown`; its
/// messages on stderr. The module it made.
fn build(dir: &Path) -> Result<PathBuf, Fail> {
    let toml = dir.join("Cargo.toml");
    let out = Command::new("cargo")
        .args(["build", "--release", "--target", TARGET])
        .args(["--message-format", "json-render-diagnostics"])
        .arg("--manifest-path")
        .arg(&toml)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| Fail {
            status: 127,
            message: format!("cargo: {}", io_text(&e)),
            hint: Some("a Rust app needs cargo: https://rustup.rs".into()),
        })?;
    if !out.status.success() {
        return Err(Fail {
            status: 1,
            message: format!("{}: the build failed", toml.display()),
            hint: Some(format!(
                "the errors above; the target, once: rustup target add {TARGET}"
            )),
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let module = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .filter(|v| {
            v.pointer("/target/kind")
                .and_then(Value::as_array)
                .is_some_and(|k| k.iter().any(|k| k == "cdylib"))
        })
        .flat_map(|v| {
            v["filenames"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect::<Vec<_>>()
        })
        .find(|f| f.extension().is_some_and(|e| e == "wasm"));
    module.ok_or_else(|| Fail {
        status: 1,
        message: format!("{}: no .wasm built", toml.display()),
        hint: Some("[lib] crate-type = [\"cdylib\"]".into()),
    })
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
    /// `ptyrc-box --headless` flags, as given.
    pub flags: Vec<OsString>,
}

pub fn test(o: TestOpts) -> Result<ExitCode, Fail> {
    let m = manifest_of(&o.dir)?;
    let args = init_args(&m, &o.args)?;
    let program = runner()?;
    let status = Command::new(&program)
        .arg("--headless")
        .args(guest(&o.dir, &m)?)
        .arg(args.to_string())
        .args(&o.flags)
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

/// `dev run`: the app in the session; started again when a file of
/// the tree changes. Ends with Ctrl-C.
pub fn run(common: &Common, dir: &Path, given: &[String]) -> Result<ExitCode, Fail> {
    let program = runner()?;
    let socket = socket(common)?;
    let poll = Duration::from_millis(300);
    let mut seen = BTreeMap::new();
    snapshot(dir, &mut seen, 0);
    loop {
        // A failed start waits for a change, as an exit does.
        let mut child = match manifest_of(dir).and_then(|m| {
            let args = init_args(&m, given)?;
            let guest = guest(dir, &m)?;
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
            std::thread::sleep(poll);
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
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
