// SPDX-License-Identifier: GPL-3.0-or-later
//! Maximal rectangles of open cells.

use ptyrc_vt::Rect;

/// Every maximal rectangle of `true` cells of a `w x h` mask (row-major),
/// each once. Histogram and stack, row by row: O(w * h).
///
/// A rectangle popped off the stack cannot grow left, right nor up; it
/// is kept when it cannot grow down either: a cell of the next row under
/// it is closed, or it stands on the last row.
pub(crate) fn rooms(open: &[bool], w: usize, h: usize) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut heights = vec![0u16; w];
    // Closed cells of the next row, as prefix sums.
    let mut closed = vec![0u32; w + 1];
    let mut stack: Vec<(usize, u16)> = Vec::with_capacity(w);
    for y in 0..h {
        let row = &open[y * w..(y + 1) * w];
        for (hgt, &o) in heights.iter_mut().zip(row) {
            *hgt = if o { *hgt + 1 } else { 0 };
        }
        let last = y + 1 == h;
        if !last {
            let next = &open[(y + 1) * w..(y + 2) * w];
            for x in 0..w {
                closed[x + 1] = closed[x] + (!next[x]) as u32;
            }
        }
        stack.clear();
        for x in 0..=w {
            let cur = if x < w { heights[x] } else { 0 };
            let mut start = x;
            while let Some(&(s, hh)) = stack.last() {
                if hh < cur {
                    break;
                }
                stack.pop();
                if hh > cur && (last || closed[x] > closed[s]) {
                    out.push(Rect::new(
                        s as u16,
                        (y + 1 - hh as usize) as u16,
                        (x - s) as u16,
                        hh,
                    ));
                }
                start = s;
            }
            if cur > 0 {
                stack.push((start, cur));
            }
        }
    }
    out
}
