// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for ranger (Miller columns).
//!
//! Recognised from the title row (`user@host /path/entry`) above a status
//! row (`drwxr-xr-x ... 3/42  Top`) or a console (`:`). The columns have
//! no border and no backdrop: they are read from the selected entries,
//! each shown in inverse video over the width of its column, one blank
//! cell between two columns.

use ptyrc_core::plugin::{Chrome, PaneView, TuiSupport, ViewHint};
use ptyrc_vt::{Attrs, Grid, Rect};
use regex::Regex;
use std::sync::LazyLock;

static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\S+@\S+ ([/~]\S*)").unwrap());
static PERMS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-dlcbps][-rwxsStT]{9} ").unwrap());
static POSITION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)/(\d+)\s+(All|Top|Bot|\d+%)\s*$").unwrap());

/// Narrowest run of inverse cells taken for a selected entry.
const MIN_RUN: u16 = 3;

pub struct Ranger;

/// What an area of the screen shows of ranger.
struct Look {
    /// The area: title row, columns, status row.
    area: Rect,
    /// `[x0, x1)` of each column, left to right.
    columns: Vec<(u16, u16)>,
    /// Index of the selected entry, 0-based, from the status row.
    selected: Option<i64>,
    /// Path on the title row, as shown (it may be abbreviated).
    path: String,
    /// The last row is the status row, not a console.
    status: bool,
}

fn text(grid: &Grid, area: Rect, y: u16) -> String {
    (area.x..area.right())
        .map(|x| grid.cell(x, y))
        .filter(|c| c.width != 0)
        .map(|c| c.text.as_str())
        .collect()
}

fn inverse(grid: &Grid, x: u16, y: u16) -> bool {
    grid.cell(x, y).style.attrs.contains(Attrs::INVERSE)
}

/// Runs of inverse cells on a row, `[x0, x1)`.
fn runs(grid: &Grid, area: Rect, y: u16) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut start = None;
    for x in area.x..=area.right() {
        let on = x < area.right() && inverse(grid, x, y);
        match (start, on) {
            (None, true) => start = Some(x),
            (Some(s), false) => {
                if x - s >= MIN_RUN {
                    out.push((s, x));
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// Columns from the selected entries of the rows `[y0, y1)`. A column
/// without one (an empty parent, the preview of a file) takes what is
/// left between its neighbours.
fn columns(grid: &Grid, area: Rect, y0: u16, y1: u16) -> Vec<(u16, u16)> {
    let mut found: Vec<(u16, u16)> = (y0..y1).flat_map(|y| runs(grid, area, y)).collect();
    found.sort();
    found.dedup();
    let mut out = Vec::new();
    let mut at = area.x;
    for (x0, x1) in found {
        if x0 < at {
            continue; // inside a column already told
        }
        if x0 > at + 1 {
            out.push((at, x0 - 1));
        }
        out.push((x0, x1));
        at = x1 + 1;
    }
    if at < area.right() {
        out.push((at, area.right()));
    }
    out
}

impl Look {
    fn of(grid: &Grid, area: Rect) -> Option<Look> {
        if area.h < 4 || area.w < 12 || area.bottom() > grid.rows() || area.right() > grid.cols() {
            return None;
        }
        let title = text(grid, area, area.y);
        let path = TITLE.captures(&title)?[1].to_string();
        let last = text(grid, area, area.bottom() - 1);
        let position = POSITION.captures(&last);
        let status = PERMS.is_match(&last) || position.is_some();
        if !status && !last.starts_with([':', '/', '?', '!']) {
            return None;
        }
        let selected = position
            .and_then(|c| c[1].parse::<i64>().ok())
            .filter(|n| *n >= 1)
            .map(|n| n - 1);
        let columns = columns(grid, area, area.y + 1, area.bottom() - 1);
        Some(Look {
            area,
            columns,
            selected,
            path,
            status,
        })
    }

    fn rect(&self, column: usize) -> Rect {
        let (x0, x1) = self.columns[column];
        Rect::new(x0, self.area.y + 1, x1 - x0, self.area.h - 2)
    }

    /// The column of the current directory: the preview, when shown, is
    /// on its right.
    fn main(&self) -> usize {
        match self.columns.len() {
            0 | 1 => 0,
            // The parent and the current directory.
            2 => 1,
            n => n - 2,
        }
    }

    /// The directory shown by the main column.
    fn dir(&self) -> &str {
        match self.path.rfind('/') {
            Some(0) | None => "/",
            Some(i) => &self.path[..i],
        }
    }
}

/// What a pane is: ranger whole, or one of its columns.
enum Part {
    Whole,
    Column(usize),
}

fn part(pane: &PaneView) -> Option<(Look, Part)> {
    if let Some(look) = Look::of(pane.grid, pane.rect) {
        return Some((look, Part::Whole));
    }
    let look = Look::of(pane.grid, pane.grid.rect())?;
    let i = (0..look.columns.len()).find(|i| look.rect(*i) == pane.rect)?;
    Some((look, Part::Column(i)))
}

impl TuiSupport for Ranger {
    fn name(&self) -> &'static str {
        "ranger"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        match part(pane) {
            Some((look, _)) if look.status => 0.9,
            Some(_) => 0.6,
            None => 0.0,
        }
    }

    /// Not split (inside another app's pane): the title and status rows.
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        match part(pane)?.1 {
            Part::Whole => Some(Chrome {
                top: 1,
                bottom: 1,
                ..Chrome::default()
            }),
            Part::Column(_) => None,
        }
    }

    /// Main column: the index of the selected entry less its row.
    fn view_hint(&self, pane: &PaneView) -> Option<ViewHint> {
        let (look, Part::Column(i)) = part(pane)? else {
            return None;
        };
        if i != look.main() {
            return None;
        }
        let rect = look.rect(i);
        let row = (rect.y..rect.bottom()).find(|y| inverse(pane.grid, rect.x, *y))?;
        Some(ViewHint {
            row: Some(look.selected? - (row - rect.y) as i64),
            col: None,
        })
    }

    /// The directory of the main column.
    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        let (look, part) = part(pane)?;
        match part {
            Part::Whole if pane.rect == pane.grid.rect() => None,
            Part::Whole => Some(format!("ranger:{}", look.dir())),
            Part::Column(i) if i == look.main() => Some(format!("ranger:{}", look.dir())),
            Part::Column(_) => None,
        }
    }

    /// Title row, columns, status row.
    fn segment(&self, screen: &PaneView) -> Option<Vec<Rect>> {
        let look = Look::of(screen.grid, screen.rect)?;
        if look.columns.len() < 2 {
            return None;
        }
        let a = look.area;
        let mut rects = vec![
            Rect::new(a.x, a.y, a.w, 1),
            Rect::new(a.x, a.bottom() - 1, a.w, 1),
        ];
        rects.extend((0..look.columns.len()).map(|i| look.rect(i)));
        Some(rects)
    }
}
