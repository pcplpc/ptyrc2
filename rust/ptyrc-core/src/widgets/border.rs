// SPDX-License-Identifier: GPL-3.0-or-later
//! Borders of widgets: presets, the wire form, and the composition of a
//! widget's cells (border, title, content).

use crate::scene::{ColorSpec, Content, Run, StyleSpec};
use ptyrc_vt::{Color, char_width};
use serde::{Deserialize, Serialize};

/// Where the title sits along its edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    #[default]
    Right,
}

/// The edge the title is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    #[default]
    Bottom,
}

/// The title on an edge: `before`, the text, `after`. Bottom right by
/// default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleSpec {
    pub side: Side,
    pub align: Align,
    pub before: String,
    pub after: String,
    pub style: StyleSpec,
}

impl Default for TitleSpec {
    fn default() -> TitleSpec {
        TitleSpec {
            side: Side::Bottom,
            align: Align::Right,
            before: " ".into(),
            after: " ".into(),
            style: StyleSpec::default(),
        }
    }
}

fn chase_len() -> u16 {
    3
}

fn chase_ms() -> u32 {
    80
}

fn chase_runs() -> u16 {
    1
}

/// Runs of border cells in another style that go round the border,
/// clockwise: `runs` of them, evenly spaced, `len` cells each, a cell
/// every `ms`. `len` 0: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaseSpec {
    #[serde(default = "chase_len")]
    pub len: u16,
    #[serde(default = "chase_ms")]
    pub ms: u32,
    #[serde(default = "chase_runs")]
    pub runs: u16,
    #[serde(default)]
    pub style: StyleSpec,
}

impl ChaseSpec {
    /// The step at `now`: the border cell the first run has reached,
    /// counted along the border without end.
    pub fn step(&self, now: std::time::Duration) -> u64 {
        now.as_millis() as u64 / self.ms.max(1) as u64
    }

    /// When the step after that of `now` starts.
    pub fn next(&self, now: std::time::Duration) -> std::time::Duration {
        std::time::Duration::from_millis((self.step(now) + 1) * self.ms.max(1) as u64)
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chase: Option<ChaseSpec>,
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
            chase: None,
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
            chase: None,
        }
    }

    /// `alert`: heavy, bright yellow, bold, a white chase; made to be
    /// seen.
    pub fn alert() -> BorderSpec {
        let bright = |n: u8| StyleSpec {
            fg: Some(ColorSpec(Color::Indexed(n))),
            bold: true,
            ..StyleSpec::default()
        };
        BorderSpec {
            style: bright(11),
            title: TitleSpec {
                style: bright(11),
                ..TitleSpec::default()
            },
            chase: Some(ChaseSpec {
                len: 4,
                ms: 60,
                runs: 2,
                style: bright(15),
            }),
            ..BorderSpec::lines("┏┓┗┛", "━", "┃")
        }
    }

    /// The chase, when one shows.
    pub fn chasing(&self) -> Option<&ChaseSpec> {
        self.chase
            .as_ref()
            .filter(|c| c.len > 0 && c.runs > 0 && !self.is_none())
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
        if let Some(c) = &self.chase
            && (c.ms < 20 || c.ms > 10_000 || c.len > 64 || c.runs > 16)
        {
            return Err("border chase: ms 20 to 10000, len 64 at most, runs 16 at most".into());
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
        ("alert", BorderSpec::alert()),
        ("none", BorderSpec::none()),
    ]
}

/// The title members of a border object; absent ones are the base's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitlePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chase: Option<ChaseSpec>,
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
        if let Some(chase) = self.chase {
            b.chase = Some(chase);
        }
        if let Some(t) = &self.title {
            if let Some(side) = t.side {
                b.title.side = side;
            }
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
    Object(Box<BorderPatch>),
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

/// A widget as one opaque `cells` content: the border, the title on
/// one of its edges, the content of inner size `w x h`. A cell without a
/// background is given the default one: nothing behind shows through.
pub fn compose(
    border: &BorderSpec,
    title: &str,
    content: Option<&Content>,
    w: u16,
    h: u16,
) -> Content {
    compose_at(border, title, content, w, h, None, [0; 4])
}

/// The cell of a border of outer size `ow x oh` at `i` along it,
/// clockwise from the top-left corner.
fn along(i: usize, ow: usize, oh: usize) -> (usize, usize) {
    let (a, b) = (ow - 1, oh - 1);
    if i < a {
        (i, 0)
    } else if i < a + b {
        (a, i - a)
    } else if i < 2 * a + b {
        (a - (i - a - b), b)
    } else {
        (0, b - (i - 2 * a - b))
    }
}

/// `compose`, with the chase of the border at `step` (see `ChaseSpec`)
/// and a margin of opaque blanks around it: cells on the left, top,
/// right, bottom.
pub fn compose_at(
    border: &BorderSpec,
    title: &str,
    content: Option<&Content>,
    w: u16,
    h: u16,
    step: Option<u64>,
    margin: [u16; 4],
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
        // The chase goes under the title.
        let around = 2 * (ow - 1) + 2 * (oh - 1);
        if let (Some(chase), Some(step)) = (border.chasing(), step)
            && around > 0
        {
            let runs = (chase.runs as usize).min(around);
            for run in 0..runs {
                let head = (step as usize % around) + run * around / runs;
                for k in 0..(chase.len as usize).min(around) {
                    let (x, y) = along((head + around - k) % around, ow, oh);
                    if let Some(slot) = &mut grid[y][x] {
                        slot.style = chase.style;
                    }
                }
            }
        }
        let edge = match border.title.side {
            Side::Top => 0,
            Side::Bottom => oh - 1,
        };
        title_on(&mut grid[edge], border, title);
    }
    let [ml, mt, mr, mb] = margin.map(|m| m as usize);
    if ml + mt + mr + mb > 0 {
        let blank = || Some(Slot::blank(StyleSpec::default()));
        let wide = ml + ow + mr;
        let mut padded: Vec<Vec<Option<Slot>>> = Vec::with_capacity(mt + oh + mb);
        padded.extend((0..mt).map(|_| (0..wide).map(|_| blank()).collect()));
        for row in grid {
            let mut line: Vec<Option<Slot>> = (0..ml).map(|_| blank()).collect();
            line.extend(row);
            line.extend((0..mr).map(|_| blank()));
            padded.push(line);
        }
        padded.extend((0..mb).map(|_| (0..wide).map(|_| blank()).collect()));
        grid = padded;
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

/// Put the title on the edge `row` (corners included), between the
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
