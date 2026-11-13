// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu at work: what it asks is done on the session, what the
//! session tells is shown. Shared by the two fronts.

use super::catalog::{self, Catalog};
use super::menu::{Action, AppInfo, FrameInfo, Menu, State};
use super::{overlay, tty};
use crate::{Common, Fail};
use ptyrc_proto::{Client, ClientError, SelectError};
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// An app that ends this soon after its start did not start.
pub const EARLY: Duration = super::START_WAIT;
/// Lines of a log shown.
const LOG_LINES: usize = 200;

/// What the menu needs of a session.
pub trait Backend {
    fn apps(&mut self) -> Result<Vec<AppInfo>, Fail>;
    fn frames(&mut self) -> Result<Vec<FrameInfo>, Fail>;
    /// An app of the catalog. Its id.
    fn start(&mut self, name: &str, args: &[String]) -> Result<u64, Fail>;
    fn stop(&mut self, id: u64) -> Result<(), Fail>;
    fn pause(&mut self, id: u64) -> Result<(), Fail>;
    fn resume(&mut self, id: u64) -> Result<(), Fail>;
    fn restart(&mut self, id: u64, argv: Option<&[String]>) -> Result<(), Fail>;
    fn forget(&mut self, id: u64) -> Result<(), Fail>;
    /// `app.move`: the app moves what it drew.
    fn move_to(&mut self, id: u64, frame: &str) -> Result<(), Fail>;
    /// The last lines an app wrote on stderr.
    fn log(&mut self, app: &AppInfo) -> Vec<String>;
    /// Show a frame on the screen itself; `None`: no more.
    fn highlight(&mut self, _frame: Option<u64>) {}
}

/// What a key leaves the front to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Flow {
    Stay,
    Quit,
    Session(String),
}

/// A menu and its session.
pub struct Runner<B: Backend> {
    pub menu: Menu,
    pub backend: B,
    /// Apps just started, and when they are looked at.
    watch: Vec<(u64, Instant)>,
}

impl<B: Backend> Runner<B> {
    pub fn new(mut menu: Menu, backend: B) -> Runner<B> {
        menu.note("", false);
        let mut runner = Runner {
            menu,
            backend,
            watch: Vec::new(),
        };
        runner.refresh();
        runner.reframe();
        runner
    }

    /// Read the table again.
    fn refresh(&mut self) {
        match self.backend.apps() {
            Ok(apps) => self.menu.apps(apps),
            Err(fail) => self.menu.note(&fail.message, true),
        }
    }

    /// Read the frames again.
    fn reframe(&mut self) {
        if let Ok(frames) = self.backend.frames()
            && let Some(Action::Highlight(frame)) = self.menu.frames(frames)
        {
            self.backend.highlight(frame);
        }
    }

    /// A key of the user.
    pub fn key(&mut self, key: &str, now: Instant) -> Flow {
        let mut flow = Flow::Stay;
        for action in self.menu.key(key) {
            let done = match &action {
                Action::Quit => {
                    flow = Flow::Quit;
                    continue;
                }
                Action::Session(name) => {
                    flow = Flow::Session(name.clone());
                    continue;
                }
                Action::Highlight(frame) => {
                    self.backend.highlight(*frame);
                    continue;
                }
                Action::Log(id) => {
                    if let Some(app) = self.menu.info(*id).cloned() {
                        let lines = self.backend.log(&app);
                        self.menu.log(*id, lines);
                    }
                    continue;
                }
                Action::Start { name, args } => self.backend.start(name, args).map(Some),
                Action::Restart { id, argv } => {
                    (self.backend.restart(*id, argv.as_deref())).map(|()| Some(*id))
                }
                Action::Stop(id) => self.backend.stop(*id).map(|()| None),
                Action::Pause(id) => self.backend.pause(*id).map(|()| None),
                Action::Resume(id) => self.backend.resume(*id).map(|()| None),
                Action::Forget(id) => self.backend.forget(*id).map(|()| None),
                Action::Move { id, frame } => self.backend.move_to(*id, frame).map(|()| None),
            };
            match done {
                Ok(started) => {
                    self.watch.extend(started.map(|id| (id, now + EARLY)));
                    self.refresh();
                    if let (Action::Start { .. }, Some(id)) = (&action, started) {
                        self.menu.select(id);
                    }
                }
                Err(fail) => {
                    // Refused: the table shown may be an old one.
                    self.refresh();
                    let hint = fail.hint.filter(|h| !h.is_empty());
                    let text = match hint {
                        Some(hint) => format!("{}: {hint}", fail.message),
                        None => fail.message,
                    };
                    self.menu.note(&text, true);
                }
            }
        }
        flow
    }

    /// An event of the session: `apps`, `frames`.
    pub fn event(&mut self, topic: &str, data: &Value) {
        match topic {
            "apps" => match AppInfo::from_json(data) {
                Some(info) => self.menu.app(info),
                None => self.refresh(),
            },
            "frames" => self.reframe(),
            _ => {}
        }
    }

    /// A second passed: the table is read again. The times shown
    /// follow, and what an app draws on, which changes without an event.
    pub fn beat(&mut self) {
        self.refresh();
    }

    /// When `tick` has something to do.
    pub fn deadline(&self) -> Option<Instant> {
        self.watch.iter().map(|w| w.1).min()
    }

    /// Look at the apps started `EARLY` ago: one that ended with an
    /// error says why. True when something is to draw.
    pub fn tick(&mut self, now: Instant) -> bool {
        let due: Vec<u64> = (self.watch.iter())
            .filter(|w| w.1 <= now)
            .map(|w| w.0)
            .collect();
        if due.is_empty() {
            return false;
        }
        self.watch.retain(|w| w.1 > now);
        self.refresh();
        for id in due {
            let Some(app) = self.menu.info(id).cloned() else {
                continue;
            };
            if app.state == State::Exited && app.status != Some(0) {
                let log = self.backend.log(&app);
                let last = log.iter().rev().find(|l| !l.trim().is_empty());
                let why = match (last, app.status) {
                    (Some(line), _) => line.trim().to_string(),
                    (None, Some(status)) => format!("ended at once, status {status}"),
                    (None, None) => "ended at once".to_string(),
                };
                self.menu.note(&format!("{}: {why}", app.label()), true);
            }
        }
        true
    }
}

/// A session of a driver, as the menu's backend.
pub struct Session {
    pub client: Client,
    /// The frames last read: a highlight is of the size of its view.
    frames: Vec<FrameInfo>,
    /// Highlights are drawn (the overlay); the sprite of the one shown.
    pub lights: bool,
    light: Option<u64>,
}

impl Session {
    pub fn new(client: Client) -> (Session, Option<Catalog>) {
        let catalog = catalog::load();
        let session = Session {
            client,
            frames: Vec::new(),
            lights: false,
            light: None,
        };
        (session, catalog)
    }

    pub fn name(&self) -> String {
        self.client.hello()["session"].as_str().unwrap_or("").into()
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, Fail> {
        Ok(self.client.call(method, params)?)
    }

    fn on(&mut self, method: &str, id: u64) -> Result<(), Fail> {
        self.call(method, json!({"app": id})).map(|_| ())
    }
}

impl Backend for Session {
    fn apps(&mut self) -> Result<Vec<AppInfo>, Fail> {
        let apps = super::list(&mut self.client)?;
        Ok(apps.iter().filter_map(AppInfo::from_json).collect())
    }

    fn frames(&mut self) -> Result<Vec<FrameInfo>, Fail> {
        let list = self.call("frame.list", json!({}))?;
        let frames = list["frames"].as_array().map_or(&[][..], Vec::as_slice);
        self.frames = frames.iter().filter_map(FrameInfo::from_json).collect();
        Ok(self.frames.clone())
    }

    fn start(&mut self, name: &str, args: &[String]) -> Result<u64, Fail> {
        let mut words = vec![name.to_string()];
        words.extend_from_slice(args);
        let (argv, name) = super::app_argv(&words)?;
        let started = self.call("app.start", json!({"argv": argv, "name": name}))?;
        started["app"]["id"]
            .as_u64()
            .ok_or_else(|| "app.start: no id".into())
    }

    fn stop(&mut self, id: u64) -> Result<(), Fail> {
        self.on("app.stop", id)
    }

    fn pause(&mut self, id: u64) -> Result<(), Fail> {
        self.on("app.pause", id)
    }

    fn resume(&mut self, id: u64) -> Result<(), Fail> {
        self.on("app.resume", id)
    }

    fn restart(&mut self, id: u64, argv: Option<&[String]>) -> Result<(), Fail> {
        let params = match argv {
            Some(argv) => json!({"app": id, "argv": argv}),
            None => json!({"app": id}),
        };
        self.call("app.restart", params).map(|_| ())
    }

    fn forget(&mut self, id: u64) -> Result<(), Fail> {
        self.on("app.forget", id)
    }

    fn move_to(&mut self, id: u64, frame: &str) -> Result<(), Fail> {
        let frame = frame
            .parse::<u64>()
            .map_or_else(|_| json!(frame), |n| json!(n));
        self.call("app.move", json!({"app": id, "frame": frame}))
            .map(|_| ())
    }

    fn log(&mut self, app: &AppInfo) -> Vec<String> {
        let text = (app.log.as_ref()).and_then(|path| std::fs::read(path).ok());
        let text = String::from_utf8_lossy(&text.unwrap_or_default()).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        let from = lines.len().saturating_sub(LOG_LINES);
        lines[from..].iter().map(|l| l.to_string()).collect()
    }

    /// A front tint over the view of the frame. Errors are not told:
    /// the frame may have gone since it was listed.
    fn highlight(&mut self, frame: Option<u64>) {
        if !self.lights {
            return;
        }
        if let Some(sprite) = self.light.take() {
            let _ = self.client.call("draw.remove", json!({"id": sprite}));
        }
        let Some(frame) = frame.and_then(|id| self.frames.iter().find(|f| f.id == id)) else {
            return;
        };
        let [_, _, w, h] = frame.rect;
        let put = json!({
            "frame": frame.id,
            "anchor": {"at": [0, 0], "space": "view"},
            "content": {"rows": vec![" ".repeat(w); h], "style": {"bg": "yellow"}, "opaque": true},
            "blend": "tint", "layer": "front", "alpha": 0.3,
        });
        let put = self.client.call("draw.put", put).ok();
        self.light = put.and_then(|p| p["id"].as_u64());
    }
}

/// The menu: in a modal widget of the session (`overlay`), else on this
/// terminal.
pub fn interactive(common: &Common, overlay: bool) -> Result<ExitCode, Fail> {
    if overlay {
        let client = Client::connect_session(common.session.as_deref())?;
        return overlay::run(client);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Fail::usage(
            "the menu needs a terminal",
            "the apps: ptyrc manage ls; `ptyrc manage --help` lists the verbs",
        ));
    }
    let mut term = tty::Term::open()?;
    let client = match Client::connect_session(common.session.as_deref()) {
        Ok(client) => client,
        // Several sessions and none named: asked.
        Err(ClientError::Select(SelectError::Ambiguous(names))) => match term.pick(names) {
            Some(name) => Client::connect_session(Some(&name))?,
            None => return Ok(ExitCode::SUCCESS),
        },
        Err(e) => return Err(e.into()),
    };
    term.run(client)
}

#[cfg(test)]
mod tests;
