// SPDX-License-Identifier: GPL-3.0-or-later
//! The map of a frame's text: pure, no host call.
//!
//! A Braille cell holds 2 by 4 dots; a dot is `cols` columns by `rows`
//! lines, on when half of its lines hold ink there (a marked line: on
//! alone). The scale is the least that fits the text in the map.

use std::collections::{BTreeMap, HashMap};

use ptyrc_app::{Color, Run};

/// Columns of the text mapped; the rest is cut.
pub const WIDTH: i64 = 100;
/// Inner rows of the variants.
pub const HEIGHTS: [u32; 8] = [4, 6, 8, 12, 16, 24, 32, 48];
const BRAILLE: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
/// What starts a comment.
const NOTES: [&str; 6] = ["# ", "#!", "//", "-- ", "/*", ";;"];
/// Foregrounds, out of the view and in it.
pub const CODE: [u8; 2] = [242, 255];
pub const NOTE: [u8; 2] = [65, 114];
/// Backgrounds: the view, the line of the cursor.
pub const GROUND: u8 = 237;
pub const CURSOR: u8 = 60;
/// Heat, newest first: edited, visited, both.
pub const EDITED: [u8; 3] = [208, 172, 130];
pub const VISITED: [u8; 3] = [75, 68, 60];
pub const BOTH: [u8; 3] = [213, 170, 133];
/// The foregrounds of the patterns, in turn (those of `marks`).
pub const INKS: [u8; 6] = [226, 120, 81, 213, 215, 203];

/// What is drawn beside the ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub cursor: bool,
    pub heat: bool,
    pub notes: bool,
    /// Seconds a heat lasts.
    pub fade: f64,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            cursor: false,
            heat: false,
            notes: false,
            fade: 60.0,
        }
    }
}

/// What is known of a frame. `lines[i]` is the frame row `y0 + i`; the
/// view is the rows `top <= y < top + view`, `width` columns. Times in
/// seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub y0: i64,
    pub lines: Vec<String>,
    pub top: i64,
    pub view: i64,
    pub width: i64,
    /// A frame row.
    pub cursor: Option<i64>,
    /// Frame row: the first pattern found in it.
    pub marks: BTreeMap<i64, usize>,
    /// Frame row: when.
    pub edited: HashMap<i64, f64>,
    pub visited: HashMap<i64, f64>,
}

impl Default for Doc {
    fn default() -> Doc {
        Doc {
            y0: 0,
            lines: Vec::new(),
            top: 0,
            view: 0,
            width: 80,
            cursor: None,
            marks: BTreeMap::new(),
            edited: HashMap::new(),
            visited: HashMap::new(),
        }
    }
}

impl Doc {
    /// The rows of the view visited at `now`.
    pub fn visit(&mut self, now: f64) {
        for y in self.top..self.top + self.view {
            self.visited.insert(y, now);
        }
    }

    /// The heat that has faded, dropped.
    pub fn forget(&mut self, now: f64, fade: f64) {
        self.edited.retain(|_, at| now - *at < fade);
        self.visited.retain(|_, at| now - *at < fade);
    }
}

/// Columns and lines a dot: the least that fit `count` lines of `width`
/// columns in `w` by `h` cells.
pub fn scale(count: usize, width: i64, w: u32, h: u32) -> (usize, usize) {
    let cols = div_ceil(width.clamp(0, WIDTH) as usize, 2 * w as usize).max(1);
    (cols, div_ceil(count, 4 * h as usize).max(1))
}

fn div_ceil(a: usize, b: usize) -> usize {
    a.div_ceil(b.max(1))
}

/// The dot columns of `line` that hold ink, a bit each (`w` up to 64).
pub fn inks(line: &str, cols: usize, w: u32) -> u128 {
    let mut bits = 0u128;
    for (i, c) in line.chars().enumerate() {
        let x = i / cols;
        if x >= 2 * w as usize {
            break;
        }
        if !c.is_whitespace() {
            bits |= 1 << x;
        }
    }
    bits
}

/// The frame row told for the cell row `y` of the map: the first of its
/// lines that holds ink; `None` when none does.
pub fn line_at(doc: &Doc, w: u32, h: u32, y: u32) -> Option<i64> {
    let (_, rows) = scale(doc.lines.len(), doc.width, w, h);
    let lo = 4 * y as usize * rows;
    let hi = doc.lines.len().min(4 * (y as usize + 1) * rows);
    (lo..hi)
        .find(|&i| !doc.lines[i].trim().is_empty())
        .map(|i| doc.y0 + i as i64)
}

/// The step of the newest heat on `rows`: 0 to 2; `None` when cold.
fn level(heat: &HashMap<i64, f64>, lo: i64, hi: i64, now: f64, fade: f64) -> Option<usize> {
    let age = (lo..hi)
        .filter_map(|y| heat.get(&y).map(|at| now - at))
        .reduce(f64::min)?;
    if age >= fade {
        return None;
    }
    Some(2.min((3.0 * age.max(0.0) / fade) as usize))
}

fn is_note(line: &str) -> bool {
    let t = line.trim_start();
    NOTES.iter().any(|n| t.starts_with(n))
}

fn pal(n: u8) -> Color {
    Color::Palette(n)
}

/// What a face of `w` cells across reads of each line: its dots, a
/// comment or not, marked or not; the same at every height.
#[derive(Clone, Debug, PartialEq)]
pub struct Lines {
    pub w: u32,
    pub ink: Vec<u128>,
    pub note: Vec<bool>,
    pub marked: Vec<bool>,
}

impl Lines {
    pub fn of(doc: &Doc, w: u32) -> Lines {
        let (cols, _) = scale(doc.lines.len(), doc.width, w, 1);
        let at = |i: usize| doc.y0 + i as i64;
        Lines {
            w,
            ink: doc.lines.iter().map(|l| inks(l, cols, w)).collect(),
            note: doc.lines.iter().map(|l| is_note(l)).collect(),
            marked: (0..doc.lines.len())
                .map(|i| doc.marks.contains_key(&at(i)))
                .collect(),
        }
    }
}

/// The map: `h` rows of runs, `w` cells each.
///
/// A cell has one colour. Foreground, the first that holds: a mark (its
/// pattern's), an edit (`look.heat`), the view, a visit, a comment
/// (`look.notes`), code. Background: the line of the cursor
/// (`look.cursor`), then the view.
pub fn face(doc: &Doc, w: u32, h: u32, look: &Look, now: f64) -> Vec<Vec<Run>> {
    face_of(doc, &Lines::of(doc, w), h, look, now)
}

/// `face`, of the lines read once for `lines.w`.
pub fn face_of(doc: &Doc, lines: &Lines, h: u32, look: &Look, now: f64) -> Vec<Vec<Run>> {
    let (n, w) = (doc.lines.len(), lines.w);
    let (_, rows) = scale(n, doc.width, w, h);
    let dots = 2 * w as usize;
    let mut out = Vec::with_capacity(h as usize);
    // A part (`rows` lines) a dot row: of each dot column, the lines
    // with ink there, one marked among them, one not a comment.
    let mut count = vec![0usize; dots];
    let mut marked = vec![false; dots];
    let mut code = vec![false; dots];
    for cy in 0..h as usize {
        let first = 4 * cy * rows;
        let (lo, hi) = (
            doc.y0 + first as i64,
            doc.y0 + n.min(first + 4 * rows) as i64,
        );
        let seen = lo < doc.top + doc.view && doc.top < hi;
        let mut bg = seen.then_some(GROUND);
        if look.cursor && doc.cursor.is_some_and(|c| lo <= c && c < hi) {
            bg = Some(CURSOR);
        }
        let (mut edited, mut visited) = (None, None);
        if look.heat {
            edited = level(&doc.edited, lo, hi, now, look.fade);
            visited = level(&doc.visited, lo, hi, now, look.fade);
        }
        let pen = (lo < hi)
            .then(|| doc.marks.range(lo..hi).map(|(_, p)| *p).min())
            .flatten();
        // Per cell: its glyph, a dot of comments alone, a dot of code.
        let mut glyphs = vec![(0u8, false, false); w as usize];
        for (dy, bits) in BRAILLE.iter().enumerate() {
            let start = first + dy * rows;
            let end = n.min(start + rows);
            let part = end.saturating_sub(start);
            count.fill(0);
            marked.fill(false);
            code.fill(false);
            for i in start..end {
                let mut ink = lines.ink[i];
                while ink != 0 {
                    let x = ink.trailing_zeros() as usize;
                    ink &= ink - 1;
                    count[x] += 1;
                    marked[x] |= lines.marked[i];
                    code[x] |= !lines.note[i];
                }
            }
            for x in 0..dots {
                if count[x] > 0 && (marked[x] || 2 * count[x] >= part) {
                    let g = &mut glyphs[x / 2];
                    g.0 |= bits[x % 2];
                    g.1 |= !code[x];
                    g.2 |= code[x];
                }
            }
        }
        let mut runs: Vec<Run> = Vec::new();
        let mut last: Option<(u8, Option<u8>)> = None;
        for &(glyph, noted, code) in &glyphs {
            let fg = if let Some(p) = pen {
                INKS[p % INKS.len()]
            } else if let (Some(e), Some(v), false) = (edited, visited, seen) {
                BOTH[e.max(v)]
            } else if let Some(e) = edited {
                EDITED[e]
            } else if seen || visited.is_none() {
                let pens = if look.notes && noted && !code {
                    NOTE
                } else {
                    CODE
                };
                pens[usize::from(seen)]
            } else {
                VISITED[visited.unwrap_or(0)]
            };
            let t = match glyph {
                0 => ' ',
                g => char::from_u32(0x2800 + u32::from(g)).unwrap_or(' '),
            };
            match runs.last_mut() {
                Some(run) if last == Some((fg, bg)) => run.t.push(t),
                _ => {
                    let mut run = Run::new(t.to_string()).fg(pal(fg));
                    if let Some(b) = bg {
                        run = run.bg(pal(b));
                    }
                    runs.push(run);
                    last = Some((fg, bg));
                }
            }
        }
        out.push(runs);
    }
    out
}

/// The lines known: `412`, `3.4k`.
pub fn title(doc: &Doc) -> String {
    let n = doc.lines.len();
    if n < 1000 {
        n.to_string()
    } else {
        format!("{:.1}k", n as f64 / 1000.0)
    }
}

/// The heights offered for `count` lines: those of `HEIGHTS` under the
/// one that shows a line a dot row, and that one.
pub fn heights(count: usize) -> Vec<u32> {
    let need = (count.div_ceil(4) as u32).clamp(HEIGHTS[0], HEIGHTS[HEIGHTS.len() - 1]);
    let mut out: Vec<u32> = HEIGHTS.iter().copied().filter(|&h| h < need).collect();
    out.push(need);
    out
}
