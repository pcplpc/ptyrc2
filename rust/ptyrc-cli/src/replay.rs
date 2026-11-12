// SPDX-License-Identifier: GPL-3.0-or-later
//! Tapes: `ptyrc replay`, `ptyrc tape`, `ptyrc snap`.

use crate::{Common, Fail, connect, plugins, print_json};
use clap::{Args, Subcommand};
use ptyrc_proto::{Client, session};
use ptyrc_server::Tape;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

type Done = Result<ExitCode, Fail>;

#[derive(Args)]
pub struct Replay {
    /// A tape written by `ptyrc run --record`
    file: PathBuf,
    /// Session name [default: the driver pid]
    #[arg(long)]
    name: Option<String>,
    /// Tape seconds per second; 0: no waiting
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
    /// Pause at this tape time: seconds, `end`, `end+N`
    #[arg(long, value_name = "T")]
    until: Option<String>,
    /// Start paused; go on with `ptyrc tape step`
    #[arg(long)]
    step: bool,
    /// Pause at the end of the tape in place of leaving
    #[arg(long)]
    hold: bool,
    /// Generic heuristics only: no per-app preset [default: as recorded]
    #[arg(long)]
    no_presets: bool,
    /// With the per-app presets, whatever the tape says
    #[arg(long, conflicts_with = "no_presets")]
    presets: bool,
    /// Print a snapshot at each `--at` as JSON, then leave
    #[arg(long)]
    snap: bool,
    /// Tape times of the snapshots [default: those of the --check file,
    /// else end+3]
    #[arg(long, value_name = "T,...", value_delimiter = ',')]
    at: Vec<String>,
    /// Compare the snapshots with this file; status 1 when they differ
    #[arg(long, value_name = "FILE", conflicts_with = "snap")]
    check: Option<PathBuf>,
}

/// Seconds of tape time: `2.5`, `end`, `end+3`.
fn tape_time(text: &str, end: Duration) -> Result<Duration, Fail> {
    let bad = || {
        Fail::usage(
            format!("not a tape time: {text}"),
            "seconds, `end`, or `end+N`: 2.5, end, end+3",
        )
    };
    let seconds = |s: &str| {
        s.parse::<f64>()
            .ok()
            .filter(|s| s.is_finite() && *s >= 0.0)
            .map(Duration::from_secs_f64)
    };
    match text.strip_prefix("end") {
        Some("") => Ok(end),
        Some(more) => more
            .strip_prefix('+')
            .and_then(seconds)
            .map(|d| end + d)
            .ok_or_else(bad),
        None => seconds(text).ok_or_else(bad),
    }
}

pub fn replay(args: Replay) -> Done {
    let tape = Tape::load(&args.file).map_err(|e| Fail {
        status: 2,
        message: format!("{}: {e}", args.file.display()),
        hint: Some("a tape is written by: ptyrc run --record FILE -- CMD".into()),
    })?;
    if !(args.speed.is_finite() && args.speed >= 0.0) {
        return Err(Fail::usage("bad speed", "--speed: a number, 0 or more"));
    }
    let end = tape.end();
    let snaps = args.snap || args.check.is_some();
    let until = match &args.until {
        Some(text) => Some(tape_time(text, end)?),
        None => None,
    };
    let config = ptyrc_server::Config {
        name: args.name.clone(),
        headless: true,
        ..Default::default()
    };
    let presets = args.presets || (!args.no_presets && tape.header.presets);
    if !snaps {
        let replay = ptyrc_server::Replay {
            tape,
            speed: args.speed,
            until,
            paused: args.step,
            hold: args.hold,
        };
        let factory = Box::new(move || plugins::engine(false, presets));
        let status = ptyrc_server::replay(config, replay, factory)?;
        return Ok(ExitCode::from(status.clamp(0, 255) as u8));
    }

    let expected = match &args.check {
        Some(path) => {
            let fail = |message: String, hint: &str| Fail {
                status: 2,
                message: format!("{}: {message}", path.display()),
                hint: Some(hint.into()),
            };
            let text = std::fs::read_to_string(path).map_err(|e| {
                fail(
                    e.to_string(),
                    "write it with: ptyrc replay TAPE --snap > FILE",
                )
            })?;
            let value: Value = serde_json::from_str(&text)
                .map_err(|e| fail(format!("not JSON: {e}"), "a file written by --snap"))?;
            Some(value)
        }
        None => None,
    };
    let mut at = Vec::new();
    for text in &args.at {
        at.push(tape_time(text, end)?);
    }
    if at.is_empty() {
        // The times of the snapshots compared with.
        let shots = expected.as_ref().and_then(Value::as_array);
        at.extend(
            shots
                .into_iter()
                .flatten()
                .filter_map(|shot| shot["t"].as_f64())
                .filter(|t| t.is_finite() && *t >= 0.0)
                .map(Duration::from_secs_f64),
        );
        at.sort();
    }
    if at.is_empty() {
        at.push(end + Duration::from_secs(3));
    }
    // The driver in a thread, paused; this one pilots it.
    let name = args
        .name
        .unwrap_or_else(|| format!("replay-{}", std::process::id()));
    let config = ptyrc_server::Config {
        name: Some(name.clone()),
        ..config
    };
    let replay = ptyrc_server::Replay {
        tape,
        speed: 0.0,
        until: None,
        paused: true,
        hold: true,
    };
    let driver = std::thread::spawn(move || {
        let factory = Box::new(move || plugins::engine(false, presets));
        ptyrc_server::replay(config, replay, factory)
    });
    let socket = session::socket_path(&name);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut client = loop {
        if driver.is_finished() {
            return match driver.join() {
                Ok(Err(e)) => Err(e.into()),
                _ => Err(Fail::from("the replay ended before it was piloted")),
            };
        }
        match Client::connect(&socket) {
            Ok(client) => break client,
            Err(e) if Instant::now() > deadline => return Err(e.into()),
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    let shots = (|| -> Result<Vec<Value>, Fail> {
        let mut shots = Vec::new();
        for t in at {
            client.call("tape.seek", json!({"t": t.as_secs_f64()}))?;
            let mut shot = snapshot(&mut client)?;
            let seconds = (t.as_secs_f64() * 1e6).round() / 1e6;
            shot["t"] = json!(seconds);
            shots.push(shot);
        }
        Ok(shots)
    })();
    let _ = client.call("tape.quit", json!({}));
    let _ = driver.join();
    let shots = Value::Array(shots?);
    let (Some(path), Some(expected)) = (args.check, expected) else {
        print_json(&shots, true);
        return Ok(ExitCode::SUCCESS);
    };
    let mut lines = Vec::new();
    differ("", &expected, &shots, &mut lines);
    if lines.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    eprintln!(
        "ptyrc: {} differs from {}",
        args.file.display(),
        path.display()
    );
    for line in lines.iter().take(40) {
        eprintln!("  {line}");
    }
    if lines.len() > 40 {
        eprintln!("  and {} more", lines.len() - 40);
    }
    Ok(ExitCode::from(1))
}

/// Where two JSON values differ: one line per leaf, the path first.
fn differ(path: &str, expected: &Value, got: &Value, out: &mut Vec<String>) {
    match (expected, got) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let sub = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                differ(
                    &sub,
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    out,
                );
            }
        }
        // Arrays of scalars (a rect, a size) are leaves.
        (Value::Array(a), Value::Array(b))
            if a.iter()
                .chain(b)
                .any(|v| v.is_object() || v.is_array() || v.is_string()) =>
        {
            for i in 0..a.len().max(b.len()) {
                differ(
                    &format!("{path}[{i}]"),
                    a.get(i).unwrap_or(&Value::Null),
                    b.get(i).unwrap_or(&Value::Null),
                    out,
                );
            }
        }
        _ if expected == got => {}
        _ => out.push(format!("{path}: expected {expected}, got {got}")),
    }
}

/// What a session shows and how it is segmented, without ids nor times:
/// the same for the same tape.
pub fn snapshot(client: &mut Client) -> Result<Value, Fail> {
    let info = client.call("session.info", json!({}))?;
    let text = client.call("screen.text", json!({}))?;
    let cursor = client.call("screen.cursor", json!({}))?;
    let frames = client.call("frame.list", json!({}))?;
    let frames: Vec<Value> = frames["frames"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            json!({
                "kind": f["kind"], "state": f["state"], "usable": f["usable"],
                "rect": f["rect"], "origin": f["origin"], "extent": f["extent"],
                "key": f["key"], "support": f["support"],
            })
        })
        .collect();
    Ok(json!({
        "size": info["size"],
        "alt_screen": info["alt_screen"],
        "title": info["title"],
        "cursor": [cursor["x"], cursor["y"], cursor["visible"]],
        "text": text["lines"],
        "frames": frames,
    }))
}

pub fn snap(common: &Common) -> Done {
    let shot = snapshot(&mut connect(common)?)?;
    print_json(&shot, !common.json);
    Ok(ExitCode::SUCCESS)
}

#[derive(Subcommand)]
pub enum TapeVerb {
    /// Where the tape is [default]
    Status,
    /// Feed the next N records, then pause
    Step {
        #[arg(default_value_t = 1)]
        records: u64,
    },
    /// Run to a tape time without waiting, then pause: seconds, `end`, `end+N`
    Seek { t: String },
    /// Go on, at SPEED tape seconds per second (0: no waiting)
    Play { speed: Option<f64> },
    /// Stop the tape clock
    Pause,
    /// End the replay
    Quit,
}

pub fn tape(common: &Common, verb: Option<TapeVerb>) -> Done {
    let mut client = connect(common)?;
    let status = match verb.unwrap_or(TapeVerb::Status) {
        TapeVerb::Status => client.call("tape.status", json!({}))?,
        TapeVerb::Step { records } => client.call("tape.step", json!({"records": records}))?,
        TapeVerb::Seek { t } => {
            let end = client.call("tape.status", json!({}))?["end"]
                .as_f64()
                .unwrap_or(0.0);
            let t = tape_time(&t, Duration::from_secs_f64(end))?;
            client.call("tape.seek", json!({"t": t.as_secs_f64()}))?
        }
        TapeVerb::Play { speed } => match speed {
            Some(speed) => client.call("tape.play", json!({"speed": speed}))?,
            None => client.call("tape.play", json!({}))?,
        },
        TapeVerb::Pause => client.call("tape.pause", json!({}))?,
        TapeVerb::Quit => {
            client.call("tape.quit", json!({}))?;
            return Ok(ExitCode::SUCCESS);
        }
    };
    if common.json {
        print_json(&status, false);
    } else {
        let state = match (status["paused"].as_bool(), status["done"].as_bool()) {
            (Some(true), _) => "paused".to_string(),
            (_, Some(true)) => "over".to_string(),
            _ => format!("playing x{}", status["speed"]),
        };
        println!(
            "{:.3} / {:.3} s  record {}/{}  {state}",
            status["t"].as_f64().unwrap_or(0.0),
            status["end"].as_f64().unwrap_or(0.0),
            status["next"],
            status["records"],
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tape_times() {
        let end = Duration::from_secs(4);
        assert_eq!(tape_time("2.5", end).unwrap(), Duration::from_millis(2500));
        assert_eq!(tape_time("end", end).unwrap(), end);
        assert_eq!(tape_time("end+3", end).unwrap(), Duration::from_secs(7));
        for bad in ["", "-1", "end-1", "endx", "soon"] {
            assert_eq!(tape_time(bad, end).unwrap_err().status, 2, "{bad}");
        }
    }

    #[test]
    fn differences_are_named() {
        let a = json!([{"text": ["a", "b"], "frames": [{"rect": [0, 0, 80, 24]}]}]);
        let b = json!([{"text": ["a", "c"], "frames": [{"rect": [0, 0, 80, 23]}]}]);
        let mut lines = Vec::new();
        differ("", &a, &b, &mut lines);
        assert_eq!(
            lines,
            [
                "[0].frames[0].rect: expected [0,0,80,24], got [0,0,80,23]",
                "[0].text[1]: expected \"b\", got \"c\"",
            ]
        );
        lines.clear();
        differ("", &a, &a, &mut lines);
        assert!(lines.is_empty());
    }
}
