// SPDX-License-Identifier: GPL-3.0-or-later
// Mouser: the cat of a window sill, and the pointer it hunts (the
// twin's `_mouser`).
//
// A cat is pixels on a sill: a pose is a shape, the column of its
// middle, its height over the sill, the way it faces, its tail.
//
// - `idle`: the cat alone, a function of the time: it sits, looks
//   aside, grooms, stretches, lies down, sleeps at night and in naps;
//   its tail sways; it watches what flies by (`prey`).
// - `Cat`: the cat and the pointer, stepped `DT` by `DT`: it wakes,
//   notices, runs, crouches, pounces in an arc, lands, bats what it
//   caught, looks for what it missed, leaps under what is out of
//   reach, loses interest, grooms, walks home.
//
// `Cat.at(t, pointer, ...)` steps to `t`. A time before the last one,
// or another sill: the cat starts again from the trail of the pointer;
// a `Cat` just made is a function of `t` and of the trail.

import { mod, rnd } from "./sky.js";
import { noise } from "./strip.js";

export const DT = 1 / 30; // seconds a step
const G = 110.0; // pixels a second a second, down
const ACC = 70.0; // of a run
const SLOT = 8.0; // seconds an act of the cat alone
const NOTICE = 0.3; // seconds it stares before it goes
const REACH = 5.5; // pixels over its feet it can catch at
/** What it does sat: on no mullion. */
const SAT = new Set(["alert", "miss", "hold", "watch", "beg", "seek"]);

/** Pixels of a cat, `#`, the feet on the last row. `anchor`: the
 * column of its middle. `back`: seen from behind, the tail apart, on
 * the sill; else from the side, facing right. */
const shape = (rows, anchor = 2, back = true) => ({ rows, anchor, back });
const side = (...rows) => shape(rows, 3, false);

export const SIT = shape([".#.#.", ".###.", ".###.", "#####", "#####"]);
export const SHAPES = {
  sit: SIT,
  twitch: shape(["...#.", ".###.", ".###.", "#####", "#####"]),
  look: shape(["..#.#", "..###", ".###.", "#####", "#####"]),
  alert: shape([".#.#.", ".###.", ".###.", ".###.", "#####", "#####"]),
  stare: shape(["..#.#", "..###", ".###.", ".###.", "#####", "#####"]),
  lick: shape(["..#.#", ".####", "#####", "#####"]),
  raise: shape([".#.#.#", ".###.#", ".####.", "#####.", "#####."]),
  swat: shape([".#.#..", ".###..", ".#####", "#####.", "#####."]),
  beg: shape([".....#", ".#.#.#", ".####.", ".###..", ".###..", ".###..", ".###.."]),
  leap: shape([".....#", ".#.#.#", ".####.", ".###..", ".###..", ".#.#.."]),
  squat: shape([".#.#.", "#####", "#####"]),
  loaf: shape(["..#.#..", ".#####.", "#######"], 3),
  curl: shape([".#####.", "#######"], 3),
  breath: shape(["..###..", ".#####.", "#######"], 3),
  ball: shape([".###.", "#####"]),
  swell: shape([".###.", "#####", "#####"]),
  walk: side("#....#.", "#...###", ".######", ".#####.", ".#...#."),
  step: side("#....#.", "#...###", ".######", ".#####.", "..#.#.."),
  bound: side(".....#.", "#...###", ".######", ".#####.", "#.....#"),
  gather: side("#....#.", ".#..###", ".######", ".#####.", "..#.#.."),
  crouch: side(".....#.", ".######", "#######"),
  wiggle: side(".#...#.", ".######", "#######"),
  rise: side(".....#.", "....###", "..####.", ".###...", "#......"),
  fly: side("#....#.", ".######", ".#####.", "......#"),
  fall: side("#......", ".###.#.", "..#####", "...####", ".....##"),
  bow: side("#......", ".##....", ".####.#", "..#####"),
};
/** The tail of a cat seen from behind, on the sill: columns from its
 * middle, pixels up. */
export const TAILS = {
  rest: [
    [3, 0],
    [4, 0],
    [5, 1],
  ],
  flat: [
    [3, 0],
    [4, 0],
    [5, 0],
  ],
  flick: [
    [3, 0],
    [4, 1],
  ],
  none: [],
};

/** A cat: its shape, the column of its middle, its feet over the
 * sill, the way it faces (1 right), its tail and the side it is on. */
export const pose = (shape, x, z = 0.0, face = 1, tail = "none", way = 1) => ({ shape, x, z, face, tail, way });

/** The pixels `[x, y]` of `p` on a sill at the bottom of `rows` rows. */
export function cells(p, rows) {
  const cx = rnd(p.x);
  const base = rows - 1 - rnd(p.z);
  const s = p.shape;
  const out = [];
  const n = s.rows.length;
  for (let j = 0; j < n; j++) {
    const line = s.rows[n - 1 - j];
    for (let i = 0; i < line.length; i++) if (line[i] === "#") out.push([cx + p.face * (i - s.anchor), base - j]);
  }
  if (s.back && rnd(p.z) === 0) for (const [k, up] of TAILS[p.tail]) out.push([cx + p.way * k, rows - 1 - up]);
  return out;
}

/** Where a cat lives: `w` by `rows` pixels, mullions at `cols`;
 * `home`, the column of its middle at rest, its tail to `way`; `lo` to
 * `hi`, the columns its middle can take; `left` to `right`, those where
 * it shows when the pointer is gone. */
export class Sill {
  constructor(seed, w, rows, home, way, cols, lo, hi, left, right) {
    Object.assign(this, { seed, w, rows, home, way, cols, lo, hi, left, right });
    this.key = JSON.stringify([seed, w, rows, home, way, cols, lo, hi, left, right]);
  }

  /** A cat at `x`, `half` pixels to each side, is on no mullion. */
  clear(x, half = 2) {
    const cx = rnd(x);
    return !this.cols.some((c) => cx - half <= c && c <= cx + half);
  }

  /** Room at home to lie down. */
  get wide() {
    return this.clear(this.home, 3) && 3 <= this.home && this.home < this.w - 3;
  }

  /** The most its feet leave the sill. */
  get high() {
    return Math.max(2, Math.min(8, this.rows - 8));
  }

  /** The longest pounce. */
  get range() {
    return Math.max(3.0, Math.min(7.0, (this.hi - this.lo) / 3));
  }

  /** Pixels a second, at most. */
  get run() {
    return 7.0 + 0.2 * this.w;
  }

  /** The column nearest `x` to sit at: within reach, on no mullion;
   * `shown`: where it shows when the pointer is gone. */
  stop(x, shown = false) {
    const [lo, hi] = shown ? [this.left, this.right] : [this.lo, this.hi];
    const cx = rnd(Math.min(hi, Math.max(lo, x)));
    for (let d = 0; d < this.w; d++) for (const c of [cx - d, cx + d]) if (lo <= c && c <= hi && this.clear(c)) return c;
    return cx;
  }

  /** The side of the tail of a cat sat at `x`: `prefer`, if it fits. */
  tail(x, prefer) {
    const cx = rnd(x);
    for (const way of [prefer, -prefer]) {
      const cols = [3, 4, 5].map((k) => cx + way * k);
      if (cols.every((c) => 0 <= c && c < this.w && !this.cols.includes(c))) return way;
    }
    return prefer;
  }
}

// ---- the cat alone ----

const SHARES = [
  ["sit", 0.4],
  ["look", 0.16],
  ["groom", 0.14],
  ["up", 0.1],
  ["stretch", 0.08],
];

/** What the cat alone does in the slot `k`. */
export function act(seed, k, light) {
  const drowsy = noise(seed + 121, Math.floor(k / 6)) < (light < 0.25 ? 0.85 : 0.18);
  let r = noise(seed + 122, k);
  if (drowsy) return r < 0.8 ? "sleep" : "loaf";
  for (const [name, share] of SHARES) {
    r -= share;
    if (r < 0) return name;
  }
  return "loaf";
}

/** The tail of a cat that sits: it sways, flicks now and then;
 * `lash`: it beats. */
export function sway(t, lash = false) {
  if (lash) return ["flick", "flat"][Math.trunc(t * 5) % 2];
  if (Math.trunc(t) % 13 < 2) return ["rest", "flick"][Math.trunc(t * 3) % 2];
  return Math.trunc(t / 5) % 3 === 1 ? "flat" : "rest";
}

/** The cat alone at `t`, at home. `light`: of the day, 0..1; `prey`:
 * the column of what flies outside. */
export function idle(sill, t, light, prey = null) {
  const seed = sill.seed;
  const x = sill.home;
  const way = sill.way;
  const k = Math.trunc(floorDiv(t, SLOT));
  const u = t - k * SLOT;
  const now = act(seed, k, light);
  const last = k ? act(seed, k - 1, light) : "sit";
  const down = last === "sleep" || last === "loaf";
  const aside = noise(seed + 123, k) < 0.5 ? 1 : -1;
  const loaf = SHAPES[sill.wide ? "loaf" : "squat"];
  const sat = (name, face = 1, tail = null) => pose(SHAPES[name], x, 0.0, face, tail || sway(t), way);

  if (now === "sleep") {
    if (!down && u < 0.75) return pose(loaf, x);
    const breath = Math.trunc(t / 1.75) % 2;
    const names = sill.wide ? ["curl", "breath"] : ["ball", "swell"];
    return pose(SHAPES[names[breath]], x);
  }
  if (now === "loaf") {
    if (prey !== null) return pose(loaf, x);
    const ear = sill.wide && 3.0 <= u && u < 3.25;
    return pose(ear ? SHAPES.curl : loaf, x);
  }
  if (down && u < 0.5) return pose(loaf, x); // it gets up
  if (prey !== null) {
    const near = Math.abs(prey - x) < 3;
    return sat(near ? "alert" : "stare", prey > x ? 1 : -1, sway(t, true));
  }
  if (now === "look" && 1.5 <= u && u < 5.5) return sat("look", aside);
  if (now === "groom" && 1.0 <= u && u < 6.5) return sat(["lick", "look"][Math.trunc(t * 2.5) % 2], aside, "flat");
  if (now === "up" && 2.0 <= u && u < 4.5) return sat("alert", 1, sway(t, true));
  if (now === "stretch" && 2.0 <= u && u < 3.75 && sill.wide) return pose(SHAPES.bow, x, 0.0, aside);
  if (now === "sit" && 5.0 <= u && u < 5.25) return sat("twitch", aside);
  return sat("sit");
}

// ---- the cat and the pointer ----

/** Python's `a // b` of floats. */
export function floorDiv(a, b) {
  const m = a % b;
  let div = (a - m) / b;
  if (m && b < 0 !== m < 0) div -= 1.0;
  if (div) {
    const f = Math.floor(div);
    return div - f > 0.5 ? f + 1.0 : f;
  }
  return 0.0;
}

const sign = (v) => (v > 0 ? 1 : v < 0 ? -1 : 0);
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** A cat that hunts the pointer over its sill. */
export class Cat {
  constructor() {
    this.sill = null;
    this.pointer = null;
    this.mode = "idle";
    this.light = 1.0;
    this.t0 = Infinity;
    this.n = 0;
  }

  /** It does more than sit at home. */
  get busy() {
    return this.mode !== "idle";
  }

  get now() {
    return this.t0 + this.n * DT;
  }

  /** The cat at `t`. */
  at(t, pointer, sill, light = 1.0, prey = null) {
    this.light = light;
    if (pointer === null || (pointer.at === null && pointer.since === -Infinity)) {
      this.mode = "idle";
      this.sill = null;
      this.t0 = Infinity;
      return idle(sill, t, light, prey);
    }
    if (this.sill === null || sill.key !== this.sill.key || pointer !== this.pointer || t < this.now - DT) this.start(sill, pointer, t);
    while (this.now + DT <= t + 1e-9) {
      const gone = pointer.at === null && this.now >= pointer.since;
      if (this.mode === "idle" && gone) {
        this.n += Math.trunc((t - this.now) / DT);
        break;
      }
      this.step();
    }
    return this.pose(t, prey);
  }

  start(sill, pointer, t) {
    this.sill = sill;
    this.pointer = pointer;
    let start = pointer.since;
    if (pointer.trail.length) start = Math.min(start, pointer.trail[0][2]);
    start = Math.max(start, t - 40.0);
    this.t0 = Math.floor(start / DT) * DT;
    this.n = 0;
    this.mode = "idle";
    this.since = this.t0;
    this.wait = 0.0;
    this.k = 0;
    this.x = sill.home;
    this.z = this.vx = this.vz = 0.0;
    this.face = this.paw = 1;
    this.dist = 0.0;
    this.fun = 0.0;
    this.cell = null; // of the pointer, as last seen
    this.ignored = null;
    this.spot = null; // where it saw it last
    this.lost = -Infinity;
    this.caught = this.steep = this.dash = false;
    this.swats = 0;
    this.gap = 3.0;
    this.top = 1.0;
    this.shuffle = false;
  }

  // What it sees.

  /** The cell of the pointer at `now`. */
  see(now) {
    const p = this.pointer;
    if (p.at === null) {
      if (now >= p.since) return null;
    } else if (now < p.since) return null;
    let seen = null;
    for (const [x, y, at] of p.trail) {
      if (at > now) break;
      seen = [x, y];
    }
    if (seen === null) seen = p.trail.length ? p.trail[0].slice(0, 2) : p.at;
    return seen;
  }

  noise(salt) {
    return noise(this.sill.seed + 130 + salt, this.k);
  }

  go(mode, wait = 0.0) {
    this.mode = mode;
    this.since = this.now;
    this.wait = wait;
    this.k += 1;
    this.shuffle = false;
    if (SAT.has(mode)) this.aside(this.cell === null);
  }

  step() {
    const sill = this.sill;
    const now = this.now;
    const seen = this.see(now);
    const same = (a, b) => a !== null && b !== null && a[0] === b[0] && a[1] === b[1];
    if (seen !== null) {
      if (!same(seen, this.cell)) {
        if (this.cell === null) this.fun = Math.max(this.fun, 0.7);
        else {
          const far = Math.abs(seen[0] - this.cell[0]) + 2 * Math.abs(seen[1] - this.cell[1]);
          this.fun = Math.min(1.0, this.fun + 0.1 + 0.03 * far);
        }
        this.cell = seen;
      }
      this.spot = [seen[0], sill.rows - 1.5 - 2 * seen[1]];
    } else if (this.cell !== null) {
      this.cell = this.ignored = null;
      this.lost = now;
    }
    this.fun *= Math.exp(-DT / 8.0);
    const fresh = seen !== null && !same(seen, this.ignored);
    this["_" + this.mode](now - this.since, seen !== null, fresh);
    this.n += 1;
  }

  // How it moves.

  /** A step of a run to `goal`, `top` pixels a second at most. True:
   * it is there. */
  move(goal, top) {
    const sill = this.sill;
    const dx = goal - this.x;
    let want = Math.abs(Math.min(top, Math.sqrt(2 * ACC * Math.abs(dx))));
    if (dx < 0 || Object.is(dx, -0)) want = -want;
    if (Math.abs(dx) < 0.1) want = 0.0;
    const dv = want - this.vx;
    const lim = ACC * DT * (dv * this.vx < 0 ? 2.0 : 1.0);
    this.vx += Math.min(lim, Math.max(-lim, dv));
    const x = clamp(this.x + this.vx * DT, sill.lo, sill.hi);
    this.dist += Math.abs(x - this.x);
    this.x = x;
    if (Math.abs(this.vx) > 0.5) this.face = sign(this.vx);
    if (Math.abs(goal - this.x) < 0.35 && Math.abs(this.vx) < 3.0) {
      this.vx = 0.0;
      return true;
    }
    return false;
  }

  /** Off a mullion before it sits; `shown`: and from behind a curtain.
   * True: on its way. */
  aside(shown = false) {
    const goal = this.sill.stop(this.x, shown);
    if (Math.abs(goal - this.x) < 0.5 && !this.shuffle) return false;
    this.shuffle = !this.move(goal, 5.0);
    return this.shuffle;
  }

  /** How far the pointer is to the right, how high, and whether a leap
   * gets there. */
  aim() {
    const sill = this.sill;
    const [sx, pz] = this.spot;
    const px = clamp(sx, sill.lo, sill.hi);
    return [px - this.x, pz, pz <= sill.high + REACH];
  }

  /** How far to the right it stands up under the pointer. */
  under() {
    return this.sill.stop(this.spot[0]) - this.x;
  }

  think(here) {
    const sill = this.sill;
    if (this.spot === null || (!here && this.now - this.lost > 1.0)) {
      this.go("seek");
      return;
    }
    const [d, , reach] = this.aim();
    if (here && this.fun < 0.15) this.go("watch", 1.0);
    else if (!reach) this.go(Math.abs(this.under()) > 1.5 ? "run" : "beg", 1.0 + this.noise(1));
    else if (Math.abs(d) > sill.range) {
      this.gap = 2.5 + this.noise(2) * Math.max(0.0, sill.range - 3.5);
      this.dash = this.noise(3) < 0.35 + 0.5 * this.fun;
      this.go("run");
    } else this.crouch();
  }

  crouch() {
    const d = this.aim()[0];
    if (Math.abs(d) > 0.75) this.face = sign(d);
    this.vx = 0.0;
    this.go("wiggle", (0.4 + 0.45 * this.noise(4)) * (1.25 - 0.5 * this.fun));
  }

  launch(up = false) {
    const sill = this.sill;
    const [px, pz] = this.spot;
    const land = clamp(px, sill.lo, sill.hi);
    const d = Math.min(sill.range + 1.0, Math.max(-sill.range - 1.0, land - this.x));
    const high = up ? sill.high : Math.min(sill.high, Math.max(1.5, pz - 2.5));
    this.vz = this.top = Math.sqrt(2 * G * high);
    this.vx = up ? 0.0 : (d * G) / (2 * this.vz);
    this.steep = up || Math.abs(d) < 1.5;
    if (!this.steep) this.face = sign(d);
    this.paw = sign(px - this.x) || this.paw;
    this.z = 0.01;
    this.caught = false;
    this.fun = Math.max(0.0, this.fun - 0.05);
    this.go("pounce");
  }

  // What it does: `u` seconds in the mode; the pointer is `here`;
  // `fresh`: here, and not one it turned away from.

  _idle(u, here, fresh) {
    if (!fresh) return;
    const a = act(this.sill.seed, Math.trunc(floorDiv(this.now, SLOT)), this.light);
    this.go(a === "sleep" || a === "loaf" ? "wake" : "alert", NOTICE);
  }

  _wake(u) {
    if (u >= 0.7) this.go("alert", NOTICE);
  }

  _alert(u, here) {
    if (this.aside()) return;
    if (this.spot !== null) this.paw = sign(this.spot[0] - this.x) || this.paw;
    if (u >= this.wait) this.think(here);
  }

  _run(u, here) {
    const sill = this.sill;
    if (!here && this.now - this.lost > 0.6) {
      this.go("seek");
      return;
    }
    const [d, , reach] = this.aim();
    let goal = this.x + this.under();
    if (reach) goal = Math.abs(d) > this.gap ? this.x + d - sign(d) * this.gap : this.x;
    const there = this.move(goal, sill.run);
    if (!reach) {
      if (there) this.go("beg", 1.0 + this.noise(1));
    } else if (Math.abs(d) <= sill.range && this.dash && Math.abs(this.vx) > 0.5 * sill.run) this.launch();
    else if (there) this.crouch();
  }

  _wiggle(u, here) {
    const [d, , reach] = this.aim();
    if (here && (Math.abs(d) > this.sill.range + 1.5 || !reach) && u > 0.15) this.think(here);
    else if (u >= this.wait) this.launch();
  }

  _pounce(u, here) {
    const sill = this.sill;
    this.vz -= G * DT;
    this.z += this.vz * DT;
    this.x = clamp(this.x + this.vx * DT, sill.lo, sill.hi);
    if (here) {
      const [d, pz] = this.aim();
      if (Math.abs(d) <= 2.5 && this.z - 1.0 <= pz && pz <= this.z + 6.5) this.caught = true;
    }
    if (this.z <= 0.0) {
      this.z = this.vx = this.vz = 0.0;
      this.go("land");
    }
  }

  _land(u, here) {
    if (u < 0.16) return;
    if (!here) {
      // no second leap at what is gone
      this.lost = -Infinity;
      this.go("miss");
      return;
    }
    const [d, , reach] = this.aim();
    if (!reach) this.go(Math.abs(this.under()) > 1.5 ? "run" : "beg", 1.0 + this.noise(1));
    else if (this.caught && Math.abs(d) <= 3.0) {
      this.fun = Math.min(1.0, this.fun + 0.1);
      this.swats = 2 + Math.trunc(this.noise(5) * 3);
      this.go("hold");
    } else this.go("miss");
  }

  _miss(u, here) {
    if (this.aside(!here)) return;
    const far = here && Math.abs(this.aim()[0]) > this.sill.range;
    if (u >= (far ? 0.25 : 0.55)) this.think(here);
  }

  _hold(u, here) {
    if (!here) {
      if (this.now - this.lost > 0.5) this.go("seek");
      return;
    }
    const d = this.aim()[0];
    if (Math.abs(d) > 3.5) {
      this.go("alert", 0.12);
      return;
    }
    if (this.aside()) return;
    this.paw = sign(d) || this.paw;
    if (u >= 0.32 * this.swats) this.go("watch", 0.9 + 1.6 * this.noise(6));
  }

  _watch(u, here) {
    if (!here) {
      if (this.now - this.lost > 0.5) this.go("seek");
      return;
    }
    if (this.aside()) return;
    const [d, pz, reach] = this.aim();
    this.paw = sign(d) || this.paw;
    if (Math.abs(d) > this.sill.range && this.fun >= 0.15) this.go("alert", 0.12);
    else if (u < this.wait) return;
    else if (this.fun < 0.15) {
      this.ignored = this.cell;
      this.go("bored");
    } else if (!reach) this.go("beg", 1.0 + this.noise(1));
    else if (Math.abs(d) <= 3.0 && pz <= 6.0) {
      this.swats = 1 + Math.trunc(this.noise(5) * 2);
      this.go("hold");
    } else this.crouch();
  }

  _beg(u, here) {
    if (!here) {
      if (this.now - this.lost > 0.5) this.go("seek");
      return;
    }
    if (this.aim()[2] || Math.abs(this.under()) > 1.5) this.go("alert", 0.12);
    else if (this.aside()) return;
    else if (this.fun < 0.15) {
      this.ignored = this.cell;
      this.go("bored");
    } else if (u >= this.wait) this.launch(true);
  }

  _bored(u, here, fresh) {
    if (fresh) this.go("alert", 0.2);
    else if (u >= 2.6) this.go("home");
  }

  _seek(u, here, fresh) {
    if (fresh) this.go("alert", 0.15);
    else if (this.aside(true)) return;
    else if (u >= 1.3 + 1.2 + 2.0 * this.noise(7)) this.go("home");
  }

  _home(u, here, fresh) {
    if (fresh) {
      this.vx = 0.0;
      this.go("alert", 0.2);
    } else if (this.move(this.sill.home, 4.5)) {
      this.x = this.sill.home;
      this.go("settle");
    }
  }

  _settle(u, here, fresh) {
    if (fresh) this.go("alert", NOTICE);
    else if (u >= 0.4) this.go("idle");
  }

  // What it looks like.

  pose(t, prey) {
    const sill = this.sill;
    const mode = this.mode;
    const u = t - this.since;
    if (mode === "idle") return idle(sill, t, this.light, prey);
    const x = this.x;
    const face = this.face;
    const toward = this.paw || 1;
    const [d, pz] = this.spot !== null ? [this.spot[0] - x, this.spot[1]] : [0.0, 0.0];
    const aside = (name, z = 0.0, way = null) => pose(SHAPES[name], x, z, way || face);
    const sat = (name, way, tail = "rest") => pose(SHAPES[name], x, 0.0, way, tail, sill.tail(x, -toward));
    const gait = (names, stride, hop = 0) => {
      const i = Math.trunc(this.dist / stride) % 2;
      return aside(names[i], hop * (1 - i));
    };

    if (this.shuffle || mode === "home") return gait(["walk", "step"], 1.5);
    if (mode === "wake") {
      const wide = sill.wide && sill.home === rnd(x);
      return u < 0.35 ? pose(SHAPES[wide ? "loaf" : "squat"], x) : pose(SIT, x, 0.0, 1, "flick", sill.way);
    }
    if (mode === "alert") return sat(Math.abs(d) < 2 ? "alert" : "stare", toward, "flick");
    if (mode === "run") return Math.abs(this.vx) < 4.0 ? gait(["walk", "step"], 1.5) : gait(["bound", "gather"], 2.5, 1);
    if (mode === "wiggle") return aside(["crouch", "wiggle"][Math.trunc(t * 7) % 2]);
    if (mode === "pounce") {
      if (this.steep) return pose(SHAPES.leap, x, this.z, toward);
      const k = this.vz / this.top;
      return aside(k > 0.4 ? "rise" : k < -0.4 ? "fall" : "fly", this.z);
    }
    if (mode === "land") return this.steep ? pose(SHAPES.squat, x) : aside("crouch");
    if (mode === "miss") return sat("stare", Math.trunc(u / 0.18) % 2 ? toward : -toward, "flick");
    if (mode === "hold") {
      const beat = mod(u, 0.32) < 0.14;
      return sat(beat || pz > 6.0 ? "raise" : "swat", toward, "flick");
    }
    if (mode === "watch") return sat(Math.abs(d) < 2 ? "alert" : pz < 8 ? "look" : "stare", toward, sway(t, this.fun > 0.3));
    if (mode === "beg") return sat("beg", Math.trunc(u / 0.3) % 2 ? 1 : -1, "flat");
    if (mode === "bored") return sat(["lick", "look"][Math.trunc(t * 2.5) % 2], -toward, "flat");
    if (mode === "seek") {
      if (u < 1.3) return sat("stare", 0.5 <= u && u < 0.9 ? -toward : toward, "flick");
      return sat(Math.trunc(u / 1.1) % 2 ? "look" : "sit", toward, sway(t, true));
    }
    // settle
    return pose(SHAPES.look, x, 0.0, face, "rest", sill.way);
  }
}
