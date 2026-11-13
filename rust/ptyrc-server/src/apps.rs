// SPDX-License-Identifier: GPL-3.0-or-later
//! The apps of a session: the client programs the driver started
//! (children) and the processes connected from elsewhere (clients).
//! No clock inside: time is an argument.

use crate::engine::{ConnId, Owned};
use crate::start;
use rustix::process::{Pid, Signal, getpgid, kill_process, kill_process_group};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// An app that stays after SIGTERM is killed after this.
pub(crate) const KILL_AFTER: Duration = Duration::from_secs(2);
/// Exited children kept in the table.
const EXITED_KEPT: usize = 16;
/// Set for a child: its id in the table.
pub(crate) const ENV_APP: &str = "PTYRC_APP";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Child,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Running,
    Paused,
    Exited,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Paused => "paused",
            State::Exited => "exited",
        }
    }
}

/// Why a request on an app is refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// No such id.
    Unknown,
    /// Not in this state, or not for this kind.
    NotUsable(String),
    /// It did not start.
    Spawn(String),
}

pub(crate) struct App {
    pub id: u64,
    pub kind: Kind,
    pub name: String,
    pub argv: Vec<String>,
    pub pid: u32,
    pub state: State,
    /// Of an exited child; 128 + signal when killed.
    pub status: Option<i32>,
    /// Starts so far.
    pub runs: u32,
    /// When the present state began.
    pub since: Duration,
    pub conns: BTreeSet<ConnId>,
    child: Option<Child>,
    /// Asked to end: when it is killed if it stays.
    kill_at: Option<Duration>,
    /// Started again when it has ended.
    again: bool,
    /// Forgotten when it ends (the hotkey's).
    ephemeral: bool,
    log: Option<PathBuf>,
}

impl App {
    fn signal(&self, signal: Signal) {
        let Some(pid) = Pid::from_raw(self.pid as i32) else {
            return;
        };
        let _ = match self.kind {
            Kind::Child => kill_process_group(pid, signal),
            Kind::Client => kill_process(pid, signal),
        };
    }

    /// As JSON, without `me`, `sprites` and `widgets`.
    fn json(&self, now: Duration) -> Value {
        let since = now.saturating_sub(self.since).as_millis() as f64 / 1000.0;
        json!({
            "id": self.id,
            "kind": match self.kind { Kind::Child => "child", Kind::Client => "client" },
            "name": self.name,
            "argv": self.argv,
            "pid": self.pid,
            "state": self.state.name(),
            "status": self.status,
            "runs": self.runs,
            "since": since,
            "conns": self.conns.iter().map(|c| c.0).collect::<Vec<_>>(),
            "log": self.log.as_ref().map(|p| p.to_string_lossy()),
        })
    }
}

pub(crate) struct Apps {
    entries: BTreeMap<u64, App>,
    next_id: u64,
    /// For the children: the session and its socket.
    env: Vec<(String, String)>,
    /// `<socket dir>/<session>`: the log of app N is `<that>.app-N.log`.
    log_base: PathBuf,
    /// Entries whose info changed since `take_changed`.
    changed: BTreeSet<u64>,
    /// Entries that left the table since `take_gone`.
    gone: Vec<App>,
}

fn file_name(program: &str) -> String {
    Path::new(program)
        .file_name()
        .map_or_else(|| program.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The command line of `pid`; empty when it cannot be read.
fn cmdline(pid: u32) -> Vec<String> {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return Vec::new();
    };
    bytes
        .split(|b| *b == 0)
        .filter(|w| !w.is_empty())
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect()
}

impl Apps {
    /// `socket`: of the session, `<dir>/<session>.sock`.
    pub fn new(env: Vec<(String, String)>, socket: &Path) -> Apps {
        Apps {
            entries: BTreeMap::new(),
            next_id: 1,
            env,
            log_base: socket.with_extension(""),
            changed: BTreeSet::new(),
            gone: Vec::new(),
        }
    }

    fn log_path(&self, id: u64) -> PathBuf {
        let mut name = self.log_base.clone().into_os_string();
        name.push(format!(".app-{id}.log"));
        PathBuf::from(name)
    }

    /// `argv` in its own process group; stdin and stdout null, stderr to
    /// `log`.
    fn spawn(&self, id: u64, argv: &[String], log: &Path) -> Result<Child, String> {
        let (program, args) = argv.split_first().ok_or("argv: empty")?;
        let stderr = match File::create(log) {
            Ok(file) => Stdio::from(file),
            Err(_) => Stdio::null(),
        };
        Command::new(program)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env(ENV_APP, id.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .process_group(0)
            .spawn()
            .map_err(|e| format!("cannot run {program}: {}", start::describe(&e)))
    }

    /// Start a child. `ephemeral`: forgotten when it ends.
    pub fn start(
        &mut self,
        argv: Vec<String>,
        name: Option<String>,
        ephemeral: bool,
        now: Duration,
    ) -> Result<u64, Refused> {
        let id = self.next_id;
        let log = self.log_path(id);
        let child = self.spawn(id, &argv, &log).map_err(|e| {
            let _ = fs::remove_file(&log);
            Refused::Spawn(e)
        })?;
        self.next_id += 1;
        let name = name.unwrap_or_else(|| file_name(&argv[0]));
        let app = App {
            id,
            kind: Kind::Child,
            name,
            argv,
            pid: child.id(),
            state: State::Running,
            status: None,
            runs: 1,
            since: now,
            conns: BTreeSet::new(),
            child: Some(child),
            kill_at: None,
            again: false,
            ephemeral,
            log: Some(log),
        };
        self.entries.insert(id, app);
        self.changed.insert(id);
        Ok(id)
    }

    /// A connection was accepted from `peer`: the child whose group
    /// holds it, else the client of that pid, made when new.
    pub fn attach(&mut self, conn: ConnId, peer: Option<u32>, now: Duration) -> Option<u64> {
        let peer = peer?;
        let group = Pid::from_raw(peer as i32)
            .and_then(|pid| getpgid(Some(pid)).ok())
            .map(|g| g.as_raw_nonzero().get() as u32);
        let live = |a: &&mut App| a.state != State::Exited;
        let of_group = |a: &&mut App| a.kind == Kind::Child && Some(a.pid) == group;
        let of_pid = |a: &&mut App| a.kind == Kind::Client && a.pid == peer;
        let mut entries = self.entries.values_mut();
        let found = entries.find(|a| live(a) && (of_group(a) || of_pid(a)));
        let id = match found {
            Some(app) => {
                app.conns.insert(conn);
                app.id
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                let argv = cmdline(peer);
                let name = argv.first().map_or_else(String::new, |p| file_name(p));
                let app = App {
                    id,
                    kind: Kind::Client,
                    name,
                    argv,
                    pid: peer,
                    state: State::Running,
                    status: None,
                    runs: 1,
                    since: now,
                    conns: BTreeSet::from([conn]),
                    child: None,
                    kill_at: None,
                    again: false,
                    ephemeral: false,
                    log: None,
                };
                self.entries.insert(id, app);
                id
            }
        };
        self.changed.insert(id);
        Some(id)
    }

    /// A connection closed. A client without one leaves the table.
    pub fn detach(&mut self, conn: ConnId) {
        let Some(id) = self.of_conn(conn) else {
            return;
        };
        let app = self.entries.get_mut(&id).expect("found");
        app.conns.remove(&conn);
        self.changed.insert(id);
        if app.kind == Kind::Client && app.conns.is_empty() {
            self.remove(id);
        }
    }

    fn remove(&mut self, id: u64) {
        if let Some(app) = self.entries.remove(&id) {
            if let Some(log) = &app.log {
                let _ = fs::remove_file(log);
            }
            self.changed.remove(&id);
            self.gone.push(app);
        }
    }

    pub fn get(&self, id: u64) -> Option<&App> {
        self.entries.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &App> {
        self.entries.values()
    }

    /// The app a connection belongs to.
    pub fn of_conn(&self, conn: ConnId) -> Option<u64> {
        self.iter().find(|a| a.conns.contains(&conn)).map(|a| a.id)
    }

    /// The info of the app of `conn` changed.
    pub fn touch(&mut self, conn: ConnId) {
        self.changed.extend(self.of_conn(conn));
    }

    fn entry(&mut self, id: u64) -> Result<&mut App, Refused> {
        self.entries.get_mut(&id).ok_or(Refused::Unknown)
    }

    /// SIGTERM, SIGCONT when paused; SIGKILL after `KILL_AFTER`. An
    /// exited child: nothing.
    pub fn stop(&mut self, id: u64, now: Duration) -> Result<(), Refused> {
        let app = self.entry(id)?;
        if app.state == State::Exited {
            return Ok(());
        }
        app.signal(Signal::TERM);
        if app.state == State::Paused {
            app.signal(Signal::CONT);
        }
        app.kill_at = Some(now + KILL_AFTER);
        Ok(())
    }

    /// SIGSTOP. True: it was running; the caller hides what it owns.
    pub fn pause(&mut self, id: u64, now: Duration) -> Result<bool, Refused> {
        let app = self.entry(id)?;
        match app.state {
            State::Exited => Err(Refused::NotUsable(format!("app {id} has exited"))),
            State::Paused => Ok(false),
            State::Running => {
                app.signal(Signal::STOP);
                app.state = State::Paused;
                app.since = now;
                self.changed.insert(id);
                Ok(true)
            }
        }
    }

    /// SIGCONT. True: it was paused; the caller shows what it owns.
    pub fn resume(&mut self, id: u64, now: Duration) -> Result<bool, Refused> {
        let app = self.entry(id)?;
        match app.state {
            State::Exited => Err(Refused::NotUsable(format!("app {id} has exited"))),
            State::Running => Ok(false),
            State::Paused => {
                app.signal(Signal::CONT);
                app.state = State::Running;
                app.since = now;
                self.changed.insert(id);
                Ok(true)
            }
        }
    }

    /// Stop a child and start it again when it has ended; an exited one
    /// at once. `argv`, `name`: as before when absent.
    pub fn restart(
        &mut self,
        id: u64,
        argv: Option<Vec<String>>,
        name: Option<String>,
        now: Duration,
    ) -> Result<(), Refused> {
        let app = self.entry(id)?;
        if app.kind == Kind::Client {
            return Err(Refused::NotUsable(format!(
                "app {id} was not started by the driver"
            )));
        }
        if argv.as_ref().is_some_and(Vec::is_empty) {
            return Err(Refused::Spawn("argv: empty".into()));
        }
        if let Some(argv) = argv {
            app.argv = argv;
            app.name = name.unwrap_or_else(|| file_name(&app.argv[0]));
        } else if let Some(name) = name {
            app.name = name;
        }
        let exited = app.state == State::Exited;
        app.again = !exited;
        self.changed.insert(id);
        match exited {
            true => self.respawn(id, now),
            false => self.stop(id, now),
        }
    }

    fn respawn(&mut self, id: u64, now: Duration) -> Result<(), Refused> {
        let log = self.log_path(id);
        let argv = self.entries[&id].argv.clone();
        let spawned = self.spawn(id, &argv, &log);
        let app = self.entries.get_mut(&id).expect("an entry");
        app.again = false;
        self.changed.insert(id);
        match spawned {
            Ok(child) => {
                app.pid = child.id();
                app.child = Some(child);
                app.state = State::Running;
                app.status = None;
                app.runs += 1;
                app.since = now;
                app.conns.clear();
                Ok(())
            }
            Err(e) => {
                let _ = fs::write(&log, format!("{e}\n"));
                app.status = Some(127);
                app.since = now;
                Err(Refused::Spawn(e))
            }
        }
    }

    /// An exited child leaves the table.
    pub fn forget(&mut self, id: u64) -> Result<(), Refused> {
        match self.entry(id)?.state {
            State::Exited => {
                self.remove(id);
                Ok(())
            }
            _ => Err(Refused::NotUsable(format!("app {id} has not exited"))),
        }
    }

    /// After SIGCHLD: note the children that ended, start again those
    /// asked to.
    pub fn reap(&mut self, now: Duration) {
        let mut ended = Vec::new();
        for app in self.entries.values_mut() {
            let Some(child) = &mut app.child else {
                continue;
            };
            if let Ok(Some(status)) = child.try_wait() {
                app.status = Some(
                    status
                        .code()
                        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
                );
                app.child = None;
                app.state = State::Exited;
                app.since = now;
                app.kill_at = None;
                ended.push(app.id);
            }
        }
        for id in ended {
            self.changed.insert(id);
            let app = &self.entries[&id];
            if app.again {
                let _ = self.respawn(id, now);
            } else if app.ephemeral {
                self.remove(id);
            }
        }
        loop {
            let exited = self.entries.values().filter(|a| a.state == State::Exited);
            let exited: Vec<(Duration, u64)> = exited.map(|a| (a.since, a.id)).collect();
            if exited.len() <= EXITED_KEPT {
                break;
            }
            let (_, oldest) = exited.into_iter().min().expect("more than none");
            self.remove(oldest);
        }
    }

    /// When `tick` is next due.
    pub fn due(&self) -> Option<Duration> {
        self.iter().filter_map(|a| a.kill_at).min()
    }

    /// Kill the apps that stayed after they were asked to end.
    pub fn tick(&mut self, now: Duration) {
        for app in self.entries.values_mut() {
            if app.kill_at.is_some_and(|at| at <= now) {
                app.kill_at = None;
                if app.state != State::Exited {
                    app.signal(Signal::KILL);
                }
            }
        }
    }

    /// The end of the session: SIGTERM to every child, logs removed.
    pub fn finish(&mut self) {
        for app in self.entries.values() {
            if app.kind == Kind::Child && app.state != State::Exited {
                app.signal(Signal::TERM);
                app.signal(Signal::CONT);
            }
            if let Some(log) = &app.log {
                let _ = fs::remove_file(log);
            }
        }
    }

    pub fn take_changed(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.changed).into_iter().collect()
    }

    /// The last info of the entries that left, `state` `gone`.
    pub fn take_gone(&mut self, now: Duration) -> Vec<Value> {
        let gone = std::mem::take(&mut self.gone);
        let json = |app: App| {
            let mut info = app.json(now);
            info["state"] = json!("gone");
            info["conns"] = json!([]);
            info["log"] = Value::Null;
            info["sprites"] = json!(0);
            info["widgets"] = json!(0);
            info["frames"] = json!([]);
            info
        };
        gone.into_iter().map(json).collect()
    }

    /// The info of an app. `owned`: what a connection owns. `me`: the connection that asks.
    pub fn info(
        &self,
        id: u64,
        now: Duration,
        me: Option<ConnId>,
        owned: &dyn Fn(ConnId) -> Owned,
    ) -> Option<Value> {
        let app = self.get(id)?;
        let mut info = app.json(now);
        let mut all = Owned::default();
        for conn in &app.conns {
            let owned = owned(*conn);
            all.sprites += owned.sprites;
            all.widgets += owned.widgets;
            all.frames.extend(owned.frames);
        }
        info["sprites"] = json!(all.sprites);
        info["widgets"] = json!(all.widgets);
        info["frames"] = json!(all.frames);
        if let Some(me) = me {
            info["me"] = json!(app.conns.contains(&me));
        }
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::ZERO;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn apps(tag: &str) -> (Apps, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ptyrc-apps-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        (Apps::new(Vec::new(), &dir.join("s.sock")), dir)
    }

    fn sh(script: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), script.into()]
    }

    fn wait(apps: &mut Apps, now: Duration, mut done: impl FnMut(&Apps) -> bool) {
        for _ in 0..2000 {
            apps.reap(now);
            if done(apps) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out");
    }

    fn none(_: ConnId) -> Owned {
        Owned::default()
    }

    #[test]
    fn a_child_starts_stops_and_stays() {
        let (mut apps, dir) = apps("stop");
        let id = apps
            .start(sh("exec sleep 60"), Some("nap".into()), false, T0)
            .unwrap();
        assert_eq!(apps.take_changed(), [id]);
        let info = apps.info(id, secs(3), Some(ConnId(9)), &none).unwrap();
        assert_eq!(info["name"], "nap");
        assert_eq!(info["state"], "running");
        assert_eq!(info["since"], 3.0);
        assert_eq!(info["me"], false);
        assert_eq!(info["kind"], "child");
        assert!(dir.join("s.app-1.log").exists());
        apps.stop(id, secs(3)).unwrap();
        assert_eq!(apps.due(), Some(secs(3) + KILL_AFTER));
        wait(&mut apps, secs(4), |a| {
            a.get(id).unwrap().state == State::Exited
        });
        let app = apps.get(id).unwrap();
        assert_eq!((app.status, app.since), (Some(128 + 15), secs(4)));
        assert_eq!(apps.due(), None);
        assert_eq!(apps.take_changed(), [id]);
        // Stopped again: nothing. Forgotten: gone, with its log.
        apps.stop(id, secs(5)).unwrap();
        assert_eq!(apps.pause(id, secs(5)).map_err(|_| ()), Err(()));
        apps.forget(id).unwrap();
        assert_eq!(apps.forget(id), Err(Refused::Unknown));
        let gone = apps.take_gone(secs(5));
        assert_eq!(gone[0]["state"], "gone");
        assert!(!dir.join("s.app-1.log").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_child_that_stays_is_killed() {
        let (mut apps, dir) = apps("kill");
        let script = "trap '' TERM; echo up >&2; while :; do sleep 1; done";
        let id = apps.start(sh(script), None, false, T0).unwrap();
        assert_eq!(apps.get(id).unwrap().name, "sh");
        let log = dir.join("s.app-1.log");
        wait(&mut apps, T0, |_| {
            fs::read_to_string(&log).is_ok_and(|l| l == "up\n")
        });
        apps.stop(id, T0).unwrap();
        apps.tick(secs(1));
        std::thread::sleep(Duration::from_millis(50));
        apps.reap(secs(1));
        assert_eq!(apps.get(id).unwrap().state, State::Running);
        apps.tick(secs(2));
        wait(&mut apps, secs(2), |a| {
            a.get(id).unwrap().state == State::Exited
        });
        assert_eq!(apps.get(id).unwrap().status, Some(128 + 9));
        apps.finish();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pause_resume_restart() {
        let (mut apps, dir) = apps("restart");
        let id = apps.start(sh("exec sleep 60"), None, false, T0).unwrap();
        let first = apps.get(id).unwrap().pid;
        assert_eq!(apps.pause(id, secs(1)), Ok(true));
        assert_eq!(apps.pause(id, secs(1)), Ok(false));
        assert_eq!(apps.get(id).unwrap().state, State::Paused);
        assert_eq!(apps.resume(id, secs(2)), Ok(true));
        assert_eq!(apps.resume(id, secs(2)), Ok(false));
        // Restarted paused, with another command: the same id.
        apps.pause(id, secs(3)).unwrap();
        apps.restart(id, Some(sh("exec sleep 61")), Some("nap".into()), secs(3))
            .unwrap();
        wait(&mut apps, secs(4), |a| {
            let app = a.get(id).unwrap();
            app.state == State::Running && app.pid != first
        });
        let app = apps.get(id).unwrap();
        assert_eq!((app.runs, app.name.as_str()), (2, "nap"));
        assert_eq!(app.argv[2], "exec sleep 61");
        // An exited one starts at once; a command that is not there
        // leaves it exited.
        apps.stop(id, secs(5)).unwrap();
        wait(&mut apps, secs(5), |a| {
            a.get(id).unwrap().state == State::Exited
        });
        apps.restart(id, None, None, secs(6)).unwrap();
        assert_eq!(apps.get(id).unwrap().runs, 3);
        apps.stop(id, secs(6)).unwrap();
        wait(&mut apps, secs(6), |a| {
            a.get(id).unwrap().state == State::Exited
        });
        let gone = apps.restart(id, Some(vec!["/no/such/program".into()]), None, secs(7));
        assert!(matches!(gone, Err(Refused::Spawn(_))));
        assert_eq!(apps.get(id).unwrap().status, Some(127));
        assert_eq!(apps.restart(77, None, None, T0), Err(Refused::Unknown));
        assert!(matches!(
            apps.start(vec!["/no/such/program".into()], None, false, T0),
            Err(Refused::Spawn(_))
        ));
        apps.finish();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn connections_find_their_app() {
        let (mut apps, dir) = apps("conns");
        let id = apps.start(sh("sleep 60"), None, false, T0).unwrap();
        let child = apps.get(id).unwrap().pid;
        // A peer of the child's group: here, the child itself.
        assert_eq!(apps.attach(ConnId(1), Some(child), T0), Some(id));
        // Another process: a client, one entry a pid.
        let me = std::process::id();
        let client = apps.attach(ConnId(2), Some(me), T0).unwrap();
        assert_ne!(client, id);
        assert_eq!(apps.attach(ConnId(3), Some(me), T0), Some(client));
        assert_eq!(apps.attach(ConnId(4), None, T0), None);
        let info = apps
            .info(client, T0, Some(ConnId(3)), &|c| Owned {
                sprites: 2,
                widgets: 1,
                frames: [0, c.0 as u32].into(),
            })
            .unwrap();
        assert_eq!(info["kind"], "client");
        assert_eq!(info["conns"], json!([2, 3]));
        assert_eq!((&info["sprites"], &info["widgets"]), (&json!(4), &json!(2)));
        assert_eq!(info["frames"], json!([0, 2, 3]));
        assert_eq!(info["me"], true);
        assert_eq!(info["pid"], me);
        assert!(!info["argv"].as_array().unwrap().is_empty());
        assert!(matches!(
            apps.restart(client, None, None, T0),
            Err(Refused::NotUsable(_))
        ));
        apps.detach(ConnId(2));
        assert!(apps.get(client).is_some());
        apps.detach(ConnId(3));
        assert!(apps.get(client).is_none());
        assert_eq!(apps.take_gone(T0)[0]["id"], client);
        apps.detach(ConnId(1));
        assert!(apps.get(id).unwrap().conns.is_empty());
        apps.finish();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exited_are_kept_to_a_bound_and_ephemeral_ones_go() {
        let (mut apps, dir) = apps("kept");
        for n in 0..EXITED_KEPT as u64 + 2 {
            apps.start(sh("exit 3"), None, false, secs(n)).unwrap();
        }
        let hot = apps.start(sh("exit 0"), None, true, T0).unwrap();
        wait(&mut apps, secs(100), |a| {
            a.iter().all(|a| a.state == State::Exited)
        });
        assert!(apps.get(hot).is_none());
        assert_eq!(apps.iter().count(), EXITED_KEPT);
        assert_eq!(apps.get(5).unwrap().status, Some(3));
        fs::remove_dir_all(dir).unwrap();
    }
}
