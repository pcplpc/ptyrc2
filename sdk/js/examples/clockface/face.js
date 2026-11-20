// SPDX-License-Identifier: GPL-3.0-or-later
// The faces: pure, a time in, rows or a canvas out. tests.js tests them.

import { Canvas, style } from "ptyrc";

const INK = style({ fg: 14, bold: true });
const DIM = style({ fg: 8 });

// Block digits, three cells by three rows.
const DIGITS = [
  ["█▀█", "█ █", "▀▀▀"],
  [" █ ", " █ ", " ▀ "],
  ["▀▀█", "█▀▀", "▀▀▀"],
  ["▀▀█", " ▀█", "▀▀▀"],
  ["█ █", "▀▀█", "  ▀"],
  ["█▀▀", "▀▀█", "▀▀▀"],
  ["█▀▀", "█▀█", "▀▀▀"],
  ["▀▀█", "  █", "  ▀"],
  ["█▀█", "█▀█", "▀▀▀"],
  ["█▀█", "▀▀█", "▀▀▀"],
];

/** `MM:SS` of `ms`; past an hour, the minutes go on counting. */
export function clock(ms) {
  const s = Math.floor(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

/** The sizes, largest first: the driver shows the largest that fits. */
export const SIZES = { big: [17, 3], line: [5, 1] };

let canvas;

/** The face of `ms` in `w` by `h` cells: a canvas, or one row. */
export function face(ms, w, h, paused) {
  const text = clock(ms);
  if (h < 3) return [[{ t: text, fg: paused ? 8 : 14, bold: !paused }]];
  if (!canvas || canvas.width !== w || canvas.height !== h) {
    canvas?.release();
    canvas = new Canvas(w, h);
  }
  canvas.clear();
  let x = 0;
  for (const ch of text) {
    if (ch === ":") {
      canvas.set(x, 1, "▪", paused ? DIM : INK);
      x += 2;
      continue;
    }
    const rows = DIGITS[Number(ch)];
    for (let y = 0; y < 3; y++) canvas.text(x, y, rows[y], paused ? DIM : INK);
    x += 4;
  }
  return canvas;
}
