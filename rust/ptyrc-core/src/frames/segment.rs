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

/// Fewest word edges two rows share to be rows of one table.
const ENTRY_EDGES: u32 = 6;

pub(crate) struct Segmenter<'a> {
    pub screen: &'a Screen,
    pub cursor_row: Option<u16>,
    pub min_rows: u16,
    pub min_cols: u16,
    pub distrust: &'a [Distrust],
    pub now: Duration,
    pub leaves: Vec<Leaf>,
    pub seps: Vec<Sep>,
    /// Asked of every rectangle reached below the whole screen: the
    /// panes a preset makes of it, final.
    pub ask: Option<&'a dyn Fn(Rect) -> Option<Vec<Leaf>>>,
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

    /// Dominant paint of a row span and the number of cells that show it.
    fn row_fill(&self, r: &Rect, row: u16) -> (u16, u16) {
        let (mut key, mut votes) = (0u16, 0i32);
        for c in r.x..r.right() {
            let p = self.paint(c, row);
            if votes == 0 {
                key = p;
                votes = 1;
            } else if key == p {
                votes += 1;
            } else {
                votes -= 1;
            }
        }
        let count = (r.x..r.right())
            .filter(|&c| self.paint(c, row) == key)
            .count() as u16;
        (count, key)
    }

    /// The backdrop of a bar row: one colour, not the default, across the
    /// span.
    fn is_bar_row(&self, r: &Rect, row: u16) -> Option<u16> {
        let (count, key) = self.row_fill(r, row);
        (key != 0 && count as u32 * 10 >= r.w as u32 * 9).then_some(key)
    }

    /// Is a row of one backdrop an entry of a table: its words start and
    /// end in the columns of a row next to it that is no bar, digits
    /// under digits, letters under letters. The highlight of a list, told
    /// before it moves; a header of letters over figures is not one.
    fn entry(&self, r: &Rect, row: u16) -> bool {
        fn class(c: char) -> char {
            if c.is_ascii_digit() {
                '0'
            } else if c.is_alphabetic() {
                'a'
            } else {
                c
            }
        }
        // Per cell: the class of a glyph that starts a word, and of one
        // that ends a word.
        let edges = |y: u16| -> Vec<(Option<char>, Option<char>)> {
            let text: Vec<char> = self.screen.lines[y as usize]
                .text
                .chars()
                .chain(std::iter::repeat(' '))
                .skip(r.x as usize)
                .take(r.w as usize)
                .collect();
            (0..text.len())
                .map(|i| {
                    let ink = |j: usize| text.get(j).is_some_and(|c| *c != ' ');
                    let c = ink(i).then(|| class(text[i]));
                    (
                        c.filter(|_| i == 0 || !ink(i - 1)),
                        c.filter(|_| !ink(i + 1)),
                    )
                })
                .collect()
        };
        let mine = edges(row);
        let count = |e: &[(Option<char>, Option<char>)]| {
            e.iter()
                .map(|(a, b)| a.is_some() as u32 + b.is_some() as u32)
                .sum::<u32>()
        };
        let alike = |other: u16| {
            if self.is_bar_row(r, other).is_some() {
                return false;
            }
            let theirs = edges(other);
            let same: u32 = mine
                .iter()
                .zip(&theirs)
                .map(|(a, b)| {
                    (a.0.is_some() && a.0 == b.0) as u32 + (a.1.is_some() && a.1 == b.1) as u32
                })
                .sum();
            same >= ENTRY_EDGES && same * 10 >= count(&mine).min(count(&theirs)) * 6
        };
        (row > r.y && alike(row - 1)) || (row + 1 < r.bottom() && alike(row + 1))
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
            && !self.distrusted(true, &Rect::new(r.x, row, r.w, 1))
            && let Some(key) = self.is_bar_row(r, row)
        {
            let same = |other: u16| self.is_bar_row(r, other) == Some(key);
            let above = row > r.y && same(row - 1);
            let below = row + 1 < r.bottom() && same(row + 1);
            if !above && !below && !self.entry(r, row) {
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
        // A rule from top to bottom inside the rectangle parts two apps:
        // it cuts before a preset is asked (an app left of another
        // looks like the whole to its preset).
        let parted = r.w > 2 && (r.x + 1..r.right() - 1).any(|c| self.vsep(&r, c));
        if depth > 0
            && !parted
            && let Some(ask) = self.ask
            && let Some(leaves) = ask(r)
        {
            self.leaves.extend(leaves);
            return;
        }
        // Horizontal cuts. Rules first, alone: what lies between two
        // rules is one app's, bars included, and is asked of the presets.
        let mut cuts: Vec<(u16, bool)> = (r.y..r.bottom())
            .filter_map(|row| self.hsep(&r, row).map(|bar| (row, bar)))
            .collect();
        if self.ask.is_some() && cuts.iter().any(|c| !c.1) && cuts.len() < r.h as usize {
            cuts.retain(|c| !c.1);
        }
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
        let boxes = self.boxes(&r);
        for b in &boxes {
            self.split(Rect::new(b.x + 1, b.y + 1, b.w - 2, b.h - 2), depth + 1);
        }
        // Panels floating inside: no border, another backdrop.
        for p in self.panels(&r) {
            if !boxes.iter().any(|b| b.intersects(&p)) {
                self.split(p, depth + 1);
            }
        }
        self.leaf(r);
    }

    /// The most frequent paint of `r`: its backdrop.
    fn backdrop(&self, r: &Rect) -> u16 {
        let mut counts: Vec<(u16, u32)> = Vec::new();
        for row in r.y..r.bottom() {
            let mut c = r.x;
            while c < r.right() {
                let p = self.paint(c, row);
                let mut e = c + 1;
                while e < r.right() && self.paint(e, row) == p {
                    e += 1;
                }
                match counts.iter_mut().find(|(k, _)| *k == p) {
                    Some((_, n)) => *n += (e - c) as u32,
                    None => counts.push((p, (e - c) as u32)),
                }
                c = e;
            }
        }
        counts
            .into_iter()
            .max_by_key(|&(k, n)| (n, std::cmp::Reverse(k)))
            .map_or(0, |(k, _)| k)
    }

    /// Panels inside `r`: rectangles of one backdrop that is not `r`'s,
    /// at least a pane in size, narrower than `r`. A row belongs to a
    /// panel when it shows the panel's colour at both edges and nothing of
    /// `r`'s backdrop between them: a highlighted entry inside stays in.
    /// Largest first, disjoint.
    fn panels(&self, r: &Rect) -> Vec<Rect> {
        let mut found: Vec<Rect> = Vec::new();
        if r.w <= self.min_cols || r.h < self.min_rows {
            return found;
        }
        let back = self.backdrop(r);
        // Open spans `(x0, x1, key, first row)`, extended row by row.
        let mut open: Vec<(u16, u16, u16, u16)> = Vec::new();
        let mut done: Vec<Rect> = Vec::new();
        for row in r.y..=r.bottom() {
            let mut spans: Vec<(u16, u16, u16)> = Vec::new();
            if row < r.bottom() {
                let mut c = r.x;
                while c < r.right() {
                    let key = self.paint(c, row);
                    if key == back || (c > r.x && self.paint(c - 1, row) == key) {
                        c += 1;
                        continue;
                    }
                    // As far as `back` does not show; the span ends at the
                    // last cell of `key` in there.
                    let (mut e, mut last) = (c, c);
                    while e < r.right() && self.paint(e, row) != back {
                        if self.paint(e, row) == key {
                            last = e;
                        }
                        e += 1;
                    }
                    let x1 = last + 1;
                    if x1 - c >= self.min_cols && x1 - c < r.w {
                        spans.push((c, x1, key));
                    }
                    c += 1;
                }
            }
            open.retain(|&(x0, x1, key, first)| {
                if spans.contains(&(x0, x1, key)) {
                    return true;
                }
                if row - first >= self.min_rows {
                    done.push(Rect::new(x0, first, x1 - x0, row - first));
                }
                false
            });
            for (x0, x1, key) in spans {
                if !open.iter().any(|o| (o.0, o.1, o.2) == (x0, x1, key)) {
                    open.push((x0, x1, key, row));
                }
            }
        }
        done.sort_by_key(|p| std::cmp::Reverse(p.w as u32 * p.h as u32));
        for p in done {
            if !found.iter().any(|f| f.intersects(&p)) {
                found.push(p);
            }
        }
        found
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

/// Separators that are content, not structure, from two segmentations.
/// `prev` and `next` are the row texts before and after. The `parent` of
/// each is the area where separators of its kind are not to be believed.
///
/// - A separator that moved with its surroundings: it scrolled.
/// - A bar that moved without its text: the highlight of a list (a
///   selected process, a menu entry), on another entry now. Its area is
///   the list: the rows between the nearest separators that did not move.
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
            // A status line takes its text along; a highlight lands on
            // another entry.
            let was = Screen::hash_of(&prev(o.pos), o.a, o.b);
            let is = Screen::hash_of(&next(n.pos), o.a, o.b);
            if o.bar && o.parent == n.parent && is != super::screen::BLANK && was != is {
                let (lo, hi) = (o.pos.min(n.pos), o.pos.max(n.pos));
                let fixed = |s: &&Sep| {
                    s.a == o.a
                        && s.b == o.b
                        && old
                            .iter()
                            .any(|x| x.pos == s.pos && x.a == s.a && x.b == s.b && x.bar == s.bar)
                };
                let top = new
                    .iter()
                    .filter(fixed)
                    .filter(|s| s.pos < lo)
                    .map(|s| s.pos + 1)
                    .max()
                    .unwrap_or(o.parent.y);
                let bottom = new
                    .iter()
                    .filter(fixed)
                    .filter(|s| s.pos > hi)
                    .map(|s| s.pos)
                    .min()
                    .unwrap_or(o.parent.bottom());
                out.push(Sep {
                    parent: Rect::new(o.a, top, o.b - o.a, bottom - top),
                    ..*o
                });
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::{Cell, Color, Grid, Style};

    const BACK: u8 = 235;
    const PANEL: u8 = 238;
    const PICK: u8 = 60;

    fn fill(grid: &mut Grid, r: Rect, bg: u8) {
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                grid.cell_mut(x, y).style.bg = Color::Indexed(bg);
            }
        }
    }

    fn text(grid: &mut Grid, x: u16, y: u16, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            let style: Style = grid.cell(x + i as u16, y).style;
            grid.put(x + i as u16, y, Cell::narrow(&ch.to_string(), style));
        }
    }

    fn leaves(grid: &Grid, distrust: &[Distrust]) -> (Vec<(Rect, u8)>, Vec<Sep>) {
        let mut screen = Screen::default();
        screen.scan(grid);
        let (leaves, seps) = Segmenter {
            screen: &screen,
            cursor_row: None,
            min_rows: 3,
            min_cols: 8,
            distrust,
            now: Duration::ZERO,
            leaves: Vec::new(),
            seps: Vec::new(),
            ask: None,
        }
        .run();
        (leaves.iter().map(|l| (l.rect(), l.kind)).collect(), seps)
    }

    /// A screen painted whole, as themed TUIs do, with a modal panel and
    /// its highlighted entry.
    fn modal(pick: u16) -> Grid {
        let mut g = Grid::new(60, 20);
        fill(&mut g, Rect::new(0, 0, 60, 20), BACK);
        fill(&mut g, Rect::new(15, 4, 30, 10), PANEL);
        fill(&mut g, Rect::new(16, pick, 28, 1), PICK);
        for y in 5..13 {
            text(&mut g, 17, y, &format!("entry {y}"));
        }
        g
    }

    #[test]
    fn a_painted_screen_has_no_bar() {
        let mut g = Grid::new(60, 20);
        fill(&mut g, Rect::new(0, 0, 60, 20), BACK);
        text(&mut g, 2, 3, "hello");
        let (l, seps) = leaves(&g, &[]);
        assert_eq!(l, vec![(Rect::new(0, 0, 60, 20), 0)]);
        assert!(seps.is_empty());
        // A status line of another colour is one.
        fill(&mut g, Rect::new(0, 19, 60, 1), PICK);
        let (l, _) = leaves(&g, &[]);
        assert_eq!(
            l,
            vec![(Rect::new(0, 0, 60, 19), 0), (Rect::new(0, 19, 60, 1), 1)]
        );
    }

    #[test]
    fn a_row_of_mixed_backdrops_is_not_a_bar() {
        // Half the row in one colour, half in another: the entry of a
        // panel, seen from the screen.
        let (l, seps) = leaves(&modal(7), &[]);
        assert!(seps.iter().all(|s| s.parent != Rect::new(0, 0, 60, 20)));
        assert!(l.contains(&(Rect::new(0, 0, 60, 20), 0)), "{l:?}");
    }

    #[test]
    fn a_panel_is_a_pane() {
        let mut g = Grid::new(60, 20);
        fill(&mut g, Rect::new(0, 0, 60, 20), BACK);
        fill(&mut g, Rect::new(15, 4, 30, 10), PANEL);
        text(&mut g, 17, 6, "inside");
        let (l, _) = leaves(&g, &[]);
        assert_eq!(
            l,
            vec![(Rect::new(0, 0, 60, 20), 0), (Rect::new(15, 4, 30, 10), 0)]
        );
        // A sidebar: the full height, not the full width.
        let mut g = Grid::new(60, 20);
        fill(&mut g, Rect::new(40, 0, 20, 20), PANEL);
        let (l, _) = leaves(&g, &[]);
        assert!(l.contains(&(Rect::new(40, 0, 20, 20), 0)), "{l:?}");
        // Too small, or as wide as the screen: not a panel.
        let mut g = Grid::new(60, 20);
        fill(&mut g, Rect::new(10, 2, 5, 10), PANEL);
        fill(&mut g, Rect::new(30, 2, 20, 2), PANEL);
        fill(&mut g, Rect::new(0, 8, 60, 4), PANEL);
        let (l, _) = leaves(&g, &[]);
        assert_eq!(l, vec![(Rect::new(0, 0, 60, 20), 0)]);
    }

    #[test]
    fn a_highlight_that_moves_is_not_a_bar() {
        let panel = Rect::new(15, 4, 30, 10);
        // First seen, the entry cuts the panel.
        let (l, old) = leaves(&modal(7), &[]);
        assert!(l.contains(&(Rect::new(15, 7, 30, 1), 1)), "{l:?}");
        // It moves and the text does not, even when the entries change
        // in the same burst (a table re-sorted at each key): a list.
        let (a, mut b) = (modal(7), modal(9));
        text(&mut b, 17, 7, "ENTRY");
        text(&mut b, 17, 9, "ENTRY");
        let (_, new) = leaves(&b, &[]);
        let line = |g: &Grid, r: u16| Line::from_cells(g.row(r));
        let moved = scrolled_seps(&old, &new, &|r| line(&a, r), &|r| line(&b, r));
        assert_eq!(moved.len(), 1);
        assert!(moved[0].bar);
        assert_eq!(moved[0].parent, panel);
        let distrust = [Distrust {
            bar: true,
            area: moved[0].parent,
            until: Duration::from_secs(1),
        }];
        let (l, seps) = leaves(&b, &distrust);
        assert_eq!(l, vec![(Rect::new(0, 0, 60, 20), 0), (panel, 0)]);
        assert!(seps.is_empty());
    }

    #[test]
    fn an_entry_of_a_table_is_not_a_bar() {
        // A header and, under it, the selected row of a table.
        let table = |g: &mut Grid| {
            fill(g, Rect::new(0, 4, 60, 1), PANEL);
            text(g, 0, 4, "    PID USER       CPU%  MEM%  Command");
            for (i, pid) in [101, 2202, 33, 4404, 55].iter().enumerate() {
                let row = format!("{pid:>7} user{i}      {i}.3   0.{i}  /usr/bin/cmd{i}");
                text(g, 0, 5 + i as u16, &row);
            }
        };
        let mut g = Grid::new(60, 20);
        table(&mut g);
        fill(&mut g, Rect::new(0, 5, 60, 1), PICK);
        table(&mut g);
        let (l, _) = leaves(&g, &[]);
        assert_eq!(
            l,
            vec![
                (Rect::new(0, 0, 60, 4), 0),
                (Rect::new(0, 4, 60, 1), 1),
                (Rect::new(0, 5, 60, 15), 0)
            ]
        );
        // A status line over text of another shape is a bar.
        let mut g = Grid::new(60, 20);
        text(&mut g, 0, 9, "fn main() { let total = 12345; }");
        fill(&mut g, Rect::new(0, 10, 60, 1), PICK);
        text(&mut g, 0, 10, "src/main.rs                    12,1   Top");
        text(&mut g, 0, 11, ":wq");
        let (l, _) = leaves(&g, &[]);
        assert!(l.contains(&(Rect::new(0, 10, 60, 1), 1)), "{l:?}");
    }

    #[test]
    fn a_bar_that_takes_its_text_along_is_still_a_bar() {
        // A status line that jumps from the bottom to the top.
        let screen = |row: u16| {
            let mut g = Grid::new(60, 20);
            fill(&mut g, Rect::new(0, row, 60, 1), PICK);
            text(&mut g, 0, row, "status");
            g
        };
        let (a, b) = (screen(19), screen(0));
        let (_, old) = leaves(&a, &[]);
        let (_, new) = leaves(&b, &[]);
        let line = |g: &Grid, r: u16| Line::from_cells(g.row(r));
        assert!(scrolled_seps(&old, &new, &|r| line(&a, r), &|r| line(&b, r)).is_empty());
    }
}
