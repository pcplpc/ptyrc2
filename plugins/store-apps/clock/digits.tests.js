// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of digits.js (test_clock.py: test_fonts; the layout
// that test_bounds relies on).

import { assert, assertEqual } from "ptyrc";
import { LARGE, SMALL, beside, blocks, clip, decode, fit, width } from "./digits.js";

const text = (row) => row.map((r) => r.t).join("");

export function test_fonts() {
  for (const [font, w, h] of [
    [SMALL, 3, 3],
    [LARGE, 6, 5],
  ]) {
    assertEqual(new Set(font.map((g) => g.join("\n"))).size, 11);
    const digits = font.slice(0, 10);
    assert(digits.every((g) => g.length === h && g.every((r) => r.length === w)));
    const rows = Array.from({ length: h }, (_, y) => digits.map((g) => g[y]).join(" "));
    assertEqual(decode(rows, font), "0123456789");
  }
  assertEqual(SMALL[8], ["█▀█", "█▀█", "▀▀▀"]);
  assertEqual(decode(["█▀█ ?", "█▀█ ?", "▀▀▀ ?"], SMALL), "8?");
}

export function test_blocks() {
  // A column between digits, a colon's width about a colon (one here).
  const rows = blocks("1:2", SMALL, [1, 2, 3]);
  assertEqual(rows.map(text), ["▄█  ▄ ▀▀█", " █  ▄ █▀▀", "▀▀▀   ▀▀▀"]);
  assertEqual(rows[0][0], { t: "▄█ ", fg: 1 });
  assertEqual(blocks("1:2", LARGE, [1, 2, 3])[0][1], { t: "  " });
  assertEqual(decode(rows.map(text), SMALL), "1:2");
}

export function test_layout() {
  assertEqual(width([{ t: "ab" }, { t: "⣿c" }]), 4);
  assertEqual(clip([{ t: "ab", fg: 1 }, { t: "cd" }], 3), [{ t: "ab", fg: 1 }, { t: "c" }]);
  assertEqual(clip([{ t: "ab" }], 0), []);
  // Centred across and down; cut to the box.
  assertEqual(fit([[[{ t: "ab" }]]], 6, 3), [[], [{ t: "  " }, { t: "ab" }]]);
  assertEqual(fit([[[{ t: "abcdef" }], [{ t: "x" }]]], 4, 1), [[{ t: "abcd" }]]);
  // Bottom lines level, `gap` columns after the widest of the left.
  const two = beside([[{ t: "a" }], [{ t: "bbb" }]], [[{ t: "c" }]], 2);
  assertEqual(two.map(text), ["a", "bbb  c"]);
}
