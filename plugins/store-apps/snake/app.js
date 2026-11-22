// SPDX-License-Identifier: GPL-3.0-or-later
// Snake: plays alone, around the words of a scrolling buffer.
//
// - The view as a map: every glyph of the frame is a wall. The snake
//   goes between the words to the apple (breadth-first over the free
//   cells of the view).
// - The topic `text` (`Sight`): the map is read again when the driver
//   says the text changed or scrolled, and only then.
// - Frame cells: the snake and the apple belong to the text. Scroll
//   and they move with it; an apple the text cuts off goes elsewhere.
//   Scrolled out of sight, or written on, the snake is a ghost: brought
//   to the edge of the view, unseen, it goes through the text to the
//   nearest safe cell, unhurt.
// - One sprite for the whole snake, updated in place each step.
//
// `-e`: an apple bursts when eaten and goes down the body as a lump;
// one in six is golden; a lost tail crumbles; the body shaded; the
// score, top right. `-x`: foes that go for the middle of the body at
// half the speed and bite it off. `-l`: beams announced along a free
// row or column, then fired: what is in them burns. `-a`: all three.
// `--grab`: the arrows steer it (`keys-arrows`), at half the speed;
// ctrl+t gives them back and ends; denied, it plays alone.
//
// `Game` is pure and draws from a `Twister`: the twin's game, step for
// step, for the same seed. Steps are counted from session time.

import { call, exit, log, period, PtyrcError, random } from "ptyrc";
import { Sight } from "ptyrc/sight";
import { ACROSS, BODY, Batch, Board, DEBRIS, FOE, GOLD, HOT, LUMP, RED, X, Y, apple, beam, burst, debris, foe, key, tally, wallsOf } from "./arcade.js";
import { Twister } from "./twister.js";

export const GOLDEN = 1 / 6; // share of golden apples
export const FOES = 2; // at once
export const FOE_EVERY = [60, 140]; // steps between two foes
export const BEAMS = 2; // at once
export const BEAM_EVERY = [80, 200]; // steps between two beams
export const FUSE = 14; // steps a beam is announced
export const FIRE = 5; // then fires
export const SNAKE_Z = 4; // over the foes (2) and the particles (3), under the score (5)
export const ARROWS = { left: [-1, 0], right: [1, 0], up: [0, -1], down: [0, 1] };
export const HINT = "←↑↓→  ctrl+t";
const CATCH_UP = 4; // steps a tick at most

// Neighbours of a key, in the twin's order: right, left, down, up.
const STEPS = [ACROSS, -ACROSS, 1, -1];

/** Membership of any of `sets`. */
const any = (...sets) => ({
  has(k) {
    for (let i = 0; i < sets.length; i++) if (sets[i].has(k)) return true;
    return false;
  },
});

/** `box` `[x0, y0, x1, y1]` holds the key `k`. */
const holds = (box, k) => {
  const x = X(k);
  const y = Y(k);
  return box[0] <= x && x < box[2] && box[1] <= y && y < box[3];
};

/** Same box and same walls, as the twin's tuples compare. */
function sameMap(a, box, walls) {
  if (!a || a[0].some((v, i) => v !== box[i])) return false;
  if (a[1] === walls) return true;
  if (a[1].size !== walls.size) return false;
  for (const k of walls) if (!a[1].has(k)) return false;
  return true;
}

/** First step of a shortest path from `start` to `goal` over cells
 * where `free(k)` holds; null when there is none. */
export function route(start, goal, free) {
  const first = new Map();
  const queue = [start];
  const seen = new Set([start]);
  for (let q = 0; q < queue.length; q++) {
    const here = queue[q];
    if (here === goal) return first.get(here) ?? null;
    for (const d of STEPS) {
      const there = here + d;
      if (!seen.has(there) && (there === goal || free(there))) {
        seen.add(there);
        first.set(there, first.has(here) ? first.get(here) : there);
        queue.push(there);
      }
    }
  }
  return null;
}

/** `route` over the cells of `box` out of `taken`: the same step. */
export function way(start, goal, box, taken) {
  const [x0, y0, x1, y1] = box;
  const w = Math.max(0, x1 - x0);
  const h = Math.max(0, y1 - y0);
  const at = (k) => {
    const x = X(k) - x0;
    const y = Y(k) - y0;
    return x >= 0 && x < w && y >= 0 && y < h ? y * w + x : -1;
  };
  // The first step to each cell of the box reached; 0: not reached
  // (no key is 0), -1: the start.
  const first = new Float64Array(w * h);
  const s = at(start);
  if (s >= 0) first[s] = -1;
  const queue = [start];
  for (let q = 0; q < queue.length; q++) {
    const here = queue[q];
    const i = at(here);
    const step = i >= 0 ? first[i] : -1;
    for (let d = 0; d < 4; d++) {
      const there = here + STEPS[d];
      const j = at(there);
      if (there === start || (j >= 0 && first[j] !== 0)) continue;
      if (there === goal) return step === -1 ? there : step;
      if (j >= 0 && !taken.has(there)) {
        first[j] = step === -1 ? there : step;
        queue.push(there);
      }
    }
  }
  return null;
}

/** The walls of a map on a grid of its box: `way` on indices. */
export class Plan {
  constructor(box, walls) {
    const [x0, y0, x1, y1] = box;
    this.box = box;
    this.walls = walls;
    this.w = Math.max(0, x1 - x0);
    this.h = Math.max(0, y1 - y0);
    const n = this.w * this.h;
    this.base = new Uint8Array(n);
    for (const k of walls) {
      const i = this.at(k);
      if (i >= 0) this.base[i] = 1;
    }
    this.shut = new Uint8Array(n);
    this.first = new Int32Array(n);
    this.queue = new Int32Array(n);
  }

  /** The index of the key `k` in the box; -1: out of it. */
  at(k) {
    const x = X(k) - this.box[0];
    const y = Y(k) - this.box[1];
    return x >= 0 && x < this.w && y >= 0 && y < this.h ? y * this.w + x : -1;
  }

  /** `way(start, goal, box, taken)`, `taken` the walls and the cells
   * of `extra` (lists or sets of keys): the same step. */
  way(start, goal, extra = []) {
    const { w, h, shut, first, queue } = this;
    const s = this.at(start);
    const g = goal === null ? -1 : this.at(goal);
    if (s < 0 || g < 0) return way(start, goal, this.box, any(this.walls, ...extra.map((e) => new Set(e))));
    shut.set(this.base);
    for (const e of extra) {
      for (const k of e) {
        const i = k === null ? -1 : this.at(k);
        if (i >= 0) shut[i] = 1;
      }
    }
    // The first step to each cell reached, as its index + 1; 0: not
    // reached; -1: the start.
    first.fill(0);
    first[s] = -1;
    queue[0] = s;
    let tail = 1;
    for (let q = 0; q < tail; q++) {
      const i = queue[q];
      const step = first[i];
      const x = i % w;
      // Right, left, down, up: the twin's order.
      for (let d = 0; d < 4; d++) {
        let j;
        if (d === 0) j = x + 1 < w ? i + 1 : -1;
        else if (d === 1) j = x > 0 ? i - 1 : -1;
        else if (d === 2) j = i + w < w * h ? i + w : -1;
        else j = i >= w ? i - w : -1;
        if (j < 0 || first[j] !== 0) continue;
        const to = step === -1 ? j + 1 : step;
        if (j === g) return key(this.box[0] + ((to - 1) % w), this.box[1] + Math.floor((to - 1) / w));
        if (!shut[j]) {
          first[j] = to;
          queue[tail++] = j;
        }
      }
    }
    return null;
  }
}

/** The part of `room` that `need` does not change: the region of
 * `start` (the largest if null) on a grid with a border of walls, its
 * core (off the hanging trees) in depth-first preorder, the bridges. */
export function rooms(box, walls, start, plan = null) {
  const [x0, y0, x1, y1] = box;
  // (x, y) at (y - y0 + 1) * across + x - x0 + 1.
  const across = Math.max(0, x1 - x0) + 2;
  const down = Math.max(0, y1 - y0) + 2;
  const size = across * down;
  const open = new Uint8Array(size);
  if (plan) {
    // The walls of the plan, by rows: no key to read.
    const { base, w } = plan;
    for (let y = 0, i = across + 1; y < down - 2; y++, i += 2) for (let x = 0; x < w; x++, i++) open[i] = base[y * w + x] ^ 1;
  } else {
    for (let row = across; row < across * (down - 1); row += across) open.fill(1, row + 1, row + across - 1);
    for (const k of walls) {
      const x = X(k);
      const y = Y(k);
      if (x0 <= x && x < x1 && y0 <= y && y < y1) open[(y - y0 + 1) * across + x - x0 + 1] = 0;
    }
  }
  // The cells joined to `i`, the nearest first, into `out`; shut behind.
  const region = (i, out) => {
    out[0] = i;
    open[i] = 0;
    let n = 1;
    for (let q = 0; q < n; q++) {
      const h = out[q];
      if (open[h + 1]) (open[h + 1] = 0), (out[n++] = h + 1);
      if (open[h - 1]) (open[h - 1] = 0), (out[n++] = h - 1);
      if (open[h + across]) (open[h + across] = 0), (out[n++] = h + across);
      if (open[h - across]) (open[h - across] = 0), (out[n++] = h - across);
    }
    return n;
  };
  let cells = new Int32Array(size);
  let nc = 0;
  if (start === null) {
    let spare = new Int32Array(size);
    for (let i = across; i < across * (down - 1); i++) {
      if (!open[i]) continue;
      const n = region(i, spare);
      if (n > nc) [cells, spare, nc] = [spare, cells, n];
    }
  } else if (holds(box, start)) {
    const i = (Y(start) - y0 + 1) * across + X(start) - x0 + 1;
    if (open[i]) nc = region(i, cells);
  }
  cells = cells.subarray(0, nc);
  const out = { x0, y0, across, cells, order: null, cuts: null };
  // The region's cells are open again; a neighbour in it is one of
  // the twin's `near`.
  const mine = new Uint8Array(size);
  for (let k = 0; k < nc; k++) open[cells[k]] = mine[cells[k]] = 1;
  // Off the hanging trees: what is left has a cycle through each cell.
  const degree = new Uint8Array(size);
  const leaves = new Int32Array(nc);
  let nl = 0;
  for (let k = 0; k < nc; k++) {
    const i = cells[k];
    const d = mine[i + 1] + mine[i - 1] + mine[i + across] + mine[i - across];
    degree[i] = d;
    if (d < 2) leaves[nl++] = i;
  }
  for (let k = 0; k < nl; k++) open[leaves[k]] = 0;
  const near = [1, -1, across, -across];
  for (let q = 0; q < nl; q++) {
    for (let d = 0; d < 4; d++) {
      const n = leaves[q] + near[d];
      if (mine[n] && --degree[n] < 2 && open[n]) {
        open[n] = 0;
        leaves[nl++] = n;
      }
    }
  }
  let root = -1;
  for (let k = 0; k < nc; k++) {
    if (open[cells[k]]) {
      root = cells[k];
      break;
    }
  }
  if (root < 0) return out;
  // Bridges, depth first without recursion (Tarjan); preorder numbers:
  // a subtree is an interval. `open` now marks the core alone.
  const ncore = nc - nl;
  const disc = new Int32Array(size).fill(-1);
  const low = new Int32Array(size);
  const parent = new Int32Array(size).fill(-1);
  disc[root] = low[root] = 0;
  const order = new Int32Array(ncore);
  order[0] = root;
  let no = 1;
  const cuts = []; // far side of a bridge: first, size
  const stack = new Int32Array(ncore); // cells
  const next = new Uint8Array(ncore); // the direction to try next
  let top = 0;
  stack[0] = root;
  while (top >= 0) {
    const here = stack[top];
    let deeper = false;
    while (next[top] < 4) {
      const there = here + near[next[top]++];
      if (!open[there]) continue;
      const t = disc[there];
      if (t < 0) {
        disc[there] = low[there] = no;
        order[no++] = there;
        parent[there] = here;
        stack[++top] = there;
        next[top] = 0;
        deeper = true;
        break;
      }
      if (there !== parent[here] && t < low[here]) low[here] = t;
    }
    if (deeper) continue;
    top -= 1;
    if (top >= 0) {
      const up = stack[top];
      if (low[here] < low[up]) low[up] = low[here];
      if (low[here] > disc[up]) cuts.push(disc[here], no - disc[here]);
    }
  }
  out.order = order.subarray(0, no);
  out.cuts = cuts;
  return out;
}

/** `room` from its `rooms`: the cells out of the dead ends for `need`. */
export function pick(r, need, start) {
  const { x0, y0, across, cells, order, cuts } = r;
  const cell = (i) => key((i % across) + x0 - 1, Math.floor(i / across) + y0 - 1);
  const all = () => {
    const out = [];
    for (let k = 0; k < cells.length; k++) {
      const c = cell(cells[k]);
      if (c !== start) out.push(c);
    }
    return out;
  };
  if (!order) return all();
  const n = order.length;
  const marks = new Int32Array(n + 1);
  for (let k = 0; k < cuts.length; k += 2) {
    const first = cuts[k];
    const size = cuts[k + 1];
    if (size < need && need <= n - size) {
      marks[first] += 1;
      marks[first + size] -= 1;
    } else if (n - size < need && need <= size) {
      marks[0] += 1;
      marks[first] -= 1;
      marks[first + size] += 1;
      marks[n] -= 1;
    }
  }
  const out = [];
  let depth = 0;
  for (let k = 0; k < n; k++) {
    depth += marks[k];
    if (depth === 0) {
      const c = cell(order[k]);
      if (c !== start) out.push(c);
    }
  }
  return out.length ? out : all();
}

/** Cells of `box` free of `walls`, in the region of `start` (the
 * largest if null), out of the dead ends; `start` left out.
 *
 * A dead end: a cell no cycle passes through, nor leads to; or a side
 * of a bridge (a cut one cell wide) holding fewer than `need` cells,
 * the other side `need` or more. All the region but `start` when
 * nothing is left. */
export function room(box, walls, start, need, plan = null) {
  return pick(rooms(box, walls, start, plan), need, start);
}

export class Foe {
  constructor(n, at) {
    this.n = n; // serial
    this.at = at;
  }
}

/** `cells` in a line, sorted: along a row if `across`, else down a
 * column. Announced for `fuse` steps, then firing for `left`. */
export class Beam {
  constructor(n, cells, across, fuse = FUSE, left = FIRE) {
    Object.assign(this, { n, cells, across, fuse, left });
  }

  get firing() {
    return this.fuse <= 0;
  }
}

/** The snake, the apple and the rules; no drawing.
 *
 * `golden`, `enemies`, `lasers`: the flags; `player`: steered by
 * `press`. Each step leaves in `events` what happened: `["eat", cell,
 * golden]`, `["lost", cells]`, `["pop", cell]`, `["zap", beam]`. Cells
 * are keys (`arcade.key`); `box` is `[x0, y0, x1, y1]`, the view in
 * frame cells; `walls` a set of keys. */
export class Game {
  constructor(rng, { golden = false, enemies = false, lasers = false, player = false } = {}) {
    this.rng = rng;
    this.player = player;
    this.heading = null; // the player's last arrow: a step of `STEPS`
    this.body = []; // head first
    this.apple = null;
    this.grow = 0;
    this.eaten = 0;
    this.fancy = golden;
    this.enemies = enemies;
    this.lasers = lasers;
    this.golden = false; // the apple in play is
    this.lumps = []; // apples on their way down the body
    this.foes = [];
    this.beams = [];
    this.events = [];
    this.steps = 0;
    this.serial = 0;
    this.foeDue = enemies ? rng.randint(20, 50) : 0;
    this.beamDue = lasers ? rng.randint(20, 50) : 0;
    this.map = null; // the last seen: [box, walls]
    this.ghost = false; // on its way back into the view
    this.known = null; // [box, walls, Map(need: cells)]: `largest`
    this.plans = null; // the `Plan` of the last map
    this.parts = null; // {plan, of: Map(start: rooms)}
    this.made = 0; // `rooms` made
  }

  /** An arrow (`up`, `down`, `left`, `right`): the heading, unless
   * back over the neck. Anything else is ignored. */
  press(name) {
    const way = Object.hasOwn(ARROWS, name) ? ARROWS[name][0] * ACROSS + ARROWS[name][1] : null;
    if (way === null || this.body.length < 2) {
      this.heading = way ?? this.heading;
      return;
    }
    if (this.body[0] + way !== this.body[1]) this.heading = way;
  }

  freeCell(box, walls) {
    const [x0, y0, x1, y1] = box;
    if (x1 <= x0 || y1 <= y0) return null;
    for (let i = 0; i < 200; i++) {
      const x = this.rng.randrange(x0, x1);
      const c = key(x, this.rng.randrange(y0, y1));
      if (!walls.has(c) && !this.body.includes(c)) return c;
    }
    return null;
  }

  /** A cell for the apple, or for a new snake: in `room` of the head
   * (the largest region with no snake or a ghost), out of `taken` and
   * the body; a pocket holds the snake twice, grown, else any size.
   * Any free cell when there is none. */
  spot(box, walls, taken) {
    const head = this.body.length && !this.ghost ? this.body[0] : null;
    const body = new Set(this.body);
    for (const need of [2 * (this.body.length + this.grow + 5), 0]) {
      const from = head !== null ? pick(this.rooms(box, walls, head), need, head) : this.largest(box, walls, need);
      const cells = from.filter((c) => !taken.has(c) && !body.has(c));
      if (cells.length) return this.rng.choice(cells);
    }
    return this.freeCell(box, taken);
  }

  /** The `Plan` of the map, kept while it stays. */
  plan(box, walls) {
    if (!this.plans || !sameMap([this.plans.box, this.plans.walls], box, walls)) this.plans = new Plan(box, walls);
    return this.plans;
  }

  /** `rooms` of `start` on the map, kept while it stays: the largest
   * region's and the last start's. */
  rooms(box, walls, start) {
    const plan = this.plan(box, walls);
    if (this.parts?.plan !== plan) this.parts = { plan, of: new Map() };
    const of = this.parts.of;
    if (!of.has(start)) {
      for (const k of of.keys()) if (k !== null) of.delete(k);
      of.set(start, rooms(box, walls, start, plan));
      this.made += 1;
    }
    return of.get(start);
  }

  /** `room` of the largest region, kept while the map stays. */
  largest(box, walls, need) {
    if (!sameMap(this.known, box, walls)) this.known = [box, walls, new Map()];
    const known = this.known[2];
    if (!known.has(need)) known.set(need, pick(this.rooms(box, walls, null), need, null));
    return known.get(need);
  }

  /** Makes the `rooms` of the apple's cell, the head that eats it. */
  ready(box, walls) {
    if (this.apple === null || this.ghost || this.parts?.of.has(this.apple)) return false;
    this.rooms(box, walls, this.apple);
    return true;
  }

  /** One move inside `box`. */
  step(box, walls) {
    const inside = (c) => holds(box, c);
    this.events = [];
    this.steps += 1;
    // Scrolled out of sight, or text written on it: gone.
    this.foes = this.foes.filter((f) => inside(f.at) && !walls.has(f.at));
    this.beams = this.beams.filter((b) => b.cells.every((c) => inside(c) && !walls.has(c)));
    this.crawl(box, walls, inside);
    if (this.enemies) this.hunt(box, walls, inside);
    if (this.lasers) this.burn(box, walls, inside);
    this.lumps = this.lumps.filter((i) => i + 2 < this.body.length).map((i) => i + 2);
  }

  plant(box, walls, taken) {
    this.apple = this.spot(box, walls, taken);
    this.golden = this.fancy && this.rng.random() < GOLDEN;
  }

  /** Loses the body from segment `i` on. */
  cut(i) {
    const lost = this.body.slice(i);
    if (lost.length) this.events.push(["lost", lost]);
    this.body = this.body.slice(0, i);
  }

  crawl(box, walls, inside) {
    const foes = new Set(this.foes.map((f) => f.at));
    const warned = new Set(this.beams.flatMap((b) => b.cells));
    const firing = new Set(this.beams.filter((b) => b.firing).flatMap((b) => b.cells));
    const taken = foes.size || warned.size ? any(walls, foes, warned) : walls;
    const gone = (c) => !inside(c) || walls.has(c);

    if (!this.body.length) {
      const start = this.spot(box, walls, taken);
      this.body = start !== null ? [start] : [];
      this.grow = 3;
      this.apple = null; // maybe out of its reach
    } else if (this.ghost || gone(this.body[0])) {
      // Scrolled out of sight, or text written on its head.
      this.haunt(box, walls);
      if (this.apple === null || gone(this.apple)) this.plant(box, walls, taken);
      return;
    } else if (this.apple !== null && !sameMap(this.map, box, walls) && this.plan(box, walls).way(this.body[0], this.apple) === null) {
      // The text moved or changed: an apple cut off goes elsewhere.
      this.apple = null;
    }
    this.map = [box, walls];
    if (this.apple === null || gone(this.apple)) this.plant(box, walls, taken);
    if (!this.body.length) return;
    const head = this.body[0];
    if (this.player) {
      this.steer(head, inside, any(walls, foes));
      return;
    }
    const around = STEPS.map((d) => head + d);
    const body = new Set(this.body);
    const plan = this.plan(box, walls);
    let to = null;
    // Through an announced beam only when there is no other way.
    const tries = warned.size ? [[foes, body, warned], [foes, body, firing]] : [[foes, body]];
    for (const extra of tries) {
      const shut = any(walls, ...extra);
      to = this.apple !== null ? plan.way(head, this.apple, extra) : null;
      if (to === null) {
        // No way to the apple: any free neighbour, to stay alive.
        const near = around.filter((c) => inside(c) && !shut.has(c));
        to = near.length ? this.rng.choice(near) : null;
      }
      if (to !== null) break;
    }
    if (to === null || firing.has(to)) {
      // Cornered: shed the tail and try again from the head.
      this.cut(1);
      this.grow = 3;
      return;
    }
    this.body.unshift(to);
    this.grown(to);
  }

  /** The head just moved to `to`: eat, grow or drag the tail. */
  grown(to) {
    if (to === this.apple) {
      this.events.push(["eat", to, this.golden]);
      this.grow += this.golden ? 5 : 2;
      this.apple = null;
      this.eaten += 1;
      this.lumps.push(-2);
    }
    if (this.grow) this.grow -= 1;
    else this.body.pop();
  }

  /** The player's move: one cell along `heading`; none against `stops`
   * or the side of the view. Into the body, bitten off there. */
  steer(head, inside, stops) {
    if (this.heading === null) return;
    const to = head + this.heading;
    if (!inside(to) || stops.has(to)) return;
    const tail = this.grow ? null : this.body[this.body.length - 1]; // moves away
    const i = this.body.indexOf(to);
    if (i >= 0 && to !== tail) this.cut(i);
    this.body.unshift(to);
    this.grown(to);
  }

  /** The ghost's move: through anything, one cell towards the nearest
   * cell of the largest region off its body. Farther than a cell out of
   * the view, the body is first shifted, unseen, to its edge. Solid
   * again on that cell; it waits while there is none. */
  haunt(box, walls) {
    this.ghost = true;
    const [x0, y0, x1, y1] = box;
    let hx = X(this.body[0]);
    let hy = Y(this.body[0]);
    const dx = Math.min(Math.max(hx, x0 - 1), x1) - hx;
    const dy = Math.min(Math.max(hy, y0 - 1), y1) - hy;
    if (dx || dy) this.body = this.body.map((c) => c + dx * ACROSS + dy);
    hx += dx;
    hy += dy;
    const body = new Set(this.body);
    let safe = [];
    for (const need of [2 * (this.body.length + this.grow + 5), 0]) {
      safe = this.largest(box, walls, need).filter((c) => !body.has(c));
      if (safe.length) break;
    }
    if (!safe.length) return;
    // The nearest; a tie to the least `(x, y)`, as keys sort.
    let goal = null;
    let best = Infinity;
    for (const c of safe) {
      const d = Math.abs(X(c) - hx) + Math.abs(Y(c) - hy);
      if (d < best || (d === best && c < goal)) [goal, best] = [c, d];
    }
    const gx = X(goal);
    const gy = Y(goal);
    const to = Math.abs(gx - hx) >= Math.abs(gy - hy) ? key(hx + Math.sign(gx - hx), hy) : key(hx, hy + Math.sign(gy - hy));
    this.body.unshift(to);
    if (this.grow) this.grow -= 1;
    else this.body.pop();
    if (to === goal) {
      this.ghost = false;
      this.map = null; // the apple is checked again from there
    }
  }

  /** A foe comes; foes move every other step; one that reaches the
   * body bites it off there and is gone. */
  hunt(box, walls, inside) {
    const rng = this.rng;
    if (this.foes.length < FOES && this.steps >= this.foeDue) {
      this.foeDue = this.steps + rng.randint(...FOE_EVERY);
      const taken = any(walls, new Set(this.foes.map((f) => f.at)), new Set([this.apple]));
      const at = this.freeCell(box, taken);
      const head = this.body.length ? this.body[0] : null;
      if (at !== null && (head === null || Math.abs(X(at) - X(head)) + Math.abs(Y(at) - Y(head)) >= 8)) {
        this.serial += 1;
        this.foes.push(new Foe(this.serial, at));
      }
    }
    if (this.steps % 2) return;
    for (const one of [...this.foes]) {
      const others = new Set(this.foes.filter((f) => f !== one).map((f) => f.at));
      const extra = [others, [this.apple], this.body];
      const taken = any(walls, others, new Set([this.apple]), new Set(this.body));
      const big = this.body.length >= 4 && !this.ghost;
      const goal = big ? this.body[this.body.length >> 1] : null;
      let to = goal !== null ? this.plan(box, walls).way(one.at, goal, extra) : null;
      if (to === null) {
        const near = STEPS.map((d) => one.at + d).filter((c) => inside(c) && !taken.has(c));
        to = near.length ? rng.choice(near) : one.at;
      }
      if (to === goal) {
        this.cut(this.body.indexOf(to));
        this.foes.splice(this.foes.indexOf(one), 1);
        this.events.push(["pop", to]);
      } else {
        one.at = to;
      }
    }
  }

  /** Beams age, fire and burn; a new one is aimed. */
  burn(box, walls, inside) {
    for (const b of this.beams) {
      if (b.fuse > 0) {
        b.fuse -= 1;
        if (b.fuse === 0) this.events.push(["zap", b]);
      } else {
        b.left -= 1;
      }
    }
    this.beams = this.beams.filter((b) => b.left > 0);
    const hot = new Set(this.beams.filter((b) => b.firing).flatMap((b) => b.cells));
    const hit = this.ghost ? -1 : this.body.findIndex((c) => hot.has(c));
    if (hit >= 0) this.cut(hit);
    for (const f of this.foes.filter((f) => hot.has(f.at))) {
      this.foes.splice(this.foes.indexOf(f), 1);
      this.events.push(["pop", f.at]);
    }
    if (this.beams.length < BEAMS && this.steps >= this.beamDue) {
      this.beamDue = this.steps + this.rng.randint(...BEAM_EVERY);
      const b = this.aim(box, walls, inside);
      if (b !== null) this.beams.push(b);
    }
  }

  /** A beam through a segment of the snake, or through a free cell:
   * from wall to wall, 5 cells or more. */
  aim(box, walls, inside) {
    const rng = this.rng;
    const seed = this.body.length && rng.random() < 0.5 ? rng.choice(this.body) : this.freeCell(box, walls);
    if (seed === null) return null;
    for (const across of rng.sample([true, false], 2)) {
      const d = across ? ACROSS : 1;
      const cells = [seed];
      for (const sign of [1, -1]) {
        let c = seed + sign * d;
        while (inside(c) && !walls.has(c)) {
          cells.push(c);
          c += sign * d;
        }
      }
      if (cells.length >= 5) {
        this.serial += 1;
        return new Beam(this.serial, cells.sort((a, b) => a - b), across);
      }
    }
    return null;
  }

  /** The snake as a sprite: its top-left cell `[x, y]` and rows of
   * runs. `fancy`: shaded from head to tail, a lump where an apple is.
   * A ghost is dim. */
  picture(fancy = false) {
    const xs = this.body.map(X);
    const ys = this.body.map(Y);
    const left = Math.min(...xs);
    const top = Math.min(...ys);
    const w = Math.max(...xs) - left + 1;
    const rows = Array.from({ length: Math.max(...ys) - top + 1 }, () => Array.from({ length: w }, () => ({ t: " " })));
    const n = this.body.length;
    for (let i = n - 1; i >= 0; i--) {
      let run;
      if (i === 0) run = { t: "@", fg: 10, bold: true };
      else if (!fancy) run = { t: "o", fg: 2 };
      else if (this.lumps.includes(i) || this.lumps.includes(i - 1)) run = { t: "O", fg: LUMP, bold: true };
      else run = { t: "o", fg: BODY[Math.floor((i * BODY.length) / n)] };
      if (this.ghost) run = { ...run, dim: true };
      rows[ys[i] - top][xs[i] - left] = run;
    }
    return [[left, top], rows];
  }
}

/** The sprites of a game's foes, beams and particles, and its score:
 * when `effects`, or with a `hint`. `rng`: the particles' own. */
export class Stage {
  constructor(rng, effects, hint = null) {
    this.rng = rng;
    this.effects = effects;
    this.hint = hint;
    this.foes = new Map(); // serial: [handle, at]
    this.beams = new Map(); // serial: [handle, firing]
    this.board = new Board();
    this.best = 0;
  }

  /** The game after its steps; `events`: theirs, in order. */
  show(out, frame, game, events) {
    const put = (at, spec) => out.put({ frame, anchor: { at: [X(at), Y(at)], space: "frame" }, ...spec });
    const alive = new Map(game.foes.map((f) => [f.n, f]));
    for (const [n, [h]] of [...this.foes]) {
      if (!alive.has(n)) {
        out.remove(h);
        this.foes.delete(n);
      }
    }
    for (const [n, it] of alive) {
      const was = this.foes.get(n);
      if (!was) this.foes.set(n, [put(it.at, foe()), it.at]);
      else if (was[1] !== it.at) {
        out.update(was[0], { anchor: { at: [X(it.at), Y(it.at)], space: "frame" } });
        was[1] = it.at;
      }
    }
    const lit = new Map(game.beams.map((b) => [b.n, b]));
    for (const [n, [h]] of [...this.beams]) {
      if (!lit.has(n)) {
        out.remove(h);
        this.beams.delete(n);
      }
    }
    for (const [n, it] of lit) {
      const look = beam(it.cells.length, it.across, it.firing);
      const was = this.beams.get(n);
      if (!was) this.beams.set(n, [put(it.cells[0], { z: -1, ...look }), it.firing]);
      else if (was[1] !== it.firing) {
        out.update(was[0], look);
        was[1] = it.firing;
      }
    }
    if (this.effects) {
      for (const e of events) this.play(put, ...e);
      this.best = Math.max(this.best, game.body.length);
    }
    if (this.effects || this.hint) this.board.show(out, frame, tally(game.body.length, this.effects ? this.best : null, this.hint));
  }

  /** An event of the game, as particles. */
  play(put, what, ...data) {
    const rng = this.rng;
    if (what === "eat") {
      const [at, golden] = data;
      const sparks = golden ? burst(26, rng, GOLD, { speed: [6.0, 20.0] }) : burst(10, rng, RED);
      for (const one of sparks) put(at, one);
    } else if (what === "lost") {
      const [cells] = data;
      for (const at of rng.sample(cells, Math.min(cells.length, DEBRIS))) put(at, debris(rng));
    } else if (what === "pop") {
      for (const one of burst(12, rng, FOE, { glyphs: "x+'." })) put(data[0], one);
    } else if (what === "zap") {
      const [it] = data;
      for (const at of [it.cells[0], it.cells[it.cells.length - 1]]) {
        for (const one of burst(4, rng, HOT, { speed: [3.0, 9.0], life: [0.2, 0.5] })) put(at, one);
      }
    }
  }
}

/** The options as the game takes them; `error`: a refusal, or null. */
export function options(args = {}) {
  const all = args.all === true;
  const o = {
    speed: args.speed ?? 12.0,
    effects: all || args.effects === true,
    enemies: all || args.enemies === true,
    lasers: all || args.lasers === true,
    grab: args.grab === true,
  };
  o.error = o.speed > 0 ? null : "--speed must be positive";
  return o;
}

/** Steps due at `now` ms, at `rate` a second: the twin's step `k` falls
 * at `(k + 1) / rate` seconds. */
export const due = (now, rate) => Math.floor((now * rate) / 1000);

// The run.
let o;
let game;
let stage;
let sight;
let walls = new Set();
let done = 0; // steps made
let waited = false; // the last tick read a new map and made no step
let rate = 12;
let snake = null; // its handle
let fruit = null; // the apple's handle
let shown = null; // [apple, golden] drawn

export function init(args) {
  o = options(args);
  if (o.error) {
    log(`snake: ${o.error}`);
    exit(2);
    return;
  }
  const b = random(4);
  const seed = ((b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0) % 1000000007;
  if (o.grab) {
    try {
      call("input.grab", { keys: Object.keys(ARROWS), escape: "ctrl+t" });
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      log(`snake: the arrows: ${e.code}; it plays alone`);
      o.grab = false;
    }
  }
  game = new Game(new Twister(seed), { golden: o.effects, enemies: o.enemies, lasers: o.lasers, player: o.grab });
  // Its own draws: the particles do not change the game of a seed.
  stage = new Stage(new Twister(seed), o.effects, o.grab ? HINT : null);
  sight = new Sight();
  // With a player, half the speed.
  rate = o.grab ? o.speed / 2 : o.speed;
  period(Math.max(33, Math.min(1000, Math.round(1000 / rate))));
}

export function tick(now) {
  if (!sight.look()) return;
  const [w, h] = sight.size;
  const [ox, oy] = sight.info?.origin ?? [0, 0];
  const box = [ox, oy, ox + w, oy + h];
  if (sight.changed) {
    walls = wallsOf(sight.rows, ox, oy);
    if (!waited) {
      // A new map: read this tick, played the next, its steps caught
      // up; the cost of a tick halved. Ready the plan and, before the
      // first step, the room of a new snake.
      waited = true;
      game.plan(box, walls);
      if (!game.body.length) game.largest(box, walls, 2 * (game.grow + 5));
      return;
    }
  }
  waited = false;
  const target = due(now, rate);
  const n = Math.min(target - done, CATCH_UP);
  done = target;
  if (n <= 0) return;
  const events = [];
  const made = game.made;
  for (let i = 0; i < n; i++) {
    game.step(box, walls);
    events.push(...game.events);
  }
  draw(sight.info?.id ?? sight.frame, events);
  // The room of the apple's cell, made in a tick that made none.
  if (game.made === made) game.ready(box, walls);
}

function draw(frame, events) {
  const out = new Batch(call);
  stage.show(out, frame, game, events);
  if (!game.body.length) {
    // A full screen: wait for room. Or burnt: back next step.
    if (snake) out.remove(snake);
    snake = null;
  } else {
    const [at, cells] = game.picture(o.effects);
    const anchor = { at, space: "frame" };
    if (!snake) snake = out.put({ frame, anchor, content: { cells }, z: SNAKE_Z });
    else out.update(snake, { anchor, content: { cells } });
    const now = [game.apple, game.golden];
    if (game.apple !== null && (!shown || now[0] !== shown[0] || now[1] !== shown[1])) {
      const look = o.effects ? apple(game.golden) : { content: { rows: ["●"], style: { fg: 9 } } };
      const place = { at: [X(game.apple), Y(game.apple)], space: "frame" };
      if (!fruit) fruit = out.put({ frame, anchor: place, ...look });
      else {
        out.update(fruit, { anchor: place });
        if (o.effects && shown[1] !== game.golden) out.update(fruit, look);
      }
      shown = now;
    }
  }
  try {
    out.send();
  } catch (e) {
    if (!(e instanceof PtyrcError)) throw e;
    if (e.code !== "not-usable") throw e;
    sight.touch(); // off screen in the middle: the next look waits
  }
}

export function event(topic, data) {
  if (sight.event(topic, data)) return;
  if (topic === "input" && data?.released) exit(0);
  else if (topic === "input" && data?.key) game.press(data.key);
}

export function stop() {
  if (o?.grab) {
    try {
      call("input.release");
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
  }
}
