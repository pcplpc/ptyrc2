// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of braille.js (test_canvas.py: test_braille).

import { assert, assertEqual } from "ptyrc";
import { Braille, DOTS } from "./braille.js";

export function test_braille() {
  const dots = new Braille(3, 2);
  assertEqual(dots.rows(), ["   ", "   "]);
  assertEqual(dots.cells(), []);
  assertEqual([dots.glyph(0, 0), dots.glyph(9, 9)], ["", ""]);
  dots.set(0, 0);
  dots.set(1, 3);
  dots.set(5, 7);
  dots.set(6, 0); // outside: ignored
  dots.set(0, -1);
  assert(dots.get(0, 0) && dots.get(1, 3) && !dots.get(1, 0));
  assert(!dots.get(99, 0));
  assertEqual(dots.rows(), ["⢁  ", "  ⢀"]);
  assertEqual(dots.cells(), [
    [0, 0],
    [2, 1],
  ]);
  dots.set(0, 0, false);
  assertEqual(dots.glyph(0, 0), "⢀");
  const line = new Braille(2, 1);
  line.line(0, 0, 3, 3);
  assertEqual(line.rows(), ["⠑⢄"]);
  // The eight dots are the eight bits.
  for (let bits = 1; bits < 256; bits++) {
    const cell = new Braille(1, 1);
    for (let y = 0; y < 4; y++) for (let x = 0; x < 2; x++) if (bits & DOTS[y][x]) cell.set(x, y);
    assertEqual(cell.rows(), [String.fromCharCode(0x2800 + bits)]);
  }
}
