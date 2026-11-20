// SPDX-License-Identifier: GPL-3.0-or-later
//! The pointer told to an effect: `"effect": {"params": {"pointer":
//! true, "bomb": true}}` of `draw.put`, `draw.update`, taken out before
//! the effect sees them. `pointer`: `hover`, the cell of its canvas
//! under the pointer, or null, tuned when it changes. `bomb`: on the
//! release of a press on its canvas, `bomb` `[x, y, ms]`, or `[x, y,
//! ms, dx, dy]` released that many cells away. The app is told nothing.
//! The tunes are queued (`take_feed`); the caller sends them.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// The effect's name, `pointer`, `bomb`, as asked.
pub type Flags = (String, Option<bool>, Option<bool>);

/// The cell of the canvas of a sprite at a screen cell, when shown there.
pub type CanvasAt<'a> = &'a dyn Fn(u64, u16, u16) -> Option<[i64; 2]>;

/// Longest press told, ms.
pub const MAX_HOLD: u128 = 60_000;

/// A report of the left button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Press,
    /// A move with the button down.
    Drag,
    Release,
}

/// A press on a canvas: its cell, when, the screen cells of the press
/// and of the last report, whether it is taken from the program.
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
struct Pointed<C> {
    owner: C,
    name: String,
    pointer: bool,
    bomb: bool,
    /// As last told.
    at: Option<[i64; 2]>,
    down: Option<Down>,
}

/// The effects told the pointer, by sprite; `C` who drew it. The tunes
/// to send: `(owner, sprite, effect)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Nozzles<C> {
    by_sprite: BTreeMap<u64, Pointed<C>>,
    feed: Vec<(C, u64, Value)>,
}

impl<C> Default for Nozzles<C> {
    fn default() -> Self {
        Nozzles {
            by_sprite: BTreeMap::new(),
            feed: Vec::new(),
        }
    }
}

/// The flags of a request, taken out of its effect's params (the
/// effect does not know them): the effect's name, `pointer`, `bomb`.
pub fn take_flags(method: &str, params: &mut Value) -> Option<Flags> {
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

impl<C: Copy + PartialEq> Nozzles<C> {
    /// `sprite` of `owner` asked the flags; one not given keeps its
    /// value. Both off: forgotten.
    pub fn note(&mut self, owner: C, sprite: u64, (name, pointer, bomb): Flags) {
        let old = self.by_sprite.remove(&sprite);
        let keep = |new: Option<bool>, old: Option<bool>| new.or(old).unwrap_or(false);
        let pointer = keep(pointer, old.as_ref().map(|o| o.pointer));
        let bomb = keep(bomb, old.as_ref().map(|o| o.bomb));
        if pointer || bomb {
            let entry = Pointed {
                owner,
                name,
                pointer,
                bomb,
                at: None,
                down: None,
            };
            self.by_sprite.insert(sprite, entry);
        }
    }

    /// A tune refused: the sprite is gone.
    pub fn forget(&mut self, sprite: u64) {
        self.by_sprite.remove(&sprite);
    }

    pub fn forget_owner(&mut self, owner: C) {
        self.by_sprite.retain(|_, p| p.owner != owner);
    }

    /// Of `sprite`: the cell last told hovered; a press held.
    pub fn told(&self, sprite: u64) -> Option<(Option<[i64; 2]>, bool)> {
        (self.by_sprite.get(&sprite)).map(|p| (p.at, p.down.is_some()))
    }

    /// Some effect is told the pointer's moves; its clicks.
    pub fn wants(&self) -> (bool, bool) {
        let all = self.by_sprite.values();
        (all.clone().any(|p| p.pointer), all.clone().any(|p| p.bomb))
    }

    /// The tunes queued, in order.
    pub fn take_feed(&mut self) -> Vec<(C, u64, Value)> {
        std::mem::take(&mut self.feed)
    }

    /// A move of the pointer with no button down.
    pub fn hover(&mut self, x: u16, y: u16, canvas: CanvasAt) {
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.pointer) {
            let at = canvas(*id, x, y);
            if at != p.at {
                p.at = at;
                let tune = json!({"name": p.name, "params": {"hover": at}});
                self.feed.push((p.owner, *id, tune));
            }
        }
    }

    /// A report of the left button at a cell, at `now`: true when taken
    /// from the program. `takes`: whether `owner` takes the clicks it is
    /// told (`clicks-grab`).
    pub fn click(
        &mut self,
        click: Click,
        here: (u16, u16),
        now: Duration,
        canvas: CanvasAt,
        takes: &dyn Fn(C) -> bool,
    ) -> bool {
        let mut taken = false;
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.bomb) {
            if click == Click::Press {
                p.down = canvas(*id, here.0, here.1).map(|at| Down {
                    at,
                    since: now,
                    start: here,
                    last: here,
                    taken: takes(p.owner),
                });
                taken |= p.down.is_some_and(|d| d.taken);
                continue;
            }
            let Some(mut down) = p.down else {
                continue;
            };
            down.last = here;
            taken |= down.taken;
            if click == Click::Drag {
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
            self.feed.push((p.owner, *id, tune));
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        n.note(1, 7, ("sand".into(), Some(true), None));
        assert_eq!(n.wants(), (true, false));
        n.hover(21, 3, &canvas);
        n.hover(21, 3, &canvas);
        assert_eq!(n.told(7), Some((Some([1, 1]), false)));
        n.hover(0, 0, &canvas);
        let tunes: Vec<Value> = (n.take_feed().into_iter())
            .map(|(_, _, t)| t["params"].clone())
            .collect();
        assert_eq!(tunes, [json!({"hover": [1, 1]}), json!({"hover": null})]);
        // `bomb` added, `pointer` kept.
        n.note(1, 7, ("sand".into(), None, Some(true)));
        assert_eq!(n.wants(), (true, true));
        let ms = Duration::from_millis;
        let none = |_: i32| false;
        let at = |n: &mut Nozzles<i32>, c, x, y, t, takes: &dyn Fn(i32) -> bool| {
            n.click(c, (x, y), ms(t), &canvas, takes)
        };
        assert!(!at(&mut n, Click::Press, 22, 4, 100, &none));
        assert_eq!(n.told(7), Some((None, true)));
        assert!(!at(&mut n, Click::Drag, 25, 3, 200, &none));
        assert!(!at(&mut n, Click::Release, 25, 3, 400, &none));
        let (_, id, tune) = n.take_feed().pop().unwrap();
        assert_eq!(id, 7);
        assert_eq!(tune["params"], json!({"bomb": [2, 2, 300, 3, -1]}));
        // Taken when the app takes them; a press elsewhere is nothing.
        let all = |_: i32| true;
        assert!(at(&mut n, Click::Press, 22, 4, 500, &all));
        assert!(at(&mut n, Click::Release, 22, 4, 510, &all));
        assert_eq!(
            n.take_feed().pop().unwrap().2["params"],
            json!({"bomb": [2, 2, 10]})
        );
        assert!(!at(&mut n, Click::Press, 0, 0, 600, &all));
        assert!(!at(&mut n, Click::Release, 0, 0, 610, &all));
        assert!(n.take_feed().is_empty());
        // Held past the longest press: told as the longest.
        at(&mut n, Click::Press, 22, 4, 1_000, &all);
        at(&mut n, Click::Release, 22, 4, 100_000, &all);
        assert_eq!(n.take_feed()[0].2["params"]["bomb"][2], 60_000);
        // Both off: forgotten; an owner gone, its sprites.
        n.note(1, 7, ("sand".into(), Some(false), Some(false)));
        assert_eq!((n.wants(), n.told(7)), ((false, false), None));
        n.note(2, 8, ("sand".into(), Some(true), None));
        n.forget_owner(2);
        assert_eq!(n.wants(), (false, false));
    }
}
