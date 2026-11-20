// SPDX-License-Identifier: GPL-3.0-or-later
//! Life: Conway's game of life in Braille, behind the text.
//!
//! A cell of the terminal holds 2 by 4 cells of the world, the dots of
//! a Braille glyph: a view of 80 by 24 is a world of 160 by 96, a torus
//! (`world`). One canvas the size of the view, drawn at each tick.
//!
//! - `--colour`: a glyph in the colour of the age of its youngest cell;
//!   `--trail`: a glyph with no cell alive shows those that died there,
//!   grey, darker for 4 generations (`picture`).
//! - A still world (`Watch`) is made again: a soup, or with `--guests`
//!   a pattern dropped (`guests`), which also come at `--rate` a second,
//!   of `--kinds`.
//! - The text, read as a shape (`sight`, `shape`): nothing is drawn on
//!   the cell on each side of a glyph; `--walls`: no cell lives there.
//! - `--cursor`: the pointer over the canvas (`hover`, `pointer`) kills
//!   the cells within `REACH` of it.
//! - `--particles`: a spark where a glyph loses `BLAST` cells at once,
//!   at most `BLASTS` a generation, and where a pattern lands; drawn
//!   from a sequence of their own, so that they do not change the world.
//!
//! The world, the drops and the sparks draw from `random`: `--seed N`
//! of the launcher gives the same run.

pub mod canvas;
pub mod dice;
pub mod guests;
pub mod patterns;
pub mod picture;
pub mod sight;
pub mod world;

use dice::{Dice, Host, Seeded};
use guests::{Guests, Landing};
use patterns::Kind;
use picture::{AGES, Picture, TRAIL, picture};
use ptyrc_app::{App, Canvas, Color, Value, call, exit, json, log, period, subscribe};
use sight::{Look, Sight};
use world::{Mask, Watch, World, blasts, wall};

/// Cells around the pointer that die.
pub const REACH: i64 = 6;
/// Particles a generation, at most.
pub const BLASTS: usize = 4;
/// Generations a tick, at most: a late tick does not run ahead.
pub const CATCH_UP: u64 = 4;
const SPARKS: [u8; 4] = [231, 228, 156, 114];

/// The options of `init`, `--all` spread.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub speed: f64,
    pub colour: bool,
    pub trail: bool,
    pub guests: bool,
    pub rate: f64,
    pub kinds: Vec<Kind>,
    pub particles: bool,
    pub walls: bool,
    pub cursor: bool,
}

impl Options {
    /// `Err`: what is wrong, for the log.
    pub fn of(args: &Value) -> Result<Options, String> {
        let on = |k: &str| args[k].as_bool() == Some(true);
        let all = on("all");
        let speed = args["speed"].as_f64().unwrap_or(10.0);
        if speed.is_nan() || speed <= 0.0 {
            return Err("--speed must be positive".into());
        }
        let names: Vec<&str> = (args["kinds"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default())
        .iter()
        .filter_map(Value::as_str)
        .collect();
        let kinds = match names.is_empty() || names.contains(&"all") {
            true => Kind::ALL.to_vec(),
            false => (names.iter())
                .map(|n| Kind::parse(n).ok_or(format!("no kind {n:?}")))
                .collect::<Result<_, _>>()?,
        };
        Ok(Options {
            speed,
            colour: all || on("colour"),
            trail: all || on("trail"),
            guests: all || on("guests"),
            rate: args["rate"].as_f64().unwrap_or(guests::RATE).max(0.0),
            kinds,
            particles: all || on("particles"),
            walls: all || on("walls"),
            cursor: all || on("cursor"),
        })
    }
}

/// A particle at the view cell `at`: it leaves in a random direction at
/// `speed` (columns a second; a row counts double), goes through
/// `glyphs` in `life` seconds and falls by `gravity`.
pub fn spark(
    dice: &mut impl Dice,
    at: (usize, usize),
    glyphs: &str,
    speed: (f64, f64),
    life: (f64, f64),
    gravity: f64,
) -> Value {
    let angle = dice.uniform(0.0, std::f64::consts::TAU);
    let v = dice.uniform(speed.0, speed.1);
    let seconds = dice.uniform(life.0, life.1);
    let fg = SPARKS[dice.below(SPARKS.len())];
    let glyphs: Vec<char> = glyphs.chars().collect();
    let last = glyphs.len() - 1;
    let ms = ((seconds * 1000.0 / glyphs.len() as f64) as u64).max(40);
    let frames: Vec<Value> = (glyphs.iter().enumerate())
        .map(|(i, g)| {
            let mut run = json!({"t": (g.to_string()), "fg": fg});
            if i == 0 {
                run["bold"] = json!(true);
            }
            if i == last && last > 0 {
                run["dim"] = json!(true);
            }
            json!({"cells": [[run]], "ms": ms})
        })
        .collect();
    json!({
        "anchor": {"at": [at.0, at.1], "space": "view"},
        "frames": frames,
        "loop": false,
        "motion": {"vx": (v * angle.cos()), "vy": (v * angle.sin() / 2.0), "ax": 0, "ay": gravity, "bounce": false},
        "ttl_ms": (((seconds + 0.5) * 1000.0) as u64),
        "z": 3,
    })
}

/// The cell of the world under a `hover` of the canvas, `(w, h)` the
/// view: the middle of the terminal cell. `None`: left, or off it.
pub fn touched(data: &Value, (w, h): (u32, u32)) -> Option<(i64, i64)> {
    if data["sprite"].is_null() {
        return None;
    }
    let at = |v: &Value| Some((v[0].as_i64()?, v[1].as_i64()?));
    // `pos`: the cell of the frame; `at`: of the sprite, the canvas at 0, 0.
    let (x, y) = at(&data["pos"]).or_else(|| at(&data["at"]))?;
    let inside = (0..i64::from(w)).contains(&x) && (0..i64::from(h)).contains(&y);
    inside.then_some((2 * x + 1, 4 * y + 2))
}

pub struct Life {
    o: Options,
    sight: Sight,
    world: World,
    watch: Watch,
    /// The cells off the text.
    clear: Option<Mask>,
    canvas: Option<Canvas>,
    shown: Picture,
    guests: Option<Guests<Seeded>>,
    /// Generations to the next drop.
    guest: f64,
    dice: Seeded,
    /// The particles' own.
    fx: Seeded,
    /// Generations run.
    done: u64,
    /// Cells of the world the pointer went over since the last tick.
    wiped: Vec<(i64, i64)>,
    told: bool,
}

impl Life {
    fn generation(&mut self) -> Option<Landing> {
        let open = self.clear.as_deref().filter(|_| self.o.walls);
        self.world.step(open);
        self.guest -= 1.0;
        let mut landed = None;
        let due = self.guests.is_some() && self.guest <= 0.0;
        if self.watch.still(self.world.population()) || due {
            if let Some(g) = &mut self.guests {
                landed = g.drop(&mut self.world, open, self.clear.as_deref());
                self.guest = g.wait(self.o.rate) * self.o.speed;
            }
            if self.guests.is_none() || landed.is_none() && self.world.population() == 0 {
                self.world.soup(&mut self.dice);
            }
            self.watch.reset();
        }
        landed
    }

    /// Fits the world, the canvas and the map of the text to the view.
    fn fit(&mut self) -> bool {
        let (w, h) = self.sight.size;
        if w == 0 || h == 0 {
            return false;
        }
        let (ww, wh) = (2 * w as usize, 4 * h as usize);
        let fresh = self.world.width() == 0;
        let sized = (ww, wh) != (self.world.width(), self.world.height());
        if sized {
            self.world.resize(ww, wh);
            self.watch.reset();
            self.shown = Picture::new(w, h);
            self.canvas = None; // dropped: its sprite removed
            self.canvas = match Canvas::new(w, h) {
                Some(c) => Some(c),
                None => {
                    log(&format!("life: no canvas of {w}x{h}"));
                    exit(1)
                }
            };
        }
        if self.sight.changed || sized {
            self.clear = Some(wall(&self.sight.rows, w as usize, h as usize));
        }
        if fresh {
            let open = self.clear.as_deref().filter(|_| self.o.walls);
            match &mut self.guests {
                Some(g) => drop(g.open(&mut self.world, open, self.clear.as_deref())),
                None => self.world.soup(&mut self.dice),
            }
        }
        true
    }

    fn draw(&mut self, landed: &[Landing]) {
        let clear = self.clear.as_deref();
        picture(
            &self.world,
            self.o.colour,
            self.o.trail,
            clear,
            &mut self.shown,
        );
        let Some(canvas) = &mut self.canvas else {
            return;
        };
        let shown = &self.shown;
        shown
            .dots
            .paint(canvas, 0, 0, |x, y| Color::Palette(shown.ink(x, y)));
        canvas.present(&json!({"anchor": {"at": [0, 0], "space": "view"}}));
        if !self.o.particles {
            return;
        }
        let w = self.world.words();
        let seen = |&(x, y): &(usize, usize)| {
            clear.is_none_or(|c| c[4 * y * w + 2 * x / 64] >> (2 * x % 64) & 1 != 0)
        };
        let mut hit: Vec<(usize, usize)> = blasts(&self.world).into_iter().filter(seen).collect();
        let mut calls = Vec::new();
        for _ in 0..hit.len().min(BLASTS) {
            let at = hit.swap_remove(self.fx.below(hit.len()));
            let s = spark(&mut self.fx, at, "⠶⠒⠂", (2.0, 8.0), (0.3, 0.7), 0.0);
            calls.push(json!({"method": "draw.put", "params": s}));
        }
        for l in landed {
            for _ in 0..14 {
                let s = spark(
                    &mut self.fx,
                    (l.x / 2, l.y / 4),
                    "⣿⠿⠶⠒⠂",
                    (4.0, 14.0),
                    (0.4, 0.9),
                    8.0,
                );
                calls.push(json!({"method": "draw.put", "params": s}));
            }
        }
        if !calls.is_empty()
            && let Err(e) = call("batch", json!({"calls": calls}))
            && !self.told
        {
            self.told = true;
            log(&format!("life: the particles: {e}"));
        }
    }
}

impl App for Life {
    fn init(args: Value) -> Life {
        let o = match Options::of(&args) {
            Ok(o) => o,
            Err(e) => {
                log(&format!("life: {e}"));
                exit(2)
            }
        };
        let mut host = Host;
        let mut dice = Seeded::new(host.next_u64());
        let fx = Seeded::new(host.next_u64());
        let mut guests = o
            .guests
            .then(|| Guests::new(Seeded::new(dice.next_u64()), &o.kinds));
        let guest = guests.as_mut().map_or(0.0, |g| g.wait(o.rate) * o.speed);
        if o.cursor
            && let Err(e) = subscribe(&["hover"])
        {
            log(&format!("life: hover: {e}"));
        }
        period((1000.0 / o.speed).round().max(33.0) as u32);
        let ages = if o.colour { AGES.len() } else { 1 };
        let gone = if o.trail { TRAIL.len() } else { 1 };
        Life {
            sight: Sight::new(Look::Shape, false),
            world: World::new(0, 0, ages, gone),
            watch: Watch::default(),
            clear: None,
            canvas: None,
            shown: Picture::new(0, 0),
            guests,
            guest,
            dice,
            fx,
            done: 0,
            wiped: Vec::new(),
            told: false,
            o,
        }
    }

    fn tick(&mut self, now_ms: u64) {
        let due = (now_ms as f64 * self.o.speed / 1000.0) as u64 + 1;
        let n = due.saturating_sub(self.done).min(CATCH_UP);
        self.done = self.done.max(due);
        if n == 0 || !self.sight.look() || !self.fit() {
            return;
        }
        let mut landed = Vec::new();
        for _ in 0..n {
            landed.extend(self.generation());
        }
        for (x, y) in std::mem::take(&mut self.wiped) {
            self.world.wipe(x, y, REACH);
        }
        self.draw(&landed);
    }

    fn event(&mut self, topic: &str, data: Value) {
        if self.sight.event(topic, &data) {
            return;
        }
        if topic == "hover"
            && let Some(cell) = touched(&data, self.sight.size)
        {
            self.wiped.push(cell);
        }
    }
}

ptyrc_app::app!(Life);

#[cfg(test)]
mod tests {
    use super::*;

    /// `test_life_options`; the bounds the manifest says are the launcher's.
    #[test]
    fn options() {
        let o = Options::of(&json!({"speed": 10, "rate": 3, "kinds": ["ship", "gun"]})).unwrap();
        assert_eq!(o.kinds, [Kind::Ship, Kind::Gun]);
        let all = Options::of(&json!({"kinds": ["all"], "all": true})).unwrap();
        assert_eq!(all.kinds, Kind::ALL);
        assert!(all.colour && all.trail && all.guests && all.particles && all.walls && all.cursor);
        assert_eq!(Options::of(&json!({"kinds": []})).unwrap().kinds, Kind::ALL);
        assert_eq!(Options::of(&json!({})).unwrap().rate, guests::RATE);
        assert!(Options::of(&json!({"kinds": ["fish"]})).is_err());
        assert!(Options::of(&json!({"speed": 0})).is_err());
        assert!(Options::of(&json!({"speed": -1})).is_err());
    }

    /// `test_life_touched`.
    #[test]
    fn the_cell_under_the_pointer() {
        let size = (80, 24);
        let at = |v: Value| touched(&v, size);
        assert_eq!(
            at(json!({"sprite": 3, "at": [2, 1], "x": 2, "y": 1, "pos": [2, 1]})),
            Some((5, 6))
        );
        assert_eq!(
            at(json!({"sprite": null, "at": null, "x": 0, "y": 0, "pos": null})),
            None
        );
        assert_eq!(at(json!({"sprite": 3, "at": [80, 0], "pos": null})), None);
        assert_eq!(
            at(json!({"sprite": 4, "at": [0, 0], "pos": [79, 23]})),
            Some((159, 94))
        );
    }

    /// The cost of a tick of `-a` on a view of 200 by 60, natively:
    /// `cargo test --release -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_tick_on_200x60() {
        let (w, h) = (200u32, 60u32);
        let rows: Vec<String> = (0..h)
            .map(|y| {
                if y % 3 == 0 {
                    "words in the way ".repeat(8)
                } else {
                    String::new()
                }
            })
            .collect();
        let clear = wall(&rows, w as usize, h as usize);
        let mut world = World::new(2 * w as usize, 4 * h as usize, AGES.len(), TRAIL.len());
        let mut dice = Seeded::new(4);
        let mut guests = Guests::new(Seeded::new(5), &Kind::ALL);
        let mut shown = Picture::new(w, h);
        let mut canvas = Canvas::new(w, h).unwrap();
        let mut times = Vec::new();
        for g in 0..300 {
            let t = std::time::Instant::now();
            if g == 0 {
                guests.open(&mut world, Some(&clear), Some(&clear));
                world.soup(&mut dice);
            }
            world.step(Some(&clear));
            if g % 10 == 0 {
                guests.drop(&mut world, Some(&clear), Some(&clear));
            }
            world.wipe(200, 120, REACH);
            picture(&world, true, true, Some(&clear), &mut shown);
            shown
                .dots
                .paint(&mut canvas, 0, 0, |x, y| Color::Palette(shown.ink(x, y)));
            std::hint::black_box(blasts(&world));
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let first = times[0];
        times.sort_by(f64::total_cmp);
        let (median, worst) = (times[times.len() / 2], times[times.len() - 1]);
        eprintln!(
            "a tick of -a, 200x60: first {first:.2} ms, median {median:.2} ms, worst {worst:.2} ms"
        );
    }

    #[test]
    fn a_spark() {
        let mut d = Seeded::new(1);
        let s = spark(&mut d, (3, 4), "⠶⠒⠂", (2.0, 8.0), (0.3, 0.7), 0.0);
        assert_eq!(s["anchor"]["at"], json!([3, 4]));
        let frames = s["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0]["cells"][0][0]["bold"], true);
        assert_eq!(frames[2]["cells"][0][0]["dim"], true);
        assert!((800..=1200).contains(&s["ttl_ms"].as_u64().unwrap()));
    }
}
