// SPDX-License-Identifier: GPL-3.0-or-later
//! The cats and their home, pure: the twin's `Plate`, `Mouse`, `Fish`,
//! `Cat`, `Home`. What is drawn goes through a [`Stage`]; draws come
//! from a [`Twister`], the twin's `random.Random`. Cells are frame
//! cells; the frame's origin is 0, 0.

use std::collections::{HashSet, VecDeque};

use crate::look::{
    self, BW, COATS, Coat, EARS, EYES, FW, H, Look, MW, NOSES, PH, PLAIN, PW, Seg, Skin, W,
};
use crate::place::{Ledge, arc, hypot, ledges, mass, near, perch, set_order};
use crate::twister::Twister;
use crate::yarn::{Drawn, Ground, Yarn};

pub const STEP: f64 = 0.13;
pub const HOP: f64 = 1.0 / 30.0;
pub const RISE: f64 = 0.05;
pub const READ: f64 = 0.3;
pub const AWAY: (f64, f64) = (2.0, 4.0);
pub const COVERED: f64 = 0.2;
pub const GAP: i64 = 3;
pub const BITE: f64 = 2.5;
pub const EMPTY: f64 = 3.0;
pub const PLATES: usize = 3;
pub const SERVED: (f64, f64) = (25.0, 45.0);
pub const FOOD: (f64, f64) = (20.0, 50.0);
pub const MICE: (f64, f64) = (15.0, 40.0);
pub const VISIT: (f64, f64) = (10.0, 20.0);
pub const SCURRY: (f64, f64) = (0.1, 0.045);
pub const WITS: (f64, f64) = (0.05, 0.55);
pub const TRIES: i64 = 2;
pub const RUN: f64 = 0.05;
pub const EASE: [f64; 3] = [0.1, 1.0 / 12.0, 1.0 / 15.0];
pub const NOTICE: f64 = 0.3;
pub const FUN: f64 = 8.0;
pub const BORED: f64 = 0.15;
pub const POUNCE: i64 = 14;
pub const REACH: i64 = 3;
pub const POKE: (i64, i64) = (12, 3);
pub const STEALS: i64 = 2;
pub const LYING: (f64, f64) = (20.0, 40.0);
pub const FISHES: (f64, f64) = (25.0, 60.0);
pub const TOYS: (f64, f64) = (25.0, 60.0);
pub const UNDONE: f64 = 2.5;
pub const LEAD: i64 = 2;
pub const WORDS: [&str; 3] = ["food", "fish", "toy"];

const INF: f64 = f64::INFINITY;
const RESTING: [&str; 6] = ["sit", "groom", "alert", "stare", "puzzled", "purr"];

/// A box of cells, `(x0, y0, x1, y1)`, ends excluded.
pub type Area = (i64, i64, i64, i64);

/// What the world draws on; ids are the stage's.
pub trait Stage {
    /// Draw at `(x, y)` (frame cells), or move and redraw `id`.
    fn put(&mut self, id: Option<u64>, x: i64, y: i64, look: &Look, z: i64) -> u64;
    /// New content for `id`, in place.
    fn set(&mut self, id: u64, look: &Look);
    fn remove(&mut self, id: u64);
    /// `(visible, covered)` of each, `None`: not listed.
    fn infos(&mut self, ids: &[u64]) -> Vec<Option<(bool, f64)>>;
    /// What is put from `begin` to `end`: one request.
    fn begin(&mut self);
    fn end(&mut self);
    /// A blank sprite over the view, `w` by `h`, at z -9.
    fn mat(&mut self, w: i64, h: i64) -> u64;
    /// The view as read: rows (`#` ink, the words as shown), its box
    /// `(x0, y0, x1, y1)` in frame cells. `None`: it cannot be drawn on.
    fn view(&mut self) -> Option<(Vec<String>, Area)>;
}

/// The view as read.
#[derive(Clone, Debug, Default)]
pub struct Room {
    pub rows: Vec<Vec<char>>,
    pub walls: Vec<Vec<bool>>,
    /// `(x0, y0, x1, y1)`.
    pub view: (i64, i64, i64, i64),
}

impl Ground for Room {
    fn solid(&self, x: i64, y: i64) -> bool {
        let (x0, y0, x1, y1) = self.view;
        if !(x0 <= x && x < x1) || y >= y1 {
            return true;
        }
        y >= y0 && self.walls[(y - y0) as usize][(x - x0) as usize]
    }

    fn top(&self) -> i64 {
        self.view.1
    }
}

pub type Cell = (i64, i64);

#[derive(Clone, Debug)]
pub struct Plate {
    pub key: u64,
    pub x: i64,
    pub g: i64,
    pub word: Option<Cell>,
    pub until: f64,
    pub left: usize,
    /// Who eats there, by the side it faces; in the order seated.
    pub seats: Vec<(bool, usize)>,
    pub sprite: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Mouse {
    pub g: i64,
    pub lo: i64,
    pub hi: i64,
    pub x: i64,
    pub goal: i64,
    pub right: bool,
    pub still: bool,
    pub pace: f64,
    pub out: bool,
    pub due: f64,
    pub leave: f64,
    pub fright: f64,
    pub threat: i64,
    pub sprite: Option<u64>,
}

impl Mouse {
    fn head(&mut self, goal: i64, out: bool) {
        self.goal = self.lo.max(goal.min(self.hi));
        self.out = out;
        self.right = self.goal >= self.x;
    }
}

#[derive(Clone, Debug)]
pub struct Fish {
    pub x: i64,
    pub y: i64,
    pub word: Option<Cell>,
    pub until: f64,
    pub left: usize,
    pub holder: Option<usize>,
    pub steals: i64,
    pub right: bool,
    pub sprite: Option<u64>,
}

impl Fish {
    pub fn g(&self) -> i64 {
        self.y + 1
    }
}

/// The ball of yarn and what the home knows of it.
#[derive(Clone, Debug)]
pub struct Toy {
    pub yarn: Yarn,
    pub word: Option<Cell>,
    pub until: f64,
    pub player: Option<usize>,
    pub ball: Option<u64>,
    pub wire: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Cat {
    pub coat: Coat,
    pub skin: Skin,
    pub z: i64,
    pub sprite: Option<u64>,
    pub x: i64,
    pub g: i64,
    pub right: bool,
    pub act: &'static str,
    pub due: f64,
    pub goal: i64,
    pub stride: i64,
    pub path: VecDeque<Cell>,
    pub beat: f64,
    pub landing: Option<Cell>,
    /// The key of its plate.
    pub meal: Option<u64>,
    /// It has the home's fish.
    pub fish: bool,
    /// It plays with the home's ball.
    pub toy: bool,
    pub prey: Option<&'static str>,
    pub tries: i64,
    pub fun: f64,
    pub mused: f64,
    pub ignored: Option<Cell>,
    pub spooked: bool,
}

impl Cat {
    fn new(coat: Coat, skin: Skin, z: i64) -> Cat {
        Cat {
            coat,
            skin,
            z,
            sprite: None,
            x: 0,
            g: 0,
            right: true,
            act: "away",
            due: 0.0,
            goal: 0,
            stride: 0,
            path: VecDeque::new(),
            beat: HOP,
            landing: None,
            meal: None,
            fish: false,
            toy: false,
            prey: None,
            tries: 0,
            fun: 0.0,
            mused: 0.0,
            ignored: None,
            spooked: false,
        }
    }

    /// The places it holds: where it is, where it goes.
    pub fn claims(&self) -> Vec<Cell> {
        if self.act == "away" {
            return Vec::new();
        }
        let mut out = vec![(self.x, self.g)];
        if self.act == "leap" {
            out.extend(self.landing);
        } else if self.act == "walk" || self.act == "run" {
            out.push((self.goal, self.g));
        }
        out
    }

    /// The pointer at `p` is in its box, or next to it.
    pub fn holds(&self, p: Option<Cell>) -> bool {
        p.is_some_and(|p| {
            self.x - 1 <= p.0 && p.0 <= self.x + W && self.g - H <= p.1 && p.1 < self.g
        })
    }

    /// The rows up to the pointer over its box, when it can leap that
    /// high; else 0.
    pub fn under(&self, p: Option<Cell>) -> i64 {
        let Some(p) = p else { return 0 };
        if !(self.x <= p.0 && p.0 < self.x + W) {
            return 0;
        }
        let rise = self.g - H - p.1;
        if 0 < rise && rise <= REACH { rise } else { 0 }
    }

    fn pace(&self) -> f64 {
        if self.act == "walk" {
            return STEP;
        }
        let i = (self.stride - 1).min((self.goal - self.x).abs());
        if (0..3).contains(&i) {
            EASE[i as usize]
        } else {
            RUN
        }
    }

    fn after(&mut self, now: f64, seconds: f64) {
        let late = now - self.due;
        let off = if 0.0 <= late && late < seconds / 2.0 {
            late
        } else {
            0.0
        };
        self.due = now + seconds - off;
    }

    fn amuse(&mut self, now: f64) {
        self.fun *= (-(0.0f64.max(now - self.mused)) / FUN).exp();
        self.mused = now;
    }

    fn jump(&mut self, to: Cell, now: f64, start: Cell) {
        self.act = "leap";
        self.due = now;
        self.landing = Some(to);
        self.right = to.0 >= start.0;
        self.path = arc(start, to).into();
        self.beat = HOP;
    }

    fn go(&mut self, act: &'static str, goal: i64, now: f64) {
        self.act = act;
        self.goal = goal;
        self.due = now;
        self.stride = 0;
        self.right = goal >= self.x;
    }
}

/// The options of a home.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kinds {
    pub count: usize,
    pub varied: bool,
    pub food: bool,
    pub mice: bool,
    pub fish: bool,
    pub toys: bool,
    pub hunt: bool,
}

pub struct Home<S: Stage> {
    pub stage: S,
    pub rng: Twister,
    pub idle: bool,
    pub cursor: Option<Cell>,
    pub room: Room,
    pub spots: Vec<Ledge>,
    pub read: f64,
    pub stale: bool,
    pub cats: Vec<Cat>,
    pub plates: Vec<Plate>,
    keys: u64,
    pub eaten: HashSet<Cell>,
    pub mouse: Option<Mouse>,
    pub next_food: f64,
    pub next_mouse: f64,
    pub fish: Option<Fish>,
    pub toy: Option<Toy>,
    pub next_fish: f64,
    pub next_toy: f64,
    pub hunt: bool,
    pub pointer: Option<Cell>,
    pub mat: Option<(u64, i64, i64)>,
    joint: bool,
}

/// The first of `items` least by `less`.
fn least<T: Clone>(items: &[T], less: impl Fn(&T, &T) -> bool) -> Option<T> {
    let mut best: Option<&T> = None;
    for it in items {
        if best.is_none_or(|b| less(it, b)) {
            best = Some(it);
        }
    }
    best.cloned()
}

/// The words of `rows` at their cells: `\b(food|fish|toy)\b`, any case.
fn words_of(rows: &[Vec<char>], view: (i64, i64, i64, i64)) -> Vec<(Cell, &'static str)> {
    let wordy = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        let mut x = 0;
        while x < row.len() {
            let start_ok = x == 0 || !wordy(row[x - 1]);
            let hit = start_ok
                .then(|| {
                    WORDS.iter().find(|w| {
                        let n = w.chars().count();
                        x + n <= row.len()
                            && row[x..x + n]
                                .iter()
                                .zip(w.chars())
                                .all(|(a, b)| a.to_lowercase().eq(std::iter::once(b)))
                            && (x + n == row.len() || !wordy(row[x + n]))
                    })
                })
                .flatten();
            if let Some(w) = hit {
                out.push(((view.0 + x as i64, view.1 + y as i64), *w));
                x += w.len();
            } else {
                x += 1;
            }
        }
    }
    out
}

impl<S: Stage> Home<S> {
    pub fn new(stage: S, mut rng: Twister, k: &Kinds, now: f64) -> Home<S> {
        let coats: Vec<Coat> = if k.varied {
            rng.sample(&COATS, COATS.len())
        } else {
            COATS.to_vec()
        };
        let mut cats = Vec::new();
        for i in 0..k.count {
            let mut skin = PLAIN;
            if k.varied {
                skin = Skin {
                    ears: rng.choice(&EARS),
                    eyes: rng.choice(&EYES),
                    nose: rng.choice(&NOSES),
                };
            }
            let mut cat = Cat::new(coats[i % coats.len()], skin, i as i64);
            cat.due = if i > 0 {
                now + i as f64 * rng.uniform(0.5, 1.5)
            } else {
                0.0
            };
            cats.push(cat);
        }
        let mut at = |on: bool, a: f64, b: f64| if on { now + rng.uniform(a, b) } else { INF };
        let next_food = at(k.food, 4.0, 10.0);
        let next_mouse = at(k.mice, 3.0, 8.0);
        let next_fish = at(k.fish, 5.0, 12.0);
        let next_toy = at(k.toys, 6.0, 14.0);
        Home {
            stage,
            rng,
            idle: false,
            cursor: None,
            room: Room::default(),
            spots: Vec::new(),
            read: 0.0,
            stale: true,
            cats,
            plates: Vec::new(),
            keys: 0,
            eaten: HashSet::new(),
            mouse: None,
            next_food,
            next_mouse,
            fish: None,
            toy: None,
            next_fish,
            next_toy,
            hunt: k.hunt,
            pointer: None,
            mat: None,
            joint: false,
        }
    }

    // -- drawing -----------------------------------------------------

    fn open(&mut self) -> bool {
        if self.joint {
            return false;
        }
        self.joint = true;
        self.stage.begin();
        true
    }

    fn close(&mut self, opened: bool) {
        if opened {
            self.stage.end();
            self.joint = false;
        }
    }

    fn put(&mut self, id: Option<u64>, x: i64, y: i64, look: &Look, z: i64) -> u64 {
        if id.is_none() {
            return self.stage.put(None, x, y, look, z);
        }
        let o = self.open();
        let id = self.stage.put(id, x, y, look, z);
        self.close(o);
        id
    }

    fn yarn_stale(&mut self) {
        if let Some(t) = &mut self.toy
            && t.yarn.stale
        {
            t.yarn.stale = false;
            self.stale = true;
        }
    }

    fn put_drawn(&mut self, d: Drawn) {
        if d.wire.is_none() && d.ball.is_none() {
            return;
        }
        let o = self.open();
        if let Some((x, y, l)) = d.wire {
            let id = self.toy.as_ref().and_then(|t| t.wire);
            let id = self.put(id, x, y, &l, -3);
            if let Some(t) = &mut self.toy {
                t.wire = Some(id);
            }
        }
        if let Some((x, y, l)) = d.ball {
            let id = self.toy.as_ref().and_then(|t| t.ball);
            let id = self.put(id, x, y, &l, -2);
            if let Some(t) = &mut self.toy {
                t.ball = Some(id);
            }
        }
        self.close(o);
    }

    // -- the place ---------------------------------------------------

    /// Read the view, find its ledges, lay the things of its words;
    /// false while it cannot be drawn on.
    pub fn survey(&mut self, now: f64) -> bool {
        let Some((shape, view)) = self.stage.view() else {
            return false;
        };
        let (w, h) = (view.2 - view.0, view.3 - view.1);
        let mut rows: Vec<String> = shape.into_iter().take(h as usize).collect();
        rows.resize(h as usize, String::new());
        let rows = look::pad(&rows, w as usize);
        self.spots = ledges(&rows, view);
        let rows: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
        self.room.walls = rows.iter().map(|r| mass(r)).collect();
        self.room.rows = rows;
        self.room.view = view;
        self.read = now;
        self.stale = false;
        if self.hunt {
            self.carpet(w, h);
        }
        self.lay(now);
        true
    }

    fn fresh(&mut self, now: f64) {
        if self.stale || now - self.read > 2.0 {
            self.survey(now);
        }
    }

    fn carpet(&mut self, w: i64, h: i64) {
        if let Some((id, mw, mh)) = self.mat {
            if (mw, mh) == (w, h) {
                return;
            }
            self.stage.remove(id);
        }
        let id = self.stage.mat(w, h);
        self.mat = Some((id, w, h));
    }

    /// The box at `(x, y)` is in the view as read, with no text.
    pub fn free(&self, x: i64, y: i64, w: i64, h: i64) -> bool {
        let (x0, y0, x1, y1) = self.room.view;
        if x < x0 || y < y0 || x + w > x1 || y + h > y1 {
            return false;
        }
        let rows = &self.room.rows[(y - y0) as usize..(y - y0 + h) as usize];
        rows.iter().all(|r| {
            r[(x - x0) as usize..(x - x0 + w) as usize]
                .iter()
                .all(|&c| look::is_space(c))
        })
    }

    /// Another cat holds cells of the box at `x` above `g`, or next to it.
    pub fn taken(&self, c: usize, x: i64, g: i64) -> bool {
        self.cats.iter().enumerate().any(|(i, o)| {
            i != c
                && o.claims()
                    .iter()
                    .any(|&(ox, og)| (x - ox).abs() <= W && (g - og).abs() < H)
        })
    }

    /// Something new to see: the cats at rest look soon; of those
    /// asleep, a share.
    fn call(&mut self, now: f64, asleep: f64) {
        for i in 0..self.cats.len() {
            let act = self.cats[i].act;
            if RESTING.contains(&act)
                || ((act == "sleep" || act == "yawn") && self.rng.random() < asleep)
            {
                let d = now + self.rng.uniform(0.2, 1.2);
                self.cats[i].due = self.cats[i].due.min(d);
            }
        }
    }

    // -- things on the floor -----------------------------------------

    fn things(&self) -> Vec<(i64, i64, i64)> {
        let mut out: Vec<(i64, i64, i64)> = self.plates.iter().map(|p| (p.x, p.g, PW)).collect();
        if let Some(f) = &self.fish
            && f.holder.is_none_or(|h| self.cats[h].act != "leap")
        {
            out.push((f.x, f.g(), FW));
        }
        if let Some(t) = &self.toy {
            out.push((t.yarn.x(), t.yarn.g(), BW));
        }
        out
    }

    fn chairs(&self, x: i64, g: i64, w: i64) -> Vec<(bool, i64)> {
        [(true, x - W), (false, x + w)]
            .into_iter()
            .filter(|&(_, cx)| self.free(cx, g - H, W, H))
            .collect()
    }

    fn room_for(&self, x: i64, g: i64, w: i64) -> bool {
        self.free(x, g - 1, w, 1)
            && !self
                .things()
                .iter()
                .any(|&(tx, tg, tw)| tg == g && x < tx + tw + 1 && tx < x + w + 1)
    }

    fn laid(&self, x: i64, g: i64) -> bool {
        self.things()
            .iter()
            .any(|&(tx, tg, tw)| (g - tg).abs() < H && x < tx + tw && tx < x + W)
    }

    fn beside(&self, x: i64, y: i64, n: i64, w: i64) -> Option<Cell> {
        let (x0, _, x1, _) = self.room.view;
        let over = x0.max((x + (n - w).div_euclid(2)).min(x1 - w));
        let sites: Vec<Cell> = [(x + n + GAP, y + 1), (x - w - GAP, y + 1), (over, y)]
            .into_iter()
            .filter(|s| self.room_for(s.0, s.1, w))
            .collect();
        sites
            .iter()
            .find(|s| !self.chairs(s.0, s.1, w).is_empty())
            .or(sites.first())
            .copied()
    }

    fn site(&mut self, w: i64) -> Option<Cell> {
        let spots = self.spots.clone();
        for s in self.rng.sample(&spots, spots.len()) {
            let x = self.rng.randint(s.x0, s.x1 + W - w);
            let busy = self.cats.iter().any(|c| {
                c.claims()
                    .iter()
                    .any(|&(ox, og)| og == s.g && ox < x + w + 1 && x < ox + W + 1)
            });
            if busy || near(x, s.g, self.cursor) || !self.room_for(x, s.g, w) {
                continue;
            }
            if !self.chairs(x, s.g, w).is_empty() {
                return Some((x, s.g));
            }
        }
        None
    }

    fn plate(&self, key: u64) -> Option<usize> {
        self.plates.iter().position(|p| p.key == key)
    }

    fn dish(&mut self, at: Cell, word: Option<Cell>, until: f64) {
        self.keys += 1;
        let left = look::PLATE.len() - 1;
        let id = self.put(None, at.0, at.1 - PH, &look::plate_look(left), -1);
        self.plates.push(Plate {
            key: self.keys,
            x: at.0,
            g: at.1,
            word,
            until,
            left,
            seats: Vec::new(),
            sprite: Some(id),
        });
    }

    fn lay(&mut self, now: f64) {
        let words = words_of(&self.room.rows, self.room.view);
        let has = |w: &Option<Cell>| w.is_none_or(|w| words.iter().any(|(c, _)| *c == w));
        let lost = |home: &Home<S>, word: &Option<Cell>, x: i64, g: i64, w: i64| {
            !has(word) || !home.free(x, g - 1, w, 1)
        };
        for key in self.plates.iter().map(|p| p.key).collect::<Vec<_>>() {
            let p = &self.plates[self.plate(key).unwrap()];
            if lost(self, &p.word, p.x, p.g, PW) {
                self.clear(key, now);
            }
        }
        if let Some(f) = &self.fish
            && f.holder.is_none()
            && f.left > 0
            && lost(self, &f.word, f.x, f.g(), FW)
        {
            self.lose(now);
        }
        if let Some(t) = &self.toy {
            let gone = !has(&t.word);
            if gone || (!t.yarn.moving && lost(self, &None, t.yarn.x(), t.yarn.g(), BW)) {
                self.wind(now);
            } else if let Some(t) = &mut self.toy {
                t.yarn.wake(&self.room, now);
            }
        }
        self.eaten.retain(|c| words.iter().any(|(w, _)| w == c));
        let mut laid: Vec<Cell> = self.plates.iter().filter_map(|p| p.word).collect();
        laid.extend(self.eaten.iter().copied());
        laid.extend(self.fish.as_ref().and_then(|f| f.word));
        laid.extend(self.toy.as_ref().and_then(|t| t.word));
        for &(word, kind) in &words {
            if laid.contains(&word) {
                continue;
            }
            let (w, full) = match kind {
                "fish" => (FW, self.fish.is_some()),
                "toy" => (BW, self.toy.is_some()),
                _ => (PW, self.plates.len() >= PLATES),
            };
            if full {
                continue;
            }
            let Some(at) = self.beside(word.0, word.1, kind.len() as i64, w) else {
                continue;
            };
            match kind {
                "food" => {
                    self.dish(at, Some(word), INF);
                    self.call(now, 1.0);
                }
                "fish" => self.drop(at, Some(word), INF),
                _ => self.spool(at, Some(word), INF),
            }
        }
        if let Some(m) = &self.mouse
            && !self
                .spots
                .iter()
                .any(|s| s.g == m.g && s.x0 <= m.lo && m.hi <= s.x1 + W - MW)
        {
            self.shoo(now);
        }
    }

    // -- the plates --------------------------------------------------

    fn serve(&mut self, now: f64) {
        self.next_food = now + 5.0;
        if self.plates.len() >= PLATES || (self.stale && !self.survey(now)) {
            return;
        }
        if let Some(at) = self.site(PW) {
            let until = now + self.rng.uniform(SERVED.0, SERVED.1);
            self.dish(at, None, until);
            self.next_food = INF;
            self.call(now, 1.0);
        }
    }

    /// The nearest free side of a plate with food left.
    fn seat(&self, c: usize) -> Option<(u64, bool)> {
        let cat = &self.cats[c];
        let mut seats: Vec<(f64, usize, bool, u64)> = Vec::new();
        for (i, p) in self.plates.iter().enumerate() {
            if p.left == 0 {
                continue;
            }
            for (right, cx) in self.chairs(p.x, p.g, PW) {
                if !p.seats.iter().any(|s| s.0 == right)
                    && !near(cx, p.g, self.cursor)
                    && !self.taken(c, cx, p.g)
                {
                    seats.push((hypot(cx - cat.x, 2 * (p.g - cat.g)), i, right, p.key));
                }
            }
        }
        least(&seats, |a, b| (a.0, a.1, a.2) < (b.0, b.1, b.2)).map(|s| (s.3, s.2))
    }

    fn bite(&mut self, key: u64, now: f64) {
        let i = self.plate(key).unwrap();
        self.plates[i].left -= 1;
        let p = &self.plates[i];
        let id = self.put(p.sprite, p.x, p.g - PH, &look::plate_look(p.left), -1);
        let p = &mut self.plates[i];
        p.sprite = Some(id);
        if p.left == 0 {
            p.until = now + EMPTY;
            if let Some(w) = p.word {
                self.eaten.insert(w);
            }
        }
    }

    fn clear(&mut self, key: u64, now: f64) {
        let i = self.plate(key).unwrap();
        let p = self.plates.remove(i);
        if let Some(id) = p.sprite {
            self.stage.remove(id);
        }
        for &(_, c) in &p.seats {
            self.quit(c);
            if self.cats[c].act == "eat" {
                self.rest(c, "alert", now, Some(0.6));
            }
        }
        if p.word.is_none() {
            self.next_food = now + self.rng.uniform(FOOD.0, FOOD.1);
        }
    }

    // -- the fish ----------------------------------------------------

    fn fish_show(&mut self, flop: bool) {
        let Some(f) = &self.fish else { return };
        let l = look::fish_look(f.left, f.right, flop);
        let id = self.put(f.sprite, f.x, f.y, &l, -1);
        if let Some(f) = &mut self.fish {
            f.sprite = Some(id);
        }
    }

    fn fish_by(&mut self, c: usize, mouth: bool) {
        let cat = &self.cats[c];
        let (x, y, right) = (
            if cat.right { cat.x + W } else { cat.x - FW },
            cat.g - if mouth { 2 } else { 1 },
            cat.right,
        );
        if let Some(f) = &mut self.fish {
            (f.x, f.y, f.right) = (x, y, right);
        }
        self.fish_show(false);
    }

    fn drop(&mut self, at: Cell, word: Option<Cell>, until: f64) {
        self.fish = Some(Fish {
            x: at.0,
            y: at.1 - 1,
            word,
            until,
            left: look::FISH.len() - 1,
            holder: None,
            steals: 0,
            right: true,
            sprite: None,
        });
        self.fish_show(true);
        self.call(self.read, 0.5);
    }

    fn toss(&mut self, now: f64) {
        self.next_fish = now + 5.0;
        if self.fish.is_some() || (self.stale && !self.survey(now)) {
            return;
        }
        if let Some(at) = self.site(FW) {
            let until = now + self.rng.uniform(LYING.0, LYING.1);
            self.drop(at, None, until);
            self.next_fish = INF;
        }
    }

    /// The fish goes: eaten, lost, or never taken.
    fn lose(&mut self, now: f64) {
        let Some(f) = self.fish.take() else { return };
        if let Some(id) = f.sprite {
            self.stage.remove(id);
        }
        if let Some(h) = f.holder {
            self.cats[h].fish = false;
        }
        if f.word.is_none() {
            self.next_fish = now + self.rng.uniform(FISHES.0, FISHES.1);
        }
    }

    fn bait(&self, c: usize) -> Option<Cell> {
        let f = self.fish.as_ref()?;
        if f.holder.is_some() || f.left == 0 {
            return None;
        }
        let cat = &self.cats[c];
        let ends: Vec<(f64, i64)> = self
            .chairs(f.x, f.g(), FW)
            .into_iter()
            .filter(|&(_, cx)| !near(cx, f.g(), self.cursor) && !self.taken(c, cx, f.g()))
            .map(|(_, cx)| (hypot(cx - cat.x, 2 * (f.g() - cat.g)), cx))
            .collect();
        least(&ends, |a, b| a < b).map(|e| (e.1, f.g()))
    }

    fn loot(&mut self, c: usize) -> Option<Cell> {
        let f = self.fish.as_ref()?;
        let owner = f.holder?;
        if owner == c
            || self.cats[owner].act != "eat"
            || f.left == 0
            || f.steals >= STEALS
            || self.rng.random() < 0.5
        {
            return None;
        }
        let f = self.fish.as_ref()?;
        let x = if self.cats[owner].right {
            f.x + FW
        } else {
            f.x - W
        };
        let g = f.g();
        let free = self.free(x, g - H, W, H) && !self.taken(c, x, g);
        if !free || near(x, g, self.cursor) {
            return None;
        }
        self.fish.as_mut()?.steals += 1;
        Some((x, g))
    }

    fn nibble(&mut self, now: f64) -> bool {
        let Some(f) = &mut self.fish else {
            return false;
        };
        f.left -= 1;
        self.fish_show(false);
        let f = self.fish.as_mut().unwrap();
        if f.left > 0 {
            return true;
        }
        f.holder = None;
        f.until = now + EMPTY;
        if let Some(w) = f.word {
            self.eaten.insert(w);
        }
        false
    }

    // -- the ball of yarn --------------------------------------------

    fn spool(&mut self, at: Cell, word: Option<Cell>, until: f64) {
        let yarn = Yarn::new(&mut self.rng, at.0, at.1);
        self.toy = Some(Toy {
            yarn,
            word,
            until,
            player: None,
            ball: None,
            wire: None,
        });
        let d = self.toy.as_mut().unwrap().yarn.show(&self.room);
        self.put_drawn(d);
        self.call(self.read, 0.5);
    }

    fn throw(&mut self, now: f64) {
        self.next_toy = now + 5.0;
        if self.toy.is_some() || (self.stale && !self.survey(now)) {
            return;
        }
        if let Some(at) = self.site(BW) {
            let until = now + self.rng.uniform(LYING.0, LYING.1);
            self.spool(at, None, until);
            self.next_toy = INF;
        }
    }

    fn plaything(&self, c: usize) -> Option<i64> {
        let t = self.toy.as_ref()?;
        if t.yarn.moving || t.player.is_some_and(|p| p != c) {
            return None;
        }
        if t.player.is_none() && t.yarn.plays == 0 {
            return None;
        }
        let cat = &self.cats[c];
        let (yx, yg) = (t.yarn.x(), t.yarn.g());
        let mut sides = [yx - W, yx + BW];
        if 2 * cat.x + W > 2 * yx {
            sides.reverse();
        }
        sides.into_iter().find(|&x| {
            self.free(x, yg - H, W, H) && !self.taken(c, x, yg) && !near(x, yg, self.cursor)
        })
    }

    fn wind(&mut self, now: f64) {
        let Some(t) = self.toy.take() else { return };
        if let Some(c) = t.player {
            self.quit(c);
            if ["bat", "wiggle", "roll"].contains(&self.cats[c].act) {
                self.rest(c, "alert", now, Some(0.6));
            }
        }
        if let Some(id) = t.ball {
            self.stage.remove(id);
        }
        if let Some(id) = t.wire {
            match t.yarn.fade(&self.room) {
                None => self.stage.remove(id),
                Some(l) => self.stage.set(id, &l),
            }
        }
        match t.word {
            None => self.next_toy = now + self.rng.uniform(TOYS.0, TOYS.1),
            Some(w) if t.yarn.plays == 0 => {
                self.eaten.insert(w);
            }
            Some(_) => {}
        }
    }

    // -- the pointer -------------------------------------------------

    fn below(&self, c: usize, p: Cell) -> Option<Cell> {
        let (px, py) = p;
        let cat = &self.cats[c];
        let mut best: Option<(i64, f64, i64, i64)> = None;
        for s in &self.spots {
            let x = s.x0.max((px - W / 2).min(s.x1));
            if !(s.g - H - REACH <= py && py < s.g && x <= px && px < x + W) {
                continue;
            }
            if near(x, s.g, self.cursor) || self.laid(x, s.g) || self.taken(c, x, s.g) {
                continue;
            }
            let rise = 0.max(s.g - H - py);
            let cost = (rise, hypot(x - cat.x, 2 * (s.g - cat.g)), x, s.g);
            if best.is_none_or(|b| cost < b) {
                best = Some(cost);
            }
        }
        best.map(|b| (b.2, b.3))
    }

    /// A move of the host mouse: where it is in the frame; `None`: out
    /// of it.
    pub fn point(&mut self, at: Option<Cell>, now: f64) {
        let was = std::mem::replace(&mut self.pointer, at);
        if at != was {
            for c in 0..self.cats.len() {
                self.notice(c, was, now);
            }
        }
    }

    // -- the mouse ---------------------------------------------------

    fn loose(&mut self, now: f64) {
        self.next_mouse = now + 5.0;
        if self.stale && !self.survey(now) {
            return;
        }
        let long: Vec<Ledge> = self
            .spots
            .iter()
            .filter(|s| s.x1 + W - MW - s.x0 >= 16)
            .copied()
            .collect();
        if long.is_empty() {
            return;
        }
        let s = self.rng.choice(&long);
        let (lo, hi) = (s.x0, s.x1 + W - MW);
        let x = self.rng.choice(&[lo, hi]);
        let goal = self.rng.randint(lo, hi);
        let leave = now + self.rng.uniform(VISIT.0, VISIT.1);
        self.mouse = Some(Mouse {
            g: s.g,
            lo,
            hi,
            x,
            goal,
            right: goal >= x,
            still: false,
            pace: SCURRY.0,
            out: false,
            due: now,
            leave,
            fright: INF,
            threat: 0,
            sprite: None,
        });
        for c in &mut self.cats {
            c.tries = 0;
        }
        self.call(now, 0.5);
    }

    fn shoo(&mut self, now: f64) {
        if let Some(m) = self.mouse.take()
            && let Some(id) = m.sprite
        {
            self.stage.remove(id);
        }
        self.next_mouse = now + self.rng.uniform(MICE.0, MICE.1);
    }

    fn mouse_show(&mut self, step: Option<i64>) {
        let Some(m) = &self.mouse else { return };
        let l = look::mouse_look(m.right, step);
        let id = self.put(m.sprite, m.x, m.g - 1, &l, -1);
        if let Some(m) = &mut self.mouse {
            m.sprite = Some(id);
        }
    }

    fn mouse_step(&mut self, now: f64) {
        let rng = &mut self.rng;
        let Some(m) = &mut self.mouse else { return };
        if now >= m.fright {
            (m.fright, m.pace, m.still) = (INF, SCURRY.1, false);
            let away = 2 * m.x + MW >= 2 * m.threat + W;
            m.head(if away { m.hi } else { m.lo }, true);
        } else if m.still {
            m.still = false;
            if now >= m.leave {
                let near_lo = m.x - m.lo <= m.hi - m.x;
                m.head(if near_lo { m.lo } else { m.hi }, true);
            } else {
                let g = rng.randint(m.lo, m.hi);
                m.head(g, false);
            }
        }
        if m.x == m.goal {
            if m.out {
                return self.shoo(now);
            }
            m.still = true;
            self.mouse_show(None);
            let d = now + self.rng.uniform(0.6, 2.2);
            self.mouse.as_mut().unwrap().due = d;
            return;
        }
        m.x += if m.goal > m.x { 1 } else { -1 };
        let x = m.x;
        self.mouse_show(Some(x));
        let m = self.mouse.as_mut().unwrap();
        m.due = now + m.pace;
    }

    fn scent(&mut self, c: usize) -> Option<Cell> {
        let m = self.mouse.as_ref()?;
        if self.cats[c].tries >= TRIES {
            return None;
        }
        let mut ahead = 0;
        if m.x != m.goal {
            let way = if m.goal > m.x { 1 } else { -1 };
            ahead = self.rng.randint(0, 4) * way;
        }
        let m = self.mouse.as_ref()?;
        let cat = &self.cats[c];
        let behind = 2 * cat.x + W <= 2 * m.x + MW;
        let x = m.x + ahead + if behind { MW - W } else { 0 };
        let x = m.lo.max(x.min(m.hi + MW - W));
        if near(x, m.g, self.cursor) || self.taken(c, x, m.g) {
            return None;
        }
        Some((x, m.g))
    }

    fn caught(&self, c: usize) -> bool {
        let cat = &self.cats[c];
        self.mouse
            .as_ref()
            .is_some_and(|m| m.g == cat.g && m.x < cat.x + W && cat.x < m.x + MW)
    }

    // -- the cats ----------------------------------------------------

    fn show(&mut self, c: usize, pose: &'static str, step: Option<i64>) {
        let k = &self.cats[c];
        let content = look::look(pose, k.right, step, &k.coat, &k.skin);
        let o = self.open();
        let k = &self.cats[c];
        let id = self.put(k.sprite, k.x, k.g - H, &content, k.z);
        self.cats[c].sprite = Some(id);
        if self.cats[c].fish && self.cats[c].act == "leap" {
            self.fish_by(c, true);
        }
        self.close(o);
    }

    fn rest(&mut self, c: usize, pose: &'static str, now: f64, seconds: Option<f64>) {
        let k = &mut self.cats[c];
        k.act = pose;
        k.due = seconds.map_or(INF, |s| now + s);
        self.show(c, pose, None);
    }

    fn jump(&mut self, c: usize, to: Cell, now: f64) {
        let k = &mut self.cats[c];
        k.jump(to, now, (k.x, k.g));
    }

    fn pick(&mut self, c: usize, far: bool) -> Option<(i64, Ledge)> {
        let mut spots = Vec::new();
        for s in self.spots.clone() {
            let r = self.rng.randint(s.x0, s.x1);
            for x in set_order(&[s.x0, s.x1, r]) {
                if !near(x, s.g, self.cursor) && !self.taken(c, x, s.g) && !self.laid(x, s.g) {
                    spots.push((x, s));
                }
            }
        }
        if spots.is_empty() {
            return None;
        }
        let some = self.rng.sample(&spots, spots.len().min(6));
        if !far {
            return Some(some[0]);
        }
        let k = &self.cats[c];
        let d = |p: &(i64, Ledge)| hypot(p.0 - k.x, 2 * (p.1.g - k.g));
        least(&some, |a, b| d(a) > d(b))
    }

    /// Leave the plate and the ball, forget the mouse.
    fn quit(&mut self, c: usize) {
        if let Some(key) = self.cats[c].meal
            && let Some(i) = self.plate(key)
        {
            self.plates[i].seats.retain(|s| s.1 != c);
        }
        if self.cats[c].toy
            && let Some(t) = &mut self.toy
            && t.player == Some(c)
        {
            t.player = None;
        }
        let k = &mut self.cats[c];
        (k.meal, k.toy, k.prey) = (None, false, None);
    }

    fn flee(&mut self, c: usize, now: f64) {
        if ["leap", "away"].contains(&self.cats[c].act) {
            return;
        }
        if self.stale || now - self.read > 2.0 {
            self.survey(now);
        }
        if let Some(to) = self.pick(c, true) {
            self.quit(c);
            self.jump(c, (to.0, to.1.g), now);
        }
    }

    fn arrive(&mut self, c: usize, now: f64) {
        let fresh = !self.stale && !self.spots.is_empty() && now - self.read < 1.0;
        let to = if fresh || self.survey(now) {
            self.pick(c, false)
        } else {
            None
        };
        let Some(to) = to else {
            self.cats[c].due = now + 0.3;
            return;
        };
        let (x, g) = (to.0, to.1.g);
        let top = (self.room.view.1 + H).max(g - 6);
        let k = &mut self.cats[c];
        k.jump((x, g), now, (x, g));
        k.path = (top..=g).map(|y| (x, y)).collect();
        k.beat = RISE;
    }

    fn leave(&mut self, c: usize, now: f64) {
        self.quit(c);
        if self.cats[c].fish {
            self.lose(now);
        }
        let d = now + self.rng.uniform(AWAY.0, AWAY.1);
        let k = &mut self.cats[c];
        k.spooked = false;
        (k.act, k.due) = ("away", d);
    }

    fn dine(&mut self, c: usize, key: u64, right: bool, now: f64) {
        let i = self.plate(key).unwrap();
        let p = &mut self.plates[i];
        match p.seats.iter_mut().find(|s| s.0 == right) {
            Some(s) => s.1 = c,
            None => p.seats.push((right, c)),
        }
        let (px, pg) = (p.x, p.g);
        self.cats[c].meal = Some(key);
        let x = if right { px - W } else { px + PW };
        self.jump(c, (x, pg), now);
    }

    fn hunt(&mut self, c: usize, to: Cell, now: f64) {
        self.cats[c].tries += 1;
        self.jump(c, to, now);
        self.cats[c].prey = Some("mouse");
        if let Some(m) = &mut self.mouse {
            let d = now + self.rng.uniform(WITS.0, WITS.1);
            m.threat = to.0;
            m.fright = m.fright.min(d);
            m.due = m.due.min(m.fright);
        }
    }

    fn land(&mut self, c: usize, now: f64) {
        let k = &mut self.cats[c];
        (k.x, k.g) = k.landing.unwrap_or((k.x, k.g));
        let prey = k.prey.take();
        self.stale = true;
        let p = self.pointer;
        if self.cats[c].fish {
            self.settle(c, now);
        } else if let Some(key) = self.cats[c].meal {
            let px = self.plates[self.plate(key).unwrap()].x;
            self.cats[c].right = px > self.cats[c].x;
            self.rest(c, "eat", now, Some(BITE));
        } else if self.cats[c].toy {
            self.reach(c, now);
        } else if prey == Some("mouse") && self.caught(c) {
            self.shoo(now);
            self.rest(c, "eat", now, Some(2.0));
        } else if prey == Some("fish") {
            self.grab(c, now);
        } else if prey == Some("steal") {
            self.snatch(c, now);
        } else if prey == Some("pointer") && self.cats[c].holds(p) {
            self.swat(c, now);
        } else if prey == Some("pointer") && self.cats[c].under(p) != 0 {
            self.rest(c, "stare", now, Some(0.7));
        } else if prey == Some("pointer") {
            self.rest(c, "puzzled", now, Some(0.8));
        } else {
            self.rest(c, "alert", now, Some(0.9));
        }
    }

    fn notice(&mut self, c: usize, was: Option<Cell>, now: f64) {
        let p = self.pointer;
        self.cats[c].amuse(now);
        let Some(p) = p else {
            let k = &self.cats[c];
            if !k.toy && ["stare", "wiggle", "beg"].contains(&k.act) {
                self.rest(c, "puzzled", now, Some(0.8));
            }
            return;
        };
        let k = &mut self.cats[c];
        match was {
            None => k.fun = k.fun.max(0.7),
            Some(was) => {
                let far = (p.0 - was.0).abs() + 2 * (p.1 - was.1).abs();
                k.fun = 1.0f64.min(k.fun + 0.1 + 0.03 * far as f64);
            }
        }
        k.ignored = None;
        let right = p.0 >= k.x + W / 2;
        if (k.act == "stare" || k.act == "wiggle") && !k.toy && !k.fish && right != k.right {
            k.right = right;
            let act = k.act;
            self.show(c, act, None);
        }
        let k = &mut self.cats[c];
        if k.act == "sleep" || k.act == "yawn" {
            let (dx, dy) = POKE;
            if (p.0 - k.x - W / 2).abs() <= dx && (p.1 - k.g + 2).abs() <= dy {
                self.rest(c, "alert", now, Some(0.5));
            }
        } else if RESTING.contains(&k.act) {
            k.due = k.due.min(now + NOTICE);
        }
    }

    fn swat(&mut self, c: usize, now: f64) {
        if let Some(p) = self.pointer {
            let k = &mut self.cats[c];
            k.right = p.0 >= k.x + W / 2;
        }
        let s = 0.32 * self.rng.randint(2, 4) as f64;
        self.rest(c, "bat", now, Some(s));
    }

    fn stalk(&mut self, c: usize, now: f64) -> bool {
        let p = self.pointer;
        self.cats[c].amuse(now);
        let Some(pp) = p else { return false };
        if p == self.cats[c].ignored {
            return false;
        }
        if self.cats[c].fun < BORED {
            self.cats[c].ignored = p;
            if self.cats[c].holds(p) {
                let s = self.rng.uniform(3.0, 5.0);
                self.rest(c, "purr", now, Some(s));
            } else if ["stare", "bat", "puzzled"].contains(&self.cats[c].act) {
                let k = &mut self.cats[c];
                k.right = pp.0 < k.x + W / 2;
                let s = self.rng.uniform(2.5, 5.0);
                self.rest(c, "groom", now, Some(s));
            } else {
                return false;
            }
            return true;
        }
        self.fresh(now);
        let k = &mut self.cats[c];
        k.right = pp.0 >= k.x + W / 2;
        let to = self.below(c, pp);
        let k = &self.cats[c];
        let here = perch(&self.spots, k.x, k.g);
        if k.holds(p) {
            self.swat(c, now);
        } else if k.under(p) != 0 {
            let s = self.rng.uniform(0.6, 1.2);
            self.rest(c, "beg", now, Some(s));
        } else if to.is_none() {
            self.rest(c, "stare", now, Some(0.8));
        } else if let Some(here) = here
            && let Some(to) = to
            && to.1 == k.g
            && here.has(to.0)
            && (to.0 - k.x).abs() > POUNCE
        {
            let gap = self.rng.randint(6, 10);
            let x = self.cats[c].x;
            let goal = to.0 + if to.0 < x { gap } else { -gap };
            self.cats[c].go("run", goal, now);
        } else {
            let wait = (0.4 + 0.45 * self.rng.random()) * (1.25 - 0.5 * self.cats[c].fun);
            self.rest(c, "wiggle", now, Some(wait));
        }
        true
    }

    fn pounce(&mut self, c: usize, now: f64) {
        let p = self.pointer;
        let to = p.and_then(|p| self.below(c, p));
        let k = &self.cats[c];
        match to {
            None => {
                let pose = if p.is_some() { "stare" } else { "puzzled" };
                self.rest(c, pose, now, Some(0.8));
            }
            Some(to) if to == (k.x, k.g) => self.rest(c, "stare", now, Some(0.2)),
            Some(to) => {
                self.jump(c, to, now);
                self.cats[c].prey = Some("pointer");
            }
        }
    }

    fn hop(&mut self, c: usize, now: f64) {
        let rise = self.cats[c].under(self.pointer);
        if rise == 0 {
            return self.rest(c, "stare", now, Some(0.5));
        }
        let k = &mut self.cats[c];
        let ups: Vec<Cell> = (1..=rise).map(|i| (k.x, k.g - i)).collect();
        (k.act, k.due, k.landing) = ("leap", now, Some((k.x, k.g)));
        let mut path: VecDeque<Cell> = ups.iter().copied().collect();
        path.push_back(*ups.last().unwrap());
        path.extend(ups[..ups.len() - 1].iter().rev());
        path.push_back((k.x, k.g));
        k.path = path;
        k.beat = RISE;
        k.prey = Some("pointer");
    }

    fn beside_fish(&self, c: usize) -> bool {
        let k = &self.cats[c];
        self.fish
            .as_ref()
            .is_some_and(|f| f.y == k.g - 1 && (k.x == f.x - W || k.x == f.x + FW))
    }

    fn grab(&mut self, c: usize, now: f64) {
        let ok = self
            .fish
            .as_ref()
            .is_some_and(|f| f.holder.is_none() && f.left > 0);
        if !ok {
            return self.rest(c, "puzzled", now, Some(0.8));
        }
        if !self.beside_fish(c) {
            return self.rest(c, "alert", now, Some(0.9));
        }
        let f = self.fish.as_mut().unwrap();
        let k = &mut self.cats[c];
        k.right = f.x > k.x;
        f.holder = Some(c);
        k.fish = true;
        f.until = INF;
        f.right = k.right;
        self.fish_show(false);
        self.rest(c, "bat", now, Some(0.5));
    }

    fn snatch(&mut self, c: usize, now: f64) {
        let owner = self.fish.as_ref().and_then(|f| f.holder);
        let ok = owner.is_some_and(|o| {
            o != c
                && self.cats[o].act == "eat"
                && self.fish.as_ref().unwrap().left > 0
                && self.beside_fish(c)
        });
        if !ok {
            return self.rest(c, "puzzled", now, Some(0.8));
        }
        let owner = owner.unwrap();
        let fx = self.fish.as_ref().unwrap().x;
        self.cats[c].right = fx > self.cats[c].x;
        if self.rng.random() < 0.5 {
            self.cats[owner].fish = false;
            self.rest(owner, "hiss", now, Some(1.2));
            self.fish.as_mut().unwrap().holder = Some(c);
            self.cats[c].fish = true;
            self.rest(c, "bat", now, Some(0.35));
        } else {
            self.rest(owner, "hiss", now, Some(1.0));
            self.cats[c].spooked = true;
            self.rest(c, "alert", now, Some(0.5));
        }
    }

    fn carry(&mut self, c: usize, now: f64) {
        self.fresh(now);
        match self.pick(c, true) {
            None => self.settle(c, now),
            Some(to) => self.jump(c, (to.0, to.1.g), now),
        }
    }

    fn settle(&mut self, c: usize, now: f64) {
        let (x, y) = (self.cats[c].x, self.cats[c].g - 1);
        if !self.free(x + W, y, FW, 1) && self.free(x - FW, y, FW, 1) {
            self.cats[c].right = false;
        } else if self.free(x + W, y, FW, 1) {
            self.cats[c].right = true;
        }
        self.fish_by(c, false);
        self.rest(c, "eat", now, Some(BITE));
    }

    fn play(&mut self, c: usize, x: i64, now: f64, keen: bool) {
        let t = self.toy.as_mut().unwrap();
        t.player = Some(c);
        let yg = t.yarn.g();
        self.cats[c].toy = true;
        let k = &self.cats[c];
        let here = perch(&self.spots, k.x, k.g);
        if (x, yg) == (k.x, k.g) {
            self.reach(c, now);
        } else if let Some(here) = here
            && here.g == yg
            && here.has(x)
            && (keen || self.rng.random() < 0.5)
        {
            self.cats[c].go("run", x, now);
        } else {
            self.jump(c, (x, yg), now);
        }
    }

    fn reach(&mut self, c: usize, now: f64) {
        let t = self.toy.as_ref().unwrap();
        if t.yarn.moving {
            return self.watch(c, now);
        }
        let (yx, yg, plays) = (t.yarn.x(), t.yarn.g(), t.yarn.plays);
        let k = &mut self.cats[c];
        if k.g != yg || !(k.x == yx - W || k.x == yx + BW) {
            self.quit(c);
            return self.rest(c, "puzzled", now, Some(0.8));
        }
        k.right = yx > k.x;
        if plays > 0 {
            self.rest(c, "bat", now, Some(0.45));
        } else {
            self.rest(c, "roll", now, Some(1.8));
        }
    }

    fn knock(&mut self, c: usize, now: f64) {
        let way = if self.cats[c].right { 1 } else { -1 };
        let t = self.toy.as_mut().unwrap();
        t.yarn.knock(&mut self.rng, way, now);
        self.yarn_stale();
        let s = self.rng.uniform(0.2, 0.45);
        self.rest(c, "wiggle", now, Some(s));
    }

    fn trail(&self, c: usize) -> Option<i64> {
        let t = self.toy.as_ref().unwrap();
        let k = &self.cats[c];
        let here = perch(&self.spots, k.x, k.g)?;
        let yx = t.yarn.x();
        let ahead = 2 * yx + BW >= 2 * k.x + W;
        let goal = if ahead { yx - W - LEAD } else { yx + BW + LEAD };
        let goal = here.x0.max(goal.min(here.x1));
        (goal != k.x && (goal > k.x) == ahead).then_some(goal)
    }

    fn follow(&mut self, c: usize) {
        let goal = self.trail(c);
        let k = &mut self.cats[c];
        k.goal = goal.unwrap_or(k.x);
        if k.goal != k.x {
            k.right = k.goal > k.x;
        }
    }

    fn watch(&mut self, c: usize, now: f64) {
        let yx = self.toy.as_ref().unwrap().yarn.x();
        let k = &mut self.cats[c];
        let right = 2 * yx + BW >= 2 * k.x + W;
        if k.act == "wiggle" && right == k.right {
            k.due = now + 0.15;
        } else {
            k.right = right;
            self.rest(c, "wiggle", now, Some(0.15));
        }
    }

    fn chase(&mut self, c: usize, now: f64) {
        if self.toy.as_ref().unwrap().yarn.moving {
            match self.trail(c) {
                Some(goal) => self.cats[c].go("run", goal, now),
                None => self.watch(c, now),
            }
        } else if let Some(x) = self.plaything(c) {
            self.play(c, x, now, true);
        } else {
            self.toy.as_mut().unwrap().yarn.plays = 0;
            self.quit(c);
            self.rest(c, "puzzled", now, Some(0.8));
        }
    }

    fn stir(&mut self, c: usize, now: f64) {
        let act = self.cats[c].act;
        if !self.idle && (act == "sleep" || act == "yawn") {
            self.rest(c, "alert", now, Some(0.6));
        } else if self.idle && (act == "sit" || act == "groom") {
            let d = now + self.rng.uniform(0.5, 2.0);
            self.cats[c].due = self.cats[c].due.min(d);
        }
    }

    fn check_cat(&mut self, c: usize, info: Option<(bool, f64)>, now: f64) -> bool {
        if info.is_none() {
            self.cats[c].sprite = None;
        }
        if !info.is_some_and(|i| i.0) {
            self.leave(c, now);
            return false;
        }
        true
    }

    fn cat_step(&mut self, c: usize, now: f64) {
        let act = self.cats[c].act;
        let k = &self.cats[c];
        let (meal_left, toy, fish) = (
            k.meal
                .and_then(|m| self.plate(m))
                .is_some_and(|i| self.plates[i].left > 0),
            k.toy,
            k.fish,
        );
        if act == "away" {
            self.arrive(c, now);
        } else if act == "leap" {
            if let Some(p) = self.cats[c].path.pop_front() {
                (self.cats[c].x, self.cats[c].g) = p;
                self.show(c, "leap", None);
                let beat = self.cats[c].beat;
                self.cats[c].after(now, beat);
            } else {
                self.land(c, now);
            }
        } else if act == "walk" || act == "run" {
            if act == "run" && toy && self.toy.as_ref().unwrap().yarn.moving {
                self.follow(c);
            }
            let k = &self.cats[c];
            let ahead = k.x + if k.goal > k.x { 1 } else { -1 };
            if k.x == k.goal {
                return self.halt(c, now, false);
            }
            if self.taken(c, ahead, k.g) {
                self.cats[c].goal = self.cats[c].x;
                return self.halt(c, now, true);
            }
            let k = &mut self.cats[c];
            k.x = ahead;
            k.stride += 1;
            let s = k.stride - 1;
            self.show(c, act, Some(s));
            let pace = self.cats[c].pace();
            self.cats[c].after(now, pace);
        } else if act == "eat" && meal_left {
            let key = self.cats[c].meal.unwrap();
            self.bite(key, now);
            self.cats[c].due = now + BITE;
        } else if act == "eat" && fish {
            if self.nibble(now) {
                self.cats[c].due = now + BITE;
            } else {
                self.cats[c].fish = false;
                let s = self.rng.uniform(2.5, 5.0);
                self.rest(c, "groom", now, Some(s));
            }
        } else if act == "eat" {
            self.quit(c);
            let s = self.rng.uniform(2.5, 5.0);
            self.rest(c, "groom", now, Some(s));
        } else if act == "bat" && toy {
            self.knock(c, now);
        } else if act == "bat" && fish {
            self.carry(c, now);
        } else if act == "bat" {
            let s = self.rng.uniform(0.9, 2.5);
            self.rest(c, "stare", now, Some(s));
        } else if act == "hiss" && fish {
            self.rest(c, "eat", now, Some(BITE));
        } else if act == "wiggle" && toy {
            self.chase(c, now);
        } else if act == "wiggle" {
            self.pounce(c, now);
        } else if act == "beg" {
            self.hop(c, now);
        } else if act == "roll" && toy {
            let t = self.toy.as_mut().unwrap();
            t.until = t.until.min(now + UNDONE);
            self.quit(c);
            let s = self.rng.uniform(2.5, 5.0);
            self.rest(c, "groom", now, Some(s));
        } else if self.cats[c].spooked {
            self.cats[c].spooked = false;
            self.flee(c, now);
            if self.cats[c].act != "leap" {
                let s = self.rng.uniform(2.0, 5.0);
                self.rest(c, "sit", now, Some(s));
            }
        } else if self.stalk(c, now) {
        } else if let Some(aim) = self.scent(c) {
            self.hunt(c, aim, now);
        } else if let Some(aim) = self.bait(c) {
            self.jump(c, aim, now);
            self.cats[c].prey = Some("fish");
        } else if let Some(aim) = self.loot(c) {
            self.jump(c, aim, now);
            self.cats[c].prey = Some("steal");
        } else if let Some((key, right)) = self.seat(c) {
            self.dine(c, key, right, now);
        } else if let Some(x) = self.plaything(c)
            && self.toy.is_some()
        {
            self.play(c, x, now, false);
        } else if self.cats[c].act == "yawn" {
            self.rest(c, "sleep", now, None);
        } else if self.idle {
            self.rest(c, "yawn", now, Some(1.4));
        } else {
            self.wander(c, now);
        }
    }

    fn halt(&mut self, c: usize, now: f64, barred: bool) {
        if self.cats[c].toy && barred {
            self.quit(c);
            self.rest(c, "puzzled", now, Some(0.8));
        } else if self.cats[c].toy {
            self.chase(c, now);
        } else if self.cats[c].act == "run" {
            let s = self.rng.uniform(0.3, 0.7);
            self.rest(c, "wiggle", now, Some(s));
        } else {
            let s = self.rng.uniform(2.0, 6.0);
            self.rest(c, "sit", now, Some(s));
        }
    }

    fn wander(&mut self, c: usize, now: f64) {
        let roll = self.rng.random();
        let k = &self.cats[c];
        let here = perch(&self.spots, k.x, k.g);
        if roll < 0.25 && k.act != "groom" {
            let s = self.rng.uniform(2.5, 5.0);
            self.rest(c, "groom", now, Some(s));
        } else if roll < 0.65
            && let Some(here) = here
            && here.x1 > here.x0
        {
            let mut goal = self.rng.randint(here.x0, here.x1);
            let x = self.cats[c].x;
            if (goal - x).abs() < 3 {
                goal = if x - here.x0 > here.x1 - x {
                    here.x0
                } else {
                    here.x1
                };
            }
            let free = !self.taken(c, goal, here.g);
            if free && !near(goal, here.g, self.cursor) {
                return self.cats[c].go("walk", goal, now);
            }
            let s = self.rng.uniform(2.0, 5.0);
            self.rest(c, "sit", now, Some(s));
        } else if roll < 0.85
            && let Some(to) = self.pick(c, false)
        {
            self.jump(c, (to.0, to.1.g), now);
        } else {
            let s = self.rng.uniform(2.0, 6.0);
            self.rest(c, "sit", now, Some(s));
        }
    }

    // -- what happens ------------------------------------------------

    /// The pane went idle, or busy.
    pub fn set_idle(&mut self, idle: bool, now: f64) {
        self.idle = idle;
        for c in 0..self.cats.len() {
            self.stir(c, now);
        }
    }

    /// The cursor moved (frame cells), or went (`None`).
    pub fn set_cursor(&mut self, at: Option<Cell>, now: f64) {
        self.cursor = at;
        for c in 0..self.cats.len() {
            let k = &self.cats[c];
            if near(k.x, k.g, self.cursor) {
                self.flee(c, now);
            }
        }
    }

    /// After a change of the text or of the view: is each cat seen,
    /// and does it still stand on something?
    fn check(&mut self, now: f64) {
        let cats: Vec<usize> = (0..self.cats.len())
            .filter(|&c| {
                let k = &self.cats[c];
                k.act != "leap" && k.act != "away" && k.sprite.is_some()
            })
            .collect();
        let mut infos = Vec::new();
        if !cats.is_empty() {
            let ids: Vec<u64> = cats.iter().map(|&c| self.cats[c].sprite.unwrap()).collect();
            infos = self.stage.infos(&ids);
        }
        let mut seen = Vec::new();
        for (&c, &info) in cats.iter().zip(&infos) {
            if self.check_cat(c, info, now) {
                seen.push((c, info));
            }
        }
        if !self.survey(now) {
            return;
        }
        for (c, info) in seen {
            let k = &self.cats[c];
            let held = k.meal.is_some() || k.fish || k.toy;
            let off = !held && perch(&self.spots, k.x, k.g).is_none();
            if info.is_some_and(|i| i.1 > COVERED || off) {
                self.flee(c, now);
            }
        }
    }

    /// What is due: a look at the text, the cats, the mouse, the
    /// plates, the fish, the ball.
    pub fn step(&mut self, now: f64) {
        if self.stale && now - self.read >= READ {
            self.stale = false;
            self.check(now);
        }
        for c in 0..self.cats.len() {
            if now >= self.cats[c].due {
                self.cat_step(c, now);
            }
        }
        match &self.mouse {
            Some(m) => {
                if now >= m.due {
                    self.mouse_step(now);
                }
            }
            None => {
                if now >= self.next_mouse {
                    self.loose(now);
                }
            }
        }
        for key in self.plates.iter().map(|p| p.key).collect::<Vec<_>>() {
            let Some(i) = self.plate(key) else { continue };
            let p = &mut self.plates[i];
            if now >= p.until {
                if p.left > 0 && !p.seats.is_empty() {
                    p.until = now + 2.0 * BITE;
                } else {
                    self.clear(key, now);
                }
            }
        }
        if now >= self.next_food {
            self.serve(now);
        }
        match &self.fish {
            Some(f) => {
                if now >= f.until {
                    self.lose(now);
                }
            }
            None => {
                if now >= self.next_fish {
                    self.toss(now);
                }
            }
        }
        if let Some(t) = &mut self.toy
            && now >= t.yarn.due
        {
            let d = t.yarn.step(&self.room, &mut self.rng, now);
            self.yarn_stale();
            self.put_drawn(d);
        }
        if let Some(t) = &mut self.toy
            && t.yarn.plays == 0
            && t.player.is_none()
        {
            t.until = t.until.min(now + UNDONE);
        }
        match &mut self.toy {
            Some(t) => {
                if now >= t.until {
                    if t.yarn.plays > 0 && t.player.is_some() {
                        t.until = now + 2.0 * BITE;
                    } else {
                        self.wind(now);
                    }
                }
            }
            None => {
                if now >= self.next_toy {
                    self.throw(now);
                }
            }
        }
    }

    /// When `step` has something to do.
    pub fn due(&self, now: f64) -> f64 {
        let mut t = self.cats.iter().map(|c| c.due).fold(INF, f64::min);
        t = self.plates.iter().map(|p| p.until).fold(t, f64::min);
        t = t.min(self.next_food);
        if self.stale {
            t = t.min(now + READ);
        }
        t = t.min(self.mouse.as_ref().map_or(self.next_mouse, |m| m.due));
        t = t.min(self.fish.as_ref().map_or(self.next_fish, |f| f.until));
        t.min(
            self.toy
                .as_ref()
                .map_or(self.next_toy, |y| y.yarn.due.min(y.until)),
        )
    }
}

/// The blank rows of the mat.
pub fn mat_look(w: i64, h: i64) -> Look {
    let row = vec![Seg {
        t: " ".repeat(w as usize),
        fg: None,
        bold: false,
        dim: false,
    }];
    Look::Still(vec![row; h as usize])
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::look::tests::{fnv, text};

    /// A stage that writes down what is asked, as the twin's fake pilot.
    #[derive(Default)]
    pub struct Rec {
        pub log: Vec<String>,
        pub n: u64,
        pub cover: f64,
        pub rows: Vec<String>,
        pub w: i64,
        pub h: i64,
        /// The sprites drawn and not removed; those that fade.
        pub alive: HashSet<u64>,
        pub faded: HashSet<u64>,
    }

    impl Stage for Rec {
        fn put(&mut self, id: Option<u64>, x: i64, y: i64, look: &Look, z: i64) -> u64 {
            let t = fnv(&text(look));
            match id {
                None => {
                    self.n += 1;
                    self.alive.insert(self.n);
                    self.log.push(format!("draw {} {x} {y} {z} {t}", self.n));
                    self.n
                }
                Some(id) => {
                    self.log.push(format!("at {id} {x} {y}"));
                    self.log.push(format!("set {id} {t}"));
                    id
                }
            }
        }
        fn set(&mut self, id: u64, look: &Look) {
            self.faded.insert(id);
            self.log.push(format!("set {id} {}", fnv(&text(look))));
        }
        fn remove(&mut self, id: u64) {
            self.alive.remove(&id);
            self.log.push(format!("rm {id}"));
        }
        fn infos(&mut self, ids: &[u64]) -> Vec<Option<(bool, f64)>> {
            self.log.push("[".into());
            for id in ids {
                self.log.push(format!("info {id}"));
            }
            self.log.push("]".into());
            ids.iter().map(|_| Some((true, self.cover))).collect()
        }
        fn begin(&mut self) {
            self.log.push("[".into());
        }
        fn end(&mut self) {
            self.log.push("]".into());
        }
        fn mat(&mut self, w: i64, h: i64) -> u64 {
            self.n += 1;
            self.alive.insert(self.n);
            self.log.push(format!("mat {} {w} {h}", self.n));
            self.n
        }
        fn view(&mut self) -> Option<(Vec<String>, Area)> {
            Some((self.rows.clone(), (0, 0, self.w, self.h)))
        }
    }

    pub struct Scene {
        pub name: String,
        pub seed: u64,
        pub w: i64,
        pub h: i64,
        pub until: f64,
        pub kinds: Kinds,
        pub screens: Vec<Vec<String>>,
        pub events: Vec<(f64, String, Vec<Option<f64>>)>,
        pub sums: Vec<(i64, String)>,
    }

    pub fn scenes() -> Vec<Scene> {
        let mut out: Vec<Scene> = Vec::new();
        for line in include_str!("world.txt").lines() {
            let mut it = line.splitn(2, ' ');
            let (head, rest) = (it.next().unwrap(), it.next().unwrap_or(""));
            let s = out.last_mut();
            match head {
                "scene" => {
                    let words: Vec<&str> = rest.split(' ').collect();
                    let kv = |k: &str| {
                        words
                            .iter()
                            .find_map(|w| w.strip_prefix(&format!("{k}=")))
                            .unwrap_or("0")
                    };
                    let on = |k: &str| kv(k) == "1";
                    out.push(Scene {
                        name: words[0].into(),
                        seed: kv("seed").parse().unwrap(),
                        w: kv("w").parse().unwrap(),
                        h: kv("h").parse().unwrap(),
                        until: kv("until").parse().unwrap(),
                        kinds: Kinds {
                            count: kv("count").parse().unwrap(),
                            varied: on("varied"),
                            food: on("food"),
                            mice: on("mice"),
                            fish: on("fish"),
                            toys: on("toys"),
                            hunt: on("hunt"),
                        },
                        screens: Vec::new(),
                        events: Vec::new(),
                        sums: Vec::new(),
                    });
                }
                "screen" => s.unwrap().screens.push(Vec::new()),
                "|" => s.unwrap().screens.last_mut().unwrap().push(rest.into()),
                "ev" => {
                    let w: Vec<&str> = rest.split(' ').filter(|w| !w.is_empty()).collect();
                    let args = w[2..].iter().map(|a| a.parse().ok()).collect();
                    s.unwrap()
                        .events
                        .push((w[0].parse().unwrap(), w[1].into(), args));
                }
                "sum" => {
                    let (sec, h) = rest.split_once(' ').unwrap();
                    s.unwrap().sums.push((sec.parse().unwrap(), h.into()));
                }
                _ => {}
            }
        }
        out
    }

    fn tick(k: i64) -> f64 {
        k as f64 / 30.0 + ((k * 7919) % 13) as f64 / 1000.0
    }

    fn bits(f: f64) -> String {
        format!("{:016x}", f.to_bits())
    }

    /// The lines of `scene`, by second.
    pub fn run(scene: &Scene) -> Vec<(i64, String)> {
        let stage = Rec {
            rows: scene.screens[0].clone(),
            w: scene.w,
            h: scene.h,
            ..Rec::default()
        };
        let mut home = Home::new(stage, Twister::new(scene.seed), &scene.kinds, 0.0);
        let mut events = scene.events.iter().peekable();
        let mut out = Vec::new();
        let mut k = 0;
        loop {
            let now = tick(k);
            if now >= scene.until {
                break;
            }
            k += 1;
            if [1, 2, 3].contains(&(k % 50)) {
                continue;
            }
            while let Some((t, what, a)) = events.peek()
                && *t <= now
            {
                events.next();
                home.stage.log.push(format!("ev {what}"));
                let at = (a.len() == 2).then(|| (a[0].unwrap() as i64, a[1].unwrap() as i64));
                match what.as_str() {
                    "idle" => home.set_idle(a[0] != Some(0.0), now),
                    "cursor" => home.set_cursor(at, now),
                    "screen" => {
                        home.stage.rows = scene.screens[a[0].unwrap() as usize].clone();
                        home.stale = true;
                    }
                    "cover" => home.stage.cover = a[0].unwrap(),
                    "hover" => home.point(at, now),
                    _ => unreachable!(),
                }
            }
            home.step(now);
            for (i, c) in home.cats.iter().enumerate() {
                let l = format!(
                    "cat {i} {} {} {} {} {}",
                    c.act,
                    c.x,
                    c.g,
                    u8::from(c.right),
                    bits(c.due)
                );
                home.stage.log.push(l);
            }
            let d = format!("due {}", bits(home.due(now)));
            home.stage.log.push(d);
            out.extend(home.stage.log.drain(..).map(|l| (now as i64, l)));
        }
        out
    }

    /// The scene's sums against the twin's; `Err`: the first second
    /// that differs, and its lines.
    pub fn compare(scene: &Scene) -> Result<(), String> {
        let lines = run(scene);
        let (mut h, mut sec) = (0u64, 0i64);
        let mut sums = Vec::new();
        for (s, l) in &lines {
            if *s != sec {
                sums.push((sec, format!("{h:016x}")));
                sec = *s;
            }
            h = u64::from_str_radix(&fnv(&format!("{h:016x}{l}")), 16).unwrap();
        }
        sums.push((sec, format!("{h:016x}")));
        for (a, b) in sums.iter().zip(&scene.sums) {
            if a != b {
                let said: Vec<&str> = lines
                    .iter()
                    .filter(|(s, _)| *s == a.0)
                    .map(|(_, l)| l.as_str())
                    .collect();
                return Err(format!(
                    "{} second {}:\n{}",
                    scene.name,
                    a.0,
                    said.join("\n")
                ));
            }
        }
        assert_eq!(sums.len(), scene.sums.len(), "{}", scene.name);
        Ok(())
    }

    fn scene(name: &str) {
        let s = scenes().into_iter().find(|s| s.name == name).unwrap();
        if let Err(e) = compare(&s) {
            panic!("{e}");
        }
    }

    #[test]
    fn one() {
        scene("one");
    }

    #[test]
    fn three() {
        scene("three");
    }
    #[test]
    fn food() {
        scene("food");
    }

    #[test]
    fn mice() {
        scene("mice");
    }

    #[test]
    fn fish() {
        scene("fish");
    }

    #[test]
    fn toys() {
        scene("toys");
    }

    #[test]
    fn hunt() {
        scene("hunt");
    }

    #[test]
    fn all() {
        scene("all");
    }
    // test_cat_soak.py: everything on, random events, a few simulated
    // minutes; the state stays sound and everything goes at the end.
    // The twin's `move` (a frame replaced) is not the store app's.

    const LONG: f64 = 60.0;
    const SPIN: usize = 200;
    const MOVING: [&str; 4] = ["leap", "away", "walk", "run"];
    const IDLE: [&str; 4] = ["sleep", "sit", "groom", "yawn"];

    fn base() -> Vec<String> {
        let mut rows = vec![String::new(); 24];
        rows[3] = "#".repeat(60);
        rows[8] = "#".repeat(60);
        rows
    }

    fn sound(home: &Home<Rec>) {
        for (i, it) in home.cats.iter().enumerate() {
            if it.fish {
                assert_eq!(home.fish.as_ref().unwrap().holder, Some(i), "{}", it.act);
            }
            if it.toy {
                assert_eq!(home.toy.as_ref().unwrap().player, Some(i), "{}", it.act);
            }
            if let Some(key) = it.meal {
                let p = home.plates.iter().find(|p| p.key == key).expect(it.act);
                assert!(p.seats.iter().any(|s| s.1 == i), "{}", it.act);
            }
            assert!(it.due < INF || it.act == "sleep", "{}", it.act);
        }
        if let Some(f) = &home.fish
            && let Some(h) = f.holder
        {
            assert!(home.cats[h].fish);
        }
        if let Some(t) = &home.toy {
            let (x0, _, x1, y1) = home.room.view;
            let y = &t.yarn;
            assert!(y.plays >= 0 && y.points.len() <= crate::yarn::THREAD);
            assert!(
                x0 <= y.x() && y.x() <= x1 - BW && y.g() <= y1,
                "{} {}",
                y.x(),
                y.g()
            );
            if !y.moving {
                assert!(y.px % 2.0 == 0.0 && y.py % 4.0 == 0.0, "{} {}", y.px, y.py);
            }
            if let Some(p) = t.player {
                assert!(home.cats[p].toy);
            }
        }
        for p in &home.plates {
            for &(_, c) in &p.seats {
                assert_eq!(home.cats[c].meal, Some(p.key));
            }
        }
        let still: Vec<&Cat> = home
            .cats
            .iter()
            .filter(|c| !MOVING.contains(&c.act))
            .collect();
        for (i, a) in still.iter().enumerate() {
            for b in &still[i + 1..] {
                let apart = a.x + W <= b.x || b.x + W <= a.x || a.g <= b.g - H || b.g <= a.g - H;
                assert!(apart, "{:?} {:?}", (a.x, a.g, a.act), (b.x, b.g, b.act));
            }
        }
    }

    fn known(home: &Home<Rec>) -> HashSet<u64> {
        let mut out: HashSet<u64> = home.cats.iter().filter_map(|c| c.sprite).collect();
        out.extend(home.plates.iter().filter_map(|p| p.sprite));
        out.extend(home.mat.map(|m| m.0));
        out.extend(home.mouse.as_ref().and_then(|m| m.sprite));
        out.extend(home.fish.as_ref().and_then(|f| f.sprite));
        if let Some(t) = &home.toy {
            out.extend(t.ball);
            out.extend(t.wire);
        }
        out
    }

    fn sprites(home: &Home<Rec>) -> HashSet<u64> {
        home.stage
            .alive
            .difference(&home.stage.faded)
            .copied()
            .collect()
    }

    fn soak(seed: u64, count: usize, minutes: f64, hovers: f64) {
        let stage = Rec {
            rows: base(),
            w: 80,
            h: 24,
            ..Rec::default()
        };
        let kinds = Kinds {
            count,
            varied: false,
            food: true,
            mice: true,
            fish: true,
            toys: true,
            hunt: true,
        };
        let mut home = Home::new(stage, Twister::new(seed), &kinds, 0.0);
        let mut rng = Twister::new(seed + 1000);
        let (w, h) = (80, 24);
        let mut words: Vec<(usize, usize, &str)> = Vec::new();
        let mut since: Vec<(&str, f64)> = home.cats.iter().map(|c| (c.act, 0.0)).collect();
        let (mut now, mut steps, mut same) = (0.0, 0usize, 0usize);
        let (mut events, mut screens) = (0.5, 3.0);
        let check = |home: &Home<Rec>, since: &mut Vec<(&'static str, f64)>, now: f64| {
            sound(home);
            for (i, it) in home.cats.iter().enumerate() {
                let (act, t0) = since[i];
                if it.act != act {
                    since[i] = (it.act, now);
                } else if !IDLE.contains(&act) {
                    assert!(now - t0 < LONG, "{act} {}", now - t0);
                }
            }
            let ids = sprites(home);
            assert!(ids.is_subset(&known(home)), "{ids:?} {:?}", known(home));
            assert!(ids.len() <= count + 1 + PLATES + 4);
        };
        let end = minutes * 60.0;
        while now < end {
            let due = home.due(now).min(events).min(end);
            if due <= now {
                same += 1;
                assert!(
                    same < SPIN,
                    "{:?}",
                    home.cats.iter().map(|c| (c.act, c.due)).collect::<Vec<_>>()
                );
            } else {
                (same, now) = (0, due);
            }
            if now >= events {
                events = now - (1.0 - rng.random()).ln() * 0.8;
                let roll = rng.random();
                if roll < hovers {
                    let mut at = match home.pointer {
                        Some(p) if rng.random() >= 0.3 => {
                            Some((p.0 + rng.randint(-3, 3), p.1 + rng.randint(-1, 1)))
                        }
                        _ => Some((rng.randrange(0, w), rng.randrange(0, h))),
                    };
                    if rng.random() < 0.1 {
                        at = None;
                    }
                    home.point(at, now);
                } else {
                    let roll = (roll - hovers) / (1.0 - hovers);
                    if roll < 0.3 {
                        let c = rng.below(home.cats.len() as u32) as usize;
                        let (x, g) = (home.cats[c].x, home.cats[c].g);
                        let mut at = Some((x + rng.randint(-8, 14), g - rng.randint(0, 4)));
                        if rng.random() < 0.2 {
                            at = None;
                        }
                        home.set_cursor(at, now);
                    } else if roll < 0.55 {
                        let idle = rng.random() < 0.5;
                        home.set_idle(idle, now);
                    } else if now >= screens {
                        screens = now + rng.uniform(10.0, 25.0);
                        let rows = &mut home.stage.rows;
                        let put = |rows: &mut Vec<String>, row: usize, col: usize, t: &str| {
                            let mut r: Vec<char> = rows[row].chars().collect();
                            r.resize(r.len().max(col + t.len()), ' ');
                            for (i, ch) in t.chars().enumerate() {
                                r[col + i] = ch;
                            }
                            rows[row] = r.into_iter().collect();
                        };
                        if !words.is_empty() && (rng.random() < 0.5 || words.len() >= 3) {
                            let i = rng.below(words.len() as u32) as usize;
                            let (row, col, word) = words.remove(i);
                            put(rows, row, col, &" ".repeat(word.len()));
                        } else if rng.random() < 0.15 {
                            words.clear();
                            *rows = base();
                        } else {
                            let (row, col) =
                                (rng.randint(11, 17) as usize, rng.randint(0, 69) as usize);
                            let word = rng.choice(&WORDS);
                            words.push((row, col, word));
                            put(rows, row, col, &format!(" {word} "));
                        }
                        home.stale = true;
                    }
                }
            }
            home.step(now);
            steps += 1;
            check(&home, &mut since, now);
        }
        assert!((steps as f64) / end < 400.0, "{}", steps as f64 / end);
        // Everything off, the words gone: all of it goes.
        home.point(None, now);
        home.set_cursor(None, now);
        home.stage.rows = base();
        home.stale = true;
        let end = now + 60.0;
        while now < end {
            (
                home.next_food,
                home.next_mouse,
                home.next_fish,
                home.next_toy,
            ) = (INF, INF, INF, INF);
            now = (now + 0.01).max(home.due(now).min(end));
            home.step(now);
            check(&home, &mut since, now);
        }
        assert!(home.fish.is_none() && home.toy.is_none() && home.plates.is_empty());
        assert!(home.mouse.is_none());
        assert!(
            home.cats
                .iter()
                .all(|c| !c.fish && !c.toy && c.meal.is_none())
        );
        assert!(sprites(&home).len() <= count + 1, "{:?}", sprites(&home));
    }

    #[test]
    fn soaks() {
        for (seed, count) in [(1, 1), (2, 2), (3, 3), (4, 4), (5, 2), (6, 3)] {
            soak(seed, count, 4.0, if seed % 2 == 1 { 0.45 } else { 0.05 });
        }
    }

    /// The twin's `toy`: a home, one cat, the word `toy` on the screen
    /// and its ball.
    fn toy() -> Home<Rec> {
        let mut rows = base();
        rows[13] = format!("{} toy ", " ".repeat(29));
        let stage = Rec {
            rows,
            w: 80,
            h: 24,
            ..Rec::default()
        };
        let kinds = Kinds {
            count: 1,
            varied: false,
            food: false,
            mice: false,
            fish: false,
            toys: false,
            hunt: false,
        };
        let mut home = Home::new(stage, Twister::new(7), &kinds, 0.0);
        assert!(home.survey(0.0));
        assert!(home.toy.as_ref().is_some_and(|t| t.word.is_some()));
        home
    }

    #[test]
    fn toy_left_played_out() {
        // Played out, its player gone before it rolled: the ball goes,
        // and its word gets no other.
        let mut home = toy();
        let t = home.toy.as_mut().unwrap();
        (t.player, t.yarn.plays) = (Some(0), 0);
        let word = t.word.unwrap();
        home.cats[0].toy = true;
        home.quit(0);
        let mut now: f64 = 0.0;
        while home.toy.is_some() {
            now = (now + 0.01).max(home.due(now));
            assert!(now < 30.0);
            home.step(now);
        }
        assert!(home.eaten.contains(&word));
        home.stale = true;
        home.survey(now + 1.0);
        assert!(home.toy.is_none());
    }

    #[test]
    fn toy_move() {
        // The frame moved (the store app's `move`: a stale read of the
        // same cells): the word keeps its ball, nothing eaten.
        let mut home = toy();
        let word = home.toy.as_ref().unwrap().word;
        home.stale = true;
        assert!(home.survey(2.0));
        assert!(home.eaten.is_empty());
        assert_eq!(home.toy.as_ref().map(|t| t.word), Some(word));
    }

    #[test]
    fn toy_thread_taken() {
        // The driver took the thread (its rows forgotten): the ball goes
        // all the same, and another comes later.
        let mut home = toy();
        let t = home.toy.as_mut().unwrap();
        t.word = None;
        t.yarn.knock(&mut home.rng, 1, 0.0);
        let d = t.yarn.step(&home.room, &mut home.rng, 0.0);
        home.put_drawn(d);
        let wire = home.toy.as_ref().unwrap().wire.expect("a thread");
        home.stage.remove(wire);
        home.wind(1.0);
        assert!(home.toy.is_none() && home.next_toy < INF);
    }

    #[test]
    fn spooked_away() {
        // Hissed at, then out of the view: it comes back at ease.
        let mut home = toy();
        home.cats[0].spooked = true;
        home.leave(0, 0.0);
        assert!(!home.cats[0].spooked);
    }
}
