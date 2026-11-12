// SPDX-License-Identifier: GPL-3.0-or-later
//! Safe heatmap and placement.

use super::screen::Screen;
use crate::types::Prefer;
use ptyrc_vt::Rect;
use std::time::Duration;

const TAU_FAST: f32 = 2.0;
const TAU_SLOW: f32 = 60.0;
const TAU_OCC: f32 = 5.0;
/// Weight of the preferred position against a mean `safe` of 1.
const BIAS: f32 = 0.3;

/// Per cell of a pane: activity (two time constants) and occupancy.
#[derive(Clone, Debug, Default)]
pub(crate) struct Heat {
    w: u16,
    h: u16,
    fast: Vec<f32>,
    slow: Vec<f32>,
    occ: Vec<f32>,
    last: Option<Duration>,
}

impl Heat {
    /// Account for the screen as scanned: `changed` cells are events.
    pub fn update(&mut self, screen: &Screen, rect: Rect, now: Duration, events: bool) {
        let n = rect.w as usize * rect.h as usize;
        let fresh = self.w != rect.w || self.h != rect.h || self.last.is_none();
        if fresh {
            self.w = rect.w;
            self.h = rect.h;
            self.fast = vec![0.0; n];
            self.slow = vec![0.0; n];
            self.occ = vec![0.0; n];
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
                let occupied = if screen.nonempty[s] { 1.0 } else { 0.0 };
                if fresh {
                    self.occ[i] = occupied;
                    continue;
                }
                self.fast[i] *= ff;
                self.slow[i] *= fs;
                self.occ[i] += (occupied - self.occ[i]) * fo;
                if events && screen.changed[s] {
                    self.fast[i] = 1.0;
                    self.slow[i] += 0.2 * (1.0 - self.slow[i]);
                    // A cell that just filled counts as occupied at once.
                    self.occ[i] = self.occ[i].max(occupied);
                }
            }
        }
    }

    pub fn activity(&self, i: usize) -> f32 {
        self.fast[i].max(self.slow[i])
    }

    pub fn safe(&self, i: usize) -> f32 {
        (1.0 - self.activity(i)) * (1.0 - self.occ[i])
    }

    /// Mean activity of the pane.
    pub fn mean_activity(&self) -> f32 {
        if self.fast.is_empty() {
            return 0.0;
        }
        (0..self.fast.len()).map(|i| self.activity(i)).sum::<f32>() / self.fast.len() as f32
    }

    /// `safe` over a sub-rectangle (pane-relative), row-major.
    pub fn view(&self, sub: Rect) -> HeatView {
        let mut safe = Vec::with_capacity(sub.w as usize * sub.h as usize);
        let mut activity = Vec::with_capacity(safe.capacity());
        let mut occupancy = Vec::with_capacity(safe.capacity());
        for r in sub.y..sub.bottom() {
            for c in sub.x..sub.right() {
                let known = c < self.w && r < self.h;
                let i = r as usize * self.w as usize + c as usize;
                safe.push(if known { self.safe(i) } else { 0.0 });
                activity.push(if known { self.activity(i) } else { 1.0 });
                occupancy.push(if known { self.occ[i] } else { 1.0 });
            }
        }
        HeatView {
            cols: sub.w,
            rows: sub.h,
            safe,
            activity,
            occupancy,
        }
    }
}

/// Heatmap of a frame's view, row-major, values in `0..=1`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatView {
    pub cols: u16,
    pub rows: u16,
    /// `(1 - activity) * (1 - occupancy)`.
    pub safe: Vec<f32>,
    pub activity: Vec<f32>,
    pub occupancy: Vec<f32>,
}

impl HeatView {
    pub fn safe_at(&self, col: u16, row: u16) -> f32 {
        self.safe[row as usize * self.cols as usize + col as usize]
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
