// SPDX-License-Identifier: GPL-3.0-or-later
//! ABI 1: names, the canvas cell. See `README.md`.

use serde_json::{Map, Value, json};

/// The ABI version.
pub const ABI: u32 = 1;
/// The only module a guest imports from.
pub const MODULE: &str = "ptyrc";
/// Bytes of a canvas cell.
pub const CELL: usize = 16;
/// Shortest tick period, ms (30 Hz).
pub const MIN_PERIOD_MS: u32 = 33;

const KIND_NONE: u8 = 0;
const KIND_DEFAULT: u8 = 1;
const KIND_PALETTE: u8 = 2;
const KIND_RGB: u8 = 3;

const ATTRS: [(u16, &str); 7] = [
    (1, "bold"),
    (2, "dim"),
    (4, "italic"),
    (8, "underline"),
    (16, "inverse"),
    (32, "strike"),
    (64, "blink"),
];

/// A colour of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// That of the cell below.
    None,
    /// The terminal's default.
    Default,
    Palette(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    fn read(b: &[u8]) -> Color {
        match b[0] {
            KIND_DEFAULT => Color::Default,
            KIND_PALETTE => Color::Palette(b[1]),
            KIND_RGB => Color::Rgb(b[1], b[2], b[3]),
            _ => Color::None,
        }
    }

    /// The four bytes of the cell.
    pub fn bytes(self) -> [u8; 4] {
        match self {
            Color::None => [KIND_NONE, 0, 0, 0],
            Color::Default => [KIND_DEFAULT, 0, 0, 0],
            Color::Palette(i) => [KIND_PALETTE, i, 0, 0],
            Color::Rgb(r, g, b) => [KIND_RGB, r, g, b],
        }
    }

    fn wire(self) -> Option<Value> {
        match self {
            Color::None => None,
            Color::Default => Some(json!("default")),
            Color::Palette(i) => Some(json!(i)),
            Color::Rgb(r, g, b) => Some(json!(format!("#{r:02x}{g:02x}{b:02x}"))),
        }
    }
}

/// A decoded cell. `ch` is `None` where nothing is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: Option<char>,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u16,
}

impl Cell {
    /// Decode 16 bytes. A value that is not a scalar, or a control
    /// character, reads as a space.
    pub fn read(b: &[u8]) -> Cell {
        let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let ch = match v {
            0 => None,
            v => Some(char::from_u32(v).filter(|c| !c.is_control()).unwrap_or(' ')),
        };
        Cell {
            ch,
            fg: Color::read(&b[4..8]),
            bg: Color::read(&b[8..12]),
            attrs: u16::from_le_bytes([b[12], b[13]]) & 0x7f,
        }
    }

    /// Encode, as a guest would.
    pub fn bytes(self) -> [u8; CELL] {
        let mut b = [0u8; CELL];
        b[0..4].copy_from_slice(&(self.ch.map_or(0, |c| c as u32)).to_le_bytes());
        b[4..8].copy_from_slice(&self.fg.bytes());
        b[8..12].copy_from_slice(&self.bg.bytes());
        b[12..14].copy_from_slice(&self.attrs.to_le_bytes());
        b
    }

    fn style(self) -> (Color, Color, u16) {
        (self.fg, self.bg, self.attrs)
    }
}

/// One row of a canvas as runs of the wire (`content.cells`).
/// A cell with nothing drawn is a space without `bg`: a hole. The cell
/// after a wide glyph is skipped. A glyph of no width reads as a space.
pub fn row_runs(row: &[u8]) -> Value {
    let mut runs: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut style = (Color::None, Color::None, 0u16);
    let mut skip = false;
    let flush = |text: &mut String, style: (Color, Color, u16), runs: &mut Vec<Value>| {
        if text.is_empty() {
            return;
        }
        let mut run = Map::new();
        run.insert("t".into(), Value::String(std::mem::take(text)));
        if let Some(c) = style.0.wire() {
            run.insert("fg".into(), c);
        }
        if let Some(c) = style.1.wire() {
            run.insert("bg".into(), c);
        }
        for (bit, name) in ATTRS {
            if style.2 & bit != 0 {
                run.insert(name.into(), Value::Bool(true));
            }
        }
        runs.push(Value::Object(run));
    };
    for raw in row.chunks_exact(CELL) {
        if skip {
            skip = false;
            continue;
        }
        let cell = Cell::read(raw);
        let (ch, st) = match cell.ch {
            None => (' ', (Color::None, Color::None, 0)),
            Some(c) => match ptyrc_vt::char_width(c) {
                0 => (' ', cell.style()),
                1 => (c, cell.style()),
                _ => {
                    skip = true;
                    (c, cell.style())
                }
            },
        };
        if st != style {
            flush(&mut text, style, &mut runs);
            style = st;
        }
        text.push(ch);
    }
    // A wide glyph in the last column does not fit.
    if skip {
        text.pop();
        text.push(' ');
    }
    // Trailing holes carry nothing.
    if style == (Color::None, Color::None, 0) {
        let kept = text.trim_end_matches(' ').len();
        text.truncate(kept);
    }
    flush(&mut text, style, &mut runs);
    Value::Array(runs)
}

/// One row as plain text, holes as spaces; for the headless screen.
pub fn row_text(row: &[u8]) -> Vec<Option<char>> {
    let mut out = Vec::with_capacity(row.len() / CELL);
    let mut skip = false;
    for raw in row.chunks_exact(CELL) {
        if skip {
            skip = false;
            out.push(None);
            continue;
        }
        match Cell::read(raw).ch {
            None => out.push(None),
            Some(c) => match ptyrc_vt::char_width(c) {
                0 => out.push(Some(' ')),
                1 => out.push(Some(c)),
                _ => {
                    skip = true;
                    out.push(Some(c));
                }
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(c: char, fg: Color) -> [u8; CELL] {
        Cell {
            ch: Some(c),
            fg,
            bg: Color::None,
            attrs: 0,
        }
        .bytes()
    }

    #[test]
    fn runs() {
        let mut row = Vec::new();
        row.extend(cell('a', Color::Palette(1)));
        row.extend(cell('b', Color::Palette(1)));
        row.extend([0u8; CELL]);
        row.extend(cell('世', Color::Rgb(255, 0, 16)));
        row.extend(cell('x', Color::None));
        row.extend([0u8; CELL]);
        assert_eq!(
            row_runs(&row),
            json!([{"t": "ab", "fg": 1}, {"t": " "}, {"t": "世", "fg": "#ff0010"}])
        );
    }

    #[test]
    fn control_is_a_space() {
        let mut raw = [0u8; CELL];
        raw[0] = 0x1b;
        assert_eq!(Cell::read(&raw).ch, Some(' '));
        raw[0..4].copy_from_slice(&0xD800u32.to_le_bytes());
        assert_eq!(Cell::read(&raw).ch, Some(' '));
        raw[0..4].copy_from_slice(&0x9bu32.to_le_bytes());
        assert_eq!(Cell::read(&raw).ch, Some(' '));
    }
}
