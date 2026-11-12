// SPDX-License-Identifier: GPL-3.0-or-later
//! Screen state kept between updates: per-cell hashes and classes, per-row
//! text. Everything the frames need after the grid is gone.

use ptyrc_vt::{Attrs, Cell, Color, Grid, Style};

/// FNV-1a.
pub(crate) fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Hash of a blank row.
pub(crate) const BLANK: u64 = 0;

/// Hash of a row text, trailing spaces ignored. `BLANK` when empty.
pub(crate) fn text_hash(s: &str) -> u64 {
    let t = s.trim_end_matches(' ');
    if t.is_empty() {
        BLANK
    } else {
        fnv(t.as_bytes()).max(1)
    }
}

/// Text of a run of cells. `cols[k]` is the column of the `k`-th char;
/// `None` when char `k` is at column `k` (one narrow char per cell).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Line {
    pub text: String,
    pub cols: Option<Box<[u16]>>,
    pub ascii: bool,
}

impl Line {
    pub fn from_cells(cells: &[Cell]) -> Line {
        // One ASCII char per cell: the usual row.
        if cells.iter().all(|c| c.width == 1 && c.text.len() <= 1) {
            let byte = |c: &Cell| c.text.as_bytes().first().map_or(' ', |b| *b as char);
            return Line {
                text: cells.iter().map(byte).collect(),
                cols: None,
                ascii: true,
            };
        }
        let mut text = String::with_capacity(cells.len());
        let mut cols: Option<Vec<u16>> = None;
        let mut ascii = true;
        let mut count = 0usize;
        for (c, cell) in cells.iter().enumerate() {
            if cell.width == 0 {
                if cols.is_none() {
                    cols = Some((0..count as u16).collect());
                }
                continue;
            }
            let t: &str = if cell.text.is_empty() {
                " "
            } else {
                cell.text.as_str()
            };
            let n = if t.len() == 1 { 1 } else { t.chars().count() };
            if (n != 1 || c != count) && cols.is_none() {
                cols = Some((0..count as u16).collect());
            }
            if let Some(v) = cols.as_mut() {
                v.extend(std::iter::repeat_n(c as u16, n));
            }
            ascii &= t.is_ascii();
            text.push_str(t);
            count += n;
        }
        Line {
            text,
            cols: cols.map(Vec::into_boxed_slice),
            ascii,
        }
    }

    /// One narrow char per column.
    pub fn trivial(&self) -> bool {
        self.cols.is_none()
    }

    /// Columns `[a, b)`, rebased to `a`.
    pub fn slice(&self, a: u16, b: u16) -> Line {
        if self.cols.is_none() && self.ascii {
            let end = (b as usize).min(self.text.len());
            let start = (a as usize).min(end);
            return Line {
                text: self.text[start..end].to_string(),
                cols: None,
                ascii: true,
            };
        }
        let mut text = String::new();
        let mut cols: Vec<u16> = Vec::new();
        let mut trivial = true;
        for (k, ch) in self.text.chars().enumerate() {
            let col = self.col_of(k);
            if col < a || col >= b {
                continue;
            }
            let rel = col - a;
            trivial &= rel as usize == cols.len();
            cols.push(rel);
            text.push(ch);
        }
        let ascii = text.is_ascii();
        Line {
            text,
            cols: if trivial {
                None
            } else {
                Some(cols.into_boxed_slice())
            },
            ascii,
        }
    }

    /// Column of the `k`-th char.
    pub fn col_of(&self, k: usize) -> u16 {
        match &self.cols {
            Some(c) => c
                .get(k)
                .copied()
                .unwrap_or_else(|| c.last().map_or(0, |l| l + 1)),
            None => k as u16,
        }
    }

    /// A row without text.
    pub fn blank() -> Line {
        Line {
            text: String::new(),
            cols: None,
            ascii: true,
        }
    }

    pub fn trim_end(&mut self) {
        let n = self.text.trim_end_matches(' ').len();
        if n == self.text.len() {
            return;
        }
        self.text.truncate(n);
        if let Some(c) = self.cols.take() {
            let keep = self.text.chars().count();
            self.cols = Some(c[..keep.min(c.len())].to_vec().into_boxed_slice());
        }
    }

    pub fn hash(&self) -> u64 {
        text_hash(&self.text)
    }
}

/// Cell classes.
pub(crate) mod class {
    pub const V: u16 = 1; // vertical border glyph
    pub const H: u16 = 2; // horizontal border glyph
    pub const PIPE: u16 = 4; // '|'
    pub const DASH: u16 = 8; // '-'
    pub const TL: u16 = 16;
    pub const TR: u16 = 32;
    pub const BL: u16 = 64;
    pub const BR: u16 = 128;
    pub const BLANK: u16 = 256;
}

fn classify(text: &str) -> u16 {
    use class::*;
    match text.as_bytes().first() {
        None | Some(b' ') => return BLANK,
        Some(b'|') => return PIPE,
        Some(b'-') => return DASH,
        Some(b) if b.is_ascii() => return 0,
        Some(_) => {}
    }
    let Some(ch) = text.chars().next() else {
        return BLANK;
    };
    match ch {
        ' ' => BLANK,
        '|' => PIPE,
        '-' => DASH,
        '│' | '┃' | '║' | '╎' | '╏' | '┆' | '┇' | '┊' | '┋' | '▏' | '▕' | '▌' | '▐' => {
            V
        }
        '─' | '━' | '═' | '╌' | '╍' | '┄' | '┅' | '┈' | '┉' => H,
        '┌' | '╭' | '┏' | '╔' => V | H | TL,
        '┐' | '╮' | '┓' | '╗' => V | H | TR,
        '└' | '╰' | '┗' | '╚' => V | H | BL,
        '┘' | '╯' | '┛' | '╝' => V | H | BR,
        '\u{251c}'..='\u{254b}' | '\u{255e}'..='\u{256c}' => V | H, // junctions
        _ => 0,
    }
}

fn color_code(c: Color) -> u32 {
    match c {
        Color::Default => 0,
        Color::Indexed(i) => 1 + i as u32,
        Color::Rgb(r, g, b) => 0x0100_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

/// What fills the background of a cell: 0 for nothing, else a key of the
/// colour shown.
fn paint(style: &Style) -> u16 {
    if style.attrs.contains(Attrs::INVERSE) {
        let c = color_code(style.fg);
        0x8000 | ((c ^ (c >> 15)) as u16 & 0x7fff)
    } else {
        match color_code(style.bg) {
            0 => 0,
            c => ((c ^ (c >> 15)) as u16 & 0x7fff).max(1),
        }
    }
}

fn style_hash(s: &Style) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in [
        color_code(s.fg),
        color_code(s.bg),
        color_code(s.ul),
        s.attrs.0 as u32,
        s.link,
    ] {
        h = (h ^ v as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Hash of a cell; `style` is the `style_hash` of its style.
fn cell_hash(cell: &Cell, style: u64) -> u32 {
    let h = (fnv(cell.text.as_bytes()) ^ style).wrapping_mul(0x0100_0000_01b3);
    (h ^ (h >> 32)) as u32
}

/// What a run of cells of one style shares.
struct Pen {
    style: Style,
    hash: u64,
    paint: u16,
    /// Colour code of the backdrop.
    back: u32,
    /// `Cell::has_ink` of a blank, and of a glyph.
    ink: [bool; 2],
}

impl Pen {
    fn new(style: Style) -> Pen {
        let probe = |text| Cell::narrow(text, style);
        Pen {
            style,
            hash: style_hash(&style),
            paint: paint(&style),
            back: color_code(probe(" ").backdrop()),
            ink: [probe(" ").has_ink(), probe("a").has_ink()],
        }
    }
}

/// Hash of what a cell shows: equal for cells that look alike
/// (`Cell::looks_like`). `full` is its `cell_hash`.
fn look_hash(ink: bool, full: u32, back: u32) -> u32 {
    if ink {
        full | 1
    } else {
        back.wrapping_mul(0x9e37_79b1) & !1
    }
}

#[derive(Default)]
pub(crate) struct Screen {
    pub cols: u16,
    pub rows: u16,
    hash: Vec<u32>,
    pub class: Vec<u16>,
    pub paint: Vec<u16>,
    /// What the cell shows; see `look_hash`.
    pub look: Vec<u32>,
    /// Foreground: `Cell::has_ink`.
    pub ink: Vec<bool>,
    /// Background: colour code of `Cell::backdrop`, 0 for the default.
    pub back: Vec<u32>,
    pub row_dirty: Vec<bool>,
    pub lines: Vec<Line>,
    /// Lines before the last scan; meaningful for dirty rows only.
    pub prev_lines: Vec<Line>,
    pub any_change: bool,
    pub class_change: bool,
    /// `Grid::row_stamp` of each row at its last scan.
    stamps: Vec<(u64, u64)>,
    /// Number of scans that found a change.
    pub changes: u64,
}

impl Screen {
    /// Take a grid in. Returns `true` when the size changed (everything is
    /// then dirty).
    pub fn scan(&mut self, grid: &Grid) -> bool {
        let (cols, rows) = (grid.cols(), grid.rows());
        let resized = cols != self.cols || rows != self.rows;
        if resized {
            let n = cols as usize * rows as usize;
            self.cols = cols;
            self.rows = rows;
            self.hash = vec![0; n];
            self.class = vec![0; n];
            self.paint = vec![0; n];
            self.look = vec![0; n];
            self.ink = vec![false; n];
            self.back = vec![0; n];
            self.row_dirty = vec![false; rows as usize];
            self.lines = vec![Line::default(); rows as usize];
            self.prev_lines = vec![Line::default(); rows as usize];
            self.stamps = vec![(0, 0); rows as usize];
        }
        self.any_change = resized;
        self.class_change = resized;
        let w = cols as usize;
        // The style of a run of cells is hashed once.
        let mut pen = Pen::new(Style::default());
        for r in 0..rows as usize {
            let cells = grid.row(r as u16);
            let base = r * w;
            // A row not written since its last scan: nothing to read.
            let stamp = grid.row_stamp(r as u16);
            if stamp == self.stamps[r] {
                debug_assert!(self.same_row(r, cells), "row {r} written, stamp kept");
                self.row_dirty[r] = false;
                continue;
            }
            self.stamps[r] = stamp;
            let mut dirty = resized;
            for (c, cell) in cells.iter().enumerate() {
                if cell.style != pen.style {
                    pen = Pen::new(cell.style);
                }
                let h = cell_hash(cell, pen.hash);
                let i = base + c;
                if h != self.hash[i] || resized {
                    self.hash[i] = h;
                    dirty = true;
                    let cl = classify(&cell.text);
                    let p = pen.paint;
                    if cl != self.class[i] || p != self.paint[i] {
                        self.class[i] = cl;
                        self.paint[i] = p;
                        self.class_change = true;
                    }
                    let ink = pen.ink[!cell.is_blank() as usize];
                    debug_assert_eq!(ink, cell.has_ink());
                    debug_assert_eq!(pen.back, color_code(cell.backdrop()));
                    self.back[i] = pen.back;
                    self.ink[i] = ink;
                    self.look[i] = look_hash(ink, h, pen.back);
                }
            }
            self.row_dirty[r] = dirty;
            if dirty {
                self.any_change = true;
                let line = Line::from_cells(cells);
                self.prev_lines[r] = std::mem::replace(&mut self.lines[r], line);
            }
        }
        self.changes += self.any_change as u64;
        resized
    }

    /// The cells are those of the last scan of row `r`.
    fn same_row(&self, r: usize, cells: &[Cell]) -> bool {
        let base = r * self.cols as usize;
        cells
            .iter()
            .enumerate()
            .all(|(c, cell)| cell_hash(cell, style_hash(&cell.style)) == self.hash[base + c])
    }

    /// Forget the change marks of the last scan.
    pub fn settle(&mut self) {
        if self.any_change {
            self.row_dirty.fill(false);
            self.any_change = false;
            self.class_change = false;
        }
    }

    pub fn idx(&self, col: u16, row: u16) -> usize {
        row as usize * self.cols as usize + col as usize
    }

    /// Hash of columns `[a, b)` of a row.
    pub fn row_hash(&self, row: u16, a: u16, b: u16) -> u64 {
        Self::hash_of(&self.lines[row as usize], a, b)
    }

    pub fn hash_of(line: &Line, a: u16, b: u16) -> u64 {
        if line.cols.is_none() && line.ascii {
            let end = (b as usize).min(line.text.len());
            let start = (a as usize).min(end);
            text_hash(&line.text[start..end])
        } else {
            line.slice(a, b).hash()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::Style;

    #[test]
    fn line_slices() {
        let g = Grid::from_lines(10, &["abcdef"]);
        let l = Line::from_cells(g.row(0));
        assert!(l.trivial() && l.ascii);
        assert_eq!(l.slice(2, 4).text, "cd");
        let mut g = Grid::new(8, 1);
        g.put(0, 0, Cell::narrow("a", Style::default()));
        g.put(1, 0, Cell::wide("世", Style::default()));
        g.put(3, 0, Cell::narrow("b", Style::default()));
        let l = Line::from_cells(g.row(0));
        assert!(!l.trivial());
        assert_eq!(l.col_of(2), 3);
        let s = l.slice(1, 4);
        assert_eq!(s.text, "世b");
        assert_eq!(s.col_of(1), 2);
        assert_eq!(Screen::hash_of(&l, 3, 8), text_hash("b"));
    }

    #[test]
    fn ascii_rows_take_the_short_path() {
        let mut g = Grid::from_lines(6, &["a|-  b"]);
        g.cell_mut(4, 0).text = Default::default();
        let l = Line::from_cells(g.row(0));
        assert_eq!(
            (l.text.as_str(), l.trivial(), l.ascii),
            ("a|-  b", true, true)
        );
        let classes: Vec<u16> = g.row(0).iter().map(|c| classify(&c.text)).collect();
        use class::*;
        assert_eq!(classes, [0, PIPE, DASH, BLANK, BLANK, 0]);
        assert_eq!(classify("é"), 0);
        assert_eq!(classify("│"), V);
    }

    #[test]
    fn scan_marks_changes() {
        let mut s = Screen::default();
        let g = Grid::from_lines(4, &["ab", "cd"]);
        assert!(s.scan(&g));
        assert!(!s.scan(&g));
        assert!(!s.any_change);
        let g2 = Grid::from_lines(4, &["ab", "cx"]);
        s.scan(&g2);
        assert!(s.any_change && s.row_dirty[1] && !s.row_dirty[0]);
        assert_eq!(s.prev_lines[1].text, "cd  ");
    }

    #[test]
    fn scan_skips_rows_not_written() {
        let mut s = Screen::default();
        let mut g = Grid::from_lines(4, &["ab", "cd", "ef"]);
        s.scan(&g);
        let changes = s.changes;
        // Same grid: no row is read.
        s.scan(&g);
        assert!(!s.any_change && s.changes == changes);
        // A write that changes nothing: the row is read, and found clean.
        g.put(0, 1, Cell::narrow("c", Style::default()));
        s.scan(&g);
        assert!(!s.any_change && !s.row_dirty[1]);
        g.put(1, 2, Cell::narrow("x", Style::default()));
        s.scan(&g);
        assert_eq!(s.row_dirty, [false, false, true]);
        assert_eq!(s.changes, changes + 1);
        assert_eq!(s.lines[2].text, "ex  ");
        s.settle();
        // Another grid, equal or not, is read whole.
        let c = g.clone();
        s.scan(&c);
        assert!(!s.any_change);
        let other = Grid::from_lines(4, &["ab", "cd", "zz"]);
        s.scan(&other);
        assert_eq!(s.row_dirty, [false, false, true]);
        s.settle();
        // Back to the first one, whose stamps were seen: read again.
        s.scan(&g);
        assert_eq!(s.row_dirty, [false, false, true]);
        assert_eq!(s.lines[2].text, "ex  ");
        // A scroll moves every row of its region.
        s.settle();
        g.scroll_up(0, 3, 1, &Cell::default());
        s.scan(&g);
        assert_eq!(s.row_dirty, [true, true, true]);
        assert_eq!(s.lines[0].text, "cd  ");
    }

    #[test]
    fn look_ignores_the_pen_of_a_blank() {
        let grey = Style {
            bg: Color::Indexed(8),
            ..Style::default()
        };
        let mut bold = grey;
        bold.attrs.insert(Attrs::BOLD);
        bold.fg = Color::Indexed(1);
        let mut g = Grid::new(3, 1);
        g.put(0, 0, Cell::blank(grey));
        g.put(1, 0, Cell::narrow("a", grey));
        let mut s = Screen::default();
        s.scan(&g);
        let before = s.look.clone();
        assert!(!s.ink[0] && s.ink[1] && !s.ink[2]);
        assert_eq!((s.back[0], s.back[2]), (9, 0));
        assert_ne!(s.look[0], s.look[2]);
        g.put(0, 0, Cell::blank(bold));
        g.put(1, 0, Cell::narrow("a", bold));
        s.scan(&g);
        assert!(s.row_dirty[0]);
        assert_eq!(s.look[0], before[0]);
        assert_ne!(s.look[1], before[1]);
    }
}
