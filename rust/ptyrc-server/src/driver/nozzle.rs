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
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// A press on a canvas: its cell, when, the screen cells of the press
/// and of the last move, whether it is taken from the program.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Down {
    at: [i64; 2],
    since: Duration,
    start: (u16, u16),
    last: (u16, u16),
    taken: bool,
}

/// An effect told the pointer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Pointed {
    conn: ConnId,
    name: String,
    pointer: bool,
    bomb: bool,
    /// As last told.
    at: Option<[i64; 2]>,
    down: Option<Down>,
}

/// The effects told the pointer, by sprite; the tunes to send.
#[derive(Default)]
pub(super) struct Nozzles {
    by_sprite: BTreeMap<u64, Pointed>,
    feed: Vec<(ConnId, u64, Value)>,
}

/// The effect's name, `pointer`, `bomb`, as asked.
pub(super) type Flags = (String, Option<bool>, Option<bool>);

/// Longest press told, ms.
const MAX_HOLD: u128 = 60_000;

/// The flags of a request, taken out of its effect's params (the
/// effect does not know them): the effect's name, `pointer`, `bomb`.
pub(super) fn take_flags(method: &str, params: &mut Value) -> Option<Flags> {
    if !matches!(method, "draw.put" | "draw.update") {
        return None;
    }
    let effect = params.get_mut("effect")?;
    let name = effect.get("name")?.as_str()?.to_string();
    let object = effect.get_mut("params")?.as_object_mut()?;
    let mut flag = |key: &str| {
        let on = object.get(key)?.as_bool()?;
        object.remove(key);
        Some(on)
    };
    let (pointer, bomb) = (flag("pointer"), flag("bomb"));
    (pointer.is_some() || bomb.is_some()).then_some((name, pointer, bomb))
}

impl Nozzles {
    /// `sprite` of `conn` asked the flags.
    pub fn note(&mut self, conn: ConnId, sprite: u64, (name, pointer, bomb): Flags) {
        let old = self.by_sprite.remove(&sprite);
        let keep = |new: Option<bool>, old: Option<bool>| new.or(old).unwrap_or(false);
        let pointer = keep(pointer, old.as_ref().map(|o| o.pointer));
        let bomb = keep(bomb, old.as_ref().map(|o| o.bomb));
        if pointer || bomb {
            let entry = Pointed {
                conn,
                name,
                pointer,
                bomb,
                at: None,
                down: None,
            };
            self.by_sprite.insert(sprite, entry);
        }
    }

    pub fn forget_conn(&mut self, conn: ConnId) {
        self.by_sprite.retain(|_, p| p.conn != conn);
    }

    /// Some effect is told the pointer's moves; its clicks.
    pub fn wants(&self) -> (bool, bool) {
        let all = self.by_sprite.values();
        (all.clone().any(|p| p.pointer), all.clone().any(|p| p.bomb))
    }

    /// A move of the host pointer with no button down.
    pub fn hover(&mut self, x: u16, y: u16, canvas: &dyn Fn(u64, u16, u16) -> Option<[i64; 2]>) {
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.pointer) {
            let at = canvas(*id, x, y);
            if at != p.at {
                p.at = at;
                let tune = json!({"name": p.name, "params": {"hover": at}});
                self.feed.push((p.conn, *id, tune));
            }
        }
    }

    /// A report of a button: true when taken from the program. `takes`:
    /// whether `conn` takes the clicks it is told.
    pub fn click(
        &mut self,
        report: Report,
        now: Duration,
        canvas: &dyn Fn(u64, u16, u16) -> Option<[i64; 2]>,
        takes: &dyn Fn(ConnId) -> bool,
    ) -> bool {
        let here = (report.x, report.y);
        let mut taken = false;
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.bomb) {
            if report.wheel() {
                continue;
            }
            if report.press() {
                p.down = canvas(*id, report.x, report.y).map(|at| Down {
                    at,
                    since: now,
                    start: here,
                    last: here,
                    taken: takes(p.conn),
                });
                taken |= p.down.is_some_and(|d| d.taken);
                continue;
            }
            let Some(mut down) = p.down else {
                continue;
            };
            down.last = here;
            taken |= down.taken;
            if !report.release {
                p.down = Some(down);
                continue;
            }
            p.down = None;
            let ms = now.saturating_sub(down.since).as_millis().min(MAX_HOLD) as u64;
            let mut bomb = vec![json!(down.at[0]), json!(down.at[1]), json!(ms)];
            if down.last != down.start {
                let d = |a: u16, b: u16| i64::from(a) - i64::from(b);
                bomb.push(json!(d(down.last.0, down.start.0)));
                bomb.push(json!(d(down.last.1, down.start.1)));
            }
            let tune = json!({"name": p.name, "params": {"bomb": bomb}});
            self.feed.push((p.conn, *id, tune));
        }
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
            let reply = hook!(self, now, |ctx| engine.handle(
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
            gone.iter()
                .for_each(|id| drop(self.nozzles.by_sprite.remove(id)));
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
