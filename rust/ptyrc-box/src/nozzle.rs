// SPDX-License-Identifier: GPL-3.0-or-later
//! The pointer told to an effect, as the driver tells it (`ptyrc-server`,
//! `driver/nozzle.rs`): `"effect": {"params": {"pointer": true, "bomb":
//! true}}` of `draw.put`, `draw.update`, taken out before the effect
//! sees them. `pointer`: `hover`, the cell of its canvas under the
//! pointer, or null, tuned when it changes. `bomb`: on the release of a
//! press on its canvas, `bomb` `[x, y, ms]`, or `[x, y, ms, dx, dy]`
//! released that many cells away. The app is told nothing.

use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The effect's name, `pointer`, `bomb`, as asked.
pub(crate) type Flags = (String, Option<bool>, Option<bool>);

/// The cell of the canvas of a sprite at a screen cell, when shown there.
pub(crate) type CanvasAt<'a> = &'a dyn Fn(u64, u16, u16) -> Option<[i64; 2]>;

/// Longest press told, ms (the driver's `MAX_HOLD`).
const MAX_HOLD: u64 = 60_000;

/// A press on a canvas: its cell, when, the screen cells of the press
/// and of the last report.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Down {
    at: [i64; 2],
    since: u64,
    start: (u16, u16),
    last: (u16, u16),
}

#[derive(Clone, Debug, PartialEq)]
struct Pointed {
    name: String,
    pointer: bool,
    bomb: bool,
    /// As last told.
    at: Option<[i64; 2]>,
    down: Option<Down>,
}

/// The effects told the pointer, by sprite.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Nozzles {
    by_sprite: BTreeMap<u64, Pointed>,
}

/// The flags of a request, taken out of its effect's params.
pub(crate) fn take_flags(method: &str, params: &mut Value) -> Option<Flags> {
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
    /// `sprite` asked the flags; one not given keeps its value.
    pub fn note(&mut self, sprite: u64, (name, pointer, bomb): Flags) {
        let old = self.by_sprite.remove(&sprite);
        let keep = |new: Option<bool>, old: Option<bool>| new.or(old).unwrap_or(false);
        let pointer = keep(pointer, old.as_ref().map(|o| o.pointer));
        let bomb = keep(bomb, old.as_ref().map(|o| o.bomb));
        if pointer || bomb {
            let entry = Pointed {
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

    /// Of `sprite`: the cell last told hovered; a press held.
    #[cfg(test)]
    pub fn told(&self, sprite: u64) -> Option<(Option<[i64; 2]>, bool)> {
        (self.by_sprite.get(&sprite)).map(|p| (p.at, p.down.is_some()))
    }

    /// Some effect is told the pointer's moves; its clicks.
    pub fn wants(&self) -> (bool, bool) {
        let all = self.by_sprite.values();
        (all.clone().any(|p| p.pointer), all.clone().any(|p| p.bomb))
    }

    /// A move with no button down: the tunes, by sprite.
    pub fn hover(&mut self, x: u16, y: u16, canvas: CanvasAt) -> Vec<(u64, Value)> {
        let mut tunes = Vec::new();
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.pointer) {
            let at = canvas(*id, x, y);
            if at != p.at {
                p.at = at;
                tunes.push((*id, json!({"name": p.name, "params": {"hover": at}})));
            }
        }
        tunes
    }

    /// A press (`press`) or a release of the left button at `now` ms:
    /// the tunes, and whether a press fell on a canvas or a release
    /// ended one.
    pub fn click(
        &mut self,
        press: bool,
        (x, y): (u16, u16),
        now: u64,
        canvas: CanvasAt,
    ) -> (Vec<(u64, Value)>, bool) {
        let mut tunes = Vec::new();
        let mut told = false;
        for (id, p) in self.by_sprite.iter_mut().filter(|(_, p)| p.bomb) {
            if press {
                p.down = canvas(*id, x, y).map(|at| Down {
                    at,
                    since: now,
                    start: (x, y),
                    last: (x, y),
                });
                told |= p.down.is_some();
                continue;
            }
            let Some(mut down) = p.down.take() else {
                continue;
            };
            told = true;
            down.last = (x, y);
            let ms = now.saturating_sub(down.since).min(MAX_HOLD);
            let mut bomb = vec![json!(down.at[0]), json!(down.at[1]), json!(ms)];
            if down.last != down.start {
                let d = |a: u16, b: u16| i64::from(a) - i64::from(b);
                bomb.push(json!(d(down.last.0, down.start.0)));
                bomb.push(json!(d(down.last.1, down.start.1)));
            }
            tunes.push((*id, json!({"name": p.name, "params": {"bomb": bomb}})));
        }
        (tunes, told)
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

    // The cases of the driver's `nozzle.rs`, but the drag.
    #[test]
    fn the_flags_are_the_drivers() {
        let (left, got) = flags(json!({"rate": 5, "pointer": true, "bomb": false}));
        assert_eq!(left, json!({"rate": 5}));
        assert_eq!(got, Some(("sand".into(), Some(true), Some(false))));
        let (left, got) = flags(json!({"bomb": [1, 2, 3]}));
        assert_eq!((left, got), (json!({"bomb": [1, 2, 3]}), None));
        let mut other = json!({"content": {"rows": ["x"]}, "pointer": true});
        assert_eq!(take_flags("draw.put", &mut other), None);
    }

    #[test]
    fn the_pointer_and_the_clicks_are_told_to_the_effect() {
        let mut n = Nozzles::default();
        n.note(7, ("sand".into(), Some(true), None));
        assert_eq!(n.wants(), (true, false));
        let mut tunes = n.hover(21, 3, &canvas);
        tunes.extend(n.hover(21, 3, &canvas));
        tunes.extend(n.hover(0, 0, &canvas));
        let tunes: Vec<Value> = tunes
            .into_iter()
            .map(|(_, t)| t["params"].clone())
            .collect();
        assert_eq!(tunes, [json!({"hover": [1, 1]}), json!({"hover": null})]);
        n.note(7, ("sand".into(), None, Some(true)));
        assert_eq!(n.wants(), (true, true));
        assert_eq!(n.click(true, (22, 4), 100, &canvas), (vec![], true));
        let (tunes, told) = n.click(false, (25, 3), 400, &canvas);
        assert!(told);
        assert_eq!(tunes[0].0, 7);
        assert_eq!(tunes[0].1["params"], json!({"bomb": [2, 2, 300, 3, -1]}));
        n.click(true, (22, 4), 500, &canvas);
        let (tunes, _) = n.click(false, (22, 4), 510, &canvas);
        assert_eq!(tunes[0].1["params"], json!({"bomb": [2, 2, 10]}));
        // A press elsewhere is nothing.
        assert_eq!(n.click(true, (0, 0), 600, &canvas), (vec![], false));
        assert_eq!(n.click(false, (0, 0), 610, &canvas), (vec![], false));
        n.note(7, ("sand".into(), Some(false), Some(false)));
        assert_eq!(n.wants(), (false, false));
    }
}
