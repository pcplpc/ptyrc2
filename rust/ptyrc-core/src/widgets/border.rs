// SPDX-License-Identifier: GPL-3.0-or-later
//! Borders of widgets: presets, the wire form, and the composition of a
//! widget's cells (border, title, content).

use crate::scene::{ColorSpec, Content, Run, StyleSpec};
use ptyrc_vt::{Color, char_width};
use serde::{Deserialize, Serialize};

/// Where the title sits on the top edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// The title on the top edge: `before`, the text, `after`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleSpec {
    pub align: Align,
    pub before: String,
    pub after: String,
    pub style: StyleSpec,
}

impl Default for TitleSpec {
    fn default() -> TitleSpec {
        TitleSpec {
            align: Align::Left,
            before: " ".into(),
            after: " ".into(),
            style: StyleSpec::default(),
        }
    }
}

/// A border, one cell thick. `corners`: four glyphs, top-left,
/// top-right, bottom-left, bottom-right. An edge is a pattern repeated
/// from the top or the left. All empty: no border, no title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorderSpec {
    pub corners: String,
    pub top: String,
    pub bottom: String,
    pub left: String,
    pub right: String,
    pub style: StyleSpec,
    pub title: TitleSpec,
}

impl Default for BorderSpec {
    /// `plain`.
    fn default() -> BorderSpec {
        BorderSpec::lines("┌┐└┘", "─", "│")
    }
}

impl BorderSpec {
    /// Corners, one glyph along both horizontal edges, one along both
    /// vertical edges; dim.
    pub fn lines(corners: &str, horizontal: &str, vertical: &str) -> BorderSpec {
        BorderSpec {
            corners: corners.into(),
            top: horizontal.into(),
            bottom: horizontal.into(),
            left: vertical.into(),
            right: vertical.into(),
            style: StyleSpec {
                dim: true,
                ..StyleSpec::default()
            },
            title: TitleSpec::default(),
        }
    }

    /// No border: the outer size is the inner one.
    pub fn none() -> BorderSpec {
        BorderSpec {
            corners: String::new(),
            top: String::new(),
            bottom: String::new(),
            left: String::new(),
            right: String::new(),
            style: StyleSpec::default(),
            title: TitleSpec::default(),
        }
    }

    pub fn is_none(&self) -> bool {
        self.corners.is_empty()
            && self.top.is_empty()
            && self.bottom.is_empty()
            && self.left.is_empty()
            && self.right.is_empty()
    }

    /// Cells the border adds to each side.
    pub fn thickness(&self) -> u16 {
        if self.is_none() { 0 } else { 1 }
    }

    /// Four corners and four edges of glyphs of one cell, or nothing.
    pub fn check(&self) -> Result<(), String> {
        if self.is_none() {
            return Ok(());
        }
        let single = |what: &str, s: &str| -> Result<(), String> {
            match s.chars().find(|c| c.is_control() || char_width(*c) != 1) {
                Some(c) => Err(format!("border {what}: {c:?} is not a glyph of one cell")),
                None => Ok(()),
            }
        };
        if self.corners.chars().count() != 4 {
            return Err(format!(
                "border corners: four glyphs, not {:?}",
                self.corners
            ));
        }
        single("corners", &self.corners)?;
        for (what, edge) in [
            ("top", &self.top),
            ("bottom", &self.bottom),
            ("left", &self.left),
            ("right", &self.right),
        ] {
            if edge.is_empty() {
                return Err(format!("border {what}: a pattern of one glyph or more"));
            }
            single(what, edge)?;
        }
        Ok(())
    }
}

/// The core presets, `plain` first.
pub fn presets() -> Vec<(&'static str, BorderSpec)> {
    vec![
        ("plain", BorderSpec::default()),
        ("round", BorderSpec::lines("╭╮╰╯", "─", "│")),
        ("heavy", BorderSpec::lines("┏┓┗┛", "━", "┃")),
        ("double", BorderSpec::lines("╔╗╚╝", "═", "║")),
        ("ascii", BorderSpec::lines("++++", "-", "|")),
        ("none", BorderSpec::none()),
    ]
}

/// The title members of a border object; absent ones are the base's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitlePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
}

/// A border object on the wire: the preset `base` (`plain`) with the
/// members given replaced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BorderPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePatch>,
}

impl BorderPatch {
    /// `base` with the members given.
    pub fn apply(&self, base: &BorderSpec) -> BorderSpec {
        let mut b = base.clone();
        let set = |dst: &mut String, src: &Option<String>| {
            if let Some(s) = src {
                *dst = s.clone();
            }
        };
        set(&mut b.corners, &self.corners);
        set(&mut b.top, &self.top);
        set(&mut b.bottom, &self.bottom);
        set(&mut b.left, &self.left);
        set(&mut b.right, &self.right);
        if let Some(style) = self.style {
            b.style = style;
        }
        if let Some(t) = &self.title {
            if let Some(align) = t.align {
                b.title.align = align;
            }
            set(&mut b.title.before, &t.before);
            set(&mut b.title.after, &t.after);
            if let Some(style) = t.style {
                b.title.style = style;
            }
        }
        b
    }
}

/// `border` on the wire: a preset name or an object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Border {
    Name(String),
    Object(BorderPatch),
}

/// One cell of a widget: its text (one glyph, a wide one taking two
/// columns) and style.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot {
    text: String,
    width: u8,
    style: StyleSpec,
}

impl Slot {
    fn blank(style: StyleSpec) -> Slot {
        Slot {
            text: " ".into(),
            width: 1,
            style,
        }
    }
}

/// `over` on `base`: colours of `over` win when set, flags add.
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

/// Lay `text` from column `x` of `row`, at most up to column `end`
/// (excluded). A wide glyph that does not fit is a blank; a combining
/// mark joins the glyph before it; a control character is a blank.
/// Returns the column after the last glyph.
fn lay(row: &mut [Option<Slot>], mut x: usize, end: usize, text: &str, style: StyleSpec) -> usize {
    let end = end.min(row.len());
    let mut last: Option<usize> = None;
    for ch in text.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        let w = char_width(ch);
        if w == 0 {
            if let Some(Some(slot)) = last.map(|i| &mut row[i]) {
                slot.text.push(ch);
            }
            continue;
        }
        if x >= end {
            break;
        }
        if x + w > end {
            row[x] = Some(Slot::blank(style));
            x += 1;
            last = None;
            continue;
        }
        row[x] = Some(Slot {
            text: ch.to_string(),
            width: w as u8,
            style,
        });
        if w == 2 {
            row[x + 1] = None;
        }
        last = Some(x);
        x += w;
    }
    x
}

/// Width in cells of `text`.
fn width(text: &str) -> usize {
    text.chars()
        .map(|c| if c.is_control() { 1 } else { char_width(c) })
        .sum()
}

/// `text` cut to `room` cells, `…` last when it is cut.
fn cut(text: &str, room: usize) -> String {
    if width(text) <= room {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = if ch.is_control() { 1 } else { char_width(ch) };
        if used + w > room.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// The inner cells of a content, `w x h`: clipped, padded with blanks.
fn inner(content: Option<&Content>, w: u16, h: u16) -> Vec<Vec<Option<Slot>>> {
    let (w, h) = (w as usize, h as usize);
    let base = content.and_then(|c| c.style).unwrap_or_default();
    let mut rows: Vec<Vec<Option<Slot>>> = Vec::with_capacity(h);
    for y in 0..h {
        let mut row: Vec<Option<Slot>> = (0..w).map(|_| Some(Slot::blank(base))).collect();
        if let Some(c) = content {
            if let Some(text) = c.rows.as_ref().and_then(|r| r.get(y)) {
                lay(&mut row, 0, w, text, base);
            } else if let Some(runs) = c.cells.as_ref().and_then(|r| r.get(y)) {
                let mut x = 0;
                for run in runs {
                    x = lay(&mut row, x, w, &run.t, merge(&base, &run.style));
                }
            }
        }
        rows.push(row);
    }
    rows
}

/// A widget as one opaque `cells` content: the border, the title on its
/// top edge, the content of inner size `w x h`. A cell without a
/// background is given the default one: nothing behind shows through.
pub fn compose(
    border: &BorderSpec,
    title: &str,
    content: Option<&Content>,
    w: u16,
    h: u16,
) -> Content {
    let t = border.thickness() as usize;
    let (iw, ih) = (w as usize, h as usize);
    let (ow, oh) = (iw + 2 * t, ih + 2 * t);
    let mut grid: Vec<Vec<Option<Slot>>> = Vec::with_capacity(oh);
    let body = inner(content, w, h);
    if t == 0 {
        grid = body;
    } else {
        let style = border.style;
        let corners: Vec<char> = border.corners.chars().collect();
        let glyph = |c: char| {
            Some(Slot {
                text: c.to_string(),
                width: 1,
                style,
            })
        };
        let edge = |pattern: &str, i: usize| {
            let p: Vec<char> = pattern.chars().collect();
            glyph(p[i % p.len()])
        };
        let mut top: Vec<Option<Slot>> = Vec::with_capacity(ow);
        top.push(glyph(corners[0]));
        top.extend((0..iw).map(|i| edge(&border.top, i)));
        top.push(glyph(corners[1]));
        title_on(&mut top, border, title);
        grid.push(top);
        for (y, row) in body.into_iter().enumerate() {
            let mut line = Vec::with_capacity(ow);
            line.push(edge(&border.left, y));
            line.extend(row);
            line.push(edge(&border.right, y));
            grid.push(line);
        }
        let mut bottom: Vec<Option<Slot>> = Vec::with_capacity(ow);
        bottom.push(glyph(corners[2]));
        bottom.extend((0..iw).map(|i| edge(&border.bottom, i)));
        bottom.push(glyph(corners[3]));
        grid.push(bottom);
    }
    let cells = grid
        .into_iter()
        .map(|row| {
            let mut runs: Vec<Run> = Vec::new();
            for slot in row.into_iter().flatten() {
                let mut style = slot.style;
                if style.bg.is_none() {
                    style.bg = Some(ColorSpec(Color::Default));
                }
                match runs.last_mut() {
                    Some(run) if run.style == style => run.t.push_str(&slot.text),
                    _ => runs.push(Run {
                        t: slot.text,
                        style,
                    }),
                }
            }
            runs
        })
        .collect();
    Content {
        cells: Some(cells),
        opaque: true,
        ..Content::default()
    }
}

/// Put the title on the top edge `row` (corners included), between the
/// corners: cut with `…`, absent under 3 cells of room.
fn title_on(row: &mut [Option<Slot>], border: &BorderSpec, title: &str) {
    if title.is_empty() {
        return;
    }
    let spec = &border.title;
    let span = row.len().saturating_sub(2);
    let room = span.saturating_sub(width(&spec.before) + width(&spec.after));
    if room < 3 {
        return;
    }
    let text = cut(title, room);
    let total = width(&spec.before) + width(&text) + width(&spec.after);
    let x = 1 + match spec.align {
        Align::Left => 0,
        Align::Center => (span - total) / 2,
        Align::Right => span - total,
    };
    let style = spec.style;
    let end = 1 + span;
    let x = lay(row, x, end, &spec.before, border.style);
    let x = lay(row, x, end, &text, style);
    lay(row, x, end, &spec.after, border.style);
}
