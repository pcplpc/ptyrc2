// SPDX-License-Identifier: GPL-3.0-or-later
//! The event loop: app pty, host terminal, signals, clients, one timer.

use crate::conn::Conn;
use crate::engine::{ConnId, Ctx, Effects, Engine, Reply, SessionInfo, Token};
use crate::host::Host;
use crate::hostcolors::{Known, QUERY_BG, QUERY_FG, Replies};
use crate::keys;
use crate::pty::{self, Pty};
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
    pty: Pty,
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
    next_conn: u64,
    next_token: u64,
    repaint: bool,
    exit_status: Option<i32>,
    colors: Known,
    replies: Replies,
    /// When to stop waiting for the host's colours.
    replies_until: Option<Duration>,
}

/// Run a session to its end: spawn the app, serve, return its exit status
/// (128 + signal when killed).
pub fn run(cfg: Config, mut engine: Box<dyn Engine>) -> Result<i32, RunError> {
    let mut driver = Driver::start(cfg)?;
    let result = driver.serve(engine.as_mut());
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
            let pty = pty::spawn(&cfg.argv, &env, cols, rows)
                .map_err(|e| start::exec_error(program, &e))?;
            Ok::<_, RunError>((signals, host, pty, cols, rows))
        })();
        let (signals, host, pty, cols, rows) = match built {
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
            pty,
            pty_eof: false,
            pty_queue: Vec::new(),
            host,
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
        })
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
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
    fn serve(&mut self, engine: &mut dyn Engine) -> io::Result<i32> {
        hook!(self, self.now(), |ctx| engine.on_start(ctx));
        let colors = self.colors.colors;
        hook!(self, self.now(), |ctx| engine.on_host_colors(colors, ctx));
        self.ask_host_colors()?;
        loop {
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
            } else {
                let wake = engine.next_wake(now);
                let wake = match (wake, self.replies_until) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                wake.map(|w| w.saturating_sub(now))
            };
            let ready = self.wait(timeout)?;

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
        if !self.pty_eof {
            let mut flags = PollFlags::IN;
            if !self.pty_queue.is_empty() {
                flags |= PollFlags::OUT;
            }
            srcs.push(Src::Pty);
            fds.push(PollFd::new(&self.pty.master, flags));
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
        while burst.len() < BURST_MAX_BYTES {
            match rustix::io::read(&self.pty.master, &mut buf) {
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
                    let mut fds = [PollFd::new(&self.pty.master, PollFlags::IN)];
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
        let mut out = Vec::with_capacity(burst.len() + 256);
        hook!(self, now, |ctx| engine.on_burst(&burst, now, &mut out, ctx));
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
                    hook!(self, self.now(), |ctx| {
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

    /// Keyboard bytes on their way to the app.
    fn forward_input(&mut self, bytes: &[u8], engine: &mut dyn Engine) {
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
        while sent < self.pty_queue.len() {
            match rustix::io::write(&self.pty.master, &self.pty_queue[sent..]) {
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
        self.pty_queue.drain(..sent);
    }

    fn reap(&mut self) -> io::Result<()> {
        if self.exit_status.is_none()
            && let Some(status) = self.pty.child.try_wait()?
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
        if let Some(pid) = Pid::from_raw(self.info.pid as i32) {
            let _ = kill_process_group(pid, Signal::HUP);
            let deadline = Instant::now() + Duration::from_millis(300);
            while Instant::now() < deadline && matches!(self.pty.child.try_wait(), Ok(None)) {
                std::thread::sleep(Duration::from_millis(10));
            }
            if matches!(self.pty.child.try_wait(), Ok(None)) {
                let _ = kill_process_group(pid, Signal::KILL);
                let _ = self.pty.child.wait();
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
        // Also signals SIGWINCH to the app.
        pty::set_winsize(self.pty.master.as_fd(), cols, rows)?;
        let now = self.now();
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
                Some(result) => self.respond(id, &request_id, result),
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
            "subscribe" => self.subscribe(conn, &params),
            "input.send" => self.input_send(&params, engine),
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
        if let Value::Object(extras) = engine.session_extras() {
            object.extend(extras);
        }
        info
    }

    fn session_resize(
        &mut self,
        params: &Value,
        engine: &mut dyn Engine,
    ) -> Result<Value, RpcError> {
        if !self.info.headless {
            return Err(RpcError::new(
                ErrorCode::BadRequest,
                "the host terminal sets the size",
            ));
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

    fn subscribe(&mut self, conn: ConnId, params: &Value) -> Result<Value, RpcError> {
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
        if let Some(conn) = self.conns.get_mut(&conn) {
            conn.topics.extend(topics);
        }
        self.refresh_topics();
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
        if self.exit_status.is_some() {
            return Err(RpcError::new(ErrorCode::Gone, "the app exited"));
        }
        self.pty_queue.extend(bytes);
        Ok(json!({}))
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
            self.pending.retain(|_, (conn, _)| *conn != id);
            self.refresh_topics();
            hook!(self, self.now(), |ctx| engine.on_disconnect(id, ctx));
        }
    }

    /// The end: clean the host, tell the clients, remove the socket.
    /// `status`: the app's exit status, or `None` after a driver error.
    fn finish(&mut self, engine: &mut dyn Engine, status: Option<i32>) -> i32 {
        let status = status.unwrap_or(1);
        let mut out = Vec::new();
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
