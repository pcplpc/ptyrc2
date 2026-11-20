// SPDX-License-Identifier: GPL-3.0-or-later
// The API: ptyrc.d.ts. The rules: AGENTS.md.
import { Canvas, period, style } from "ptyrc";

const INK = style({ fg: 11, bold: true });
const WIDTH = 16;
let canvas;
let label = "t";

// An act refused in init (`denied`) ends the app unless caught: AGENTS.md, Errors.
export function init(args) {
  label = String(args.label ?? "t");
  canvas = new Canvas(WIDTH, 1);
  period(100);
  draw(0);
}

export function tick(now) {
  draw(now);
}

function draw(now) {
  const text = `${label} = ${(now / 1000).toFixed(1)} s`;
  // Right-aligned: the last cell of the canvas, at -1, the last column.
  canvas.clear().text(Math.max(0, WIDTH - text.length), 0, text, INK);
  // Behind the text, as an app that declares nothing draws.
  canvas.present({ anchor: { at: [-1, 0], space: "view" } });
}
