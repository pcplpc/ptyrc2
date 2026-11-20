// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev probe git [DIR]`, `ptyrc dev probe system`: the answer of
//! the driver's probe now (`ptyrc-probe`), as JSON: a fixture for `dev
//! test --probe git=FILE.json`, `system=FILE.json`.

use crate::Fail;
use ptyrc_probe::{git, system};
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

fn now_s() -> f64 {
    let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    since.map_or(0.0, |d| d.as_secs_f64())
}

fn bad(e: String) -> Fail {
    Fail::usage(e, "ptyrc dev probe --help")
}

/// The params of the probe, as an app would give them.
fn params(parts: Option<&str>, count: Option<(&str, u64)>) -> Value {
    let mut p = json!({});
    if let Some(parts) = parts {
        let parts: Vec<&str> = parts
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        p["parts"] = json!(parts);
    }
    if let Some((name, n)) = count {
        p[name] = json!(n);
    }
    p
}

/// `probe.git` of the repository holding `dir`, now; `days` in UTC
/// (headless `--zone` by default).
pub fn git(dir: &Path, parts: Option<&str>, log: Option<u64>) -> Result<ExitCode, Fail> {
    let ask = git::Ask::from_json(&params(parts, log.map(|n| ("log", n)))).map_err(bad)?;
    let dir = dir
        .canonicalize()
        .map_err(|e| Fail::from(format!("{}: {e}", dir.display())))?;
    let answer = git::probe(
        &dir,
        &ask,
        git::Now {
            at: now_s(),
            offset: 0,
        },
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&answer).unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}

/// `probe.system` now: a second sample [`system::SAMPLE`] after the
/// first, so that the shares are not the zeros of a first answer.
pub fn system(parts: Option<&str>, top: Option<u64>) -> Result<ExitCode, Fail> {
    let ask = system::Ask::from_json(&params(parts, top.map(|n| ("top", n)))).map_err(bad)?;
    let mut sampler = system::Sampler::new();
    sampler.probe(&ask, now_s());
    std::thread::sleep(Duration::from_secs_f64(system::SAMPLE) + Duration::from_millis(50));
    let answer = sampler.probe(&ask, now_s());
    println!(
        "{}",
        serde_json::to_string_pretty(&answer).unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}
