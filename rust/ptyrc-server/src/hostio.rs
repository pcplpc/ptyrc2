// SPDX-License-Identifier: GPL-3.0-or-later
//! The workers of the host functions: what may block runs off the
//! driver's loop, in order. The disk (`clock.*`, `storage.*`, `file.*`):
//! one thread. The probes (`probe.*`: `git` may take 2 s a command): a
//! lane an app on a pool of `POOL` threads, one job of an app at a
//! time, the apps in turn; an app's hung `git` holds one thread, not
//! the others' probes. A job is a function of its lane's state; its
//! answer completes the pending reply from the loop, woken by a byte on
//! a socket pair.

use crate::engine::Token;
use crate::zone::{self, Zone};
use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::SystemTime;

/// Jobs queued a lane, at most; then `not-usable`.
pub(crate) const QUEUE: usize = 64;
/// The threads of the probe lanes.
pub(crate) const POOL: usize = 4;
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
    /// The probes of each app (its probe lane: its own only).
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

    /// The same places, nothing kept: the state of a new lane.
    fn like(&self) -> State {
        State {
            tzdir: self.tzdir.clone(),
            tz: self.tz.clone(),
            localtime: self.localtime.clone(),
            zones: HashMap::new(),
            storage: self.storage.clone(),
            probes: HashMap::new(),
            git: self.git.clone(),
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

/// Which worker a job runs on: the disk, or the probe lane of an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    Disk,
    Probe(u64),
}

type Answer = (Token, Result<Value, RpcError>);

/// The probe lane of an app.
struct Slot {
    /// None while its job runs.
    state: Option<State>,
    queue: VecDeque<(Token, Job)>,
    /// Forgotten while its job ran: dropped when it ends.
    gone: bool,
}

/// The probe lanes, shared by the pool.
struct Lanes {
    template: State,
    slots: HashMap<u64, Slot>,
    /// The apps with a job queued and none running, in turn.
    ready: VecDeque<u64>,
    closed: bool,
}

struct Pool {
    lanes: Mutex<Lanes>,
    wake: Condvar,
}

impl Pool {
    fn lock(&self) -> MutexGuard<'_, Lanes> {
        self.lanes.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The driver's end.
pub(crate) struct Hostio {
    disk: SyncSender<(Token, Job)>,
    pool: Arc<Pool>,
    done: Receiver<Answer>,
    wake: UnixStream,
}

impl Drop for Hostio {
    fn drop(&mut self) {
        self.pool.lock().closed = true;
        self.pool.wake.notify_all();
    }
}

/// A thread running the jobs of `inbox` on `state`.
fn worker(
    name: &str,
    mut state: State,
    inbox: Receiver<(Token, Job)>,
    outbox: mpsc::Sender<Answer>,
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

/// A thread of the pool: the next app's next job, on its state.
fn prober(
    name: &str,
    pool: Arc<Pool>,
    outbox: mpsc::Sender<Answer>,
    mut ring: UnixStream,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut lanes = pool.lock();
            loop {
                if lanes.closed {
                    return;
                }
                let Some(app) = lanes.ready.pop_front() else {
                    lanes = pool.wake.wait(lanes).unwrap_or_else(|e| e.into_inner());
                    continue;
                };
                let Some(slot) = lanes.slots.get_mut(&app) else {
                    continue;
                };
                // In turn: idle, a job queued.
                if slot.state.is_none() || slot.queue.is_empty() {
                    continue;
                }
                let mut state = slot.state.take().expect("checked");
                let (token, job) = slot.queue.pop_front().expect("checked");
                drop(lanes);
                let answer = job(&mut state);
                lanes = pool.lock();
                if let Some(slot) = lanes.slots.get_mut(&app) {
                    match (slot.queue.is_empty(), slot.gone) {
                        (true, true) => drop(lanes.slots.remove(&app)),
                        (true, false) => slot.state = Some(state),
                        (false, _) => {
                            slot.state = Some(state);
                            lanes.ready.push_back(app);
                            pool.wake.notify_one();
                        }
                    }
                }
                if outbox.send((token, answer)).is_err() {
                    return;
                }
                // Full: a byte already waits, the loop will drain.
                let _ = ring.write(&[1]);
            }
        })
        .map(drop)
}

impl Hostio {
    /// The workers, started: `disk` its state; `probe`, where each
    /// probe lane's state starts from.
    pub fn spawn(disk: State, probe: State) -> io::Result<Hostio> {
        let (outbox, done) = mpsc::channel();
        let (wake, ring) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        ring.set_nonblocking(true)?;
        let (jobs, inbox) = mpsc::sync_channel::<(Token, Job)>(QUEUE);
        worker(
            "ptyrc-hostio",
            disk,
            inbox,
            outbox.clone(),
            ring.try_clone()?,
        )?;
        let pool = Arc::new(Pool {
            lanes: Mutex::new(Lanes {
                template: probe,
                slots: HashMap::new(),
                ready: VecDeque::new(),
                closed: false,
            }),
            wake: Condvar::new(),
        });
        let hostio = Hostio {
            disk: jobs,
            pool,
            done,
            wake,
        };
        for _ in 0..POOL {
            let pool = hostio.pool.clone();
            prober("ptyrc-probe", pool, outbox.clone(), ring.try_clone()?)?;
        }
        Ok(hostio)
    }

    /// Queue `job` on `lane`, answered under `token`. Refused when the
    /// queue is full.
    pub fn submit(&self, lane: Lane, token: Token, job: Job) -> Result<(), RpcError> {
        let busy = || {
            RpcError::new(ErrorCode::NotUsable, "host busy").with_hint(format!(
                "{QUEUE} requests of the host wait already: ask again later"
            ))
        };
        let app = match lane {
            Lane::Disk => {
                return match self.disk.try_send((token, job)) {
                    Ok(()) => Ok(()),
                    Err(TrySendError::Full(_)) => Err(busy()),
                    Err(TrySendError::Disconnected(_)) => Err(RpcError::new(
                        ErrorCode::Internal,
                        "the host worker has ended",
                    )),
                };
            }
            Lane::Probe(app) => app,
        };
        let mut lanes = self.pool.lock();
        if !lanes.slots.contains_key(&app) {
            let fresh = Slot {
                state: Some(lanes.template.like()),
                queue: VecDeque::new(),
                gone: false,
            };
            lanes.slots.insert(app, fresh);
        }
        let slot = lanes.slots.get_mut(&app).expect("inserted");
        if slot.queue.len() >= QUEUE {
            return Err(busy());
        }
        slot.gone = false;
        slot.queue.push_back((token, job));
        // Idle and not in turn yet: in turn.
        let idle = slot.state.is_some() && slot.queue.len() == 1;
        if idle {
            lanes.ready.push_back(app);
            self.pool.wake.notify_one();
        }
        Ok(())
    }

    /// The probe lane of `app`, its state with it, dropped: at once when
    /// idle, else when its queue is done.
    pub fn forget(&self, app: u64) {
        let mut lanes = self.pool.lock();
        let Some(slot) = lanes.slots.get_mut(&app) else {
            return;
        };
        if slot.state.is_some() && slot.queue.is_empty() {
            lanes.slots.remove(&app);
        } else {
            slot.gone = true;
        }
    }

    /// The probe lanes kept (tests).
    #[cfg(test)]
    fn lanes(&self) -> usize {
        self.pool.lock().slots.len()
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
    use std::sync::Barrier;
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

    /// A job of `lane` that tells it started, then waits for the gate.
    fn held(gate: &Arc<Barrier>) -> (Job, Receiver<()>) {
        let (started, on) = mpsc::channel();
        let gate = gate.clone();
        let job: Job = Box::new(move |_| {
            started.send(()).unwrap();
            gate.wait();
            Ok(json!("held"))
        });
        (job, on)
    }

    #[test]
    fn a_full_queue_is_refused_and_a_lane_held_stalls_not_the_others() {
        let mut io = Hostio::spawn(state(), state()).unwrap();
        let gate = Arc::new(Barrier::new(2));
        let (job, on) = held(&gate);
        io.submit(Lane::Probe(1), Token(0), job).unwrap();
        // The pool holds the first; its lane takes 64 more.
        on.recv().unwrap();
        for i in 1..=QUEUE as u64 {
            io.submit(Lane::Probe(1), Token(i), Box::new(|_| Ok(json!(null))))
                .unwrap();
        }
        let e = io
            .submit(Lane::Probe(1), Token(99), Box::new(|_| Ok(json!(null))))
            .unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (ErrorCode::NotUsable, "host busy")
        );
        // The disk and another app's probes answer meanwhile.
        io.submit(Lane::Disk, Token(100), Box::new(|_| Ok(json!("disk"))))
            .unwrap();
        io.submit(Lane::Probe(2), Token(101), Box::new(|_| Ok(json!("2"))))
            .unwrap();
        let mut got: Vec<u64> = wait(&mut io, 2).iter().map(|(t, _)| t.0).collect();
        got.sort();
        assert_eq!(got, [100, 101]);
        gate.wait();
        let rest = wait(&mut io, QUEUE + 1);
        let order: Vec<u64> = rest.iter().map(|(t, _)| t.0).collect();
        assert_eq!(order, (0..=QUEUE as u64).collect::<Vec<_>>());
    }

    #[test]
    fn a_lane_an_app_its_state_its_own_one_job_at_a_time() {
        let mut io = Hostio::spawn(state(), state()).unwrap();
        // More apps held than threads: the pool is bounded, the rest wait.
        let gate = Arc::new(Barrier::new(POOL + 1));
        let mut ons = Vec::new();
        for app in 0..POOL as u64 {
            let (job, on) = held(&gate);
            io.submit(Lane::Probe(app), Token(app), job).unwrap();
            ons.push(on);
        }
        for on in &ons {
            on.recv().unwrap();
        }
        let count = |state: &mut State| {
            let probes = state.probes.entry(9).or_default();
            let n = probes.git.as_ref().map_or(0, |g| g.1.as_u64().unwrap()) + 1;
            probes.git = Some((Instant::now(), json!(n), json!(null)));
            Ok(json!([n, state.probes.len()]))
        };
        for i in 0..3 {
            io.submit(Lane::Probe(9), Token(10 + i), Box::new(count))
                .unwrap();
        }
        io.submit(Lane::Probe(8), Token(20), Box::new(count))
            .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(io.drain().is_empty());
        gate.wait();
        let got: HashMap<u64, Value> = (wait(&mut io, POOL + 4).into_iter())
            .map(|(t, v)| (t.0, v.unwrap()))
            .collect();
        // In order on its lane; its state seen by it alone.
        assert_eq!(got[&10], json!([1, 1]));
        assert_eq!(got[&11], json!([2, 1]));
        assert_eq!(got[&12], json!([3, 1]));
        assert_eq!(got[&20], json!([1, 1]));
        // Forgotten: its state dropped.
        assert_eq!(io.lanes(), POOL + 2);
        io.forget(9);
        assert_eq!(io.lanes(), POOL + 1);
        io.submit(Lane::Probe(9), Token(30), Box::new(count))
            .unwrap();
        assert_eq!(wait(&mut io, 1)[0].1.as_ref().unwrap(), &json!([1, 1]));
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
