// SPDX-License-Identifier: GPL-3.0-or-later
//! A guest and its limits of time: late, paused, ended.

use crate::core::{Core, Host, Limits};
use crate::engine::Guest;
use crate::error::Error;
use serde_json::Value;
use std::time::{Duration, Instant};

/// What `stop` is given (plan, Guest ABI).
pub const STOP: Duration = Duration::from_millis(500);

/// One app. After an error every call answers that error: a guest that
/// trapped is not entered again.
pub struct Runner<H: Host> {
    guest: Guest<H>,
    late: u32,
    paused: bool,
    ended: Option<Error>,
}

impl<H: Host> Runner<H> {
    /// Load a module. Nothing of it has run.
    pub fn load(wasm: &[u8], host: H, limits: Limits) -> Result<Runner<H>, Error> {
        Ok(Runner {
            guest: Guest::load(wasm, Core::new(host, limits))?,
            late: 0,
            paused: false,
            ended: None,
        })
    }

    pub fn host(&self) -> &H {
        &self.guest.core().host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.guest.core_mut().host
    }

    /// The tick period wanted, ms; 0: events only.
    pub fn period(&self) -> u32 {
        self.guest.core().period()
    }

    /// Three late calls in a row: no `tick`, no `event` until `resume`.
    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.late = 0;
    }

    /// The limits met so far: `memory`, `late`, `paused`, `call`,
    /// `canvas`, `log`.
    pub fn met(&self) -> &[&'static str] {
        self.guest.core().met()
    }

    pub fn ended(&self) -> Option<&Error> {
        self.ended.as_ref()
    }

    /// Linear memory now, bytes.
    pub fn memory_size(&self) -> usize {
        self.guest.memory_size()
    }

    pub fn init(&mut self, args: &Value, now_ms: u64) -> Result<(), Error> {
        let bound = self.guest.core().limits.init;
        let args = serde_json::to_vec(args).unwrap_or_default();
        self.enter(now_ms, false, |g| g.init(&args, bound))
    }

    /// Not delivered while paused.
    pub fn tick(&mut self, now_ms: u64) -> Result<(), Error> {
        let bound = self.guest.core().limits.hard;
        self.enter(now_ms, true, |g| g.tick(now_ms, bound))
    }

    /// Not delivered while paused.
    pub fn event(&mut self, topic: &str, data: &Value, now_ms: u64) -> Result<(), Error> {
        let bound = self.guest.core().limits.hard;
        let data = serde_json::to_vec(data).unwrap_or_default();
        self.enter(now_ms, true, |g| g.event(topic.as_bytes(), &data, bound))
    }

    /// The optional `stop`, 500 ms. The guest is not entered after it.
    pub fn stop(&mut self, now_ms: u64) -> Result<(), Error> {
        let done = self.enter(now_ms, false, |g| g.stop(STOP));
        self.ended.get_or_insert(Error::Exit(0));
        done
    }

    fn enter(
        &mut self,
        now_ms: u64,
        timed: bool,
        f: impl FnOnce(&mut Guest<H>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Some(e) = &self.ended {
            return Err(e.clone());
        }
        if timed && self.paused {
            return Ok(());
        }
        self.guest.core_mut().set_now(now_ms);
        let start = Instant::now();
        if let Err(e) = f(&mut self.guest) {
            self.guest.core_mut().host.log(&e.to_string());
            self.ended = Some(e.clone());
            return Err(e);
        }
        if timed {
            let core = self.guest.core_mut();
            if start.elapsed() > core.limits.late {
                self.late += 1;
                core.meet("late");
                if self.late >= core.limits.late_row {
                    self.paused = true;
                    core.meet("paused");
                }
            } else {
                self.late = 0;
            }
        }
        Ok(())
    }
}
