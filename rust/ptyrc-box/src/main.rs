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
//! `--words`: the requests checked as the driver checks a store app;
//! `--user-word`, `--user-pattern`, `--user-regex`: what the user gave
//! (`word`, `pattern` options); `--front`, `--back`, `--prefer`,
//! `--alpha`, `--border`: the launcher's options, applied as the driver
//! applies them. `--surface widget|frame|screen`: the manifest's, on
//! the one frame: draws clipped as the driver clips them.
//! `--event`, `--key`, `--pointer`, `--click`, `--press`, `--release`,
//! `--focus`, `--paste`:
//! what the user does, at a session time.
//! `--screen [T:]FILE`, again for each: the screen from T on.
//! `--clock ISO`, `--zone NAME`, `--storage DIR`, `--file NAME=PATH`,
//! `--write-file NAME=PATH` (the manifest's `write = true`),
//! `--probe git|system=FILE.json`: the host functions (`hostfn`).
//! `--assets DIR`, live too: `asset.*`, answered by the runner.
//! `--styles`: under each screen, its styles.
//!
//! `--unit FILE --script APP.js`: the unit tests of a script app:
//! FILE, a script of the tree of `APP.js` by relative path, its exports
//! `test_*` run in order, each under the limits of an `event` but
//! time (`script::UNIT_TEST`, `UNIT_FILE`); no
//! session, `call` refused. The report on stdout; status 0 when all
//! passed, 1 else (`script::unit`).
//!
//! `--manifest JSON`: the manifest as its reader (the driver, `ptyrc dev
//! test`) read it: handed to `init` under `ptyrc:manifest`; its
//! `[limits]` applied as far as `granted` (headless: the grants) holds
//! `limit-memory`, `limit-time` (`manifest.rs`).
//!
//! The OS profile (`profile::apply`) is applied once the files are
//! read and, live, the module loaded and the socket open.
//! `--no-profile`: not applied, for debugging; said on stderr.
//! `PTYRC_PROFILE=full`: a profile with a part missing is refused.
//!
//! Status: that of `exit`; 0 after SIGTERM; 70 a guest ended by the
//! runner; 2 usage; 5 the connection, or the profile refused.

use ptyrc_box::assets::Assets;
use ptyrc_box::grants::Grants;
use ptyrc_box::headless::{self, Inject, Options, Tape, Under};
use ptyrc_box::hostfn::{self, HostFns, Named, Zones};
use ptyrc_box::{Error, Limits, live, manifest, profile, script};
use ptyrc_proto::{Client, session};
use ptyrc_rules::surface::Surface;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const USAGE: &str =
    "usage: ptyrc-box [--no-profile] [--manifest JSON] [--assets DIR] [--seed N] [--script] MODULE [ARGS]
       ptyrc-box --headless [--no-profile] [--manifest JSON] [--assets DIR] [--script] MODULE [ARGS]
                 [--size CxR] [--for T] [--at T,...] [--seed N]
                 [--tape FILE | --screen [T:]FILE...] [--json] [--styles]
                 [--grant NAME,...] [--deny NAME,...] [--words WORD,...]
                 [--event T:TOPIC:JSON]... [--key T:KEY]... [--pointer T:X,Y]...
                 [--click T:X,Y]... [--press T:X,Y]... [--release T:X,Y]...
                 [--focus T:TAG]... [--paste T:TEXT]... [--resize T:CxR]...
                 [--clock ISO] [--zone NAME] [--storage DIR] [--file NAME=PATH]...
                 [--write-file NAME=PATH]... [--probe git|system=FILE.json]...
                 [--user-word WORD]... [--user-pattern TEXT]... [--user-regex RE]...
                 [--front | --back] [--prefer P] [--alpha A] [--border B]
                 [--surface widget|frame|screen]
       ptyrc-box --unit FILE [--no-profile] [--manifest JSON] --script APP.js [ARGS]
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

/// `PTYRC_PROFILE=full`: refuse to start without the whole profile.
const ENV_PROFILE: &str = "PTYRC_PROFILE";

#[derive(Default)]
struct Cli {
    headless: bool,
    no_profile: bool,
    script: bool,
    unit: Option<String>,
    module: Option<OsString>,
    args: Option<OsString>,
    size: Option<(u32, u32)>,
    duration: Option<u64>,
    at: Vec<u64>,
    seed: Option<u64>,
    tape: Option<PathBuf>,
    /// `(ms, file)`.
    screen: Vec<(u64, PathBuf)>,
    json: bool,
    styles: bool,
    manifest: Option<manifest::Given>,
    grant: Option<Vec<String>>,
    deny: Vec<String>,
    words: Vec<String>,
    inject: Vec<(u64, Inject)>,
    /// `--clock`: Unix ms at session time 0.
    clock: Option<i64>,
    zone: Option<String>,
    storage: Option<PathBuf>,
    /// `(option, path, write)`.
    files: Vec<(String, PathBuf, bool)>,
    user_words: Vec<String>,
    /// `(pattern, regex)`.
    patterns: Vec<(String, bool)>,
    launcher: headless::Launcher,
    surface: Option<Surface>,
    /// `(git | system, file)`.
    probes: Vec<(String, PathBuf)>,
    assets: Option<PathBuf>,
}

/// `2026-11-20T12:00:00Z`, `...T12:00:00.250+01:00`: Unix ms.
fn iso_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let s = text.get(r)?;
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut rest = &text[19..];
    let mut ms = 0;
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        let digits = format!("{:0<3}", &frac[..n.min(3)]);
        ms = digits.parse::<i64>().ok()?;
        rest = &frac[n..];
    }
    let offset_s = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (oh, om) = rest[1..].split_once(':')?;
            if oh.len() != 2 || om.len() != 2 {
                return None;
            }
            let (oh, om): (i64, i64) = (oh.parse().ok()?, om.parse().ok()?);
            sign * (oh * 3600 + om * 60)
        }
    };
    let days = ptyrc_core::zone::days_from_civil(y, mo as u8, d as u8);
    Some((days * 86_400 + h * 3600 + mi * 60 + sec - offset_s) * 1000 + ms)
}

/// `NAME=VALUE` of `flag`.
fn pair(flag: &str, v: &str) -> Result<(String, PathBuf), String> {
    match v.split_once('=') {
        Some((n, p)) if !n.is_empty() && !p.is_empty() => Ok((n.to_owned(), PathBuf::from(p))),
        _ => Err(format!("{flag} {v}: NAME=PATH")),
    }
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
            "--unit" => cli.unit = Some(string(value("--unit")?)),
            "--profile" => cli.no_profile = false,
            "--no-profile" => cli.no_profile = true,
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
            flag @ ("--pointer" | "--click" | "--press" | "--release") => {
                let v = string(value(flag)?);
                let (t, xy) = timed(flag, &v)?;
                let cell = (xy.split_once(','))
                    .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                let (x, y) = cell.ok_or_else(|| format!("{flag} {v}: T:X,Y"))?;
                cli.inject.push((
                    t,
                    match flag {
                        "--pointer" => Inject::Pointer(x, y),
                        "--press" => Inject::Press(x, y),
                        "--release" => Inject::Release(x, y),
                        _ => Inject::Click(x, y),
                    },
                ));
            }
            "--focus" => {
                let v = string(value("--focus")?);
                let (t, tag) = timed("--focus", &v)?;
                cli.inject.push((t, Inject::Focus(tag.to_owned())));
            }
            "--paste" => {
                let v = string(value("--paste")?);
                let (t, text) = timed("--paste", &v)?;
                cli.inject.push((t, Inject::Paste(text.to_owned())));
            }
            "--resize" => {
                let v = string(value("--resize")?);
                let (t, size) = timed("--resize", &v)?;
                let size = (size.split_once('x'))
                    .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)))
                    .filter(|&(c, r): &(u16, u16)| {
                        (1..=4096).contains(&c) && (1..=4096).contains(&r)
                    });
                let (c, r) = size.ok_or_else(|| format!("--resize {v}: T:COLSxROWS"))?;
                cli.inject.push((t, Inject::Resize(c, r)));
            }
            "--tape" => cli.tape = Some(value("--tape")?.into()),
            "--screen" => {
                let v = value("--screen")?;
                let text = string(v.clone());
                // `T:FILE` when T is a time; else the whole is the file.
                let timed = (text.split_once(':')).and_then(|(t, f)| Some((millis(t)?, f)));
                cli.screen.push(match timed {
                    Some((t, f)) => (t, PathBuf::from(f)),
                    None => (0, PathBuf::from(v)),
                });
            }
            "--styles" => cli.styles = true,
            "--clock" => {
                let v = string(value("--clock")?);
                let ms = iso_ms(&v)
                    .ok_or_else(|| format!("--clock {v}: an instant, as 2026-11-20T12:00:00Z"))?;
                cli.clock = Some(ms);
            }
            "--zone" => cli.zone = Some(string(value("--zone")?)),
            "--storage" => cli.storage = Some(value("--storage")?.into()),
            flag @ ("--file" | "--write-file") => {
                let (n, p) = pair(flag, &string(value(flag)?))?;
                cli.files.push((n, p, flag == "--write-file"));
            }
            "--user-word" => cli.user_words.push(string(value("--user-word")?)),
            flag @ ("--user-pattern" | "--user-regex") => {
                let p = string(value(flag)?);
                cli.patterns.push((p, flag == "--user-regex"));
            }
            "--front" => cli.launcher.front = Some(true),
            "--back" => cli.launcher.front = Some(false),
            "--prefer" => cli.launcher.prefer = Some(string(value("--prefer")?)),
            "--alpha" => {
                let v = string(value("--alpha")?);
                let a = v.parse::<f64>().ok().filter(|a| (0.0..=1.0).contains(a));
                cli.launcher.alpha = Some(a.ok_or_else(|| format!("--alpha {v}: 0 to 1"))?);
            }
            "--border" => cli.launcher.border = Some(string(value("--border")?)),
            "--surface" => {
                let v = string(value("--surface")?);
                let s = Surface::parse(&v, ptyrc_core::RAW);
                cli.surface =
                    Some(s.ok_or_else(|| format!("--surface {v}: widget, frame or screen"))?);
            }
            "--probe" => {
                let v = string(value("--probe")?);
                let (n, p) = pair("--probe", &v)?;
                if n != "git" && n != "system" {
                    return Err(format!("--probe {v}: git=FILE.json or system=FILE.json"));
                }
                cli.probes.push((n, p));
            }
            "--assets" => cli.assets = Some(value("--assets")?.into()),
            "--manifest" => {
                cli.manifest = Some(manifest::Given::parse(&string(value("--manifest")?))?);
            }
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
        || cli.tape.is_some()
        || !cli.screen.is_empty()
        || cli.json
        || cli.styles
        || cli.grant.is_some()
        || !cli.deny.is_empty()
        || !cli.words.is_empty()
        || !cli.inject.is_empty()
        || cli.clock.is_some()
        || cli.zone.is_some()
        || cli.storage.is_some()
        || !cli.files.is_empty()
        || !cli.probes.is_empty()
        || !cli.user_words.is_empty()
        || !cli.patterns.is_empty()
        || cli.launcher != headless::Launcher::default()
        || cli.surface.is_some();
    if headless_only && !cli.headless {
        return Err(
            "--size, --for, --at, --tape, --screen, --json, --styles, --grant, \
             --deny, --words, --event, --key, --pointer, --click, --press, --release, --focus, \
             --paste, --resize, --clock, \
             --zone, --storage, --file, --write-file, --probe, --user-word, --user-pattern, \
             --user-regex, --front, --back, --prefer, --alpha, --border, --surface: with \
             --headless"
                .into(),
        );
    }
    if cli.files.len() > hostfn::MAX_HANDLES {
        return Err(format!("--file: {} at most", hostfn::MAX_HANDLES));
    }
    if cli.tape.is_some() && (!cli.screen.is_empty() || cli.size.is_some()) {
        return Err("--tape gives the screen and its size: no --screen, no --size".into());
    }
    if cli.module.is_none() {
        return Err(String::new());
    }
    if cli.unit.is_some() && (cli.headless || !cli.script) {
        return Err("--unit: with --script, without --headless".into());
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
    // Unit tests: FILE of the tree in place of the entry.
    let main = match &cli.unit {
        Some(file) => file.trim_start_matches("./"),
        None => main,
    };
    if !sources.contains_key(main) {
        return Err(format!("{}: no such script", dir.join(main).display()));
    }
    Ok((
        script::interpreter()?,
        script::init_args(main, &sources, args),
    ))
}

/// Apply the OS profile, or say it is not. `told`: the lines of what
/// was applied are logged (what is missing always is).
fn confine(cli: &Cli, told: bool) -> Result<(), String> {
    if cli.no_profile {
        eprintln!("ptyrc-box: WARNING: --no-profile: the OS profile is NOT applied");
        return Ok(());
    }
    let status = profile::apply();
    for line in status.lines() {
        if told || line.contains("MISSING") {
            eprintln!("ptyrc-box: {line}");
        }
    }
    let required = std::env::var_os(ENV_PROFILE).is_some_and(|v| v == "full");
    if required && !status.full() {
        return Err(format!("{ENV_PROFILE}=full: the profile is not whole"));
    }
    Ok(())
}

/// The argument of `init` with `ptyrc:manifest`, and the limits
/// granted; without `--manifest`, as given and the defaults.
fn given(cli: &Cli, args: serde_json::Value, granted: &[String]) -> (serde_json::Value, Limits) {
    match &cli.manifest {
        None => (args, Limits::default()),
        Some(m) => (
            manifest::with_member(args, m.member(granted)),
            m.limits(granted),
        ),
    }
}

fn headless_run(cli: &Cli, wasm: &[u8], args: serde_json::Value) -> ExitCode {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let under = match (&cli.tape, &cli.screen[..]) {
        (Some(p), _) => match read(p)
            .and_then(|t| Tape::parse(&t).map_err(|e| format!("{}: {e}", p.display())))
        {
            Ok(t) => Under::Tape(t),
            Err(e) => return fail(2, e),
        },
        (_, []) => Under::Empty,
        (_, files) => {
            let mut screens = Vec::new();
            for (t, p) in files {
                match read(p) {
                    Ok(text) => screens.push((*t, text)),
                    Err(e) => return fail(2, e),
                }
            }
            Under::Screens(screens)
        }
    };
    let d = Options::default();
    let grants = match (&cli.grant, cli.deny.is_empty()) {
        (Some(grant), _) => Some(Grants::new(grant, &cli.deny, &cli.words)),
        // Denied alone: every permission but these.
        (None, false) => {
            let all: Vec<String> = ALL.iter().map(|s| s.to_string()).collect();
            Some(Grants::new(&all, &cli.deny, &cli.words))
        }
        (None, true) => None,
    };
    let grants = grants.map(|mut g| {
        g.user_words = cli.user_words.clone();
        g.patterns = cli.patterns.clone();
        g
    });
    // Headless: what the grants hold, else all.
    let granted: Vec<String> = match &grants {
        Some(g) => g.held(),
        None => ALL.iter().map(|s| s.to_string()).collect(),
    };
    let host = match host_fns(cli, &granted) {
        Ok(h) => h,
        Err(e) => return fail(2, e),
    };
    let assets = match read_assets(cli) {
        Ok(a) => a,
        Err(e) => return fail(2, e),
    };
    if let Err(e) = confine(cli, false) {
        return fail(5, e);
    }
    let (args, limits) = given(cli, args, &granted);
    let options = Options {
        size: cli.size.unwrap_or(d.size),
        under,
        args,
        duration_ms: cli.duration.unwrap_or(d.duration_ms),
        at: cli.at.clone(),
        seed: cli.seed.unwrap_or(d.seed),
        inject: cli.inject.clone(),
        grants,
        limits,
        host,
        assets,
        launcher: cli.launcher.clone(),
        surface: cli.surface,
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
        print!("{}", report.text(cli.styles));
    }
    let status = report.result.as_ref().map_or(0, |r| r.1);
    match report.end {
        None | Some(Error::Exit(0)) => ExitCode::from(status),
        Some(Error::Exit(code)) => ExitCode::from(code.clamp(1, 255) as u8),
        Some(_) => ExitCode::from(70),
    }
}

/// `--assets DIR`, read whole.
fn read_assets(cli: &Cli) -> Result<Option<Assets>, String> {
    cli.assets.as_deref().map(Assets::read).transpose()
}

/// The clock, the zones, the storage, the files and the probes of a
/// headless run, read before the profile.
fn host_fns(cli: &Cli, granted: &[String]) -> Result<HostFns, String> {
    use ptyrc_core::zone::{self, Zone};
    let mut h = HostFns {
        wall0: cli.clock.unwrap_or(headless::DEFAULT_CLOCK),
        ..HostFns::default()
    };
    // The database read whole, only for an app that holds the clock.
    let clock = granted.iter().any(|g| g == "clock");
    if clock {
        h.zones = Zones::read(&zone::database());
    }
    if let Some(name) = &cli.zone {
        // Not held: the zone alone, checked, unused.
        let found = match clock {
            true => h.zones.get(name),
            false => zone::load(&zone::database(), name),
        };
        let zone = match found {
            Ok(z) => Some(z),
            Err(_) if name == "UTC" => Some(Zone::utc()),
            Err(_) => Zone::posix(name, name),
        };
        h.zone = Some(zone.ok_or_else(|| {
            format!("--zone {name}: not a zone of the database, nor a POSIX rule (UTC-2)")
        })?);
        if !clock {
            eprintln!("ptyrc-box: note: --zone {name}: clock not held, the zone unused");
        }
    }
    if let Some(dir) = &cli.storage {
        let list =
            std::fs::read_dir(dir).map_err(|e| format!("--storage {}: {e}", dir.display()))?;
        for entry in list.flatten() {
            let Some(key) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !ptyrc_box::assets::valid_component(&key)
                || !entry.file_type().is_ok_and(|t| t.is_file())
            {
                continue;
            }
            let bytes = std::fs::read(entry.path())
                .map_err(|e| format!("--storage {}: {e}", entry.path().display()))?;
            h.storage
                .insert(key, String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    for (option, path, write) in &cli.files {
        let mut named = Named::read(option, path).map_err(|e| format!("--file {option}: {e}"))?;
        named.write = *write;
        h.files.push(named);
    }
    for (name, path) in &cli.probes {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("--probe {name}: {}: {e}", path.display()))?;
        let answer = serde_json::from_str(&text)
            .map_err(|e| format!("--probe {name}: {}: {e}", path.display()))?;
        h.probes.insert(name.clone(), answer);
    }
    Ok(h)
}

fn unit_run(cli: &Cli, wasm: &[u8], args: serde_json::Value) -> ExitCode {
    if let Err(e) = confine(cli, false) {
        return fail(5, e);
    }
    let all: Vec<String> = ALL.iter().map(|s| s.to_string()).collect();
    let (args, mut limits) = given(cli, args, &all);
    // The author's code on the author's machine: a bound of its own.
    limits.hard = limits.hard.max(script::UNIT_TEST);
    limits.init = limits.init.max(script::UNIT_TEST);
    limits.late = limits.late.max(script::UNIT_TEST);
    let (lines, status) = script::unit(wasm, &args, limits);
    for line in lines {
        println!("{line}");
    }
    ExitCode::from(status)
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
    if cli.unit.is_some() {
        return unit_run(&cli, &wasm, args);
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
    // Live: the driver tells what is granted.
    let granted = (cli.manifest.as_ref())
        .and_then(|m| m.granted.clone())
        .unwrap_or_default();
    let (args, limits) = given(&cli, args, &granted);
    let mut host = live::Live::new(client, Box::new(std::io::stderr()));
    if let Some(seed) = cli.seed {
        host = host.seeded(seed);
    }
    host.assets = match read_assets(&cli) {
        Ok(a) => a,
        Err(e) => return fail(2, e),
    };
    let confined = || confine(&cli, true).map_err(Error::Host);
    match live::run_confined(&wasm, host, &args, limits, rate, term, confined) {
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(e @ Error::Host(_)) => fail(5, e),
        Err(e) => fail(70, e),
    }
}
