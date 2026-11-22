// SPDX-License-Identifier: GPL-3.0-or-later
//! A ball of yarn and its thread: the twin's `Yarn`, but its word, its
//! time, its player and its sprites (the world's).
//!
//! A bat (`knock`) sends it along and up. It moves in dots, 2 by 4 a
//! cell (`px`, `py`: its top left dot; `vx`, `vy`: dots a second),
//! `TICK` a step: it falls, bounces on the text and on the floor of the
//! view, off its sides, rolls slower and slower, and stops on a whole
//! cell. Its thread: the dots it went through (`points`: column, row,
//! the row where it rests); a dot in the air sinks a row a step, down
//! to the floor of its column, where it lies as a `wave`.

use std::collections::HashSet;

use ptyrc_app::canvas::Braille;

use crate::look::{self, Look, YARNS};
use crate::place::{round, wave};
use crate::twister::Twister;

/// Seconds a step of the ball, of its thread.
pub const TICK: f64 = 1.0 / 30.0;
/// Bats a ball takes.
pub const PLAYS: (i64, i64) = (3, 5);
/// What a bat gives it along; and up, one of: a hop, a jump, a leap.
pub const BAT: (f64, f64) = (18.0, 34.0);
pub const LIFT: [(f64, f64); 3] = [(24.0, 32.0), (36.0, 48.0), (68.0, 74.0)];
pub const GRAVITY: f64 = 150.0;
pub const FALL: f64 = 90.0;
/// Share of its speed a floor gives back; under `SOFT`, none.
pub const BOUNCE: f64 = 0.5;
pub const SOFT: f64 = 14.0;
/// Share a side gives back.
pub const WALL: f64 = 0.6;
/// One time in three: this share, this speed at least.
pub const KICK: (f64, f64) = (1.2, 26.0);
pub const GRIP: f64 = 28.0;
pub const CREEP: f64 = 4.0;
/// Dots of thread, at most.
pub const THREAD: usize = 1200;

/// What stops a ball: the twin's `Home.solid`, `view[1]`.
pub trait Ground {
    fn solid(&self, x: i64, y: i64) -> bool;
    /// The first row of the view.
    fn top(&self) -> i64;
}

#[derive(Clone, Debug)]
pub struct Yarn {
    pub px: f64,
    pub py: f64,
    pub vx: f64,
    pub vy: f64,
    /// Where it rolls, or last did.
    pub way: i64,
    pub moving: bool,
    /// Dots, to the right.
    pub rolled: f64,
    pub color: u8,
    /// Bats left.
    pub plays: i64,
    pub points: Vec<[i64; 3]>,
    /// Where the thread ends.
    pub tip: Option<(i64, i64)>,
    /// The points that sink.
    pub loose: Vec<usize>,
    /// The dots that lie.
    pub down: HashSet<(i64, i64)>,
    /// Bats taken.
    pub stroke: i64,
    pub due: f64,
    /// The ball as drawn: `(ix, iy, turn, plays > 0)`.
    pub shown: Option<(i64, i64, i64, bool)>,
    /// The thread changed since drawn.
    pub fresh: bool,
    /// It stopped in the text: the world looks again (`Home.stale`).
    pub stale: bool,
}

/// What `show` draws: the thread at a cell, the ball at a cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub wire: Option<(i64, i64, Look)>,
    pub ball: Option<(i64, i64, Look)>,
}

impl Yarn {
    /// A ball at the cell `x`, on the row above `g`.
    pub fn new(rng: &mut Twister, x: i64, g: i64) -> Yarn {
        let color = rng.choice(&YARNS);
        let plays = rng.randint(PLAYS.0, PLAYS.1);
        Yarn {
            px: 2.0 * x as f64,
            py: 4.0 * (g - 1) as f64,
            vx: 0.0,
            vy: 0.0,
            way: 1,
            moving: false,
            rolled: 0.0,
            color,
            plays,
            points: Vec::new(),
            tip: None,
            loose: Vec::new(),
            down: HashSet::new(),
            stroke: 0,
            due: f64::INFINITY,
            shown: None,
            fresh: false,
            stale: false,
        }
    }

    pub fn x(&self) -> i64 {
        round(self.px) >> 1
    }

    pub fn g(&self) -> i64 {
        (round(self.py) + 3).div_euclid(4) + 1
    }

    /// Its box at the dot `(px, py)` has a solid cell in it.
    pub fn hits(&self, ground: &impl Ground, px: f64, py: f64) -> bool {
        let (y0, y1) = ((py / 4.0).floor() as i64, ((py + 4.0) / 4.0).ceil() as i64);
        let (x0, x1) = ((px / 2.0).floor() as i64, ((px + 4.0) / 2.0).ceil() as i64);
        (y0..y1).any(|cy| (x0..x1).any(|cx| ground.solid(cx, cy)))
    }

    /// A bat to the right (`way` 1) or to the left (-1).
    pub fn knock(&mut self, rng: &mut Twister, way: i64, now: f64) {
        self.plays -= 1;
        self.stroke += 1;
        self.way = way;
        self.vx = way as f64 * rng.uniform(BAT.0, BAT.1);
        let lift = rng.choice(&LIFT);
        self.vy = -rng.uniform(lift.0, lift.1);
        self.moving = true;
        self.due = self.due.min(now);
    }

    /// `dt` seconds: along, then down; an axis that would enter a solid
    /// cell is turned back instead.
    pub fn fly(&mut self, ground: &impl Ground, rng: &mut Twister, dt: f64) {
        let on = self.vy >= 0.0 && self.hits(ground, self.px, self.py + 0.5);
        if on {
            self.vy = 0.0;
            self.py = 4.0 * ((self.py + 0.5) / 4.0).floor();
            let mut speed = self.vx.abs() - GRIP * dt;
            if speed < CREEP {
                let at = round(self.px);
                if at.rem_euclid(2) == 0 {
                    return self.stop(ground, at);
                }
                speed = CREEP;
            }
            self.vx = self.way as f64 * speed;
        } else {
            self.vy = (self.vy + GRAVITY * dt).min(FALL);
        }
        let nx = self.px + self.vx * dt;
        if self.hits(ground, nx, self.py) {
            let mut back = WALL * self.vx.abs();
            if self.vx.abs() > 2.0 * CREEP && rng.random() < 1.0 / 3.0 {
                back = (KICK.0 * self.vx.abs()).max(KICK.1);
            }
            self.way = -self.way;
            self.vx = self.way as f64 * back;
        } else {
            self.rolled += nx - self.px;
            self.px = nx;
        }
        let ny = self.py + self.vy * dt;
        if !self.hits(ground, self.px, ny) {
            self.py = ny;
        } else if self.vy > 0.0 {
            self.py = 4.0 * (ny / 4.0).floor();
            self.vx *= 0.85;
            self.vy = if self.vy > SOFT {
                -self.vy * BOUNCE
            } else {
                0.0
            };
        } else {
            self.vy *= -0.4;
        }
    }

    fn stop(&mut self, ground: &impl Ground, at: i64) {
        (self.px, self.vx, self.vy, self.moving) = (at as f64, 0.0, 0.0, false);
        if self.hits(ground, self.px, self.py) {
            self.stale = true;
        }
    }

    /// The row of dots where a dot of the thread at `(x, y)` lies once
    /// fallen: over the first solid cell under it.
    pub fn floor(ground: &impl Ground, x: i64, y: i64) -> i64 {
        let (cx, mut cy) = (x >> 1, y >> 2);
        if ground.solid(cx, cy) {
            return y;
        }
        while !ground.solid(cx, cy + 1) {
            cy += 1;
        }
        4 * cy + 3 - wave(x)
    }

    /// Thread from where it ends to the ball.
    pub fn lay(&mut self, ground: &impl Ground) {
        let x = round(self.px) + if self.way > 0 { 1 } else { 2 };
        let y = round(self.py) + 3;
        let on = self.vy == 0.0 && self.hits(ground, self.px, self.py + 0.5);
        let (ax, ay) = self.tip.unwrap_or((x, y));
        let mut n = if on {
            (x - ax).abs()
        } else {
            (x - ax).abs().max((y - ay).abs())
        };
        if self.tip.is_none() {
            n = 1;
        }
        self.tip = Some((x, y));
        for i in 1..=n {
            let qx = round(ax as f64 + ((x - ax) * i) as f64 / n as f64);
            let mut qy = round(ay as f64 + ((y - ay) * i) as f64 / n as f64);
            let mut rest = Yarn::floor(ground, qx, qy);
            if on && rest <= qy {
                qy = rest.max(qy - 2);
            }
            rest = rest.max(qy);
            if qy < rest {
                self.loose.push(self.points.len());
            } else if self.down.contains(&(qx, qy)) {
                continue;
            } else {
                self.down.insert((qx, qy));
            }
            self.points.push([qx, qy, rest]);
            self.fresh = true;
        }
        if self.points.len() > THREAD {
            let cut = self.points.len() - THREAD;
            self.points.drain(..cut);
            self.slack();
        }
    }

    /// What sinks, what lies: counted again.
    pub fn slack(&mut self) {
        self.loose = (0..self.points.len())
            .filter(|&i| self.points[i][1] < self.points[i][2])
            .collect();
        self.down = self
            .points
            .iter()
            .filter(|p| p[1] == p[2])
            .map(|p| (p[0], p[1]))
            .collect();
    }

    /// A row of dots down, for the dots in the air; one that lands on
    /// thread is dropped.
    pub fn sink(&mut self) {
        if self.loose.is_empty() {
            return;
        }
        let mut landed = false;
        for &i in &self.loose {
            let p = &mut self.points[i];
            p[1] += 1;
            if p[1] == p[2] {
                landed = true;
                if self.down.contains(&(p[0], p[1])) {
                    p[2] = -1;
                } else {
                    self.down.insert((p[0], p[1]));
                }
            }
        }
        if landed {
            self.points.retain(|p| p[2] >= 0);
            self.slack();
        }
        self.fresh = true;
    }

    /// The text changed: a ball that was batted falls if nothing holds
    /// it, and its thread; one never batted stays by its word.
    pub fn wake(&mut self, ground: &impl Ground, now: f64) {
        for i in 0..self.points.len() {
            let [x, y, _] = self.points[i];
            self.points[i][2] = y.max(Yarn::floor(ground, x, y));
        }
        self.slack();
        if self.stroke != 0 && !self.moving {
            self.moving = !self.hits(ground, self.px, self.py + 0.5);
        }
        if self.moving || !self.loose.is_empty() {
            self.due = self.due.min(now);
        }
    }

    /// The thread: the cell of its top left corner, its rows; `keep`:
    /// one dot in so many. `None`: nothing of it in the view.
    pub fn strand(&self, ground: &impl Ground, keep: i64) -> Option<(i64, i64, Vec<String>)> {
        let top = 4 * ground.top();
        let points: Vec<&[i64; 3]> = self.points.iter().filter(|p| p[1] >= top).collect();
        if points.is_empty() {
            return None;
        }
        let x0 = points.iter().map(|p| p[0]).min()? >> 1;
        let y0 = points.iter().map(|p| p[1]).min()? >> 2;
        let x1 = points.iter().map(|p| p[0]).max()? >> 1;
        let y1 = points.iter().map(|p| p[1]).max()? >> 2;
        let mut dots = Braille::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32);
        let mut last: Option<(i64, i64)> = None;
        for p in points {
            let (x, y) = (p[0], p[1]);
            let (ax, ay) = match last {
                Some(l) if (x - l.0).abs() <= 1 => l,
                _ => (x, y),
            };
            let joint: Vec<(i64, i64)> = if (y - ay).abs() > 2 {
                let hx = if y > ay { x } else { ax };
                (y.min(ay)..=y.max(ay)).map(|qy| (hx, qy)).collect()
            } else {
                let n = 1.max((x - ax).abs()).max((y - ay).abs());
                (0..=n)
                    .map(|i| {
                        (
                            round(ax as f64 + ((x - ax) * i) as f64 / n as f64),
                            round(ay as f64 + ((y - ay) * i) as f64 / n as f64),
                        )
                    })
                    .collect()
            };
            for (qx, qy) in joint {
                if (qx + qy).rem_euclid(keep) == 0 {
                    dots.set(qx - 2 * x0, qy - 4 * y0, true);
                }
            }
            last = Some((x, y));
        }
        for (cx, cy) in dots.cells() {
            if ground.solid(x0 + i64::from(cx), y0 + i64::from(cy)) {
                dots.set_cell(cx, cy, 0);
            }
        }
        Some((x0, y0, dots.rows()))
    }

    /// The thread, thinner and thinner, once.
    pub fn fade(&self, ground: &impl Ground) -> Option<Look> {
        let frames: Vec<_> = [1, 2, 4]
            .iter()
            .filter_map(|&k| self.strand(ground, k))
            .map(|(_, _, rows)| (look::thread_cells(&rows, self.color, true), 300))
            .collect();
        (!frames.is_empty()).then_some(Look::Frames {
            frames,
            looped: false,
        })
    }

    /// What is to draw now: the thread if it changed, the ball if it
    /// moved.
    pub fn show(&mut self, ground: &impl Ground) -> Drawn {
        let (ix, iy) = (round(self.px), round(self.py));
        let ball = (
            ix,
            iy,
            round(self.rolled / 2.0).rem_euclid(4),
            self.plays > 0,
        );
        let wire = if self.fresh {
            self.strand(ground, 1)
        } else {
            None
        };
        self.fresh = false;
        let mut out = Drawn {
            wire: None,
            ball: None,
        };
        if let Some((x0, y0, rows)) = wire {
            out.wire = Some((
                x0,
                y0,
                Look::Still(look::thread_cells(&rows, self.color, false)),
            ));
        }
        if Some(ball) != self.shown {
            let l = look::ball_look(self.color, self.plays, ix & 1, iy & 3, ball.2);
            out.ball = Some((ix >> 1, iy >> 2, l));
            self.shown = Some(ball);
        }
        out
    }

    /// The steps due, three at most, and one picture.
    pub fn step(&mut self, ground: &impl Ground, rng: &mut Twister, now: f64) -> Drawn {
        let steps = 1 + (((now - self.due) / TICK).trunc() as i64).clamp(0, 2);
        for _ in 0..steps {
            if self.moving {
                let n = 1.max((self.vx.abs().max(self.vy.abs()) * TICK).ceil() as i64);
                for _ in 0..n {
                    if self.moving {
                        self.fly(ground, rng, TICK / n as f64);
                        self.lay(ground);
                    }
                }
            }
            self.sink();
        }
        let drawn = self.show(ground);
        if !self.moving && self.loose.is_empty() {
            self.due = f64::INFINITY;
        } else {
            self.due += steps as f64 * TICK;
            if self.due < now {
                self.due = now + TICK;
            }
        }
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::pad;
    use crate::look::tests::{fnv, text, twin};
    use crate::place::mass;

    struct Walls {
        view: (i64, i64, i64, i64),
        walls: Vec<Vec<bool>>,
    }

    impl Ground for Walls {
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

    fn py_list(points: &[[i64; 3]]) -> String {
        let p: Vec<String> = points
            .iter()
            .map(|p| format!("[{}, {}, {}]", p[0], p[1], p[2]))
            .collect();
        format!("[{}]", p.join(", "))
    }

    fn py_strand(s: &(i64, i64, Vec<String>)) -> String {
        let rows: Vec<String> = s.2.iter().map(|r| format!("'{r}'")).collect();
        format!("({}, {}, [{}])", s.0, s.1, rows.join(", "))
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn the_twins() {
        let screens: [(&[&str], (i64, i64, i64, i64), i64, i64, u64); 2] = [
            (
                &[
                    "",
                    "",
                    "",
                    "hello world, this is a line",
                    "",
                    "",
                    "",
                    "  short",
                    "",
                    "",
                    "a long line of text with words in it and more",
                ],
                (0, 0, 60, 14),
                10,
                4,
                3,
            ),
            (
                &[
                    "",
                    "",
                    "",
                    "",
                    "     a stair of text",
                    "",
                    "",
                    "                 and lower down",
                    "",
                    "",
                    "",
                    "                  at the very end",
                ],
                (0, 0, 50, 13),
                21,
                5,
                11,
            ),
        ];
        let want: std::collections::HashMap<_, _> = twin("yarn.").into_iter().collect();
        for (i, (rows, view, x, g, seed)) in screens.iter().enumerate() {
            let (w, h) = ((view.2 - view.0) as usize, (view.3 - view.1) as usize);
            let mut all: Vec<&str> = rows.to_vec();
            all.resize(h, "");
            let rows = pad(&all, w);
            let walls = rows
                .iter()
                .map(|r| mass(&r.chars().collect::<Vec<_>>()))
                .collect();
            let mut ground = Walls { view: *view, walls };
            let mut rng = Twister::new(*seed);
            let mut yarn = Yarn::new(&mut rng, *x, *g);
            assert_eq!(
                format!("{} {}", yarn.color, yarn.plays),
                want[format!("yarn.{i}.new").as_str()]
            );
            let (mut now, mut way) = (0.0, 1);
            for k in 0..400 {
                if !yarn.moving && yarn.loose.is_empty() && yarn.plays > 0 && k % 40 == 0 {
                    yarn.knock(&mut rng, way, now);
                    way = -way;
                }
                let mut drawn = Vec::new();
                if now >= yarn.due {
                    let d = yarn.step(&ground, &mut rng, now);
                    for (x, y, z, l) in [d.wire.map(|w| (w, -3)), d.ball.map(|b| (b, -2))]
                        .into_iter()
                        .flatten()
                        .map(|((x, y, l), z)| (x, y, z, l))
                    {
                        drawn.push(format!("{x},{y},{z},{}", fnv(&text(&l))));
                    }
                }
                if k == 250 {
                    ground.walls[(*g - 1) as usize] = vec![false; w];
                    yarn.wake(&ground, now);
                }
                let st = [yarn.px, yarn.py, yarn.vx, yarn.vy, yarn.rolled].map(f64::to_bits);
                let mut line: Vec<String> = st.iter().map(u64::to_string).collect();
                for v in [
                    yarn.way,
                    i64::from(yarn.moving),
                    yarn.plays,
                    yarn.points.len() as i64,
                    yarn.x(),
                    yarn.g(),
                ] {
                    line.push(v.to_string());
                }
                line.push(fnv(&py_list(&yarn.points)));
                line.push(
                    yarn.strand(&ground, 1)
                        .map_or("-".into(), |s| fnv(&py_strand(&s))),
                );
                line.push(u8::from(yarn.stale).to_string());
                line.push(if drawn.is_empty() {
                    "-".into()
                } else {
                    drawn.join(";")
                });
                assert_eq!(
                    line.join(" "),
                    want[format!("yarn.{i}.{k}").as_str()],
                    "yarn.{i}.{k}"
                );
                now += if k % 3 != 0 { TICK / 2.0 } else { TICK * 1.7 };
            }
            let fade = yarn.fade(&ground).map_or("-".into(), |l| fnv(&text(&l)));
            assert_eq!(fade, want[format!("yarn.{i}.fade").as_str()]);
        }
    }
}
