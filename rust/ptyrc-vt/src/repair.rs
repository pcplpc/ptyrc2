// SPDX-License-Identifier: GPL-3.0-or-later
//! The repair painter: the bytes that turn what the host shows into a
//! desired grid, leaving the terminal state as found.

use crate::grid::{Attrs, Cell, Color, Grid, Style};
use crate::sgr::{push_num, write_sgr, write_sgr_from};
use crate::vt::{Charset, Vt};
use crate::width::is_hidden;
use std::borrow::Cow;
use std::io::Write;
use unicode_width::UnicodeWidthChar;

/// Glyph of a cell as it can be printed: its text when the emulator would
/// store it back in one cell of that width (a base character of the cell's
/// width, then zero-width ones), spaces otherwise. Hidden characters
/// (`is_hidden`) are left out. No control character is ever printed: a
/// cell's text is untrusted (a sprite's, a widget's).
fn glyph(cell: &Cell) -> Cow<'_, str> {
    let text = cell.text.as_str();
    let mut chars = text.chars();
    let ok = match chars.next() {
        Some(c) if !c.is_control() && !is_hidden(c) => {
            let w = if c.is_ascii() {
                1
            } else {
                c.width().unwrap_or(0)
            };
            w == cell.width as usize
                && text.len() <= 32
                && chars.all(|c| !c.is_control() && (is_hidden(c) || c.width() == Some(0)))
        }
        _ => false,
    };
    if !ok {
        Cow::Borrowed(if cell.width == 2 { "  " } else { " " })
    } else if text.chars().any(is_hidden) {
        Cow::Owned(text.chars().filter(|&c| !is_hidden(c)).collect())
    } else {
        Cow::Borrowed(text)
    }
}

/// OSC 8 for link `id` of `vt`; a link that holds a control character
/// closes instead.
fn write_link(out: &mut Vec<u8>, vt: &Vt, id: u32) {
    match vt.link(id) {
        Some(link) if !(link.params.chars().chain(link.uri.chars())).any(char::is_control) => {
            let _ = write!(out, "\x1b]8;{};{}\x1b\\", link.params, link.uri);
        }
        _ => out.extend_from_slice(b"\x1b]8;;\x1b\\"),
    }
}

struct Painter<'a> {
    host: &'a Vt,
    buf: Vec<u8>,
    /// Where the host cursor is, when known.
    at: Option<(u16, u16)>,
    pen: Style,
}

impl Painter<'_> {
    /// Absolute move; origin mode is off while painting.
    fn goto(&mut self, col: u16, row: u16) {
        match self.at {
            Some((c, r)) if c == col && r == row => {}
            Some((c, r)) if r == row && col == c + 1 => self.buf.extend_from_slice(b"\x1b[C"),
            Some((c, r)) if r == row && col > c => {
                self.buf.extend_from_slice(b"\x1b[");
                push_num(&mut self.buf, col - c);
                self.buf.push(b'C');
            }
            Some((_, r)) if r == row => {
                self.buf.extend_from_slice(b"\x1b[");
                push_num(&mut self.buf, col + 1);
                self.buf.push(b'G');
            }
            _ => {
                self.buf.extend_from_slice(b"\x1b[");
                push_num(&mut self.buf, row + 1);
                self.buf.push(b';');
                push_num(&mut self.buf, col + 1);
                self.buf.push(b'H');
            }
        }
        self.at = Some((col, row));
    }

    fn style(&mut self, style: Style) {
        if style.link != self.pen.link {
            write_link(&mut self.buf, self.host, style.link);
        }
        if (Style { link: 0, ..style })
            != (Style {
                link: 0,
                ..self.pen
            })
        {
            write_sgr_from(&mut self.buf, &self.pen, &style);
        }
        self.pen = style;
    }

    /// A default blank under a pen that is not the default one: erased,
    /// it costs no change of pen. The erasure fills with the background
    /// of the pen, the default one here, as on a host without BCE.
    fn erases(&self, cell: &Cell) -> bool {
        self.pen != Style::default()
            && self.pen.bg == Color::Default
            && !self.pen.attrs.contains(Attrs::INVERSE)
            && *cell == Cell::default()
    }

    /// Erase `n` cells from `(col, row)`, or to the end of the row; the
    /// cursor stays.
    fn erase(&mut self, col: u16, row: u16, n: u16, to_end: bool) {
        self.goto(col, row);
        self.buf.extend_from_slice(b"\x1b[");
        if to_end {
            self.buf.push(b'K');
        } else {
            if n > 1 {
                push_num(&mut self.buf, n);
            }
            self.buf.push(b'X');
        }
    }

    /// Print the cell at `(col, row)`; `col` is not a spacer.
    fn cell(&mut self, col: u16, row: u16, cell: &Cell) {
        self.goto(col, row);
        self.style(cell.style);
        self.buf.extend_from_slice(glyph(cell).as_bytes());
        let next = col + cell.width.max(1) as u16;
        // Past the last column the cursor stays there, wrap pending or not.
        self.at = if next < self.host.cols() {
            Some((next, row))
        } else {
            None
        };
    }
}

/// Make `host.grid()` equal `desired`: paint the cells that differ and
/// restore the cursor, pen, hyperlink, charset, insert and origin modes
/// found on entry. The bytes are appended to `out` and fed to `host`.
/// Nothing is emitted when the grids are equal or of different sizes, or
/// when `host` is inside a sequence.
pub fn repair(host: &mut Vt, desired: &Grid, out: &mut Vec<u8>) {
    let (cols, rows) = (host.cols(), host.rows());
    if desired.cols() != cols
        || desired.rows() != rows
        || !host.in_ground()
        || host.grid() == desired
    {
        return;
    }
    let cursor = host.cursor();
    let entry_pen = host.pen();
    let modes = host.modes();
    let (top, _) = host.margins();
    let last_pos = host.last_pos;

    let mut p = Painter {
        host,
        buf: Vec::with_capacity(256),
        at: None,
        pen: entry_pen,
    };
    if !modes.sync_update {
        p.buf.extend_from_slice(b"\x1b[?2026h");
    }
    if modes.origin {
        p.buf.extend_from_slice(b"\x1b[?6l");
    }
    if modes.insert {
        p.buf.extend_from_slice(b"\x1b[4l");
    }
    if modes.shift_out {
        p.buf.push(0x0f);
    }
    if modes.g0 != Charset::Ascii {
        p.buf.extend_from_slice(b"\x1b(B");
    }
    // Under a pending wrap a glyph would not land on the cursor cell.
    if !modes.origin && !cursor.pending_wrap {
        p.at = Some((cursor.col, cursor.row));
    }

    for row in 0..rows {
        let have = host.grid().row(row);
        let want = desired.row(row);
        if have == want {
            continue;
        }
        let mut col = 0;
        while col < cols {
            let cell = &want[col as usize];
            let w = cell.width.max(1) as u16;
            let end = (col + w).min(cols) as usize;
            if have[col as usize..end] != want[col as usize..end] {
                if p.erases(cell) {
                    // The run of such blanks, up to the last that differs
                    // or to the end of the row.
                    let run = want[col as usize..]
                        .iter()
                        .take_while(|c| **c == Cell::default())
                        .count();
                    let to_end = col as usize + run == cols as usize;
                    let n = (0..run)
                        .rev()
                        .find(|i| have[col as usize + i] != want[col as usize + i])
                        .map_or(1, |i| i + 1);
                    p.erase(col, row, n as u16, to_end);
                    col += if to_end { run } else { n } as u16;
                    continue;
                }
                if cell.width == 0 {
                    // A spacer that differs alone: its head is repainted.
                    if col > 0 && want[col as usize - 1].width == 2 {
                        p.cell(col - 1, row, &want[col as usize - 1]);
                    } else {
                        p.cell(col, row, &Cell::blank(cell.style));
                    }
                } else {
                    p.cell(col, row, cell);
                }
            }
            col += w;
        }
    }

    // Restore, in an order where no step undoes an earlier one.
    if modes.origin {
        p.buf.extend_from_slice(b"\x1b[?6h");
        p.at = None;
    }
    let row_arg = if modes.origin {
        cursor.row.saturating_sub(top)
    } else {
        cursor.row
    };
    // Printing again the glyph the cursor follows restores the cursor, a
    // pending wrap, and what REP and combining marks refer to.
    let reprint = last_pos
        .and_then(|(col, row)| {
            let cell = desired.cell(col, row);
            let end = col + cell.width as u16;
            let lands = row == cursor.row
                && cell.width != 0
                && if end >= cols {
                    end == cols && cursor.col == cols - 1 && cursor.pending_wrap == modes.autowrap
                } else {
                    cursor.col == end && !cursor.pending_wrap
                };
            lands.then_some((col, cell))
        })
        .or_else(|| {
            // The glyph of the last column: the only way back to a pending
            // wrap.
            let last = desired.cell(cols - 1, cursor.row);
            match (cursor.pending_wrap, last.width) {
                (false, _) => None,
                (true, 0) if cols >= 2 => Some((cols - 2, desired.cell(cols - 2, cursor.row))),
                (true, _) => Some((cols - 1, last)),
            }
        });
    if let Some((col, cell)) = reprint {
        let _ = write!(p.buf, "\x1b[{};{}H", row_arg + 1, col + 1);
        p.style(cell.style);
        p.buf.extend_from_slice(glyph(cell).as_bytes());
    } else if p.at != Some((cursor.col, cursor.row)) {
        let _ = write!(p.buf, "\x1b[{};{}H", row_arg + 1, cursor.col + 1);
    }
    p.style(entry_pen);
    if modes.g0 != Charset::Ascii {
        p.buf.extend_from_slice(b"\x1b(0");
    }
    if modes.shift_out {
        p.buf.push(0x0e);
    }
    if modes.insert {
        p.buf.extend_from_slice(b"\x1b[4h");
    }
    if !modes.sync_update {
        p.buf.extend_from_slice(b"\x1b[?2026l");
    }

    let buf = p.buf;
    host.feed(&buf);
    out.extend_from_slice(&buf);
}

/// Rows of `grid` as text with SGR sequences, each ending in a reset.
/// `links`: the `Vt` that owns the hyperlink ids, to emit OSC 8.
pub fn render_ansi(grid: &Grid, links: Option<&Vt>) -> Vec<String> {
    let mut rows = Vec::with_capacity(grid.rows() as usize);
    for row in 0..grid.rows() {
        let mut buf = Vec::new();
        let mut pen = Style::default();
        for cell in grid.row(row).iter().filter(|c| c.width != 0) {
            if let Some(vt) = links
                && cell.style.link != pen.link
            {
                write_link(&mut buf, vt, cell.style.link);
            }
            if (Style {
                link: 0,
                ..cell.style
            }) != (Style { link: 0, ..pen })
            {
                write_sgr(&mut buf, &cell.style);
            }
            pen = cell.style;
            buf.extend_from_slice(glyph(cell).as_bytes());
        }
        if links.is_some() && pen.link != 0 {
            buf.extend_from_slice(b"\x1b]8;;\x1b\\");
        }
        if (Style { link: 0, ..pen }) != Style::default() {
            buf.extend_from_slice(b"\x1b[0m");
        }
        rows.push(String::from_utf8_lossy(&buf).into_owned());
    }
    rows
}
