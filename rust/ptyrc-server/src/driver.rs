// SPDX-License-Identifier: GPL-3.0-or-later
//! The event loop: app pty, host terminal, signals, clients, one timer.

use crate::conn::Conn;
use crate::engine::{ConnId, Ctx, Effects, Engine, Reply, SessionInfo, Token};
use crate::grab::{self, Grab};
use crate::host::Host;
use crate::hostcolors::{Known, QUERY_BG, QUERY_FG, Replies};
use crate::keys;
use crate::mouse;
use crate::player::{Player, Replay, Step};
use crate::pty::{self, Pty};
use crate::record::{Header, Record, Recorder};
use crate::start::{self, RunError, RunErrorKind};
use ptyrc_proto::envelope::{event, response};
use ptyrc_proto::session::{self, ENV_SESSION, ENV_SOCKET};
use ptyrc_proto::suggest;
use ptyrc_proto::{ErrorCode, PROTOCOL, Request, RpcError, SERVER, TOPICS};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group};
use serde_json::{Value, json};
use signal_hook::consts::{SIGCHLD, SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Upper bounds of a burst (design: "Coalescing").
const BURST_MAX_BYTES: usize = 1 << 20;
/// Below this size a burst is written at once, without the quiet time.
const BURST_MIN_WAIT: usize = 32;
const BURST_MAX_TIME: Duration = Duration::from_millis(8);
/// Keyboard input is not read while this much waits for the app.
const PTY_QUEUE_HIGH: usize = 1 << 20;
const DEFAULT_SIZE: (u16, u16) = (80, 24);
/// How long the host has to tell its colours.
const HOST_COLORS_WAIT: Duration = Duration::from_millis(1000);

/// How to run a session.
#[derive(Clone, Debug)]
pub struct Config {
    /// The app and its arguments.
    pub argv: Vec<String>,
    /// Session name; the driver pid when absent.
    pub name: Option<String>,
    /// No host terminal.
    pub headless: bool,
    /// Initial `(cols, rows)`; required meaning only without a host
    /// terminal, where it defaults to 80x24.
    pub size: Option<(u16, u16)>,
    /// Headless: append the host stream to this file.
    pub host_log: Option<PathBuf>,
    /// Quiet time that ends a burst, in milliseconds. 0: no coalescing.
    pub coalesce_ms: u64,
    /// Default foreground and background of the host. One that is
    /// absent is asked of the host terminal (OSC 10, OSC 11) when
    /// `ask_host_colors`; without an answer: light grey, black.
    pub host_fg: Option<ptyrc_vt::Rgb>,
    pub host_bg: Option<ptyrc_vt::Rgb>,
    pub ask_host_colors: bool,
    /// Write the session to this tape (`record.rs`).
    pub record: Option<PathBuf>,
    /// Record what is typed into the app too.
    pub record_input: bool,
    /// Whether the engine has the per-app presets: written on the tape.
    pub presets: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            argv: Vec::new(),
            name: None,
            headless: false,
            size: None,
            host_log: None,
            coalesce_ms: 1,
            host_fg: None,
            host_bg: None,
            ask_host_colors: true,
            record: None,
            record_input: false,
            presets: true,
        }
    }
}

struct Signals {
    wake: UnixStream,
    winch: Arc<AtomicBool>,
    chld: Arc<AtomicBool>,
    term: [(i32, Arc<AtomicBool>); 3],
}

impl Signals {
    fn install() -> io::Result<Signals> {
        let (wake, notify) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        notify.set_nonblocking(true)?;
        let flag = |signal: i32| -> io::Result<Arc<AtomicBool>> {
            let flag = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal, flag.clone())?;
            signal_hook::low_level::pipe::register(signal, notify.try_clone()?)?;
            Ok(flag)
        };
        Ok(Signals {
            winch: flag(SIGWINCH)?,
            chld: flag(SIGCHLD)?,
            term: [
                (SIGTERM, flag(SIGTERM)?),
                (SIGHUP, flag(SIGHUP)?),
                (SIGINT, flag(SIGINT)?),
            ],
            wake,
        })
    }

    fn drain(&mut self) {
        let mut buf = [0u8; 256];
        while matches!(self.wake.read(&mut buf), Ok(n) if n > 0) {}
    }

    fn terminated(&self) -> Option<i32> {
        self.term
            .iter()
            .find(|(_, f)| f.swap(false, Ordering::Relaxed))
            .map(|(s, _)| *s)
    }
}

struct Driver {
    cfg: Config,
    info: SessionInfo,
    start: Instant,
    /// The app; `None` in a replay.
    pty: Option<Pty>,
    recorder: Option<Recorder>,
    /// The tape of a replay, in place of an app.
    player: Option<Player>,
    /// A client holds keys of the host keyboard.
    grab: Option<Grab>,
    /// The pty returned end of file or EIO.
    pty_eof: bool,
    /// Bytes waiting to be typed into the app.
    pty_queue: Vec<u8>,
    host: Host,
    stdin_open: bool,
    signals: Signals,
    listener: UnixListener,
    conns: BTreeMap<ConnId, Conn>,
    /// Union of the topics clients subscribed to.
    topics: HashSet<String>,
    pending: HashMap<Token, (ConnId, Value)>,
    /// Messages to send once the request being answered is.
    after: Vec<(ConnId, Value)>,
    next_conn: u64,
    next_token: u64,
    repaint: bool,
    exit_status: Option<i32>,
    colors: Known,
    replies: Replies,
    /// When to stop waiting for the host's colours.
    replies_until: Option<Duration>,
    /// The host was asked for mouse reports, for a client.
    host_mouse: bool,
    /// A press went to this sprite: so do the moves and the release.
    mouse_held: Option<Value>,
    /// A replay: makes the engine again when the tape is rewound.
    factory: Option<Box<dyn FnMut() -> Box<dyn Engine>>>,
}

/// Name the session and listen on its socket.
fn bind_session(cfg: &Config) -> Result<(String, PathBuf, UnixListener), RunError> {
    let name = cfg
        .name
        .clone()
        .unwrap_or_else(|| std::process::id().to_string());
    if !session::valid_name(&name) {
        return Err(
            RunError::new(RunErrorKind::Usage, format!("bad session name: {name}")).with_hint(
                "a name is made of letters, digits, `.`, `_`, `-`, and does not start with `.`",
            ),
        );
    }
    let dir = session::ensure_runtime_dir().map_err(|e| {
        RunError::new(
            RunErrorKind::Session,
            format!(
                "no socket directory {}: {}",
                session::runtime_dir().display(),
                start::describe(&e)
            ),
        )
        .with_hint("XDG_RUNTIME_DIR must be a directory of yours, closed to others")
    })?;
    let socket = session::socket_path(&name);
    if socket.exists() {
        if session::is_live(&socket) {
            return Err(RunError::new(
                RunErrorKind::Session,
                format!("session {name} is already running"),
            )
            .with_hint(format!(
                "choose another --name, or pilot it: ptyrc -s {name} text"
            )));
        }
        fs::remove_file(&socket)?;
    }
    let bind = |e: io::Error| {
        RunError::new(
            RunErrorKind::Session,
            format!("no socket in {}: {}", dir.display(), start::describe(&e)),
        )
    };
    let listener = UnixListener::bind(&socket).map_err(bind)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok((name, socket, listener))
}

/// Run a session to its end: spawn the app, serve, return its exit status
/// (128 + signal when killed).
pub fn run(cfg: Config, mut engine: Box<dyn Engine>) -> Result<i32, RunError> {
    let mut driver = Driver::start(cfg)?;
    let result = driver.serve(&mut engine);
    let status = driver.finish(engine.as_mut(), result.as_ref().ok().copied());
    result.and(Ok(status)).map_err(RunError::from)
}

/// Replay a tape in place of an app: a headless session whose clock is
/// the tape's. Returns the status the tape ended with.
/// `factory` makes the engine: once at the start, and again each time the
/// tape is rewound.
pub fn replay(
    cfg: Config,
    replay: Replay,
    mut factory: Box<dyn FnMut() -> Box<dyn Engine>>,
) -> Result<i32, RunError> {
    let mut driver = Driver::start_replay(cfg, replay)?;
    let mut engine = factory();
    driver.factory = Some(factory);
    let result = driver.serve(&mut engine);
    let status = driver.finish(engine.as_mut(), result.as_ref().ok().copied());
    result.and(Ok(status)).map_err(RunError::from)
}

/// Run an engine hook with a `Ctx`, then apply what it asked for.
macro_rules! hook {
    ($driver:expr, $now:expr, |$ctx:ident| $body:expr) => {{
        let mut fx = Effects::default();
        let value = {
            let mut $ctx = Ctx {
                info: &$driver.info,
                topics: &$driver.topics,
                now: $now,
                fx: &mut fx,
            };
            let $ctx = &mut $ctx;
            $body
        };
        $driver.apply(fx);
        value
    }};
}

impl Driver {
    fn start(cfg: Config) -> Result<Driver, RunError> {
        let Some(program) = cfg.argv.first() else {
            return Err(RunError::new(RunErrorKind::Usage, "no command")
                .with_hint("ptyrc run -- CMD [ARG...]"));
        };
        start::check_command(program)?;
        let (name, socket, listener) = bind_session(&cfg)?;

        // From here the socket file exists: remove it on any failure.
        let built = (|| {
            let signals = Signals::install()?;
            let host = if cfg.headless {
                let log = match &cfg.host_log {
                    Some(path) => Some(OpenOptions::new().create(true).append(true).open(path)?),
                    None => None,
                };
                Host::headless(log)
            } else {
                Host::terminal()?
            };
            let (cols, rows) = match (cfg.headless, cfg.size) {
                (true, size) => size.unwrap_or(DEFAULT_SIZE),
                (false, size) => host.size().or(size).unwrap_or(DEFAULT_SIZE),
            };
            let mut env = vec![
                (ENV_SESSION.to_string(), name.clone()),
                (
                    ENV_SOCKET.to_string(),
                    socket.to_string_lossy().into_owned(),
                ),
            ];
            if std::env::var_os("TERM").is_none() {
                env.push(("TERM".to_string(), "xterm-256color".to_string()));
            }
            let header = Header {
                size: (cols, rows),
                argv: cfg.argv.clone(),
                host_colors: Some(Known::new(cfg.host_fg, cfg.host_bg).colors),
                presets: cfg.presets,
            };
            let recorder = match &cfg.record {
                Some(path) => Some(Recorder::create(path, &header, cfg.record_input).map_err(
                    |e| {
                        RunError::new(
                            RunErrorKind::Usage,
                            format!("no tape {}: {}", path.display(), start::describe(&e)),
                        )
                    },
                )?),
                None => None,
            };
            let pty = pty::spawn(&cfg.argv, &env, cols, rows)
                .map_err(|e| start::exec_error(program, &e))?;
            Ok::<_, RunError>((signals, host, pty, cols, rows, recorder))
        })();
        let (signals, host, pty, cols, rows, recorder) = match built {
            Ok(parts) => parts,
            Err(e) => {
                let _ = fs::remove_file(&socket);
                return Err(e);
            }
        };

        let info = SessionInfo {
            name,
            socket,
            argv: cfg.argv.clone(),
            pid: pty.child.id(),
            cols,
            rows,
            headless: cfg.headless,
        };
        let colors = Known::new(cfg.host_fg, cfg.host_bg);
        Ok(Driver {
            stdin_open: host.interactive || !cfg.headless,
            cfg,
            info,
            start: Instant::now(),
            pty: Some(pty),
            recorder,
            player: None,
            grab: None,
            pty_eof: false,
            pty_queue: Vec::new(),
            host,
            signals,
            listener,
            conns: BTreeMap::new(),
            topics: HashSet::new(),
            pending: HashMap::new(),
            after: Vec::new(),
            next_conn: 1,
            next_token: 1,
            repaint: false,
            exit_status: None,
            colors,
            replies: Replies::default(),
            replies_until: None,
            host_mouse: false,
            mouse_held: None,
            factory: None,
        })
    }

    fn start_replay(cfg: Config, replay: Replay) -> Result<Driver, RunError> {
        let (name, socket, listener) = bind_session(&cfg)?;
        let signals = match Signals::install() {
            Ok(signals) => signals,
            Err(e) => {
                let _ = fs::remove_file(&socket);
                return Err(e.into());
            }
        };
        let (cols, rows) = replay.tape.header.size;
        let info = SessionInfo {
            name,
            socket,
            argv: replay.tape.header.argv.clone(),
            pid: 0,
            cols,
            rows,
            headless: true,
        };
        let colors = Self::tape_colors(&cfg, &replay.tape.header);
        Ok(Driver {
            stdin_open: false,
            cfg,
            info,
            start: Instant::now(),
            after: Vec::new(),
            pty: None,
            recorder: None,
            player: Some(Player::new(replay)),
            grab: None,
            pty_eof: true,
            pty_queue: Vec::new(),
            host: Host::headless(None),
            signals,
            listener,
            conns: BTreeMap::new(),
            topics: HashSet::new(),
            pending: HashMap::new(),
            next_conn: 1,
            next_token: 1,
            repaint: false,
            exit_status: None,
            colors,
            replies: Replies::default(),
            replies_until: None,
            host_mouse: false,
            mouse_held: None,
            factory: None,
        })
    }

    /// The colours a replay starts with: the options, else the tape's.
    fn tape_colors(cfg: &Config, header: &Header) -> Known {
        let tape = header.host_colors;
        Known::new(
            cfg.host_fg.or(tape.map(|c| c.fg)),
            cfg.host_bg.or(tape.map(|c| c.bg)),
        )
    }

    /// The tape was rewound: a new engine, the session as at its start.
    /// Sprites are gone; requests the old engine held are refused.
    fn rewind(&mut self, engine: &mut Box<dyn Engine>) {
        let Some(player) = &self.player else {
            return;
        };
        let header = player.tape().header.clone();
        let held: Vec<Token> = self
            .pending
            .keys()
            .filter(|token| !player.waiters.contains(token))
            .copied()
            .collect();
        for token in held {
            if let Some((conn, id)) = self.pending.remove(&token) {
                let error = RpcError::new(ErrorCode::NotUsable, "the tape was rewound")
                    .with_hint("ask again: the session started over");
                self.respond(conn, &id, Err(error));
            }
        }
        if let Some(factory) = &mut self.factory {
            *engine = factory();
        }
        (self.info.cols, self.info.rows) = header.size;
        self.colors = Self::tape_colors(&self.cfg, &header);
        self.grab = None;
        hook!(self, Duration::ZERO, |ctx| engine.on_start(ctx));
        let colors = self.colors.colors;
        hook!(self, Duration::ZERO, |ctx| engine
            .on_host_colors(colors, ctx));
        self.emit("size", json!({"size": [header.size.0, header.size.1]}));
        self.repaint = true;
    }

    /// Monotonic time: since the start, or the tape's in a replay.
    fn now(&self) -> Duration {
        match &self.player {
            Some(player) => player.now(),
            None => self.start.elapsed(),
        }
    }

    /// Feed what the tape holds up to its present time.
    fn play_tape(&mut self, engine: &mut dyn Engine) -> io::Result<()> {
        loop {
            let Some(player) = &mut self.player else {
                return Ok(());
            };
            match player.step() {
                Step::Feed(Record::Output(burst)) => {
                    let now = self.now();
                    let mut out = Vec::with_capacity(burst.len() + 256);
                    hook!(self, now, |ctx| engine.on_burst(&burst, now, &mut out, ctx));
                }
                Step::Feed(Record::Resize(cols, rows)) => self.resize(engine, cols, rows)?,
                Step::Feed(Record::HostColors(colors)) => {
                    self.colors.learn(0, colors.fg);
                    self.colors.learn(1, colors.bg);
                    hook!(self, self.now(), |ctx| {
                        engine.on_host_colors(colors, ctx);
                        ctx.request_repaint();
                    });
                }
                Step::Feed(_) => {}
                Step::Paused => {
                    let status = player.status_json();
                    for token in std::mem::take(&mut player.waiters) {
                        if let Some((conn, id)) = self.pending.remove(&token) {
                            self.respond(conn, &id, Ok(status.clone()));
                        }
                    }
                    return Ok(());
                }
                Step::Done => {
                    self.exit_status = Some(player.status);
                    return Ok(());
                }
                Step::Wait => return Ok(()),
            }
        }
    }

    fn hello(&self) -> Value {
        json!({
            "protocol": PROTOCOL,
            "server": SERVER,
            "session": self.info.name,
            "pid": self.info.pid,
            "driver_pid": std::process::id(),
            "argv": self.info.argv,
            "size": [self.info.cols, self.info.rows],
            "headless": self.info.headless,
            "host_colors": self.colors.json(),
        })
    }

    /// The loop. Returns the exit status of the app.
    fn serve(&mut self, engine: &mut Box<dyn Engine>) -> io::Result<i32> {
        hook!(self, self.now(), |ctx| engine.on_start(ctx));
        let colors = self.colors.colors;
        hook!(self, self.now(), |ctx| engine.on_host_colors(colors, ctx));
        self.ask_host_colors()?;
        loop {
            if self.player.as_mut().is_some_and(Player::take_rewound) {
                self.rewind(engine);
            }
            let engine = engine.as_mut();
            self.play_tape(engine)?;
            // Tick: deadline passed, or repaint asked.
            let now = self.now();
            let wake = engine.next_wake(now);
            if self.repaint || wake.is_some_and(|w| w <= now) {
                self.repaint = false;
                let mut out = Vec::new();
                hook!(self, now, |ctx| engine.on_tick(now, &mut out, ctx));
                self.host.write(&out)?;
            }

            if let Some(status) = self.exit_status
                && (self.pty_eof || self.drain_pty(engine)?)
            {
                return Ok(status);
            }

            let now = self.now();
            if self.replies_until.is_some_and(|t| t <= now) {
                // The host does not tell: the defaults stand.
                self.replies_until = None;
                let held = self.replies.give_up();
                self.forward_input(&held, engine);
            }
            let timeout = if self.repaint {
                Some(Duration::ZERO)
            } else if let Some(player) = &mut self.player {
                // Tape time: nothing is due while paused; a tape run
                // without waiting moves its clock in place of sleeping.
                let wake = match (engine.next_wake(now), player.due()) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                match wake {
                    Some(at) if !player.paused() => {
                        Some(player.sleep_until(at).unwrap_or(Duration::ZERO))
                    }
                    // The clock stands still: what waits on it is held
                    // to the real one.
                    _ if player.paused() => engine
                        .stall_wake()
                        .map(|at| at.saturating_duration_since(Instant::now())),
                    _ => None,
                }
            } else {
                let wake = engine.next_wake(now);
                let wake = match (wake, self.replies_until) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                wake.map(|w| w.saturating_sub(now))
            };
            let ready = self.wait(timeout)?;
            if self.player.as_ref().is_some_and(Player::paused) {
                hook!(self, self.now(), |ctx| engine.on_stall(ctx));
            }

            if ready.signal {
                self.signals.drain();
            }
            if let Some(signal) = self.signals.terminated() {
                return Ok(self.terminate(signal));
            }
            if self.signals.winch.swap(false, Ordering::Relaxed) {
                self.resize_from_host(engine)?;
            }
            if self.signals.chld.swap(false, Ordering::Relaxed) || self.pty_eof {
                self.reap()?;
            }
            if ready.pty_in {
                self.pump_pty(engine)?;
            }
            if ready.stdin {
                self.pump_stdin(engine)?;
            }
            if ready.pty_out || !self.pty_queue.is_empty() {
                self.flush_pty();
            }
            if ready.listener {
                self.accept();
            }
            for id in ready.conns_in {
                self.pump_conn(id, engine);
            }
            for id in ready.conns_out {
                if let Some(conn) = self.conns.get_mut(&id) {
                    conn.flush();
                }
            }
            self.reap_conns(engine);
        }
    }

    /// Sleep until something is ready or `timeout` passes.
    fn wait(&mut self, timeout: Option<Duration>) -> io::Result<Ready> {
        #[derive(Clone, Copy)]
        enum Src {
            Pty,
            Stdin,
            Signal,
            Listener,
            Conn(ConnId),
        }
        let stdin = rustix::stdio::stdin();
        let mut srcs = Vec::new();
        let mut fds = Vec::new();
        if let Some(pty) = self.pty.as_ref().filter(|_| !self.pty_eof) {
            let mut flags = PollFlags::IN;
            if !self.pty_queue.is_empty() {
                flags |= PollFlags::OUT;
            }
            srcs.push(Src::Pty);
            fds.push(PollFd::new(&pty.master, flags));
        }
        if self.stdin_open && self.pty_queue.len() < PTY_QUEUE_HIGH {
            srcs.push(Src::Stdin);
            fds.push(PollFd::new(&stdin, PollFlags::IN));
        }
        srcs.push(Src::Signal);
        fds.push(PollFd::new(&self.signals.wake, PollFlags::IN));
        srcs.push(Src::Listener);
        fds.push(PollFd::new(&self.listener, PollFlags::IN));
        for (id, conn) in &self.conns {
            let mut flags = PollFlags::IN;
            if conn.wants_write() {
                flags |= PollFlags::OUT;
            }
            srcs.push(Src::Conn(*id));
            fds.push(PollFd::new(&conn.stream, flags));
        }
        let timeout = timeout.map(|t| Timespec {
            tv_sec: t.as_secs() as i64,
            tv_nsec: t.subsec_nanos() as _,
        });
        let mut ready = Ready::default();
        match poll(&mut fds, timeout.as_ref()) {
            Ok(_) => {}
            Err(Errno::INTR) => {
                ready.signal = true;
                return Ok(ready);
            }
            Err(e) => return Err(e.into()),
        }
        let hup = PollFlags::HUP | PollFlags::ERR;
        for (src, fd) in srcs.iter().zip(&fds) {
            let got = fd.revents();
            let readable = got.intersects(PollFlags::IN | hup);
            match src {
                Src::Pty => {
                    ready.pty_in = readable;
                    ready.pty_out = got.contains(PollFlags::OUT);
                }
                Src::Stdin => ready.stdin = readable,
                Src::Signal => ready.signal = readable,
                Src::Listener => ready.listener = readable,
                Src::Conn(id) => {
                    if readable {
                        ready.conns_in.push(*id);
                    }
                    if got.contains(PollFlags::OUT) {
                        ready.conns_out.push(*id);
                    }
                }
            }
        }
        Ok(ready)
    }

    /// Read one burst from the app: all that is readable, then what comes
    /// within `coalesce_ms`, bounded in time and size.
    fn read_burst(&mut self) -> Vec<u8> {
        let first = Instant::now();
        let quiet = Duration::from_millis(self.cfg.coalesce_ms);
        let mut burst = Vec::new();
        let mut buf = [0u8; 65536];
        let Some(pty) = &self.pty else {
            return burst;
        };
        while burst.len() < BURST_MAX_BYTES {
            match rustix::io::read(&pty.master, &mut buf) {
                Ok(0) => {
                    self.pty_eof = true;
                    break;
                }
                Ok(n) => {
                    burst.extend_from_slice(&buf[..n]);
                    if first.elapsed() >= BURST_MAX_TIME {
                        break;
                    }
                }
                Err(Errno::INTR) => {}
                Err(Errno::AGAIN) => {
                    let left = BURST_MAX_TIME.saturating_sub(first.elapsed()).min(quiet);
                    // A few bytes are a key echo or a cursor move: no wait.
                    if burst.len() < BURST_MIN_WAIT || left.is_zero() {
                        break;
                    }
                    let wait = Timespec {
                        tv_sec: 0,
                        tv_nsec: left.subsec_nanos() as _,
                    };
                    let mut fds = [PollFd::new(&pty.master, PollFlags::IN)];
                    match poll(&mut fds, Some(&wait)) {
                        Ok(n) if n > 0 => {}
                        Err(Errno::INTR) => {}
                        _ => break,
                    }
                }
                // EIO: the app side of the pty is closed.
                Err(_) => {
                    self.pty_eof = true;
                    break;
                }
            }
        }
        burst
    }

    fn pump_pty(&mut self, engine: &mut dyn Engine) -> io::Result<bool> {
        let burst = self.read_burst();
        if burst.is_empty() {
            return Ok(false);
        }
        let now = self.now();
        if let Some(recorder) = &mut self.recorder {
            recorder.output(now, &burst);
        }
        let mut out = Vec::with_capacity(burst.len() + 256);
        hook!(self, now, |ctx| engine.on_burst(&burst, now, &mut out, ctx));
        if self.host_mouse && burst.windows(3).any(|w| w == b"\x1b[?") {
            // The app may have set the mouse of the host: ours again.
            out.extend_from_slice(mouse::HOST_ON);
        }
        self.host.write(&out)?;
        Ok(true)
    }

    /// After the app exited: forward what it left in the pty. `true` when
    /// nothing is left.
    fn drain_pty(&mut self, engine: &mut dyn Engine) -> io::Result<bool> {
        for _ in 0..64 {
            if self.pty_eof || !self.pump_pty(engine)? {
                return Ok(true);
            }
        }
        Ok(true)
    }

    fn pump_stdin(&mut self, engine: &mut dyn Engine) -> io::Result<()> {
        let mut buf = [0u8; 65536];
        match rustix::io::read(rustix::stdio::stdin(), &mut buf) {
            Ok(0) => self.stdin_open = false,
            Ok(n) => {
                let fed = self.replies.feed(&buf[..n]);
                let mut changed = false;
                for (which, rgb) in fed.colors.into_iter().enumerate() {
                    if let Some(rgb) = rgb {
                        changed |= self.colors.learn(which, rgb);
                    }
                }
                if !self.replies.waiting() {
                    self.replies_until = None;
                }
                if changed {
                    let colors = self.colors.colors;
                    let now = self.now();
                    if let Some(recorder) = &mut self.recorder {
                        recorder.host_colors(now, colors);
                    }
                    hook!(self, now, |ctx| {
                        engine.on_host_colors(colors, ctx);
                        ctx.request_repaint();
                    });
                }
                self.forward_input(&fed.forward, engine);
            }
            Err(Errno::INTR | Errno::AGAIN) => {}
            Err(_) => self.stdin_open = false,
        }
        Ok(())
    }

    /// Ask the host for mouse reports while a client listens to `mouse`;
    /// give it back the modes of the app after.
    fn sync_host_mouse(&mut self, engine: &dyn Engine) {
        let wanted = self.host.interactive && self.topics.contains("mouse");
        if wanted == self.host_mouse {
            return;
        }
        self.host_mouse = wanted;
        let _ = if wanted {
            self.host.write(mouse::HOST_ON)
        } else {
            self.mouse_held = None;
            let mut out = mouse::HOST_OFF.to_vec();
            out.extend(engine.mouse_mode().set_sequence());
            self.host.write(&out)
        };
    }

    /// Take the mouse reports out of keyboard bytes: one on a sprite is
    /// a `mouse` event; another is written again as the app asked, or
    /// dropped when it did not.
    fn sort_mouse(&mut self, bytes: &[u8], engine: &dyn Engine) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        let mut rest = bytes;
        while let Some(start) = mouse::find(rest) {
            out.extend_from_slice(&rest[..start]);
            rest = &rest[start..];
            let Some((report, len)) = mouse::parse(rest) else {
                out.extend_from_slice(&rest[..3]);
                rest = &rest[3..];
                continue;
            };
            rest = &rest[len..];
            let hit = match self.mouse_held.clone() {
                // Held: the sprite of the press, the cell of the report.
                Some(mut held) if !report.press() => {
                    let at = engine.sprite_at(report.x, report.y);
                    let same = at.as_ref().is_some_and(|at| at["sprite"] == held["sprite"]);
                    match at {
                        Some(at) if same => Some(at),
                        _ => {
                            held["x"] = json!(report.x);
                            held["y"] = json!(report.y);
                            held["at"] = Value::Null;
                            held["frame"] = Value::Null;
                            held["pos"] = Value::Null;
                            Some(held)
                        }
                    }
                }
                _ => engine.sprite_at(report.x, report.y),
            };
            match hit {
                Some(mut data) => {
                    if report.press() {
                        self.mouse_held = Some(data.clone());
                    } else if report.release {
                        self.mouse_held = None;
                    }
                    data["button"] = json!(report.button());
                    data["action"] = json!(report.action());
                    data["mods"] = json!(report.mods());
                    self.emit("mouse", data);
                }
                None => out.extend(report.encode(engine.mouse_mode()).unwrap_or_default()),
            }
        }
        out.extend_from_slice(rest);
        out
    }

    /// Keyboard bytes on their way to the app.
    fn forward_input(&mut self, bytes: &[u8], engine: &mut dyn Engine) {
        let sorted;
        let bytes = if self.host_mouse {
            sorted = self.sort_mouse(bytes, engine);
            &sorted[..]
        } else {
            bytes
        };
        let held;
        let bytes = match &self.grab {
            Some(grab) => {
                let sorted = grab.sort(bytes);
                let conn = grab.conn;
                for (name, key) in sorted.taken {
                    let data = json!({"key": name, "bytes": keys::base64_encode(&key),
                                      "grabbed": true});
                    self.send_event(conn, "input", data);
                }
                if sorted.released {
                    self.grab = None;
                    self.send_event(conn, "input", json!({"released": "escape"}));
                }
                held = sorted.forward;
                &held[..]
            }
            None => bytes,
        };
        if bytes.is_empty() {
            return;
        }
        let forward = hook!(self, self.now(), |ctx| {
            if ctx.has_subscribers("input") {
                ctx.emit("input", json!({"bytes": keys::base64_encode(bytes)}));
            }
            engine.on_input(bytes, ctx)
        });
        if forward {
            self.pty_queue.extend_from_slice(bytes);
        }
    }

    /// Ask the host terminal for the default colours nobody gave. The
    /// replies come with the keys.
    fn ask_host_colors(&mut self) -> io::Result<()> {
        use crate::hostcolors::Source;
        let ask = |source: Source| source == Source::Default;
        let (fg, bg) = (ask(self.colors.from[0]), ask(self.colors.from[1]));
        if !self.cfg.ask_host_colors
            || !self.host.interactive
            || !rustix::termios::isatty(rustix::stdio::stdout())
            || !(fg || bg)
        {
            return Ok(());
        }
        self.replies.expect(fg, bg);
        self.replies_until = Some(self.now() + HOST_COLORS_WAIT);
        if fg {
            self.host.write(QUERY_FG)?;
        }
        if bg {
            self.host.write(QUERY_BG)?;
        }
        Ok(())
    }

    /// Type queued bytes into the app, as much as the pty takes.
    fn flush_pty(&mut self) {
        let mut sent = 0;
        let Some(pty) = &self.pty else {
            // A tape reads nothing.
            self.pty_queue.clear();
            return;
        };
        while sent < self.pty_queue.len() {
            match rustix::io::write(&pty.master, &self.pty_queue[sent..]) {
                Ok(0) => break,
                Ok(n) => sent += n,
                Err(Errno::INTR) => {}
                Err(Errno::AGAIN) => break,
                Err(_) => {
                    // The app is gone; nobody will read this.
                    sent = self.pty_queue.len();
                }
            }
        }
        if let Some(recorder) = &mut self.recorder {
            recorder.input(self.start.elapsed(), &self.pty_queue[..sent]);
        }
        self.pty_queue.drain(..sent);
    }

    fn reap(&mut self) -> io::Result<()> {
        if self.exit_status.is_none()
            && let Some(pty) = &mut self.pty
            && let Some(status) = pty.child.try_wait()?
        {
            self.exit_status = Some(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
            );
        }
        Ok(())
    }

    /// A fatal signal reached the driver: hang up the app.
    fn terminate(&mut self, signal: i32) -> i32 {
        if let Some(pty) = &mut self.pty
            && let Some(pid) = Pid::from_raw(self.info.pid as i32)
        {
            let _ = kill_process_group(pid, Signal::HUP);
            let deadline = Instant::now() + Duration::from_millis(300);
            while Instant::now() < deadline && matches!(pty.child.try_wait(), Ok(None)) {
                std::thread::sleep(Duration::from_millis(10));
            }
            if matches!(pty.child.try_wait(), Ok(None)) {
                let _ = kill_process_group(pid, Signal::KILL);
                let _ = pty.child.wait();
            }
        }
        128 + signal
    }

    fn resize_from_host(&mut self, engine: &mut dyn Engine) -> io::Result<()> {
        match self.host.size() {
            Some((cols, rows)) => self.resize(engine, cols, rows),
            None => Ok(()),
        }
    }

    fn resize(&mut self, engine: &mut dyn Engine, cols: u16, rows: u16) -> io::Result<()> {
        if (cols, rows) == (self.info.cols, self.info.rows) {
            return Ok(());
        }
        self.info.cols = cols;
        self.info.rows = rows;
        let now = self.now();
        if let Some(pty) = &self.pty {
            // Also signals SIGWINCH to the app.
            pty::set_winsize(pty.master.as_fd(), cols, rows)?;
        }
        if let Some(recorder) = &mut self.recorder {
            recorder.resize(now, cols, rows);
        }
        let mut out = Vec::new();
        hook!(self, now, |ctx| {
            engine.on_resize(cols, rows, now, &mut out, ctx);
            ctx.emit("size", json!({"size": [cols, rows]}));
        });
        self.host.write(&out)
    }

    fn accept(&mut self) {
        while let Ok((stream, _)) = self.listener.accept() {
            let Ok(mut conn) = Conn::new(stream) else {
                continue;
            };
            conn.send(&event("hello", self.hello()));
            self.conns.insert(ConnId(self.next_conn), conn);
            self.next_conn += 1;
        }
    }

    fn pump_conn(&mut self, id: ConnId, engine: &mut dyn Engine) {
        let Some(conn) = self.conns.get_mut(&id) else {
            return;
        };
        for line in conn.read() {
            let request: Request = match serde_json::from_slice(&line) {
                Ok(request) => request,
                Err(e) => {
                    let id_value = serde_json::from_slice::<Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id").cloned())
                        .unwrap_or(Value::Null);
                    let error = RpcError::new(ErrorCode::BadRequest, e.to_string());
                    self.respond(id, &id_value, Err(error));
                    continue;
                }
            };
            let Request {
                id: request_id,
                method,
                params,
            } = request;
            match self.dispatch(id, &method, params, engine, false) {
                Some(result) => {
                    self.respond(id, &request_id, result);
                    for (to, message) in std::mem::take(&mut self.after) {
                        if let Some(conn) = self.conns.get_mut(&to) {
                            conn.send(&message);
                        }
                    }
                }
                None => {
                    // The engine holds the token allocated last.
                    self.pending
                        .insert(Token(self.next_token - 1), (id, request_id));
                }
            }
        }
    }

    fn respond(&mut self, conn: ConnId, id: &Value, result: Result<Value, RpcError>) {
        if let Some(conn) = self.conns.get_mut(&conn) {
            conn.send(&response(id, result));
        }
    }

    /// Answer a request. `None`: the engine left it pending.
    fn dispatch(
        &mut self,
        conn: ConnId,
        method: &str,
        params: Value,
        engine: &mut dyn Engine,
        in_batch: bool,
    ) -> Option<Result<Value, RpcError>> {
        Some(match method {
            "session.info" => Ok(self.session_info(engine)),
            "session.resize" => self.session_resize(&params, engine),
            "subscribe" => self.subscribe(conn, &params, engine),
            "input.send" => self.input_send(&params, engine),
            "input.click" => self.input_click(&params, engine),
            "input.grab" => self.input_grab(conn, &params),
            "input.release" => {
                self.grab.take_if(|g| g.conn == conn);
                Ok(json!({}))
            }
            _ if method.starts_with("tape.") => self.tape(method, &params, in_batch)?,
            "batch" if in_batch => Err(RpcError::new(ErrorCode::BadRequest, "batch inside batch")),
            "batch" => self.batch(conn, params, engine),
            _ => {
                let token = Token(self.next_token);
                self.next_token += 1;
                let now = self.now();
                match hook!(self, now, |ctx| engine
                    .handle(conn, token, method, params, ctx))
                {
                    Reply::Ok(value) => Ok(value),
                    Reply::Err(error) => Err(error),
                    Reply::Pending if in_batch => Err(RpcError::new(
                        ErrorCode::BadRequest,
                        format!("{method}: not allowed in a batch"),
                    )),
                    Reply::Pending => return None,
                }
            }
        })
    }

    fn session_info(&self, engine: &dyn Engine) -> Value {
        let mut info = self.hello();
        let object = info.as_object_mut().expect("hello is an object");
        object.insert("exited".into(), json!(self.exit_status.is_some()));
        object.insert("grabbed".into(), json!(self.grab.is_some()));
        if let Value::Object(extras) = engine.session_extras() {
            object.extend(extras);
        }
        if let Some(player) = &self.player {
            object.insert("tape".into(), player.status_json());
        }
        info
    }

    fn session_resize(
        &mut self,
        params: &Value,
        engine: &mut dyn Engine,
    ) -> Result<Value, RpcError> {
        if !self.info.headless {
            return Err(
                RpcError::new(ErrorCode::BadRequest, "the host terminal sets the size")
                    .with_hint("only a headless session is resized: ptyrc run --headless"),
            );
        }
        let size = params
            .get("size")
            .and_then(Value::as_array)
            .filter(|s| s.len() == 2);
        let dim = |i: usize| {
            size.and_then(|s| s[i].as_u64())
                .filter(|&n| (1..=4096).contains(&n))
        };
        let (Some(cols), Some(rows)) = (dim(0), dim(1)) else {
            return Err(RpcError::bad_params("size: [cols, rows]"));
        };
        self.resize(engine, cols as u16, rows as u16)
            .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
        Ok(json!({}))
    }

    fn subscribe(
        &mut self,
        conn: ConnId,
        params: &Value,
        engine: &mut dyn Engine,
    ) -> Result<Value, RpcError> {
        let topics = params
            .get("topics")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .map(|t| t.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
            })
            .ok_or_else(|| RpcError::bad_params("topics: array of strings"))?;
        if let Some(bad) = topics.iter().find(|t| !TOPICS.contains(&t.as_str())) {
            return Err(
                RpcError::bad_params(format!("no such topic: {bad}")).with_hint(format!(
                    "{}topics: {}",
                    suggest::did_you_mean(bad, TOPICS),
                    TOPICS.join(", ")
                )),
            );
        }
        let snapshot = match params.get("snapshot") {
            None => false,
            Some(Value::Bool(snapshot)) => *snapshot,
            Some(_) => return Err(RpcError::bad_params("snapshot: true or false")),
        };
        if snapshot {
            // Sent after the answer, to this client only.
            let now = self.now();
            for topic in &topics {
                let state = match topic.as_str() {
                    "size" => vec![json!({"size": [self.info.cols, self.info.rows]})],
                    topic => engine.snapshot(topic, now),
                };
                let events = state.into_iter().map(|data| event(topic, data));
                self.after.extend(events.map(|message| (conn, message)));
            }
        }
        if let Some(conn) = self.conns.get_mut(&conn) {
            conn.topics.extend(topics);
        }
        self.refresh_topics();
        self.sync_host_mouse(engine);
        // A tick, so that the engine sees the new topics.
        self.repaint = true;
        Ok(json!({}))
    }

    fn refresh_topics(&mut self) {
        self.topics = self
            .conns
            .values()
            .flat_map(|c| c.topics.iter().cloned())
            .collect();
    }

    fn input_send(&mut self, params: &Value, engine: &dyn Engine) -> Result<Value, RpcError> {
        let mut bytes = Vec::new();
        let mut given = false;
        if let Some(text) = params.get("text") {
            let text = text
                .as_str()
                .ok_or_else(|| RpcError::bad_params("text: string"))?;
            let paste = params
                .get("paste")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if paste && engine.bracketed_paste() {
                bytes.extend_from_slice(b"\x1b[200~");
                bytes.extend_from_slice(text.as_bytes());
                bytes.extend_from_slice(b"\x1b[201~");
            } else {
                bytes.extend_from_slice(text.as_bytes());
            }
            given = true;
        }
        if let Some(list) = params.get("keys") {
            let list = list
                .as_array()
                .ok_or_else(|| RpcError::bad_params("keys: array of strings"))?;
            let app_cursor = engine.app_cursor_keys();
            for key in list {
                let key = key
                    .as_str()
                    .ok_or_else(|| RpcError::bad_params("keys: array of strings"))?;
                let key = keys::encode_key(key, app_cursor)
                    .map_err(|e| RpcError::bad_params(e.message).with_hint(e.hint))?;
                bytes.extend(key);
            }
            given = true;
        }
        if let Some(raw) = params.get("bytes") {
            let raw = raw.as_str().and_then(keys::base64_decode);
            bytes.extend(raw.ok_or_else(|| RpcError::bad_params("bytes: base64"))?);
            given = true;
        }
        if !given {
            return Err(RpcError::bad_params("one of text, keys, bytes"));
        }
        self.type_in(bytes)
    }

    /// Queue bytes for the app, on behalf of a client.
    fn type_in(&mut self, bytes: Vec<u8>) -> Result<Value, RpcError> {
        if self.player.is_some() {
            return Err(RpcError::new(
                ErrorCode::BadRequest,
                "a replay takes no input",
            ));
        }
        if self.exit_status.is_some() {
            return Err(RpcError::new(ErrorCode::Gone, "the app exited"));
        }
        self.pty_queue.extend(bytes);
        Ok(json!({}))
    }

    /// A click at a screen cell, in the form the app asked for.
    fn input_click(&mut self, params: &Value, engine: &dyn Engine) -> Result<Value, RpcError> {
        let cell = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|n| u16::try_from(n).ok())
        };
        let (Some(x), Some(y)) = (cell("x"), cell("y")) else {
            return Err(RpcError::bad_params("x, y: a screen cell"));
        };
        let on_screen = |x: u16, y: u16| {
            if x >= self.info.cols || y >= self.info.rows {
                return Err(RpcError::bad_params(format!(
                    "{x},{y} is off the screen ({}x{})",
                    self.info.cols, self.info.rows
                )));
            }
            Ok((x, y))
        };
        let at = on_screen(x, y)?;
        let to = match params.get("to") {
            None | Some(Value::Null) => None,
            Some(to) => {
                let cell = |i: usize| to.get(i).and_then(Value::as_u64);
                let cell = |i: usize| cell(i).and_then(|n| u16::try_from(n).ok());
                match (cell(0), cell(1)) {
                    (Some(x), Some(y)) => Some(on_screen(x, y)?),
                    _ => return Err(RpcError::bad_params("to: [x, y], a screen cell")),
                }
            }
        };
        let mut mods = 0;
        if let Some(list) = params.get("mods").filter(|m| !m.is_null()) {
            let names = list.as_array().map(|a| a.iter().map(Value::as_str));
            for name in names.ok_or_else(|| RpcError::bad_params("mods: array of names"))? {
                let name = name.unwrap_or("");
                mods |= mouse::mod_bits(name).ok_or_else(|| {
                    RpcError::bad_params(format!("mods: no such modifier: {name}")).with_hint(
                        format!(
                            "{}modifiers: {}",
                            suggest::did_you_mean(name, mouse::MODS),
                            mouse::MODS.join(", ")
                        ),
                    )
                })?;
            }
        }
        let count = match params.get("count") {
            None | Some(Value::Null) => 1,
            Some(count) => match count.as_u64() {
                Some(n @ 1..=3) => n as u8,
                _ => return Err(RpcError::bad_params("count: 1, 2 or 3 clicks")),
            },
        };
        let button = params
            .get("button")
            .map_or(Some("left"), Value::as_str)
            .filter(|b| mouse::BUTTONS.contains(b))
            .ok_or_else(|| {
                let given = params.get("button").and_then(Value::as_str).unwrap_or("");
                RpcError::bad_params("button: no such button").with_hint(format!(
                    "{}buttons: {}",
                    suggest::did_you_mean(given, mouse::BUTTONS),
                    mouse::BUTTONS.join(", ")
                ))
            })?;
        let mode = engine.mouse_mode();
        if !mode.on {
            return Err(RpcError::new(
                ErrorCode::NotUsable,
                "the app does not listen to the mouse",
            )
            .with_hint("it asked for no mouse reports (modes 9, 1000, 1002, 1003)"));
        }
        let gesture = mouse::Gesture {
            button,
            mods,
            at,
            to,
            count,
        };
        let bytes = mouse::gesture(mode, gesture).ok_or_else(|| {
            RpcError::new(
                ErrorCode::NotUsable,
                "the app's mouse mode (X10) cannot tell a cell past 222",
            )
        })?;
        self.type_in(bytes)
    }

    fn input_grab(&mut self, conn: ConnId, params: &Value) -> Result<Value, RpcError> {
        if self.grab.as_ref().is_some_and(|g| g.conn != conn) {
            return Err(RpcError::new(
                ErrorCode::NotUsable,
                "another client holds the keys",
            ));
        }
        let keys = match params.get("keys") {
            None | Some(Value::Null) => None,
            Some(list) => Some(
                list.as_array()
                    .and_then(|a| {
                        a.iter()
                            .map(|k| k.as_str().map(str::to_string))
                            .collect::<Option<Vec<_>>>()
                    })
                    .ok_or_else(|| RpcError::bad_params("keys: array of key names"))?,
            ),
        };
        for key in keys.iter().flatten() {
            let known = keys::encode_key(key, false)
                .is_ok_and(|b| grab::name(&b).as_deref() == Some(key.as_str()));
            if !known {
                return Err(RpcError::bad_params(format!("not a key: {key}"))
                    .with_hint(format!("keys: {}", keys::KEY_NAMES)));
            }
        }
        let escape = match params.get("escape") {
            None | Some(Value::Null) => grab::ESCAPE,
            Some(key) => key
                .as_str()
                .ok_or_else(|| RpcError::bad_params("escape: a key name"))?,
        };
        let grab = Grab::new(conn, keys, escape).map_err(|e| {
            RpcError::bad_params(e)
                .with_hint("a named key, as ctrl+] or f12: it never reaches the app")
        })?;
        self.grab = Some(grab);
        Ok(json!({"escape": escape}))
    }

    /// `tape.*`: the clock of a replay. `None`: answered at the next
    /// pause.
    fn tape(
        &mut self,
        method: &str,
        params: &Value,
        in_batch: bool,
    ) -> Option<Result<Value, RpcError>> {
        let Some(player) = &mut self.player else {
            return Some(Err(RpcError::new(
                ErrorCode::BadRequest,
                format!("{method}: this session is not a replay"),
            )
            .with_hint("ptyrc run --record FILE -- CMD, then ptyrc replay FILE")));
        };
        let seconds = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_f64)
                .filter(|s| s.is_finite() && *s >= 0.0)
        };
        let moved = match method {
            "tape.status" => return Some(Ok(player.status_json())),
            "tape.quit" => {
                self.exit_status = Some(player.status);
                return Some(Ok(json!({})));
            }
            "tape.pause" => {
                player.pause();
                return Some(Ok(player.status_json()));
            }
            "tape.play" => {
                if params.get("speed").is_some() && seconds("speed").is_none() {
                    return Some(Err(RpcError::bad_params("speed: a number, 0 or more")));
                }
                player.play(seconds("speed"));
                return Some(Ok(player.status_json()));
            }
            "tape.seek" => match seconds("t") {
                Some(t) => {
                    player.seek(Duration::from_secs_f64(t));
                    Ok(())
                }
                None => Err("t: seconds of tape time".to_string()),
            },
            "tape.step" => {
                let records = params.get("records").map_or(Some(1), Value::as_u64);
                match records.filter(|n| *n > 0) {
                    Some(n) => player.step_records(n as usize),
                    None => Err("records: a count, 1 or more".to_string()),
                }
            }
            _ => return Some(Err(RpcError::unknown_method(method).with_hint(
                "tape methods: tape.status, tape.step, tape.seek, tape.play, tape.pause, tape.quit",
            ))),
        };
        if let Err(message) = moved {
            return Some(Err(RpcError::bad_params(message)));
        }
        if in_batch {
            return Some(Ok(json!({})));
        }
        let token = Token(self.next_token);
        self.next_token += 1;
        player.waiters.push(token);
        None
    }

    fn batch(
        &mut self,
        conn: ConnId,
        params: Value,
        engine: &mut dyn Engine,
    ) -> Result<Value, RpcError> {
        let Some(Value::Array(calls)) = params.get("calls").cloned() else {
            return Err(RpcError::bad_params("calls: array of {method, params}"));
        };
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            let result = match serde_json::from_value::<Request>(call) {
                Ok(call) => self
                    .dispatch(conn, &call.method, call.params, engine, true)
                    .expect("no pending reply in a batch"),
                Err(e) => Err(RpcError::new(ErrorCode::BadRequest, e.to_string())),
            };
            results.push(match result {
                Ok(value) => json!({"result": value}),
                Err(error) => json!({"error": error}),
            });
        }
        // The whole batch shows in one repaint.
        self.repaint = true;
        Ok(json!({"results": results}))
    }

    /// Apply what an engine hook asked for.
    fn apply(&mut self, fx: Effects) {
        self.pty_queue.extend(fx.pty_input);
        self.repaint |= fx.repaint;
        for (topic, data) in fx.events {
            self.emit(&topic, data);
        }
        for (token, result) in fx.completions {
            if let Some((conn, id)) = self.pending.remove(&token) {
                self.respond(conn, &id, result);
            }
        }
    }

    /// An event for one client, subscribed or not.
    fn send_event(&mut self, conn: ConnId, topic: &str, data: Value) {
        if let Some(conn) = self.conns.get_mut(&conn) {
            conn.send(&event(topic, data));
        }
    }

    fn emit(&mut self, topic: &str, data: Value) {
        if !self.topics.contains(topic) {
            return;
        }
        let message = event(topic, data);
        for conn in self.conns.values_mut().filter(|c| c.topics.contains(topic)) {
            conn.send(&message);
        }
    }

    /// Close the connections marked dead.
    fn reap_conns(&mut self, engine: &mut dyn Engine) {
        let dead: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.dead)
            .map(|(id, _)| *id)
            .collect();
        if dead.is_empty() {
            return;
        }
        for id in dead {
            self.conns.remove(&id);
            self.grab.take_if(|g| g.conn == id);
            self.pending.retain(|_, (conn, _)| *conn != id);
            self.refresh_topics();
            self.sync_host_mouse(engine);
            hook!(self, self.now(), |ctx| engine.on_disconnect(id, ctx));
        }
    }

    /// The end: clean the host, tell the clients, remove the socket.
    /// `status`: the app's exit status, or `None` after a driver error.
    fn finish(&mut self, engine: &mut dyn Engine, status: Option<i32>) -> i32 {
        let status = status.unwrap_or(1);
        if let Some(recorder) = &mut self.recorder {
            recorder.exit(self.start.elapsed(), status);
        }
        let mut out = Vec::new();
        if self.host_mouse {
            out.extend_from_slice(mouse::HOST_OFF);
        }
        hook!(self, self.now(), |ctx| engine.on_exit(&mut out, ctx));
        let _ = self.host.write(&out);
        self.emit("exit", json!({"status": status}));
        let pending: Vec<(ConnId, Value)> = self.pending.drain().map(|(_, v)| v).collect();
        for (conn, id) in pending {
            self.respond(
                conn,
                &id,
                Err(RpcError::new(ErrorCode::Gone, "the app exited")),
            );
        }
        for conn in self.conns.values_mut() {
            conn.drain_blocking(Duration::from_millis(200));
        }
        self.conns.clear();
        let _ = fs::remove_file(&self.info.socket);
        crate::host::restore();
        status
    }
}

/// What `wait` found ready.
#[derive(Default)]
struct Ready {
    pty_in: bool,
    pty_out: bool,
    stdin: bool,
    signal: bool,
    listener: bool,
    conns_in: Vec<ConnId>,
    conns_out: Vec<ConnId>,
}
