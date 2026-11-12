// SPDX-License-Identifier: GPL-3.0-or-later
//! Cell grid: the screen model shared by the emulator, the frames and the
//! compositor.

use smol_str::SmolStr;

/// Terminal colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Attribute bit set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attrs(pub u16);

impl Attrs {
    pub const BOLD: u16 = 1 << 0;
    pub const DIM: u16 = 1 << 1;
    pub const ITALIC: u16 = 1 << 2;
    pub const UNDERLINE: u16 = 1 << 3;
    pub const BLINK: u16 = 1 << 4;
    pub const INVERSE: u16 = 1 << 5;
    pub const HIDDEN: u16 = 1 << 6;
    pub const STRIKE: u16 = 1 << 7;
    pub const OVERLINE: u16 = 1 << 8;
    /// Underline shapes; at most one, with `UNDERLINE` set.
    pub const UL_DOUBLE: u16 = 1 << 9;
    pub const UL_CURLY: u16 = 1 << 10;
    pub const UL_DOTTED: u16 = 1 << 11;
    pub const UL_DASHED: u16 = 1 << 12;
    pub const UL_SHAPES: u16 = Self::UL_DOUBLE | Self::UL_CURLY | Self::UL_DOTTED | Self::UL_DASHED;

    pub fn contains(self, bits: u16) -> bool {
        self.0 & bits == bits
    }

    pub fn insert(&mut self, bits: u16) {
        self.0 |= bits;
    }

    pub fn remove(&mut self, bits: u16) {
        self.0 &= !bits;
    }
}

/// Rendition of a cell. `link` is a hyperlink id of the owning `Vt`, 0 for
/// none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub ul: Color,
    pub attrs: Attrs,
    pub link: u32,
}

/// One cell. `width` is 1, 2 for the head of a wide glyph, 0 for its
/// spacer (empty `text`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub text: SmolStr,
    pub width: u8,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell::blank(Style::default())
    }
}

impl Cell {
    /// A space with `style`.
    pub fn blank(style: Style) -> Cell {
        Cell {
            text: SmolStr::new_static(" "),
            width: 1,
            style,
        }
    }

    /// A narrow glyph.
    pub fn narrow(text: &str, style: Style) -> Cell {
        Cell {
            text: SmolStr::new(text),
            width: 1,
            style,
        }
    }

    /// The head of a wide glyph.
    pub fn wide(text: &str, style: Style) -> Cell {
        Cell {
            text: SmolStr::new(text),
            width: 2,
            style,
        }
    }

    /// The spacer after a wide head.
    pub fn spacer(style: Style) -> Cell {
        Cell {
            text: SmolStr::default(),
            width: 0,
            style,
        }
    }

    /// No glyph: a space or a spacer of a blank.
    pub fn is_blank(&self) -> bool {
        self.text == " " || (self.width == 1 && self.text.is_empty())
    }

    /// Blank and nothing visible behind it: default background, not inverse.
    pub fn is_empty(&self) -> bool {
        self.is_blank()
            && self.style.bg == Color::Default
            && !self.style.attrs.contains(Attrs::INVERSE)
    }

    /// Foreground: the cell shows a glyph or a line. A blank is ink only
    /// when underlined, struck or overlined; hidden text is not.
    pub fn has_ink(&self) -> bool {
        const LINES: u16 = Attrs::UNDERLINE | Attrs::STRIKE | Attrs::OVERLINE;
        if self.is_blank() {
            self.style.attrs.0 & LINES != 0
        } else {
            !self.style.attrs.contains(Attrs::HIDDEN)
        }
    }

    /// Background: the colour that fills the cell. `bg`, or `fg` when
    /// inverse.
    pub fn backdrop(&self) -> Color {
        if self.style.attrs.contains(Attrs::INVERSE) {
            self.style.fg
        } else {
            self.style.bg
        }
    }

    /// Same on screen. Two cells without ink look alike when their
    /// backdrops are equal, whatever the rest of their style.
    pub fn looks_like(&self, other: &Cell) -> bool {
        match (self.has_ink(), other.has_ink()) {
            (false, false) => self.backdrop() == other.backdrop(),
            (true, true) => self == other,
            _ => false,
        }
    }
}

/// Screen rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> u16 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u16 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        col >= self.x && col < self.right() && row >= self.y && row < self.bottom()
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        if r <= x || b <= y {
            Rect::default()
        } else {
            Rect::new(x, y, r - x, b - y)
        }
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        !self.intersect(other).is_empty()
    }
}

/// `cols x rows` cells. Rows are stored through an indirection table, so a
/// vertical scroll moves no cell.
#[derive(Clone, Debug)]
pub struct Grid {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    /// Storage row of each screen row.
    lines: Vec<u16>,
}

impl PartialEq for Grid {
    fn eq(&self, other: &Grid) -> bool {
        self.cols == other.cols
            && self.rows == other.rows
            && (0..self.rows).all(|r| self.row(r) == other.row(r))
    }
}

impl Eq for Grid {}

impl Grid {
    pub fn new(cols: u16, rows: u16) -> Grid {
        Grid {
            cols,
            rows,
            cells: vec![Cell::default(); cols as usize * rows as usize],
            lines: (0..rows).collect(),
        }
    }

    /// A grid from text lines, one cell per `char` (narrow), default style.
    /// For tests and tools.
    pub fn from_lines(cols: u16, lines: &[&str]) -> Grid {
        let mut grid = Grid::new(cols, lines.len() as u16);
        for (row, line) in lines.iter().enumerate() {
            for (col, ch) in line.chars().take(cols as usize).enumerate() {
                let mut buf = [0u8; 4];
                let cell = Cell::narrow(ch.encode_utf8(&mut buf), Style::default());
                grid.put(col as u16, row as u16, cell);
            }
        }
        grid
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.cols, self.rows)
    }

    fn index(&self, col: u16, row: u16) -> usize {
        debug_assert!(col < self.cols && row < self.rows);
        self.lines[row as usize] as usize * self.cols as usize + col as usize
    }

    pub fn cell(&self, col: u16, row: u16) -> &Cell {
        &self.cells[self.index(col, row)]
    }

    /// Raw access: the caller keeps wide pairs consistent. Prefer `put`.
    pub fn cell_mut(&mut self, col: u16, row: u16) -> &mut Cell {
        let i = self.index(col, row);
        &mut self.cells[i]
    }

    pub fn row(&self, row: u16) -> &[Cell] {
        let start = self.index(0, row);
        &self.cells[start..start + self.cols as usize]
    }

    pub fn row_mut(&mut self, row: u16) -> &mut [Cell] {
        let start = self.index(0, row);
        let cols = self.cols as usize;
        &mut self.cells[start..start + cols]
    }

    /// Detach the cell at `(col, row)` from a wide pair: the other half
    /// becomes a blank of its own style.
    fn orphan(&mut self, col: u16, row: u16) {
        match self.cell(col, row).width {
            0 if col > 0 => {
                let head = self.cell_mut(col - 1, row);
                if head.width == 2 {
                    *head = Cell::blank(head.style);
                }
            }
            2 if col + 1 < self.cols => {
                let spacer = self.cell_mut(col + 1, row);
                if spacer.width == 0 {
                    *spacer = Cell::blank(spacer.style);
                }
            }
            _ => {}
        }
    }

    /// Write a glyph. A wide glyph it cuts loses its other half, as on a
    /// terminal. A wide `cell` also takes `col + 1` (its spacer); it is
    /// refused, and `false` returned, on the last column. A spacer cell is
    /// refused: spacers come with their head.
    pub fn put(&mut self, col: u16, row: u16, cell: Cell) -> bool {
        if col >= self.cols || row >= self.rows || cell.width == 0 {
            return false;
        }
        if cell.width == 2 {
            if col + 1 >= self.cols {
                return false;
            }
            self.orphan(col, row);
            self.orphan(col + 1, row);
            let style = cell.style;
            *self.cell_mut(col, row) = cell;
            *self.cell_mut(col + 1, row) = Cell::spacer(style);
        } else {
            self.orphan(col, row);
            *self.cell_mut(col, row) = cell;
        }
        true
    }

    /// Write printable ASCII bytes as narrow cells from `(col, row)`; the
    /// run must fit in the row. Same result as `put` for each.
    pub fn put_ascii(&mut self, col: u16, row: u16, bytes: &[u8], style: Style) {
        let n = bytes.len();
        if n == 0 || row >= self.rows || col as usize + n > self.cols as usize {
            return;
        }
        self.orphan(col, row);
        self.orphan(col + n as u16 - 1, row);
        let start = self.index(col, row);
        for (cell, b) in self.cells[start..start + n].iter_mut().zip(bytes) {
            let ch = [*b];
            cell.text = SmolStr::new_inline(std::str::from_utf8(&ch).unwrap_or(" "));
            cell.width = 1;
            cell.style = style;
        }
    }

    /// Fill a row with copies of `fill`.
    pub fn clear_row(&mut self, row: u16, fill: &Cell) {
        for cell in self.row_mut(row) {
            cell.clone_from(fill);
        }
    }

    /// Fill columns `c0..c1` of a row with `fill` (narrow); wide glyphs cut
    /// at the edges lose their other half.
    pub fn erase(&mut self, row: u16, c0: u16, c1: u16, fill: &Cell) {
        let c1 = c1.min(self.cols);
        if row >= self.rows || c0 >= c1 {
            return;
        }
        self.orphan(c0, row);
        self.orphan(c1 - 1, row);
        let start = self.index(c0, row);
        for cell in &mut self.cells[start..start + (c1 - c0) as usize] {
            cell.clone_from(fill);
        }
    }

    /// Move rows `top..bottom` up by `n`; the freed rows take `fill`.
    pub fn scroll_up(&mut self, top: u16, bottom: u16, n: u16, fill: &Cell) {
        let bottom = bottom.min(self.rows);
        if top >= bottom || n == 0 {
            return;
        }
        let n = n.min(bottom - top);
        self.lines[top as usize..bottom as usize].rotate_left(n as usize);
        for row in bottom - n..bottom {
            self.clear_row(row, fill);
        }
    }

    /// Move rows `top..bottom` down by `n`; the freed rows take `fill`.
    pub fn scroll_down(&mut self, top: u16, bottom: u16, n: u16, fill: &Cell) {
        let bottom = bottom.min(self.rows);
        if top >= bottom || n == 0 {
            return;
        }
        let n = n.min(bottom - top);
        self.lines[top as usize..bottom as usize].rotate_right(n as usize);
        for row in top..top + n {
            self.clear_row(row, fill);
        }
    }

    /// Insert `n` cells of `fill` at `(col, row)`; the row's tail shifts
    /// right and falls off the edge.
    pub fn insert_cells(&mut self, col: u16, row: u16, n: u16, fill: &Cell) {
        if col >= self.cols || row >= self.rows || n == 0 {
            return;
        }
        let n = n.min(self.cols - col);
        if self.cell(col, row).width == 0 {
            self.orphan(col, row);
            *self.cell_mut(col, row) = fill.clone();
        }
        let start = self.index(col, row);
        let end = self.index(0, row) + self.cols as usize;
        let tail = &mut self.cells[start..end];
        tail.rotate_right(n as usize);
        for cell in &mut tail[..n as usize] {
            cell.clone_from(fill);
        }
        let last = self.cell_mut(self.cols - 1, row);
        if last.width == 2 {
            *last = Cell::blank(last.style);
        }
    }

    /// Delete `n` cells at `(col, row)`; the row's tail shifts left and
    /// `fill` enters from the edge.
    pub fn delete_cells(&mut self, col: u16, row: u16, n: u16, fill: &Cell) {
        if col >= self.cols || row >= self.rows || n == 0 {
            return;
        }
        let n = n.min(self.cols - col);
        if self.cell(col, row).width == 0 {
            self.orphan(col, row);
        }
        let start = self.index(col, row);
        let end = self.index(0, row) + self.cols as usize;
        let tail = &mut self.cells[start..end];
        tail.rotate_left(n as usize);
        let len = tail.len();
        for cell in &mut tail[len - n as usize..] {
            cell.clone_from(fill);
        }
        let first = self.cell_mut(col, row);
        if first.width == 0 {
            *first = Cell::blank(first.style);
        }
    }

    /// Text of a row, spacers skipped, trailing blanks kept.
    pub fn row_text(&self, row: u16) -> String {
        self.row(row)
            .iter()
            .filter(|c| c.width != 0)
            .map(|c| c.text.as_str())
            .collect()
    }

    /// Text of the grid, one string per row, trailing spaces trimmed.
    pub fn text(&self) -> Vec<String> {
        (0..self.rows)
            .map(|r| self.row_text(r).trim_end_matches(' ').to_string())
            .collect()
    }

    /// Crop or pad to a new size, top-left kept, no reflow.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = Grid::new(cols, rows);
        for row in 0..rows.min(self.rows) {
            for col in 0..cols.min(self.cols) {
                *next.cell_mut(col, row) = self.cell(col, row).clone();
            }
            // A wide head cut by the new edge.
            if cols < self.cols && cols > 0 && next.cell(cols - 1, row).width == 2 {
                let style = next.cell(cols - 1, row).style;
                *next.cell_mut(cols - 1, row) = Cell::blank(style);
            }
        }
        *self = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_cuts_wide_pairs() {
        let s = Style::default();
        let mut g = Grid::new(6, 1);
        assert!(g.put(1, 0, Cell::wide("世", s)));
        assert_eq!(g.cell(2, 0).width, 0);
        // Over the spacer: the head goes.
        assert!(g.put(2, 0, Cell::narrow("x", s)));
        assert_eq!(g.row_text(0), "  x   ");
        // Over the head: the spacer goes.
        assert!(g.put(3, 0, Cell::wide("界", s)));
        assert!(g.put(3, 0, Cell::narrow("y", s)));
        assert_eq!(g.row_text(0), "  xy  ");
        assert_eq!(g.cell(4, 0).width, 1);
        // Wide on the last column is refused.
        assert!(!g.put(5, 0, Cell::wide("界", s)));
    }

    #[test]
    fn ink_and_backdrop() {
        let mut inv = Style::default();
        inv.attrs.insert(Attrs::INVERSE);
        inv.fg = Color::Indexed(4);
        let grey = Style {
            bg: Color::Indexed(8),
            ..Style::default()
        };
        let mut bold = grey;
        bold.attrs.insert(Attrs::BOLD);
        let mut ul = Style::default();
        ul.attrs.insert(Attrs::UNDERLINE);
        let mut hid = Style::default();
        hid.attrs.insert(Attrs::HIDDEN);

        assert!(!Cell::default().has_ink());
        assert!(!Cell::blank(grey).has_ink());
        assert!(Cell::blank(ul).has_ink());
        assert!(Cell::narrow("a", grey).has_ink());
        assert!(!Cell::narrow("a", hid).has_ink());
        assert!(Cell::spacer(Style::default()).has_ink());
        assert_eq!(Cell::blank(inv).backdrop(), Color::Indexed(4));
        assert_eq!(Cell::blank(grey).backdrop(), Color::Indexed(8));
        // A cleared cell redrawn with another pen is the same cell.
        assert!(Cell::blank(grey).looks_like(&Cell::blank(bold)));
        assert!(!Cell::blank(grey).looks_like(&Cell::default()));
        assert!(!Cell::narrow("a", grey).looks_like(&Cell::blank(grey)));
        assert!(Cell::narrow("a", grey).looks_like(&Cell::narrow("a", grey)));
    }

    #[test]
    fn text_trims() {
        let g = Grid::from_lines(8, &["ab", "  c  "]);
        assert_eq!(g.text(), vec!["ab", "  c"]);
    }
}
