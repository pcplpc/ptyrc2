// SPDX-License-Identifier: GPL-3.0-or-later
//! Safe heatmap and placement.

use super::screen::Screen;
use crate::types::Prefer;
use ptyrc_vt::Rect;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TAU_FAST: f32 = 2.0;
const TAU_SLOW: f32 = 60.0;
const TAU_OCC: f32 = 5.0;
/// Weight of the preferred position against a mean `safe` of 1.
const BIAS: f32 = 0.3;
/// A cell is static after this long without a visible change.
pub const IDLE_SECS: f32 = 10.0;
/// A frame is idle from this share of static cells.
pub const IDLE_SHARE: f32 = 0.8;
/// A change undone within this delay never happened: a clear and the
/// identical redraw that follows it.
const GRACE: Duration = Duration::from_millis(150);
/// Backdrops counted when looking for the dominant one.
const BACKDROPS: usize = 32;
const STILL_MAX: f32 = 86_400.0;

/// What a cell of a frame is, for a client that wants to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeatClass {
    /// Changed on screen within `IDLE_SECS`.
    Hot,
    /// Foreground that does not move.
    Text,
    /// Background of another colour than the frame's.
    Panel,
    /// Background of the frame's colour.
    #[default]
    Free,
}

impl HeatClass {
    /// `h`, `t`, `p`, `.`.
    pub fn letter(self) -> char {
        match self {
            HeatClass::Hot => 'h',
            HeatClass::Text => 't',
            HeatClass::Panel => 'p',
            HeatClass::Free => '.',
        }
    }
}

/// The state of a cell before a change that may still be undone.
#[derive(Clone, Copy, Debug)]
struct Saved {
    look: u32,
    at: Duration,
    fast: f32,
    slow: f32,
    still: f32,
}

/// Per cell of a pane: activity (two time constants), occupancy, time
/// without change, class.
#[derive(Clone, Debug, Default)]
pub(crate) struct Heat {
    w: u16,
    h: u16,
    fast: Vec<f32>,
    slow: Vec<f32>,
    occ: Vec<f32>,
    /// Seconds without a visible change, or since the cell is watched.
    still: Vec<f32>,
    /// What the cell showed at the last update.
    look: Vec<u32>,
    /// `Text`, `Panel` or `Free`, at the last update.
    kind: Vec<HeatClass>,
    saved: Vec<Option<Saved>>,
    /// Dominant backdrop of the pane, as a colour code.
    backdrop: u32,
    last: Option<Duration>,
}

impl Heat {
    /// Account for the screen as scanned. An event is a cell that does not
    /// look as it did at the last update.
    pub fn update(&mut self, screen: &Screen, rect: Rect, now: Duration) {
        let n = rect.w as usize * rect.h as usize;
        let fresh = self.w != rect.w || self.h != rect.h || self.last.is_none();
        if fresh {
            self.w = rect.w;
            self.h = rect.h;
            self.fast = vec![0.0; n];
            self.slow = vec![0.0; n];
            self.occ = vec![0.0; n];
            self.still = vec![0.0; n];
            self.look = vec![0; n];
            self.kind = vec![HeatClass::Free; n];
            self.saved = vec![None; n];
        }
        let reclass = fresh || screen.any_change;
        if reclass {
            self.backdrop = dominant(screen, rect, self.backdrop);
        }
        let dt = self
            .last
            .map_or(0.0, |l| now.saturating_sub(l).as_secs_f32());
        self.last = Some(now);
        let (ff, fs, fo) = (
            (-dt / TAU_FAST).exp(),
            (-dt / TAU_SLOW).exp(),
            1.0 - (-dt / TAU_OCC).exp(),
        );
        for r in 0..rect.h {
            let base = screen.idx(rect.x, rect.y + r);
            let own = r as usize * rect.w as usize;
            for c in 0..rect.w as usize {
                let (i, s) = (own + c, base + c);
                let ink = screen.ink[s];
                if reclass {
                    self.kind[i] = if ink {
                        HeatClass::Text
                    } else if screen.back[s] != self.backdrop {
                        HeatClass::Panel
                    } else {
                        HeatClass::Free
                    };
                }
                let occupied = if ink { 1.0 } else { 0.0 };
                let look = screen.look[s];
                if fresh {
                    self.occ[i] = occupied;
                    self.look[i] = look;
                    continue;
                }
                self.fast[i] *= ff;
                self.slow[i] *= fs;
                self.occ[i] += (occupied - self.occ[i]) * fo;
                self.still[i] = (self.still[i] + dt).min(STILL_MAX);
                if look == self.look[i] {
                    continue;
                }
                let pending = self.saved[i].filter(|sv| now.saturating_sub(sv.at) <= GRACE);
                match pending {
                    Some(sv) if sv.look == look => {
                        // Back to what it was: the change never happened.
                        self.fast[i] = sv.fast;
                        self.slow[i] = sv.slow;
                        self.still[i] = sv.still + now.saturating_sub(sv.at).as_secs_f32();
                        self.saved[i] = None;
                    }
                    Some(_) => self.still[i] = 0.0,
                    None => {
                        self.saved[i] = Some(Saved {
                            look: self.look[i],
                            at: now,
                            fast: self.fast[i],
                            slow: self.slow[i],
                            still: self.still[i],
                        });
                        self.fast[i] = 1.0;
                        self.slow[i] += 0.2 * (1.0 - self.slow[i]);
                        self.still[i] = 0.0;
                    }
                }
                self.look[i] = look;
                // A cell that just filled counts as occupied at once.
                self.occ[i] = self.occ[i].max(occupied);
            }
        }
    }

    /// Mean activity of the pane.
    pub fn mean_activity(&self) -> f32 {
        if self.fast.is_empty() {
            return 0.0;
        }
        self.fast
            .iter()
            .zip(&self.slow)
            .map(|(f, s)| f.max(*s))
            .sum::<f32>()
            / self.fast.len() as f32
    }

    /// The heatmap of a sub-rectangle (pane-relative), row-major, as it
    /// stands at `now`.
    pub fn view(&self, sub: Rect, now: Duration) -> HeatView {
        let n = sub.w as usize * sub.h as usize;
        let dt = self
            .last
            .map_or(0.0, |l| now.saturating_sub(l).as_secs_f32());
        let (ff, fs, fo) = (
            (-dt / TAU_FAST).exp(),
            (-dt / TAU_SLOW).exp(),
            1.0 - (-dt / TAU_OCC).exp(),
        );
        let hot_floor = (-IDLE_SECS / TAU_FAST).exp();
        let mut view = HeatView {
            cols: sub.w,
            rows: sub.h,
            safe: Vec::with_capacity(n),
            activity: Vec::with_capacity(n),
            occupancy: Vec::with_capacity(n),
            still: Vec::with_capacity(n),
            class: Vec::with_capacity(n),
        };
        for r in sub.y..sub.bottom() {
            for c in sub.x..sub.right() {
                if c >= self.w || r >= self.h {
                    view.safe.push(0.0);
                    view.activity.push(1.0);
                    view.occupancy.push(1.0);
                    view.still.push(0.0);
                    view.class.push(HeatClass::Hot);
                    continue;
                }
                let i = r as usize * self.w as usize + c as usize;
                let kind = self.kind[i];
                let fast = self.fast[i] * ff;
                let activity = fast.max(self.slow[i] * fs);
                let target = if kind == HeatClass::Text { 1.0 } else { 0.0 };
                let occ = self.occ[i] + (target - self.occ[i]) * fo;
                view.safe.push((1.0 - activity) * (1.0 - occ));
                view.activity.push(activity);
                view.occupancy.push(occ);
                view.still.push((self.still[i] + dt).min(STILL_MAX));
                view.class.push(if fast > hot_floor {
                    HeatClass::Hot
                } else {
                    kind
                });
            }
        }
        view
    }
}

/// The backdrop most cells of `rect` have. `current` wins a tie.
fn dominant(screen: &Screen, rect: Rect, current: u32) -> u32 {
    let mut counts: Vec<(u32, u32)> = Vec::new();
    let mut at = 0usize;
    for r in 0..rect.h {
        let base = screen.idx(rect.x, rect.y + r);
        for &code in &screen.back[base..base + rect.w as usize] {
            if counts.get(at).is_none_or(|e| e.0 != code) {
                match counts.iter().position(|e| e.0 == code) {
                    Some(k) => at = k,
                    None if counts.len() < BACKDROPS => {
                        counts.push((code, 0));
                        at = counts.len() - 1;
                    }
                    None => continue,
                }
            }
            counts[at].1 += 1;
        }
    }
    counts
        .iter()
        .max_by_key(|e| (e.1, e.0 == current))
        .map_or(current, |e| e.0)
}

/// Heatmap of a frame's view, row-major, values in `0..=1`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatView {
    pub cols: u16,
    pub rows: u16,
    /// `(1 - activity) * (1 - occupancy)`.
    pub safe: Vec<f32>,
    /// Visible change, decaying.
    pub activity: Vec<f32>,
    /// Foreground, averaged over time.
    pub occupancy: Vec<f32>,
    /// Seconds without a visible change.
    pub still: Vec<f32>,
    pub class: Vec<HeatClass>,
}

/// How much of a frame is static.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Idle {
    /// Share of the cells static for `IDLE_SECS`.
    pub score: f32,
    /// `score >= IDLE_SHARE`.
    pub idle: bool,
    /// Share of the cells static and without foreground.
    pub free: f32,
}

impl HeatView {
    pub fn safe_at(&self, col: u16, row: u16) -> f32 {
        self.safe[row as usize * self.cols as usize + col as usize]
    }

    /// Classes as letters, one string per row: `h` hot, `t` text, `p`
    /// panel, `.` free.
    pub fn classes(&self) -> Vec<String> {
        self.class
            .chunks(self.cols.max(1) as usize)
            .map(|row| row.iter().map(|c| c.letter()).collect())
            .collect()
    }

    /// Share of static cells, and of static cells without foreground.
    pub fn idle(&self) -> Idle {
        let n = self.still.len();
        if n == 0 {
            return Idle::default();
        }
        let (mut calm, mut free) = (0usize, 0usize);
        for (still, class) in self.still.iter().zip(&self.class) {
            if *still >= IDLE_SECS && *class != HeatClass::Hot {
                calm += 1;
                free += (*class != HeatClass::Text) as usize;
            }
        }
        let score = calm as f32 / n as f32;
        Idle {
            score,
            idle: score >= IDLE_SHARE,
            free: free as f32 / n as f32,
        }
    }

    /// `safe` as digits 0 to 9, one string per row.
    pub fn digits(&self) -> Vec<String> {
        self.safe
            .chunks(self.cols.max(1) as usize)
            .map(|row| {
                row.iter()
                    .map(|v| (b'0' + (v * 9.0).round().clamp(0.0, 9.0) as u8) as char)
                    .collect()
            })
            .collect()
    }

    /// Mean `safe` of a rectangle and its bias towards `prefer`.
    pub fn score(&self, x: u16, y: u16, w: u16, h: u16, prefer: Prefer) -> Option<f32> {
        if w == 0 || h == 0 || x + w > self.cols || y + h > self.rows {
            return None;
        }
        let mut sum = 0.0;
        for r in y..y + h {
            let base = r as usize * self.cols as usize;
            sum += self.safe[base + x as usize..base + (x + w) as usize]
                .iter()
                .sum::<f32>();
        }
        Some(sum / (w as f32 * h as f32) + self.bias(x, y, w, h, prefer))
    }

    fn bias(&self, x: u16, y: u16, w: u16, h: u16, prefer: Prefer) -> f32 {
        let (fx, fy) = ((self.cols - w) as f32, (self.rows - h) as f32);
        let (tx, ty) = match prefer {
            Prefer::TopLeft => (0.0, 0.0),
            Prefer::Top => (0.5, 0.0),
            Prefer::TopRight => (1.0, 0.0),
            Prefer::Left => (0.0, 0.5),
            Prefer::Center => (0.5, 0.5),
            Prefer::Right => (1.0, 0.5),
            Prefer::BottomLeft => (0.0, 1.0),
            Prefer::Bottom => (0.5, 1.0),
            Prefer::BottomRight => (1.0, 1.0),
            Prefer::Any => return 0.0,
        };
        let dx = if fx > 0.0 {
            (x as f32 / fx - tx).abs()
        } else {
            0.0
        };
        let dy = if fy > 0.0 {
            (y as f32 / fy - ty).abs()
        } else {
            0.0
        };
        BIAS * (1.0 - (dx + dy) / 2.0)
    }

    /// Best `w x h` spot not overlapping `taken`: `(x, y, score)`.
    pub fn best(&self, w: u16, h: u16, prefer: Prefer, taken: &[Rect]) -> Option<(u16, u16, f32)> {
        if w == 0 || h == 0 || w > self.cols || h > self.rows {
            return None;
        }
        // Summed-area table.
        let (cw, ch) = (self.cols as usize + 1, self.rows as usize + 1);
        let mut sat = vec![0.0f64; cw * ch];
        for r in 0..self.rows as usize {
            let mut run = 0.0f64;
            for c in 0..self.cols as usize {
                run += self.safe[r * self.cols as usize + c] as f64;
                sat[(r + 1) * cw + c + 1] = sat[r * cw + c + 1] + run;
            }
        }
        let area = w as f64 * h as f64;
        let mut best: Option<(u16, u16, f32)> = None;
        for y in 0..=self.rows - h {
            for x in 0..=self.cols - w {
                if taken.iter().any(|t| t.intersects(&Rect::new(x, y, w, h))) {
                    continue;
                }
                let (x0, y0, x1, y1) = (x as usize, y as usize, (x + w) as usize, (y + h) as usize);
                let sum =
                    sat[y1 * cw + x1] - sat[y0 * cw + x1] - sat[y1 * cw + x0] + sat[y0 * cw + x0];
                let s = (sum / area) as f32 + self.bias(x, y, w, h, prefer);
                if best.is_none_or(|b| s > b.2 + 1e-6) {
                    best = Some((x, y, s));
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(cols: u16, rows: u16) -> HeatView {
        let n = cols as usize * rows as usize;
        HeatView {
            cols,
            rows,
            safe: vec![1.0; n],
            activity: vec![0.0; n],
            occupancy: vec![0.0; n],
            still: vec![60.0; n],
            class: vec![HeatClass::Free; n],
        }
    }

    #[test]
    fn corners() {
        let v = flat(40, 10);
        assert_eq!(
            v.best(6, 3, Prefer::TopRight, &[]).map(|b| (b.0, b.1)),
            Some((34, 0))
        );
        assert_eq!(
            v.best(6, 3, Prefer::BottomLeft, &[]).map(|b| (b.0, b.1)),
            Some((0, 7))
        );
        assert_eq!(
            v.best(6, 3, Prefer::Center, &[]).map(|b| (b.0, b.1)),
            Some((17, 3))
        );
        assert_eq!(
            v.best(6, 3, Prefer::Any, &[]).map(|b| (b.0, b.1)),
            Some((0, 0))
        );
        assert_eq!(v.best(41, 3, Prefer::Any, &[]), None);
    }

    #[test]
    fn avoids_taken_and_unsafe() {
        let mut v = flat(40, 10);
        let got = v
            .best(6, 3, Prefer::TopRight, &[Rect::new(30, 0, 10, 3)])
            .unwrap();
        assert!(!Rect::new(got.0, got.1, 6, 3).intersects(&Rect::new(30, 0, 10, 3)));
        // The top-right corner is busy: the spot moves off it.
        for r in 0..4 {
            for c in 28..40 {
                v.safe[r * 40 + c] = 0.0;
            }
        }
        let got = v.best(6, 3, Prefer::TopRight, &[]).unwrap();
        assert!(
            !Rect::new(got.0, got.1, 6, 3).intersects(&Rect::new(28, 0, 12, 4)),
            "{got:?}"
        );
        assert_eq!(v.digits()[0].len(), 40);
    }
}
