// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a cat may stand, and how it gets there: the twin's `ledges`,
//! `mass`, `perch`, `near`, `arc`, `wave`. Frame cells.

use crate::look::{BW, H, W, is_space};

/// Cells of text, at least, under the cat.
pub const GROUND: i64 = 5;
/// Columns, rows: the cursor that close is a fright.
pub const NEAR: (i64, i64) = (7, 2);

/// Where the cat may stand: its left column from `x0` to `x1`, its
/// paws on the row above `g`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ledge {
    pub g: i64,
    pub x0: i64,
    pub x1: i64,
}

impl Ledge {
    pub fn has(&self, x: i64) -> bool {
        self.x0 <= x && x <= self.x1
    }
}

/// The ledges of a view: `rows` its text, `view` its frame cells
/// `(x0, y0, x1, y1)`, ends excluded. Under the cat, text on `GROUND`
/// cells at least; around it, its box free and a free column on each
/// side. The floor of the view counts.
pub fn ledges<S: AsRef<str>>(rows: &[S], view: (i64, i64, i64, i64)) -> Vec<Ledge> {
    let (x0, y0, x1, y1) = view;
    let (width, height) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut ink: Vec<Vec<bool>> = (0..height)
        .map(|y| {
            let mut r: Vec<bool> = rows
                .get(y)
                .map(|r| {
                    r.as_ref()
                        .chars()
                        .take(width)
                        .map(|c| !is_space(c))
                        .collect()
                })
                .unwrap_or_default();
            r.resize(width, false);
            r
        })
        .collect();
    ink.push(vec![true; width]);
    let (w, h) = (W as usize, H as usize);
    let mut out = Vec::new();
    for g in h..=height {
        let free: Vec<bool> = (0..width).map(|x| !(g - h..g).any(|y| ink[y][x])).collect();
        let mut run: Vec<usize> = Vec::new();
        for x in 0..(width + 2).saturating_sub(w) {
            let ok = x + w <= width
                && free[x.saturating_sub(1)..(x + w + 1).min(width)]
                    .iter()
                    .all(|&f| f)
                && ink[g][x..x + w].iter().filter(|&&i| i).count() >= GROUND as usize;
            if ok {
                run.push(x);
            } else if let (Some(&a), Some(&b)) = (run.first(), run.last()) {
                out.push(Ledge {
                    g: y0 + g as i64,
                    x0: x0 + a as i64,
                    x1: x0 + b as i64,
                });
                run.clear();
            }
        }
    }
    out
}

/// The cells of a row that stop a ball: its glyphs, and the blanks
/// between two of them, `BW + 1` at most.
pub fn mass(row: &[char]) -> Vec<bool> {
    let mut out: Vec<bool> = row.iter().map(|c| !is_space(*c)).collect();
    let mut last: Option<usize> = None;
    for x in 0..out.len() {
        if out[x] {
            if let Some(l) = last
                && (x - l - 1) as i64 <= BW + 1
            {
                out[l + 1..x].iter_mut().for_each(|b| *b = true);
            }
            last = Some(x);
        }
    }
    out
}

/// The ledge the cat at `x` above row `g` stands on.
pub fn perch(spots: &[Ledge], x: i64, g: i64) -> Option<Ledge> {
    spots.iter().find(|s| s.g == g && s.has(x)).copied()
}

/// The cursor is in the cat's box, or `NEAR` it.
pub fn near(x: i64, g: i64, cursor: Option<(i64, i64)>) -> bool {
    let Some((cx, cy)) = cursor else { return false };
    let (dx, dy) = NEAR;
    x - dx <= cx && cx < x + W + dx && g - H - dy <= cy && cy < g + dy
}

/// `math.hypot` of two integers: exact under 2^53.
pub fn hypot(dx: i64, dy: i64) -> f64 {
    ((dx * dx + dy * dy) as f64).sqrt()
}

/// Python's `round`: half to even.
pub fn round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// The cells of a jump from `a` to `b`, one per hop: a parabola,
/// higher and longer for a long jump; `b` is the last.
pub fn arc(a: (i64, i64), b: (i64, i64)) -> Vec<(i64, i64)> {
    let far = hypot(b.0 - a.0, 2 * (b.1 - a.1));
    let n = round(6.0 + far / 4.0).clamp(6, 24);
    let rise = (1.5 + far / 20.0).min(4.0);
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let lift = 4.0 * rise * t * (1.0 - t);
            let x = a.0 as f64 + (b.0 - a.0) as f64 * t;
            let y = a.1 as f64 + (b.1 - a.1) as f64 * t - lift;
            (round(x), round(y))
        })
        .collect()
}

/// How high over the floor the thread lies at the dot column `x`: 0 to
/// 2 dots, a wave.
pub fn wave(x: i64) -> i64 {
    round(1.0 + (0.9 * x as f64).sin())
}

/// The order CPython iterates a set of a few small non-negative ints
/// (a table of 8): the twin's `{a, b, c}`.
pub fn set_order(items: &[i64]) -> Vec<i64> {
    let mut table: [Option<i64>; 8] = [None; 8];
    for &x in items {
        debug_assert!(x >= 0 && items.len() <= 4);
        let mut i = (x & 7) as usize;
        let mut perturb = x as u64;
        loop {
            match table[i] {
                None => {
                    table[i] = Some(x);
                    break;
                }
                Some(y) if y == x => break,
                Some(_) => {
                    perturb >>= 5;
                    i = (i * 5 + 1 + perturb as usize) & 7;
                }
            }
        }
    }
    table.iter().flatten().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::tests::twin;

    fn nums(s: &str) -> Vec<i64> {
        s.split(['.', ':', ','])
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().unwrap())
            .collect()
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn the_twins() {
        let screens: [(&[&str], (i64, i64, i64, i64)); 4] = [
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
            ),
            (
                &[
                    "",
                    "",
                    "",
                    "#### ##### ###",
                    "",
                    "",
                    "",
                    "############",
                    "",
                    "   ###########",
                ],
                (3, 2, 40, 12),
            ),
            (&[], (0, 0, 30, 6)),
            (
                &[
                    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                    "",
                    "",
                    "",
                    "   yyyyyyyyyyyyyyyyyyyy",
                ],
                (0, 0, 30, 8),
            ),
        ];
        for ((rows, view), (_, want)) in screens.iter().zip(twin("ledges.")) {
            let got: Vec<String> = ledges(rows, *view)
                .iter()
                .map(|s| format!("{}:{}:{}", s.g, s.x0, s.x1))
                .collect();
            let got = if got.is_empty() {
                "-".into()
            } else {
                got.join(",")
            };
            assert_eq!(got, want);
        }
        let rows = ["", "ab  cd   ef    gh", " x x  x   x    ", "#  #"];
        for (row, (_, want)) in rows.iter().zip(twin("mass.")) {
            let chars: Vec<char> = row.chars().collect();
            let got: String = mass(&chars)
                .iter()
                .map(|&b| if b { '#' } else { '.' })
                .collect();
            assert_eq!(if got.is_empty() { "-" } else { &got }, want);
        }
        let (_, w) = twin("wave ")[0];
        let got: Vec<String> = (-8..40).map(|x| wave(x).to_string()).collect();
        assert_eq!(got.join(","), w);
        for (name, want) in twin("arc.") {
            let n = nums(&name[4..]);
            let got: Vec<String> = arc((n[0], n[1]), (n[2], n[3]))
                .iter()
                .map(|(x, y)| format!("{x}:{y}"))
                .collect();
            assert_eq!(got.join(","), want, "{name}");
        }
        for (name, want) in twin("set.") {
            let got: Vec<String> = set_order(&nums(&name[4..]))
                .iter()
                .map(i64::to_string)
                .collect();
            assert_eq!(got.join(","), want, "{name}");
        }
        for (name, want) in twin("near.") {
            let n: Vec<i64> = name[5..].split('.').map(|p| p.parse().unwrap()).collect();
            assert_eq!(
                u8::from(near(n[0], n[1], Some((n[2], n[3])))).to_string(),
                want,
                "{name}"
            );
        }
    }

    #[test]
    fn ledges_of_a_town() {
        let first = ledges(&["", "", "", "0123456789"], (0, 0, 40, 6));
        assert_eq!(first[0].x1, 5, "over the end of a line, not on nothing");
        let path = arc((0, 10), (30, 14));
        assert_eq!(path.last(), Some(&(30, 14)));
        assert!(path.iter().map(|p| p.1).min().unwrap() < 10, "up first");
        assert!(near(10, 20, Some((12, 18))) && near(10, 20, Some((4, 21))));
        assert!(!near(10, 20, Some((40, 18))) && !near(10, 20, None));
    }
}
