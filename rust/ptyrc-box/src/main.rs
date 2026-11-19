// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc-box MODULE [ARGS]`: run one guest in the session of
//! `PTYRC_SOCKET`, with the token of `PTYRC_TOKEN`. `ARGS`: the JSON
//! given to `init` (default `{}`). The app's log is stderr.
//!
//! Status: that of `exit`; 0 after SIGTERM; 70 a guest ended by the
//! runner; 2 usage; 5 the connection.

use ptyrc_box::{Limits, live};
use ptyrc_proto::{Client, session};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn fail(status: u8, text: impl std::fmt::Display) -> ExitCode {
    eprintln!("ptyrc-box: {text}");
    ExitCode::from(status)
}

fn main() -> ExitCode {
    let mut argv = std::env::args_os().skip(1);
    let (Some(module), args, None) = (argv.next(), argv.next(), argv.next()) else {
        return fail(2, "usage: ptyrc-box MODULE [ARGS]");
    };
    let args = match args.map(|a| serde_json::from_str(&a.to_string_lossy())) {
        None => serde_json::json!({}),
        Some(Ok(v)) => v,
        Some(Err(e)) => return fail(2, format!("ARGS: {e}")),
    };
    let wasm = match std::fs::read(&module) {
        Ok(w) => w,
        Err(e) => return fail(2, format!("{}: {e}", module.to_string_lossy())),
    };
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
        Err(e @ ptyrc_box::Error::Host(_)) => fail(5, e),
        Err(e) => fail(70, e),
    }
}
