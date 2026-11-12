// SPDX-License-Identifier: GPL-3.0-or-later
//! The clock of a replay: tape time, played, paused, or run to a mark.

use crate::engine::Token;
use crate::record::{Record, Tape};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How to replay a tape.
#[derive(Clone, Debug)]
pub struct Replay {
    pub tape: Tape,
    /// Tape seconds per second; 0: no waiting.
    pub speed: f64,
    /// Pause when the tape clock reaches this.
    pub until: Option<Duration>,
    /// Start paused.
    pub paused: bool,
    /// At the end of the tape: pause, in place of ending the session.
    pub hold: bool,
}

/// What the player wants done next.
pub(crate) enum Step {
    /// Feed this record now.
    Feed(Record),
    /// The mark is reached: the clock stopped there.
    Paused,
    /// The tape is over and nothing holds the session.
    Done,
    /// Nothing before `Player::due`.
    Wait,
}

pub(crate) struct Player {
    tape: Tape,
    next: usize,
    /// Tape time at `anchor`.
    base: Duration,
    anchor: Instant,
    speed: f64,
    paused: bool,
    /// Where to pause.
    mark: Option<Duration>,
    /// The mark is run to without waiting, whatever the speed.
    rush: bool,
    hold: bool,
    /// Status given by the tape's exit record.
    pub status: i32,
    /// Requests answered at the next pause.
    pub waiters: Vec<Token>,
}

impl Player {
    pub fn new(replay: Replay) -> Player {
        Player {
            tape: replay.tape,
            next: 0,
            base: Duration::ZERO,
            anchor: Instant::now(),
            speed: replay.speed.max(0.0),
            paused: replay.paused,
            mark: replay.until,
            rush: false,
            hold: replay.hold,
            status: 0,
            waiters: Vec::new(),
        }
    }

    fn fast(&self) -> bool {
        self.rush || self.speed == 0.0
    }

    fn next_time(&self) -> Option<Duration> {
        self.tape.records.get(self.next).map(|(t, _)| *t)
    }

    /// Tape time.
    pub fn now(&self) -> Duration {
        if self.paused || self.fast() {
            return self.base;
        }
        let now = self.base + self.anchor.elapsed().mul_f64(self.speed);
        self.mark.map_or(now, |mark| now.min(mark))
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    fn stop(&mut self, at: Duration) {
        self.base = at;
        self.paused = true;
        self.mark = None;
        self.rush = false;
    }

    /// The next thing to do at the present tape time.
    pub fn step(&mut self) -> Step {
        if self.paused {
            return Step::Wait;
        }
        let now = self.now();
        let limit = self.mark.unwrap_or(Duration::MAX);
        match self.next_time() {
            Some(t) if t <= now && t <= limit => {
                // The record is fed at its own time.
                if self.fast() {
                    self.base = t;
                }
                let record = self.tape.records[self.next].1.clone();
                self.next += 1;
                if let Record::Exit(status) = record {
                    self.status = status;
                }
                Step::Feed(record)
            }
            _ if self.mark.is_some_and(|mark| now >= mark) => {
                self.stop(now);
                Step::Paused
            }
            None if self.mark.is_none() && self.hold => {
                self.stop(now);
                Step::Paused
            }
            None if self.mark.is_none() => Step::Done,
            _ => Step::Wait,
        }
    }

    /// Tape time of the next record or of the mark.
    pub fn due(&self) -> Option<Duration> {
        if self.paused {
            return None;
        }
        match (self.next_time(), self.mark) {
            (Some(t), Some(mark)) => Some(t.min(mark)),
            (t, mark) => t.or(mark),
        }
    }

    /// How long to sleep until tape time `at`. `None`: do not sleep, the
    /// clock was moved there.
    pub fn sleep_until(&mut self, at: Duration) -> Option<Duration> {
        if self.fast() {
            self.base = self.base.max(at);
            return None;
        }
        Some(at.saturating_sub(self.now()).div_f64(self.speed))
    }

    /// Run to tape time `to` without waiting, then pause.
    pub fn seek(&mut self, to: Duration) -> Result<(), String> {
        let now = self.now();
        if to < now {
            return Err(format!(
                "a tape goes forward: it is at {:.3} s",
                now.as_secs_f64()
            ));
        }
        self.base = now;
        self.paused = false;
        self.mark = Some(to);
        self.rush = true;
        Ok(())
    }

    /// Run through the next `records` records, then pause.
    pub fn step_records(&mut self, records: usize) -> Result<(), String> {
        let last = (self.next + records.max(1) - 1).min(self.tape.records.len().saturating_sub(1));
        match self.tape.records.get(last) {
            Some((t, _)) if self.next < self.tape.records.len() => self.seek(*t),
            _ => Err("the tape is over".into()),
        }
    }

    pub fn play(&mut self, speed: Option<f64>) {
        self.base = self.now();
        self.anchor = Instant::now();
        if let Some(speed) = speed {
            self.speed = speed.max(0.0);
        }
        self.paused = false;
        self.mark = None;
        self.rush = false;
    }

    pub fn pause(&mut self) {
        let now = self.now();
        self.stop(now);
    }

    pub fn status_json(&self) -> Value {
        let seconds = |d: Duration| (d.as_secs_f64() * 1e6).round() / 1e6;
        json!({
            "t": seconds(self.now()),
            "end": seconds(self.tape.end()),
            "records": self.tape.records.len(),
            "next": self.next,
            "paused": self.paused,
            "speed": self.speed,
            "done": self.next >= self.tape.records.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tape() -> Tape {
        let at = |ms: u64, text: &str| {
            (
                Duration::from_millis(ms),
                Record::Output(text.as_bytes().to_vec()),
            )
        };
        Tape {
            size: (80, 24),
            argv: vec![],
            records: vec![at(10, "a"), at(20, "b"), at(500, "c")],
        }
    }

    fn replay(speed: f64, until: Option<u64>, paused: bool, hold: bool) -> Player {
        Player::new(Replay {
            tape: tape(),
            speed,
            until: until.map(Duration::from_millis),
            paused,
            hold,
        })
    }

    /// Run until the player waits on nothing; what was fed, and how it
    /// ended.
    fn run(player: &mut Player) -> (String, bool) {
        let mut fed = String::new();
        loop {
            match player.step() {
                Step::Feed(Record::Output(bytes)) => fed.push_str(&String::from_utf8_lossy(&bytes)),
                Step::Feed(_) => {}
                Step::Paused => return (fed, false),
                Step::Done => return (fed, true),
                Step::Wait => match player.due() {
                    Some(at) => assert!(player.sleep_until(at).is_none()),
                    None => return (fed, false),
                },
            }
        }
    }

    #[test]
    fn fast_to_the_end() {
        let mut player = replay(0.0, None, false, false);
        assert_eq!(run(&mut player), ("abc".into(), true));
        assert_eq!(player.now(), Duration::from_millis(500));
    }

    #[test]
    fn until_pauses_and_steps_go_on() {
        let mut player = replay(0.0, Some(100), false, false);
        assert_eq!(run(&mut player), ("ab".into(), false));
        assert!(player.paused());
        assert_eq!(player.now(), Duration::from_millis(100));
        assert!(player.seek(Duration::from_millis(50)).is_err());
        player.step_records(1).unwrap();
        assert_eq!(run(&mut player), ("c".into(), false));
        assert_eq!(player.now(), Duration::from_millis(500));
        assert!(player.step_records(1).is_err());
        // Past the end: time passes, nothing is fed.
        player.seek(Duration::from_secs(3)).unwrap();
        assert_eq!(run(&mut player), (String::new(), false));
        assert_eq!(player.now(), Duration::from_secs(3));
    }

    #[test]
    fn step_from_a_paused_start_and_hold() {
        let mut player = replay(1.0, None, true, true);
        assert_eq!(run(&mut player), (String::new(), false));
        player.step_records(2).unwrap();
        assert_eq!(run(&mut player), ("ab".into(), false));
        assert_eq!(player.status_json()["next"], 2);
        player.play(Some(0.0));
        assert_eq!(run(&mut player), ("c".into(), false));
        assert!(player.paused());
    }

    #[test]
    fn real_time_sleeps_scaled() {
        let mut player = replay(2.0, None, false, false);
        let sleep = player.sleep_until(Duration::from_millis(400)).unwrap();
        assert!(sleep <= Duration::from_millis(200) && sleep > Duration::from_millis(150));
    }
}
