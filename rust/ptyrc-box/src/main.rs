// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc-box [--script] MODULE [ARGS]`: run one guest in the session
//! of `PTYRC_SOCKET`, with the token of `PTYRC_TOKEN`. `ARGS`: the JSON
//! given to `init` (default `{}`). The app's log is stderr.
//!
//! `--script APP.js`: a script app; the guest is the interpreter
//! (`script::interpreter_path`), the sources are the scripts of the
//! directory of `APP.js`.
//!
//! `--headless`: no session; `headless::run`, the report on stdout
//! (`--json`: one JSON value), the log on stderr. `--grant`, `--deny`,
//! `--words`: the requests checked as the driver checks a store app.
//! `--event`, `--key`: what the user does, at a session time.
//!
//! Status: that of `exit`; 0 after SIGTERM; 70 a guest ended by the
//! runner; 2 usage; 5 the connection.

use ptyrc_box::grants::Grants;
use ptyrc_box::headless::{self, Inject, Options, Tape, Under};
use ptyrc_box::{Error, Limits, live, script};
use ptyrc_proto::{Client, session};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const USAGE: &str = "usage: ptyrc-box [--script] MODULE [ARGS]
       ptyrc-box --headless [--script] MODULE [ARGS] [--size CxR] [--for T]
                 [--at T,...] [--seed N] [--tape FILE | --screen FILE] [--json]
                 [--grant NAME,...] [--deny NAME,...] [--words WORD,...]
                 [--event T:TOPIC:JSON]... [--key T:KEY]...
T: milliseconds, or with a unit: 500ms, 2s";

/// The permissions of the plan (`ptyrc-store`, `PERMISSIONS`).
const ALL: &[&str] = &[
    "layout",
    "shape",
    "words",
    "text",
    "front-widget",
    "front",
    "screen-wide",
    "keys-arrows",
    "keys-grab",
    "keys-read",
    "pointer",
    "clicks",
    "clicks-grab",
    "clock",
    "storage",
    "files",
    "files-write",
    "probe-git",
    "probe-system",
    "limit-memory",
    "limit-time",
];

fn fail(status: u8, text: impl std::fmt::Display) -> ExitCode {
    eprintln!("ptyrc-box: {text}");
    ExitCode::from(status)
}

/// `1500`, `1500ms`, `1.5s`: milliseconds.
fn millis(text: &str) -> Option<u64> {
    let (n, scale) = if let Some(n) = text.strip_suffix("ms") {
        (n, 1.0)
    } else if let Some(n) = text.strip_suffix('s') {
        (n, 1000.0)
    } else {
        (text, 1.0)
    };
    let v: f64 = n.parse().ok()?;
    (v.is_finite() && v >= 0.0 && v * scale < 1e15).then(|| (v * scale).round() as u64)
}

#[derive(Default)]
struct Cli {
    headless: bool,
    script: bool,
    module: Option<OsString>,
    args: Option<OsString>,
    size: Option<(u32, u32)>,
    duration: Option<u64>,
    at: Vec<u64>,
    seed: Option<u64>,
    tape: Option<PathBuf>,
    screen: Option<PathBuf>,
    json: bool,
    grant: Option<Vec<String>>,
    deny: Vec<String>,
    words: Vec<String>,
    inject: Vec<(u64, Inject)>,
}

fn list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `T:REST`: the time and the rest.
fn timed<'a>(flag: &str, v: &'a str) -> Result<(u64, &'a str), String> {
    let (t, rest) = v
        .split_once(':')
        .ok_or_else(|| format!("{flag} {v}: T:..."))?;
    let t = millis(t).ok_or_else(|| format!("{flag} {v}: {t}: a time"))?;
    Ok((t, rest))
}

fn parse(argv: impl Iterator<Item = OsString>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut argv = argv.peekable();
    while let Some(arg) = argv.next() {
        let text = arg.to_string_lossy().into_owned();
        let mut value = |name: &str| -> Result<OsString, String> {
            argv.next()
                .ok_or_else(|| format!("{name}: a value is wanted"))
        };
        let string = |v: OsString| v.to_string_lossy().into_owned();
        match text.as_str() {
            "--headless" => cli.headless = true,
            "--script" => cli.script = true,
            "--size" => {
                let v = string(value("--size")?);
                let size = v
                    .split_once('x')
                    .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)))
                    .filter(|&(c, r): &(u32, u32)| {
                        (1..=4096).contains(&c) && (1..=4096).contains(&r)
                    });
                cli.size = Some(size.ok_or_else(|| format!("--size {v}: COLSxROWS, as 80x24"))?);
            }
            "--for" => {
                let v = string(value("--for")?);
                cli.duration = Some(millis(&v).ok_or_else(|| format!("--for {v}: a time"))?);
            }
            "--at" => {
                let v = string(value("--at")?);
                for t in v.split(',') {
                    cli.at
                        .push(millis(t.trim()).ok_or_else(|| format!("--at {t}: a time"))?);
                }
            }
            "--seed" => {
                let v = string(value("--seed")?);
                cli.seed = Some(v.parse().map_err(|_| format!("--seed {v}: an integer"))?);
            }
            "--json" => cli.json = true,
            "--grant" => {
                let v = string(value("--grant")?);
                cli.grant.get_or_insert_with(Vec::new).extend(list(&v));
            }
            "--deny" => cli.deny.extend(list(&string(value("--deny")?))),
            "--words" => cli.words.extend(list(&string(value("--words")?))),
            "--event" => {
                let v = string(value("--event")?);
                let (t, rest) = timed("--event", &v)?;
                let (topic, data) = rest.split_once(':').unwrap_or((rest, "{}"));
                let data = serde_json::from_str(data)
                    .map_err(|e| format!("--event {v}: the data: {e}"))?;
                if topic.is_empty() {
                    return Err(format!("--event {v}: T:TOPIC:JSON"));
                }
                cli.inject.push((t, Inject::Event(topic.to_owned(), data)));
            }
            "--key" => {
                let v = string(value("--key")?);
                let (t, key) = timed("--key", &v)?;
                if headless::key_bytes(key).is_none() && key != "ctrl+t" {
                    return Err(format!(
                        "--key {v}: {key}: not a key; {}",
                        headless::KEY_NAMES
                    ));
                }
                cli.inject.push((t, Inject::Key(key.to_owned())));
            }
            "--tape" => cli.tape = Some(value("--tape")?.into()),
            "--screen" => cli.screen = Some(value("--screen")?.into()),
            "--help" | "-h" => return Err(String::new()),
            t if t.starts_with("--") => return Err(format!("{t}: unknown option")),
            _ if cli.module.is_none() => cli.module = Some(arg),
            _ if cli.args.is_none() => cli.args = Some(arg),
            _ => return Err(format!("{text}: one MODULE, one ARGS")),
        }
    }
    let headless_only = cli.size.is_some()
        || cli.duration.is_some()
        || !cli.at.is_empty()
        || cli.seed.is_some()
        || cli.tape.is_some()
        || cli.screen.is_some()
        || cli.json
        || cli.grant.is_some()
        || !cli.deny.is_empty()
        || !cli.words.is_empty()
        || !cli.inject.is_empty();
    if headless_only && !cli.headless {
        return Err(
            "--size, --for, --at, --seed, --tape, --screen, --json, --grant, --deny, \
             --words, --event, --key: with --headless"
                .into(),
        );
    }
    if cli.tape.is_some() && (cli.screen.is_some() || cli.size.is_some()) {
        return Err("--tape gives the screen and its size: no --screen, no --size".into());
    }
    if cli.module.is_none() {
        return Err(String::new());
    }
    Ok(cli)
}

/// The module and the argument of `init`.
fn load(cli: &Cli, args: serde_json::Value) -> Result<(Vec<u8>, serde_json::Value), String> {
    let module = cli.module.as_deref().unwrap_or_default();
    if !cli.script {
        let wasm =
            std::fs::read(module).map_err(|e| format!("{}: {e}", module.to_string_lossy()))?;
        return Ok((wasm, args));
    }
    let app = Path::new(module);
    let (Some(main), Some(dir)) = (app.file_name().and_then(|n| n.to_str()), app.parent()) else {
        return Err(USAGE.into());
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let sources = script::sources(dir)?;
    if !sources.contains_key(main) {
        return Err(format!("{}: no such script", app.display()));
    }
    Ok((
        script::interpreter()?,
        script::init_args(main, &sources, args),
    ))
}

fn headless_run(cli: &Cli, wasm: &[u8], args: serde_json::Value) -> ExitCode {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let under = match (&cli.tape, &cli.screen) {
        (Some(p), _) => match read(p)
            .and_then(|t| Tape::parse(&t).map_err(|e| format!("{}: {e}", p.display())))
        {
            Ok(t) => Under::Tape(t),
            Err(e) => return fail(2, e),
        },
        (_, Some(p)) => match read(p) {
            Ok(t) => Under::Text(t),
            Err(e) => return fail(2, e),
        },
        _ => Under::Empty,
    };
    let d = Options::default();
    let options = Options {
        size: cli.size.unwrap_or(d.size),
        under,
        args,
        duration_ms: cli.duration.unwrap_or(d.duration_ms),
        at: cli.at.clone(),
        seed: cli.seed.unwrap_or(d.seed),
        inject: cli.inject.clone(),
        grants: match (&cli.grant, cli.deny.is_empty()) {
            (Some(grant), _) => Some(Grants::new(grant, &cli.deny, &cli.words)),
            // Denied alone: every permission but these.
            (None, false) => {
                let all: Vec<String> = ALL.iter().map(|s| s.to_string()).collect();
                Some(Grants::new(&all, &cli.deny, &cli.words))
            }
            (None, true) => None,
        },
        ..d
    };
    let report = match headless::run(wasm, &options) {
        Ok(r) => r,
        Err(e) => return fail(70, e),
    };
    for line in &report.log {
        eprintln!("{line}");
    }
    if cli.json {
        println!("{}", report.to_json());
    } else {
        print!("{report}");
    }
    match report.end {
        None | Some(Error::Exit(0)) => ExitCode::SUCCESS,
        Some(Error::Exit(code)) => ExitCode::from(code.clamp(1, 255) as u8),
        Some(_) => ExitCode::from(70),
    }
}

fn main() -> ExitCode {
    let cli = match parse(std::env::args_os().skip(1)) {
        Ok(c) => c,
        Err(e) if e.is_empty() => return fail(2, USAGE),
        Err(e) => return fail(2, format!("{e}\n{USAGE}")),
    };
    let args: serde_json::Value = match cli
        .args
        .as_ref()
        .map(|a| serde_json::from_str(&a.to_string_lossy()))
    {
        None => serde_json::json!({}),
        Some(Ok(v)) => v,
        Some(Err(e)) => return fail(2, format!("ARGS: {e}")),
    };
    let (wasm, args) = match load(&cli, args) {
        Ok(x) => x,
        Err(e) => return fail(2, e),
    };
    if cli.headless {
        return headless_run(&cli, &wasm, args);
    }
    let Some(socket) = std::env::var_os(session::ENV_SOCKET).map(PathBuf::from) else {
        return fail(2, format!("{} is not set", session::ENV_SOCKET));
    };
    let client = match Client::connect(&socket) {
        Ok(c) => c,
        Err(e) => return fail(5, e),
    };
    let rate = std::env::var("PTYRC_CLOCK_RATE")
        .ok()
        .and_then(|r| r.parse::<f64>().ok())
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(1.0);
    let term = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(e) = signal_hook::flag::register(signal, term.clone()) {
            return fail(5, e);
        }
    }
    let host = live::Live::new(client, Box::new(std::io::stderr()));
    match live::run(&wasm, host, &args, Limits::default(), rate, term) {
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(e @ Error::Host(_)) => fail(5, e),
        Err(e) => fail(70, e),
    }
}
