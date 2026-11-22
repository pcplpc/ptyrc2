// SPDX-License-Identifier: GPL-3.0-or-later
// Fire: one widget, from a candle (3x3) to a hearth (45x13).
//
// `scene(t, w, h, look, poke)` is pure: the flame is a field of heat,
// a function of the place and the time (tongues that lean and lick,
// turbulence that rises); any moment can be drawn. The deck draws the
// variant shown every `PERIOD` ms; the driver picks the variant and
// the place.
//
// Bare: a flame of glyphs, ` .:^*x#`, red to yellow. `-b`: half
// blocks, two rows of heat a cell. `-l`: what burns, a candle on its
// dish, a brazier, logs in a hearth (border `hearth`); in glyphs too
// without `-b`. `-c`: it burns down to embers over `--burn` seconds,
// then a log is put on. `-k`: sparks rise and die. `-g`: the light of
// the fire, a halo; with `-l`, the bricks behind the logs. `-a`: all.
// Hover (`Poke`): the flames rise and sparks fly for a few seconds.

import { exit, log, period, random } from "ptyrc";
import { Deck } from "ptyrc/deck";

export const SIZES = [
  [3, 3],
  [5, 4],
  [9, 5],
  [15, 7],
  [23, 9],
  [33, 11],
  [45, 13],
];
export const PERIOD = 100; // ms between two draws
export const GLYPHS = " .:^*x#";
/** Heat to a colour of the 256, cold to hot. */
export const RAMP = [52, 88, 124, 160, 196, 202, 208, 214, 220, 226, 227, 228, 229, 230];
export const BURN = 3600; // seconds from a log to the next
export const FULL = 0.85; // the fuel of a fire that does not burn down
export const POKED = 4; // seconds a poke lasts
const [WAX, WICK, BRASS, IRON] = [230, 238, 136, 240];
const BARK = [94, 58, 95];
const WOOD = [130, 136, 137]; // the bark in glyphs: thin strokes, brighter
const GLOW = [172, 178, 180]; // and lit by the flames behind
const GRAIN = "=-~";
export const COALS = ":*#"; // bark and embers, in glyphs
export const LIT = [131, 95, 52, 237, 235]; // bricks, near the flames to far

/** A mantel over jambs of brick. */
export const HEARTH = {
  corners: "▛▜▙▟",
  top: "▀",
  bottom: "▄",
  left: "▌",
  right: "▐",
  style: { fg: 131 },
  title: { style: { fg: 180 } },
};

/** What is drawn, as the options name it. */
export function look(o = {}) {
  return {
    blocks: !!o.blocks,
    logs: !!o.logs,
    cycle: !!o.cycle,
    sparks: !!o.sparks,
    glow: !!o.glow,
    seed: o.seed ?? 1,
    burn: o.burn ?? BURN,
  };
}

/** A hash of the lattice, 0 to 1 (uint32 arithmetic). */
export function hash(seed, x, y) {
  let n = (Math.imul(x, 374761393) + Math.imul(y, 668265263) + Math.imul(seed, 2246822519 | 0)) >>> 0;
  n = Math.imul((n ^ (n >>> 13)) >>> 0, 1274126177) >>> 0;
  return ((n ^ (n >>> 16)) & 0xffff) / 0xffff;
}

/** Value noise in [0, 1], smooth in `x` and `y`. */
export function noise(seed, x, y) {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  let fx = x - xi;
  let fy = y - yi;
  fx = fx * fx * (3 - 2 * fx);
  fy = fy * fy * (3 - 2 * fy);
  const top = hash(seed, xi, yi) * (1 - fx) + hash(seed, xi + 1, yi) * fx;
  const low = hash(seed, xi, yi + 1) * (1 - fx) + hash(seed, xi + 1, yi + 1) * fx;
  return top * (1 - fy) + low * fy;
}

/** How much burns at `t` seconds, 0 to 1. With `cycle`: a log catches,
 * burns down to embers over `burn` seconds, and the next is put on. */
export function fuel(t, lk) {
  if (!lk.cycle) return FULL;
  const p = (t / lk.burn) % 1;
  if (p < 0.02) return 0.3 + (0.7 * p) / 0.02; // it catches
  if (p < 0.7) return 1 - (0.5 * (p - 0.02)) / 0.68;
  if (p < 0.9) return 0.5 - (0.42 * (p - 0.7)) / 0.2; // it dies
  return 0.08 - (0.04 * (p - 0.9)) / 0.1; // embers
}

function strength(t, lk, poke) {
  let f = fuel(t, lk);
  if (poke !== null && poke >= 0 && poke < POKED) f = Math.min(1.25, f + 0.45 * Math.exp(-poke / 1.2));
  return f;
}

const clamp = (v) => Math.max(0, Math.min(1, v));

/** Heat, 0 to 1, at column `x`, `y` half rows over the bed of a fire
 * `w` columns wide that may rise `top` half rows, of strength `f`.
 * `spread`: the share of the width the bed takes. */
export function heat(x, y, t, w, top, f, seed, spread = 0.94) {
  const half = w / 2;
  const cx = (w - 1) / 2;
  if (w <= 5) {
    // One flame: a tear that sways from its foot.
    const sway = (noise(seed, 7, t * 1.1) - 0.5) * 1.6;
    const u = (x - cx - sway * (y / Math.max(top, 1)) ** 1.5) / (half * 0.75);
    if (Math.abs(u) >= 1) return 0;
    const rise = top * (0.7 + 0.3 * f) * (0.9 + 0.2 * noise(seed, 3, t * 4));
    const v = y / (rise * (1 - Math.abs(u) ** 1.6));
    return v < 1 ? clamp((1 - v) * 2.4) : 0;
  }
  const r = riseOf(x, t, w, top, f, seed, spread);
  return r ? flame(x, y, t, r, leanOf(y, t, seed), seed) : 0;
}

/** How high the flame of column `x` stands, in half rows; 0: none. */
function riseOf(x, t, w, top, f, seed, spread) {
  const u = (x - (w - 1) / 2) / ((w / 2) * spread);
  if (Math.abs(u) >= 1) return 0;
  // Tongues: the height wanders along the bed and in time.
  const lick = noise(seed, x * 0.42 + 50, t * 1.4);
  const wide = noise(seed + 3, x * 0.13, t * 0.5);
  let rise = top * f * (1 - u * u) ** 0.7 * (0.2 + 0.72 * lick ** 1.3 + 0.36 * wide);
  rise = Math.min(rise, top * 1.05);
  return rise > 0.05 ? rise : 0;
}

/** How far the flames lean at half row `y`. */
function leanOf(y, t, seed) {
  return (noise(seed + 5, y * 0.1, t * 0.7) - 0.5) * 2.5;
}

/** Heat at `(x, y)` in a flame `rise` half rows high. */
function flame(x, y, t, rise, lean, seed) {
  const v = y / rise;
  if (v >= 1) return 0;
  const churn = noise(seed + 1, x * 0.5 + lean, y * 0.33 - t * 3.2);
  const fine = noise(seed + 2, x * 1.3, y * 0.9 - t * 5.5);
  return clamp((1 - v) ** 0.8 * (0.62 + 0.6 * churn) + 0.18 * (fine - 0.5));
}

/** The bed: a slow glow along the bottom, 0 to 1. */
function embers(x, t, w, seed) {
  const u = (x - (w - 1) / 2) / (w / 2);
  return Math.max(0, noise(seed + 9, x * 0.9, t * 0.35) * (1 - Math.abs(u) ** 3));
}

const colour = (v) => RAMP[Math.min(RAMP.length - 1, Math.floor(v * RAMP.length))];

/** A generator of `[0, 1)` from three integers: the same draws for the
 * same three. */
function draws(a, b, c) {
  let s = (Math.imul(a, 0x9e3779b1) ^ Math.imul(b, 0x85ebca77) ^ Math.imul(c, 0xc2b2ae3d)) >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let z = s;
    z = Math.imul(z ^ (z >>> 15), z | 1);
    z ^= z + Math.imul(z ^ (z >>> 7), z | 61);
    return ((z ^ (z >>> 14)) >>> 0) / 4294967296;
  };
}

/** The sparks in flight at `t` over a fire `w` by `h` cells of strength
 * `f`: a `Map` of `"x,y"` to a cell. A slot throws one spark a period,
 * the same at the same time; a poke `poke` seconds old throws a burst. */
export function sparks(t, w, h, f, seed, poke = null) {
  const out = new Map();
  const cx = (w - 1) / 2;
  const fly = (rnd, age, life) => {
    if (!(age >= 0 && age < life)) return;
    const u = (a, b) => a + (b - a) * rnd();
    const x0 = cx + u(-0.3, 0.3) * w;
    const speed = u(0.5, 1) * h; // rows per second
    const drift = u(-1.5, 1.5);
    const swing = u(0.3, 1);
    const phase = u(0, 2 * Math.PI);
    const k = age / life;
    const up = speed * life * (k - 0.45 * k * k); // it brakes
    const x = Math.round(x0 + drift * age + swing * Math.sin(phase + 5 * age));
    const y = Math.round(h - 1 - 0.3 * h * f - up);
    if (x < 0 || x >= w || y < 0 || y >= h) return;
    let cell;
    if (k < 0.4) cell = { t: "*", fg: 227, bold: true };
    else if (k < 0.75) cell = { t: "·", fg: 214, bold: true };
    else cell = { t: ".", fg: 166 };
    out.set(`${x},${y}`, cell);
  };
  if (w < 3 || h < 3) return out;
  for (let slot = 0; slot < Math.max(1, Math.floor(w / 6)); slot++) {
    const base = draws(seed, 7919, slot);
    const per = 1.6 + 2.9 * base();
    const offset = 5 * base();
    const nth = Math.floor((t + offset) / per);
    const age = t + offset - nth * per;
    const rnd = draws(seed, 104729 + slot, nth);
    if (rnd() < 0.25 + 0.75 * f) fly(rnd, age, 0.7 + 0.8 * rnd()); // a low fire throws few
  }
  if (poke !== null && poke >= 0 && poke < POKED) {
    const nth = Math.floor((t - poke) * 10); // the burst of this poke
    for (let i = 0; i < 3 + Math.floor(w / 4); i++) {
      const rnd = draws(seed, 15485863 + i, nth);
      fly(rnd, poke - 0.8 * rnd(), 0.8 + rnd());
    }
  }
  return out;
}

const center = (s, w) => {
  const pad = w - s.length;
  if (pad <= 0) return s;
  // As Python's str.center: the odd cell on the right, but for an odd
  // width of an odd pad, on the left.
  const left = Math.floor(pad / 2) + (pad & w & 1);
  return " ".repeat(left) + s + " ".repeat(pad - left);
};

/** What burns, with `logs`: its rows, the lowest of the box, and how
 * many of them the flames may lick. `blocks`: in blocks; else glyphs. */
function furniture(t, w, h, f, seed, blocks) {
  const cells = (text, fg) => [...text].map((c) => (c !== " " ? { t: c, fg } : { t: " " }));
  if (w <= 5) {
    // A candle; on a dish when there is a row for it.
    const [wax, dish] = blocks ? [w < 5 ? "█" : "▐█▌", "▀▀▀"] : [w < 5 ? "H" : "|_|", "==="];
    const body = cells(center(wax, w), WAX);
    if (h < 4) return [[body], 0];
    return [[body, cells(center(dish, w), BRASS)], 0];
  }
  if (w <= 12) {
    const rim = blocks ? `◥${"█".repeat(w - 2)}◤` : `\\${"_".repeat(w - 2)}/`;
    const bowl = cells(rim, IRON);
    for (let x = 1; x < w - 1; x++) {
      // Coals show along the rim.
      const g = embers(x, t, w, seed) * (0.5 + 0.5 * f);
      if (g > 0.45 && blocks) bowl[x] = { t: "▀", fg: colour(g * 0.6), bg: IRON };
      else if (g > 0.45) bowl[x] = { t: g < 0.7 ? "o" : "*", fg: colour(g * 0.6), bold: true };
    }
    const legs = cells(` /${" ".repeat(w - 4)}\\ `, IRON);
    return [[bowl, legs], 0];
  }
  // Two logs, the shorter across the longer, on andirons.
  const long = w - 6;
  let short = Math.max(5, Math.floor((long * 2) / 3));
  let lean = Math.floor((long - short) / 3);
  const hotOf = (row, x) => (noise(seed + 20 + row, x * 0.8, t * 0.5) - 0.3 - 0.3 * Math.min(1, f)) * 2.2;
  const log = (n, row) => {
    const out = [];
    for (let i = 0; i < n; i++) {
      const x = i + row * 17;
      const bark = BARK[Math.floor(hash(seed + row, x, 0) * BARK.length)];
      const glyph = hash(seed + row, x, 1) < 0.7 ? "▓" : "▒";
      // More of them as the fire dies: the logs turn to coals.
      const hot = hotOf(row, x);
      if (hot > 0.12) out.push({ t: "▒", fg: colour(Math.min(0.55, hot)), bg: 52 });
      else out.push({ t: glyph, fg: bark });
    }
    return [{ t: "▐", fg: 137 }, ...out.slice(1, -1), { t: "▌", fg: 137 }];
  };
  // A log in glyphs, `at` cells along its row: bark `=-~`, the cut end
  // `(@)` on the left of a lower log, on the right of an upper.
  const twig = (n, row, at = 0) => {
    const out = [];
    for (let i = 0; i < n; i++) {
      const x = at + i + row * 17;
      const bark = WOOD[Math.floor(hash(seed + row, x, 0) * WOOD.length)];
      const grain = hash(seed + row, x, 1);
      const glyph = GRAIN[grain < 0.62 ? 0 : grain < 0.86 ? 1 : 2];
      const hot = hotOf(row, x);
      if (hot > 0.12) {
        const coal = COALS[Math.min(COALS.length - 1, Math.floor((hot - 0.12) * 5))];
        out.push({ t: coal, fg: colour(Math.min(0.6, hot)), bold: true });
      } else out.push({ t: glyph, fg: bark });
    }
    const end = [..."(@)"].map((c) => ({ t: c, fg: c === "@" ? 180 : 137 }));
    if (row) return [{ t: "(", fg: 137 }, ...out.slice(1, n - 3), ...end];
    return [...end, ...out.slice(3, -1), { t: ")", fg: 137 }];
  };
  // `count` logs end to end over `total` cells.
  const pile = (total, row, count) => {
    const out = [];
    for (let i = 0; i < count; i++) {
      const n = Math.floor((total * (i + 1)) / count) - Math.floor((total * i) / count);
      out.push(...twig(n, row, out.length));
    }
    return out;
  };
  let count = 1;
  if (!blocks && long >= 24) {
    // A pile: a log each dozen cells, one fewer across them.
    count = Math.floor(long / 12);
    short = Math.floor((long * (count - 1)) / count);
    lean = Math.floor((long - short) / 2);
  }
  const lay = blocks ? (n, row) => log(n, row) : pile;
  const blank = () => Array.from({ length: w }, () => ({ t: " " }));
  const low = blank();
  low[1] = low[w - 2] = { t: "╨", fg: IRON };
  low.splice(3, long, ...lay(long, 0, count));
  const high = blank();
  high[1] = high[w - 2] = { t: "╥", fg: IRON };
  high.splice(3 + lean, short, ...lay(short, 1, Math.max(1, count - 1)));
  return [[high.slice(0, w), low.slice(0, w)], 1];
}

/** A wall of bricks three cells long, a course a row. */
const brick = (x, y) => "┬──┴──"[(x + 3 * (y % 2)) % 6];

/** The fire at `t` seconds in a box of `w` by `h` cells: rows of runs.
 * `poke`: seconds since it was poked, null when it was not. */
export function scene(t, w, h, lk = look(), poke = null) {
  const { seed } = lk;
  const f = strength(t, lk, poke);
  const grid = Array.from({ length: h }, () => Array.from({ length: w }, () => ({ t: " " })));
  let floor = h; // rows of flame: those over the furniture
  let licked = 0;
  if (lk.logs) {
    const [rows, n] = furniture(t, w, h, f, seed, lk.blocks);
    licked = n;
    floor = h - rows.length;
    grid.splice(floor, rows.length, ...rows);
  }
  const bed = floor + licked;
  let top = 2 * bed; // half rows
  if (lk.logs && w <= 5) top = 2 * floor;
  const spread = lk.logs && w > 12 ? 0.82 : 0.94; // as wide as the logs
  // A column has one height, a row one lean: worked out once.
  const rises = w > 5 ? Array.from({ length: w }, (_, x) => riseOf(x, t, w, top, f, seed, spread)) : [];
  const leans = new Map();
  const at = (x, y) => {
    let v = 0;
    if (w <= 5) v = heat(x, y, t, w, top, f, seed, spread);
    else if (y < rises[x]) {
      let lean = leans.get(y);
      if (lean === undefined) leans.set(y, (lean = leanOf(y, t, seed)));
      v = flame(x, y, t, rises[x], lean, seed);
    }
    if (y < 1 && w > 5) v = Math.max(v, 0.45 * embers(x, t, w, seed));
    return v;
  };
  let lit = 0; // the heat of the whole, for the light
  for (let row = 0; row < bed; row++) {
    const y = 2 * (bed - 1 - row); // half rows from the bed, the lower
    for (let x = 0; x < w; x++) {
      let low;
      let high;
      if (lk.blocks) [low, high] = [at(x, y + 0.5), at(x, y + 1.5)];
      else low = high = at(x, y + 1);
      lit += low + high;
      const under = grid[row][x];
      if (under.t !== " ") {
        // A log in the flames: lit from behind.
        if ("bg" in under || Math.max(low, high) < 0.25) continue;
        if (lk.blocks) grid[row][x] = { ...under, bg: colour(0.55 * Math.max(low, high)) };
        else if (WOOD.includes(under.fg)) grid[row][x] = { ...under, fg: GLOW[WOOD.indexOf(under.fg)] };
        continue;
      }
      if (!lk.blocks) {
        const glyph = GLYPHS[Math.min(GLYPHS.length - 1, Math.floor(low * GLYPHS.length))];
        if (glyph !== " ") grid[row][x] = { t: glyph, fg: colour(low), bold: low > 0.5 };
      } else if (low >= 0.07 && high >= 0.07) grid[row][x] = { t: "▀", fg: colour(high), bg: colour(low) };
      else if (low >= 0.07) grid[row][x] = { t: "▄", fg: colour(low) };
      else if (high >= 0.07) grid[row][x] = { t: "▀", fg: colour(high) };
    }
  }
  if (lk.logs && w <= 5 && floor >= 1) {
    // The wick, where the flame stands.
    const x = Math.floor(w / 2);
    if (grid[floor - 1][x].t === " ") grid[floor - 1][x] = { t: "╷", fg: WICK };
  }
  if (lk.glow) light(grid, t, w, h, lk, lit / Math.max(1, w * bed * 2), bed);
  if (lk.sparks) {
    for (const [key, cell] of sparks(t, w, floor, f, seed, poke)) {
      const [x, y] = key.split(",").map(Number);
      const under = grid[y][x];
      // Not over the flames.
      if (" ░┬─┴".includes(under.t)) grid[y][x] = "bg" in under ? { ...cell, bg: 52 } : cell;
    }
  }
  return grid.map(runs);
}

/** The light of the fire on the free cells: a halo that breathes with
 * the flames; behind logs, bricks. */
function light(grid, t, w, h, lk, lit, bed) {
  const flicker = 0.75 + 0.5 * noise(lk.seed + 11, 0, t * 6);
  const power = Math.min(1, (0.25 + 2.2 * lit) * flicker);
  const cx = (w - 1) / 2;
  const cy = bed - 1;
  const wall = lk.logs && w > 12;
  for (let y = 0; y < (wall ? h : bed); y++) {
    for (let x = 0; x < w; x++) {
      if (grid[y][x].t !== " ") continue;
      const near = Math.hypot((x - cx) / (w / 2), ((y - cy) / Math.max(1, bed)) * 0.9);
      const l = power * Math.max(0, 1.15 - near);
      if (wall) {
        if (y >= bed) continue;
        const level = Math.min(LIT.length - 1, Math.floor((1 - Math.min(1, l * 1.3)) * LIT.length));
        grid[y][x] = { t: brick(x, y), fg: LIT[level] };
      } else if (l > 0.62) grid[y][x] = { t: " ", bg: 52 };
      else if (l > 0.42) grid[y][x] = { t: "░", fg: 52 };
    }
  }
}

const same = (a, b) => {
  const ka = Object.keys(a).filter((k) => k !== "t" && a[k] !== false);
  const kb = Object.keys(b).filter((k) => k !== "t" && b[k] !== false);
  return ka.length === kb.length && ka.every((k) => a[k] === b[k]);
};

/** Cells to runs: neighbours of one style joined; `bold: false` dropped. */
function runs(cells) {
  const out = [];
  for (const c of cells) {
    const last = out[out.length - 1];
    if (last && same(last, c)) last.t += c.t;
    else {
      const run = { ...c };
      if (run.bold === false) delete run.bold;
      out.push(run);
    }
  }
  return out;
}

/** The hover: a poke when the pointer comes over the fire, and one more
 * each `again` ms it keeps moving there. Session ms. */
export class Poke {
  constructor(again = POKED * 1000) {
    this.again = again;
    this.at = null;
    this.over = false;
  }

  move(now) {
    if (!this.over || this.at === null || now - this.at >= this.again) this.at = now;
    this.over = true;
  }

  leave() {
    this.over = false;
  }

  /** Seconds since the last poke; null when it is spent. */
  age(now) {
    if (this.at === null) return null;
    const age = (now - this.at) / 1000;
    return age < POKED ? age : null;
  }
}

/** The options of `init`, `--all` spread; null `burn` refused. */
export function options(args) {
  const all = args.all === true;
  const o = {};
  for (const k of ["blocks", "logs", "cycle", "sparks", "glow"]) o[k] = all || args[k] === true;
  o.burn = Number.isFinite(args.burn) ? args.burn : BURN;
  return o;
}

/** The time a fire starts at: a log well alight. */
export const start = (lk) => (lk.cycle ? 0.03 * lk.burn : 0);

let deck;

export function init(args) {
  const o = options(args);
  if (!(o.burn > 0)) {
    log("fire: --burn: more than 0");
    exit(2);
    return;
  }
  const b = random(4);
  const seed = 1 + (((b[0] | (b[1] << 8) | (b[2] << 16)) >>> 0) % 1000000);
  const lk = look({ ...o, seed });
  const poke = new Poke();
  const render = (w, h) => scene(start(lk) + deck.now / 1000, w, h, lk, poke.age(deck.now));
  deck = new Deck({ border: o.logs ? HEARTH : "round" });
  // The next draw shows it: no draw of its own.
  deck.add("fire", SIZES, render, {
    hover: () => {
      poke.move(deck.now);
      return false;
    },
    leave: () => {
      poke.leave();
      return false;
    },
  });
  period(PERIOD);
}

export function tick(now) {
  deck.tick(now);
}

export function event(topic, data) {
  deck.event(topic, data);
}
