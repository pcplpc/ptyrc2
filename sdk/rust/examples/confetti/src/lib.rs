// SPDX-License-Identifier: GPL-3.0-or-later
//! Confetti: small pops of colour, as in the first ptyrc.
//!
//! A round every 3 s: `bursts` bursts, each at a random time of the
//! round; a burst is four pops, one a colour, each within a second, each
//! at a random cell away from the last ones. A pop is a 3x3 canvas
//! played frame by frame, then released. `one-shot`: one round, then
//! exit. `front`: over the text; behind it by default.

use ptyrc_app::{App, Canvas, Cell, Color, Value, attr, json};

const PERIOD: u64 = 3000;
const STEP: u64 = 100;
const MAX_BURSTS: u64 = 10;
/// Bright green, magenta, blue, yellow.
const COLORS: [u8; 4] = [10, 13, 12, 11];
/// The pop, frame by frame.
const POP: [[&str; 3]; 4] = [
    ["   ", " · ", "   "],
    ["   ", " * ", "   "],
    ["· ·", " o ", "· ·"],
    [" · ", "· ·", " · "],
];
/// No two pops this near among the last ones.
const REACH: i32 = 3;

struct Pop {
    canvas: Canvas,
    start: u64,
    place: Value,
    shown: Option<usize>,
}

struct Confetti {
    bursts: u64,
    one_shot: bool,
    layer: &'static str,
    size: (u32, u32),
    /// `(at, colour)`, the latest first.
    shots: Vec<(u64, u8)>,
    pops: Vec<Pop>,
    avoid: Vec<(i32, i32)>,
    rounds: u64,
}

impl Confetti {
    fn round(&mut self) {
        let start = self.rounds * PERIOD;
        self.rounds += 1;
        for _ in 0..self.bursts {
            let at = start + u64::from(ptyrc_app::random_below(PERIOD as u32));
            for color in COLORS {
                let wait = STEP * u64::from(ptyrc_app::random_below(11));
                self.shots.push((at + wait, color));
            }
        }
        self.shots.sort_by(|a, b| b.cmp(a));
    }

    fn spot(&mut self) -> (i32, i32) {
        let (w, h) = (self.size.0.max(1), self.size.1.max(1));
        let mut cell = (0, 0);
        for _ in 0..32 {
            cell = (
                ptyrc_app::random_below(w) as i32,
                ptyrc_app::random_below(h) as i32,
            );
            let near =
                |(x, y): &(i32, i32)| (x - cell.0).abs() <= REACH && (y - cell.1).abs() <= REACH;
            if !self.avoid.iter().any(near) {
                break;
            }
        }
        let kept = (self.bursts * COLORS.len() as u64).min(MAX_BURSTS) as usize;
        self.avoid.push(cell);
        if self.avoid.len() > kept {
            self.avoid.remove(0);
        }
        cell
    }

    fn shoot(&mut self, now: u64, color: u8) {
        let (x, y) = self.spot();
        let Some(mut canvas) = Canvas::new(3, 3) else {
            ptyrc_app::log("confetti: no canvas");
            return;
        };
        canvas.fill(Cell {
            ch: None,
            fg: Color::Palette(color),
            ..Cell::default()
        });
        let place = json!({"anchor": {"at": [x - 1, y - 1]}, "layer": self.layer, "z": 100});
        self.pops.push(Pop {
            canvas,
            start: now,
            place,
            shown: None,
        });
    }
}

fn draw(pop: &mut Pop, frame: usize) {
    let last = frame == POP.len() - 1;
    for (y, row) in POP[frame].iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let (x, y) = (x as u32, y as u32);
            pop.canvas.set_char(x, y, (ch != ' ').then_some(ch));
            pop.canvas.set_attrs(x, y, if last { attr::DIM } else { 0 });
        }
    }
    pop.canvas.present(&pop.place);
}

impl App for Confetti {
    fn init(args: Value) -> Confetti {
        let bursts = args.get("bursts").and_then(Value::as_u64).unwrap_or(1);
        let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
        let size = ptyrc_app::size().unwrap_or((80, 24));
        if let Err(e) = ptyrc_app::subscribe(&["size"]) {
            ptyrc_app::log(&format!("confetti: {e}"));
        }
        let mut app = Confetti {
            bursts: bursts.clamp(1, MAX_BURSTS),
            one_shot: flag("one-shot"),
            layer: if flag("front") { "front" } else { "back" },
            size,
            shots: Vec::new(),
            pops: Vec::new(),
            avoid: Vec::new(),
            rounds: 0,
        };
        app.round();
        ptyrc_app::period(50);
        app
    }

    fn tick(&mut self, now: u64) {
        while self.shots.last().is_some_and(|s| s.0 <= now) {
            let (_, color) = self.shots.pop().unwrap_or_default();
            self.shoot(now, color);
        }
        if !self.one_shot && now >= self.rounds * PERIOD {
            self.round();
        }
        self.pops.retain_mut(|pop| {
            let frame = ((now - pop.start) / STEP) as usize;
            if frame >= POP.len() {
                return false;
            }
            if pop.shown != Some(frame) {
                pop.shown = Some(frame);
                draw(pop, frame);
            }
            true
        });
        if self.one_shot && self.shots.is_empty() && self.pops.is_empty() {
            ptyrc_app::exit(0);
        }
    }

    fn event(&mut self, topic: &str, data: Value) {
        let n = |i: usize| data.pointer(&format!("/size/{i}")).and_then(Value::as_u64);
        if let ("size", Some(c), Some(r)) = (topic, n(0), n(1)) {
            self.size = (c as u32, r as u32);
        }
    }
}

ptyrc_app::app!(Confetti);
