// SPDX-License-Identifier: GPL-3.0-or-later
//! Snake: plays alone between the words; the text is the wall.
//!
//! Each step reads `screen.shape` (`#` where there is ink). The apple's
//! distance field (breadth-first over the cells without ink) is begun
//! again when the apple or the text changes, and grows by `BUDGET`
//! cells a step; the head goes to the free neighbour nearest the apple,
//! else to a free neighbour drawn at random, else it sheds its tail.
//! An apple the finished field does not join to the head is planted
//! again. Text written on the head: the snake
//! is lost and comes back elsewhere.
//!
//! `shape` denied: a blank screen (`"degree": "none"`): it plays on.
//! `grab`: the arrows steer it; `keys-arrows` denied: it plays alone.
//! The twin of `sdk/js/examples/snake`: the same seed, the same game.

use ptyrc_app::{App, Canvas, Cell, Color, Value, attr, call, json, log, random_below};

const ARROWS: [(&str, (i32, i32)); 4] = [
    ("right", (1, 0)),
    ("left", (-1, 0)),
    ("down", (0, 1)),
    ("up", (0, -1)),
];
const STEPS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
/// Cells of the field a step, as the script twin needs.
const BUDGET: usize = 2000;

struct Snake {
    w: usize,
    h: usize,
    canvas: Canvas,
    /// Rows of `screen.shape`.
    lines: Vec<String>,
    /// 1: ink.
    solid: Vec<u8>,
    /// Cell indices, the head first.
    body: Vec<usize>,
    /// Body segments per cell.
    occ: Vec<u8>,
    grow: u32,
    apple: Option<usize>,
    /// Steps to the apple; -1: none.
    dist: Vec<i32>,
    /// The cells of the field still to grow from: `queue[qh..]`.
    queue: Vec<usize>,
    qh: usize,
    player: bool,
    heading: Option<(i32, i32)>,
    told_blank: bool,
}

fn canvas(w: usize, h: usize) -> Canvas {
    Canvas::new(w as u32, h as u32).unwrap_or_else(|| ptyrc_app::exit(1))
}

impl Snake {
    fn resize(&mut self, (w, h): (u32, u32)) {
        (self.w, self.h) = (w as usize, h as usize);
        self.canvas = canvas(self.w, self.h);
        self.occ = vec![0; self.w * self.h];
        self.solid = vec![0; self.w * self.h];
        self.dist = vec![-1; self.w * self.h];
        self.queue.clear();
        self.qh = 0;
        self.body.clear();
        self.apple = None;
        self.lines = vec!["\0".into()]; // never a shape: read again
    }

    fn free(&self, i: usize) -> bool {
        self.occ[i] == 0 && self.solid[i] == 0
    }

    fn random_free(&self) -> Option<usize> {
        for _ in 0..200 {
            let x = random_below(self.w as u32) as usize;
            let y = random_below(self.h as u32) as usize;
            if self.free(y * self.w + x) {
                return Some(y * self.w + x);
            }
        }
        None
    }

    /// Begins the field of `goal`.
    fn begin(&mut self, goal: usize) {
        self.dist.fill(-1);
        self.dist[goal] = 0;
        self.queue.clear();
        self.queue.push(goal);
        self.qh = 0;
    }

    /// Grows the field by `BUDGET` cells at most.
    fn grow_field(&mut self) {
        let (w, n) = (self.w, self.w * self.h);
        for _ in 0..BUDGET {
            let Some(&i) = self.queue.get(self.qh) else {
                break;
            };
            self.qh += 1;
            let d = self.dist[i] + 1;
            let x = i % w;
            let near = [
                (x + 1 < w).then(|| i + 1),
                (x > 0).then(|| i - 1),
                (i + w < n).then(|| i + w),
                i.checked_sub(w),
            ];
            for j in near.into_iter().flatten() {
                if self.dist[j] < 0 && self.solid[j] == 0 {
                    self.dist[j] = d;
                    self.queue.push(j);
                }
            }
        }
    }

    fn done(&self) -> bool {
        self.qh >= self.queue.len()
    }

    fn plant(&mut self) {
        self.apple = self.random_free();
        if let Some(a) = self.apple {
            self.begin(a);
        }
    }

    fn cut(&mut self, i: usize) {
        for c in self.body.drain(i..) {
            self.occ[c] -= 1;
        }
    }

    fn look(&mut self) {
        let answer = call("screen.shape", Value::Null).unwrap_or(Value::Null);
        if answer["degree"] == "none" && !self.told_blank {
            self.told_blank = true;
            log("snake: shape denied: the text is not seen; it plays on a blank screen");
        }
        let lines: Vec<String> = answer["lines"]
            .as_array()
            .map(|a| a.iter().map(|l| l.as_str().unwrap_or("").into()).collect())
            .unwrap_or_default();
        if lines == self.lines {
            return;
        }
        self.solid.fill(0);
        for (y, line) in lines.iter().enumerate().take(self.h) {
            for (x, &b) in line.as_bytes().iter().enumerate().take(self.w) {
                self.solid[y * self.w + x] = u8::from(b == b'#');
            }
        }
        self.lines = lines;
        if let Some(a) = self.apple {
            self.begin(a);
        }
    }

    /// The next cell of the head.
    fn choose(&self, head: usize) -> Option<usize> {
        let (x, y) = ((head % self.w) as i32, (head / self.w) as i32);
        let near: Vec<usize> = STEPS
            .iter()
            .map(|(dx, dy)| (x + dx, y + dy))
            .filter(|&(x, y)| x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h)
            .map(|(x, y)| y as usize * self.w + x as usize)
            .filter(|&c| self.free(c))
            .collect();
        let mut best: Option<usize> = None;
        for &c in &near {
            if self.dist[c] >= 0 && best.is_none_or(|b| self.dist[c] < self.dist[b]) {
                best = Some(c);
            }
        }
        best.or_else(|| (!near.is_empty()).then(|| near[random_below(near.len() as u32) as usize]))
    }

    /// The player's move: along `heading`; none against ink or the side.
    fn steer(&mut self, head: usize, (dx, dy): (i32, i32)) -> Option<usize> {
        let x = (head % self.w) as i32 + dx;
        let y = (head / self.w) as i32 + dy;
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return None;
        }
        let to = y as usize * self.w + x as usize;
        if self.solid[to] != 0 {
            return None;
        }
        if let Some(k) = self.body.iter().position(|&c| c == to)
            && k > 0
            && !(k == self.body.len() - 1 && self.grow == 0)
        {
            self.cut(k);
        }
        Some(to)
    }

    fn step(&mut self) {
        self.look();
        if self.body.is_empty() {
            let Some(s) = self.random_free() else { return };
            self.body.push(s);
            self.occ[s] += 1;
            self.grow = 3;
            self.apple = None;
        }
        let head = self.body[0];
        if self.solid[head] != 0 {
            self.cut(0); // written on: lost
            return;
        }
        match self.apple {
            Some(a) if self.solid[a] == 0 => {
                if self.done() && self.dist[head] < 0 {
                    self.plant(); // cut off
                }
            }
            _ => self.plant(),
        }
        if self.apple.is_none() {
            return;
        }
        self.grow_field();
        let to = match self.heading.filter(|_| self.player) {
            Some(way) => match self.steer(head, way) {
                Some(to) => to,
                None => return, // it waits
            },
            None => match self.choose(head) {
                Some(to) => to,
                None => {
                    self.cut(1); // cornered
                    self.grow = 3;
                    return;
                }
            },
        };
        self.body.insert(0, to);
        self.occ[to] += 1;
        if Some(to) == self.apple {
            self.grow += 2;
            self.apple = None;
        }
        if self.grow > 0 {
            self.grow -= 1;
        } else if let Some(t) = self.body.pop() {
            self.occ[t] -= 1;
        }
    }

    fn draw(&mut self) {
        let w = self.w as u32;
        let cell = |ch, fg, attrs| Cell {
            ch: Some(ch),
            fg: Color::Palette(fg),
            attrs,
            ..Cell::default()
        };
        self.canvas.clear();
        if let Some(a) = self.apple {
            self.canvas.set(a as u32 % w, a as u32 / w, cell('●', 9, 0));
        }
        for (k, &c) in self.body.iter().enumerate().rev() {
            let look = if k > 0 {
                cell('o', 2, 0)
            } else {
                cell('@', 10, attr::BOLD)
            };
            self.canvas.set(c as u32 % w, c as u32 / w, look);
        }
        if self.player {
            let x = w.saturating_sub(12);
            for (i, ch) in "←↑↓→ ctrl+t".chars().enumerate() {
                self.canvas.set(x + i as u32, 0, cell(ch, 8, 0));
            }
        }
        self.canvas
            .present(&json!({"anchor": {"at": [0, 0], "space": "view"}}));
    }
}

impl App for Snake {
    fn init(args: Value) -> Snake {
        let speed = args["speed"].as_u64().unwrap_or(12).clamp(1, 30) as u32;
        let size = ptyrc_app::size().unwrap_or((80, 24));
        let mut app = Snake {
            w: 1,
            h: 1,
            canvas: canvas(1, 1),
            lines: Vec::new(),
            solid: Vec::new(),
            body: Vec::new(),
            occ: Vec::new(),
            grow: 0,
            apple: None,
            dist: Vec::new(),
            queue: Vec::new(),
            qh: 0,
            player: false,
            heading: None,
            told_blank: false,
        };
        app.resize(size);
        if let Err(e) = ptyrc_app::subscribe(&["size"]) {
            log(&format!("snake: {e}"));
        }
        if args["grab"].as_bool() == Some(true) {
            let keys: Vec<&str> = ARROWS.iter().map(|a| a.0).collect();
            match call("input.grab", json!({"keys": keys})) {
                Ok(_) => app.player = true,
                Err(e) => log(&format!("snake: the arrows: {}; it plays alone", e.code)),
            }
        }
        let ms = if app.player { 2000 } else { 1000 };
        ptyrc_app::period((f64::from(ms) / f64::from(speed)).round() as u32);
        app.step();
        app.draw();
        app
    }

    fn tick(&mut self, _now_ms: u64) {
        self.step();
        self.draw();
    }

    fn event(&mut self, topic: &str, data: Value) {
        let n = |i: usize| data["size"][i].as_u64().map(|n| n as u32);
        match topic {
            "size" => {
                if let (Some(c), Some(r)) = (n(0), n(1)) {
                    self.resize((c, r));
                }
            }
            "input" if !data["released"].is_null() => ptyrc_app::exit(0),
            "input" => {
                let key = data["key"].as_str().unwrap_or("");
                let Some(&(_, (dx, dy))) = ARROWS.iter().find(|a| a.0 == key) else {
                    return;
                };
                let Some(&head) = self.body.first() else {
                    return;
                };
                let x = (head % self.w) as i32 + dx;
                let y = (head / self.w) as i32 + dy;
                let back = self
                    .body
                    .get(1)
                    .is_some_and(|&n| x >= 0 && y >= 0 && y as usize * self.w + x as usize == n);
                if !back {
                    self.heading = Some((dx, dy));
                }
            }
            _ => {}
        }
    }

    fn stop(&mut self) {
        if self.player {
            let _ = call("input.release", Value::Null);
        }
    }
}

ptyrc_app::app!(Snake);
