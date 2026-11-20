// SPDX-License-Identifier: GPL-3.0-or-later
//! Patterns of Conway's game of life, by kind: the bodies of their RLE
//! (`b` dead, `o` alive, `$` end of row, a count before each), under
//! the names the LifeWiki (conwaylife.com) gives them. What the table
//! says of each is checked by running it (`world::tests::the_library`).
//!
//! - `Still`: a still life, `period` 1.
//! - `Oscillator`: itself again after `period` generations.
//! - `Ship`: itself again after `period` generations, `step` cells away.
//! - `Seed`: small and long-lived, a methuselah or the start of
//!   something else.
//! - `Gun`: sends gliders.
//! - `Puffer`: moves and leaves debris, or ships (a rake).
//! - `Filler`: grows in every direction until the world is full.

/// A pattern as rows of cells, `true` alive; every row as wide.
pub type Art = Vec<Vec<bool>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Still,
    Oscillator,
    Ship,
    Seed,
    Gun,
    Puffer,
    Filler,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Ship,
        Kind::Seed,
        Kind::Oscillator,
        Kind::Puffer,
        Kind::Gun,
        Kind::Still,
        Kind::Filler,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Still => "still",
            Kind::Oscillator => "oscillator",
            Kind::Ship => "ship",
            Kind::Seed => "seed",
            Kind::Gun => "gun",
            Kind::Puffer => "puffer",
            Kind::Filler => "filler",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// `period`: 0 when the pattern never comes back; `step`: cells a
/// period, `(x, y)`.
#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub name: &'static str,
    pub kind: Kind,
    pub rle: &'static str,
    pub period: u32,
    pub step: (i32, i32),
}

impl Pattern {
    pub fn art(&self) -> Art {
        decode(self.rle)
    }
}

/// The pattern named `name`.
pub fn named(name: &str) -> Option<&'static Pattern> {
    LIBRARY.iter().find(|p| p.name == name)
}

/// The rows of the body of an RLE, padded to one width. `None`: not RLE.
pub fn decode(rle: &str) -> Art {
    let mut rows: Art = Vec::new();
    let mut row = Vec::new();
    let mut n = 0usize;
    for c in rle.chars() {
        if let Some(d) = c.to_digit(10) {
            n = n * 10 + d as usize;
            continue;
        }
        let count = n.max(1);
        n = 0;
        match c {
            'b' => row.extend(std::iter::repeat_n(false, count)),
            'o' => row.extend(std::iter::repeat_n(true, count)),
            '$' => {
                rows.push(std::mem::take(&mut row));
                rows.extend(std::iter::repeat_n(Vec::new(), count - 1));
            }
            '!' => break,
            _ => {}
        }
    }
    rows.push(row);
    let w = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(w, false);
    }
    rows
}

/// `art` in the orientation `k` of its 8: bit 0 transposes, then bit 1
/// mirrors `x`, bit 2 mirrors `y`.
pub fn turn(art: &[Vec<bool>], k: u32) -> Art {
    let mut out: Art = art.to_vec();
    if k & 1 != 0 && !out.is_empty() {
        let (w, h) = (out[0].len(), out.len());
        out = (0..w)
            .map(|x| (0..h).map(|y| art[y][x]).collect())
            .collect();
    }
    if k & 2 != 0 {
        out.iter_mut().for_each(|row| row.reverse());
    }
    if k & 4 != 0 {
        out.reverse();
    }
    out
}

/// `step` of a pattern, once turned by `turn(art, k)`.
pub fn heading((dx, dy): (i32, i32), k: u32) -> (i32, i32) {
    let (dx, dy) = if k & 1 != 0 { (dy, dx) } else { (dx, dy) };
    (
        if k & 2 != 0 { -dx } else { dx },
        if k & 4 != 0 { -dy } else { dy },
    )
}

const fn p(
    kind: Kind,
    name: &'static str,
    rle: &'static str,
    period: u32,
    step: (i32, i32),
) -> Pattern {
    Pattern {
        name,
        kind,
        rle,
        period,
        step,
    }
}

use Kind::{Filler, Gun, Oscillator, Puffer, Seed, Ship, Still};

/// Every pattern; 240.
#[rustfmt::skip]
pub static LIBRARY: [Pattern; 240] = [
    p(Still, "block", "2o$2o", 1, (0, 0)),
    p(Still, "snake", "2obo$ob2o", 1, (0, 0)),
    p(Still, "boat", "2ob$obo$bo", 1, (0, 0)),
    p(Still, "ship", "2ob$obo$b2o", 1, (0, 0)),
    p(Still, "tub", "bob$obo$bo", 1, (0, 0)),
    p(Still, "bi-block", "2ob2o$2ob2o", 1, (0, 0)),
    p(Still, "aircraft-carrier", "2o2b$o2bo$2b2o", 1, (0, 0)),
    p(Still, "beehive", "b2ob$o2bo$b2o", 1, (0, 0)),
    p(Still, "shillelagh", "2o3b$o2b2o$b2obo", 1, (0, 0)),
    p(Still, "barge", "bo2b$obob$bobo$2bo", 1, (0, 0)),
    p(Still, "eater-1", "2o2b$obob$2bob$2b2o", 1, (0, 0)),
    p(Still, "loaf", "b2ob$o2bo$bobo$2bo", 1, (0, 0)),
    p(Still, "long-boat", "bo2b$obob$bobo$2b2o", 1, (0, 0)),
    p(Still, "long-ship", "2o2b$obob$bobo$2b2o", 1, (0, 0)),
    p(Still, "pond", "b2ob$o2bo$o2bo$b2o", 1, (0, 0)),
    p(Still, "block-on-table", "2b2o$2b2o2$4o$o2bo", 1, (0, 0)),
    p(Still, "hat", "2bo2b$bobob$bobob$2ob2o", 1, (0, 0)),
    p(Still, "mango", "b2o2b$o2bob$bo2bo$2b2o", 1, (0, 0)),
    p(Still, "canoe", "3b2o$4bo$3bob$obo2b$2o", 1, (0, 0)),
    p(Still, "cis-boat-with-tail", "3b2o$3bob$2obob$obo2b$bo", 1, (0, 0)),
    p(Still, "integral-sign", "3b2o$2bobo$2bo2b$obo2b$2o", 1, (0, 0)),
    p(Still, "long-barge", "bo3b$obo2b$bobob$2bobo$3bo", 1, (0, 0)),
    p(Still, "bookends", "2o3b2o$obobobo$2bobo2b$b2ob2o", 1, (0, 0)),
    p(Still, "beehive-with-tail", "b2o3b$o2bo2b$b2obob$4bob$4b2o", 1, (0, 0)),
    p(Still, "honeycomb", "2b2o2b$bo2bob$ob2obo$bo2bob$2b2o", 1, (0, 0)),
    p(Still, "dead-spark-coil", "2o3b2o$obobobo$2bobo2b$obobobo$2o3b2o", 1, (0, 0)),
    p(Still, "elevener", "4b2o$3bobo$3bo2b$b3o2b$o5b$2o", 1, (0, 0)),
    p(Still, "scorpion", "3bo3b$b3o3b$o3b2ob$obobobo$b2obobo$5bo", 1, (0, 0)),
    p(Still, "eater-2", "3bob2o$b3ob2o$o6b$b3ob2o$3bobob$3bobob$4bo", 1, (0, 0)),
    p(Still, "bakery", "4b2o4b$3bo2bo3b$3bobo4b$b2obo3bob$o2bo3bobo$obo3bo2bo$bo3bob2ob$4bobo3b$3bo2bo3b$4b2o", 1, (0, 0)),
    p(Still, "cthulhu", concat!(
        "bo3bo3bo$obobobobobo$obobobobobo$b2obobob2o$4bobo$4bobo$3b2ob2o$2bo4bo$2b2ob2o$3bobo$2bo2b",
        "o$2bobo$b2ob2o",
    ), 1, (0, 0)),
    p(Still, "honey-farm", "6bo6b$5bobo5b$5bobo5b$6bo6b2$b2o7b2ob$o2bo5bo2bo$b2o7b2ob2$6bo6b$5bobo5b$5bobo5b$6bo", 1, (0, 0)),
    p(Oscillator, "blinker", "3o", 2, (0, 0)),
    p(Oscillator, "toad", "b3o$3o", 2, (0, 0)),
    p(Oscillator, "beacon", "2o2b$o3b$3bo$2b2o", 2, (0, 0)),
    p(Oscillator, "clock", "2bob$obob$bobo$bo", 2, (0, 0)),
    p(Oscillator, "bipole", "2o3b$obo2b2$2bobo$3b2o", 2, (0, 0)),
    p(Oscillator, "pentadecathlon", "2bo4bo2b$2ob4ob2o$2bo4bo", 15, (0, 0)),
    p(Oscillator, "monogram", "2o3b2o$bobobob$b2ob2ob$bobobob$2o3b2o", 4, (0, 0)),
    p(Oscillator, "figure-eight", "2o4b$2obo2b$4bob$bo4b$2bob2o$4b2o", 8, (0, 0)),
    p(Oscillator, "mold", "3b2ob$2bo2bo$o2bobo$4bob$ob2o2b$bo", 4, (0, 0)),
    p(Oscillator, "quad", "2o2b2o$o2bobo$bo4b$4bob$obo2bo$2o2b2o", 2, (0, 0)),
    p(Oscillator, "tripole", "2o4b$obo3b2$2bobob$5bo$4b2o", 2, (0, 0)),
    p(Oscillator, "spark-coil", "2o4b2o$obo2bobo$2bo2bo2b$obo2bobo$2o4b2o", 2, (0, 0)),
    p(Oscillator, "test-tube-baby", "2o4b2o$obo2bobo$2bo2bo2b$2bo2bo2b$3b2o", 2, (0, 0)),
    p(Oscillator, "jam", "3b2ob$2bo2bo$o2bobo$o3bob$o5b$3bo2b$b2o", 3, (0, 0)),
    p(Oscillator, "tumbler", "bo5bob$obo3bobo$o2bobo2bo$2bo3bo2b$2b2ob2o", 14, (0, 0)),
    p(Oscillator, "caterer", "2bo5b$o3b4o$o3bo3b$o7b$3bo4b$b2o", 3, (0, 0)),
    p(Oscillator, "1-beacon", "2b2o3b$bobo3b$o2bob2o$2obo2bo$bobo3b$bo2bo2b$2b2o", 2, (0, 0)),
    p(Oscillator, "fox", "4bo2b$4bo2b$2bo2bob$2o5b$4bobo$2bobobo$6bo", 2, (0, 0)),
    p(Oscillator, "mazing", "3b2o2b$bobo3b$o5bo$bo3b2o2$3bobob$4bo", 4, (0, 0)),
    p(Oscillator, "skewed-quad", "b2o4b$bo3b2o$2bobobo2$obobo2b$2o3bob$4b2o", 2, (0, 0)),
    p(Oscillator, "smiley", "3ob3o$bobobob2$bo3bob2$obobobo$2bobo", 8, (0, 0)),
    p(Oscillator, "traffic-light", "2b3o2b2$o5bo$o5bo$o5bo2$2b3o", 2, (0, 0)),
    p(Oscillator, "why-not", "3bo3b$3bobob$bo5b$ob5o$bo5b$3bobob$3bo", 2, (0, 0)),
    p(Oscillator, "blocker", "6bobob$5bo4b$2o2bo4bo$2obo2bob2o$4b2o", 8, (0, 0)),
    p(Oscillator, "candlefrobra", "5bo4b$bob2obob2o$obo3bob2o$bo4bo3b$5b2o", 3, (0, 0)),
    p(Oscillator, "fumarole", "3b2o3b$bo4bob$bo4bob$bo4bob$2bo2bo2b$obo2bobo$2o4b2o", 5, (0, 0)),
    p(Oscillator, "light-bulb", "b2obo2b$bob2o2b2$2b3o2b$bo3bob$bo3bob$2bobo2b$obobobo$2o3b2o", 2, (0, 0)),
    p(Oscillator, "4-boats", "3bo4b$2bobo3b$bob2o3b$obo2b2ob$b2o2bobo$3b2obob$3bobo2b$4bo", 2, (0, 0)),
    p(Oscillator, "cross", "2b4o2b$2bo2bo2b$3o2b3o$o6bo$o6bo$3o2b3o$2bo2bo2b$2b4o", 3, (0, 0)),
    p(Oscillator, "great-on-off", "2b2o4b$bo2bo3b$bobo4b$2obo2bob$4b2obo$7bo$4b3ob$4bo", 2, (0, 0)),
    p(Oscillator, "octagon-2", "3b2o3b$2bo2bo2b$bo4bob$o6bo$o6bo$bo4bob$2bo2bo2b$3b2o", 5, (0, 0)),
    p(Oscillator, "pentapole", "2o6b$obo5b2$2bobo3b2$4bobob$7bo$6b2o", 2, (0, 0)),
    p(Oscillator, "phoenix-1", "3bo4b$3bobo2b$bo6b$6b2o$2o6b$6bob$2bobo3b$4bo", 2, (0, 0)),
    p(Oscillator, "pulsar-quadrant", "5bo2b$3b3o2b$2bo3b2o$o2bo2bob$o3bobob$o4bo2b2$2b3o", 3, (0, 0)),
    p(Oscillator, "unix", "b2o5b$b2o5b2$bo6b$obo5b$o2bo2b2o$4bob2o$2b2o", 6, (0, 0)),
    p(Oscillator, "coes-p8", "2o10b$2o2b2o6b$5b2o5b$4bo2bo4b$7bo2b2o$5bobo2b2o", 8, (0, 0)),
    p(Oscillator, "elkies-p5", "bo7b$o2b3o3b$2bo6b$3bobo2bo$2b2ob4o$4bo4b$4bobo2b$5b2o", 5, (0, 0)),
    p(Oscillator, "two-eaters", "2o7b$bo7b$bobo5b$2b2o5b$5b2o2b$5bobob$7bob$7b2o", 3, (0, 0)),
    p(Oscillator, "koks-galaxy", "2bo2bobob$2obob3ob$bo6bo$2o5bob2$bo5b2o$o6bob$b3obob2o$bobo2bo", 8, (0, 0)),
    p(Oscillator, "two-pulsar-quadrants", "4bo4b$4bo4b$3b2o4b$2bo6b$o2bo2b3o$o3bobo2b$o4bo3b2$2b3o", 3, (0, 0)),
    p(Oscillator, "laputa", "3b2ob2o4b$3b2obo3b2o$8bo2bo$b6ob3ob$o2bobo6b$2o3bob2o3b$4b2ob2o", 2, (0, 0)),
    p(Oscillator, "1-2-3", "2b2o6b$o2bo6b$2obob2o3b$bobo2bo3b$bo4bob2o$2b3obob2o$5bo4b$4bo5b$4b2o", 3, (0, 0)),
    p(Oscillator, "surprise", "3bo4b2o$3b3o2bob$b2o3bobob$o2b2obob2o$bo6bob$2obob2o2bo$bobo3b2ob$bo2b3o3b$2o4bo", 3, (0, 0)),
    p(Oscillator, "blonker", "o2b2o4bo$2o2bob2obo$4bobo$5b2o$7bo$7bo3bo$9bobo$10bo", 6, (0, 0)),
    p(Oscillator, "candelabra", "4b2o4b2o4b$bo2bo6bo2bob$obobo6bobobo$bo2bob4obo2bob$4bobo2bobo4b$5bo4bo", 3, (0, 0)),
    p(Oscillator, "achims-p4", concat!(
        "2b2o3b2o2b$bo2bobo2bob$bob2ob2obob$2o7b2o$2bobobobo2b$2o7b2o$bob2ob2obob$bo2bobo2bob$2b2o3",
        "b2o",
    ), 4, (0, 0)),
    p(Oscillator, "mathematician", "4bo4b$3bobo3b$3bobo3b$2b2ob2o2b$o7bo$3o3b3o2$9o$o7bo$3b4o2b$3bo2b2o", 5, (0, 0)),
    p(Oscillator, "burloaferimeter", "4b2o4b$5bo4b$4bo5b$3bob3o2b$3bobo2bob$2obo3bobo$2obo4bob$4b4o2b2$4b2o4b$4b2o", 7, (0, 0)),
    p(Oscillator, "karels-p15", "2bo4bo2b$2b6o2b$2bo4bo2b4$2b6o2b$bo6bob$o8bo$bo6bob$2b6o", 15, (0, 0)),
    p(Oscillator, "t-nosed-p4", "4b3o4b$5bo5b2$4b3o4b$3bo3bo3b$2bo5bo2b$2bobobobo2b$b2obobob2ob$o2b2ob2o2bo$2o7b2o", 4, (0, 0)),
    p(Oscillator, "revolver", "o12bo$3o4bo3b3o$3bobobo2bo3b$2bo6bobo2b$2bobo6bo2b$3bo2bobobo3b$3o3bo4b3o$o12bo", 2, (0, 0)),
    p(Oscillator, "pentant", "2o8b$bo8b$bobo6b$2b2o4b2o$9bo$5b4ob$5bo4b$2bo3b3ob$2b4o2bob$5bo4b$4bo5b$4b2o", 5, (0, 0)),
    p(Oscillator, "confused-eaters", "o10b$3o8b$3bo7b$2bo8b$2bo2bo5b$5bo5b$3bobo5b$3b2o2b2o2b$7bobob$9bob$9b2o", 4, (0, 0)),
    p(Oscillator, "heart", "5bo5b$4bo2bo3b$bo2bo2bo3b$obobobob2ob$bo2bo2bo3b$4bo5bo$5b5ob2$7bo3b$6bobo2b$7bo", 5, (0, 0)),
    p(Oscillator, "star", "4b3o4b2$2bobobobo2b2$obo5bobo$o9bo$obo5bobo2$2bobobobo2b2$4b3o", 3, (0, 0)),
    p(Oscillator, "hertz-oscillator", "3b2obo4b$3bob2o4b2$4b3o4b$3bobobob2o$3bo3bob2o$2obo3bo3b$2obo3bo3b$4b3o4b2$4b2obo3b$4bob2o", 8, (0, 0)),
    p(Oscillator, "hustler", "5b2o4b$5b2o4b2$3b4o4b$obo4bo3b$2obo3bo3b$3bo3bob2o$3bo4bobo$4b4o3b2$4b2o5b$4b2o", 3, (0, 0)),
    p(Oscillator, "negentropy", concat!(
        "3b2obo4b$3bob2o4b2$4b3o4b$3bobobob2o$3b2o2bob2o$2obo3bo3b$2obo3bo3b$4b3o4b2$4b2obo3b$4bob2",
        "o",
    ), 2, (0, 0)),
    p(Oscillator, "rats", "5b2o5b$6bo5b$4bo7b$2obob4o3b$2obo5bobo$3bo2b3ob2o$3bo4bo3b$4b3obo3b$7bo4b$6bo5b$6b2o", 6, (0, 0)),
    p(Oscillator, "toaster", concat!(
        "2bo6b2o$bobob2o2bob$bobobobobob$2obo3bob2o$o2b2ob2o2bo$bo7bob$bo7bob$o2b2ob2o2bo$2obo3bob2",
        "o$bobobobobob$bobob2o2bob$2bo6b2o",
    ), 5, (0, 0)),
    p(Oscillator, "38p7.2", concat!(
        "4bo3bo4b$o2bobobobo2bo$obo2bobo2bobo$bo2b2ob2o2bob$5bobo5b$2b2o5b2o2b$2bo7bo2b$4bo3bo4b2$b",
        "o2bo3bo2bob$2b2o5b2o",
    ), 7, (0, 0)),
    p(Oscillator, "cauldron", "5bo5b$4bobo4b$5bo5b2$3b5o3b$obo5bobo$2obo3bob2o$3bo3bo3b$3bo3bo3b$4b3o4b2$4b2obo3b$4bob2o", 8, (0, 0)),
    p(Oscillator, "clock-2", "6b2o4b$6b2o4b2$4b4o4b$2obo4bo3b$2obo2bobo3b$3bo2bobob2o$3bobo2bob2o$4b4o4b2$4b2o6b$4b2o", 4, (0, 0)),
    p(Oscillator, "pinwheel", "6b2o4b$6b2o4b2$4b4o4b$2obo4bo3b$2obo2bobo3b$3bo3b2ob2o$3bobo2bob2o$4b4o4b2$4b2o6b$4b2o", 4, (0, 0)),
    p(Oscillator, "pseudo-barberpole", "10b2o$11bo$9bo2b$7bobo2b2$5bobo4b2$3bobo6b2$2b2o8b$o11b$2o", 5, (0, 0)),
    p(Oscillator, "queen-bee-shuttle", "9bo12b$7bobo12b$6bobo13b$2o3bo2bo11b2o$2o4bobo11b2o$7bobo12b$9bo", 30, (0, 0)),
    p(Oscillator, "pentoad", "11b2o$11bob$9bobob$9b2o2b$5b2o6b$6bo6b$6bo6b$6b2o5b$2b2o9b$bobo9b$bo11b$2o", 5, (0, 0)),
    p(Oscillator, "t-nosed-p6", concat!(
        "4b2o3b2o4b$4bobobobo4b$5bo3bo5b2$o3b2o3b2o3bo$obob2o3b2obobo$o3b2o3b2o3bo2$5bo3bo5b$4bobob",
        "obo4b$4b2o3b2o",
    ), 6, (0, 0)),
    p(Oscillator, "achims-p16", concat!(
        "7b2o4b$7bobo3b$2bo4bob2o2b$b2o5bo4b$o2bo9b$3o10b2$10b3o$9bo2bo$4bo5b2ob$2b2obo4bo2b$3bobo7",
        "b$4b2o",
    ), 16, (0, 0)),
    p(Oscillator, "diamond-ring", concat!(
        "6bo6b$5bobo5b$4bobobo4b$4bo3bo4b$2b2o2bo2b2o2b$bo4bo4bob$obob2ob2obobo$bo4bo4bob$2b2o2bo2b",
        "2o2b$4bo3bo4b$4bobobo4b$5bobo5b$6bo",
    ), 3, (0, 0)),
    p(Oscillator, "dinner-table", concat!(
        "bo11b$b3o7b2o$4bo6bob$3b2o4bobob$9b2o2b$6bo6b$4b2obo5b2$2bo3bo2bo3b$bob2o4bo3b$bo6bo4b$2o7",
        "b3ob$11bo",
    ), 12, (0, 0)),
    p(Oscillator, "pulsar", concat!(
        "2b3o3b3o2b2$o4bobo4bo$o4bobo4bo$o4bobo4bo$2b3o3b3o2b2$2b3o3b3o2b$o4bobo4bo$o4bobo4bo$o4bob",
        "o4bo2$2b3o3b3o",
    ), 3, (0, 0)),
    p(Oscillator, "worker-bee", "2o12b2o$bo12bob$bobo8bobob$2b2o8b2o2b2$5b6o5b2$2b2o8b2o2b$bobo8bobob$bo12bob$2o12b2o", 9, (0, 0)),
    p(Oscillator, "chemist", concat!(
        "7bo7b$7b3o5b$10bo4b$5b3o2bo2b2o$4bobobobobobo$4bo3bobobo2b$b2obo5bob2ob$2bobobo3bo4b$obobo",
        "bobobo4b$2o2bo2b3o5b$4bo10b$5b3o7b$7bo",
    ), 5, (0, 0)),
    p(Oscillator, "rattlesnake", "8b2o$8bo$9bo$8b2o4$5bo$5b2o$2obobob3o$ob2obobobo$6bo2b3o$7b2o3bo$9b3o$9bo", 11, (0, 0)),
    p(Oscillator, "roteightor", concat!(
        "bo12b$b3o8b2o$4bo7bob$3b2o5bobob$10b2o2b2$6b2o6b$5b2obo5b$6b3o5b$2b2o3b3o4b$bobo5b2o3b$bo7",
        "bo4b$2o8b3ob$12bo",
    ), 8, (0, 0)),
    p(Oscillator, "unicycle", concat!(
        "2b2o3b4o3b$2b2o2bo4bo2b$2obo2bo3bobob$3o4bo3bobo$13bo$13bo$b2o7bo2bo$o2bo7b2ob$o13b$o13b$o",
        "bo3bo4b3o$bobo3bo2bob2o$2bo4bo2b2o2b$3b4o3b2o",
    ), 6, (0, 0)),
    p(Oscillator, "eureka", "bo14bob$obo4bo7bobo$bo3b2ob2o6bob$7bo10b4$7bo10b$bo3b2ob2o6bob$obo4bo7bobo$bo14bo", 30, (0, 0)),
    p(Oscillator, "sombreros", concat!(
        "3b2o8b2o3b$3bobo6bobo3b$5bo6bo5b$3bob2o4b2obo3b$b3o10b3ob$o3bobo4bobo3bo$b3o10b3ob$3bob2o4",
        "b2obo3b$5bo6bo5b$3bobo6bobo3b$3b2o8b2o",
    ), 6, (0, 0)),
    p(Oscillator, "airforce", concat!(
        "7bo6b$6bobo5b$7bo6b2$5b5o4b$4bo5bob2o$3bob2o3bob2o$3bobo2bobo3b$2obo3b2obo3b$2obo5bo4b$4b5",
        "o5b2$6bo7b$5bobo6b$6bo",
    ), 7, (0, 0)),
    p(Oscillator, "statorless-p5", concat!(
        "o13bo$b2o9b2o$o4b2ob2o4bo$2obo7bob2o$bobo2bobo2bobo$2bobo5bobo$2b3o5b3o$2b3o5b3o$2bobo5bob",
        "o$bobo2bobo2bobo$2obo7bob2o$o4b2ob2o4bo$b2o9b2o$o13bo",
    ), 5, (0, 0)),
    p(Oscillator, "ellison-p4-hw-emulator", concat!(
        "11b2o11b$4bo3bo6bo3bo4b$3bobo12bobo3b$3bobo12bobo3b$2obobob10obobob2o$2obo16bob2o$3bo16bo3",
        "b$3bobo4b2obo4bobo3b$4b2o4bob2o4b2o",
    ), 4, (0, 0)),
    p(Oscillator, "snacker", concat!(
        "2o16b2o$bo16bob$bobo12bobob$2b2o12b2o2b$7bo4bo7b$5b2ob4ob2o5b$7bo4bo7b$2b2o12b2o2b$bobo12b",
        "obob$bo16bob$2o16b2o",
    ), 9, (0, 0)),
    p(Oscillator, "jolson", concat!(
        "b2o6b2o2b$o2bo4bo2bob$o2bo4bo2bob$o2bo4bo2bob$b2o6b2o2b3$7bo5b$5bo2bob2ob$6b2o2b2ob3$6b4o3",
        "b$5b6o2b$4b8ob$3b2o6b2o$4b8ob$5b6o2b$6b4o",
    ), 15, (0, 0)),
    p(Oscillator, "bakers-dozen", concat!(
        "2o9b2o10b$4obo5b2o10b$obo2b3o15b$11bo11b$4b2o4bobo10b$4bo5bo2bo4bo4b$11b2o4b2o4b2$15b3o2bo",
        "bo$10b2o5bob4o$10b2o9b2o",
    ), 12, (0, 0)),
    p(Oscillator, "44p7.2", concat!(
        "11bo4b$10bobo3b$10bobo3b$9b2obob2o$12b2obo$8bob2o4b$11bo4b$8b3o5b$5bobo8b$3bo3bob2o5b$b3ob",
        "obob2o5b$o4b2o9b$b4o11b$4bo11b$3bo12b$3b2o",
    ), 7, (0, 0)),
    p(Oscillator, "octagon-4", concat!(
        "7b2o$7b2o2$6b4o$5bo4bo$4bo6bo$3bo8bo$2obo8bob2o$2obo8bob2o$3bo8bo$4bo6bo$5bo4bo$6b4o2$7b2o",
        "$7b2o",
    ), 4, (0, 0)),
    p(Oscillator, "carnival-shuttle", concat!(
        "33bo3bo$2o3b2o26b5o$bobobo3bo2bo6b2o3bo2bo7bo2b$b2ob2o2b2o3b2o4b2o2b2o3b2o4bobob$bobobo3bo",
        "2bo6b2o3bo2bo7bo2b$2o3b2o26b5o$33bo3bo",
    ), 12, (0, 0)),
    p(Oscillator, "harbor", concat!(
        "5b2o3b2o5b$5bobobobo5b$6bo3bo6b2$5b2o3b2o5b$2o2bobo3bobo2b2o$obob2o5b2obobo$bo13bob2$bo13b",
        "ob$obob2o5b2obobo$2o2bobo3bobo2b2o$5b2o3b2o5b2$6bo3bo6b$5bobobobo5b$5b2o3b2o",
    ), 5, (0, 0)),
    p(Oscillator, "turning-toads", concat!(
        "15bo6bo14b$14b2o5b2o6b2o6b$6b3obobob2obobob2obobo10b$2b2obo6bobo4bobo4bobo2bob2o2b$o2bobo3",
        "bo18b4obo2bo$2obobo27bob2o$3bo29bo3b$3b2o27b2o",
    ), 4, (0, 0)),
    p(Oscillator, "twin-bees-shuttle", "17b2o10b$2o15bobo7b2o$2o17bo7b2o$17b3o9b4$17b3o9b$2o17bo9b$2o15bobo9b$17b2o", 46, (0, 0)),
    p(Oscillator, "crown", concat!(
        "16b2o5b$3b2o13bo4b$2bo12bo7b$5bo3b3o3bo3bo3b$bo3bo4bo4bo3b4o$obo2bo11bo5b$o2bo19b$b2o20b$6",
        "b2o8b2o5b$6bo2b6o2bo5b$7b2o6b2o6b$4b3o10b3o3b$4bo2bobo4bobo2bo3b$5b2o4b2o4b2o",
    ), 12, (0, 0)),
    p(Oscillator, "ship-in-a-bottle", concat!(
        "4b2o6b2o4b$3bo2bo4bo2bo3b$3bobo6bobo3b$b2o2b3o2b3o2b2ob$o6bo2bo6bo$ob2o10b2obo$bobo10bobob",
        "$3b2o3b2o3b2o3b$7bobo8b$7b2o9b$3b2o8b2o3b$bobo10bobob$ob2o10b2obo$o6bo2bo6bo$b2o2b3o2b3o2b",
        "2ob$3bobo6bobo3b$3bo2bo4bo2bo3b$4b2o6b2o",
    ), 16, (0, 0)),
    p(Oscillator, "windmill", concat!(
        "11bo6b$9b2obo5b$7b2o9b$10b2o6b$7b3o8b2$3o15b$3b2o2b3ob2o5b$10b7ob$b7o10b$5b2ob3o2b2o3b$15b",
        "3o2$8b3o7b$6b2o10b$9b2o7b$5bob2o9b$6bo",
    ), 4, (0, 0)),
    p(Oscillator, "swine", concat!(
        "33bo$9bo2b2o7bo10bobo$o2b2o2bobobo5bo3bo7bo2b2o$o10bo2bobo3bo4bo2bobob2ob2o$o3bo4bo4b2obob",
        "o2bob2obo4b2o$3b2o4bob2obo2bobob2o4bo4bo3bo$2ob2obobo2bo4bo3bobo2bo10bo$3b2o2bo7bo3bo5bobo",
        "bo2b2o2bo$2bobo10bo7b2o2bo$3bo",
    ), 5, (0, 0)),
    p(Oscillator, "gourmet", concat!(
        "10b2o8b$10bo9b$4b2ob2obo4b2o3b$2bo2bobobo5bo4b$2b2o4bo8bo2b$16b2o2b2$16b2o2b$o10bo3bobo2b$",
        "3o7b3o3bo3b$3bo6bobo4b3o$2bobo14bo$2b2o16b2$2b2o16b$2bo8bo4b2o2b$4bo5bobobo2bo2b$3b2o4bob2",
        "ob2o4b$9bo10b$8b2o",
    ), 32, (0, 0)),
    p(Oscillator, "hebdarole", concat!(
        "4b2o3bo4bo3b2o4b$bo2bo2bo2bo2bo2bo2bo2bob$obobob2obo4bob2obobobo$bo2bo2bobob2obobo2bo2bob$",
        "4b2o3b2o2b2o3b2o4b$8bo6bo8b$7bo8bo7b$6bo10bo6b$7b2o6b2o7b2$3b2o2b2o6b2o2b2o3b$3bo2bo10bo2b",
        "o3b$4bobo3bo2bo3bobo4b$3b2obobob4obobob2o3b$7b2o6b2o7b$9b2o2b2o9b$9bo2bobo9b$10b2o",
    ), 7, (0, 0)),
    p(Oscillator, "gabriels-p138", concat!(
        "8b3o10b$7bo2bo10b$7bo2bo10b$7b2o12b4$17b3ob$17bo2bo$20bo$3o15b3o$o20b$o2bo17b$b3o17b4$12b2",
        "o7b$10bo2bo7b$10bo2bo7b$10b3o",
    ), 138, (0, 0)),
    p(Oscillator, "sixty-nine", concat!(
        "9bo11b$8bobo10b2$6bo3b2o9b$5bo5bo9b$6bobo12b$8b2o6bo4b$16bo4b$2bo5b2o4b3o4b$2bo11b2o5b$3o7",
        "b2o2b2o2b3o$2o6bob2o4b3o2b$2o2b3obobo5b3o2b$2b3o16b$2b3o6bo9b$10bobo8b2$8bo3b2o7b$7bo5bo7b",
        "$8bobo10b$10b2o",
    ), 4, (0, 0)),
    p(Oscillator, "p54-shuttle", concat!(
        "2o25b2o$bo25bob$bobo7bo13bobob$2b2o5bo2bo5bo6b2o2b$12bo5b2o9b$8bo10b2o8b$8bo3b2o4b2o9b$9b5",
        "o15b2$9b5o15b$8bo3b2o4b2o9b$8bo10b2o8b$12bo5b2o9b$2b2o5bo2bo5bo6b2o2b$bobo7bo13bobob$bo25b",
        "ob$2o25b2o",
    ), 54, (0, 0)),
    p(Oscillator, "achims-p144", concat!(
        "2o24b2o$2o24b2o$18b2o8b$17bo2bo7b$18b2o8b$14bo13b$13bobo12b$12bo3bo11b$12bo2bo12b2$12bo2bo",
        "12b$11bo3bo12b$12bobo13b$13bo14b$8b2o18b$7bo2bo17b$8b2o18b$2o24b2o$2o24b2o",
    ), 144, (0, 0)),
    p(Oscillator, "jasons-p11", concat!(
        "8bob2ob2obo$8b2obobob2o2$8b3o3b3o$7bo3bobo3bo$7bobobobobobo$11bobo$4b2o2b3o3b3o2b2o$2obo3b",
        "o9bo3bob2o$bobobobo9bobobobo$o2bo3bo9bo3bo2bo$2o2b3o11b3o2b2o2$2o2b3o11b3o2b2o$o2bo3bo9bo3",
        "bo2bo$bobobobo9bobobobo$2obo3bo9bo3bob2o$4b2o2b3o3b3o2b2o$11bobo$7bobobobobobo$7bo3bobo3bo",
        "$8b3o3b3o2$8b2obobob2o$8bob2ob2obo",
    ), 11, (0, 0)),
    p(Oscillator, "pre-pulsar-shuttle-29", concat!(
        "15bo12b$13b3o12b$12bo15b$12b2o14b2$bo26b$obo4bo20b$bo5b2o6b2o11b$7bo7b2o11b3$19b2o7b$7bo11",
        "b2o5b2o$bo5b2o17bob$obo4bo16bobob$bo22b2o2b4$14bo5bo7b$13b3o3b3o6b5$13bo7bo6b$12bobo5bobo5",
        "b$13bo7bo",
    ), 29, (0, 0)),
    p(Oscillator, "centinal", concat!(
        "2o48b2o$bo48bob$bobo21b2o21bobob$2b2o8bo12b2o12b2o7b2o2b$11b2o26bobo10b$10b2o29bo10b$11b2o",
        "2b2o22b3o10b4$11b2o2b2o22b3o10b$10b2o29bo10b$11b2o26bobo10b$2b2o8bo12b2o12b2o7b2o2b$bobo21",
        "b2o21bobob$bo48bob$2o48b2o",
    ), 100, (0, 0)),
    p(Oscillator, "ring-of-fire", concat!(
        "16bo17b$14bobobo15b$12bobobobobo13b$10bobobobobobobo11b$8bobobo2b2obobobobo9b$6bobobobo6bo",
        "2bobobo7b$4bobobo2bo10bobobobo5b$5b2obo14bo2bobobo3b$3bo3bo18bob2o4b$4b3o20bo3bo2b$2bo25b3",
        "o3b$3b2o27bob$bo3bo24b2o2b$2b4o23bo3bo$o29b3ob$b3o29bo$o3bo23b4o2b$2b2o24bo3bob$bo27b2o3b$",
        "3b3o25bo2b$2bo3bo20b3o4b$4b2obo18bo3bo3b$3bobobo2bo14bob2o5b$5bobobobo10bo2bobobo4b$7bobob",
        "o2bo6bobobobo6b$9bobobobob2o2bobobo8b$11bobobobobobobo10b$13bobobobobo12b$15bobobo14b$17bo",
    ), 2, (0, 0)),
    p(Oscillator, "popover", concat!(
        "21bo10b$20bobo9b2$13b2o3bo3b2o8b$13b2o2bo5bo8b$18bobo11b$20b2o10b$14b2o16b$3b2o8bo2bo15b$b",
        "obo9bobo4bo11b$o5bo7bo5bo11b$bo3b2o2b3o8bo11b2$3bobo16b2o3b2o3b$4bo12bo3bo2bo2b2o3b$8bo7b3",
        "o3bobo7b$7bobo6bobo4bo8b$3b2o2bo2bo16bo4b$3b2o3b2o16bobo3b2$11bo8b3o2b2o3bob$11bo13bo5bo$1",
        "1bo5b2o9bobob$16bobo8b2o3b$16b3o13b$10b2o20b$11bobo18b$8bo5bo2b2o13b$8b2o3bo3b2o13b2$9bobo",
        "20b$10bo",
    ), 32, (0, 0)),
    p(Oscillator, "superfountain", concat!(
        "15b2o6b2o6b2o2b2o9b$9b2o5bo6bo5bo2bo2bo2bob2o4b$4bo4bobob3o4b2obo5b2o5b3obobo3b$3bobo5bobo",
        "5bo2bo8b5o4bo2bo2b$4bo5bo2bo4bobo6b4o2bo3b2o2bobo2b$4bo7b4o2bob2o4bo4b5o3bobo2b2o$4b3o3bo5",
        "bobo2bo4b2o2bo3bo2b2obobobob$8b2o2bob2obob2o4b2o2bo4bo6b2obob$4bo3bo6bobo3b2ob2o3b4o4b2obo",
        "2bo2b$3bo5bobo5bo3bobob5obo6bob3o3b$4bo3b3o5bo4bo10b3o2bo8b$4b3ob2o2b2o2bo2b4o2bobob2obobo",
        "2bob2o5b$o3bo9bo3b2ob2obo2bo6bo2bo2bo5b$4b3ob2o2b2o2bo2bo3b3o6b2o4b2o6b$4bo3b3o5bo3bo3bo21",
        "b$3bo5bobo5bo5bo3bo18b$4bo3bo6bobo2b3o3b2o18b$8b2o2bob2obobo3b2o21b$4b3o3bo5bo2bobo3b3o18b",
        "$4bo7b4o3bob4o2bo18b$4bo5bo2bo6bo4bo20b$3bobo5bobo8bo2b2o19b$4bo4bobob3o5b2o23b$9b2o5bo29b",
        "$15b2o",
    ), 4, (0, 0)),
    p(Oscillator, "pre-pulsar-shuttle-26", concat!(
        "16bo3bo16b$10b2o4bo3bo4b2o10b$10bo5bo3bo5bo10b$7b2obo15bob2o7b$6bobob2o13b2obobo6b$6bobo6b",
        "o5bo6bobo6b$4b2o2bo5b3o3b3o5bo2b2o4b$3bo4b2o17b2o4bo3b$3b5o21b5o3b$7bo21bo7b$b4o27b4ob$bo2",
        "bo27bo2bob2$15b2o3b2o15b$6bo9bo3bo9bo6b$5b2o6bo9bo6b2o5b$3o3bo6b2o7b2o6bo3b3o4$3o3bo6b2o7b",
        "2o6bo3b3o$5b2o6bo9bo6b2o5b$6bo9bo3bo9bo6b$15b2o3b2o15b2$bo2bo27bo2bob$b4o27b4ob$7bo21bo7b$",
        "3b5o21b5o3b$3bo4b2o17b2o4bo3b$4b2o2bo5b3o3b3o5bo2b2o4b$6bobo6bo5bo6bobo6b$6bobob2o13b2obob",
        "o6b$7b2obo15bob2o7b$10bo5bo3bo5bo10b$10b2o4bo3bo4b2o10b$16bo3bo",
    ), 26, (0, 0)),
    p(Oscillator, "newshuttle", concat!(
        "19bo2b2obo3bo19b$17b3o2bob2o3b3o17b$7b2o7bo15bo7b2o7b$8bo7b2o5b3o5b2o7bo8b$8bobo11bo3bo11b",
        "obo8b$9b2o11b2ob2o11b2o9b2$2bo10b3o17b3o10bo2b$2b3o9bo19bo9b3o2b$5bo13b3o5b3o13bo5b$4b2o14",
        "bo7bo14b2o4b3$7bo33bo7b$7b2o31b2o7b$7bo33bo7b$2b2o18b2ob2o18b2o2b$bobo18bo3bo18bobob$bo21b",
        "3o21bob$2o7bo15bobo11bo7b2o$9b2o15b2o10b2o9b$9bo9b2o18bo9b$4b2o10b2o2bo10b2o10b2o2b2o$2obo",
        "bo10bob2o10bobo10bobo2bo$bobo14bo11bo14bobob$o2bobo10bobo10b2obo10bobob2o$2o2b2o10b2o10bo2",
        "b2o10b2o4b$9bo18b2o9bo9b$9b2o10b2o15b2o9b$2o7bo11bobo15bo7b2o$bo21b3o21bob$bobo18bo3bo18bo",
        "bob$2b2o18b2ob2o18b2o2b$7bo33bo7b$7b2o31b2o7b$7bo33bo7b3$4b2o14bo7bo14b2o4b$5bo13b3o5b3o13",
        "bo5b$2b3o9bo19bo9b3o2b$2bo10b3o17b3o10bo2b2$9b2o11b2ob2o11b2o9b$8bobo11bo3bo11bobo8b$8bo7b",
        "2o5b3o5b2o7bo8b$7b2o7bo15bo7b2o7b$17b3o3b2obo2b3o17b$19bo3bob2o2bo",
    ), 28, (0, 0)),
    p(Ship, "glider", "bob$2bo$3o", 4, (1, 1)),
    p(Ship, "lwss", "bo2bo$o4b$o3bo$4o", 4, (-2, 0)),
    p(Ship, "mwss", "3bo2b$bo3bo$o5b$o4bo$5o", 4, (-2, 0)),
    p(Ship, "hwss", "3b2o2b$bo4bo$o6b$o5bo$6o", 4, (-2, 0)),
    p(Ship, "25p3h1v0.2", "7b2obo5b$4b2obob2ob3ob$b4o2b2o6bo$o4bo3bo3b2ob$b2o", 3, (0, -1)),
    p(Ship, "sidecar", "bo6b$o5bob$o5bob$5obob2$4b2o2b$2bo4bo$bo6b$bo5bo$b6o", 4, (-2, 0)),
    p(Ship, "loafer", "b2o2bob2o$o2bo2b2o$bobo$2bo$8bo$6b3o$5bo$6bo$7b2o", 7, (-1, 0)),
    p(Ship, "coe-ship", "4b6o$2b2o5bo$2obo5bo$4bo3bob$6bo3b$6b2o2b$5b4ob$5b2ob2o$7b2o", 16, (8, 0)),
    p(Ship, "copperhead", "b2o2b2o$3b2o$3b2o$obo2bobo$o6bo2$o6bo$b2o2b2o$2b4o2$3b2o$3b2o", 10, (0, -1)),
    p(Ship, "x66", "2bo6b$2o7b$o2b3o2bo$o4b3ob$b3o2b2ob2$b3o2b2ob$o4b3ob$o2b3o2bo$2o7b$2bo", 4, (-2, 0)),
    p(Ship, "edge-repair-spaceship-1", "8bo7b$7b4o5b$2bo3bo3b2ob2ob$b4o5bo2b2ob$o3bo7bo2bo$bobo2bo9b$5bo", 3, (0, -1)),
    p(Ship, "turtle", concat!(
        "b3o7bo$b2o2bob2ob2o$3b3o4bob$bo2bobo3bob$o4bo4bob$o4bo4bob$bo2bobo3bob$3b3o4bob$b2o2bob2ob",
        "2o$b3o7bo",
    ), 3, (-1, 0)),
    p(Ship, "31p8h4v0", "b5o$bo4bo$bo$2bo4bo$4bo$5b2o2bo$3bo2bo3bo$bo7bo$o5bo2bo$o3b2o$4o2b2o", 8, (-4, 0)),
    p(Ship, "25p3h1v0.1", "10bo5b$8b3ob3ob$7b2o6bo$2bo3bo2bo3b2ob$b4o11b$o3bo11b$bobo2bo9b$5bo", 3, (0, -1)),
    p(Ship, "b29", concat!(
        "3b2o3b$2b2o4b$4bo3b$6b2o$5bo2b2$4bo2bo$b2obo3b$2o4bob$2bobo2bo$7bo$4bo2bo$5bobo$5bobo$6b2o",
        "$6bo",
    ), 4, (-1, -1)),
    p(Ship, "pushalong-1", "3bo7b$bo2bo6b$o3bo6b$o3bo6b$2obo7b$5b2o4b$2bo2b2o4b$4bo6b$7b3ob$6b5o$5b2ob3o$6b2o", 4, (-2, 0)),
    p(Ship, "30p5h2v0", "4bo8b$3b3o7b$2b2ob2o6b2$bobobobo2bo2b$2o3bo3b3ob$2o3bo6bo$10bobo$8bobo2b$9bo2bo$12bo", 5, (0, -2)),
    p(Ship, "dart", concat!(
        "7bo7b$6bobo6b$5bo3bo5b$6b3o6b2$4b2o3b2o4b$2bo3bobo3bo2b$b2o3bobo3b2ob$o5bobo5bo$bob2obobob",
        "2obo",
    ), 3, (0, -1)),
    p(Ship, "canada-goose", concat!(
        "3o10b$o9b2ob$bo6b3obo$3b2o2b2o4b$4bo8b$8bo4b$4b2o3bo3b$3bobob2o4b$3bobo2bob2ob$2bo4b2o4b$2",
        "b2o9b$2b2o",
    ), 4, (-1, -1)),
    p(Ship, "crab", "8b2o3b$7b2o4b$9bo3b$11b2o$10bo2b2$9bo2bo$b2o5b2o3b$2o5bo5b$2bo4bobo3b$4b2o2bo4b$4b2o", 4, (-1, -1)),
    p(Ship, "44p5h2v0", concat!(
        "4bo5bo4b$3b3o3b3o3b$2bo2bo3bo2bo2b$b3o7b3ob$2bobo5bobo2b$4b2o3b2o4b$o4bo3bo4bo$5bo3bo5b$2o",
        "3bo3bo3b2o$2bo2bo3bo2bo2b$4bo5bo",
    ), 5, (0, -2)),
    p(Ship, "hivenudger", concat!(
        "4o5bo2bo$o3bo3bo4b$o7bo3bo$bo2bo3b4ob2$5b2o6b$5b2o6b$5b2o6b2$bo2bo3b4ob$o7bo3bo$o3bo3bo4b$",
        "4o5bo2bo",
    ), 4, (-2, 0)),
    p(Ship, "orion-2", concat!(
        "b2o10b$2o11b$2bo10b$4bo4b3ob$4b3o4b2o$5b3obobob$12bo$5bobo5b$4b2obo5b$5bo7b$3b2obo6b$6bo6b",
        "$4b2o",
    ), 4, (-1, -1)),
    p(Ship, "weekender", concat!(
        "bo12bob$bo12bob$obo10bobo$bo12bob$bo12bob$2bo3b4o3bo2b$6b4o6b$2b4o4b4o2b2$4bo6bo4b$5b2o2b2",
        "o",
    ), 7, (0, -2)),
    p(Ship, "brain", concat!(
        "b3o9b3ob$obob2o5b2obobo$obobo7bobobo$bob2ob2ob2ob2obob$5bobobobo5b$3bobobobobobo3b$2b2obob",
        "obobob2o2b$2b3o2bobo2b3o2b$2b2o2bo3bo2b2o2b$bo4b2ob2o4bob$bo13bo",
    ), 3, (0, -1)),
    p(Ship, "46p4h1v0", concat!(
        "3bo11bo3b$3bo11bo3b$2bobo9bobo2b2$bo3bo7bo3bob$bob6ob6obob$o7bobo7bo$o7bobo7bo$o17bo$bob2o",
        "b2o3b2ob2obo",
    ), 4, (0, -1)),
    p(Ship, "wasp", concat!(
        "10bo3bo7b$7bobob2ob3o5b$6bo2bo6b2ob2ob$b2o2b2o2bo3bo2bo2b2ob$b2obob2o2bo2bo4bo2bo$o3bo4b2o",
        "11b$obobo2bo2b2o10b$9bo12b$b3o",
    ), 3, (0, -1)),
    p(Ship, "60p5h2v0", concat!(
        "5bo7bo5b$3b2ob2o3b2ob2o3b$6b2o3b2o6b$8bobo8b$bo4bobobobo4bob$3o5bobo5b3o$o5bobobobo5bo$2bo",
        "2bo2bobo2bo2bo2b$2b2o3b2ob2o3b2o2b$o7bobo7bo$o6b2ob2o6bo",
    ), 5, (0, -2)),
    p(Ship, "orion", concat!(
        "3b2o9b$3bobo8b$3bo10b$2obo10b$o4bo8b$ob2o6b3ob$5b3o4b2o$6b3obobob$13bo$6bobo5b$5b2obo5b$6b",
        "o7b$4b2obo6b$7bo6b$5b2o",
    ), 4, (-1, -1)),
    p(Ship, "spider", concat!(
        "9bo7bo9b$3b2obobob2o3b2obobob2o3b$3obob3o9b3obob3o$o3bobo5bobo5bobo3bo$4b2o6bobo6b2o4b$b2o",
        "9bobo9b2ob$b2ob2o15b2ob2ob$5bo15bo",
    ), 5, (0, -1)),
    p(Ship, "schick-engine", concat!(
        "bo2bo15b$o19b$o3bo15b$4o9b2o5b$6b3o5b2o4b$6b2ob2o6b3o$6b3o5b2o4b$4o9b2o5b$o3bo15b$o19b$bo2",
        "bo",
    ), 12, (-6, 0)),
    p(Ship, "37p4h1v0", "bo$bo8bo$obo5bo3bo$8bo3b2o$5bob2o5b2o$b6o2bo6bo$2b2o6bo3b3o$10bo3bob2o$13bo$18bo$17bo$17bo", 4, (0, -1)),
    p(Ship, "64p2h1v0", concat!(
        "5b3o15b3o5b$4bo3bo13bo3bo4b$3b2o4bo11bo4b2o3b$2bobob2ob2o3b3o3b2ob2obobo2b$b2obo4bob2ob3ob",
        "2obo4bob2ob$o4bo3bo4bobo4bo3bo4bo$12bo5bo12b$2o7b2o9b2o7b2o",
    ), 2, (0, -1)),
    p(Ship, "fireship", concat!(
        "4bo2bo$4bo2bo$3bo4bo$3bo4bo$3bo4bo$3bo4bo$2b3o2b3o$2bob4obo$3bo4bo3$5b2o$5b2o$5b2o$3bo4bo$",
        "b3o4b3o$3o6b3o$2o8b2o$b10o$2b8o$4b4o",
    ), 10, (0, -1)),
    p(Ship, "hammerhead", concat!(
        "5o13b$o4bo7b2o3b$o11b2ob3o$bo9b2ob4o$3b2o3b2ob2o2b2ob$5bo4bo2bo4b$6bobobobo5b$7bo10b$7bo10",
        "b$6bobobobo5b$5bo4bo2bo4b$3b2o3b2ob2o2b2ob$bo9b2ob4o$o11b2ob3o$o4bo7b2o3b$5o",
    ), 4, (-2, 0)),
    p(Ship, "swan", concat!(
        "bo10b2o10b$5o6b2o11b$o2b2o8bo7b2ob$2b2obo5b2o6b3obo$11b2o3bob2o4b$5bobo6b2o8b$10b3obo4bo4b",
        "$7b3o3bo4bo5b$8bo7bo7b$8bo6bo8b2$11bo",
    ), 4, (-1, -1)),
    p(Ship, "56p6h1v0", concat!(
        "5b3o10b3o5b$3obo7b2o7bob3o$4bo3bo2bo2bo2bo3bo4b$4bo5bo4bo5bo4b$10b2o2b2o10b$7bo3bo2bo3bo7b",
        "$7bobo6bobo7b$8b10o8b$10bo4bo10b$8bo8bo8b$7bo10bo7b$8bo8bo",
    ), 6, (0, -1)),
    p(Ship, "big-glider", concat!(
        "3b3o12b$3bo2b3o9b$4bobo11b$2o7bo8b$obo4bo2bo7b$o8b2o7b$b2o15b$bo2bo5bob2o4b$bo9b2obo3b$3bo",
        "bo6b2o2bob$4b2obo4b2o3bo$8bo7bob$7b4o3bobob$7bob2o3b4o$8bo3b2obo2b$13b2o3b$9bob3o4b$10bo2b",
        "o",
    ), 4, (-1, -1)),
    p(Ship, "sparky", concat!(
        "10b2obo12b2o2b$6b3obob3o8bob3o2b$2bobo10bo3b2obo4bo2b$2o4b2o5bo3b4obo3b2o2b$2bob2o2bo4b3ob",
        "o5b4obob$8bo4bo13b3o$2bob2o2bo4b3obo5b4obob$2o4b2o5bo3b4obo3b2o2b$2bobo10bo3b2obo4bo2b$6b3",
        "obob3o8bob3o2b$10b2obo12b2o",
    ), 4, (-1, 0)),
    p(Ship, "enterprise", concat!(
        "8bo12b$6b2obo11b$4b2o3bo11b$3bo3b2o12b$2bo4b2obo10b$bo2b2ob3obo9b$bo19b$3bobo3b2o10b$2o3bo",
        "bo13b$o3bo3b2o11b$bo2b2o2bo2bo9b$4b2o3bo8bo2b$10bobo3bobobo$12b2ob3o2bo$12b2o4b2ob$13bob2o",
        "4b2$11bobo7b$11bo9b$11b3o7b$12bobo",
    ), 4, (-1, -1)),
    p(Ship, "ecologist", concat!(
        "bo2bo5b2o15b$o8b4o14b$o3bo3b2ob2o14b$4o5b2o16b4$11b2o14b$2b3o7b2o13b$2bo6bo2bo14b$2bobo6bo",
        "15b$3b2o3b2o17b$25b2o$25bob$bo2bo18b3ob$o26b$o3bo22b$4o",
    ), 20, (-10, 0)),
    p(Ship, "58p5h1v1", concat!(
        "20b2ob$20b2ob$19bo2bo$16b2obo2bo$22bo$14b2o3bo2bo$14b2o5bob$15bob5ob$16bo6b3$13b3o7b$13bo9",
        "b$11b2o10b$5b2o4bo11b$5b3o3bo11b$3bo4bo14b$3bo3bo15b$7bo15b$2b2obobo15b$2o5bo15b$2o4b2o15b",
        "$2b4o",
    ), 5, (-1, -1)),
    p(Ship, "fly", concat!(
        "2bo31b$bobo30b$bobo22bobo3bob$bo23b2obobo2bo$11b3o8bo9bob$2o9b2o2bob2o3bo2b4o5b$bobo9b4o2b",
        "obo2b2o4b2o2b$b2o8bo2bo3b3o5b3o5b$2bo7bo4bo2b2o2b2o2bo2bo4b$3bo2bo3bo4bo2b3obobo4b2o3b$7bo",
        "b2o4bo2b4o5bo6b$4b2o3b2o4bo2b4o5bo6b$4bobo3bo4bo2b3obobo4b2o3b$3b2o5bo4bo2b2o2b2o2bo2bo4b$",
        "4bobo4bo2bo3b3o5b3o5b$5bo7b4o2bobo2b2o4b2o2b$11b2o2bob2o3bo2b4o5b$11b3o8bo9bob$25b2obobo2b",
        "o$26bobo3bo",
    ), 3, (-1, 0)),
    p(Ship, "snail", concat!(
        "bo36b$bo36b$o37b$b3o17b3o3b3o8b$b2obo9bo3bobo6b3o8b$2bo11b2obo7bo4b4o4b$6bo6bo3bobo3b2obo5",
        "b2o4b$3bo2bob3o3b2o9bo8b2obo$3b2obo5bo5bo17bob$9bob7o20b2$9bob7o20b$3b2obo5bo5bo17bob$3bo2",
        "bob3o3b2o9bo8b2obo$6bo6bo3bobo3b2obo5b2o4b$2bo11b2obo7bo4b4o4b$b2obo9bo3bobo6b3o8b$b3o17b3",
        "o3b3o8b$o37b$bo36b$bo",
    ), 5, (-1, 0)),
    p(Ship, "seal", concat!(
        "2b2o30b$b3ob3o26b$3bo3b2o25b$2ob2o2bo2bobo21b$o6bob4o21b$6b2obo2bo21b$o4b2o3bob2o20b$2o8bo",
        "bo2bo18b$o2b5ob3o4bo17b$bo9bo4bo17b$b2obo2bo3bo3bo18b$2bo9b2o20b$9b2o2b2o4bo14b$2bo3b2obo4",
        "b3o2bo14b$3b2obob5o5bob2o12b$7b2ob2ob2ob2o2b3o11b$17bobo4bo9b$8b2o4bobo3b6o8b$13bo7b2o11b$",
        "13bo3bo16b$13bo4bo3b3o6bo2b$14b5o3b2ob2o2b2ob2o$17b2obo2bob2o2bo4b$16b2o5bo4bo2b3o$15b3obo",
        "bobobo8b$22bo2bo8b$18bobo13b$19bo14b2$21b3o10b$21b3o10b2$20b2o12b$20b2o12b$21bo",
    ), 6, (-1, -1)),
    p(Ship, "77p6h1v1", concat!(
        "34bob$33bobo$31b2o2bo2$30bo5b$27b2o2b2o3b$27b2o7b$27bo8b$25bobo8b$25b2o9b$23bo2bo9b$22bo4b",
        "o8b$21bo5bo8b$18bo3bo13b$17bobo3b2o11b$16b2ob2o15b$15b2o4bobo12b$14b2o7bo12b$13bo9bo12b$14",
        "b2o20b$15bo20b$12bo3bo19b$11bobo22b$10bo3bob3o17b$14bo21b$8b2o26b$9b2o25b$5b4o2b2o23b$5b2o",
        "29b2$4bo31b$2bo2bo30b$2bo2bo30b$bo34b$o35b$b2o",
    ), 6, (-1, -1)),
    p(Ship, "p15-pre-pulsar-spaceship", concat!(
        "3b3o10b3o23b3o10b3o3b$4bobo8bobo25bobo8bobo4b$6bo8bo29bo8bo6b$bob5o6b5obo19bob5o6b5obob$bo",
        "b2o3b2o2b2o3b2obo19bob2o3b2o2b2o3b2obob$2o5b3o2b3o5b2o17b2o5b3o2b3o5b2o$5b2ob2o2b2ob2o27b2",
        "ob2o2b2ob2o5b2$4bo12bo25bo12bo4b$5b3o6b3o27b3o6b3o5b$4b4o6b4o25b4o6b4o4b$3b2o12b2o23b2o12b",
        "2o3b$4bo12bo25bo12bo4b$2b3o12b3o21b3o12b3o2b$2bo16bo21bo16bo2b$5bo10bo27bo10bo5b$3bobo10bo",
        "bo23bobo10bobo3b$2bo3b2o6b2o3bo21bo3b2o6b2o3bo2b$3bo5bo2bo5bo23bo5bo2bo5bo3b$9bo2bo35bo2bo",
        "9b$bob2o4bo2bo4b2obo19bob2o4bo2bo4b2obob$bo3b3obo2bob3o3bo19bo3b3obo2bob3o3bob$2bo6bo2bo6b",
        "o21bo6bo2bo6bo2b$bo2b2o3bo2bo3b2o2bo19bo2b2o3bo2bo3b2o2bob$bo2b2obobo2bobob2o2bo19bo2b2obo",
        "bo2bobob2o2bob$5bo2bo4bo2bo27bo2bo4bo2bo5b$6bobo4bobo29bobo4bobo6b$27bo5bo27b$4b2obob4obob",
        "2o9bo5bo9b2obob4obob2o4b$10b2o14bobo3bobo14b2o10b$2bob2o10b2obo7bo5bo7bob2o10b2obo2b$2o2bo",
        "3b2o2b2o3bo2b2o5bo5bo5b2o2bo3b2o2b2o3bo2b2o$b2obo12bob2o19b2obo12bob2ob$3b2obo8bob2o23b2ob",
        "o8bob2o3b$4bo2bo6bo2bo25bo2bo6bo2bo4b$b2o2bo2bo4bo2bo2b2o4b2o7b2o4b2o2bo2bo4bo2bo2b2ob$bo5",
        "b2o4b2o5bo5bobo3bobo5bo5b2o4b2o5bob$b3o3bo6bo3b3o6bo5bo6b3o3bo6bo3b3ob$3b4o8b4o23b4o8b4o3b",
        "$4bo2bo6bo2bo25bo2bo6bo2bo4b$4bo12bo25bo12bo4b$4bob2o6b2obo25bob2o6b2obo4b$5bo10bo27bo10bo",
    ), 15, (0, -3)),
    p(Seed, "t-tetromino", "3o$bo", 0, (0, 0)),
    p(Seed, "pi-heptomino", "3o$obo$obo", 0, (0, 0)),
    p(Seed, "r-pentomino", "b2o$2ob$bo", 0, (0, 0)),
    p(Seed, "b-heptomino", "ob2o$3ob$bo", 0, (0, 0)),
    p(Seed, "c-heptomino", "b3o$3ob$bo", 0, (0, 0)),
    p(Seed, "century", "2b2o$3ob$bo", 0, (0, 0)),
    p(Seed, "e-heptomino", "b3o$2o$b2o", 0, (0, 0)),
    p(Seed, "herschel", "o2b$3o$obo$2bo", 0, (0, 0)),
    p(Seed, "stairstep-hexomino", "2o2b$b2ob$2b2o", 0, (0, 0)),
    p(Seed, "gliders-by-the-dozen", "2o2bo$o3bo$o2b2o", 0, (0, 0)),
    p(Seed, "thunderbird", "3o2$bob$bob$bo", 0, (0, 0)),
    p(Seed, "teardrop", "b3o$o2bo$o2bo$b2o", 0, (0, 0)),
    p(Seed, "wing", "b2ob$o2bo$bobo$2b2o", 0, (0, 0)),
    p(Seed, "acorn", "bo5b$3bo3b$2o2b3o", 0, (0, 0)),
    p(Seed, "rabbits", "o3b3o$3o2bob$bo", 0, (0, 0)),
    p(Seed, "7468m", "4bob$4b2o$2ob2ob$o", 0, (0, 0)),
    p(Seed, "die-hard", "6bob$2o6b$bo3b3o", 0, (0, 0)),
    p(Seed, "multum-in-parvo", "3b3o$2bo2bo$bo4b$o", 0, (0, 0)),
    p(Seed, "switch-engine", "bobo2b$o5b$bo2bob$3b3o", 0, (0, 0)),
    p(Seed, "5x5-infinite-growth", "3obo$o4b$3b2o$b2obo$obobo", 0, (0, 0)),
    p(Seed, "bunnies", "o5bob$2bo3bob$2bo2bobo$bobo", 0, (0, 0)),
    p(Seed, "queen-bee", "3bo3b$2bobo2b$bo3bob$2b3o2b$2o3b2o", 0, (0, 0)),
    p(Seed, "bunnies-10", "bo4b$2obo2b$4b2o$o2bo2b$o5b$o", 0, (0, 0)),
    p(Seed, "10-cell-infinite-growth", "6bob$4bob2o$4bobob$4bo3b$2bo5b$obo", 0, (0, 0)),
    p(Seed, "bunnies-9", "bo6b$2o5bo$6bob$6bob$5bo2b$4bo3b$4bo", 0, (0, 0)),
    p(Seed, "blom", "o10bo$b4o6bo$2b2o7bo$10bob$8bobo", 0, (0, 0)),
    p(Seed, "time-bomb", "bo11b2o$obo4bo6bo$7bo4bo2b$2bo2bo3bo2bo2b$2b2o6bo4b$3bo", 0, (0, 0)),
    p(Seed, "lidka", "bo7b$obo6b$bo7b8$8bo$6bobo$5b2obo2$4b3o", 0, (0, 0)),
    p(Seed, "justyna", "17bo4b$16bo2bo2b$17b3o2b$17bo2bob2$2o16bo3b$bo16bo3b$18bo3b8$19b3o$11b3o", 0, (0, 0)),
    p(Seed, "iwona", "14b3o3b6$2bo17b$3b2o15b$3bo14bob$18bob$18bob$19bo$18b2o$7b2o11b$8bo11b5$2o18b$bo", 0, (0, 0)),
    p(Seed, "block-laying-switch-engine", concat!(
        "18bo10b$b3o8bo5bo10b$o3bo6bo7bo9b$b2o9b4o2b2o9b$3b2ob2o9b3o9b$5b2o11bobo8b$19bo7b2o$19bo7b",
        "2o11$7b2o20b$7b2o20b7$15b2o12b$15b2o",
    ), 0, (0, 0)),
    p(Gun, "gosper-glider-gun", concat!(
        "24bo11b$22bobo11b$12b2o6b2o12b2o$11bo3bo4b2o12b2o$2o8bo5bo3b2o14b$2o8bo3bob2o4bobo11b$10bo",
        "5bo7bo11b$11bo3bo20b$12b2o",
    ), 0, (0, 0)),
    p(Gun, "simkin-glider-gun", concat!(
        "2o5b2o$2o5b2o2$4b2o$4b2o5$22b2ob2o$21bo5bo$21bo6bo2b2o$21b3o3bo3b2o$26bo4$20b2o$20bo$21b3o",
        "$23bo",
    ), 0, (0, 0)),
    p(Gun, "bi-gun", concat!(
        "11bo38b$10b2o38b$9b2o39b$10b2o2b2o34b$38bo11b$38b2o8b2o$39b2o7b2o$10b2o2b2o18b2o2b2o10b$2o",
        "7b2o39b$2o8b2o38b$11bo38b$34b2o2b2o10b$39b2o9b$38b2o10b$38bo",
    ), 0, (0, 0)),
    p(Gun, "b-52-bomber", concat!(
        "b2o36b$b2o17bo18b$19bobo12bobo2b$20bo12bo5b$2o7b2o23bo2bob$2obo5b2o23bobobo$3bo23bo7bo2bo$",
        "3bo23b2o7b2ob$o2bo17b2o5bo10b$b2o18bo17b$21b3o15b$36b2ob$36b2ob$b2o36b$o2bo35b$obobo16bobo",
        "4b2o5b2o2b$bo2bo17b2o4b2o5b2obo$5bo12bo3bo15bo$2bobo12bobo18bo$18bo16bo2bo$36b2o",
    ), 0, (0, 0)),
    p(Gun, "twogun", concat!(
        "27bo11b$25bobo11b$15b2o6b2o12b2o$14bo3bo4b2o12b2o$3b2o8bo5bo3b2o14b$3b2o8bo3bob2o4bobo11b$",
        "13bo5bo7bo11b$14bo3bo20b$15b2o22b$26bo12b$27b2o10b$26b2o11b4$21b2o16b$9bobo10b2o15b$9bo2bo",
        "8bo17b$2o10b2o11b2o12b$2o8bo3b2o8bobo12b$5b2o5b2o9bo6b2o7b$4bo4bo2bo10bo2bo2bo2bo6b$9bobo1",
        "1bo6b3o6b$24bobo5b3o4b$25b2o6bobo3b$35bo3b$35b2o",
    ), 0, (0, 0)),
    p(Gun, "new-gun-1", concat!(
        "25b2o5b2o$25b2o5b2o12$27b2ob2o2b$26bo5bob2$25bo7bo$25bo2bobo2bo$25b3o3b3o5$17bo16b$2o15b2o",
        "15b$2o16b2o14b$13b2o2b2o15b4$13b2o2b2o15b$2o16b2o7b2o5b$2o15b2o8b2o5b$17bo",
    ), 0, (0, 0)),
    p(Gun, "new-gun-2", concat!(
        "23b2o24b2o$23b2o24b2o$41b2o8b$40bo2bo7b$41b2o8b2$36b3o12b$36bobo12b$9b2o25b3o12b$9b2o25b2o",
        "13b$8bo2bo23b3o13b$8bo2bob2o20bobo13b$8bo4b2o20b3o13b$10b2ob2o36b$31b2o18b$21b2o7bo2bo17b$",
        "21b2o8b2o18b$49b2o$49b2o2$4b2o18bo26b$2o4b4o10b2o2b2ob3o21b$2o2b2ob3o10b2o4b4o21b$4bo19b2o",
    ), 0, (0, 0)),
    p(Gun, "period-45-glider-gun", concat!(
        "15bo2bo2bo$15b7o2$15b7o$15bo2bo2bo6$16bo2bo$12b2o2bo2bo$12b2o2bo2bo7b2o$27b2o4b2o3bo2bo3b2",
        "o$33b5o4b5o$33b2o3bo2bo3b2o$27b3o2$2ob2o$bobo23b3o$bobo6b3o$2ob2o29bobo$bobo31b2o$bobo6b3o",
        "22bo$2ob2o2$11b2o$11b2o7bo2bo2b2o$20bo2bo2b2o$20bo2bo3$47bo$45bobo$46b2o$18bo2bo2bo$18b7o2",
        "$18b7o$18bo2bo2bo",
    ), 0, (0, 0)),
    p(Gun, "vacuum-gun", concat!(
        "b2o23b2o21b$b2o23bo22b$24bobo22b$15b2o7b2o23b$2o13bobo31b$2o13bob2o30b$16b2o31b$16bo32b$44",
        "b2o3b$16bo27b2o3b$16b2o31b$2o13bob2o13bo3bo12b$2o13bobo13bo5bo7b2o2b$15b2o14bo13b2o2b$31b2",
        "o3bo12b$b2o30b3o13b$b2o46b$33b3o13b$31b2o3bo12b$31bo13b2o2b$31bo5bo7b2o2b$32bo3bo12b2$44b2",
        "o3b$44b2o3b5$37b2o10b$37bobo7b2o$39bo7b2o$37b3o9b$22bobo24b$21b3o25b$21b3o25b$21bo15b3o9b$",
        "25bobo11bo9b$21b2o4bo9bobo9b$16b2o4bo3b2o9b2o10b$15bobo6bo24b$15bo33b$14b2o",
    ), 0, (0, 0)),
    p(Puffer, "puffer-2", "b3o11b3o$o2bo10bo2bo$3bo4b3o6bo$3bo4bo2bo5bo$2bo4bo8bo", 0, (0, 0)),
    p(Puffer, "blinker-puffer-1", "3bo5b$bo3bo3b$o8b$o4bo3b$5o4b4$b2o6b$2ob3o3b$b4o4b$2b2o5b2$5b2o2b$3bo4bo$2bo6b$2bo5bo$2b6o", 0, (0, 0)),
    p(Puffer, "pufferfish", concat!(
        "3bo7bo$2b3o5b3o$b2o2bo3bo2b2o$3b3o3b3o2$4bo5bo$2bo2bo3bo2bo$o5bobo5bo$2o4bobo4b2o$6bobo$3b",
        "obo3bobo$4bo5bo",
    ), 0, (0, 0)),
    p(Puffer, "puffer-1", concat!(
        "b3o6bo5bo6b3ob$o2bo5b3o3b3o5bo2bo$3bo4b2obo3bob2o4bo3b$3bo19bo3b$3bo2bo13bo2bo3b$3bo2b2o11",
        "b2o2bo3b$2bo3b2o11b2o3bo",
    ), 0, (0, 0)),
    p(Puffer, "noahs-ark", "10bobo2b$9bo5b$10bo2bob$12b3o6$bo13b$obo12b2$o2bo11b$2b2o11b$3bo", 0, (0, 0)),
    p(Puffer, "space-rake", concat!(
        "11b2o5b4o$9b2ob2o3bo3bo$9b4o8bo$10b2o5bo2bob2$8bo13b$7b2o8b2o3b$6bo9bo2bo2b$7b5o4bo2bo2b$8",
        "b4o3b2ob2o2b$11bo4b2o4b4$18b4o$o2bo13bo3bo$4bo16bo$o3bo12bo2bob$b4o",
    ), 0, (0, 0)),
    p(Puffer, "blinker-puffer-2", concat!(
        "13b3ob$12b5o$11b2ob3o$12b2o3b3$9bobo5b$2bo5bo2bo5b$b5o3bobo5b$2o3b2ob2o7b$bo7bo7b$2b2o2bo2",
        "bo7b$10bo6b$2b2o2bo2bo7b$bo7bo7b$2o3b2ob2o7b$b5o3bobo5b$2bo5bo2bo5b$9bobo5b3$12b2o3b$11b2o",
        "b3o$12b5o$13b3o",
    ), 0, (0, 0)),
    p(Puffer, "backrake-1", concat!(
        "5b3o11b3o5b$4bo3bo9bo3bo4b$3b2o4bo7bo4b2o3b$2bobob2ob2o5b2ob2obobo2b$b2obo4bob2ob2obo4bob2",
        "ob$o4bo3bo2bobo2bo3bo4bo$12bobo12b$2o7b2obobob2o7b2o$12bobo12b$6b3o9b3o6b$6bo3bo9bo6b$6bob",
        "o4b3o11b$12bo2bo4b2o5b$15bo11b$11bo3bo11b$11bo3bo11b$15bo11b$12bobo",
    ), 0, (0, 0)),
    p(Puffer, "backrake-2", concat!(
        "3bo15b$2b3o14b$b2obo5bo8b$b3o5b3o7b$2b2o4bo2b2o3b3o$8b3o4bo2bo$18bo$18bo$18bo$2b3o12bob$2b",
        "o2bo13b$2bo16b$2bo16b$3bo15b7$3o16b$o2bo11bo3b$o13b3o2b$o12b2obo2b$o12b3o3b$bo12b2o",
    ), 0, (0, 0)),
    p(Puffer, "frothing-puffer", concat!(
        "7bo17bo$6b3o15b3o$5b2o4b3o5b3o4b2o$3b2obo2b3o2bo3bo2b3o2bob2o$4bobo2bobo3bobo3bobo2bobo$b2",
        "obobobobo4bobo4bobobobob2o$b2o3bobo4bo5bo4bobo3b2o$b3obo3bo4bobobo4bo3bob3o$2o9b2obobobob2",
        "o9b2o$12bo7bo$9b2obo7bob2o$10bo11bo$7b2obo11bob2o$7b2o15b2o$7bobobob3ob3obobobo$6b2o3bo3bo",
        "bo3bo3b2o$6bo2bo3bobobobo3bo2bo$9b2o4bobo4b2o$5b2o4bo3bobo3bo4b2o$9bob2obo3bob2obo$10bobob",
        "obobobobo$12bo2bobo2bo$11bobo5bobo",
    ), 0, (0, 0)),
    p(Filler, "max", concat!(
        "18bo8b$17b3o7b$12b3o4b2o6b$11bo2b3o2bob2o4b$10bo3bobo2bobo5b$10bo4bobobobob2o2b$12bo4bobo3",
        "b2o2b$4o5bobo4bo3bob3o2b$o3b2obob3ob2o9b2ob$o5b2o5bo13b$bo2b2obo2bo2bob2o10b$7bobobobobobo",
        "5b4o$bo2b2obo2bo2bo2b2obob2o3bo$o5b2o3bobobo3b2o5bo$o3b2obob2o2bo2bo2bob2o2bob$4o5bobobobo",
        "bobo7b$10b2obo2bo2bob2o2bob$13bo5b2o5bo$b2o9b2ob3obob2o3bo$2b3obo3bo4bobo5b4o$2b2o3bobo4bo",
        "12b$2b2obobobobo4bo10b$5bobo2bobo3bo10b$4b2obo2b3o2bo11b$6b2o4b3o12b$7b3o17b$8bo",
    ), 0, (0, 0)),
    p(Filler, "spacefiller-1", concat!(
        "5bobo21b$4bo2bo21b$3b2o24b$2bo26b$b4o24b$o4bo23b$o2bo25b$o2bo25b$bo9b3o3b3o9b$2b4obo2bo2bo",
        "3bo2bo8b$3bo3bo5bo3bo11b$4bo8bo3bo11b$4bobo6bo3bo11b2$3b3o5b3o3b3o9b$3b2o7bo5bo10b$3b3o6b7",
        "o10b$11bo7bo9b$4bobo3b11o8b$3bo2bo2bo12b2o5b$3bo5b12o3bo4b$3bo3bo13bo3bo3b$4bo3b12o5bo3b$5",
        "b2o12bo2bo2bo3b$8b11o3bobo4b$9bo7bo11b$10b7o6b3o3b$10bo5bo7b2o3b$9b3o3b3o5b3o3b2$11bo3bo6b",
        "obo4b$11bo3bo8bo4b$11bo3bo5bo3bo3b$8bo2bo3bo2bo2bob4o2b$9b3o3b3o9bob$25bo2bo$25bo2bo$23bo4",
        "bo$24b4ob$26bo2b$24b2o3b$21bo2bo4b$21bobo",
    ), 0, (0, 0)),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(art: &Art) -> Vec<String> {
        let line = |r: &Vec<bool>| r.iter().map(|&c| if c { 'O' } else { '.' }).collect();
        art.iter().map(line).collect()
    }

    #[test]
    fn decode_pads_and_skips_rows() {
        assert_eq!(rows(&decode("bo$2bo$3o!")), [".O.", "..O", "OOO"]);
        assert_eq!(rows(&decode("o2$2o")), ["O.", "..", "OO"]);
        assert_eq!(rows(&named("glider").unwrap().art()), [".O.", "..O", "OOO"]);
    }

    #[test]
    fn the_table() {
        let mut names: Vec<&str> = LIBRARY.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 240);
        for p in &LIBRARY {
            let art = p.art();
            assert_eq!(p.name, p.name.to_lowercase());
            let w = art[0].len();
            assert!(
                art.iter().all(|r| r.len() == w) && w.max(art.len()) <= 64,
                "{}",
                p.name
            );
            assert!(
                art[0].contains(&true) && art[art.len() - 1].contains(&true),
                "{}",
                p.name
            );
            assert!(
                art.iter().any(|r| r[0]) && art.iter().any(|r| r[w - 1]),
                "{}",
                p.name
            );
        }
        let fillers: Vec<&str> = LIBRARY
            .iter()
            .filter(|p| p.kind == Filler)
            .map(|p| p.name)
            .collect();
        assert_eq!(fillers, ["max", "spacefiller-1"]);
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.name()), Some(k));
        }
    }

    #[test]
    fn turn_and_heading() {
        let art = named("lwss").unwrap().art();
        let mut seen: Vec<Art> = (0..8).map(|k| turn(&art, k)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 8);
        assert_eq!(rows(&turn(&decode("2o$o"), 1)), ["OO", "O."]);
        assert_eq!(rows(&turn(&decode("2o$bo"), 2)), ["OO", "O."]);
        assert_eq!(rows(&turn(&decode("2o$bo"), 4)), [".O", "OO"]);
        assert_eq!(heading((1, -2), 0), (1, -2));
        assert_eq!(heading((1, -2), 1), (-2, 1));
        assert_eq!(heading((1, -2), 7), (2, -1));
    }
}
