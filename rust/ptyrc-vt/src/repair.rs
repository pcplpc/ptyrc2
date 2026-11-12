// SPDX-License-Identifier: GPL-3.0-or-later
//! The repair painter: the bytes that turn what the host shows into a
//! desired grid, leaving the terminal state as found.

use crate::grid::{Cell, Grid, Style};
use crate::sgr::write_sgr;
use crate::vt::{Charset, Vt};
use std::io::Write;
use unicode_width::UnicodeWidthChar;

/// Glyph of a cell as it can be printed: its text when the emulator would
/// store it back in one cell of that width (a base character of the cell's
/// width, then zero-width ones), spaces otherwise.
fn glyph(cell: &Cell) -> &str {
    let text = cell.text.as_str();
    let mut chars = text.chars();
    let ok = match chars.next() {
        Some(c) if !c.is_control() => {
            let w = if c.is_ascii() {
                1
            } else {
                c.width().unwrap_or(0)
            };
            w == cell.width as usize
                && text.len() <= 32
                && chars.all(|c| !c.is_control() && c.width() == Some(0))
        }
        _ => false,
    };
    if ok {
        text
    } else if cell.width == 2 {
        "  "
    } else {
        " "
    }
}

fn write_link(out: &mut Vec<u8>, vt: &Vt, id: u32) {
    match vt.link(id) {
        Some(link) => {
            let _ = write!(out, "\x1b]8;{};{}\x1b\\", link.params, link.uri);
        }
        None => out.extend_from_slice(b"\x1b]8;;\x1b\\"),
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
                let _ = write!(self.buf, "\x1b[{}C", col - c);
            }
            Some((_, r)) if r == row => {
                let _ = write!(self.buf, "\x1b[{}G", col + 1);
            }
            _ => {
                let _ = write!(self.buf, "\x1b[{};{}H", row + 1, col + 1);
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
            write_sgr(&mut self.buf, &style);
        }
        self.pen = style;
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
/// Nothing is emitted when the grids are equal, or of different sizes.
pub fn repair(host: &mut Vt, desired: &Grid, out: &mut Vec<u8>) {
    let (cols, rows) = (host.cols(), host.rows());
    if desired.cols() != cols || desired.rows() != rows || host.grid() == desired {
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
