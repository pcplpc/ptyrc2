// SPDX-License-Identifier: GPL-3.0-or-later
//! The pointer told to an effect, as the driver tells it:
//! `ptyrc_rules::nozzle`, its tunes handed back to the caller. One app,
//! no owner; session time in ms. The press is always told (`takes`):
//! whether the program has it too is the report's (`lacks`).

use ptyrc_rules::nozzle::{self as rules, Click};
use serde_json::Value;
use std::time::Duration;

pub(crate) use ptyrc_rules::nozzle::{Flags, take_flags};
#[cfg(test)]
use serde_json::json;

/// The cell of the canvas of a sprite at a screen cell, when shown there.
pub(crate) type CanvasAt<'a> = rules::CanvasAt<'a>;

/// The effects told the pointer, by sprite.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Nozzles {
    rules: rules::Nozzles<()>,
}

impl Nozzles {
    /// `sprite` asked the flags; one not given keeps its value.
    pub fn note(&mut self, sprite: u64, flags: Flags) {
        self.rules.note((), sprite, flags);
    }

    /// A tune refused: the sprite is gone.
    pub fn forget(&mut self, sprite: u64) {
        self.rules.forget(sprite);
    }

    /// Of `sprite`: the cell last told hovered; a press held.
    #[cfg(test)]
    pub fn told(&self, sprite: u64) -> Option<(Option<[i64; 2]>, bool)> {
        self.rules.told(sprite)
    }

    /// Some effect is told the pointer's moves; its clicks.
    pub fn wants(&self) -> (bool, bool) {
        self.rules.wants()
    }

    fn tunes(&mut self) -> Vec<(u64, Value)> {
        (self.rules.take_feed().into_iter())
            .map(|(_, id, tune)| (id, tune))
            .collect()
    }

    /// A move with no button down: the tunes, by sprite.
    pub fn hover(&mut self, x: u16, y: u16, canvas: CanvasAt) -> Vec<(u64, Value)> {
        self.rules.hover(x, y, canvas);
        self.tunes()
    }

    /// A press (`press`) or a release of the left button at `now` ms:
    /// the tunes, and whether a press fell on a canvas or a release
    /// ended one.
    pub fn click(
        &mut self,
        press: bool,
        at: (u16, u16),
        now: u64,
        canvas: CanvasAt,
    ) -> (Vec<(u64, Value)>, bool) {
        let click = if press { Click::Press } else { Click::Release };
        let now = Duration::from_millis(now);
        let told = self.rules.click(click, at, now, canvas, &|()| true);
        (self.tunes(), told)
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
