// SPDX-License-Identifier: GPL-3.0-or-later
//! A run in a session: the driver's socket, its events, real time.

use crate::core::{CallError, Host, Limits};
use crate::error::Error;
use crate::runner::Runner;
use ptyrc_proto::{Client, ClientError};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How often the signals are looked at with no tick due.
const IDLE: Duration = Duration::from_millis(200);

/// The host of a live run. The connection has presented the token
/// already (`Client::connect`).
pub struct Live {
    client: Client,
    size: (u32, u32),
    log: Box<dyn Write + Send>,
    urandom: Option<File>,
    gone: Option<String>,
    /// The app's assets, read before the profile.
    pub assets: Option<crate::assets::Assets>,
}

fn size_of(v: &Value) -> Option<(u32, u32)> {
    let a = v.as_array()?;
    Some((a.first()?.as_u64()? as u32, a.get(1)?.as_u64()? as u32))
}

impl Live {
    pub fn new(client: Client, log: Box<dyn Write + Send>) -> Live {
        let size = size_of(&client.hello()["size"]).unwrap_or((80, 24));
        Live {
            client,
            size,
            log,
            urandom: File::open("/dev/urandom").ok(),
            gone: None,
            assets: None,
        }
    }
}

impl Host for Live {
    fn assets(&self) -> Option<&crate::assets::Assets> {
        self.assets.as_ref()
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        if let Some(gone) = &self.gone {
            return Err(CallError::new("gone", gone.clone()));
        }
        match self.client.call(method, params) {
            Ok(v) => Ok(v),
            Err(ClientError::Rpc(e)) => Err(CallError {
                code: e.code.as_str().to_string(),
                message: e.message,
                hint: e.hint,
            }),
            Err(e) => {
                self.gone = Some(e.to_string());
                Err(CallError::new("gone", e.to_string()))
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn log(&mut self, text: &str) {
        let _ = writeln!(self.log, "{text}");
    }

    fn random(&mut self, buf: &mut [u8]) {
        let read = self.urandom.as_mut().map(|f| f.read_exact(buf));
        if !matches!(read, Some(Ok(()))) {
            buf.fill(0);
            self.gone = Some("no /dev/urandom".into());
        }
    }
}

/// Run a module until it ends, the driver goes, or `term` is set
/// (`stop`, then the end). `rate`: session time over real time
/// (`PTYRC_CLOCK_RATE`). The exit status.
pub fn run(
    wasm: &[u8],
    host: Live,
    args: &Value,
    limits: Limits,
    rate: f64,
    term: Arc<AtomicBool>,
) -> Result<i32, Error> {
    run_confined(wasm, host, args, limits, rate, term, || Ok(()))
}

/// `run`, with `confine` called once the module is compiled and
/// instantiated, before `init`: the place of the OS profile
/// (`profile::apply`).
pub fn run_confined(
    wasm: &[u8],
    host: Live,
    args: &Value,
    limits: Limits,
    rate: f64,
    term: Arc<AtomicBool>,
    confine: impl FnOnce() -> Result<(), Error>,
) -> Result<i32, Error> {
    let mut runner = Runner::load(wasm, host, limits)?;
    confine()?;
    let start = Instant::now();
    let now = || (start.elapsed().as_secs_f64() * 1000.0 * rate) as u64;
    let exit = |done: Result<(), Error>| match done {
        Err(Error::Exit(code)) => Ok(Some(code)),
        Err(e) => Err(e),
        Ok(()) => Ok(None),
    };
    if let Some(code) = exit(runner.init(args, now()))? {
        return Ok(code);
    }
    let mut next_tick: Option<u64> = None;
    let mut told = false;
    loop {
        if term.load(Ordering::Relaxed) {
            return Ok(exit(runner.stop(now()))?.unwrap_or(0));
        }
        if let Some(gone) = runner.host().gone.clone() {
            return Err(Error::Host(gone));
        }
        if runner.paused() && !told {
            told = true;
            runner
                .host_mut()
                .log("paused: three late in a row; no tick, no event");
        }
        let period = u64::from(runner.period());
        next_tick = match (period, next_tick) {
            (0, _) => None,
            (p, None) => Some(now() + p),
            (_, some) => some,
        };
        let wait = match next_tick {
            Some(t) => {
                Duration::from_secs_f64(t.saturating_sub(now()) as f64 / 1000.0 / rate).min(IDLE)
            }
            None => IDLE,
        };
        let event = match runner.host_mut().client.next_event(Some(wait)) {
            Ok(e) => e,
            Err(e) => return Err(Error::Host(e.to_string())),
        };
        if let Some(e) = event {
            if e.event == "size"
                && let Some(size) = size_of(&e.data["size"]).or_else(|| size_of(&e.data))
            {
                runner.host_mut().size = size;
            }
            if let Some(code) = exit(runner.event(&e.event, &e.data, now()))? {
                return Ok(code);
            }
        }
        if let Some(t) = next_tick
            && now() >= t
        {
            let at = now();
            if let Some(code) = exit(runner.tick(at))? {
                return Ok(code);
            }
            // A tick missed is not made up for.
            next_tick = Some((t + period).max(at));
        }
    }
}
