// SPDX-License-Identifier: GPL-3.0-or-later
// The land along a railway line, in layers of `strip.js` (the twin's
// `_line`).
//
// `land(line, seed, rows, rich)`: `{hz, back, front, sea}` of a line of
// `LINES` for `rows` pixel rows; `rich`: ridges, trees, houses, what
// brushes the glass. `back`: beyond the track, far to near, the last
// the verge where the poles stand; `front`: what brushes the glass,
// then the ballast of the next track; `hz`: the row of the horizon;
// `sea`: water under it, painted after the first layer.

import { rgb, rnd } from "./sky.js";
import { Fields, GROUND, Layer, Strip } from "./strip.js";

export const LINES = ["plain", "coast", "alps", "desert", "city"];

const season = (spring, summer, autumn, winter) => ({ spring, summer, autumn, winter });
const rgbs = (...cs) => cs.map(([r, g, b]) => rgb(r, g, b));

/** The crops of a plain, a season. */
export const CROPS = season(
  rgbs([135, 190, 70], [112, 172, 66], [226, 208, 70], [136, 102, 70]),
  rgbs([90, 160, 40], [120, 174, 60], [216, 190, 96], [70, 138, 52]),
  rgbs([150, 156, 60], [134, 100, 68], [188, 166, 98], [156, 120, 74]),
  rgbs([226, 231, 238], [212, 220, 232], [236, 238, 242]),
);
/** The meadows of a mountain. */
export const MEADOWS = season(
  rgbs([120, 180, 76], [140, 192, 90], [104, 166, 70]),
  rgbs([84, 150, 50], [100, 164, 58], [70, 136, 56]),
  rgbs([140, 150, 64], [160, 150, 70], [126, 136, 60]),
  rgbs([226, 231, 238], [214, 222, 234]),
);
const BALLAST = rgb(122, 112, 106);

const low = (rows) => rows - Math.max(1, Math.floor(rows / 11));

function verge(seed, rows, colour = null) {
  const out = new Layer(new Strip(seed + 9, "plain", 24), 0.0, low(rows) - 0.6, rows * 0.3);
  if (colour !== null) out.colour = colour;
  out.streak = 0.3;
  return out;
}

function bed(seed, rows) {
  return new Layer(new Strip(seed + 10, "flat"), 0.0, low(rows), 0.0, { colour: BALLAST, speed: 2.0, streak: 0.8 });
}

function brush(seed, rows, things) {
  const near = new Layer(new Strip(seed + 8, "flat"), 0.0, rows + 1.0, 0.0, { speed: 2.0 });
  near.things = things;
  near.size = rows * 0.95;
  near.gap = rnd(rows * 1.4);
  near.density = 0.3;
  return near;
}

const front = (seed, rows, things) => [brush(seed, rows, things), bed(seed, rows)];

const size = (rows, share, lo, hi) => Math.min(hi, Math.max(lo, rnd(rows * share)));

function plain(s, rows, rich) {
  const hz = rnd(rows * 0.56);
  const far = new Layer(new Strip(s + 1, "hills", 26), 0.55, hz + rows * 0.1, rows * 0.3);
  const fields = new Layer(new Strip(s + 2, "hills", 44), 0.25, rows * 0.82, rows * 0.2);
  fields.fields = new Fields(CROPS, 15.0, Math.max(1, Math.floor(rows / 11)), 0.9, 0.5);
  const out = [far, fields, verge(s, rows)];
  if (!rich) return { hz, back: out, front: [], sea: false };
  const ridge = new Layer(new Strip(s, "mountains", 34), 0.8, hz + 1, rows * 0.26);
  ridge.colour = { ...GROUND, spring: rgb(84, 110, 150), summer: rgb(84, 110, 150), autumn: rgb(104, 104, 140) };
  Object.assign(far, { things: ["pine", "tree", "bush", "tree", "poplar"], gap: 4, density: 0.5, size: 2.0 });
  fields.things = ["tree", "house", "tree", "poplar", "bush", "hay", "tree", "cow", "barn", "sheep", "poplar", "church", "pine"];
  fields.size = size(rows, 0.24, 3, 6);
  fields.gap = rnd(fields.size) + 2;
  fields.density = 0.7;
  return { hz, back: [ridge, ...out], front: front(s, rows, ["tree", "tree", "pine"]), sea: false };
}

function coast(s, rows, rich) {
  const hz = rnd(rows * 0.52);
  const amp = rows * 0.3;
  const isles = new Layer(new Strip(s + 1, "hills", 30), 0.86, hz + amp * 0.5, amp, { colour: rgb(60, 130, 70) });
  const sand = { ...GROUND, spring: rgb(214, 196, 140), summer: rgb(214, 196, 140), autumn: rgb(200, 182, 134) };
  const shore = new Layer(new Strip(s + 2, "dunes", 40), 0.3, rows * 0.84, rows * 0.26, { colour: sand, streak: 0.3 });
  const grass = { ...GROUND, spring: rgb(150, 176, 96), summer: rgb(140, 164, 90) };
  const out = [isles, shore, verge(s, rows, grass)];
  if (!rich) return { hz, back: out, front: [], sea: true };
  shore.things = ["palm", "rock", "house", "bush", "palm", "hut", "lighthouse", "hut"];
  shore.size = size(rows, 0.26, 3, 6);
  shore.gap = rnd(shore.size) + 3;
  shore.density = 0.6;
  return { hz, back: out, front: front(s, rows, ["palm", "tree"]), sea: true };
}

function alps(s, rows, rich) {
  const hz = rnd(rows * 0.66);
  const peaks = new Layer(new Strip(s, "mountains", 22), 0.86, hz + 1, rows * 0.6, { colour: rgb(112, 112, 126), snow: 0.42 });
  const woods = new Layer(new Strip(s + 2, "hills", 30), 0.3, rows * 0.86, rows * 0.26);
  woods.fields = new Fields(MEADOWS, 22.0, Math.max(1, Math.floor(rows / 11)), 1.2);
  const out = [peaks, woods, verge(s, rows)];
  if (!rich) return { hz, back: out, front: [], sea: false };
  const mid = new Layer(new Strip(s + 1, "mountains", 34), 0.6, hz + 2, rows * 0.4);
  mid.colour = { ...GROUND, summer: rgb(70, 120, 80), spring: rgb(84, 132, 84) };
  Object.assign(mid, { snow: 0.8, things: ["pine"], gap: 3, density: 0.45, size: 2.0 });
  Object.assign(woods, { things: ["pine", "pine", "chalet", "pine", "cow", "pine", "church", "pine"], gap: 3, density: 0.7 });
  woods.size = size(rows, 0.24, 3, 6);
  return { hz, back: [peaks, mid, ...out.slice(1)], front: front(s, rows, ["pine"]), sea: false };
}

function desert(s, rows, rich) {
  const hz = rnd(rows * 0.6);
  const mesas = new Layer(new Strip(s, "mesas", 13), 0.84, hz + 1, rows * 0.3, { colour: rgb(204, 116, 76) });
  mesas.strata = rgbs([204, 116, 76], [160, 84, 64], [228, 150, 96], [204, 116, 76]);
  const dunes = new Layer(new Strip(s + 1, "dunes", 34), 0.4, rows * 0.84, rows * 0.3, { colour: rgb(232, 190, 124), streak: 0.35 });
  const out = [mesas, dunes, verge(s, rows, rgb(214, 170, 110))];
  if (!rich) return { hz, back: out, front: [], sea: false };
  const buttes = new Layer(new Strip(s + 3, "mesas", 9), 0.5, hz + 2, rows * 0.2, { colour: rgb(176, 84, 56) });
  buttes.strata = rgbs([176, 84, 56], [140, 64, 48], [196, 104, 64]);
  Object.assign(dunes, { things: ["cactus", "rock", "cactus", "bush", "cactus"], gap: 6, density: 0.55 });
  dunes.size = size(rows, 0.22, 3, 5);
  return { hz, back: [mesas, buttes, ...out.slice(1)], front: front(s, rows, ["cactus", "rock"]), sea: false };
}

function town(s, rows, rich) {
  const hz = rnd(rows * 0.72);
  const far = new Layer(new Strip(s, "city", 5), 0.72, rows, rows * 0.5, { colour: rgb(128, 130, 140) });
  const near = new Layer(new Strip(s + 1, "city", 8), 0.34, rows, rows * 0.66, { colour: rgb(92, 94, 104) });
  const out = [far, near, verge(s, rows, rgb(92, 92, 100))];
  if (!rich) return { hz, back: out, front: [], sea: false };
  const hills = new Layer(new Strip(s + 2, "hills", 40), 0.92, hz, rows * 0.24, { colour: rgb(96, 110, 128) });
  const park = new Layer(new Strip(s + 4, "plain", 30), 0.12, rows * 0.95, rows * 0.1);
  Object.assign(park, { things: ["tree", "bush", "tree", "poplar"], gap: 7, density: 0.5 });
  park.size = size(rows, 0.2, 3, 5);
  return { hz, back: [hills, far, near, park, out[2]], front: front(s, rows, ["tree"]), sea: false };
}

const LAND = { plain, coast, alps, desert, city: town };

/** The land of `line` for `rows` pixel rows. */
export function land(line, seed, rows, rich = false) {
  return LAND[line](seed, rows, rich && rows >= 8);
}
