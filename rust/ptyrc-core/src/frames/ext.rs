// SPDX-License-Identifier: GPL-3.0-or-later
//! Extended frame: a sparse buffer of remembered lines and the view on it.
//! Scroll, splice, jump and edit detection from row hashes.

use super::screen::{BLANK, Line};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

/// Distance between islands, in rows.
pub(crate) const ISLAND_GAP: i64 = 100_000;

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
}

/// A change of the remembered buffer the scene must follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shift {
    pub from: i64,
    pub to: i64,
    pub dy: i64,
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
    /// Cursor row inside the pane.
    pub cursor_row: Option<usize>,
    /// Absolute frame row of pane row 0, when the app shows it.
    pub hint_oy: Option<i64>,
    pub hint_ox: Option<i64>,
    /// The side chrome is about to change: a horizontal move of the
    /// content is not a scroll.
    pub side_pending: bool,
    /// Top and bottom chrome imposed by a plugin.
    pub fixed_tb: Option<(u16, u16)>,
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
    islands: Vec<(i64, i64)>,
    cur: usize,
    width: u16,
    /// Set when `top` or `bottom` changed in the last observation.
    pub rect_changed: bool,
    pub shifts: Vec<Shift>,
    pub max_lines: usize,
}

fn need(nb: usize, percent: usize, floor: usize) -> usize {
    floor.max((nb * percent).div_ceil(100))
}

impl Ext {
    pub fn new(max_lines: usize) -> Ext {
        Ext {
            islands: vec![(0, 0)],
            max_lines,
            ..Ext::default()
        }
    }

    fn band(&self, h: usize) -> (usize, usize) {
        let (t, b) = (self.top as usize, self.bottom as usize);
        if t + b >= h { (0, h) } else { (t, h - b) }
    }

    /// Known rows: `(min, max)`.
    pub fn extent(&self) -> (i64, i64) {
        let (a, b) = self.band(self.rows.len());
        let (mut lo, mut hi) = (self.oy + a as i64, self.oy + (b as i64 - 1).max(a as i64));
        if let Some((k, _)) = self.lines.first_key_value() {
            lo = lo.min(*k);
        }
        if let Some((k, _)) = self.lines.last_key_value() {
            hi = hi.max(*k);
        }
        (lo, hi)
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
        isl.0 = isl.0.min(y);
        isl.1 = isl.1.max(y);
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
        self.shifts.push(Shift { from, to, dy });
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
        let (_, hi) = self.extent();
        self.oy = (hi.max(0) / ISLAND_GAP + 1) * ISLAND_GAP;
        self.islands.push((self.oy, self.oy));
        self.cur = self.islands.len() - 1;
        self.absolute = false;
        self.rows.clear();
    }

    fn enter_island(&mut self, h: usize) {
        let (lo, hi) = (self.oy, self.oy + h as i64);
        match self.islands.iter().position(|(a, b)| lo <= *b && hi >= *a) {
            Some(i) => self.cur = i,
            None => {
                self.islands.push((lo, lo));
                self.cur = self.islands.len() - 1;
            }
        }
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
        if self.rows.len() != h {
            self.rows = (0..h).map(|i| self.fresh(o, i)).collect();
            self.hint(o, true);
            return Outcome::Edit;
        }
        if self.rows.iter().zip(o.hashes).all(|(r, h)| r.hash == *h) {
            self.hint(o, false);
            return Outcome::Same;
        }
        let out = self.classify(o);
        if out != Outcome::Lost {
            self.hint(o, matches!(out, Outcome::Jump));
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
        if nb >= 2 && c0 * 5 < nb * 2 {
            if let Some(oy) = o.hint_oy {
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
            // What matched is already remembered.
            for i in a..b {
                if self
                    .lines
                    .get(&(self.oy + i as i64))
                    .is_some_and(|s| s.hash == nh[i])
                {
                    self.rows[i].committed = true;
                }
            }
            return Outcome::Jump;
        }

        // 5. Edit.
        for i in 0..h {
            if nh[i] != ph[i] {
                self.rows[i] = self.fresh(o, i);
            }
        }
        Outcome::Edit
    }

    #[allow(clippy::needless_range_loop)]
    fn apply_scroll(&mut self, o: &Obs, ph: &[u64], dy: i64) {
        let h = ph.len();
        let nh = o.hashes;
        let (a, b) = self.band(h);
        let inb = |i: i64| i >= a as i64 && i < b as i64;
        // Rows that leave the view are remembered on the way out.
        for j in a..b {
            if !inb(j as i64 - dy) && self.rows[j].since < o.now {
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
        self.chrome_obs.push_back((top as u16, bottom as u16));
        if self.chrome_obs.len() > 4 {
            self.chrome_obs.pop_front();
        }
        if self.chrome_obs.len() < 2 {
            return;
        }
        let t = self.chrome_obs.iter().map(|x| x.0).min().unwrap();
        let bt = self.chrome_obs.iter().map(|x| x.1).min().unwrap();
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
        let (lo, hi) = self.islands[self.cur];
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
            .position(|(x, y)| vlo <= *y && vhi >= *x)
            .filter(|i| *i != self.cur)
        else {
            return;
        };
        let (olo, ohi) = self.islands[other];
        if olo < lo {
            // The current island joins the other.
            let delta = oy - self.oy;
            self.shift_rows(lo, hi, delta);
            self.oy = oy;
            self.islands[other] = (olo.min(lo + delta), ohi.max(hi + delta));
            self.islands.remove(self.cur);
            self.cur = if other > self.cur { other - 1 } else { other };
        } else {
            // The other island joins the current one.
            let delta = self.oy - oy;
            self.shift_rows(olo, ohi, delta);
            self.islands[self.cur] = (lo.min(olo + delta), hi.max(ohi + delta));
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
            self.shift_rows(seam, i64::MAX - ISLAND_GAP, count);
        } else {
            for y in seam..seam + count {
                self.remove_line(y);
            }
            self.shift_rows(seam + count, i64::MAX - ISLAND_GAP, -count);
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

    /// Reconcile with the absolute position the app shows.
    fn hint(&mut self, o: &Obs, jumped: bool) {
        if let Some(ox) = o.hint_ox {
            self.ox = ox;
        }
        let Some(oy) = o.hint_oy else { return };
        if oy == self.oy {
            self.absolute = true;
            return;
        }
        let delta = oy - self.oy;
        if !self.absolute {
            // First sight of absolute rows: the whole buffer is renumbered.
            let (lo, hi) = self.extent();
            self.shift_rows(lo, hi, delta);
            for isl in &mut self.islands {
                isl.0 += delta;
                isl.1 += delta;
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
            self.shift_rows(from, i64::MAX - ISLAND_GAP, delta);
        }
        self.oy = oy;
        self.absolute = true;
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
