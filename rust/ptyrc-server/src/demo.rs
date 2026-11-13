// SPDX-License-Identifier: GPL-3.0-or-later
//! The demo: ctrl+t twice on the host keyboard runs a client app on the
//! session; twice again replaces it by the next.

use crate::grab;
use rustix::process::{Pid, Signal, kill_process_group};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The second ctrl+t comes within this of the first.
const WINDOW: Duration = Duration::from_millis(350);
const KEY: &[u8] = b"\x14";

pub(crate) struct Demo {
    /// Run with two more arguments: a seed, the same for the session,
    /// and the number of apps started before.
    command: Vec<String>,
    env: Vec<(String, String)>,
    seed: u64,
    started: u64,
    child: Option<Child>,
    /// Killed, not yet reaped.
    dying: Vec<Child>,
    /// A first ctrl+t, as typed, and when it stops waiting.
    held: Option<(Vec<u8>, Duration)>,
}

impl Demo {
    pub fn new(command: Vec<String>, env: Vec<(String, String)>, seed: u64) -> Demo {
        Demo {
            command,
            env,
            seed,
            started: 0,
            child: None,
            dying: Vec::new(),
            held: None,
        }
    }

    /// An app of the demo is running.
    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    /// When the held key is due.
    pub fn due(&self) -> Option<Duration> {
        self.held.as_ref().map(|(_, at)| *at)
    }

    /// The held key, given up.
    pub fn flush(&mut self) -> Vec<u8> {
        self.held.take().map(|(key, _)| key).unwrap_or_default()
    }

    /// Sort keyboard bytes: what goes on, and whether ctrl+t came twice
    /// in a row. A first ctrl+t is held, until the next key or `due`;
    /// the pair goes nowhere. Not `armed`: nothing is held.
    pub fn sort(&mut self, bytes: &[u8], now: Duration, armed: bool) -> (Vec<u8>, bool) {
        let mut forward = self.flush();
        if !armed {
            forward.extend_from_slice(bytes);
            return (forward, false);
        }
        let mut held = (!forward.is_empty()).then(|| std::mem::take(&mut forward));
        let mut twice = false;
        let mut rest = bytes;
        while !rest.is_empty() {
            let (key, tail) = rest.split_at(grab::key_len(rest));
            rest = tail;
            let is_key = grab::legacy(key).as_deref().unwrap_or(key) == KEY;
            match (held.take(), is_key) {
                (Some(_), true) => twice = true,
                (None, true) => held = Some(key.to_vec()),
                // The release of the first press, between the two.
                (Some(first), false) if grab::is_release(key) => {
                    forward.extend_from_slice(key);
                    held = Some(first);
                }
                (first, false) => {
                    forward.extend(first.unwrap_or_default());
                    forward.extend_from_slice(key);
                }
            }
        }
        self.held = held.map(|key| (key, now + WINDOW));
        (forward, twice)
    }

    /// Replace the running app by the next. False: it did not start.
    pub fn next(&mut self) -> bool {
        self.stop();
        let extra = [self.seed.to_string(), self.started.to_string()];
        self.started += 1;
        self.child = spawn(&self.command, &extra, &self.env);
        self.child.is_some()
    }

    /// Terminate the running app: its sprites go with its connection.
    pub fn stop(&mut self) {
        if let Some(child) = self.child.take() {
            terminate(&child);
            self.dying.push(child);
        }
        self.reap();
    }

    /// After SIGCHLD: forget the app that ended.
    pub fn reap(&mut self) {
        self.child
            .take_if(|child| !matches!(child.try_wait(), Ok(None)));
        self.dying
            .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
}

/// `command` and `extra`, in its own process group, stdio null.
pub(crate) fn spawn(
    command: &[String],
    extra: &[String],
    env: &[(String, String)],
) -> Option<Child> {
    let (program, args) = command.split_first()?;
    Command::new(program)
        .args(args)
        .args(extra)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()
}

/// SIGTERM to the process group of `child`.
pub(crate) fn terminate(child: &Child) {
    if let Some(group) = Pid::from_raw(child.id() as i32) {
        let _ = kill_process_group(group, Signal::TERM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Demo {
        Demo::new(Vec::new(), Vec::new(), 0)
    }

    const T0: Duration = Duration::ZERO;

    #[test]
    fn twice_in_a_row() {
        let mut d = demo();
        assert_eq!(d.sort(b"a\x14\x14b", T0, true), (b"ab".to_vec(), true));
        assert_eq!(d.due(), None);
        // Over two reads.
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), false));
        assert_eq!(d.due(), Some(WINDOW));
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), true));
        assert_eq!(d.due(), None);
        // Three: a pair, and one held.
        assert_eq!(d.sort(b"\x14\x14\x14", T0, true), (Vec::new(), true));
        assert_eq!(d.flush(), b"\x14");
    }

    #[test]
    fn once_goes_on() {
        let mut d = demo();
        // Another key between the two.
        assert_eq!(d.sort(b"\x14a\x14", T0, true), (b"\x14a".to_vec(), false));
        assert_eq!(d.sort(b"x", T0, true), (b"\x14x".to_vec(), false));
        // Too late: given up as typed.
        assert_eq!(d.sort(b"\x1b[116;5u", T0, true), (Vec::new(), false));
        assert_eq!(d.flush(), b"\x1b[116;5u");
        assert_eq!(d.flush(), b"");
        // Not armed: untouched, and what was held goes first.
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), false));
        assert_eq!(
            d.sort(b"\x14\x14", T0, false),
            (b"\x14\x14\x14".to_vec(), false)
        );
    }

    #[test]
    fn kitty_keys() {
        let mut d = demo();
        let (forward, twice) = d.sort(b"\x1b[116;5u\x1b[116;5:3u\x1b[116;5u", T0, true);
        assert_eq!((&forward[..], twice), (&b"\x1b[116;5:3u"[..], true));
    }

    #[test]
    fn apps_follow_each_other() {
        let dir = std::env::temp_dir().join(format!("ptyrc-demo-{}", std::process::id()));
        let log = dir.join("log");
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!("echo \"$0 $1\" >> {}; exec sleep 60", log.display());
        let mut d = Demo::new(vec!["sh".into(), "-c".into(), script], Vec::new(), 7);
        let wait = |done: &mut dyn FnMut() -> bool| {
            for _ in 0..1000 {
                if done() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("timed out");
        };
        let logged = |text: &str| std::fs::read_to_string(&log).is_ok_and(|l| l == text);
        assert!(!d.running());
        assert!(d.next() && d.running());
        let first = d.child.as_ref().unwrap().id();
        wait(&mut || logged("7 0\n"));
        assert!(d.next());
        assert_ne!(d.child.as_ref().unwrap().id(), first);
        wait(&mut || logged("7 0\n7 1\n"));
        // The first was terminated; so is the second.
        d.stop();
        assert!(!d.running());
        wait(&mut || {
            d.reap();
            d.dying.is_empty()
        });
        std::fs::remove_dir_all(&dir).unwrap();
        // A command that is not there.
        let mut d = Demo::new(vec!["/no/such/program".into()], Vec::new(), 0);
        assert!(!d.next() && !d.running());
    }
}
