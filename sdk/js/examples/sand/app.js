// SPDX-License-Identifier: GPL-3.0-or-later
// Sand: a thin stream piles on the text of the frame.
//
// One request: a sprite whose content is the effect `sand` of the
// driver. The simulation runs there; it reads the text under it as its
// walls at each render and answers nothing (`plan/store-model.md`,
// principle 2). The app holds no permission and then only waits.
//
// The test host (`ptyrc dev test`, server `ptyrc-box`) has no draw
// extension, and a driver may lack this one: there the app pours a
// pile of its own on the floor, blind to the text. The twin of
// `sdk/rust/examples/sand`: the same seed, the same pile.

import { call, Canvas, log, period, random, style } from "ptyrc";

const GRAIN = style({ fg: "#d7af5f" });
const STREAM = style({ fg: "#d7af5f", dim: true });
const PLACE = { anchor: { at: [0, 0], space: "view" } };

let rate = 60;
let canvas = null;
// Grains are braille dots, 2 x 4 a cell: `heights[x]`, the grains of
// the dot column `x`.
let heights = [];
let spout = 0;
let owed = 0;
let last = 0;

function u32() {
  const b = random(4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

function effect(cursor) {
  const params = { rate, cursor, seed: u32() };
  try {
    call("draw.put", { effect: { name: "sand", params } });
    return true;
  } catch (e) {
    log(`sand: the effect: ${e.code}: ${e.message}; a pile of its own`);
    return false;
  }
}

function resize(size) {
  const [w, h] = size;
  if (canvas) canvas.release();
  canvas = new Canvas(w, h);
  heights = new Array(2 * w).fill(0);
  spout = 2 * (w >> 1);
}

/** One grain on the spout; it rolls down while a side is two lower. */
function pour() {
  const w = heights.length;
  let x = spout;
  heights[x]++;
  for (;;) {
    const l = x > 0 && heights[x] - heights[x - 1] >= 2;
    const r = x + 1 < w && heights[x] - heights[x + 1] >= 2;
    if (!l && !r) return;
    const to = l && r ? (u32() & 1 ? x + 1 : x - 1) : l ? x - 1 : x + 1;
    heights[x]--;
    heights[to]++;
    x = to;
  }
}

// The dots of a cell, from the bottom: of its left column, its right.
const LEFT = [0, 0x40, 0x44, 0x46, 0x47];
const RIGHT = [0, 0x80, 0xa0, 0xb0, 0xb8];

function dots(n) {
  return Math.min(Math.max(n, 0), 4);
}

/** A column of cells: one fill for the full ones, a set for each other. */
function draw() {
  const h = canvas.height;
  canvas.clear();
  for (let c = 0; c < canvas.width; c++) {
    const a = heights[2 * c];
    const b = heights[2 * c + 1];
    const full = Math.min(a, b) >> 2;
    if (full) canvas.fill(c, h - full, 1, full, "⣿", GRAIN);
    for (let r = full; 4 * r < Math.max(a, b); r++) {
      const ch = 0x2800 + LEFT[dots(a - 4 * r)] + RIGHT[dots(b - 4 * r)];
      canvas.set(c, h - 1 - r, ch, GRAIN);
    }
  }
  const top = h - ((heights[spout] + 3) >> 2);
  if (top > 0) canvas.fill(spout >> 1, 0, 1, top, "⡇", STREAM);
  canvas.present(PLACE);
}

export function init(args) {
  if (Number.isFinite(args.rate) && args.rate > 0) rate = Math.min(args.rate, 1000);
  const info = call("session.info");
  if (info.server !== "ptyrc-box" && effect(args.cursor === true)) return;
  resize(info.size);
  call("subscribe", { topics: ["size"] });
  period(50);
}

export function event(topic, data) {
  if (topic === "size" && canvas && Array.isArray(data.size)) resize(data.size);
}

export function tick(now) {
  owed += (rate * (now - last)) / 1000;
  last = now;
  while (owed >= 1) {
    if (heights[spout] >= 4 * canvas.height) owed = 0; // full
    else {
      pour();
      owed -= 1;
    }
  }
  draw();
}
