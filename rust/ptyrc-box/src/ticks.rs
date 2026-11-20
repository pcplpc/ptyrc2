// SPDX-License-Identifier: GPL-3.0-or-later
//! When `tick` is due: one rule for the live and the headless run.

/// The next tick, in session ms. After `init`: the period from when it
/// was asked. After a step: a period asked (in `tick`, in `event`,
/// changed or not) counts from its asking; none asked, a tick given,
/// the next is due one period after the one given was due (no drift),
/// never before the step (a tick missed is not made up for). 0: none.
///
/// Headless, a step is at one time: asked is now. Live, asked is the
/// instant of the `period` call: an app that reads the wall clock then
/// asks `until` the second lands on it, not before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticks {
    period: u32,
    next: Option<u64>,
}

impl Ticks {
    /// After `init`; `asked`: when its period was asked.
    pub fn new(period: u32, asked: u64) -> Ticks {
        Ticks {
            period,
            next: (period > 0).then(|| asked + u64::from(period)),
        }
    }

    pub fn next(&self) -> Option<u64> {
        self.next
    }

    pub fn due(&self, now: u64) -> bool {
        self.next.is_some_and(|t| t <= now)
    }

    /// After a step at `now`. `period`: the runner's now; `asked`: when
    /// it was asked in the step (`now` headless), `None` not; `ticked`:
    /// the tick due was given.
    pub fn step(&mut self, period: u32, now: u64, asked: Option<u64>, ticked: bool) {
        if asked.is_some() || period != self.period {
            *self = Ticks::new(period, asked.unwrap_or(now));
        } else if ticked && let Some(t) = self.next {
            self.next = Some((t + u64::from(period)).max(now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ticks;

    #[test]
    fn a_period_changed_counts_from_its_asking() {
        let mut t = Ticks::new(100, 0);
        assert_eq!(t.next(), Some(100));
        t.step(100, 103, None, true);
        assert_eq!(t.next(), Some(200), "not asked: no drift");
        t.step(100, 203, Some(204), true);
        assert_eq!(t.next(), Some(304), "asked again: from the asking");
        t.step(250, 305, Some(306), true);
        assert_eq!(t.next(), Some(556), "changed in tick: from the asking");
        t.step(40, 400, Some(400), false);
        assert_eq!(t.next(), Some(440), "changed in an event");
        t.step(40, 600, None, true);
        assert_eq!(t.next(), Some(600), "missed: not made up for");
        t.step(0, 600, Some(600), true);
        assert_eq!(t.next(), None);
        assert!(!t.due(u64::MAX));
    }
}
