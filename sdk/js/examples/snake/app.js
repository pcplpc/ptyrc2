// SPDX-License-Identifier: GPL-3.0-or-later
// Snake: plays alone between the words; the text is the wall.
//
// Each step reads `screen.shape` (`#` where there is ink). The apple's
// distance field (breadth-first over the cells without ink) is begun
// again when the apple or the text changes, and grows by `BUDGET`
// cells a step; the head goes to the free neighbour nearest the apple,
// else to a free neighbour drawn at random, else it sheds its tail.
// An apple the finished field does not join to the head is planted
// again. Text written on the head: the snake
// is lost and comes back elsewhere.
//
// `shape` denied: a blank screen (`"degree": "none"`): it plays on.
// `grab`: the arrows steer it; `keys-arrows` denied: it plays alone.
// The twin of `sdk/rust/examples/snake`: the same seed, the same game.

import { call, Canvas, exit, log, period, random, style } from "ptyrc";

const HEAD = style({ fg: 10, bold: true });
const BODY = style({ fg: 2 });
const APPLE = style({ fg: 9 });
const HINT = style({ fg: 8 });
const PLACE = { anchor: { at: [0, 0], space: "view" } };
const ARROWS = { right: [1, 0], left: [-1, 0], down: [0, 1], up: [0, -1] };
const STEPS = [[1, 0], [-1, 0], [0, 1], [0, -1]];
// Cells of the field a step: a bound, so that a field of 200x60
// stays well under the 50 ms of a tick.
const BUDGET = 2000;

let w = 0;
let h = 0;
let canvas;
let mapKey = null;
let solid = []; // 1: ink
let body = []; // cell indices, the head first
let occ; // body segments per cell
let grow = 0;
let apple = -1;
let dist = []; // steps to the apple, -1: none
let queue = [];
let qh = 0;
let qt = 0;
let player = false;
let heading = null;
let toldBlank = false;

/** A random integer in `0..n`, as `ptyrc_app::random_below`. */
function below(n) {
  const b = random(4);
  const u = (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
  return Math.floor((u * n) / 4294967296);
}

function free(i) {
  return occ[i] === 0 && solid[i] === 0;
}

function resize(size) {
  [w, h] = size;
  if (canvas) canvas.release();
  canvas = new Canvas(w, h);
  occ = new Uint8Array(w * h);
  solid = new Array(w * h).fill(0);
  dist = new Array(w * h).fill(-1);
  queue = new Array(w * h).fill(0);
  qh = qt = 0;
  body = [];
  apple = -1;
  mapKey = null;
}

function randomFree() {
  for (let k = 0; k < 200; k++) {
    const x = below(w);
    const y = below(h);
    if (free(y * w + x)) return y * w + x;
  }
  return -1;
}

/** Begins the field of `goal`. */
function begin(goal) {
  dist.fill(-1);
  dist[goal] = 0;
  queue[0] = goal;
  qh = 0;
  qt = 1;
}

function reach(j, d) {
  if (dist[j] < 0 && solid[j] === 0) {
    dist[j] = d;
    queue[qt++] = j;
  }
}

/** Grows the field by `BUDGET` cells at most. */
function growField() {
  const n = w * h;
  for (let b = 0; b < BUDGET && qh < qt; b++) {
    const i = queue[qh++];
    const x = i % w;
    const d = dist[i] + 1;
    // Unrolled: no array made a cell.
    if (x + 1 < w) reach(i + 1, d);
    if (x > 0) reach(i - 1, d);
    if (i + w < n) reach(i + w, d);
    if (i >= w) reach(i - w, d);
  }
}

function plant() {
  apple = randomFree();
  if (apple >= 0) begin(apple);
}

function cut(i) {
  for (const c of body.splice(i)) occ[c]--;
}

function look() {
  let answer;
  try {
    answer = call("screen.shape");
  } catch (e) {
    answer = { lines: [] };
  }
  if (answer.degree === "none" && !toldBlank) {
    toldBlank = true;
    log("snake: shape denied: the text is not seen; it plays on a blank screen");
  }
  const lines = answer.lines || [];
  const key = lines.join("\n");
  if (key === mapKey) return;
  mapKey = key;
  // Runs of ink, filled whole: a call a word, not a cell.
  solid.fill(0);
  const ink = /#+/g;
  for (let y = 0; y < h && y < lines.length; y++) {
    const line = lines[y].slice(0, w);
    ink.lastIndex = 0;
    for (let m = ink.exec(line); m; m = ink.exec(line)) {
      solid.fill(1, y * w + m.index, y * w + m.index + m[0].length);
    }
  }
  if (apple >= 0) begin(apple);
}

/** The next cell of the head; -1: none. */
function choose(head) {
  const x = head % w;
  const y = (head / w) | 0;
  const near = [];
  for (const [dx, dy] of STEPS) {
    const nx = x + dx;
    const ny = y + dy;
    if (nx >= 0 && nx < w && ny >= 0 && ny < h && free(ny * w + nx)) near.push(ny * w + nx);
  }
  let best = -1;
  for (const c of near) {
    if (dist[c] >= 0 && (best < 0 || dist[c] < dist[best])) best = c;
  }
  if (best < 0 && near.length) best = near[below(near.length)];
  return best;
}

/** The player's move: along `heading`; none against ink or the side. */
function steer(head) {
  const x = (head % w) + heading[0];
  const y = ((head / w) | 0) + heading[1];
  if (x < 0 || x >= w || y < 0 || y >= h || solid[y * w + x]) return -1;
  const to = y * w + x;
  const k = body.indexOf(to);
  if (k > 0 && !(k === body.length - 1 && grow === 0)) cut(k);
  return to;
}

function step() {
  look();
  if (body.length === 0) {
    const s = randomFree();
    if (s < 0) return;
    body = [s];
    occ[s]++;
    grow = 3;
    apple = -1;
  }
  const head = body[0];
  if (solid[head]) {
    cut(0); // written on: lost
    return;
  }
  if (apple < 0 || solid[apple]) plant();
  else if (qh >= qt && dist[head] < 0) plant(); // cut off
  if (apple < 0) return;
  growField();
  let to;
  if (player && heading) {
    to = steer(head);
    if (to < 0) return; // it waits
  } else {
    to = choose(head);
    if (to < 0) {
      cut(1); // cornered
      grow = 3;
      return;
    }
  }
  body.unshift(to);
  occ[to]++;
  if (to === apple) {
    grow += 2;
    apple = -1;
  }
  if (grow > 0) grow--;
  else occ[body.pop()]--;
}

function draw() {
  canvas.clear();
  if (apple >= 0) canvas.set(apple % w, (apple / w) | 0, "●", APPLE);
  for (let k = body.length - 1; k >= 0; k--) {
    const c = body[k];
    canvas.set(c % w, (c / w) | 0, k ? "o" : "@", k ? BODY : HEAD);
  }
  if (player) canvas.text(Math.max(0, w - 12), 0, "←↑↓→ ctrl+t", HINT);
  canvas.present(PLACE);
}

export function init(args) {
  const speed = Math.min(Math.max(Number(args.speed) || 12, 1), 30);
  resize(call("session.info").size);
  call("subscribe", { topics: ["size"] });
  if (args.grab) {
    try {
      call("input.grab", { keys: Object.keys(ARROWS) });
      player = true;
    } catch (e) {
      log(`snake: the arrows: ${e.code}; it plays alone`);
    }
  }
  period(Math.round((player ? 2000 : 1000) / speed));
  step();
  draw();
}

export function tick() {
  step();
  draw();
}

export function event(topic, data) {
  if (topic === "size" && Array.isArray(data.size)) {
    resize(data.size);
  } else if (topic === "input" && data.released) {
    exit(0);
  } else if (topic === "input" && Object.hasOwn(ARROWS, data.key) && body.length) {
    const [dx, dy] = ARROWS[data.key];
    const x = (body[0] % w) + dx;
    const y = ((body[0] / w) | 0) + dy;
    if (body.length < 2 || y * w + x !== body[1]) heading = [dx, dy];
  }
}

export function stop() {
  if (player) {
    try {
      call("input.release");
    } catch (e) {}
  }
}
