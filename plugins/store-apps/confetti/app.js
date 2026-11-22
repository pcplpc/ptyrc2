// SPDX-License-Identifier: GPL-3.0-or-later
// Confetti: small pops of colour, as in the first ptyrc.
//
// A pop is one sprite: a finite animation of a few cells, played and
// removed by the driver. A burst is four of them, one per colour, each
// at its own cell within a second. `--fancy`: a burst is one explosion,
// the effect `confetti` of the driver. `--random`: the kind of each pop;
// with `--fancy`, the size, the length, the shape, the spread, the
// colours of each explosion. `--fireworks`: a rocket rises from the
// bottom of the view, slowing down, and the pop starts where it stops.
// `--all`: those three and `--sparkle`.
//
// A round is `--bursts` bursts, each at its own random offset in the
// first `PERIOD` ms; a round every `PERIOD` until the app ends; its
// sprites go with it. `--one-shot`: one round, the sprites persist, the
// app exits once the last is sent. Each shot reads the heat of the frame
// (`frame.heat`, `shape`; without it, cold everywhere) and aims at a
// cold cell, away from the previous ones.

import { call, exit, manifest, period } from "ptyrc";

export const PERIOD = 3000;
export const MAX_BURSTS = 10; // the max of --bursts in the manifest
/** The pops of a burst: bright green, magenta, blue, yellow. */
export const COLORS = [10, 13, 12, 11];
/** The pop of the first ptyrc, frame by frame. */
export const POP = [
  ["   ", " · ", "   "],
  ["   ", " * ", "   "],
  ["· ·", " o ", "· ·"],
  [" · ", "· ·", " · "],
];
/** The pops of `--random`: each a list of frames of equal size. */
export const POPS = [
  POP,
  [["·"], ["*"], ["✦"], ["*"], ["·"]],
  [
    ["     ", "  ·  ", "     "],
    ["     ", "  +  ", "     "],
    [" \\|/ ", "- * -", " /|\\ "],
    ["· ' ·", "·   ·", "· . ·"],
  ],
  [
    ["     ", "  ·  ", "     "],
    ["  ·  ", " ·o· ", "  ·  "],
    [" · · ", "·   ·", " · · "],
    ["·   ·", "     ", "·   ·"],
  ],
  [
    ["       ", "       ", "   ·   ", "       ", "       "],
    ["       ", "       ", "   *   ", "       ", "       "],
    ["       ", "  · ·  ", "   O   ", "  · ·  ", "       "],
    ["  · ·  ", "·     ·", "   ·   ", "·     ·", "  · ·  "],
    [" ·   · ", "       ", "·     ·", "       ", " ·   · "],
  ],
];
/** How the effect `confetti` throws its pieces. */
export const SHAPES = ["fan", "ball", "ring", "star", "fountain"];
/** Rockets, `[head, trail]`: the head goes through its glyphs, one a
 * step; the trail hangs under it, a glyph a row, the nearest first. */
export const ROCKETS = [
  ["|", "|:."],
  ["^", "!:'"],
  ["*+", "'.,"],
  ["o", "°·"],
  ["╿", "│╎┆"],
];
/** Ms between two drawings of a rocket. */
const STEP = 66;

const pick = (rnd, list) => list[Math.floor(rnd() * list.length)];
const uniform = (rnd, a, b) => a + (b - a) * rnd();
const int = (rnd, a, b) => a + Math.floor(rnd() * (b - a + 1));
const u32 = (rnd) => Math.floor(rnd() * 4294967296) >>> 0;

/** Ms a rocket takes to rise `rise` rows. */
export function flight(rise) {
  return Math.min(1600, 450 + 45 * rise);
}

/** `draw.put` of `art` played once, centred on `origin`, a cell of the
 * view: a frame every `step` ms, the last dim. */
export function pop(origin, color, art = POP, step = 100, persist = false) {
  const w = art[0][0].length;
  const h = art[0].length;
  const last = art.length - 1;
  const ms = Math.max(1, Math.round(step));
  return {
    frames: art.map((rows, i) => ({
      cells: rows.map((t) => [{ t, fg: color, dim: i === last }]),
      ms,
    })),
    loop: false,
    anchor: { at: [origin[0] - (w >> 1), origin[1] - (h >> 1)], space: "view" },
    z: 100,
    persist,
  };
}

/** `draw.put` of a rocket that stops at `origin` after `ms`, from `rise`
 * rows below: moved by the driver (constant braking from twice the mean
 * speed), a trail as long as it is fast. */
export function rocket(origin, rise, ms, color, look = ROCKETS[0], persist = false) {
  const [head, trail] = look;
  const glyphs = Array.from(head);
  const tail = Array.from(trail);
  const steps = Math.max(1, Math.round(ms / STEP));
  const s = ms / 1000;
  const frames = [];
  for (let i = 0; i < steps; i++) {
    const speed = 1 - i / steps;
    const cells = [[{ t: glyphs[i % glyphs.length], fg: 15, bold: true }]];
    const n = Math.round(Math.min(tail.length, rise) * speed);
    for (let k = 0; k < n; k++) cells.push([{ t: tail[k], fg: color, dim: k > 0 }]);
    frames.push({ cells, ms: Math.floor(ms / steps) });
  }
  return {
    frames,
    loop: false,
    anchor: { at: [origin[0], origin[1] + rise], space: "view" },
    motion: { vx: 0, vy: (-2 * rise) / s, ax: 0, ay: (2 * rise) / (s * s), bounce: false },
    ttl_ms: Math.round(ms),
    z: 99,
    persist,
  };
}

/** `draw.put` of one explosion of the effect `confetti`. */
export function burst(origin, count, ms, seed, persist = false, more = {}) {
  const params = { ...more, count, duration_ms: Math.round(ms), seed, origin };
  return { effect: { name: "confetti", params }, z: 100, persist };
}

/** A random cell `[x, y]` of a view given by its heat classes, a string
 * a row. A cell not hot (`h`) weighs the share of cells not hot within
 * `reach` of it; a hot cell nothing, unless all are hot. Never within
 * `reach` of a cell of `avoid`; when nothing else is left, the first of
 * `avoid` are forgotten. `rnd`: as `Math.random`. */
export function spot(classes, rnd, avoid = [], reach = 3) {
  const h = classes.length;
  const w = Math.max(0, ...classes.map((r) => r.length));
  if (!h || !w) return [0, 0];
  const cold = classes.map((r) => Array.from(r.padEnd(w, "."), (c) => c !== "h"));
  const lo = [];
  const hi = [];
  for (let x = 0; x < w; x++) {
    lo.push(Math.max(0, x - reach));
    hi.push(Math.min(w, x + reach + 1));
  }
  // `below[y][x]`: the cold cells of rows < y in the columns near x.
  const below = [new Array(w).fill(0)];
  for (const row of cold) {
    const run = [0];
    for (const c of row) run.push(run[run.length - 1] + (c ? 1 : 0));
    const prev = below[below.length - 1];
    below.push(prev.map((n, x) => n + run[hi[x]] - run[lo[x]]));
  }
  const cells = [];
  const weights = [];
  for (let y = 0; y < h; y++) {
    const y0 = Math.max(0, y - reach);
    const y1 = Math.min(h, y + reach + 1);
    const gaps = avoid.filter(([, j]) => Math.abs(y - j) <= reach).map(([i]) => i);
    for (let x = 0; x < w; x++) {
      if (gaps.some((i) => Math.abs(x - i) <= reach)) continue;
      const near = below[y1][x] - below[y0][x];
      cells.push([x, y]);
      weights.push(cold[y][x] ? near / ((y1 - y0) * (hi[x] - lo[x])) : 0);
    }
  }
  if (!cells.length) return spot(classes, rnd, avoid.slice(1), reach);
  const total = weights.reduce((a, b) => a + b, 0);
  if (!total) return pick(rnd, cells);
  let r = rnd() * total;
  for (let i = 0; i < cells.length; i++) {
    r -= weights[i];
    if (r < 0) return cells[i];
  }
  return cells[cells.length - 1];
}

/** The options of `init`, `--all` spread; `bursts` 1 to `MAX_BURSTS` by the manifest. */
export function options(args) {
  const o = {
    bursts: Number.isInteger(args.bursts) ? args.bursts : 1,
    oneShot: args["one-shot"] === true,
    sparkle: args.sparkle === true,
    fancy: args.fancy === true,
    random: args.random === true,
    fireworks: args.fireworks === true,
    size: Number.isInteger(args.size) && args.size > 0 ? Math.min(args.size, 1000) : 64,
    duration: Number.isFinite(args.duration) && args.duration > 0 ? Math.min(args.duration, 30) : 3,
  };
  if (args.all === true) o.fancy = o.random = o.fireworks = o.sparkle = true;
  return o;
}

/** The show: rounds of shots queued at their time, run by `step(now)`.
 * `env`: `put(sprite)` draws, `heat()` the heat classes of the frame,
 * `rnd()` as `Math.random`. */
export class Show {
  constructor(o, env, begin = 0) {
    this.o = o;
    this.env = env;
    this.begin = begin;
    this.queue = []; // [at, seq, action]
    this.seq = 0;
    this.avoid = [];
    // A plain burst is four pops in a second, as in the first ptyrc.
    this.colors = o.fancy ? [null] : COLORS;
    this.kept = Math.min(MAX_BURSTS, o.bursts * this.colors.length);
    this.round(0);
  }

  later(at, action) {
    this.queue.push([at, this.seq++, action]);
    this.queue.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  }

  round(n) {
    const { rnd } = this.env;
    const start = this.begin + n * PERIOD;
    for (let b = 0; b < this.o.bursts; b++) {
      const at = start + rnd() * PERIOD;
      for (const color of this.colors) {
        const wait = color === null ? 0 : 100 * int(rnd, 0, 10);
        this.later(at + wait, (now) => this.shot(color, now));
      }
    }
    if (!this.o.oneShot) this.later(start + PERIOD, () => this.round(n + 1));
  }

  bang(origin, color) {
    const { o } = this;
    const { rnd, put } = this.env;
    if (!o.fancy) {
      let art = POP;
      let step = 100;
      if (o.random) {
        art = pick(rnd, POPS);
        step = uniform(rnd, 70, 140);
      }
      return put(pop(origin, color, art, step, o.oneShot));
    }
    let size = o.size;
    let ms = o.duration * 1000;
    const more = {};
    if (o.fireworks) more.shape = "ball"; // a fan has a floor; a rocket has none
    if (o.random) {
      size = Math.max(1, Math.round(size * uniform(rnd, 0.4, 1.8)));
      ms *= uniform(rnd, 0.7, 1.2);
      more.shape = pick(rnd, SHAPES);
      more.spread = uniform(rnd, 0.5, 1.3);
      if (rnd() < 0.5) {
        // of one colour, or two
        const all = [9, 10, 11, 12, 13, 14];
        const n = int(rnd, 1, 2);
        more.colors = [];
        while (more.colors.length < n) {
          const c = all.splice(Math.floor(rnd() * all.length), 1)[0];
          more.colors.push(c);
        }
      }
    }
    put(burst(origin, size, ms, u32(rnd), o.oneShot, more));
  }

  shot(color, now) {
    const { o } = this;
    const { rnd, put } = this.env;
    let classes = this.env.heat();
    const floor = classes.length - 1;
    if (o.fireworks) classes = classes.slice(0, Math.max(1, Math.floor((classes.length * 2) / 3)));
    const origin = spot(classes, rnd, this.avoid);
    this.avoid = [...this.avoid, origin].slice(-this.kept);
    if (color === null || o.random) color = int(rnd, 9, 14);
    if (!o.fireworks) return this.bang(origin, color);
    const rise = floor - origin[1];
    const ms = flight(rise) * uniform(rnd, 0.85, 1.15);
    put(rocket(origin, rise, ms, color, pick(rnd, ROCKETS), o.oneShot));
    this.later(now + ms, () => this.bang(origin, color));
  }

  /** Runs what is due at `now`; false when nothing is left. */
  step(now) {
    while (this.queue.length && this.queue[0][0] <= now) {
      const [at, , action] = this.queue.shift();
      action(Math.max(at, now));
    }
    return this.queue.length > 0;
  }

  /** The sparkle, far behind; for `ms` and persistent, or until the app ends. */
  sparkle() {
    const { o } = this;
    if (!o.sparkle) return;
    let last = PERIOD + (o.fireworks ? flight(999) : 0);
    last += o.fancy ? o.duration * 1000 : 1500;
    const params = { count: 10 };
    if (o.oneShot) params.duration_ms = Math.round(last);
    this.env.put({ effect: { name: "sparkle", params }, layer: -2, persist: o.oneShot });
  }
}

let show = null;
let frame = 0; // the frame told: at `init`, by `move`

/** The heat classes of the frame told; `c`: the host's `call`. */
export function heat(c = call) {
  return c("frame.heat", { frame }).classes;
}

export function init(args) {
  const o = options(args);
  frame = manifest()?.frame ?? 0;
  call("subscribe", { topics: ["move"] });
  const put = (sprite) => call("draw.put", sprite);
  show = new Show(o, { put, heat: () => heat(), rnd: Math.random }, Date.now());
  show.sparkle();
  period(33);
}

export function tick(now) {
  if (!show.step(now) && show.o.oneShot) exit(0);
}

export function event(topic, data) {
  if (topic === "move" && data?.frame != null) frame = data.frame;
}
