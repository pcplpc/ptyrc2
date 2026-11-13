// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::manage::catalog;
use crate::manage::menu::tests::{frames, info};
use crate::manage::menu::text;

/// A table kept in memory; what was asked of it.
#[derive(Default)]
struct Fake {
    apps: Vec<AppInfo>,
    asked: Vec<String>,
    lit: Vec<Option<u64>>,
    /// The next start ends at once with this on stderr.
    broken: Option<String>,
    logs: Vec<(u64, String)>,
}

impl Fake {
    fn set(&mut self, id: u64, state: State) -> Result<(), Fail> {
        match self.apps.iter_mut().find(|a| a.id == id) {
            Some(app) => {
                app.state = state;
                Ok(())
            }
            None => Err(Fail {
                status: 4,
                message: format!("no-sprite: no such app: {id}"),
                hint: Some("apps: 1".into()),
            }),
        }
    }
}

impl Backend for Fake {
    fn apps(&mut self) -> Result<Vec<AppInfo>, Fail> {
        Ok(self.apps.clone())
    }

    fn frames(&mut self) -> Result<Vec<FrameInfo>, Fail> {
        Ok(frames())
    }

    fn start(&mut self, name: &str, args: &[String]) -> Result<u64, Fail> {
        let id = self.apps.iter().map(|a| a.id).max().unwrap_or(0) + 1;
        let words = format!("{name} {}", args.join(" "));
        self.asked.push(format!("start {}", words.trim()));
        let mut app = info(id, &words, "running");
        if let Some(why) = self.broken.take() {
            app.state = State::Exited;
            app.status = Some(2);
            self.logs.push((id, why));
        }
        self.apps.push(app);
        Ok(id)
    }

    fn stop(&mut self, id: u64) -> Result<(), Fail> {
        self.asked.push(format!("stop {id}"));
        self.set(id, State::Exited)
    }

    fn pause(&mut self, id: u64) -> Result<(), Fail> {
        self.asked.push(format!("pause {id}"));
        self.set(id, State::Paused)
    }

    fn resume(&mut self, id: u64) -> Result<(), Fail> {
        self.asked.push(format!("resume {id}"));
        self.set(id, State::Running)
    }

    fn restart(&mut self, id: u64, argv: Option<&[String]>) -> Result<(), Fail> {
        let argv = argv.map(|a| a.join(" ")).unwrap_or_default();
        self.asked
            .push(format!("restart {id} {argv}").trim().to_string());
        self.set(id, State::Running)
    }

    fn forget(&mut self, id: u64) -> Result<(), Fail> {
        self.asked.push(format!("forget {id}"));
        self.apps.retain(|a| a.id != id);
        Ok(())
    }

    fn move_to(&mut self, id: u64, frame: &str) -> Result<(), Fail> {
        self.asked.push(format!("move {id} {frame}"));
        Ok(())
    }

    fn log(&mut self, app: &AppInfo) -> Vec<String> {
        let log = self.logs.iter().find(|l| l.0 == app.id);
        log.map_or_else(Vec::new, |l| l.1.lines().map(str::to_string).collect())
    }

    fn highlight(&mut self, frame: Option<u64>) {
        self.lit.push(frame);
    }
}

fn runner() -> Runner<Fake> {
    let mut menu = Menu::new("s", 3);
    menu.catalog(Some(catalog::sample()));
    let fake = Fake {
        apps: vec![info(1, "bricks -a", "running")],
        ..Fake::default()
    };
    Runner::new(menu, fake)
}

fn keys(runner: &mut Runner<Fake>, keys: &str, now: Instant) -> Flow {
    let mut flow = Flow::Stay;
    for key in keys.split_whitespace() {
        flow = runner.key(key, now);
    }
    flow
}

fn screen(runner: &Runner<Fake>) -> String {
    text(&runner.menu.render(64, 18))
}

#[test]
fn keys_reach_the_session() {
    let mut r = runner();
    let t0 = Instant::now();
    assert!(screen(&r).contains("● bricks -a"));
    assert_eq!(keys(&mut r, "p", t0), Flow::Stay);
    // The list is read again after each thing done: no event needed.
    assert!(screen(&r).contains("‖ bricks -a"), "{}", screen(&r));
    keys(&mut r, "p x x", t0);
    assert_eq!(
        r.backend.asked,
        ["pause 1", "resume 1", "stop 1", "forget 1"]
    );
    assert!(!screen(&r).contains("bricks"));
    // Enter twice starts with the defaults.
    keys(&mut r, "s down enter enter", t0);
    assert_eq!(r.backend.asked.last().unwrap(), "start clock");
    assert!(screen(&r).contains("● clock"), "{}", screen(&r));
    assert_eq!(keys(&mut r, "q", t0), Flow::Quit);
}

#[test]
fn an_app_that_ends_at_once_says_why() {
    let mut r = runner();
    let t0 = Instant::now();
    r.backend.broken = Some("usage: clock\nclock: error: bad zone 'Mars'\n\n".into());
    keys(&mut r, "s down enter enter", t0);
    assert_eq!(r.deadline(), Some(t0 + EARLY));
    assert!(!r.tick(t0 + EARLY / 2));
    assert!(!screen(&r).contains("bad zone"));
    assert!(r.tick(t0 + EARLY));
    assert_eq!(r.deadline(), None);
    assert!(
        screen(&r).contains("clock: clock: error: bad zone 'Mars'"),
        "{}",
        screen(&r)
    );
    // One that runs says nothing; one that wrote nothing has its status.
    keys(&mut r, "s enter enter", t0);
    r.backend.broken = Some(String::new());
    keys(&mut r, "s enter enter", t0);
    assert!(r.tick(t0 + EARLY));
    assert!(
        screen(&r).contains("confetti: ended at once, status 2"),
        "{}",
        screen(&r)
    );
}

#[test]
fn a_refusal_is_shown() {
    let mut r = runner();
    r.backend.apps.clear();
    keys(&mut r, "x", Instant::now());
    assert!(
        screen(&r).contains("no-sprite: no such app: 1: apps: 1"),
        "{}",
        screen(&r)
    );
    assert!(!screen(&r).contains("bricks"));
}

#[test]
fn events_and_frames() {
    let mut r = runner();
    let t0 = Instant::now();
    let gone = serde_json::json!({"id": 1, "state": "gone"});
    r.event("apps", &gone);
    assert!(!screen(&r).contains("bricks"));
    // What is no info: the table is read again.
    r.event("apps", &serde_json::Value::Null);
    assert!(screen(&r).contains("bricks"));
    // The frame under the selection is shown on the screen, then not.
    keys(&mut r, "f down", t0);
    r.event("frames", &serde_json::Value::Null);
    keys(&mut r, "esc", t0);
    assert_eq!(r.backend.lit, [Some(0), Some(2), None]);
    // Moved live when it says it moves; else restarted with the frame.
    keys(&mut r, "m end enter", t0);
    r.backend.apps[0].moves = true;
    r.event("apps", &serde_json::Value::Null);
    keys(&mut r, "m home enter", t0);
    let asked = &r.backend.asked;
    assert_eq!(asked[0], "restart 1 /usr/bin/ptyrc-app bricks -f idle -a");
    assert_eq!(asked[1], "move 1 0");
}
