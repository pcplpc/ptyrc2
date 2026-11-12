// SPDX-License-Identifier: GPL-3.0-or-later
//! Extended frame: a sparse buffer of remembered lines and the view on it.
//! Scroll, splice, jump and edit detection from row hashes.

use super::screen::{BLANK, Line};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

/// Distance between islands, in rows.
pub(crate) const ISLAND_GAP: i64 = 100_000;
/// One write of the app may reach the driver in several bursts, this far
/// apart; keys repeat more slowly. A row that leaves the view sooner
/// than this after it came was on its way through: not remembered.
pub(crate) const FEED_GAP: Duration = Duration::from_millis(10);
/// Islands kept per frame; the least recently shown goes first.
const MAX_ISLANDS: usize = 8;

/// A run of frame rows whose place relative to the others is unknown.
#[derive(Clone, Copy, Debug, Default)]
struct Island {
    lo: i64,
    hi: i64,
    /// When the view was last on it, on the frame's own counter.
    used: u64,
    /// Where it is in the document, as last estimated (`Obs::progress`).
    share: Option<f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub hash: u64,
    pub since: Duration,
    pub committed: bool,
    pub line: Line,
}

#[derive(Clone, Debug)]
pub(crate) struct Stored {
    pub x0: i64,
    pub line: Line,
    pub hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Same,
    Edit,
    Scroll {
        dx: i64,
        dy: i64,
    },
    /// Rows `[from, to]` of the buffer moved by `dy`.
    Splice {
        from: i64,
        dy: i64,
    },
    Jump,
    /// Whole change, origin unknown. Nothing was applied.
    Lost,
    /// Whole change, and the app places the view on rows remembered with
    /// other content: another document. Nothing was applied.
    Other,
}

/// A change of the remembered buffer the scene must follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shift {
    pub from: i64,
    pub to: i64,
    /// `None`: the rows are forgotten, not moved.
    pub dy: Option<i64>,
}

/// One observation of the pane.
pub(crate) struct Obs<'a> {
    /// Hash per pane row, content columns only.
    pub hashes: &'a [u64],
    /// Trimmed text of a pane row, content columns only.
    pub line: &'a dyn Fn(usize) -> Line,
    /// Content width, in columns.
    pub width: u16,
    /// Sum of the emulator scroll events over the pane, as a `dy`.
    pub seed: i64,
    /// Every one of those events pushed rows into the host scrollback: a
    /// real scroll of the main screen, not a repaint by line feeds.
    pub seed_real: bool,
    /// The scroll is a tape feed: one direction, less than the height of
    /// the region it moved, and no clear in the burst. Every line that
    /// went through the region is on screen before or after.
    pub seed_tape: bool,
    /// Pane rows `[a, b)` those events moved: no top chrome among them.
    pub seed_rows: Option<(usize, usize)>,
    /// This feed and those just before it sum to the height of the
    /// region or more: lines went through unseen, the distance is not
    /// known.
    pub overfed: bool,
    /// Cursor row inside the pane.
    pub cursor_row: Option<usize>,
    /// Absolute frame row of pane row 0, when the app shows it.
    pub hint_oy: Option<i64>,
    pub hint_ox: Option<i64>,
    /// `hint_oy` was read by a generic rule, not given by a plugin: it
    /// places a jump, and is refused when it contradicts a scroll.
    pub hint_soft: bool,
    /// Rows at the bottom known as chrome (the status row read).
    pub bottom_floor: u16,
    /// The side chrome is about to change: a horizontal move of the
    /// content is not a scroll.
    pub side_pending: bool,
    /// Top and bottom chrome imposed by a plugin.
    pub fixed_tb: Option<(u16, u16)>,
    /// The pane is on the alternate screen.
    pub alt: bool,
    /// Share of the document above the last row shown, `0..=1`, read on
    /// a prompt: an estimate.
    pub progress: Option<f32>,
    pub now: Duration,
}

#[derive(Default)]
pub(crate) struct Ext {
    pub lines: BTreeMap<i64, Stored>,
    index: HashMap<u64, Vec<i64>>,
    /// Frame row of pane row 0, frame column of the first content column.
    pub oy: i64,
    pub ox: i64,
    pub rows: Vec<Row>,
    pub top: u16,
    pub bottom: u16,
    chrome_obs: VecDeque<(u16, u16)>,
    pub absolute: bool,
    islands: Vec<Island>,
    cur: usize,
    uses: u64,
    width: u16,
    /// Set when `top` or `bottom` changed in the last observation.
    pub rect_changed: bool,
    pub shifts: Vec<Shift>,
    pub max_lines: usize,
    /// A soft hint contradicted the rows seen scrolling.
    pub hint_refused: bool,
    bottom_floor: u16,
    /// Rows of the document, estimated where rows and a share were both
    /// known, and that share: the largest tells best.
    total: Option<(f32, i64)>,
    /// The last row as it was before it alone was erased, in the last
    /// observation: a prompt taken away before a feed.
    erased: Option<u64>,
}

fn need(nb: usize, percent: usize, floor: usize) -> usize {
    floor.max((nb * percent).div_ceil(100))
}

impl Ext {
    pub fn new(max_lines: usize) -> Ext {
        Ext {
            islands: vec![Island::default()],
            max_lines,
            ..Ext::default()
        }
    }

    fn band(&self, h: usize) -> (usize, usize) {
        let (t, b) = (self.top as usize, self.bottom as usize);
        if t + b >= h { (0, h) } else { (t, h - b) }
    }

    fn view_rows(&self) -> (i64, i64) {
        let (a, b) = self.band(self.rows.len());
        (self.oy + a as i64, self.oy + (b as i64 - 1).max(a as i64))
    }

    /// Known rows of the island the view is on: `(min, max)`.
    pub fn extent(&self) -> (i64, i64) {
        let (lo, hi) = self.view_rows();
        let isl = &self.islands[self.cur];
        let known = self.lines.range(isl.lo..=isl.hi);
        let first = known.clone().next().map_or(lo, |(k, _)| *k);
        let last = known.last().map_or(hi, |(k, _)| *k);
        (lo.min(first), hi.max(last))
    }

    /// Known rows of every island.
    fn extent_all(&self) -> (i64, i64) {
        let (mut lo, mut hi) = self.view_rows();
        if let Some((k, _)) = self.lines.first_key_value() {
            lo = lo.min(*k);
        }
        if let Some((k, _)) = self.lines.last_key_value() {
            hi = hi.max(*k);
        }
        (lo, hi)
    }

    /// Last row that belongs to the current island: the row before the
    /// next island.
    fn island_limit(&self) -> i64 {
        let hi = self.islands[self.cur].hi;
        self.islands
            .iter()
            .filter(|i| i.lo > hi)
            .map(|i| i.lo - 1)
            .min()
            .unwrap_or(i64::MAX - ISLAND_GAP)
    }

    fn enter(&mut self, i: usize) {
        self.cur = i;
        self.uses += 1;
        self.islands[i].used = self.uses;
    }

    /// Forget the islands shown least recently.
    fn collect(&mut self) {
        while self.islands.len() > MAX_ISLANDS {
            let Some(old) = (0..self.islands.len())
                .filter(|i| *i != self.cur)
                .min_by_key(|i| self.islands[*i].used)
            else {
                return;
            };
            let Island { lo, hi, .. } = self.islands[old];
            let keys: Vec<i64> = self.lines.range(lo..=hi).map(|(k, _)| *k).collect();
            for k in keys {
                self.remove_line(k);
            }
            self.islands.remove(old);
            // Not a move: what is drawn on these rows goes with them.
            self.shifts.push(Shift {
                from: lo - 200,
                to: hi + 200,
                dy: None,
            });
            if old < self.cur {
                self.cur -= 1;
            }
        }
    }

    fn fresh(&self, o: &Obs, i: usize) -> Row {
        Row {
            hash: o.hashes[i],
            since: o.now,
            committed: false,
            line: (o.line)(i),
        }
    }

    /// Forget the view rows; the next observation starts anew.
    pub fn reset_view(&mut self) {
        self.rows.clear();
    }

    // -- buffer --------------------------------------------------------

    fn unindex(&mut self, hash: u64, y: i64) {
        if hash == BLANK {
            return;
        }
        if let Some(v) = self.index.get_mut(&hash) {
            v.retain(|r| *r != y);
            if v.is_empty() {
                self.index.remove(&hash);
            }
        }
    }

    fn remove_line(&mut self, y: i64) {
        if let Some(old) = self.lines.remove(&y) {
            self.unindex(old.hash, y);
        }
    }

    fn insert_line(&mut self, y: i64, s: Stored) {
        self.remove_line(y);
        if s.hash != BLANK {
            self.index.entry(s.hash).or_default().push(y);
        }
        self.lines.insert(y, s);
    }

    /// Remember a view row at frame row `y`.
    fn store(&mut self, y: i64, line: &Line, hash: u64) {
        let x0 = self.ox;
        let merged = match self.lines.get(&y) {
            Some(old) if old.x0 != x0 && old.line.trivial() && line.trivial() => {
                Some(merge(old, x0, line, self.width))
            }
            _ => None,
        };
        let stored = match merged {
            Some((mx0, text)) => {
                let ascii = text.is_ascii();
                Stored {
                    x0: mx0,
                    line: Line {
                        text,
                        cols: None,
                        ascii,
                    },
                    hash,
                }
            }
            None => Stored {
                x0,
                line: line.clone(),
                hash,
            },
        };
        self.insert_line(y, stored);
        let isl = &mut self.islands[self.cur];
        isl.lo = isl.lo.min(y);
        isl.hi = isl.hi.max(y);
        if self.lines.len() > self.max_lines {
            let mid = self.oy;
            let first = *self.lines.first_key_value().unwrap().0;
            let last = *self.lines.last_key_value().unwrap().0;
            self.remove_line(if mid - first > last - mid {
                first
            } else {
                last
            });
        }
    }

    /// Move the rows `[from, to]` by `dy`, over what is there.
    fn shift_rows(&mut self, from: i64, to: i64, dy: i64) {
        if dy == 0 || from > to {
            return;
        }
        let keys: Vec<i64> = self.lines.range(from..=to).map(|(k, _)| *k).collect();
        let mut moved = Vec::with_capacity(keys.len());
        for k in keys {
            let s = self.lines.remove(&k).unwrap();
            self.unindex(s.hash, k);
            moved.push((k + dy, s));
        }
        for (k, s) in moved {
            self.insert_line(k, s);
        }
        let isl = &mut self.islands[self.cur];
        if from <= isl.hi && to >= isl.lo {
            isl.lo = isl.lo.min(from.max(isl.lo) + dy);
            isl.hi = isl.hi.max(to.min(isl.hi) + dy);
        }
        self.shifts.push(Shift {
            from,
            to,
            dy: Some(dy),
        });
    }

    fn commit_row(&mut self, i: usize) {
        let (a, b) = self.band(self.rows.len());
        if i < a || i >= b || self.rows[i].committed {
            return;
        }
        self.rows[i].committed = true;
        let (line, hash) = (self.rows[i].line.clone(), self.rows[i].hash);
        self.store(self.oy + i as i64, &line, hash);
    }

    /// Remember the view rows unchanged for `commit` or longer.
    pub fn commit_due(&mut self, now: Duration, commit: Duration) {
        for i in 0..self.rows.len() {
            if !self.rows[i].committed && now >= self.rows[i].since + commit {
                self.commit_row(i);
            }
        }
    }

    /// Remember every view row seen before `now`: the frame leaves.
    pub fn flush(&mut self, now: Duration) {
        for i in 0..self.rows.len() {
            if self.rows[i].since < now {
                self.commit_row(i);
            }
        }
    }

    pub fn next_commit(&self, commit: Duration) -> Option<Duration> {
        let (a, b) = self.band(self.rows.len());
        self.rows[a..b]
            .iter()
            .filter(|r| !r.committed)
            .map(|r| r.since + commit)
            .min()
    }

    // -- lookups -------------------------------------------------------

    /// Origin (`oy`) that puts the most view rows on remembered lines.
    fn lookup(&self, hashes: &[u64], skip: Option<(i64, i64)>) -> Option<(i64, usize)> {
        let (a, b) = self.band(hashes.len());
        let mut votes: HashMap<i64, usize> = HashMap::new();
        for (i, h) in hashes.iter().enumerate().take(b).skip(a) {
            if *h == BLANK {
                continue;
            }
            if let Some(rows) = self.index.get(h) {
                for r in rows.iter().take(16) {
                    if skip.is_some_and(|(lo, hi)| *r >= lo && *r <= hi) {
                        continue;
                    }
                    *votes.entry(*r - i as i64).or_default() += 1;
                }
            }
        }
        votes.into_iter().max_by_key(|(o, n)| {
            (
                *n,
                std::cmp::Reverse((*o - self.oy).abs()),
                std::cmp::Reverse(*o),
            )
        })
    }

    /// Share of the non-blank view rows found in the buffer, anywhere.
    pub fn share(&self, hashes: &[u64]) -> f32 {
        let nb = hashes.iter().filter(|h| **h != BLANK).count();
        if nb < 2 {
            return 0.0;
        }
        let hit = hashes
            .iter()
            .filter(|h| **h != BLANK && self.index.contains_key(h))
            .count();
        hit as f32 / nb as f32
    }

    /// Start a new island far below what is known.
    pub fn island(&mut self) {
        let (_, hi) = self.extent_all();
        self.oy = (hi.max(0) / ISLAND_GAP + 2) * ISLAND_GAP;
        self.islands.push(Island {
            lo: self.oy,
            hi: self.oy,
            used: 0,
            share: None,
        });
        self.enter(self.islands.len() - 1);
        self.absolute = false;
        self.rows.clear();
        self.collect();
    }

    /// The view was placed at `oy`: on the island that holds these rows,
    /// or the one they are nearest to (rows given by the app are of one
    /// numbering, known or not in between).
    fn enter_island(&mut self, h: usize) {
        let (lo, hi) = (self.oy, self.oy + h as i64);
        let reach = ISLAND_GAP / 2;
        let found = self
            .islands
            .iter()
            .position(|i| lo <= i.hi && hi >= i.lo)
            .or_else(|| {
                self.islands
                    .iter()
                    .position(|i| lo <= i.hi + reach && hi >= i.lo - reach)
            });
        match found {
            Some(i) => {
                let isl = &mut self.islands[i];
                isl.lo = isl.lo.min(lo);
                isl.hi = isl.hi.max(lo);
                self.enter(i);
            }
            None => {
                self.islands.push(Island {
                    lo,
                    hi: lo,
                    used: 0,
                    share: None,
                });
                self.enter(self.islands.len() - 1);
                self.collect();
            }
        }
    }

    /// First rows on an island that holds nothing yet (the pane was wiped,
    /// then repainted): when they are remembered elsewhere the view is
    /// there, and the island is given back.
    fn rejoin(&mut self, o: &Obs) -> bool {
        let Island { lo, hi, .. } = self.islands[self.cur];
        if self.islands.len() < 2 || self.lines.range(lo..=hi).any(|(_, s)| s.hash != BLANK) {
            return false;
        }
        let h = o.hashes.len();
        let (a, b) = self.band(h);
        let nb = o.hashes[a..b].iter().filter(|x| **x != BLANK).count();
        let Some((oy, votes)) = self.lookup(o.hashes, None) else {
            return false;
        };
        if votes < need(nb, 50, 3) {
            return false;
        }
        let keys: Vec<i64> = self.lines.range(lo..=hi).map(|(k, _)| *k).collect();
        for k in keys {
            self.remove_line(k);
        }
        self.islands.remove(self.cur);
        self.cur = 0;
        self.oy = oy;
        self.enter_island(h);
        self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
        self.mark_known(a, b);
        true
    }

    /// A share of the document read on screen. An island that holds
    /// nothing yet is put by it when first shown (`place`): among the others in the order of their
    /// shares when a slot is free between them, and at the estimated
    /// row within its slot. An estimate: islands stay `ISLAND_GAP`
    /// apart and are merged exactly when walked into. On the rows that
    /// start at 0 it tells the length of the document.
    fn estimate(&mut self, o: &Obs, place: bool) {
        let Some(share) = o.progress else { return };
        if o.hint_oy.is_some() || self.absolute {
            return;
        }
        let h = o.hashes.len();
        let (_, b) = self.band(h);
        let slot = |y: i64| y.div_euclid(ISLAND_GAP);
        let Island { lo, hi, .. } = self.islands[self.cur];
        let empty = self.islands.len() > 1 && self.lines.range(lo..=hi).next().is_none();
        if place && empty {
            let others = || {
                self.islands
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != self.cur)
                    .map(|(_, i)| i)
            };
            let before = others()
                .filter(|i| i.share.is_some_and(|s| s <= share))
                .map(|i| slot(i.hi))
                .max();
            let after = others()
                .filter(|i| i.share.is_some_and(|s| s > share))
                .map(|i| slot(i.lo))
                .min();
            let free = |s: i64| !others().any(|i| slot(i.lo) <= s && s <= slot(i.hi));
            let mine = slot(self.oy);
            let fits = |s: i64| before.is_none_or(|x| s > x) && after.is_none_or(|x| s < x);
            let to = if fits(mine) {
                Some(mine)
            } else {
                let from = before.map_or(0, |x| x + 1);
                let end = after.unwrap_or(from + 1);
                (from..end).find(|s| free(*s))
            };
            if let Some(to) = to {
                let row = self
                    .total
                    .map_or(0, |(_, t)| (share as f64 * t as f64) as i64 - b as i64)
                    .clamp(0, ISLAND_GAP / 2);
                self.oy = to * ISLAND_GAP + row;
                self.islands[self.cur].lo = self.oy;
                self.islands[self.cur].hi = self.oy;
            }
        } else if slot(lo) == 0
            && lo >= 0
            && share >= 0.05
            && self.total.is_none_or(|(s, _)| share >= s)
        {
            let seen = self.oy + b as i64;
            self.total = Some((share, (seen as f64 / share as f64) as i64));
        }
        self.islands[self.cur].share = Some(share);
    }

    /// Attach to a screen again: find the origin in the buffer.
    pub fn reattach(&mut self, o: &Obs) {
        self.rows.clear();
        let nb = o.hashes.iter().filter(|h| **h != BLANK).count();
        if let Some((oy, votes)) = self.lookup(o.hashes, None)
            && votes >= need(nb, 30, 2)
        {
            self.oy = oy;
            self.enter_island(o.hashes.len());
        }
    }

    // -- observation ---------------------------------------------------

    pub fn observe(&mut self, o: &Obs) -> Outcome {
        let h = o.hashes.len();
        self.width = o.width;
        self.rect_changed = false;
        if let Some((t, b)) = o.fixed_tb
            && (t, b) != (self.top, self.bottom)
        {
            self.top = t;
            self.bottom = b;
            self.rect_changed = true;
        }
        self.bottom_floor = o.bottom_floor;
        if o.fixed_tb.is_none() && self.bottom < o.bottom_floor {
            self.bottom = o.bottom_floor;
            self.rect_changed = true;
        }
        if o.side_pending && self.rows.len() == h {
            // The content columns are in doubt: this screen is not read.
            return Outcome::Same;
        }
        if self.rows.len() != h {
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            if self.rejoin(o) {
                return Outcome::Jump;
            }
            self.estimate(o, true);
            self.hint(o, true, 0);
            return Outcome::Edit;
        }
        if self.rows.iter().zip(o.hashes).all(|(r, h)| r.hash == *h) {
            self.estimate(o, false);
            self.hint(o, false, 0);
            return Outcome::Same;
        }
        let out = self.classify(o);
        if matches!(out, Outcome::Lost | Outcome::Other) {
            return out;
        }
        self.estimate(o, false);
        let dy = match out {
            Outcome::Scroll { dy, .. } => dy,
            _ => 0,
        };
        if self.hint(o, matches!(out, Outcome::Jump), dy) {
            return Outcome::Jump;
        }
        out
    }

    #[allow(clippy::needless_range_loop)]
    fn classify(&mut self, o: &Obs) -> Outcome {
        let h = o.hashes.len();
        let nh = o.hashes;
        let ph: Vec<u64> = self.rows.iter().map(|r| r.hash).collect();
        let (a, b) = self.band(h);
        let nb = nh[a..b].iter().filter(|x| **x != BLANK).count();
        let prev_nb = ph[a..b].iter().filter(|x| **x != BLANK).count();
        let mut seen: HashMap<u64, u32> = HashMap::new();
        for x in &nh[a..b] {
            *seen.entry(*x).or_default() += 1;
        }
        let weight = |x: u64| if seen.get(&x) == Some(&1) { 2usize } else { 1 };
        // (count, weighted) of the non-blank band rows aligned by `dy`.
        let score = |dy: i64| {
            let (mut c, mut w) = (0usize, 0usize);
            for i in a..b {
                let j = i as i64 + dy;
                if nh[i] != BLANK && j >= a as i64 && j < b as i64 && nh[i] == ph[j as usize] {
                    c += 1;
                    w += weight(nh[i]);
                }
            }
            (c, w)
        };
        let (c0, w0) = score(0);
        let erased = self.erased.take();

        // A jump fed in pieces: the rows are looked up, or lost. A status
        // row places it below.
        if o.overfed && o.hint_oy.is_none() {
            if let Some((oy, votes)) = self.lookup(nh, None)
                && votes >= need(nb, 30, 2)
            {
                self.oy = oy;
                self.enter_island(h);
                self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
                self.mark_known(a, b);
                return Outcome::Jump;
            }
            return Outcome::Lost;
        }

        // 1. Scroll.
        let span = (b - a) as i64;
        let mut best: Option<(i64, usize, usize)> = None;
        for d in 1..span {
            for dy in [d, -d] {
                let (c, w) = score(dy);
                if best.is_none_or(|(_, _, bw)| w > bw) {
                    best = Some((dy, c, w));
                }
            }
        }
        let mut dy = None;
        if let Some((d, c, w)) = best
            && c >= need(nb, 30, 2)
            && w * 2 > w0 * 3
        {
            dy = Some(d);
        }
        if o.seed != 0 && o.seed.abs() < span && dy != Some(o.seed) {
            let (c, w) = score(o.seed);
            if c >= 1 && w >= w0 && best.is_none_or(|(_, _, bw)| w * 2 >= bw) {
                dy = Some(o.seed);
            }
        }
        if let Some(dy) = dy {
            self.apply_scroll(o, &ph, dy);
            return Outcome::Scroll { dx: 0, dy };
        }
        // No row is where it was, nothing to align, but the distance is
        // known: the main screen scrolled into the scrollback (a page or
        // more of output at once), or the screen was fed as a tape (a
        // pager's page down or up). A pager paints a jump by a clear, by
        // cursor addressing, or by feeding a whole screen or more: none
        // of these is a tape feed.
        if o.seed != 0 && (o.seed_real || o.seed_tape) && nb >= 1 && c0 * 5 < nb * 2 {
            if o.seed_tape {
                let mut was = ph.clone();
                if o.seed > 0 && was[h - 1] == BLANK {
                    was[h - 1] = erased.unwrap_or(BLANK);
                }
                self.learn_tape_chrome(o, &was);
            }
            let (a, b) = self.band(h);
            self.flush(o.now.saturating_sub(FEED_GAP) + Duration::from_nanos(1));
            self.oy += o.seed;
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            self.mark_known(a, b);
            self.merge_islands(o);
            return Outcome::Scroll { dx: 0, dy: o.seed };
        }

        // 2. Splice.
        if let Some(out) = self.splice(o, &ph, nb, c0) {
            return out;
        }

        // 3. Horizontal scroll.
        let changed = (a..b)
            .filter(|i| nh[*i] != ph[*i] && (nh[*i] != BLANK || ph[*i] != BLANK))
            .count();
        if nb >= 2
            && !o.side_pending
            && changed * 2 >= nb
            && let Some(dx) = self.hscroll(o, a, b)
        {
            for i in a..b {
                if nh[i] != ph[i] {
                    self.rows[i] = self.fresh(o, i);
                }
            }
            for i in (0..a).chain(b..h) {
                if nh[i] != ph[i] {
                    self.rows[i] = self.fresh(o, i);
                }
            }
            self.ox += dx;
            return Outcome::Scroll { dx, dy: 0 };
        }

        // 3b. Both ways at once: the view moved down or up and the app
        // re-centred it sideways (a cursor line shorter than the offset).
        if nb >= 2
            && !o.side_pending
            && c0 * 5 < nb * 2
            && let Some((dx, dy)) = self.diagonal(o, a, b)
        {
            self.flush(o.now);
            self.ox += dx;
            self.oy += dy;
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            self.merge_islands(o);
            return Outcome::Scroll { dx, dy };
        }

        // 4. Jump.
        if nb <= 1 && prev_nb >= 3 && c0 == 0 {
            // The pane was wiped: what was there is kept, elsewhere.
            self.island();
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            return Outcome::Jump;
        }
        if prev_nb <= 1 && nb >= 2 && self.rejoin(o) {
            return Outcome::Jump;
        }
        // A jump replaces what was there. Lines added to a pane that had
        // little (a command and its output) are an edit.
        let replaced = (a..b)
            .filter(|i| ph[*i] != BLANK && nh[*i] != ph[*i])
            .count();
        if nb >= 2 && c0 * 5 < nb * 2 && replaced >= 2 && c0 * 5 < prev_nb * 2 {
            if let Some(oy) = o.hint_oy {
                let (mut known, mut same) = (0usize, 0usize);
                for i in a..b {
                    if let Some(s) = self.lines.get(&(oy + i as i64))
                        && s.hash != BLANK
                    {
                        known += 1;
                        same += (s.hash == nh[i]) as usize;
                    }
                }
                if known >= 3 && same * 5 < known {
                    return Outcome::Other;
                }
                self.oy = oy;
                self.absolute = true;
                self.enter_island(h);
            } else if let Some((oy, votes)) = self.lookup(nh, None)
                && votes >= need(nb, 30, 2)
            {
                self.oy = oy;
                self.enter_island(h);
            } else if self.lines.is_empty() {
                // Nothing remembered: nothing to keep apart.
                self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
                return Outcome::Edit;
            } else {
                return Outcome::Lost;
            }
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            self.mark_known(a, b);
            return Outcome::Jump;
        }

        // 5. Edit.
        if nh[h - 1] == BLANK && ph[h - 1] != BLANK && nh[..h - 1] == ph[..h - 1] {
            self.erased = Some(ph[h - 1]);
        }
        for i in 0..h {
            if nh[i] != ph[i] {
                self.rows[i] = self.fresh(o, i);
            }
        }
        Outcome::Edit
    }

    /// Take the screen as an edit of the rows in view, whatever moved: the
    /// pane repaints itself.
    pub fn rewrite(&mut self, o: &Obs) -> Outcome {
        let h = o.hashes.len();
        self.width = o.width;
        self.rect_changed = false;
        if self.rows.len() != h {
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            return Outcome::Edit;
        }
        let mut out = Outcome::Same;
        for i in 0..h {
            if o.hashes[i] != self.rows[i].hash {
                self.rows[i] = self.fresh(o, i);
                out = Outcome::Edit;
            }
        }
        out
    }

    /// View rows equal to what is remembered at their place need no
    /// commit.
    fn mark_known(&mut self, a: usize, b: usize) {
        for i in a..b {
            let row = &self.rows[i];
            if self
                .lines
                .get(&(self.oy + i as i64))
                .is_some_and(|s| s.hash == row.hash && s.x0 == self.ox)
            {
                self.rows[i].committed = true;
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn apply_scroll(&mut self, o: &Obs, ph: &[u64], dy: i64) {
        let h = ph.len();
        let nh = o.hashes;
        let (a, b) = self.band(h);
        let inb = |i: i64| i >= a as i64 && i < b as i64;
        // Rows that leave the view are remembered on the way out.
        for j in a..b {
            if !inb(j as i64 - dy) && self.rows[j].since + FEED_GAP <= o.now {
                self.commit_row(j);
            }
        }
        let old = std::mem::take(&mut self.rows);
        let mut rows = Vec::with_capacity(h);
        for i in 0..h {
            let j = i as i64 + dy;
            if inb(i as i64) && inb(j) && nh[i] == ph[j as usize] {
                rows.push(old[j as usize].clone());
            } else if nh[i] == ph[i] {
                let mut r = old[i].clone();
                // Same text, another frame row.
                r.committed &= !inb(i as i64);
                rows.push(r);
            } else {
                rows.push(self.fresh(o, i));
            }
        }
        self.rows = rows;
        self.oy += dy;
        self.learn_chrome(o, ph, dy);
        self.merge_islands(o);
    }

    /// Rows outside the run that moved are chrome.
    fn learn_chrome(&mut self, o: &Obs, ph: &[u64], dy: i64) {
        if o.fixed_tb.is_some() {
            return;
        }
        let h = ph.len() as i64;
        let nh = o.hashes;
        let moved = |i: i64| {
            let j = i + dy;
            j >= 0 && j < h && nh[i as usize] == ph[j as usize]
        };
        let Some(first) = (0..h).find(|i| moved(*i)) else {
            return;
        };
        let last = (0..h).rev().find(|i| moved(*i)).unwrap();
        let lead_end = if dy < 0 { first + dy } else { first };
        let tail_start = if dy > 0 { last + dy + 1 } else { last + 1 };
        let top = (0..lead_end.max(0))
            .rev()
            .find(|i| nh[*i as usize] != BLANK)
            .map_or(0, |i| i + 1);
        let bottom = (tail_start.min(h)..h)
            .find(|i| nh[*i as usize] != BLANK)
            .map_or(0, |i| h - i);
        // Blank rows under the rows that came in say nothing: a prompt
        // erased and not yet written again (a page fed in two bursts).
        // The last vote holds.
        let bottom = if bottom == 0 && tail_start < h {
            self.chrome_obs
                .back()
                .map_or(self.bottom as i64, |v| v.1 as i64)
                .min(h - tail_start)
        } else {
            bottom
        };
        // The first rows the emulator scrolled are content, moved or
        // repainted. The last may be a prompt written over.
        let top = match o.seed_rows {
            Some((a, _)) if dy == o.seed => top.min(a as i64),
            _ => top,
        };
        self.chrome_vote(top as u16, bottom as u16);
    }

    /// Chrome under a tape feed: the last rows did not travel with the
    /// rest, they were written over (a prompt). Fed forward, they are
    /// the last rows of the screen before; fed backward, the last rows
    /// of the screen after. A blank row says nothing.
    fn learn_tape_chrome(&mut self, o: &Obs, ph: &[u64]) {
        if o.fixed_tb.is_some() {
            return;
        }
        let nh = o.hashes;
        let h = ph.len();
        let (was, is) = if o.seed > 0 { (ph, nh) } else { (nh, ph) };
        if was[h - 1] == BLANK || o.seed.unsigned_abs() as usize >= h {
            return;
        }
        let bottom = (0..h)
            .rev()
            .take_while(|i| {
                let to = *i as i64 - o.seed.abs();
                to >= self.top as i64 && was[*i] != BLANK && is[to as usize] != was[*i]
            })
            .count();
        // On the alternate screen the first page tells: nothing is
        // printed there that a prompt could be taken for.
        if o.alt && self.chrome_obs.is_empty() && bottom > 0 {
            self.chrome_vote(self.top, bottom as u16);
        }
        self.chrome_vote(self.top, bottom as u16);
    }

    /// One observation of the chrome rows; the least seen lately holds.
    fn chrome_vote(&mut self, top: u16, bottom: u16) {
        let h = self.rows.len() as i64;
        self.chrome_obs.push_back((top, bottom));
        if self.chrome_obs.len() > 4 {
            self.chrome_obs.pop_front();
        }
        if self.chrome_obs.len() < 2 {
            return;
        }
        let t = self.chrome_obs.iter().map(|x| x.0).min().unwrap();
        let bt = self.chrome_obs.iter().map(|x| x.1).min().unwrap();
        let bt = bt.max(self.bottom_floor);
        if (t, bt) == (self.top, self.bottom) || (t + bt) as i64 >= h {
            return;
        }
        self.top = t;
        self.bottom = bt;
        self.rect_changed = true;
        // Chrome rows remembered as content before they were known.
        let (a, b) = self.band(h as usize);
        for i in (0..a).chain(b..h as usize) {
            let hash = self.rows[i].hash;
            if let Some(ys) = self.index.get(&hash).cloned() {
                for y in ys {
                    self.remove_line(y);
                }
            }
        }
    }

    /// The view shows lines of another island: the two become one. The
    /// island of the lower rows keeps its coordinates (it is the older
    /// one, or the one with absolute rows); the other is translated.
    fn merge_islands(&mut self, o: &Obs) {
        if self.islands.len() < 2 || o.hint_oy.is_some() {
            return;
        }
        let Island { lo, hi, .. } = self.islands[self.cur];
        let (a, b) = self.band(o.hashes.len());
        let nb = o.hashes[a..b].iter().filter(|x| **x != BLANK).count();
        let Some((oy, votes)) = self.lookup(o.hashes, Some((lo, hi))) else {
            return;
        };
        if votes < need(nb, 50, 3) || oy == self.oy {
            return;
        }
        let (vlo, vhi) = (oy + a as i64, oy + b as i64 - 1);
        let Some(other) = self
            .islands
            .iter()
            .position(|i| vlo <= i.hi && vhi >= i.lo)
            .filter(|i| *i != self.cur)
        else {
            return;
        };
        let Island {
            lo: olo, hi: ohi, ..
        } = self.islands[other];
        self.uses += 1;
        if olo < lo {
            // The current island joins the other. The rows on screen go
            // too, remembered or not: drawings are fixed on them.
            let delta = oy - self.oy;
            let (view_lo, view_hi) = self.view_rows();
            let (lo, hi) = (lo.min(view_lo), hi.max(view_hi));
            self.shift_rows(lo, hi, delta);
            self.oy = oy;
            self.islands[other] = Island {
                lo: olo.min(lo + delta),
                hi: ohi.max(hi + delta),
                used: self.uses,
                share: self.islands[self.cur].share,
            };
            self.islands.remove(self.cur);
            self.cur = if other > self.cur { other - 1 } else { other };
        } else {
            // The other island joins the current one.
            let delta = self.oy - oy;
            self.shift_rows(olo, ohi, delta);
            self.islands[self.cur] = Island {
                lo: lo.min(olo + delta),
                hi: hi.max(ohi + delta),
                used: self.uses,
                share: self.islands[self.cur].share,
            };
            self.islands.remove(other);
            if other < self.cur {
                self.cur -= 1;
            }
        }
    }

    /// Lines inserted or deleted at a seam near the cursor.
    #[allow(clippy::needless_range_loop)]
    fn splice(&mut self, o: &Obs, ph: &[u64], nb: usize, c0: usize) -> Option<Outcome> {
        let h = ph.len();
        let nh = o.hashes;
        let (a, b) = self.band(h);
        let cursor = o.cursor_row? as i64;
        let changed = (a..b).filter(|i| nh[*i] != ph[*i]).count();
        if changed < 2 {
            return None;
        }
        let n = b - a;
        // p0[s]: non-blank rows of [a, a+s) aligned at 0.
        let mut p0 = vec![0i64; n + 1];
        for s in 0..n {
            let i = a + s;
            p0[s + 1] = p0[s] + (nh[i] != BLANK && nh[i] == ph[i]) as i64;
        }
        let t0 = p0[n];
        let mut best: Option<(i64, i64, usize, usize)> = None; // total, k, s, e
        let mut ck = vec![0i64; n + 1];
        for d in 1..n as i64 {
            for k in [d, -d] {
                for s in 0..n {
                    let i = (a + s) as i64;
                    let j = i + k;
                    let m = nh[a + s] != BLANK
                        && j >= a as i64
                        && j < b as i64
                        && nh[a + s] == ph[j as usize];
                    ck[s + 1] = ck[s] + m as i64;
                }
                let (mut top, mut top_s) = (i64::MIN, 0usize);
                for e in 0..=n {
                    let v = p0[e] - ck[e];
                    if v > top {
                        top = v;
                        top_s = e;
                    }
                    let total = top + ck[e] - p0[e] + t0;
                    let mid = ck[e] - ck[top_s];
                    if mid >= 2
                        && p0[top_s] >= 1
                        && best.is_none_or(|(bt, _, _, be)| {
                            total > bt || (total == bt && e > be && k == best.unwrap().1)
                        })
                    {
                        best = Some((total, k, top_s, e));
                    }
                }
            }
        }
        let (total, k, s, e) = best?;
        if (total as usize) < need(nb, 50, 3) || total as usize <= c0 + 1 {
            return None;
        }
        let (s, e) = (a + s, a + e);
        let count = k.unsigned_abs() as i64;
        if cursor < s as i64 - 1 || cursor > s as i64 + count {
            return None;
        }
        let seam = self.oy + s as i64;
        if k < 0 {
            // Insertion of `count` lines at the seam.
            for j in (e as i64 - count).max(s as i64)..e as i64 {
                if self.rows[j as usize].since < o.now {
                    self.commit_row(j as usize);
                }
            }
            self.shift_rows(seam, self.island_limit(), count);
        } else {
            for y in seam..seam + count {
                self.remove_line(y);
            }
            self.shift_rows(seam + count, self.island_limit(), -count);
        }
        let old = std::mem::take(&mut self.rows);
        let mut rows = Vec::with_capacity(h);
        for i in 0..h {
            let j = i as i64 + k;
            if i >= s && i < e && j >= s as i64 && j < e as i64 && nh[i] == ph[j as usize] {
                rows.push(old[j as usize].clone());
            } else if nh[i] == ph[i] && (i < s || i >= e) {
                rows.push(old[i].clone());
            } else {
                rows.push(self.fresh(o, i));
            }
        }
        self.rows = rows;
        Some(Outcome::Splice { from: seam, dy: -k })
    }

    /// Horizontal shift that explains the changed rows: `new[c] ==
    /// old[c + dx]`.
    fn hscroll(&self, o: &Obs, a: usize, b: usize) -> Option<i64> {
        let w = o.width as i64;
        let news: Vec<Line> = (a..b).map(|i| (o.line)(i)).collect();
        let mut pairs: Vec<(Vec<char>, Vec<char>)> = Vec::new();
        for (k, new) in news.iter().enumerate() {
            let old = &self.rows[a + k].line;
            if !old.trivial() || !new.trivial() {
                continue;
            }
            if old.text.is_empty() && new.text.is_empty() {
                continue;
            }
            pairs.push((old.text.chars().collect(), new.text.chars().collect()));
        }
        if pairs.len() < 2 {
            return None;
        }
        // Does `dx` explain the pair; with a non-blank overlap?
        let fits = |p: &(Vec<char>, Vec<char>), dx: i64| -> (bool, bool) {
            let at = |v: &Vec<char>, c: i64| v.get(c as usize).copied().unwrap_or(' ');
            let mut solid = false;
            for c in dx.min(0).abs()..w - dx.max(0) {
                let (x, y) = (at(&p.1, c), at(&p.0, c + dx));
                if x != y {
                    return (false, false);
                }
                solid |= x != ' ';
            }
            (true, solid)
        };
        let mut order: Vec<usize> = (0..pairs.len()).collect();
        order.sort_by_key(|i| std::cmp::Reverse(pairs[*i].0.len() + pairs[*i].1.len()));
        let samples = &order[..order.len().min(3)];
        for d in 1..w {
            for dx in [d, -d] {
                if !samples.iter().all(|i| fits(&pairs[*i], dx) == (true, true)) {
                    continue;
                }
                let ok = pairs.iter().filter(|p| fits(p, dx) == (true, true)).count();
                let bad = pairs.iter().filter(|p| !fits(p, dx).0).count();
                if ok >= 2 && ok * 10 >= pairs.len() * 6 && bad * 5 <= pairs.len() {
                    return Some(dx);
                }
            }
        }
        None
    }

    /// A vertical and a horizontal shift that together explain the view:
    /// `new[i][c] == old[i + dy][c + dx]`.
    fn diagonal(&self, o: &Obs, a: usize, b: usize) -> Option<(i64, i64)> {
        let w = o.width as i64;
        let span = (b - a) as i64;
        let olds: Vec<Option<Vec<char>>> = (a..b)
            .map(|i| {
                let l = &self.rows[i].line;
                (l.trivial() && !l.text.is_empty()).then(|| l.text.chars().collect())
            })
            .collect();
        let news: Vec<Option<Vec<char>>> = (a..b)
            .map(|i| {
                let l = (o.line)(i);
                (l.trivial() && !l.text.is_empty()).then(|| l.text.chars().collect())
            })
            .collect();
        let dys: Vec<i64> = if o.seed != 0 && o.seed.abs() < span {
            vec![o.seed]
        } else {
            (1..span).flat_map(|d| [d, -d]).collect()
        };
        for dy in dys {
            let pairs: Vec<(&Vec<char>, &Vec<char>)> = (0..span)
                .filter_map(|k| {
                    let j = k + dy;
                    if j < 0 || j >= span {
                        return None;
                    }
                    Some((olds[j as usize].as_ref()?, news[k as usize].as_ref()?))
                })
                .collect();
            if pairs.len() < 3 {
                continue;
            }
            // Candidate shifts: where a piece of the longest new row is
            // found in its old row.
            let (old, new) = pairs.iter().max_by_key(|p| p.1.len())?;
            // Three pieces: what entered on one side is not in the old row.
            let first = new.iter().position(|c| *c != ' ').unwrap_or(0);
            let last = new.len().saturating_sub(12).max(first);
            for from in [first, (first + last) / 2, last] {
                let key = &new[from..(from + 12).min(new.len())];
                if key.len() < 4 || key.iter().all(|c| *c == ' ') {
                    continue;
                }
                for at in 0..old.len().saturating_sub(key.len() - 1) {
                    if &old[at..at + key.len()] != key {
                        continue;
                    }
                    let dx = at as i64 - from as i64;
                    if dx == 0 {
                        continue;
                    }
                    let ok = pairs
                        .iter()
                        .filter(|p| fits(p.0, p.1, dx, w) == (true, true))
                        .count();
                    let bad = pairs.iter().filter(|p| !fits(p.0, p.1, dx, w).0).count();
                    if ok >= 3 && ok * 10 >= pairs.len() * 6 && bad * 5 <= pairs.len() {
                        return Some((dx, dy));
                    }
                }
            }
        }
        None
    }

    /// Reconcile with the absolute position the app shows, after a
    /// scroll by `dy`. True when the scroll was in fact a jump.
    fn hint(&mut self, o: &Obs, jumped: bool, dy: i64) -> bool {
        if let Some(ox) = o.hint_ox {
            self.ox = ox;
        }
        let Some(oy) = o.hint_oy else { return false };
        if oy == self.oy {
            self.absolute = true;
            return false;
        }
        let delta = oy - self.oy;
        if self.absolute && !jumped {
            let said = delta + dy;
            let short =
                dy == 0 || said == 0 || (said.signum() == dy.signum() && said.abs() < dy.abs());
            if short && o.hint_soft {
                // The number does not count screen rows (wrapped lines).
                self.hint_refused = true;
                return false;
            }
            if !short {
                // Further than the rows moved, or the other way: a jump
                // painted as a feed, in more than one burst.
                let h = self.rows.len();
                let (a, b) = self.band(h);
                self.oy = oy;
                self.enter_island(h);
                for row in &mut self.rows {
                    row.committed = false;
                }
                self.mark_known(a, b);
                return true;
            }
        }
        if !self.absolute {
            // First sight of absolute rows: the whole buffer is renumbered.
            let (lo, hi) = self.extent_all();
            let cur = self.islands[self.cur];
            self.shift_rows(lo, hi, delta);
            for (i, isl) in self.islands.iter_mut().enumerate() {
                let was = if i == self.cur { cur } else { *isl };
                isl.lo = was.lo + delta;
                isl.hi = was.hi + delta;
            }
        } else if !jumped {
            // Lines were added or removed above the view.
            let (a, _) = self.band(self.rows.len());
            let from = self.oy + a as i64;
            if delta < 0 {
                for y in from + delta..from {
                    self.remove_line(y);
                }
            }
            self.shift_rows(from, self.island_limit(), delta);
        }
        self.oy = oy;
        self.absolute = true;
        false
    }
}

/// Merge a new view row at `x0` (authoritative over `width` columns) into a
/// stored line. Both trivial.
fn merge(old: &Stored, x0: i64, new: &Line, width: u16) -> (i64, String) {
    let o: Vec<char> = old.line.text.chars().collect();
    let n: Vec<char> = new.text.chars().collect();
    let start = old.x0.min(x0);
    let end = (old.x0 + o.len() as i64).max(x0 + n.len() as i64);
    let mut text = String::new();
    for x in start..end {
        let ch = if x >= x0 && x < x0 + width as i64 {
            n.get((x - x0) as usize).copied().unwrap_or(' ')
        } else if x >= old.x0 {
            o.get((x - old.x0) as usize).copied().unwrap_or(' ')
        } else {
            ' '
        };
        text.push(ch);
    }
    text.truncate(text.trim_end_matches(' ').len());
    (start, text)
}

/// Does a horizontal shift `dx` explain `new` from `old` over a view of
/// `w` columns (`new[c] == old[c + dx]`), and with a non-blank overlap?
fn fits(old: &[char], new: &[char], dx: i64, w: i64) -> (bool, bool) {
    let at = |v: &[char], c: i64| v.get(c as usize).copied().unwrap_or(' ');
    let mut solid = false;
    for c in dx.min(0).abs()..w - dx.max(0) {
        let (x, y) = (at(new, c), at(old, c + dx));
        if x != y {
            return (false, false);
        }
        solid |= x != ' ';
    }
    (true, solid)
}
