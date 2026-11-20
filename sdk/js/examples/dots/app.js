// SPDX-License-Identifier: GPL-3.0-or-later
// Dots: a curve in Braille over the whole frame, kept a cell from the text.
//
// A `Sight` keeps the shape of the view from the topics; the tick reads
// it again only when told. A cell within one of ink gets no dot. `shape`
// denied: a blank view, the curve everywhere. `--full`: every dot on,
// each tick (the cost of a frame-sized Braille, `sdk/js/README.md`).

import { Canvas, period } from "ptyrc";
import { Braille } from "ptyrc/canvas";
import { Sight } from "ptyrc/sight";

const INK = { fg: 6 };
const PLACE = { anchor: { at: [0, 0], space: "view" } };

let sight;
let canvas = null;
let dots = null;
let full = false;

/** The curve at `t` ms on `dots`: a Lissajous figure, 600 steps. */
export function curve(dots, t) {
  const w = 2 * dots.width - 1;
  const h = 4 * dots.height - 1;
  const a = t / 4000;
  let [px, py] = [null, null];
  for (let i = 0; i <= 600; i++) {
    const s = (i / 600) * 2 * Math.PI;
    const x = Math.round(((Math.sin(3 * s + a) + 1) / 2) * w);
    const y = Math.round(((Math.sin(2 * s) + 1) / 2) * h);
    if (px !== null) dots.line(px, py, x, y);
    [px, py] = [x, y];
  }
}

/** Every dot off within a cell of ink: `rows`, a row of the view a string. */
export function spare(dots, rows) {
  const { width, height, bits } = dots;
  for (let cy = 0; cy < height; cy++) {
    const row = rows[cy] ?? "";
    for (let cx = 0; cx < Math.min(width, row.length); cx++) {
      if (row.charCodeAt(cx) === 32) continue;
      for (let y = Math.max(0, cy - 1); y <= Math.min(height - 1, cy + 1); y++)
        for (let x = Math.max(0, cx - 1); x <= Math.min(width - 1, cx + 1); x++) bits[y * width + x] = 0;
    }
  }
}

export function init(args) {
  full = args.full === true;
  sight = new Sight();
  period(50);
}

export function tick(now) {
  if (!sight.look()) return;
  const [w, h] = sight.size;
  if (!w || !h) return;
  if (!dots || dots.width !== w || dots.height !== h) {
    canvas?.release();
    canvas = new Canvas(w, h);
    dots = new Braille(w, h);
  }
  dots.clear();
  if (full) {
    for (let y = 0; y < 4 * h; y++) for (let x = 0; x < 2 * w; x++) dots.set(x, y);
  } else {
    curve(dots, now);
  }
  spare(dots, sight.rows);
  dots.draw(canvas, 0, 0, INK);
  canvas.present(PLACE);
}

export function event(topic, data) {
  sight.event(topic, data);
}
