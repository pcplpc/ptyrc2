// SPDX-License-Identifier: GPL-3.0-or-later
// Confetti: paper falls over the surface, behind the text.

import { call, Canvas, period, style } from "ptyrc";

const GLYPHS = ["▪", "▴", "●", "◆", "▸", "✦"];
const COLOURS = ["#ff5f5f", "#ffd75f", "#5fd75f", "#5fafff", "#d787ff", "#ff87d7"];
const STYLES = COLOURS.map((fg) => style({ fg }));
const PLACE = { anchor: { at: [0, 0], space: "view" } };

let canvas;
let pieces = [];
let rate = 12; // pieces a second
let last = 0;
let owed = 0;

function resize(size) {
  const [w, h] = size;
  if (canvas && canvas.width === w && canvas.height === h) return;
  if (canvas) canvas.release();
  canvas = new Canvas(w, h);
  pieces = pieces.filter((p) => p.x < w && p.y < h);
}

function size() {
  try {
    return call("session.info").size;
  } catch {
    return [80, 24];
  }
}

export function init(args) {
  if (Number.isInteger(args.rate)) rate = args.rate; // 1 to 200: the manifest
  resize(size());
  call("subscribe", { topics: ["size"] });
  period(50);
}

export function event(topic, data) {
  if (topic === "size" && Array.isArray(data.size)) resize(data.size);
}

export function tick(now) {
  const dt = (now - last) / 1000;
  last = now;
  owed += rate * dt;
  for (; owed >= 1; owed -= 1) {
    pieces.push({
      x: Math.random() * canvas.width,
      y: 0,
      vy: 4 + Math.random() * 8, // rows a second
      sway: Math.random() * 6.28,
      glyph: GLYPHS[Math.floor(Math.random() * GLYPHS.length)],
      style: STYLES[Math.floor(Math.random() * STYLES.length)],
    });
  }
  canvas.clear();
  for (const p of pieces) {
    p.y += p.vy * dt;
    const x = Math.round(p.x + Math.sin(p.sway + now / 400));
    canvas.set(x, Math.floor(p.y), p.glyph, p.style);
  }
  pieces = pieces.filter((p) => p.y < canvas.height);
  canvas.present(PLACE);
}
