// SPDX-License-Identifier: GPL-3.0-or-later
//! The pointer told to an effect: `"effect": {"params": {"pointer":
//! true, "bomb": true}}` of `draw.put`, `draw.update`. The driver tunes
//! the effect itself: `hover` the cell of its canvas under the host
//! pointer, or null; on the release of a press on its canvas, `bomb`
//! `[x, y, ms]` or `[x, y, ms, dx, dy]`, released that many cells away.
//! The app is told nothing. The clicks are taken from the program when
//! the app holds `clicks-grab` (or the whole session); else the program
//! has them too.

use super::Driver;
use crate::engine::{ConnId, Ctx, Effects, Engine, Token};
use crate::mouse::Report;
use ptyrc_rules::nozzle::{self as rules, Click};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) use ptyrc_rules::nozzle::{Flags, take_flags};

/// The effects told the pointer (`ptyrc_rules::nozzle`); the tunes to
/// send.
#[derive(Default)]
pub(super) struct Nozzles {
    rules: rules::Nozzles<ConnId>,
    feed: Vec<(ConnId, u64, Value)>,
}

impl Nozzles {
    /// `sprite` of `conn` asked the flags.
    pub fn note(&mut self, conn: ConnId, sprite: u64, flags: Flags) {
        self.rules.note(conn, sprite, flags);
    }

    pub fn forget_conn(&mut self, conn: ConnId) {
        self.rules.forget_owner(conn);
    }

    /// Some effect is told the pointer's moves; its clicks.
    pub fn wants(&self) -> (bool, bool) {
        self.rules.wants()
    }

    /// A move of the host pointer with no button down.
    pub fn hover(&mut self, x: u16, y: u16, canvas: &dyn Fn(u64, u16, u16) -> Option<[i64; 2]>) {
        self.rules.hover(x, y, canvas);
        self.feed.extend(self.rules.take_feed());
    }

    /// A report of a button: true when taken from the program. `takes`:
    /// whether `conn` takes the clicks it is told. The wheel: nothing.
    pub fn click(
        &mut self,
        report: Report,
        now: Duration,
        canvas: &dyn Fn(u64, u16, u16) -> Option<[i64; 2]>,
        takes: &dyn Fn(ConnId) -> bool,
    ) -> bool {
        if report.wheel() {
            return false;
        }
        let click = match (report.press(), report.release) {
            (true, _) => Click::Press,
            (false, false) => Click::Drag,
            (false, true) => Click::Release,
        };
        let taken = (self.rules).click(click, (report.x, report.y), now, canvas, takes);
        self.feed.extend(self.rules.take_feed());
        taken
    }
}

impl Driver {
    /// The tunes noted, sent to the effects. A sprite gone: forgotten.
    pub(super) fn feed_nozzles(&mut self, engine: &mut dyn Engine) {
        let feed = std::mem::take(&mut self.nozzles.feed);
        let now = self.now();
        let mut gone = Vec::new();
        for (conn, id, effect) in feed {
            let token = Token(self.next_token);
            self.next_token += 1;
            let params = json!({"id": id, "effect": effect});
            let reply = hook!(self, engine, now, |ctx| engine.handle(
                conn,
                token,
                "draw.update",
                params,
                ctx
            ));
            if matches!(reply, crate::engine::Reply::Err(_)) {
                gone.push(id);
            }
        }
        if !gone.is_empty() {
            gone.iter().for_each(|id| self.nozzles.rules.forget(*id));
            self.refresh_topics();
            self.sync_host_mouse(engine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(code: u8, x: u16, y: u16, release: bool) -> Report {
        Report {
            code,
            x,
            y,
            release,
        }
    }

    /// A canvas of 10 x 5 at screen cell 20, 2.
    fn canvas(_id: u64, x: u16, y: u16) -> Option<[i64; 2]> {
        let (cx, cy) = (x as i64 - 20, y as i64 - 2);
        ((0..10).contains(&cx) && (0..5).contains(&cy)).then_some([cx, cy])
    }

    fn flags(params: Value) -> (Value, Option<Flags>) {
        let mut params = json!({"effect": {"name": "sand", "params": params}});
        let flags = take_flags("draw.put", &mut params);
        (params["effect"]["params"].clone(), flags)
    }

    #[test]
    fn the_flags_are_the_drivers() {
        let (left, got) = flags(json!({"rate": 5, "pointer": true, "bomb": false}));
        assert_eq!(left, json!({"rate": 5}));
        assert_eq!(got, Some(("sand".into(), Some(true), Some(false))));
        // A bomb tuned is the effect's.
        let (left, got) = flags(json!({"bomb": [1, 2, 3]}));
        assert_eq!((left, got), (json!({"bomb": [1, 2, 3]}), None));
        let mut other = json!({"content": {"rows": ["x"]}, "pointer": true});
        assert_eq!(take_flags("draw.put", &mut other), None);
    }

    #[test]
    fn the_pointer_and_the_clicks_are_told_to_the_effect() {
        let mut n = Nozzles::default();
        n.note(ConnId(1), 7, ("sand".into(), Some(true), None));
        assert_eq!(n.wants(), (true, false));
        n.hover(21, 3, &canvas);
        n.hover(21, 3, &canvas);
        n.hover(0, 0, &canvas);
        let tunes: Vec<Value> = n
            .feed
            .drain(..)
            .map(|(_, _, t)| t["params"].clone())
            .collect();
        assert_eq!(tunes, [json!({"hover": [1, 1]}), json!({"hover": null})]);
        // `bomb` added, `pointer` kept.
        n.note(ConnId(1), 7, ("sand".into(), None, Some(true)));
        assert_eq!(n.wants(), (true, true));
        let ms = Duration::from_millis;
        let none = |_: ConnId| false;
        assert!(!n.click(report(0, 22, 4, false), ms(100), &canvas, &none));
        assert!(!n.click(report(32, 25, 3, false), ms(200), &canvas, &none));
        assert!(!n.click(report(0, 25, 3, true), ms(400), &canvas, &none));
        let (_, id, tune) = n.feed.pop().unwrap();
        assert_eq!(id, 7);
        assert_eq!(tune["params"], json!({"bomb": [2, 2, 300, 3, -1]}));
        // Taken when the app takes them; a press elsewhere is nothing.
        let all = |_: ConnId| true;
        assert!(n.click(report(0, 22, 4, false), ms(500), &canvas, &all));
        assert!(n.click(report(0, 22, 4, true), ms(510), &canvas, &all));
        assert_eq!(
            n.feed.pop().unwrap().2["params"],
            json!({"bomb": [2, 2, 10]})
        );
        assert!(!n.click(report(0, 0, 0, false), ms(600), &canvas, &all));
        assert!(!n.click(report(0, 0, 0, true), ms(610), &canvas, &all));
        assert!(n.feed.is_empty());
        // Both off: forgotten.
        n.note(ConnId(1), 7, ("sand".into(), Some(false), Some(false)));
        assert_eq!(n.wants(), (false, false));
    }
}
