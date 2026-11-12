// SPDX-License-Identifier: GPL-3.0-or-later
//! Sprite cells: what a content, a frame of animation or an effect draws.

use super::spec::{Content, SceneError, StyleSpec};
use ptyrc_vt::{Attrs, Cell, Color, Style};

/// One sprite cell. Colours absent: inherited from the cell below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Glyph {
    /// Text and width; its style is unused.
    pub cell: Cell,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: Attrs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Transparent.
    Empty,
    Glyph(Glyph),
    /// Right half of the wide glyph before it.
    Spacer,
}

/// Columns a `char` takes: 0 (combining), 1 or 2. A small table; the
/// emulator's is the reference.
pub(crate) fn char_width(c: char) -> u8 {
    let u = c as u32;
    match u {
        0x0300..=0x036F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x200B..=0x200F
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0xE0100..=0xE01EF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// A rectangle of slots. Effects draw on it as their canvas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Canvas {
    w: u16,
    h: u16,
    slots: Vec<Slot>,
}

impl Canvas {
    pub fn new(w: u16, h: u16) -> Canvas {
        Canvas {
            w,
            h,
            slots: vec![Slot::Empty; w as usize * h as usize],
        }
    }

    pub fn width(&self) -> u16 {
        self.w
    }

    pub fn height(&self) -> u16 {
        self.h
    }

    /// Make every cell transparent.
    pub fn clear(&mut self) {
        self.slots.fill(Slot::Empty);
    }

    pub(crate) fn resize(&mut self, w: u16, h: u16) {
        if (w, h) != (self.w, self.h) {
            *self = Canvas::new(w, h);
        }
    }

    pub(crate) fn slot(&self, x: u16, y: u16) -> &Slot {
        &self.slots[y as usize * self.w as usize + x as usize]
    }

    fn set(&mut self, x: u16, y: u16, slot: Slot) {
        let i = y as usize * self.w as usize + x as usize;
        self.slots[i] = slot;
    }

    /// Detach `(x, y)` from a wide pair before it is overwritten.
    fn orphan(&mut self, x: u16, y: u16) {
        match self.slot(x, y) {
            Slot::Spacer if x > 0 => self.set(x - 1, y, Slot::Empty),
            Slot::Glyph(g) if g.cell.width == 2 && x + 1 < self.w => {
                self.set(x + 1, y, Slot::Empty)
            }
            _ => {}
        }
    }

    /// Draw `text` from `(x, y)` to the right. Outside cells are dropped;
    /// a space is drawn as a blank.
    pub fn put(&mut self, x: i32, y: i32, text: &str, style: &StyleSpec) {
        self.draw(x, y, text, style, true);
    }

    fn draw(&mut self, x: i32, y: i32, text: &str, style: &StyleSpec, spaces: bool) -> i32 {
        let mut col = x;
        let mut last: Option<(u16, u16)> = None;
        let inside_row = y >= 0 && y < self.h as i32;
        for ch in text.chars() {
            let width = char_width(ch);
            if width == 0 {
                // Combining mark: joins the glyph before it.
                if let Some((px, py)) = last
                    && let Slot::Glyph(g) = self.slot(px, py).clone()
                {
                    let mut s = g.cell.text.to_string();
                    s.push(ch);
                    let cell = if g.cell.width == 2 {
                        Cell::wide(&s, Style::default())
                    } else {
                        Cell::narrow(&s, Style::default())
                    };
                    self.set(px, py, Slot::Glyph(Glyph { cell, ..g }));
                }
                continue;
            }
            let fits = inside_row && col >= 0 && col + width as i32 <= self.w as i32;
            last = None;
            if fits && (ch != ' ' || spaces) {
                let (cx, cy) = (col as u16, y as u16);
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                self.orphan(cx, cy);
                let cell = if width == 2 {
                    self.orphan(cx + 1, cy);
                    self.set(cx + 1, cy, Slot::Spacer);
                    Cell::wide(s, Style::default())
                } else {
                    Cell::narrow(s, Style::default())
                };
                self.set(
                    cx,
                    cy,
                    Slot::Glyph(Glyph {
                        cell,
                        fg: style.fg.map(|c| c.0),
                        bg: style.bg.map(|c| c.0),
                        attrs: style.attrs(),
                    }),
                );
                last = Some((cx, cy));
            }
            col += width as i32;
        }
        col - x
    }

    /// Width in cells of a string.
    pub fn text_width(text: &str) -> u16 {
        text.chars()
            .map(|c| char_width(c) as u32)
            .sum::<u32>()
            .min(u16::MAX as u32) as u16
    }

    /// Rasterize a content.
    pub(crate) fn from_content(content: &Content) -> Result<Canvas, SceneError> {
        let base = content.style.unwrap_or_default();
        match (&content.rows, &content.cells) {
            (Some(rows), None) => {
                let w = rows
                    .iter()
                    .map(|r| Canvas::text_width(r))
                    .max()
                    .unwrap_or(0);
                let mut canvas = Canvas::new(w, clamp_h(rows.len())?);
                for (y, row) in rows.iter().enumerate() {
                    canvas.draw(0, y as i32, row, &base, content.opaque);
                }
                Ok(canvas)
            }
            (None, Some(cells)) => {
                let width = |runs: &Vec<super::spec::Run>| {
                    runs.iter()
                        .map(|r| Canvas::text_width(&r.t) as u32)
                        .sum::<u32>()
                        .min(u16::MAX as u32) as u16
                };
                let w = cells.iter().map(width).max().unwrap_or(0);
                let mut canvas = Canvas::new(w, clamp_h(cells.len())?);
                for (y, runs) in cells.iter().enumerate() {
                    let mut x = 0;
                    for run in runs {
                        let style = merge(&base, &run.style);
                        let spaces = content.opaque || run.style.bg.is_some();
                        x += canvas.draw(x, y as i32, &run.t, &style, spaces);
                    }
                }
                Ok(canvas)
            }
            (Some(_), Some(_)) => Err(SceneError::BadParams(
                "content: rows and cells are exclusive".into(),
            )),
            (None, None) => Err(SceneError::BadParams(
                "content: rows or cells required".into(),
            )),
        }
    }
}

fn clamp_h(n: usize) -> Result<u16, SceneError> {
    u16::try_from(n).map_err(|_| SceneError::BadParams("content: too many rows".into()))
}

/// `over` on top of `base`: colours of `over` win when set, flags add.
fn merge(base: &StyleSpec, over: &StyleSpec) -> StyleSpec {
    StyleSpec {
        fg: over.fg.or(base.fg),
        bg: over.bg.or(base.bg),
        bold: base.bold || over.bold,
        dim: base.dim || over.dim,
        italic: base.italic || over.italic,
        underline: base.underline || over.underline,
        inverse: base.inverse || over.inverse,
        strike: base.strike || over.strike,
        blink: base.blink || over.blink,
    }
}
