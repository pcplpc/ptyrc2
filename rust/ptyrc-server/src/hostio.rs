// SPDX-License-Identifier: GPL-3.0-or-later
//! The workers of the host functions: what may block runs off the
//! driver's loop, in order. Two lanes, a thread each: the disk
//! (`clock.*`, `storage.*`, `file.*`) and the probes (`probe.*`: `git`
//! may take 2 s a command). A job is a function of its lane's state;
//! its answer completes the pending reply from the loop, woken by a
//! byte on a socket pair.

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
    /// The probes of each app (the probe lane).
    pub probes: HashMap<u64, Probes>,
    /// The `git` run (`git` of `$PATH`).
    pub git: PathBuf,
}

/// What the probe lane keeps of an app; dropped with it.
#[derive(Default)]
pub(crate) struct Probes {
    pub sampler: Option<ptyrc_probe::system::Sampler>,
    /// The last real `probe.git`: when, what was asked, the answer.
    pub git: Option<(std::time::Instant, Value, Value)>,
}

impl State {
    pub fn from_env() -> State {
        State {
            tzdir: zone::database(),
            tz: std::env::var("TZ").ok(),
            localtime: PathBuf::from("/etc/localtime"),
            zones: HashMap::new(),
            storage: crate::storage::root(),
            probes: HashMap::new(),
            git: PathBuf::from("git"),
        }
    }

    /// No database, no storage (tests).
    #[cfg(test)]
    pub fn bare() -> State {
        State {
            tzdir: PathBuf::from("/nonexistent"),
            tz: None,
            localtime: PathBuf::from("/nonexistent"),
            zones: HashMap::new(),
            storage: None,
            probes: HashMap::new(),
            git: PathBuf::from("git"),
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

/// Which worker a job runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    Disk,
    Probe,
}

/// The driver's end.
pub(crate) struct Hostio {
    disk: SyncSender<(Token, Job)>,
    probe: SyncSender<(Token, Job)>,
    done: Receiver<(Token, Result<Value, RpcError>)>,
    wake: UnixStream,
}

/// A thread running the jobs of `inbox` on `state`.
fn worker(
    name: &str,
    mut state: State,
    inbox: Receiver<(Token, Job)>,
    outbox: mpsc::Sender<(Token, Result<Value, RpcError>)>,
    mut ring: UnixStream,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            // Ends when the driver's end is dropped.
            while let Ok((token, job)) = inbox.recv() {
                let answer = job(&mut state);
                if outbox.send((token, answer)).is_err() {
                    break;
                }
                // Full: a byte already waits, the loop will drain.
                let _ = ring.write(&[1]);
            }
        })
        .map(drop)
}

impl Hostio {
    /// The workers, started: `disk` and `probe` their states.
    pub fn spawn(disk: State, probe: State) -> io::Result<Hostio> {
        let (outbox, done) = mpsc::channel();
        let (wake, ring) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        ring.set_nonblocking(true)?;
        let lane = |name, state| {
            let (jobs, inbox) = mpsc::sync_channel::<(Token, Job)>(QUEUE);
            worker(name, state, inbox, outbox.clone(), ring.try_clone()?)?;
            io::Result::Ok(jobs)
        };
        let disk = lane("ptyrc-hostio", disk)?;
        let probe = lane("ptyrc-probe", probe)?;
        Ok(Hostio {
            disk,
            probe,
            done,
            wake,
        })
    }

    /// Queue `job` on `lane`, answered under `token`. Refused when the
    /// queue is full.
    pub fn submit(&self, lane: Lane, token: Token, job: Job) -> Result<(), RpcError> {
        let jobs = match lane {
            Lane::Disk => &self.disk,
            Lane::Probe => &self.probe,
        };
        match jobs.try_send((token, job)) {
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
        State::bare()
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
        let mut io = Hostio::spawn(state(), state()).unwrap();
        for i in 0..10u64 {
            io.submit(Lane::Disk, Token(i), Box::new(move |_| Ok(json!(i))))
                .unwrap();
        }
        let got = wait(&mut io, 10);
        let order: Vec<u64> = got.iter().map(|(t, _)| t.0).collect();
        assert_eq!(order, (0..10).collect::<Vec<_>>());
        assert!(got.iter().all(|(t, v)| v.as_ref().unwrap() == &json!(t.0)));
    }

    #[test]
    fn a_full_queue_is_refused_and_a_lane_held_stalls_not_the_other() {
        let mut io = Hostio::spawn(state(), state()).unwrap();
        let gate = Arc::new(Barrier::new(2));
        let held = gate.clone();
        let (started, on) = mpsc::channel();
        io.submit(
            Lane::Probe,
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
            io.submit(Lane::Probe, Token(i), Box::new(|_| Ok(json!(null))))
                .unwrap();
        }
        let e = io
            .submit(Lane::Probe, Token(99), Box::new(|_| Ok(json!(null))))
            .unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (ErrorCode::NotUsable, "host busy")
        );
        // The disk answers meanwhile.
        io.submit(Lane::Disk, Token(100), Box::new(|_| Ok(json!("disk"))))
            .unwrap();
        assert_eq!(wait(&mut io, 1)[0].0, Token(100));
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
