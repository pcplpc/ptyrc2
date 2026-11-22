// SPDX-License-Identifier: GPL-3.0-or-later
// The foot line of a widget that takes keys (the twin's `ptyrc.ui.foot`).

import { cells } from "ptyrc";
import { cut } from "ptyrc/deck";

/** The keys `[[text, rank], ...]` of a foot line in `w` cells: those of
 * the highest rank go first, the last of them first; rank 0 stays, cut. */
export function foot(keys, w) {
  const kept = [...keys];
  for (;;) {
    const line = kept.map(([k]) => k).join("  ");
    const worst = Math.max(0, ...kept.map(([, r]) => r));
    if (cells(line) <= w || worst === 0) return cut(line, w);
    let at = -1;
    kept.forEach(([, r], i) => r === worst && (at = i));
    kept.splice(at, 1);
  }
}
