// SPDX-License-Identifier: GPL-3.0-or-later
//! Clockface: a stopwatch in a widget, the model of a deck app; the
//! twin of `sdk/js/examples/clockface`.
//!
//! Two sizes: block digits (17x3, a Canvas) where there is room, a line
//! (5x1) where not; the driver picks. The widget asks for the keyboard,
//! quietly: focused (ctrl+t twice), `space` pauses, `r` resets. Time is
//! session time, the `now` of `tick`.

mod face;

use face::{SIZES, face};
use ptyrc_app::deck::{Deck, Spec};
use ptyrc_app::{App, Canvas, Value, json, log, period};

/// The state the deck's handlers are given.
#[derive(Default)]
struct Watch {
    /// ms on the face when it last paused, or at `since`.
    shown: u64,
    /// The session time it last started; `None`: paused.
    since: Option<u64>,
    canvas: Option<Canvas>,
}

impl Watch {
    fn elapsed(&self, now: u64) -> u64 {
        self.shown + self.since.map_or(0, |s| now.saturating_sub(s))
    }

    /// `false`: not ours, nothing to draw.
    fn key(&mut self, name: &str, now: u64) -> bool {
        match name {
            "space" if self.since.is_none() => self.since = Some(now),
            "space" => (self.shown, self.since) = (self.elapsed(now), None),
            "r" => (self.shown, self.since) = (0, self.since.map(|_| now)),
            _ => return false,
        }
        true
    }
}

struct Clockface {
    deck: Deck<Watch>,
    watch: Watch,
}

impl App for Clockface {
    fn init(args: Value) -> Clockface {
        let mut watch = Watch {
            since: Some(0),
            ..Watch::default()
        };
        let mut deck = Deck::new(json!({"border": "round", "prefer": "top-right"}));
        deck.every = args["every"] == true;
        let spec = Spec::new("clockface", SIZES, |s: &mut Watch, w, h, widget| {
            let paused = s.since.is_none();
            face(s.elapsed(widget.now), w, h, paused, &mut s.canvas)
        })
        .key(|s, name, widget| s.key(name, widget.now));
        if let Err(e) = deck.add(spec, &mut watch) {
            log(&format!("clockface: {e}"));
        }
        period(1000);
        Clockface { deck, watch }
    }

    fn tick(&mut self, now_ms: u64) {
        if let Err(e) = self.deck.tick(now_ms, &mut self.watch) {
            log(&format!("clockface: {e}"));
        }
    }

    fn event(&mut self, topic: &str, data: Value) {
        if let Err(e) = self.deck.event(topic, &data, &mut self.watch) {
            log(&format!("clockface: {e}"));
        }
    }
}

ptyrc_app::app!(Clockface);
