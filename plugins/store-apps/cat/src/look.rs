// SPDX-License-Identifier: GPL-3.0-or-later
//! The looks: the poses of a cat, its coat and skin; a plate, a mouse,
//! a fish, a ball of yarn. The twin's `look`, `dress`, `*_look`, and of
//! `_reef`: `mirror`, `pad`, `paint`.

use ptyrc_app::canvas::Braille;
use ptyrc_app::{Value, json};

/// The box of every pose.
pub const W: i64 = 11;
pub const H: i64 = 3;
/// The box of a plate.
pub const PW: i64 = 5;
pub const PH: i64 = 1;
/// Cells of a mouse, of a fish, of a ball: one row.
pub const MW: i64 = 9;
pub const FW: i64 = 6;
pub const BW: i64 = 2;

/// The colours of a cat, of the 256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coat {
    pub fur: u8,
    /// Tail, chin.
    pub shade: u8,
    pub eyes: u8,
    pub nose: u8,
}

const fn coat(fur: u8, shade: u8, eyes: u8, nose: u8) -> Coat {
    Coat {
        fur,
        shade,
        eyes,
        nose,
    }
}

/// Ginger, grey, white, tabby, siamese, black, blue, calico.
pub const COATS: [Coat; 8] = [
    coat(215, 208, 114, 211),
    coat(250, 244, 117, 217),
    coat(255, 252, 81, 218),
    coat(180, 137, 150, 211),
    coat(223, 95, 39, 181),
    coat(243, 239, 220, 211),
    coat(110, 67, 228, 217),
    coat(255, 208, 114, 211),
];

/// The features of a cat: what `POSES` is drawn with. `eyes`: at
/// ease, wide open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skin {
    pub ears: &'static str,
    pub eyes: &'static str,
    pub nose: &'static str,
}

pub const PLAIN: Skin = Skin {
    ears: "/\\_/\\",
    eyes: "oO",
    nose: ".",
};
pub const EARS: [&str; 4] = ["/\\_/\\", "|\\_/|", "/^_^\\", "/\\-/\\"];
pub const EYES: [&str; 4] = ["oO", "•●", "uO", "*O"];
pub const NOSES: [&str; 5] = [".", "w", "_", "x", "v"];

type Frame = ([&'static str; 3], u32);

/// Facing right. Each frame: three rows of `W` cells, then ms; 0: a
/// frame a column (the walk, the run), the app the clock.
pub const POSES: [(&str, &[Frame]); 17] = [
    (
        "sit",
        &[
            (["     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "], 2600),
            (["     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "], 140),
            (["     /\\_/\\ ", "    ( o.o )", "    _> ^ < "], 900),
            (["     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "], 500),
            (["     /\\_/\\ ", "    ( o.o )", "    _> ^ < "], 1400),
        ],
    ),
    (
        "alert",
        &[(["     /\\_/\\ ", "    ( O.O )", "    _> ^ < "], 400)],
    ),
    (
        "groom",
        &[
            (["     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "], 260),
            (["     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "], 260),
            (["     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "], 260),
            (["     /\\_/\\ ", "    ( o.o )", "    _> ^ < "], 700),
        ],
    ),
    (
        "yawn",
        &[
            (["     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "], 300),
            (["     /\\_/\\ ", "    ( -o- )", "    ~> ^ < "], 300),
            (["     /\\_/\\ ", "    ( -O- )", "    _> ^ < "], 500),
            (["     /\\_/\\ ", "    ( -o- )", "    _> ^ < "], 300),
        ],
    ),
    (
        "sleep",
        &[
            (["           ", "  __/\\_/\\  ", " (__( -.- )"], 1100),
            (["    z      ", "  __/\\_/\\  ", " (__( -.- )"], 900),
            (["   Z       ", " ___/\\_/\\  ", "(___( -.- )"], 900),
            (["  z        ", " ___/\\_/\\  ", "(___( -.- )"], 700),
        ],
    ),
    (
        "walk",
        &[
            (["      /\\_/\\", " ~.___(o.o)", "   /|   |\\ "], 0),
            (["      /\\_/\\", " ~.___(o.o)", "   ||   || "], 0),
            (["      /\\_/\\", " -.___(o.o)", "   |\\   /| "], 0),
            (["      /\\_/\\", " -.___(o.o)", "   ||   || "], 0),
        ],
    ),
    (
        "leap",
        &[(["      /\\_/\\", " ~-=--(O.O)", "  ''     ``"], 0)],
    ),
    // The head down, over what is right of the box.
    (
        "eat",
        &[
            (["           ", " ~._/\\_/\\  ", " (__( o.o )"], 350),
            (["           ", " ~._/\\_/\\  ", " (__( -.- )"], 200),
            (["           ", " -._/\\_/\\  ", " (__( >.< )"], 200),
            (["           ", " ~._/\\_/\\  ", " (__( -.- )"], 200),
        ],
    ),
    // The pointer, a fish, a ball: it looks, its tail beats.
    (
        "stare",
        &[
            (["     /\\_/\\ ", "    ( O.O )", "    ~> ^ < "], 180),
            (["     /\\_/\\ ", "    ( O.O )", "    _> ^ < "], 180),
        ],
    ),
    (
        "wiggle",
        &[
            (["           ", "  ~_/\\_/\\  ", " (__( O.O )"], 130),
            (["           ", " ~_ /\\_/\\  ", "(___( O.O )"], 130),
        ],
    ),
    // A paw at what is right of the box.
    (
        "bat",
        &[
            (["    /\\_/\\  ", "   ( O.O )/", "   ~> ^ <  "], 140),
            (["    /\\_/\\  ", "   ( O.O ) ", "   _> ^ <-,"], 180),
        ],
    ),
    // Up on its hind legs, under what is out of reach.
    (
        "beg",
        &[
            (["    \\/\\_/\\/", "    ( O.O )", "     | ^ | "], 250),
            (["     /\\_/\\ ", "    ( O.O )", "    /| ^ |\\"], 250),
        ],
    ),
    (
        "puzzled",
        &[(["   ? /\\_/\\ ", "    ( o.O )", "    ~> ^ < "], 400)],
    ),
    (
        "purr",
        &[
            (["  ♥  /\\_/\\ ", "    ( ^.^ )", "    ~> ^ < "], 500),
            ([" ♥   /\\_/\\ ", "    ( -.- )", "    _> ^ < "], 500),
        ],
    ),
    // A gallop, a frame a column.
    (
        "run",
        &[
            (["      /\\_/\\", " ~-=__(O.O)", "  //    \\\\ "], 0),
            (["      /\\_/\\", " ~-=__(O.O)", "   //   || "], 0),
            (["      /\\_/\\", " _-=-~(O.O)", "    || //  "], 0),
            (["      /\\_/\\", " _.-~~(O.O)", "    \\\\//   "], 0),
            (["      /\\_/\\", " _-=-~(O.O)", "    ||  \\\\ "], 0),
            (["      /\\_/\\", " ~-=__(O.O)", "   //   \\\\ "], 0),
        ],
    ),
    (
        "hiss",
        &[
            (["   _  /\\_/\\", " |/ \\_(>o<)", "  ||   ||  "], 160),
            (["   _  /\\_/\\", " \\/ \\_(>O<)", "  ||   ||  "], 160),
        ],
    ),
    // On its back, the thread round its paws.
    (
        "roll",
        &[
            (["           ", "    '' ''  ", " ~(___(^.^)"], 220),
            (["           ", "    `` ``  ", " _(___(^.^)"], 220),
        ],
    ),
];

/// The frames of `pose`; an unknown one panics.
pub fn frames(pose: &str) -> &'static [Frame] {
    POSES
        .iter()
        .find(|(n, _)| *n == pose)
        .map(|(_, f)| *f)
        .expect("a pose")
}

/// A plate, by portions left.
pub const PLATE: [&str; 5] = ["\\.../", "\\.o./", "\\.o./", "\\oOo/", "\\oOo/"];
const PLATE_INK: Ink = &[("", 75), ("oO", 173), (".", 130)];
/// Facing right: at rest, then the two steps of its walk.
pub const MOUSE: [&str; 3] = ["~-(,_,\"°>", "~~(,_,\"°>", "-~(_,_\"°>"];
const MOUSE_INK: Ink = &[("", 250), ("~-", 217), (">", 211), ("°", 231), (",", 245)];
/// A fish, its head to the left, by portions left; then as it flops.
pub const FISH: [&str; 4] = [" .,'. ", "<+++><", "<//+><", "<///><"];
const FLOP: &str = "<\\\\\\><";
const FISH_INK: Ink = &[("", 117), ("<>", 75), ("+", 252), (".,'", 244)];
/// The colours of a ball.
pub const YARNS: [u8; 6] = [203, 213, 81, 220, 120, 141];

/// A colour by glyph: the first key that holds it; `""` the rest.
pub type Ink<'a> = &'a [(&'a str, u8)];

/// A run of a row: a space is a hole (no colour, no style).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seg {
    pub t: String,
    pub fg: Option<u8>,
    pub bold: bool,
    pub dim: bool,
}

pub type Cells = Vec<Vec<Seg>>;

/// Content of a sprite: one picture, or frames of `ms` each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Look {
    Still(Cells),
    Frames {
        frames: Vec<(Cells, u32)>,
        looped: bool,
    },
}

fn cells_value(cells: &Cells) -> Value {
    let rows: Vec<Value> = cells
        .iter()
        .map(|row| {
            let runs: Vec<Value> = row
                .iter()
                .map(|s| {
                    let mut v = json!({ "t": (s.t.as_str()) });
                    if let Some(fg) = s.fg {
                        v["fg"] = Value::from(fg);
                    }
                    if s.bold {
                        v["bold"] = Value::Bool(true);
                    }
                    if s.dim {
                        v["dim"] = Value::Bool(true);
                    }
                    v
                })
                .collect();
            Value::Array(runs)
        })
        .collect();
    Value::Array(rows)
}

impl Look {
    /// The members of a sprite: `{"cells"}`, or `{"frames", "loop"}`.
    pub fn to_value(&self) -> Value {
        match self {
            Look::Still(c) => json!({ "cells": (cells_value(c)) }),
            Look::Frames { frames, looped } => {
                let f: Vec<Value> = frames
                    .iter()
                    .map(|(c, ms)| json!({ "cells": (cells_value(c)), "ms": (*ms) }))
                    .collect();
                json!({ "frames": (Value::Array(f)), "loop": (*looped) })
            }
        }
    }
}

/// Python's `str.isspace` for one char.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn swap(c: char) -> char {
    const PAIRS: [(char, char); 6] = [
        ('/', '\\'),
        ('(', ')'),
        ('<', '>'),
        ('[', ']'),
        ('{', '}'),
        ('`', '\''),
    ];
    for &(a, b) in &PAIRS {
        if c == a {
            return b;
        }
        if c == b {
            return a;
        }
    }
    c
}

/// `rows` seen from the other side: what faced right faces left.
pub fn mirror<S: AsRef<str>>(rows: &[S]) -> Vec<String> {
    rows.iter()
        .map(|r| r.as_ref().chars().rev().map(swap).collect())
        .collect()
}

/// Rows of exactly `width` chars.
pub fn pad<S: AsRef<str>>(rows: &[S], width: usize) -> Vec<String> {
    rows.iter()
        .map(|r| {
            let mut v: Vec<char> = r.as_ref().chars().take(width).collect();
            v.resize(width, ' ');
            v.into_iter().collect()
        })
        .collect()
}

/// Rows of text to rows of runs; a space is a hole.
pub fn paint<S: AsRef<str>>(rows: &[S], ink: Ink, bold: bool, dim: bool) -> Cells {
    let fg = |c: char| {
        ink.iter()
            .find(|(k, _)| !k.is_empty() && k.contains(c))
            .or_else(|| ink.iter().find(|(k, _)| k.is_empty()))
            .map(|&(_, v)| v)
    };
    rows.iter()
        .map(|row| {
            let mut runs: Vec<Seg> = Vec::new();
            for c in row.as_ref().chars() {
                let seg = if c == ' ' {
                    Seg {
                        t: " ".into(),
                        fg: None,
                        bold: false,
                        dim: false,
                    }
                } else {
                    Seg {
                        t: c.to_string(),
                        fg: fg(c),
                        bold,
                        dim,
                    }
                };
                match runs.last_mut() {
                    Some(l) if (l.fg, l.bold, l.dim) == (seg.fg, seg.bold, seg.dim) => l.t.push(c),
                    _ => runs.push(seg),
                }
            }
            runs
        })
        .collect()
}

/// `rows` of `POSES` with the ears, the eyes and the nose of `skin`.
pub fn dress(rows: &[&str], skin: &Skin) -> Vec<String> {
    let eyes: Vec<char> = skin.eyes.chars().collect();
    let eye = |c: char| match c {
        'o' => eyes[0],
        'O' => eyes[1],
        c => c,
    };
    rows.iter()
        .map(|row| {
            let row: Vec<char> = row.replace(EARS[0], skin.ears).chars().collect();
            face(&row, &eye, skin.nose)
        })
        .collect()
}

/// The twin's `\((\s?)(\S)(\S)(\S)(\s?)\)`, each match rewritten.
fn face(row: &[char], eye: &dyn Fn(char) -> char, nose: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < row.len() {
        if let Some((a, m, b, end)) = face_at(row, i) {
            out.push('(');
            if a {
                out.push(row[i + 1]);
            }
            out.push(eye(m[0]));
            if m[1] == '.' {
                out.push_str(nose);
            } else {
                out.push(m[1]);
            }
            out.push(eye(m[2]));
            if b {
                out.push(row[end - 1]);
            }
            out.push(')');
            i = end + 1;
        } else {
            out.push(row[i]);
            i += 1;
        }
    }
    out
}

/// A match at `i`: the spaces taken (before, after), the three glyphs,
/// the index of the `)`. The regex backtracks: each optional space
/// tried taken first.
fn face_at(row: &[char], i: usize) -> Option<(bool, [char; 3], bool, usize)> {
    if row.get(i) != Some(&'(') {
        return None;
    }
    let ns = |j: usize| row.get(j).filter(|c| !is_space(**c)).copied();
    for a in [true, false] {
        if a && !row.get(i + 1).is_some_and(|c| is_space(*c)) {
            continue;
        }
        let j = i + 1 + usize::from(a);
        let (Some(x), Some(y), Some(z)) = (ns(j), ns(j + 1), ns(j + 2)) else {
            continue;
        };
        for b in [true, false] {
            let k = j + 3;
            if b && !row.get(k).is_some_and(|c| is_space(*c)) {
                continue;
            }
            let end = k + usize::from(b);
            if row.get(end) == Some(&')') {
                return Some((a, [x, y, z], b, end));
            }
        }
    }
    None
}

/// A pose as content: a looped animation, or its frame `step` alone.
pub fn look(pose: &str, right: bool, step: Option<i64>, coat: &Coat, skin: &Skin) -> Look {
    let keys = format!("oO{}", skin.eyes);
    let ink: Vec<(&str, u8)> = vec![
        ("", coat.fur),
        (&keys, coat.eyes),
        ("^wxv", coat.nose),
        ("zZ", 244),
        ("?", 229),
        ("♥", 204),
        ("~_.", coat.shade),
    ];
    let cells = |rows: &[&str; 3]| {
        let rows = dress(rows, skin);
        if right {
            paint(&rows, &ink, true, false)
        } else {
            paint(&mirror(&rows), &ink, true, false)
        }
    };
    let frames = frames(pose);
    if step.is_some() || frames.len() == 1 {
        let i = step.unwrap_or(0).rem_euclid(frames.len() as i64) as usize;
        return Look::Still(cells(&frames[i].0));
    }
    Look::Frames {
        frames: frames.iter().map(|(r, ms)| (cells(r), *ms)).collect(),
        looped: true,
    }
}

/// A plate with `left` portions.
pub fn plate_look(left: usize) -> Look {
    Look::Still(paint(&[PLATE[left]], PLATE_INK, true, false))
}

fn one(row: &str, right: bool, ink: Ink, bold: bool) -> Cells {
    if right {
        paint(&[row], ink, bold, false)
    } else {
        paint(&mirror(&[row]), ink, bold, false)
    }
}

/// A mouse: the frame `step` of its walk, or at rest, its tail moving.
pub fn mouse_look(right: bool, step: Option<i64>) -> Look {
    if let Some(s) = step {
        return Look::Still(one(
            MOUSE[1 + s.rem_euclid(2) as usize],
            right,
            MOUSE_INK,
            false,
        ));
    }
    let frames = MOUSE[..2]
        .iter()
        .map(|r| (one(r, right, MOUSE_INK, false), 400))
        .collect();
    Look::Frames {
        frames,
        looped: true,
    }
}

/// A fish with `left` portions, its head to the side `right`; `flop`:
/// alive, on the floor.
pub fn fish_look(left: usize, right: bool, flop: bool) -> Look {
    if !flop {
        return Look::Still(one(FISH[left], right, FISH_INK, true));
    }
    let beat = [(FISH[3], 700), (FLOP, 180), (FISH[3], 250), (FLOP, 180)];
    let frames = beat
        .iter()
        .map(|&(r, ms)| (one(r, right, FISH_INK, true), ms))
        .collect();
    Look::Frames {
        frames,
        looped: true,
    }
}

/// The dots of a ball; those within turn with it; played out, a ring.
const BALL: [(i64, i64); 8] = [
    (1, 0),
    (2, 0),
    (0, 1),
    (3, 1),
    (0, 2),
    (3, 2),
    (1, 3),
    (2, 3),
];
const CORE: [(i64, i64); 4] = [(1, 1), (2, 1), (2, 2), (1, 2)];

/// A ball of yarn: 4 dots by 4 less its corners, `dx`, `dy` dots into
/// its first cell; one dot within off, by `turn`. No play left: a ring.
pub fn ball_look(color: u8, plays: i64, dx: i64, dy: i64, turn: i64) -> Look {
    let mut dots = Braille::new(
        (BW + i64::from(dx > 0)) as u32,
        (1 + i64::from(dy > 0)) as u32,
    );
    for &(x, y) in &BALL {
        dots.set(dx + x, dy + y, true);
    }
    if plays != 0 {
        let off = turn.rem_euclid(4) as usize;
        for (i, &(x, y)) in CORE.iter().enumerate() {
            if i != off {
                dots.set(dx + x, dy + y, true);
            }
        }
    }
    Look::Still(paint(&dots.rows(), &[("", color)], true, false))
}

/// A thread of `rows` (Braille), plain or dim.
pub fn thread_cells(rows: &[String], color: u8, dim: bool) -> Cells {
    paint(rows, &[("", color)], false, dim)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn cells_text(cells: &Cells) -> String {
        let rows: Vec<String> = cells
            .iter()
            .map(|row| {
                let runs: Vec<String> = row
                    .iter()
                    .map(|s| {
                        let mut t = s.t.clone();
                        if let Some(fg) = s.fg {
                            t += &format!(":{fg}");
                        }
                        if s.bold {
                            t += "!b";
                        }
                        if s.dim {
                            t += "!d";
                        }
                        t
                    })
                    .collect();
                runs.join("|")
            })
            .collect();
        rows.join("/")
    }

    pub fn text(look: &Look) -> String {
        match look {
            Look::Still(c) => format!("cells {}", cells_text(c)),
            Look::Frames { frames, looped } => {
                let f: Vec<String> = frames
                    .iter()
                    .map(|(c, ms)| format!("{ms}:{}", cells_text(c)))
                    .collect();
                format!("frames loop={} {}", u8::from(*looped), f.join(";"))
            }
        }
    }

    pub fn fnv(s: &str) -> String {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in s.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        format!("{h:016x}")
    }

    const SKINS: [Skin; 4] = [
        PLAIN,
        Skin {
            ears: "|\\_/|",
            eyes: "•●",
            nose: "w",
        },
        Skin {
            ears: "/^_^\\",
            eyes: "*O",
            nose: "x",
        },
        Skin {
            ears: "/\\-/\\",
            eyes: "uO",
            nose: "v",
        },
    ];

    /// The looks of `twin.py`, by name.
    fn looks() -> Vec<(String, Look)> {
        let mut out = Vec::new();
        for (p, f) in POSES {
            let n = f.len() as i64;
            for right in [true, false] {
                for step in [None, Some(0), Some(1), Some(n + 2)] {
                    for c in [0, 4] {
                        let s = step.map_or("None".into(), |s: i64| s.to_string());
                        out.push((
                            format!("look.{p}.{}.{s}.{c}.0", u8::from(right)),
                            look(p, right, step, &COATS[c], &PLAIN),
                        ));
                    }
                }
            }
            for (k, skin) in SKINS.iter().enumerate().skip(1) {
                out.push((
                    format!("look.{p}.1.None.{k}.{k}"),
                    look(p, true, None, &COATS[k], skin),
                ));
                out.push((
                    format!("look.{p}.0.1.{}.{k}", k + 4),
                    look(p, false, Some(1), &COATS[k + 4], skin),
                ));
            }
        }
        for left in 0..5 {
            out.push((format!("plate.{left}"), plate_look(left)));
        }
        for right in [true, false] {
            let r = u8::from(right);
            for step in [None, Some(0), Some(1), Some(7)] {
                let s = step.map_or("None".into(), |s: i64| s.to_string());
                out.push((format!("mouse.{r}.{s}"), mouse_look(right, step)));
            }
            for left in 0..4 {
                for flop in [false, true] {
                    out.push((
                        format!("fish.{left}.{r}.{}", u8::from(flop)),
                        fish_look(left, right, flop),
                    ));
                }
            }
        }
        for plays in [0, 2] {
            for dx in 0..2 {
                for dy in 0..4 {
                    for turn in 0..5 {
                        out.push((
                            format!("ball.{plays}.{dx}.{dy}.{turn}"),
                            ball_look(203, plays, dx, dy, turn),
                        ));
                    }
                }
            }
        }
        out
    }

    pub fn twin(prefix: &str) -> Vec<(&'static str, &'static str)> {
        include_str!("twin.txt")
            .lines()
            .filter(|l| l.starts_with(prefix))
            .map(|l| l.split_once(' ').expect("NAME VALUE"))
            .collect()
    }

    #[test]
    fn looks_are_the_twins() {
        let want: std::collections::HashMap<_, _> = ["look.", "plate.", "mouse.", "fish.", "ball."]
            .iter()
            .flat_map(|p| twin(p))
            .collect();
        let got = looks();
        assert_eq!(got.len(), want.len());
        for (name, look) in &got {
            assert_eq!(
                fnv(&text(look)),
                want[name.as_str()],
                "{name}: {}",
                text(look)
            );
        }
    }

    #[test]
    fn art() {
        for (pose, frames) in POSES {
            for (rows, _) in frames {
                assert!(
                    rows.iter().all(|r| r.chars().count() == W as usize),
                    "{pose}"
                );
            }
        }
        let Look::Still(left) = look("walk", false, Some(0), &COATS[0], &PLAIN) else {
            panic!()
        };
        let row: String = left[1].iter().map(|s| s.t.as_str()).collect();
        assert_eq!(row, "(o.o)___.~ ");
        for ears in EARS {
            for eyes in EYES {
                for nose in NOSES {
                    let skin = Skin { ears, eyes, nose };
                    for (pose, frames) in POSES {
                        for (rows, _) in frames {
                            let d = dress(rows, &skin);
                            assert!(d.iter().all(|r| r.chars().count() == 11), "{pose}");
                        }
                    }
                }
            }
        }
        let odd = Skin {
            ears: "|\\_/|",
            eyes: "*O",
            nose: "w",
        };
        assert_eq!(
            dress(&frames("sit")[0].0, &odd)[..2],
            ["     |\\_/| ", "    ( *w* )"]
        );
        assert_eq!(dress(&frames("yawn")[2].0, &odd)[1], "    ( -O- )");
        let Look::Still(grey) = look("alert", true, None, &COATS[1], &PLAIN) else {
            panic!()
        };
        let mut fg: Vec<Option<u8>> = grey[1].iter().map(|s| s.fg).collect();
        fg.sort();
        fg.dedup();
        assert_eq!(fg, [None, Some(117), Some(244), Some(250)]);
        let Look::Still(m) = mouse_look(false, Some(0)) else {
            panic!()
        };
        assert_eq!(
            m[0].iter().map(|s| s.t.as_str()).collect::<String>(),
            "<°\",_,)~~"
        );
    }
}
