// SPDX-License-Identifier: GPL-3.0-or-later
//! Segmentation of the screen into panes: separators, bars, boxes,
//! guillotine cuts.

use super::screen::{Line, Screen, class};
use crate::types::FrameKind;
use ptyrc_vt::Rect;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Leaf {
    pub rect: Rect4,
    pub kind: u8, // 0 pane, 1 chrome
}

/// `Rect` in an orderable form: `(y, x, h, w)`.
pub(crate) type Rect4 = (u16, u16, u16, u16);

pub(crate) fn r4(r: Rect) -> Rect4 {
    (r.y, r.x, r.h, r.w)
}

pub(crate) fn rect_of(r: Rect4) -> Rect {
    Rect::new(r.1, r.0, r.3, r.2)
}

impl Leaf {
    pub fn rect(&self) -> Rect {
        rect_of(self.rect)
    }

    pub fn kind(&self) -> FrameKind {
        if self.kind == 0 {
            FrameKind::Pane
        } else {
            FrameKind::Chrome
        }
    }
}

/// A horizontal separator: row `pos`, columns `[a, b)`, found while
/// cutting `parent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sep {
    pub pos: u16,
    pub a: u16,
    pub b: u16,
    pub bar: bool,
    pub parent: Rect,
}

/// Separators of a kind are not believed inside `area` until `until`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Distrust {
    pub bar: bool,
    pub area: Rect,
    pub until: Duration,
}

pub(crate) struct Segmenter<'a> {
    pub screen: &'a Screen,
    pub cursor_row: Option<u16>,
    pub min_rows: u16,
    pub min_cols: u16,
    pub distrust: &'a [Distrust],
    pub now: Duration,
    pub leaves: Vec<Leaf>,
    pub seps: Vec<Sep>,
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

impl Segmenter<'_> {
    pub fn run(mut self) -> (Vec<Leaf>, Vec<Sep>) {
        let all = Rect::new(0, 0, self.screen.cols, self.screen.rows);
        self.split(all, 0);
        self.leaves.sort();
        self.leaves.dedup();
        (self.leaves, self.seps)
    }

    fn cls(&self, col: u16, row: u16) -> u16 {
        self.screen.class[self.screen.idx(col, row)]
    }

    fn paint(&self, col: u16, row: u16) -> u16 {
        self.screen.paint[self.screen.idx(col, row)]
    }

    fn distrusted(&self, bar: bool, r: &Rect) -> bool {
        self.distrust
            .iter()
            .any(|d| d.bar == bar && d.until > self.now && contains(&d.area, r))
    }

    /// Share of filled cells and dominant paint of a row span.
    fn row_fill(&self, r: &Rect, row: u16) -> (u16, u16) {
        let (mut filled, mut key, mut votes) = (0u16, 0u16, 0i32);
        for c in r.x..r.right() {
            let p = self.paint(c, row);
            if p != 0 {
                filled += 1;
                if votes == 0 {
                    key = p;
                    votes = 1;
                } else if key == p {
                    votes += 1;
                } else {
                    votes -= 1;
                }
            }
        }
        (filled, key)
    }

    fn is_bar_row(&self, r: &Rect, row: u16) -> Option<u16> {
        let (filled, key) = self.row_fill(r, row);
        (filled as u32 * 10 >= r.w as u32 * 9).then_some(key)
    }

    /// Kind of horizontal separator at `row` of `r`: `Some(bar)`.
    fn hsep(&self, r: &Rect, row: u16) -> Option<bool> {
        if r.w >= 4 && !self.distrusted(false, r) {
            let (mut strong, mut dash) = (0u32, 0u32);
            for c in r.x..r.right() {
                let k = self.cls(c, row);
                strong += (k & class::H != 0) as u32;
                dash += (k & class::DASH != 0) as u32;
            }
            let ends =
                self.cls(r.x, row) & class::H != 0 && self.cls(r.right() - 1, row) & class::H != 0;
            if (ends && strong * 10 >= r.w as u32 * 7) || (dash == r.w as u32 && r.w >= 8) {
                return Some(false);
            }
        }
        if r.h >= 2
            && Some(row) != self.cursor_row
            && !self.distrusted(true, r)
            && let Some(key) = self.is_bar_row(r, row)
        {
            let same = |other: u16| self.is_bar_row(r, other) == Some(key);
            let above = row > r.y && same(row - 1);
            let below = row + 1 < r.bottom() && same(row + 1);
            if !above && !below {
                return Some(true);
            }
        }
        None
    }

    fn vsep(&self, r: &Rect, col: u16) -> bool {
        if r.h < 2 {
            return false;
        }
        let (mut strong, mut pipe, mut soft, mut filled) = (0u32, 0u32, 0u32, 0u32);
        for row in r.y..r.bottom() {
            let k = self.cls(col, row);
            strong += (k & class::V != 0) as u32;
            pipe += (k & class::PIPE != 0) as u32;
            soft += (k & (class::PIPE | class::BLANK) != 0) as u32;
            filled += (self.paint(col, row) != 0) as u32;
        }
        let h = r.h as u32;
        if strong * 10 >= h * 9 {
            return true;
        }
        if pipe == h && h >= 3 {
            return true;
        }
        // A filled column of blanks or pipes between unfilled neighbours.
        if soft == h && filled == h && h >= 3 {
            let col_filled = |c: u16| {
                (r.y..r.bottom())
                    .filter(|&row| self.paint(c, row) != 0)
                    .count() as u32
                    * 10
                    >= h * 9
            };
            let left = col > r.x && col_filled(col - 1);
            let right = col + 1 < r.right() && col_filled(col + 1);
            return !left && !right;
        }
        false
    }

    fn leaf(&mut self, r: Rect) {
        if r.is_empty() {
            return;
        }
        let kind = if r.h >= self.min_rows && r.w >= self.min_cols {
            0
        } else {
            1
        };
        self.leaves.push(Leaf { rect: r4(r), kind });
    }

    fn split(&mut self, r: Rect, depth: u8) {
        if r.is_empty() {
            return;
        }
        if depth > 24 {
            self.leaf(r);
            return;
        }
        // Horizontal cuts.
        let cuts: Vec<(u16, bool)> = (r.y..r.bottom())
            .filter_map(|row| self.hsep(&r, row).map(|bar| (row, bar)))
            .collect();
        if !cuts.is_empty() && cuts.len() < r.h as usize {
            let mut top = r.y;
            for (row, bar) in cuts {
                self.seps.push(Sep {
                    pos: row,
                    a: r.x,
                    b: r.right(),
                    bar,
                    parent: r,
                });
                if row > top {
                    self.split(Rect::new(r.x, top, r.w, row - top), depth + 1);
                }
                if bar {
                    self.leaves.push(Leaf {
                        rect: r4(Rect::new(r.x, row, r.w, 1)),
                        kind: 1,
                    });
                }
                top = row + 1;
            }
            if r.bottom() > top {
                self.split(Rect::new(r.x, top, r.w, r.bottom() - top), depth + 1);
            }
            return;
        }
        // Vertical cuts.
        let cuts: Vec<u16> = (r.x..r.right()).filter(|&c| self.vsep(&r, c)).collect();
        if !cuts.is_empty() && cuts.len() < r.w as usize {
            let mut left = r.x;
            for col in cuts {
                if col > left {
                    self.split(Rect::new(left, r.y, col - left, r.h), depth + 1);
                }
                left = col + 1;
            }
            if r.right() > left {
                self.split(Rect::new(left, r.y, r.right() - left, r.h), depth + 1);
            }
            return;
        }
        // Boxes floating inside.
        for b in self.boxes(&r) {
            self.split(Rect::new(b.x + 1, b.y + 1, b.w - 2, b.h - 2), depth + 1);
        }
        self.leaf(r);
    }

    /// Closed boxes of border glyphs inside `r`, outermost first, disjoint.
    fn boxes(&self, r: &Rect) -> Vec<Rect> {
        let mut found: Vec<Rect> = Vec::new();
        if r.w < 4 || r.h < 3 {
            return found;
        }
        for y in r.y..r.bottom() - 2 {
            for x in r.x..r.right() - 3 {
                if self.cls(x, y) & class::TL == 0 || found.iter().any(|b| b.contains(x, y)) {
                    continue;
                }
                // Top edge.
                let mut x2 = x + 1;
                while x2 < r.right()
                    && self.cls(x2, y) & class::H != 0
                    && self.cls(x2, y) & class::TR == 0
                {
                    x2 += 1;
                }
                if x2 >= r.right() || self.cls(x2, y) & class::TR == 0 || x2 - x < 3 {
                    continue;
                }
                // Left edge.
                let mut y2 = y + 1;
                while y2 < r.bottom()
                    && self.cls(x, y2) & class::V != 0
                    && self.cls(x, y2) & class::BL == 0
                {
                    y2 += 1;
                }
                if y2 >= r.bottom() || self.cls(x, y2) & class::BL == 0 || y2 - y < 2 {
                    continue;
                }
                if self.cls(x2, y2) & class::BR == 0 {
                    continue;
                }
                let right_ok = (y + 1..y2).all(|row| self.cls(x2, row) & class::V != 0);
                let bottom = (x + 1..x2)
                    .filter(|&c| self.cls(c, y2) & class::H != 0)
                    .count();
                if !right_ok || bottom * 10 < (x2 - x - 1) as usize * 7 {
                    continue;
                }
                let b = Rect::new(x, y, x2 - x + 1, y2 - y + 1);
                if b == *r {
                    continue;
                }
                found.push(b);
            }
        }
        found
    }
}

/// Separators that moved with their surroundings between two
/// segmentations: content, not structure. `prev` and `next` are the row
/// texts before and after.
pub(crate) fn scrolled_seps(
    old: &[Sep],
    new: &[Sep],
    prev: &dyn Fn(u16) -> Line,
    next: &dyn Fn(u16) -> Line,
) -> Vec<Sep> {
    let mut out = Vec::new();
    for o in old {
        if new
            .iter()
            .any(|n| n.pos == o.pos && n.a == o.a && n.b == o.b && n.bar == o.bar)
        {
            continue;
        }
        for n in new
            .iter()
            .filter(|n| n.a == o.a && n.b == o.b && n.bar == o.bar && n.pos != o.pos)
        {
            if old
                .iter()
                .any(|x| x.pos == n.pos && x.a == n.a && x.b == n.b && x.bar == n.bar)
            {
                continue;
            }
            let d = n.pos as i32 - o.pos as i32;
            let mut agree = 0;
            for j in [-3i32, -2, -1, 1, 2, 3] {
                let (p, q) = (o.pos as i32 + j, o.pos as i32 + j + d);
                let (top, bottom) = (o.parent.y as i32, o.parent.bottom() as i32);
                if p < top || p >= bottom || q < top || q >= bottom {
                    continue;
                }
                let a = Screen::hash_of(&prev(p as u16), o.a, o.b);
                let b = Screen::hash_of(&next(q as u16), o.a, o.b);
                if a != super::screen::BLANK && a == b {
                    agree += 1;
                }
            }
            if agree >= 2 {
                out.push(*o);
                break;
            }
        }
    }
    out
}
