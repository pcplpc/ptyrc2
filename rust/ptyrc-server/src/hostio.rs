// SPDX-License-Identifier: GPL-3.0-or-later
//! The worker of the host functions: what may block (a disk, later
//! `git`) runs on one thread, in order, never on the driver's loop. A
//! job is a function of the worker's state; its answer completes the
//! pending reply from the loop, woken by a byte on a socket pair.

use crate::engine::Token;
use crate::zone::{self, Zone};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::SystemTime;

/// Jobs queued, at most; then `not-usable`.
pub(crate) const QUEUE: usize = 64;
/// Zones kept parsed, at most.
const ZONES_KEPT: usize = 64;

/// What the worker keeps between jobs. A new kind of request adds its
/// state here and its job as a function of it.
pub(crate) struct State {
    /// The zone database (`$TZDIR`, `/usr/share/zoneinfo`).
    pub tzdir: PathBuf,
    /// `$TZ` of the driver.
    pub tz: Option<String>,
    pub localtime: PathBuf,
    zones: HashMap<String, Zone>,
    /// `<state>/ptyrc/storage`; None: no home.
    pub storage: Option<PathBuf>,
}

impl State {
    pub fn from_env() -> State {
        State {
            tzdir: zone::database(),
            tz: std::env::var("TZ").ok(),
            localtime: PathBuf::from("/etc/localtime"),
            zones: HashMap::new(),
            storage: crate::storage::root(),
        }
    }

    /// The zone `name`, `local` the machine's; parsed once.
    pub fn zone(&mut self, name: &str) -> Result<Zone, zone::Missing> {
        if name == "local" {
            // Read each time: the machine's zone may change.
            return Ok(zone::local(
                &self.tzdir,
                self.tz.as_deref(),
                &self.localtime,
            ));
        }
        if let Some(zone) = self.zones.get(name) {
            return Ok(zone.clone());
        }
        let zone = zone::load(&self.tzdir, name)?;
        if self.zones.len() >= ZONES_KEPT {
            self.zones.clear();
        }
        self.zones.insert(name.to_string(), zone.clone());
        Ok(zone)
    }

    /// The wall clock, Unix ms: the server's, never scaled.
    pub fn wall_ms(&self) -> i64 {
        let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        since.map_or(0, |d| d.as_millis() as i64)
    }
}

pub(crate) type Job = Box<dyn FnOnce(&mut State) -> Result<Value, RpcError> + Send>;

/// The driver's end.
pub(crate) struct Hostio {
    jobs: SyncSender<(Token, Job)>,
    done: Receiver<(Token, Result<Value, RpcError>)>,
    wake: UnixStream,
}

impl Hostio {
    /// The worker, started.
    pub fn spawn(state: State) -> io::Result<Hostio> {
        let (jobs, inbox) = mpsc::sync_channel::<(Token, Job)>(QUEUE);
        let (outbox, done) = mpsc::channel();
        let (wake, mut ring) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        ring.set_nonblocking(true)?;
        std::thread::Builder::new()
            .name("ptyrc-hostio".into())
            .spawn(move || {
                let mut state = state;
                // Ends when the driver's end is dropped.
                while let Ok((token, job)) = inbox.recv() {
                    let answer = job(&mut state);
                    if outbox.send((token, answer)).is_err() {
                        break;
                    }
                    // Full: a byte already waits, the loop will drain.
                    let _ = ring.write(&[1]);
                }
            })?;
        Ok(Hostio { jobs, done, wake })
    }

    /// Queue `job`, answered under `token`. Refused when the queue is full.
    pub fn submit(&self, token: Token, job: Job) -> Result<(), RpcError> {
        match self.jobs.try_send((token, job)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(RpcError::new(ErrorCode::NotUsable, "host busy")
                .with_hint(format!(
                    "{QUEUE} requests of the host wait already: ask again later"
                ))),
            Err(TrySendError::Disconnected(_)) => Err(RpcError::new(
                ErrorCode::Internal,
                "the host worker has ended",
            )),
        }
    }

    /// Readable when answers wait.
    pub fn fd(&self) -> &UnixStream {
        &self.wake
    }

    /// The answers ready.
    pub fn drain(&mut self) -> Vec<(Token, Result<Value, RpcError>)> {
        let mut bytes = [0u8; 64];
        while matches!(self.wake.read(&mut bytes), Ok(n) if n > 0) {}
        let mut out = Vec::new();
        loop {
            match self.done.try_recv() {
                Ok(answer) => out.push(answer),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return out,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    fn state() -> State {
        State {
            tzdir: PathBuf::from("/nonexistent"),
            tz: None,
            localtime: PathBuf::from("/nonexistent"),
            zones: HashMap::new(),
            storage: None,
        }
    }

    fn wait(io: &mut Hostio, n: usize) -> Vec<(Token, Result<Value, RpcError>)> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut got = Vec::new();
        while got.len() < n && Instant::now() < deadline {
            let mut fds = [rustix::event::PollFd::new(
                &io.wake,
                rustix::event::PollFlags::IN,
            )];
            let _ = rustix::event::poll(&mut fds, None);
            got.extend(io.drain());
        }
        got
    }

    #[test]
    fn in_order_and_woken() {
        let mut io = Hostio::spawn(state()).unwrap();
        for i in 0..10u64 {
            io.submit(Token(i), Box::new(move |_| Ok(json!(i))))
                .unwrap();
        }
        let got = wait(&mut io, 10);
        let order: Vec<u64> = got.iter().map(|(t, _)| t.0).collect();
        assert_eq!(order, (0..10).collect::<Vec<_>>());
        assert!(got.iter().all(|(t, v)| v.as_ref().unwrap() == &json!(t.0)));
    }

    #[test]
    fn a_full_queue_is_refused() {
        let mut io = Hostio::spawn(state()).unwrap();
        let gate = Arc::new(Barrier::new(2));
        let held = gate.clone();
        let (started, on) = mpsc::channel();
        io.submit(
            Token(0),
            Box::new(move |_| {
                started.send(()).unwrap();
                held.wait();
                Ok(json!(0))
            }),
        )
        .unwrap();
        // The worker holds the first; the queue takes 64 more.
        on.recv().unwrap();
        for i in 1..=QUEUE as u64 {
            io.submit(Token(i), Box::new(|_| Ok(json!(null)))).unwrap();
        }
        let e = io
            .submit(Token(99), Box::new(|_| Ok(json!(null))))
            .unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (ErrorCode::NotUsable, "host busy")
        );
        gate.wait();
        assert_eq!(wait(&mut io, QUEUE + 1).len(), QUEUE + 1);
    }

    #[test]
    fn zones_without_a_database() {
        let mut s = state();
        assert_eq!(s.zone("UTC").unwrap().at(0).offset_s, 0);
        assert_eq!(s.zone("local").unwrap().name, "UTC");
        assert_eq!(s.zone("Europe/Paris"), Err(zone::Missing::NoDatabase));
        assert_eq!(s.zone("../x"), Err(zone::Missing::Unknown));
    }
}
