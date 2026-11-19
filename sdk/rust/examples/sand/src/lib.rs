// SPDX-License-Identifier: GPL-3.0-or-later
//! Sand: a thin stream piles on the text of the frame.
//!
//! One request: a sprite whose content is the effect `sand` of the
//! driver. The simulation runs there; it reads the text under it as its
//! walls at each render and answers nothing (`plan/store-model.md`,
//! principle 2). The app holds no permission and then only waits.
//!
//! The test host (`ptyrc dev test`, server `ptyrc-box`) has no draw
//! extension, and a driver may lack this one: there the app pours a
//! pile of its own on the floor, blind to the text. The twin of
//! `sdk/js/examples/sand`: the same seed, the same pile.

use ptyrc_app::{App, Canvas, Cell, Color, Value, call, json, log, random_u32};

const SAND: Color = Color::Rgb(0xd7, 0xaf, 0x5f);
/// The dots of a cell, from the bottom: of its left column, its right.
const LEFT: [u32; 5] = [0, 0x40, 0x44, 0x46, 0x47];
const RIGHT: [u32; 5] = [0, 0x80, 0xa0, 0xb0, 0xb8];

/// The app's own pile. Grains are braille dots, 2 x 4 a cell:
/// `heights[x]`, the grains of the dot column `x`.
struct Pile {
    canvas: Canvas,
    heights: Vec<u32>,
    spout: usize,
}

struct Sand {
    rate: f64,
    pile: Option<Pile>,
    owed: f64,
    last: u64,
}

fn effect(rate: f64, cursor: bool) -> bool {
    let params = json!({"rate": rate, "cursor": cursor, "seed": random_u32()});
    match call(
        "draw.put",
        json!({"effect": {"name": "sand", "params": params}}),
    ) {
        Ok(_) => true,
        Err(e) => {
            log(&format!("sand: the effect: {e}; a pile of its own"));
            false
        }
    }
}

impl Pile {
    fn new((w, h): (u32, u32)) -> Pile {
        Pile {
            canvas: Canvas::new(w, h).unwrap_or_else(|| ptyrc_app::exit(1)),
            heights: vec![0; 2 * w as usize],
            spout: 2 * (w as usize / 2),
        }
    }

    fn full(&self) -> bool {
        self.heights[self.spout] >= 4 * self.canvas.height()
    }

    /// One grain on the spout; it rolls down while a side is two lower.
    fn pour(&mut self) {
        let hs = &mut self.heights;
        let mut x = self.spout;
        hs[x] += 1;
        loop {
            let l = x > 0 && hs[x] >= hs[x - 1] + 2;
            let r = x + 1 < hs.len() && hs[x] >= hs[x + 1] + 2;
            let to = match (l, r) {
                (false, false) => return,
                (true, true) if random_u32() & 1 == 1 => x + 1,
                (true, _) => x - 1,
                (false, true) => x + 1,
            };
            hs[x] -= 1;
            hs[to] += 1;
            x = to;
        }
    }

    fn draw(&mut self) {
        let h = self.canvas.height();
        let cell = |ch: u32| Cell {
            ch: char::from_u32(ch),
            fg: SAND,
            ..Cell::default()
        };
        self.canvas.clear();
        for c in 0..self.canvas.width() {
            let a = self.heights[2 * c as usize];
            let b = self.heights[2 * c as usize + 1];
            let full = a.min(b) / 4;
            for r in 0..a.max(b).div_ceil(4) {
                let ch = match r < full {
                    true => 0x28ff,
                    false => {
                        let dots = |n: u32| n.saturating_sub(4 * r).min(4) as usize;
                        0x2800 + LEFT[dots(a)] + RIGHT[dots(b)]
                    }
                };
                self.canvas.set(c, h - 1 - r, cell(ch));
            }
        }
        let top = h.saturating_sub(self.heights[self.spout].div_ceil(4));
        let stream = Cell {
            attrs: ptyrc_app::attr::DIM,
            ..cell(0x2847)
        };
        for y in 0..top {
            self.canvas.set(self.spout as u32 / 2, y, stream);
        }
        self.canvas
            .present(&json!({"anchor": {"at": [0, 0], "space": "view"}}));
    }
}

impl App for Sand {
    fn init(args: Value) -> Sand {
        let rate = args["rate"].as_f64().filter(|r| *r > 0.0).unwrap_or(60.0);
        let mut app = Sand {
            rate: rate.min(1000.0),
            pile: None,
            owed: 0.0,
            last: 0,
        };
        let info = call("session.info", Value::Null).unwrap_or_default();
        let cursor = args["cursor"].as_bool() == Some(true);
        if info["server"] != "ptyrc-box" && effect(app.rate, cursor) {
            return app;
        }
        app.pile = Some(Pile::new(ptyrc_app::size().unwrap_or((80, 24))));
        if let Err(e) = ptyrc_app::subscribe(&["size"]) {
            log(&format!("sand: {e}"));
        }
        ptyrc_app::period(50);
        app
    }

    fn tick(&mut self, now: u64) {
        let Some(pile) = &mut self.pile else { return };
        self.owed += self.rate * (now - self.last) as f64 / 1000.0;
        self.last = now;
        while self.owed >= 1.0 {
            if pile.full() {
                self.owed = 0.0;
            } else {
                pile.pour();
                self.owed -= 1.0;
            }
        }
        pile.draw();
    }

    fn event(&mut self, topic: &str, data: Value) {
        let n = |i: usize| data["size"][i].as_u64().map(|n| n as u32);
        if let ("size", Some(_), Some(c), Some(r)) = (topic, &self.pile, n(0), n(1)) {
            self.pile = Some(Pile::new((c, r)));
        }
    }
}

ptyrc_app::app!(Sand);
