// SPDX-License-Identifier: GPL-3.0-or-later
//! Frames: raw, extended, dynamic; heatmap, pool, placement.
//!
//! `Frames` is fed the screen once per burst (`update`) and the time
//! between bursts (`tick`). It holds no clock.

mod ext;
mod heat;
mod screen;
mod segment;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_heat;

pub use heat::{HeatClass, HeatView, IDLE_SECS, IDLE_SHARE, Idle};

use crate::plugin::{Chrome, PaneView, TuiSupport, Tuning, ViewHint};
use crate::types::*;
use ext::{Ext, Obs, Outcome, Row};
use heat::Heat;
use ptyrc_vt::{Cursor, Grid, Rect, VtEvent};
use screen::{Line, Screen, fnv};
use segment::{Distrust, Leaf, Segmenter, Sep, r4, rect_of, scrolled_seps};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

/// Tunables. Times in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameConfig {
    /// A candidate pane is stable after this long without structural change.
    pub settle_ms: u32,
    /// A view row is remembered after this long unchanged.
    pub commit_ms: u32,
    /// A new segmentation replaces the layout after this long; until then
    /// the panes it touches are only suspended.
    pub flap_ms: u32,
    /// The raw frame is usable this long after a resize or a screen switch.
    pub raw_settle_ms: u32,
    pub min_pane_rows: u16,
    pub min_pane_cols: u16,
    /// A placed sprite moves when another spot is better by
    /// `replace_margin` for this long.
    pub replace_ms: u32,
    pub replace_margin: f32,
    /// Remembered lines per frame.
    pub max_lines: usize,
    /// Frames kept off screen.
    pub max_inactive: usize,
    /// Cursor samples kept.
    pub trail: usize,
    /// A kind of separator seen scrolling is ignored this long.
    pub distrust_ms: u32,
    /// Share of view rows found in a remembered frame to attach it again.
    pub identity: f32,
}

impl Default for FrameConfig {
    fn default() -> FrameConfig {
        FrameConfig {
            settle_ms: 1500,
            commit_ms: 150,
            flap_ms: 100,
            raw_settle_ms: 250,
            min_pane_rows: 3,
            min_pane_cols: 8,
            replace_ms: 2000,
            replace_margin: 0.05,
            max_lines: 50_000,
            max_inactive: 64,
            trail: 256,
            distrust_ms: 300_000,
            identity: 0.5,
        }
    }
}

/// The screen at the end of a burst.
#[derive(Clone, Copy, Debug)]
pub struct ScreenInput<'a> {
    pub grid: &'a Grid,
    pub cursor: Cursor,
    pub alt_screen: bool,
    pub title: &'a str,
    pub events: &'a [VtEvent],
}

/// What changed, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FrameEvent {
    /// The set of panes changed.
    Layout {
        layout: u32,
    },
    /// A new frame, candidate.
    Added {
        frame: FrameId,
    },
    /// A remembered frame is on screen again, candidate.
    Attached {
        frame: FrameId,
    },
    Stable {
        frame: FrameId,
    },
    /// Left the screen; remembered.
    Inactive {
        frame: FrameId,
    },
    /// Forgotten.
    Dropped {
        frame: FrameId,
    },
    /// The origin moved: the view shows other rows or columns.
    Scroll {
        frame: FrameId,
        dx: i64,
        dy: i64,
    },
    /// The frame rows `[from, to]` moved by `dy` (splice, island merge,
    /// renumbering). Drawings fixed on those rows move with them.
    Shift {
        frame: FrameId,
        from: i64,
        to: i64,
        dy: i64,
    },
    /// The frame rows `[from, to]` are forgotten (an island collected).
    /// Drawings fixed on those rows go with them.
    Forgotten {
        frame: FrameId,
        from: i64,
        to: i64,
    },
    /// The view went elsewhere in the frame.
    Jump {
        frame: FrameId,
    },
    /// The content rect changed (chrome learned).
    Rect {
        frame: FrameId,
    },
}

/// A frame, as on the wire.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrameInfo {
    pub id: FrameId,
    pub kind: FrameKind,
    pub state: FrameState,
    pub usable: bool,
    /// `[x, y, w, h]` on screen.
    pub rect: [u16; 4],
    /// Frame position of the top-left cell of `rect`.
    pub origin: [i64; 2],
    /// `[y_min, y_max]` of the known rows.
    pub extent: [i64; 2],
    pub key: Option<String>,
    pub support: Option<String>,
    /// Share of the view static for `IDLE_SECS`, `0..=1`.
    pub idle: f32,
    /// `idle >= IDLE_SHARE`.
    pub is_idle: bool,
}

struct Pane {
    id: FrameId,
    kind: FrameKind,
    state: FrameState,
    since: Duration,
    rect: Rect,
    ext: Ext,
    heat: Heat,
    key: Option<String>,
    support: Option<usize>,
    support_at: Option<Duration>,
    /// The screen was cleared at this time: what is fed right after is a
    /// repaint, not a scroll.
    cleared: Option<Duration>,
    /// Last answer of `detect`, and when it was asked.
    found: Option<usize>,
    found_at: Option<Duration>,
    suspended: bool,
    /// New or attached again: not observed yet.
    fresh: bool,
    reattach: bool,
    last_active: Duration,
    /// Last time the cursor was seen inside, while on screen.
    cursor_seen: Duration,
    /// Born on the alternate screen.
    alt: bool,
    /// Replacements in a row over whole-pane changes that led to this
    /// frame, and the time of the last.
    churn: Option<(Duration, u8)>,
    /// Repaints itself: every change is an edit of the rows in view,
    /// until one that is neither in place nor of the same shape.
    monitor: bool,
    /// Its preset says so: `monitor` without a way out.
    declared: bool,
    /// Pane rows `[a, b)` under a smaller pane of the last segmentation
    /// (a palette, a modal): not read while they are.
    covered: Vec<(u16, u16)>,
    /// Feeds that follow one another closely (`FEED_GAP`): when the last
    /// came, and their sum.
    fed: Option<(Duration, i64)>,
    /// The pane was repainted in place around a cursor shown on this
    /// cell, at this time, this long after the repaint before; nothing
    /// moved since.
    repaint_cursor: Option<((u16, u16), Duration, Option<Duration>)>,
    left: u16,
    right: u16,
    gutter_cand: (u16, u8),
    gutter_row: Option<i64>,
    /// Frame row of the first content row, read on a status row.
    status_row: Option<i64>,
    /// The status row does not count screen rows: not read any more.
    status_bad: bool,
    /// Left chrome a plugin asked for.
    plugin_left: u16,
    fixed_tb: Option<(u16, u16)>,
    hint: Option<crate::plugin::ViewHint>,
    hashes: Vec<u64>,
    hashes_valid: bool,
    settle: Duration,
    commit: Duration,
}

impl Pane {
    fn content(&self) -> Rect {
        let (mut t, mut b) = (self.ext.top, self.ext.bottom);
        if t + b >= self.rect.h {
            (t, b) = (0, 0);
        }
        let (mut l, mut r) = (self.left, self.right);
        if l + r >= self.rect.w {
            (l, r) = (0, 0);
        }
        Rect::new(
            self.rect.x + l,
            self.rect.y + t,
            self.rect.w - l - r,
            self.rect.h - t - b,
        )
    }

    fn top(&self) -> u16 {
        self.content().y - self.rect.y
    }

    fn origin(&self) -> FramePos {
        FramePos {
            x: self.ext.ox,
            y: self.ext.oy + self.top() as i64,
        }
    }
}

/// What a preset said of a pane.
#[derive(Clone, Default)]
struct Advice {
    support: Option<usize>,
    chrome: Option<Chrome>,
    hint: Option<ViewHint>,
    key: Option<String>,
    tuning: Tuning,
}

struct Placed {
    pos: (u16, u16),
    better_since: Option<Duration>,
    used: Duration,
}

/// Spots handed out, by `(frame, w, h, prefer)`.
type PlaceCache = HashMap<(FrameId, u16, u16, u8), Vec<Placed>>;

/// Pane of a preset, by rectangle: the area the preset segmented.
type Areas = HashMap<segment::Rect4, segment::Rect4>;

/// How long a rectangle no longer reached keeps its presets.
const NESTED_KEEP: Duration = Duration::from_secs(10);

/// Every frame of a screen.
pub struct Frames {
    cfg: FrameConfig,
    supports: Vec<Box<dyn TuiSupport>>,
    screen: Screen,
    now: Duration,
    raw_heat: Heat,
    raw_ready: Duration,
    panes: BTreeMap<FrameId, Pane>,
    active: Vec<FrameId>,
    layout: Vec<Leaf>,
    layout_id: u32,
    layouts: Vec<Vec<Leaf>>,
    pending: Option<(Vec<Leaf>, Duration)>,
    seg: Option<Vec<Leaf>>,
    seg_cursor: Option<u16>,
    seps: Vec<Sep>,
    distrust: Vec<Distrust>,
    cursor: Cursor,
    trail: VecDeque<CursorSample>,
    next_id: FrameId,
    title_hash: u64,
    alt: bool,
    started: bool,
    /// Identity of what fills the whole screen, from a plugin: prefixes
    /// the key of every pane.
    screen_key: Option<String>,
    /// Confidence of every preset on the whole screen, and when asked.
    screen_conf: Vec<f32>,
    screen_conf_at: Option<Duration>,
    title_changed: bool,
    /// Preset answers for the leaves of the pending layout.
    advice: HashMap<segment::Rect4, Advice>,
    /// Panes made by a preset in the last segmentation, and the area it
    /// segmented.
    areas: Areas,
    /// Presets found on the rectangles the segmentation reached, and when
    /// asked.
    nested: RefCell<HashMap<segment::Rect4, (Duration, Vec<usize>)>>,
    placed: RefCell<PlaceCache>,
}

/// Replacements closer than this are in a row.
const CHURN: Duration = Duration::from_secs(3);

/// A preset is looked for again on a change of one row only this long
/// after the last time. A change of two rows or more, of the title or of
/// the layout asks at once.
const DETECT_EVERY: Duration = Duration::from_secs(1);

/// A repaint that follows a clear may come in several bursts, this long.
const REPAINT: Duration = Duration::from_millis(100);

/// A pane replaced whole is the same one repainted when, on average over
/// its rows, this share of the cells kept their glyph and this share kept
/// their class. Two views of a listing pass too: see `repainted`'s caller.
const REPAINT_SAME: f32 = 0.3;
const REPAINT_ALIKE: f32 = 0.7;

/// How long a preset outlives its last detection.
const SUPPORT_HOLD: Duration = Duration::from_secs(4);

/// A glyph that takes two cells, by its block: enough to size a match.
fn is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD)
}

/// `line N` or `lines N-M` in a status row: `N - 1`.
fn status_line(text: &str) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(k) = lower[from..].find("line") {
        let at = from + k;
        from = at + 4;
        if at > 0 && bytes[at - 1].is_ascii_alphanumeric() {
            continue;
        }
        let mut i = at + 4;
        if bytes.get(i) == Some(&b's') {
            i += 1;
        }
        let digits = i + bytes[i..].iter().take_while(|b| **b == b' ').count();
        if digits == i {
            continue;
        }
        let end = digits
            + bytes[digits..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        if end == digits || end - digits > 9 || bytes.get(end).is_some_and(u8::is_ascii_alphabetic)
        {
            continue;
        }
        let n: i64 = lower[digits..end].parse().ok()?;
        return (n >= 1).then_some(n - 1);
    }
    None
}

/// Is the new content of a pane the old one repainted: the same shape
/// with other values, as a monitor refreshes a table or a meter. Row by
/// row, cell by cell: the share of equal glyphs, and of glyphs of the same
/// class (digit, letter, or the same sign).
fn repainted(old: &[Row], new: &dyn Fn(usize) -> Line) -> bool {
    fn class(c: char) -> char {
        if c.is_ascii_digit() {
            '0'
        } else if c.is_alphabetic() {
            'a'
        } else {
            c
        }
    }
    let (mut rows, mut same, mut alike) = (0usize, 0f32, 0f32);
    for (i, row) in old.iter().enumerate() {
        let was = row.line.text.trim_end_matches(' ');
        let line = new(i);
        let is = line.text.trim_end_matches(' ');
        if was.is_empty() && is.is_empty() {
            continue;
        }
        rows += 1;
        let (mut n, mut eq, mut cl) = (0usize, 0usize, 0usize);
        let (mut a, mut b) = (was.chars(), is.chars());
        loop {
            let (x, y) = match (a.next(), b.next()) {
                (None, None) => break,
                (x, y) => (x.unwrap_or(' '), y.unwrap_or(' ')),
            };
            n += 1;
            eq += (x == y) as usize;
            cl += (class(x) == class(y)) as usize;
        }
        same += eq as f32 / n as f32;
        alike += cl as f32 / n as f32;
    }
    rows >= 2 && same >= REPAINT_SAME * rows as f32 && alike >= REPAINT_ALIKE * rows as f32
}

fn ms(v: u32) -> Duration {
    Duration::from_millis(v as u64)
}

impl Frames {
    pub fn new(cfg: FrameConfig, supports: Vec<Box<dyn TuiSupport>>) -> Frames {
        Frames {
            cfg,
            supports,
            screen: Screen::default(),
            now: Duration::ZERO,
            raw_heat: Heat::default(),
            raw_ready: Duration::ZERO,
            panes: BTreeMap::new(),
            active: Vec::new(),
            layout: Vec::new(),
            layout_id: 0,
            layouts: Vec::new(),
            pending: None,
            seg: None,
            seg_cursor: None,
            seps: Vec::new(),
            distrust: Vec::new(),
            cursor: Cursor::default(),
            trail: VecDeque::new(),
            next_id: 1,
            title_hash: 0,
            alt: false,
            started: false,
            screen_key: None,
            screen_conf: Vec::new(),
            screen_conf_at: None,
            title_changed: false,
            advice: HashMap::new(),
            areas: HashMap::new(),
            nested: RefCell::new(HashMap::new()),
            placed: RefCell::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &FrameConfig {
        &self.cfg
    }

    /// Id of the current layout. A layout seen before gets its id back.
    pub fn layout(&self) -> u32 {
        self.layout_id
    }

    /// Take the screen in, once per burst.
    pub fn update(&mut self, input: &ScreenInput, now: Duration) -> Vec<FrameEvent> {
        let mut ev = Vec::new();
        self.now = now;
        let resized = self.screen.scan(input.grid);
        let alt_changed = self.started && input.alt_screen != self.alt
            || input
                .events
                .iter()
                .any(|e| matches!(e, VtEvent::AltScreen(_)));
        if resized || alt_changed {
            self.raw_ready = now + ms(self.cfg.raw_settle_ms);
        }
        self.alt = input.alt_screen;
        let title_hash = fnv(input.title.as_bytes());
        let title_changed = self.started && title_hash != self.title_hash;
        self.title_hash = title_hash;
        if !self.started
            || (self.cursor.col, self.cursor.row) != (input.cursor.col, input.cursor.row)
        {
            self.trail.push_back(CursorSample {
                at: now,
                col: input.cursor.col,
                row: input.cursor.row,
            });
            while self.trail.len() > self.cfg.trail.max(1) {
                self.trail.pop_front();
            }
        }
        self.cursor = input.cursor;
        self.started = true;
        self.title_changed = title_changed;
        let mut screen_presets = false;
        if !self.supports.is_empty() && (self.screen.any_change || title_changed) {
            let all = Rect::new(0, 0, self.screen.cols, self.screen.rows);
            let pv = self.pane_view(input, all);
            if resized || alt_changed || self.detect_due(all, self.screen_conf_at, now) {
                self.screen_conf = self.supports.iter().map(|s| s.detect(&pv)).collect();
                self.screen_conf_at = Some(now);
            }
            screen_presets = self.screen_conf.iter().any(|c| *c >= 0.5);
            self.screen_key = self
                .supports
                .iter()
                .zip(&self.screen_conf)
                .filter(|(_, c)| **c >= 0.5)
                .find_map(|(s, _)| s.frame_key(&pv));
        }

        // Segmentation, when something structural may have changed.
        let cursor_row = Some(input.cursor.row);
        let expired = self.distrust.iter().any(|d| d.until <= now);
        // A plugin that knows the screen may segment on anything it sees.
        let plugins = screen_presets || (self.screen.any_change && !self.areas.is_empty());
        if self.seg.is_none()
            || self.screen.class_change
            || cursor_row != self.seg_cursor
            || expired
            || plugins
        {
            self.distrust.retain(|d| d.until > now);
            let (mut leaves, mut seps, mut areas) = self.segment(input, now);
            let moved = {
                let s = &self.screen;
                let prev = |r: u16| {
                    if s.row_dirty[r as usize] {
                        s.prev_lines[r as usize].clone()
                    } else {
                        s.lines[r as usize].clone()
                    }
                };
                let next = |r: u16| s.lines[r as usize].clone();
                if resized {
                    Vec::new()
                } else {
                    scrolled_seps(&self.seps, &seps, &prev, &next)
                }
            };
            if !moved.is_empty() {
                for m in moved {
                    self.distrust.push(Distrust {
                        bar: m.bar,
                        area: m.parent,
                        until: now + ms(self.cfg.distrust_ms),
                    });
                }
                (leaves, seps, areas) = self.segment(input, now);
            }
            self.areas = areas;
            self.seps = seps;
            self.seg = Some(leaves);
            self.seg_cursor = cursor_row;
        }
        if alt_changed {
            // The other screen is other documents: every frame leaves, and
            // the ones of this screen are looked for again.
            for id in std::mem::take(&mut self.active) {
                self.deactivate(id, now, &mut ev);
            }
        }
        self.reconcile(now, Some(input), resized || alt_changed, &mut ev);

        // Panes.
        let chrome_changed = title_changed
            || alt_changed
            || self.active.iter().any(|id| {
                let p = &self.panes[id];
                p.kind == FrameKind::Chrome
                    && (p.rect.y..p.rect.bottom()).any(|r| self.screen.row_dirty[r as usize])
                    && self.chrome_dirty(p)
            });
        for id in self.active.clone() {
            self.observe(id, Some(input), now, chrome_changed, &mut ev);
        }

        // Heat.
        let all = Rect::new(0, 0, self.screen.cols, self.screen.rows);
        self.raw_heat.update(&self.screen, all, now);
        for id in &self.active {
            let p = self.panes.get_mut(id).unwrap();
            p.heat.update(&self.screen, p.rect, now);
        }
        self.promote(now, &mut ev);
        self.screen.settle();
        self.placed.borrow_mut().retain(|_, v| {
            v.retain(|e| e.used + Duration::from_secs(10) > now);
            !v.is_empty()
        });
        ev
    }

    /// Let time pass without a burst: promotions, commits, pending layout.
    pub fn tick(&mut self, now: Duration) -> Vec<FrameEvent> {
        let mut ev = Vec::new();
        if now < self.now || !self.started {
            return ev;
        }
        self.now = now;
        self.reconcile(now, None, false, &mut ev);
        for id in self.active.clone() {
            let p = &self.panes[&id];
            if p.fresh {
                self.observe(id, None, now, false, &mut ev);
            }
            let p = self.panes.get_mut(&id).unwrap();
            if !p.suspended {
                p.ext.commit_due(now, p.commit);
            }
        }
        let all = Rect::new(0, 0, self.screen.cols, self.screen.rows);
        self.raw_heat.update(&self.screen, all, now);
        for id in &self.active {
            let p = self.panes.get_mut(id).unwrap();
            p.heat.update(&self.screen, p.rect, now);
        }
        self.promote(now, &mut ev);
        ev
    }

    /// Next time `tick` has something to do.
    pub fn next_wake(&self) -> Option<Duration> {
        let mut wake: Option<Duration> = None;
        let mut at = |t: Duration| wake = Some(wake.map_or(t, |w: Duration| w.min(t)));
        if let Some((_, since)) = &self.pending {
            at(*since + ms(self.cfg.flap_ms));
        }
        if self.raw_ready > self.now {
            at(self.raw_ready);
        }
        for id in &self.active {
            let p = &self.panes[id];
            if p.fresh {
                at(self.now);
            }
            if p.state == FrameState::Candidate && !p.suspended {
                at(p.since + p.settle);
            }
            if !p.suspended
                && p.kind == FrameKind::Pane
                && let Some(t) = p.ext.next_commit(p.commit)
            {
                at(t);
            }
        }
        wake
    }

    /// Every frame, on screen or remembered, by id.
    pub fn list(&self) -> Vec<FrameInfo> {
        self.list_at(self.now)
    }

    /// `list`, with `idle` as it stands at `now`.
    pub fn list_at(&self, now: Duration) -> Vec<FrameInfo> {
        let idle = |id| self.idle(id, now).unwrap_or_default();
        let raw_idle = idle(RAW);
        let mut out = vec![FrameInfo {
            id: RAW,
            kind: FrameKind::Raw,
            state: if self.now >= self.raw_ready {
                FrameState::Stable
            } else {
                FrameState::Candidate
            },
            usable: self.started && self.now >= self.raw_ready,
            rect: [0, 0, self.screen.cols, self.screen.rows],
            origin: [0, 0],
            extent: [0, (self.screen.rows as i64 - 1).max(0)],
            key: None,
            support: None,
            idle: raw_idle.score,
            is_idle: raw_idle.idle,
        }];
        for p in self.panes.values() {
            let c = p.content();
            let o = p.origin();
            let (lo, hi) = if p.kind == FrameKind::Pane {
                p.ext.extent()
            } else {
                (o.y, o.y + c.h as i64 - 1)
            };
            let calm = idle(p.id);
            out.push(FrameInfo {
                id: p.id,
                kind: p.kind,
                state: p.state,
                usable: self.usable(p),
                rect: [c.x, c.y, c.w, c.h],
                origin: [o.x, o.y],
                extent: [lo, hi],
                key: p.key.clone(),
                support: p.support.map(|s| self.supports[s].name().to_string()),
                idle: calm.score,
                is_idle: calm.idle,
            });
        }
        out
    }

    /// Text of the frame rows `[y0, y1)`: what is shown, else what is
    /// remembered, else empty. Trailing spaces trimmed. Column 0 of every
    /// string is the same frame column.
    pub fn text(&self, id: FrameId, y0: i64, y1: i64) -> Vec<String> {
        let y1 = y1.min(y0.saturating_add(100_000));
        if id == RAW {
            return (y0..y1)
                .map(|y| {
                    if y >= 0 && y < self.screen.rows as i64 {
                        self.screen.lines[y as usize]
                            .text
                            .trim_end_matches(' ')
                            .to_string()
                    } else {
                        String::new()
                    }
                })
                .collect();
        }
        let Some(p) = self.panes.get(&id) else {
            return Vec::new();
        };
        let shown = self.is_active(id) && !p.ext.rows.is_empty();
        let c = p.content();
        let top = p.top() as i64;
        let mut rows: Vec<(i64, String)> = Vec::new();
        for y in y0..y1 {
            let i = y - p.ext.oy;
            let stored = p.ext.lines.get(&y);
            if shown && i >= top && i < top + c.h as i64 {
                let row = &p.ext.rows[i as usize];
                match stored {
                    Some(s) if row.committed => rows.push((s.x0, s.line.text.clone())),
                    _ => rows.push((p.ext.ox, row.line.text.clone())),
                }
            } else if let Some(s) = stored {
                rows.push((s.x0, s.line.text.clone()));
            } else {
                rows.push((i64::MAX, String::new()));
            }
        }
        let base = rows.iter().map(|r| r.0).min().unwrap_or(0).min(p.ext.ox);
        rows.into_iter()
            .map(|(x0, t)| {
                if t.is_empty() || x0 == base {
                    t
                } else {
                    format!("{}{}", " ".repeat((x0 - base) as usize), t)
                }
            })
            .collect()
    }

    /// Heatmap of the frame's view, as of the last update or tick.
    pub fn heat(&self, id: FrameId) -> Option<HeatView> {
        self.heat_at(id, self.now)
    }

    /// Heatmap of the frame's view at `now`: time passes without a tick.
    pub fn heat_at(&self, id: FrameId, now: Duration) -> Option<HeatView> {
        let now = now.max(self.now);
        if id == RAW {
            return Some(
                self.raw_heat
                    .view(Rect::new(0, 0, self.screen.cols, self.screen.rows), now),
            );
        }
        let p = self.panes.get(&id)?;
        if !self.active.contains(&id) {
            // Not on screen: nothing is known of it now.
            return Some(Heat::default().view(Rect::new(0, 0, p.rect.w, p.rect.h), now));
        }
        Some(p.heat.view(self.heat_rect(p), now))
    }

    fn heat_rect(&self, p: &Pane) -> Rect {
        let c = p.content();
        Rect::new(c.x - p.rect.x, c.y - p.rect.y, c.w, c.h)
    }

    /// How much of the frame is static at `now`.
    pub fn idle(&self, id: FrameId, now: Duration) -> Option<Idle> {
        self.heat_at(id, now).map(|h| h.idle())
    }

    /// The innermost pane showing a screen cell, and the frame position
    /// there. The raw frame when no pane does.
    pub fn at(&self, col: u16, row: u16) -> Option<(FrameId, FramePos)> {
        if col >= self.screen.cols || row >= self.screen.rows {
            return None;
        }
        let hit = self
            .active
            .iter()
            .filter_map(|id| self.info(*id))
            .filter(|i| i.rect.contains(col, row))
            .min_by_key(|i| (i.kind != FrameKind::Pane, i.rect.w as u32 * i.rect.h as u32));
        match hit {
            Some(i) => Some((i.id, i.to_frame(col, row)?)),
            None => Some((
                RAW,
                FramePos {
                    x: col as i64,
                    y: row as i64,
                },
            )),
        }
    }

    /// As `Surfaces::find`, with the error of a bad regular expression.
    pub fn try_find(
        &self,
        id: FrameId,
        query: &TextQuery,
        known: bool,
    ) -> Result<Vec<TextMatch>, String> {
        let pattern = if query.regex {
            query.pattern.clone()
        } else {
            regex::escape(&query.pattern)
        };
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(query.ignore_case)
            .build()
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let mut scan = |line: &Line, x0: i64, y: i64, info: &SurfaceInfo| {
            // For a regular expression trailing blanks are not text: `$`
            // is the end of what is written. A literal is searched in the
            // row as it is, so `"$ "` finds a prompt.
            let hay = if query.regex {
                line.text.trim_end_matches(' ')
            } else {
                &line.text[..]
            };
            for m in re.find_iter(hay) {
                if m.is_empty() || out.len() >= 10_000 {
                    continue;
                }
                let k0 = line.text[..m.start()].chars().count();
                let k1 = k0 + m.as_str().chars().count();
                let c0 = line.col_of(k0);
                // The column after the match; at the end of the text, from
                // the width of its last glyph.
                let c1 = if k1 < line.text.chars().count() {
                    line.col_of(k1)
                } else {
                    let last = m.as_str().chars().next_back().unwrap_or(' ');
                    line.col_of(k1 - 1) + if is_wide(last) { 2 } else { 1 }
                };
                let pos = FramePos {
                    x: x0 + c0 as i64,
                    y,
                };
                out.push(TextMatch {
                    frame: info.id,
                    x: pos.x,
                    y,
                    len: c1.saturating_sub(c0).max(1),
                    text: m.as_str().to_string(),
                    screen: if info.state == FrameState::Inactive {
                        None
                    } else {
                        info.to_screen(pos)
                    },
                });
            }
        };
        let info = self.info(id).ok_or_else(|| format!("no frame {id}"))?;
        if id == RAW {
            for (r, line) in self.screen.lines.iter().enumerate() {
                scan(line, 0, r as i64, &info);
            }
            return Ok(out);
        }
        let p = &self.panes[&id];
        let shown = self.is_active(id);
        let c = p.content();
        let (first, last) = (info.origin.y, info.origin.y + c.h as i64);
        if known {
            for (y, s) in p.ext.lines.range(..first) {
                scan(&s.line, s.x0, *y, &info);
            }
        }
        if shown {
            for r in 0..c.h {
                let y = first + r as i64;
                let i = (y - p.ext.oy) as usize;
                match p.ext.rows.get(i) {
                    // The remembered line may be wider than the view.
                    Some(row) if known && row.committed && p.ext.lines.contains_key(&y) => {
                        let s = &p.ext.lines[&y];
                        scan(&s.line, s.x0, y, &info)
                    }
                    Some(row) => scan(&row.line, p.ext.ox, y, &info),
                    None => scan(
                        &self.screen.lines[(c.y + r) as usize].slice(c.x, c.right()),
                        p.ext.ox,
                        y,
                        &info,
                    ),
                }
            }
        }
        if known {
            let from = if shown { last } else { first };
            for (y, s) in p.ext.lines.range(from..) {
                scan(&s.line, s.x0, *y, &info);
            }
        }
        Ok(out)
    }

    // -- internals -----------------------------------------------------

    fn is_active(&self, id: FrameId) -> bool {
        self.active.contains(&id)
    }

    fn usable(&self, p: &Pane) -> bool {
        p.state == FrameState::Stable && !p.suspended && self.is_active(p.id)
    }

    fn chrome_dirty(&self, p: &Pane) -> bool {
        // Text only: a clock repainted with the same digits is no change.
        (0..p.rect.h).any(|r| {
            let row = (p.rect.y + r) as usize;
            self.screen.row_dirty[row]
                && Screen::hash_of(&self.screen.prev_lines[row], p.rect.x, p.rect.right())
                    != self.screen.row_hash(row as u16, p.rect.x, p.rect.right())
        })
    }

    fn pane_view<'a>(&self, input: &'a ScreenInput<'a>, rect: Rect) -> PaneView<'a> {
        let cur = (input.cursor.col, input.cursor.row);
        PaneView {
            grid: input.grid,
            rect,
            area: self.areas.get(&r4(rect)).map_or(rect, |a| rect_of(*a)),
            cursor: rect.contains(cur.0, cur.1).then_some(cur),
            title: input.title,
            alt_screen: input.alt_screen,
        }
    }

    /// Is it time to look for presets again over `rect`, last asked at
    /// `at`.
    fn detect_due(&self, rect: Rect, at: Option<Duration>, now: Duration) -> bool {
        self.title_changed
            || at.is_none_or(|t| now >= t + DETECT_EVERY)
            || (rect.y..rect.bottom())
                .filter(|r| self.screen.row_dirty[*r as usize])
                .nth(1)
                .is_some()
    }

    fn detect(&self, pv: &PaneView) -> Option<usize> {
        let whole = pv.rect == Rect::new(0, 0, self.screen.cols, self.screen.rows)
            && self.screen_conf_at == Some(self.now)
            && self.screen_conf.len() == self.supports.len();
        let mut best: Option<(usize, f32)> = None;
        for (i, s) in self.supports.iter().enumerate() {
            let c = if whole {
                self.screen_conf[i]
            } else {
                s.detect(pv)
            };
            if c >= 0.5 && best.is_none_or(|b| c > b.1) {
                best = Some((i, c));
            }
        }
        best.map(|b| b.0)
    }

    /// What the preset `support` says of a pane.
    fn advise(&self, pv: &PaneView, support: Option<usize>) -> Advice {
        match support {
            Some(s) => {
                let sup = &self.supports[s];
                Advice {
                    support,
                    chrome: sup.chrome(pv),
                    hint: sup.view_hint(pv),
                    key: sup.frame_key(pv),
                    tuning: sup.tuning(pv),
                }
            }
            None => Advice::default(),
        }
    }

    /// Key of a pane: what fills the screen first (a tmux window), then
    /// what the pane shows (a file).
    fn full_key(&self, pane_key: Option<String>) -> Option<String> {
        match (&self.screen_key, pane_key) {
            (Some(s), Some(k)) if *s != k => Some(format!("{s}/{k}")),
            (Some(s), _) => Some(s.clone()),
            (None, k) => k,
        }
    }

    /// Leaves of the rectangles a preset made of `area`.
    fn preset_leaves(&self, rects: Vec<Rect>, area: Rect) -> Vec<Leaf> {
        let mut leaves: Vec<Leaf> = rects
            .into_iter()
            .map(|r| r.intersect(&area))
            .filter(|r| !r.is_empty())
            .map(|r| Leaf {
                rect: r4(r),
                kind: (r.h < self.cfg.min_pane_rows || r.w < self.cfg.min_pane_cols) as u8,
            })
            .collect();
        leaves.sort();
        leaves.dedup();
        leaves
    }

    /// The panes a preset makes of `rect`, a rectangle below the whole
    /// screen. Presets are looked for as on a pane (`detect_due`); those
    /// found are asked at every segmentation.
    fn segment_nested(&self, input: &ScreenInput, rect: Rect, now: Duration) -> Option<Vec<Leaf>> {
        if rect.h < self.cfg.min_pane_rows || rect.w < self.cfg.min_pane_cols {
            return None;
        }
        let pv = PaneView {
            area: rect,
            ..self.pane_view(input, rect)
        };
        let mut nested = self.nested.borrow_mut();
        let at = nested.get(&r4(rect)).map(|n| n.0);
        if self.detect_due(rect, at, now) {
            let found = (0..self.supports.len())
                .filter(|i| self.supports[*i].detect(&pv) >= 0.5)
                .collect();
            if nested.len() >= 64 {
                nested.retain(|_, n| now < n.0 + NESTED_KEEP);
            }
            nested.insert(r4(rect), (now, found));
        }
        nested[&r4(rect)]
            .1
            .iter()
            .find_map(|i| self.supports[*i].segment(&pv))
            .map(|rects| self.preset_leaves(rects, rect))
    }

    fn segment(&self, input: &ScreenInput, now: Duration) -> (Vec<Leaf>, Vec<Sep>, Areas) {
        let all = Rect::new(0, 0, self.screen.cols, self.screen.rows);
        let pv = self.pane_view(input, all);
        for (i, s) in self.supports.iter().enumerate() {
            let conf = match self.screen_conf.get(i) {
                Some(c) if self.screen_conf_at.is_some() => *c,
                _ => s.detect(&pv),
            };
            if conf >= 0.5
                && let Some(rects) = s.segment(&pv)
            {
                let leaves = self.preset_leaves(rects, all);
                let areas = leaves.iter().map(|l| (l.rect, r4(all))).collect();
                return (leaves, Vec::new(), areas);
            }
        }
        let areas = RefCell::new(Areas::new());
        let ask = |rect: Rect| {
            let leaves = self.segment_nested(input, rect, now)?;
            areas
                .borrow_mut()
                .extend(leaves.iter().map(|l| (l.rect, r4(rect))));
            Some(leaves)
        };
        let (leaves, seps) = Segmenter {
            screen: &self.screen,
            cursor_row: Some(input.cursor.row),
            min_rows: self.cfg.min_pane_rows,
            min_cols: self.cfg.min_pane_cols,
            distrust: &self.distrust,
            now,
            leaves: Vec::new(),
            seps: Vec::new(),
            ask: (!self.supports.is_empty()).then_some(&ask),
        }
        .run();
        (leaves, seps, areas.into_inner())
    }

    /// Bring the layout in line with the last segmentation.
    fn reconcile(
        &mut self,
        now: Duration,
        input: Option<&ScreenInput>,
        force: bool,
        ev: &mut Vec<FrameEvent>,
    ) {
        let Some(seg) = self.seg.clone() else { return };
        if seg == self.layout && self.layout_id != 0 && !force {
            self.pending = None;
            for id in self.active.clone() {
                self.resume(id);
            }
            return;
        }
        let start = match &self.pending {
            Some((p, since)) if *p == seg => *since,
            _ => now,
        };
        if force || self.layout_id == 0 || now >= start + ms(self.cfg.flap_ms) {
            self.pending = None;
            self.commit_layout(seg, now, input, ev);
        } else {
            for id in self.active.clone() {
                let p = &self.panes[&id];
                let leaf = Leaf {
                    rect: r4(p.rect),
                    kind: (p.kind != FrameKind::Pane) as u8,
                };
                if seg.contains(&leaf) {
                    self.resume(id);
                } else {
                    self.panes.get_mut(&id).unwrap().suspended = true;
                }
            }
            // The layout will be committed on a tick, without the screen:
            // ask the presets now, for the rectangles to come. A tick
            // while it is pending keeps what was asked.
            if let Some(inp) = input {
                self.advice.clear();
                for leaf in seg.iter().filter(|l| l.kind == 0) {
                    let pv = self.pane_view(inp, leaf.rect());
                    let advice = self.advise(&pv, self.detect(&pv));
                    self.advice.insert(leaf.rect, advice);
                }
            }
            self.pending = Some((seg, start));
        }
    }

    /// End a suspension: what the pane missed is looked at again.
    fn resume(&mut self, id: FrameId) {
        let p = self.panes.get_mut(&id).unwrap();
        if p.suspended {
            p.suspended = false;
            p.hashes_valid = false;
        }
    }

    fn new_pane(&mut self, leaf: &Leaf, now: Duration) -> FrameId {
        let id = self.next_id;
        self.next_id += 1;
        self.panes.insert(
            id,
            Pane {
                id,
                kind: leaf.kind(),
                state: FrameState::Candidate,
                since: now,
                rect: leaf.rect(),
                ext: Ext::new(self.cfg.max_lines),
                heat: Heat::default(),
                key: None,
                support: None,
                support_at: None,
                cleared: None,
                found: None,
                found_at: None,
                suspended: false,
                fresh: true,
                reattach: false,
                last_active: now,
                cursor_seen: Duration::ZERO,
                alt: self.alt,
                churn: None,
                monitor: false,
                declared: false,
                covered: Vec::new(),
                repaint_cursor: None,
                fed: None,
                left: 0,
                right: 0,
                gutter_cand: (0, 0),
                gutter_row: None,
                status_row: None,
                status_bad: false,
                plugin_left: 0,
                fixed_tb: None,
                hint: None,
                hashes: Vec::new(),
                hashes_valid: false,
                settle: ms(self.cfg.settle_ms),
                commit: ms(self.cfg.commit_ms),
            },
        );
        id
    }

    fn hashes_for(&self, rect: Rect, left: u16, right: u16) -> Vec<u64> {
        let (a, b) = if left + right >= rect.w {
            (rect.x, rect.right())
        } else {
            (rect.x + left, rect.right() - right)
        };
        (rect.y..rect.bottom())
            .map(|r| self.screen.row_hash(r, a, b))
            .collect()
    }

    /// The remembered frame that this rectangle of the screen shows.
    fn recognise(
        &self,
        leaf: &Leaf,
        input: Option<&ScreenInput>,
        skip: &[FrameId],
        by_content: bool,
        left_at: Option<Duration>,
    ) -> Option<FrameId> {
        let rect = leaf.rect();
        // `by_content`: the key cannot tell the candidates apart. Without
        // the screen (a tick), the key asked while the layout was pending.
        let key = if by_content {
            None
        } else if let Some(inp) = input {
            let pv = self.pane_view(inp, rect);
            self.full_key(
                self.detect(&pv)
                    .and_then(|s| self.supports[s].frame_key(&pv)),
            )
        } else {
            self.full_key(self.advice.get(&leaf.rect).and_then(|a| a.key.clone()))
        };
        // Same rectangle first; else a frame whose rectangle overlaps (a
        // pane resized keeps its identity).
        for exact in [true, false] {
            let mut best: Option<(FrameId, f32, (Duration, Duration))> = None;
            for p in self.panes.values() {
                if p.state != FrameState::Inactive
                    || p.kind != leaf.kind()
                    || p.alt != self.alt
                    || skip.contains(&p.id)
                {
                    continue;
                }
                if exact != (p.rect == rect) {
                    continue;
                }
                if !exact {
                    let x = p.rect.intersect(&rect);
                    let small =
                        (p.rect.w as u32 * p.rect.h as u32).min(rect.w as u32 * rect.h as u32);
                    if p.kind == FrameKind::Chrome || (x.w as u32 * x.h as u32) * 2 < small {
                        continue;
                    }
                }
                let score = if p.kind == FrameKind::Chrome {
                    1.0
                } else if key.is_some() && p.key == key {
                    2.0 + p.ext.share(&self.hashes_for(rect, p.left, p.right))
                } else if key.is_some() && p.key.is_some() {
                    0.0
                } else {
                    p.ext.share(&self.hashes_for(rect, p.left, p.right))
                };
                if score >= self.cfg.identity
                    && best.is_none_or(|b| {
                        // Equal scores: the frame the cursor was in last.
                        score > b.1 || (score == b.1 && (p.cursor_seen, p.last_active) > b.2)
                    })
                {
                    best = Some((p.id, score, (p.cursor_seen, p.last_active)));
                }
            }
            if best.is_some() {
                return best.map(|b| b.0);
            }
        }
        // Too little text to tell (an empty shell resized): the one pane
        // that left the screen at this very layout, of the same key and
        // overlapping, is this one.
        let now = left_at?;
        if leaf.kind() != FrameKind::Pane {
            return None;
        }
        let inked = self
            .hashes_for(rect, 0, 0)
            .iter()
            .filter(|h| **h != screen::BLANK)
            .count();
        if inked >= 2 {
            return None;
        }
        let mut heirs = self.panes.values().filter(|p| {
            p.state == FrameState::Inactive
                && p.kind == FrameKind::Pane
                && p.alt == self.alt
                && p.last_active == now
                && p.key == key
                && !skip.contains(&p.id)
                && {
                    let x = p.rect.intersect(&rect);
                    let small =
                        (p.rect.w as u32 * p.rect.h as u32).min(rect.w as u32 * rect.h as u32);
                    (x.w as u32 * x.h as u32) * 2 >= small
                }
        });
        match (heirs.next(), heirs.next()) {
            (Some(p), None) => Some(p.id),
            _ => None,
        }
    }

    fn attach(&mut self, id: FrameId, rect: Rect, now: Duration, ev: &mut Vec<FrameEvent>) {
        let p = self.panes.get_mut(&id).unwrap();
        p.rect = rect;
        p.state = FrameState::Candidate;
        p.since = now;
        p.suspended = false;
        p.fresh = true;
        p.reattach = true;
        p.hashes_valid = false;
        p.support_at = None;
        ev.push(FrameEvent::Attached { frame: id });
    }

    fn deactivate(&mut self, id: FrameId, now: Duration, ev: &mut Vec<FrameEvent>) {
        let p = self.panes.get_mut(&id).unwrap();
        p.ext.flush(now);
        p.ext.reset_view();
        p.state = FrameState::Inactive;
        p.suspended = false;
        p.last_active = now;
        ev.push(FrameEvent::Inactive { frame: id });
    }

    fn prune(&mut self, ev: &mut Vec<FrameEvent>) {
        loop {
            let inactive: Vec<(Duration, FrameId)> = self
                .panes
                .values()
                .filter(|p| p.state == FrameState::Inactive)
                .map(|p| (p.last_active, p.id))
                .collect();
            if inactive.len() <= self.cfg.max_inactive {
                return;
            }
            let (_, id) = inactive.into_iter().min().unwrap();
            self.panes.remove(&id);
            ev.push(FrameEvent::Dropped { frame: id });
        }
    }

    fn commit_layout(
        &mut self,
        seg: Vec<Leaf>,
        now: Duration,
        input: Option<&ScreenInput>,
        ev: &mut Vec<FrameEvent>,
    ) {
        let old = std::mem::take(&mut self.active);
        // Panes whose rectangle is gone leave first: a new leaf may take
        // their identity.
        let mut kept: Vec<FrameId> = Vec::new();
        for id in old {
            let p = &self.panes[&id];
            let leaf = Leaf {
                rect: r4(p.rect),
                kind: (p.kind != FrameKind::Pane) as u8,
            };
            if seg.contains(&leaf) && !kept.iter().any(|k| self.panes[k].rect == p.rect) {
                kept.push(id);
            } else {
                self.deactivate(id, now, ev);
            }
        }
        let mut active: Vec<FrameId> = Vec::with_capacity(seg.len());
        for leaf in &seg {
            let keep = kept.iter().copied().find(|id| {
                let p = &self.panes[id];
                p.rect == leaf.rect() && p.kind == leaf.kind()
            });
            if let Some(id) = keep {
                self.resume(id);
                active.push(id);
            } else if let Some(id) = self.recognise(leaf, input, &active, false, Some(now)) {
                self.attach(id, leaf.rect(), now, ev);
                active.push(id);
            } else {
                let id = self.new_pane(leaf, now);
                ev.push(FrameEvent::Added { frame: id });
                active.push(id);
            }
        }
        self.active = active;
        self.layout_id = match self.layouts.iter().position(|l| *l == seg) {
            Some(i) => i as u32 + 1,
            None => {
                self.layouts.push(seg.clone());
                self.layouts.len() as u32
            }
        };
        self.layout = seg;
        ev.push(FrameEvent::Layout {
            layout: self.layout_id,
        });
        self.prune(ev);
    }

    fn promote(&mut self, now: Duration, ev: &mut Vec<FrameEvent>) {
        for id in &self.active {
            let p = self.panes.get_mut(id).unwrap();
            if p.state == FrameState::Candidate && !p.suspended && now >= p.since + p.settle {
                p.state = FrameState::Stable;
                ev.push(FrameEvent::Stable { frame: *id });
            }
        }
    }

    /// Line-number gutter of a pane: `(width, frame row of pane row 0)`.
    fn gutter(&self, rect: Rect) -> Option<(u16, i64)> {
        let mut run: Option<(usize, i64, u16, usize)> = None; // first row, number, end col, length
        let mut best: Option<(usize, i64, u16, usize)> = None;
        let mut nonblank = 0usize;
        for i in 0..rect.h as usize {
            let line = self.screen.lines[rect.y as usize + i].slice(rect.x, rect.right());
            if !line.text.trim_end_matches(' ').is_empty() {
                nonblank += 1;
            }
            let mut end = 0u16;
            let mut n: i64 = 0;
            let mut digits = 0;
            let mut ok = false;
            for (k, ch) in line.text.chars().enumerate() {
                if ch == ' ' && digits == 0 {
                    continue;
                }
                if ch.is_ascii_digit() && digits < 8 {
                    n = n * 10 + (ch as u8 - b'0') as i64;
                    digits += 1;
                    end = line.col_of(k) + 1;
                    continue;
                }
                ok = digits > 0 && ch == ' ';
                break;
            }
            let numbered = digits > 0
                && (ok || line.text.trim_end_matches(' ').chars().count() == end as usize);
            run = match run {
                Some((first, n0, e, len))
                    if numbered && e == end && n == n0 + (i - first) as i64 =>
                {
                    Some((first, n0, e, len + 1))
                }
                _ if numbered => Some((i, n, end, 1)),
                _ => None,
            };
            if let Some(r) = run
                && best.is_none_or(|b| r.3 > b.3)
            {
                best = Some(r);
            }
        }
        let (first, n0, end, len) = best?;
        if len < 3 || len * 10 < nonblank * 6 || end + 1 >= rect.w {
            return None;
        }
        // Numbers with nothing beside them are content (a counter, a
        // listing), not a gutter.
        let beside = (first..first + len)
            .filter(|i| {
                let line = self.screen.lines[rect.y as usize + i].slice(rect.x + end, rect.right());
                !line.text.trim_matches(' ').is_empty()
            })
            .count();
        if beside * 2 < len {
            return None;
        }
        Some((end + 1, n0 - 1 - first as i64))
    }

    /// Position shown by a status row, as pagers print it: `line 30`,
    /// `lines 30-58/300`. The row is the last of the pane or the one
    /// below it, holds the cursor, starts in standout, on the alternate
    /// screen. Returns the frame row of the first content row.
    fn status(&self, input: &ScreenInput, rect: Rect) -> Option<i64> {
        let (col, row) = (input.cursor.col, input.cursor.row);
        if !input.alt_screen
            || rect.h == 0
            || col < rect.x
            || col >= rect.right()
            || row + 1 < rect.bottom()
            || row > rect.bottom()
            || row >= self.screen.rows
        {
            return None;
        }
        let first = (rect.x..rect.right()).find(|c| !input.grid.cell(*c, row).is_blank())?;
        if !input
            .grid
            .cell(first, row)
            .style
            .attrs
            .contains(ptyrc_vt::Attrs::INVERSE)
        {
            return None;
        }
        let line = self.screen.lines[row as usize].slice(rect.x, rect.right());
        status_line(&line.text)
    }

    /// Feed a pane's tracker with the screen. `input` is absent on a tick:
    /// plugins are not consulted then.
    fn observe(
        &mut self,
        id: FrameId,
        input: Option<&ScreenInput>,
        now: Duration,
        chrome_changed: bool,
        ev: &mut Vec<FrameEvent>,
    ) {
        let Some(mut p) = self.panes.remove(&id) else {
            return;
        };
        if p.suspended {
            self.panes.insert(id, p);
            return;
        }
        if p.rect.contains(self.cursor.col, self.cursor.row) {
            p.cursor_seen = now;
        }
        if p.kind == FrameKind::Pane {
            let area = |r: Rect| r.w as u32 * r.h as u32;
            let covered: Vec<(u16, u16)> = self
                .seg
                .iter()
                .flatten()
                .map(|l| l.rect())
                .filter(|r| *r != p.rect && area(*r) < area(p.rect))
                .map(|r| r.intersect(&p.rect))
                .filter(|x| !x.is_empty())
                .map(|x| (x.y - p.rect.y, x.bottom() - p.rect.y))
                .collect();
            if covered != p.covered {
                p.covered = covered;
                p.hashes_valid = false;
            }
        }
        let dirty = !p.hashes_valid
            || (p.rect.y..p.rect.bottom()).any(|r| self.screen.row_dirty[r as usize]);
        if p.kind != FrameKind::Pane {
            p.fresh = false;
            p.hashes_valid = true;
            self.panes.insert(id, p);
            return;
        }
        let old_content = p.content();
        let mut new_key = p.key.clone();

        // Plugin knowledge: asked on a burst; on a tick, what was asked
        // for this rectangle while its layout was pending.
        let consult = input.is_some() && (dirty || p.fresh || p.support_at.is_none());
        let advice = if let Some(inp) = input
            && consult
        {
            let pv = self.pane_view(inp, p.rect);
            // Looked for on every change; once found, a preset is kept
            // through `SUPPORT_HOLD` of not being seen (a command line
            // hides the ruler vim is known by).
            if p.fresh || !p.hashes_valid || self.detect_due(p.rect, p.found_at, now) {
                p.found = self.detect(&pv);
                p.found_at = Some(now);
            }
            let found = p.found;
            let held = p.support.is_some() && p.support_at.is_some_and(|t| now < t + SUPPORT_HOLD);
            let support = found.or(if held { p.support } else { None });
            if found.is_some() || !held {
                p.support_at = Some(now);
            }
            Some(self.advise(&pv, support))
        } else if input.is_none() && p.fresh {
            let advice = self.advice.get(&r4(p.rect)).cloned();
            if advice.is_some() {
                p.support_at = Some(now);
            }
            advice
        } else {
            None
        };
        if let Some(advice) = advice {
            let t = advice.tuning;
            p.settle = ms(t.settle_ms.unwrap_or(self.cfg.settle_ms));
            p.commit = ms(t.commit_ms.unwrap_or(self.cfg.commit_ms));
            p.declared = t.monitor;
            p.support = advice.support;
            p.fixed_tb = advice.chrome.map(|c| (c.top, c.bottom));
            let (l, r) = advice.chrome.map_or((0, 0), |c| (c.left, c.right));
            p.plugin_left = l;
            if r != p.right {
                p.right = r;
                p.ext.reset_view();
                p.hashes_valid = false;
            }
            p.hint = advice.hint;
            new_key = self.full_key(advice.key);
        }
        let consult = consult || (input.is_none() && p.fresh);

        // Generic gutter: a number column is left chrome, with or without
        // a plugin. The plugin's own left chrome is a minimum.
        if dirty || consult {
            let found = self.gutter(p.rect).filter(|g| g.0 >= p.plugin_left);
            let width = found.map_or(p.plugin_left, |g| g.0);
            if width == p.left {
                p.gutter_cand = (width, 0);
            } else {
                p.gutter_cand = if p.gutter_cand.0 == width {
                    (width, p.gutter_cand.1 + 1)
                } else {
                    (width, 1)
                };
                // A number column is taken at once; its loss is believed
                // the second time (a half-drawn screen has none).
                // Sooner when the rows are the old ones moved left by the
                // width of the column.
                let moved_left = found.is_none() && p.left > width && {
                    let (mut same, mut all) = (0usize, 0usize);
                    for (i, row) in p.ext.rows.iter().enumerate().take(p.rect.h as usize) {
                        if row.line.text.is_empty() {
                            continue;
                        }
                        all += 1;
                        let x = p.rect.x + width;
                        let end = p.rect.right().saturating_sub(p.left - width).max(x);
                        let mut now_there = self.screen.lines[p.rect.y as usize + i].slice(x, end);
                        now_there.trim_end();
                        same += (now_there.text == row.line.text) as usize;
                    }
                    all >= 2 && same * 10 >= all * 6
                };
                if found.is_some() || moved_left || p.ext.lines.is_empty() || p.gutter_cand.1 >= 2 {
                    p.gutter_cand = (width, 0);
                    p.left = width;
                    p.ext.reset_view();
                    p.hashes_valid = false;
                }
            }
            p.gutter_row = found.filter(|g| g.0 == p.left).map(|g| g.1);
            if let Some(inp) = input {
                p.status_row = if p.status_bad {
                    None
                } else {
                    self.status(inp, p.rect)
                };
            }
        }

        // Row hashes of the content columns.
        let (a, b) = {
            let (l, r) = if p.left + p.right >= p.rect.w {
                (0, 0)
            } else {
                (p.left, p.right)
            };
            (p.rect.x + l, p.rect.right() - r)
        };
        if !p.hashes_valid || p.hashes.len() != p.rect.h as usize {
            p.hashes = (p.rect.y..p.rect.bottom())
                .map(|r| self.screen.row_hash(r, a, b))
                .collect();
            p.hashes_valid = true;
        } else {
            for r in 0..p.rect.h {
                if self.screen.row_dirty[(p.rect.y + r) as usize] {
                    p.hashes[r as usize] = self.screen.row_hash(p.rect.y + r, a, b);
                }
            }
        }

        // Rows under an overlay keep what they showed: they do not vote
        // for a scroll or a jump, and are not rewritten. Never seen
        // before, they are blank: the overlay's text is not the pane's.
        let known = p.ext.rows.len() == p.hashes.len();
        let mut hidden: HashMap<usize, Line> = HashMap::new();
        for (a, b) in &p.covered {
            for i in *a as usize..*b as usize {
                let (hash, line) = if known {
                    (p.ext.rows[i].hash, p.ext.rows[i].line.clone())
                } else {
                    (screen::BLANK, Line::blank())
                };
                p.hashes[i] = hash;
                hidden.insert(i, line);
            }
        }

        let screen = &self.screen;
        let rect = p.rect;
        let line = move |i: usize| {
            if let Some(l) = hidden.get(&i) {
                return l.clone();
            }
            let mut l = screen.lines[rect.y as usize + i].slice(a, b);
            l.trim_end();
            l
        };
        let mut seed = 0i64;
        let mut seed_real = true;
        let (mut travel, mut room, mut cleared) = (0i64, i64::MAX, false);
        let mut seed_rows: Option<(usize, usize)> = None;
        if let Some(inp) = input {
            for e in inp.events {
                if let VtEvent::Scroll {
                    region,
                    dy,
                    scrollback,
                } = e
                {
                    let x = region.intersect(&rect);
                    if x.w as u32 * 5 >= rect.w as u32 * 4 && x.h as u32 * 5 >= rect.h as u32 * 4 {
                        seed -= *dy as i64;
                        seed_real &= *scrollback;
                        travel += (*dy as i64).abs();
                        room = room.min(x.h as i64);
                        let (a, b) = ((x.y - rect.y) as usize, (x.bottom() - rect.y) as usize);
                        seed_rows = Some(seed_rows.map_or((a, b), |(c, d)| (a.min(c), b.max(d))));
                    }
                } else if matches!(e, VtEvent::Clear | VtEvent::AltScreen(_)) {
                    cleared = true;
                }
            }
        }
        if cleared {
            p.cleared = Some(now);
        }
        let (hint_oy, hint_ox) = match (p.fixed_tb, p.hint) {
            (Some((t, _)), Some(h)) => (h.row.map(|r| r - t as i64), h.col),
            (None, Some(h)) => (h.row.map(|r| r - p.ext.top as i64), h.col),
            _ => (None, None),
        };
        // Numbers on screen are the better witness; a status row is read
        // when nothing else tells.
        let hint_oy = p.gutter_row.or(hint_oy);
        let top = p.fixed_tb.map_or(p.ext.top, |tb| tb.0) as i64;
        let (hint_oy, hint_soft) = match hint_oy {
            Some(oy) => (Some(oy), false),
            None => (p.status_row.map(|r| r - top), p.status_row.is_some()),
        };
        let cur = (self.cursor.col, self.cursor.row);
        // Whole pages fed in one burst (a key held): a pager feeds a page
        // as the region less its prompt rows, a jump as a whole region
        // or by a clear. Two to four pages are told from a jump by the
        // count.
        let page = room - p.fixed_tb.map_or(p.ext.bottom, |tb| tb.1) as i64;
        let paged = |fed: i64| {
            page >= 2
                && page < room
                && fed.abs() % page == 0
                && (1..=4).contains(&(fed.abs() / page))
        };
        let pages = seed.abs() >= room && paged(seed);
        // A feed of a screen or more that came in pieces is a jump as
        // well: no piece tells, their sum does.
        let mut overfed = false;
        if seed != 0 && travel == seed.abs() {
            let sum = match p.fed {
                Some((t, sum)) if now <= t + ext::FEED_GAP && sum.signum() == seed.signum() => {
                    sum + seed
                }
                _ => seed,
            };
            overfed = sum != seed && sum.abs() >= room && !paged(sum);
            p.fed = (!overfed).then_some((now, sum));
        } else if dirty {
            p.fed = None;
        }
        let obs = Obs {
            hashes: &p.hashes,
            line: &line,
            width: b - a,
            seed,
            seed_real,
            seed_tape: p.cleared.is_none_or(|t| now >= t + REPAINT)
                && travel == seed.abs()
                && (seed.abs() < room || pages),
            seed_rows,
            overfed,
            cursor_row: rect
                .contains(cur.0, cur.1)
                .then(|| (cur.1 - rect.y) as usize),
            hint_oy,
            hint_ox,
            hint_soft,
            bottom_floor: (hint_soft && self.cursor.row + 1 == rect.bottom()) as u16,
            side_pending: p.gutter_cand.1 > 0,
            fixed_tb: p.fixed_tb,
            now,
        };
        if p.reattach {
            p.ext.reattach(&obs);
            p.reattach = false;
        }
        let key_changed = p.key.is_some() && new_key.is_some() && p.key != new_key && !p.fresh;
        // A monitor is told by a repaint where the cells are: no clear, no
        // scroll. A pager or an editor showing another part of a listing
        // does one or the other, or is placed by a hint.
        // Nor does it show a cursor in the pane: nothing is read or edited
        // there.
        let still = !cleared && travel == 0 && p.cleared.is_none_or(|t| now >= t + REPAINT);
        let shown = self.cursor.visible && p.rect.contains(self.cursor.col, self.cursor.row);
        let in_place = still && !shown;
        if p.monitor && dirty && p.ext.rows.len() == p.hashes.len() {
            let (mut inked, mut changed) = (0usize, 0usize);
            for (row, hash) in p.ext.rows.iter().zip(&p.hashes) {
                inked += (row.hash != screen::BLANK || *hash != screen::BLANK) as usize;
                changed += (row.hash != *hash) as usize;
            }
            if cleared || (changed * 2 >= inked && !repainted(&p.ext.rows, &line)) {
                p.monitor = false;
            }
        }
        // A monitor that scrolls its list says so, and the rows agree.
        let scrolled = p.monitor && seed != 0 && travel == seed.abs() && {
            let h = p.hashes.len() as i64;
            let (mut all, mut moved) = (0usize, 0usize);
            for i in 0..h {
                let j = i + seed;
                if (0..h).contains(&j) && p.hashes[i as usize] != screen::BLANK {
                    all += 1;
                    moved += (p.ext.rows.get(j as usize).map(|r| r.hash)
                        == Some(p.hashes[i as usize])) as usize;
                }
            }
            all >= 2 && moved * 10 >= all * 9
        };
        let out = if (p.declared || p.monitor && !scrolled) && !p.fresh {
            p.ext.rewrite(&obs)
        } else if p.fresh || dirty || consult || p.ext.rows.is_empty() {
            p.ext.observe(&obs)
        } else {
            Outcome::Same
        };
        p.fresh = false;
        if std::mem::take(&mut p.ext.hint_refused) {
            p.status_bad = true;
            p.status_row = None;
        }
        for s in p.ext.shifts.drain(..) {
            ev.push(match s.dy {
                Some(dy) => FrameEvent::Shift {
                    frame: id,
                    from: s.from,
                    to: s.to,
                    dy,
                },
                None => FrameEvent::Forgotten {
                    frame: id,
                    from: s.from,
                    to: s.to,
                },
            });
        }
        match out {
            Outcome::Scroll { dx, dy } => ev.push(FrameEvent::Scroll { frame: id, dx, dy }),
            Outcome::Jump => ev.push(FrameEvent::Jump { frame: id }),
            _ => {}
        }
        p.ext.commit_due(now, p.commit);
        if p.content() != old_content || p.ext.rect_changed {
            ev.push(FrameEvent::Rect { frame: id });
        }

        let other_doc = out == Outcome::Other;
        let same_shape =
            out == Outcome::Lost && !key_changed && still && repainted(&p.ext.rows, &line);
        // A cursor is shown: an editor paints a jump in a table this way,
        // when a key is pressed. Three repaints at even intervals (one
        // part in ten), the cursor on the same cell, no scroll nor jump
        // between them, are a monitor's, however far apart.
        let mut again = false;
        if cleared
            || matches!(
                out,
                Outcome::Scroll { .. } | Outcome::Splice { .. } | Outcome::Jump
            )
        {
            p.repaint_cursor = None;
        } else if same_shape && shown {
            let since = match p.repaint_cursor {
                Some((at, t, every)) if at == cur => {
                    let d = now.saturating_sub(t);
                    again = every.is_some_and(|e| e.abs_diff(d) * 10 <= e.max(d));
                    Some(d)
                }
                _ => None,
            };
            p.repaint_cursor = Some((cur, now, since));
        }
        let repaint = same_shape && (in_place || again);
        if !matches!(out, Outcome::Lost | Outcome::Other) && !key_changed {
            p.key = new_key;
            self.panes.insert(id, p);
            return;
        }

        // The pane shows something else entirely.
        let leaf = Leaf {
            rect: r4(p.rect),
            kind: 0,
        };
        let same_key = p.key.is_some() && p.key == new_key;
        self.panes.insert(id, p);
        let other = if same_key && !other_doc {
            None
        } else {
            self.recognise(&leaf, input, &self.active, same_key, None)
        };
        let next = if let Some(other) = other {
            self.deactivate(id, now, ev);
            self.attach(other, leaf.rect(), now, ev);
            Some(other)
        } else if !key_changed
            && !other_doc
            && (repaint
                || self.panes[&id]
                    .churn
                    .is_some_and(|(t, n)| n >= 2 && now < t + CHURN))
        {
            // The same shape with other values, or replaced twice in a row
            // and whole again: a pane that repaints itself (a monitor), not
            // documents taking turns. Rewritten in place.
            let p = self.panes.get_mut(&id).unwrap();
            p.churn = Some((now, 2));
            p.monitor = true;
            p.ext.reset_view();
            p.fresh = true;
            None
        } else if key_changed || other_doc || (chrome_changed && !same_key) {
            let before = self.panes[&id]
                .churn
                .filter(|(t, _)| now < *t + CHURN)
                .map_or(0, |(_, n)| n);
            self.deactivate(id, now, ev);
            let fresh = self.new_pane(&leaf, now);
            self.panes.get_mut(&fresh).unwrap().churn = Some((now, before + 1));
            ev.push(FrameEvent::Added { frame: fresh });
            Some(fresh)
        } else {
            // A jump inside the same frame, to rows never seen.
            let p = self.panes.get_mut(&id).unwrap();
            p.ext.island();
            p.fresh = true;
            ev.push(FrameEvent::Jump { frame: id });
            None
        };
        let target = match next {
            Some(n) => {
                let slot = self.active.iter().position(|x| *x == id).unwrap();
                self.active[slot] = n;
                self.prune(ev);
                n
            }
            None => id,
        };
        self.observe(target, input, now, false, ev);
    }
}

impl Surfaces for Frames {
    fn info(&self, id: FrameId) -> Option<SurfaceInfo> {
        if id == RAW {
            let ready = self.started && self.now >= self.raw_ready;
            return Some(SurfaceInfo {
                id,
                kind: FrameKind::Raw,
                state: if ready {
                    FrameState::Stable
                } else {
                    FrameState::Candidate
                },
                rect: Rect::new(0, 0, self.screen.cols, self.screen.rows),
                origin: FramePos::default(),
                usable: ready,
            });
        }
        let p = self.panes.get(&id)?;
        Some(SurfaceInfo {
            id,
            kind: p.kind,
            state: p.state,
            rect: p.content(),
            origin: p.origin(),
            usable: self.usable(p),
        })
    }

    fn resolve(&self, sel: &FrameSel) -> Option<FrameId> {
        let name = match sel {
            FrameSel::Id(id) => return (*id == RAW || self.panes.contains_key(id)).then_some(*id),
            FrameSel::Name(n) => n.as_str(),
        };
        let panes = || {
            self.active
                .iter()
                .map(|id| &self.panes[id])
                .filter(|p| p.kind == FrameKind::Pane)
        };
        let area = |p: &Pane| p.rect.w as u32 * p.rect.h as u32;
        match name {
            "raw" => Some(RAW),
            "active" => {
                let (c, r) = (self.cursor.col, self.cursor.row);
                Some(
                    panes()
                        .filter(|p| p.rect.contains(c, r))
                        .min_by_key(|p| area(p))
                        .map_or(RAW, |p| p.id),
                )
            }
            "largest" => Some(
                panes()
                    .max_by_key(|p| (area(p), std::cmp::Reverse(p.id)))
                    .map_or(RAW, |p| p.id),
            ),
            "quietest" => Some(
                panes()
                    .filter(|p| self.usable(p))
                    .min_by(|x, y| {
                        (x.heat.mean_activity(), std::cmp::Reverse(area(x)))
                            .partial_cmp(&(y.heat.mean_activity(), std::cmp::Reverse(area(y))))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map_or(RAW, |p| p.id),
            ),
            "idle" => Some(
                panes()
                    .filter(|p| self.usable(p))
                    .map(|p| (p.heat.view(self.heat_rect(p), self.now).idle(), p))
                    .max_by(|(a, x), (b, y)| {
                        (a.score, a.free, area(x))
                            .partial_cmp(&(b.score, b.free, area(y)))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map_or(RAW, |(_, p)| p.id),
            ),
            other => {
                let id: FrameId = other.parse().ok()?;
                (id == RAW || self.panes.contains_key(&id)).then_some(id)
            }
        }
    }

    fn text(&self, id: FrameId) -> Vec<String> {
        match self.info(id) {
            Some(info) => {
                let y0 = info.origin.y;
                Frames::text(self, id, y0, y0 + info.rect.h as i64)
            }
            None => Vec::new(),
        }
    }

    fn find(&self, id: FrameId, query: &TextQuery, known: bool) -> Vec<TextMatch> {
        self.try_find(id, query, known).unwrap_or_default()
    }

    fn place(
        &self,
        id: FrameId,
        w: u16,
        h: u16,
        prefer: Prefer,
        taken: &[Rect],
    ) -> Option<(u16, u16)> {
        let view = self.heat(id)?;
        let best = view.best(w, h, prefer, taken)?;
        let now = self.now;
        let mut cache = self.placed.borrow_mut();
        let entries = cache.entry((id, w, h, prefer as u8)).or_default();
        for e in entries.iter_mut() {
            let r = Rect::new(e.pos.0, e.pos.1, w, h);
            if taken.iter().any(|t| t.intersects(&r)) {
                continue;
            }
            let Some(current) = view.score(e.pos.0, e.pos.1, w, h, prefer) else {
                continue;
            };
            e.used = now;
            if (best.0, best.1) == e.pos || best.2 <= current + self.cfg.replace_margin {
                e.better_since = None;
                return Some(e.pos);
            }
            let since = *e.better_since.get_or_insert(now);
            if now >= since + ms(self.cfg.replace_ms) {
                e.pos = (best.0, best.1);
                e.better_since = None;
            }
            return Some(e.pos);
        }
        entries.push(Placed {
            pos: (best.0, best.1),
            better_since: None,
            used: now,
        });
        Some((best.0, best.1))
    }

    fn cursor(&self) -> (u16, u16) {
        (self.cursor.col, self.cursor.row)
    }

    fn cursor_trail(&self) -> Vec<CursorSample> {
        self.trail.iter().copied().collect()
    }
}
