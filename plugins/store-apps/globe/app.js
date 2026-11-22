// SPDX-License-Identifier: GPL-3.0-or-later
// Globe: a snow globe in one widget, from 11x5 to 30x13.
//
// `scene(t, w, h, look, shaken, shakes)` is pure: a flake is a function
// of the time, where it falls, how it swirls after a shake; no state
// but the time of the last shake. No border: the dome is the content.
// The deck draws the variant shown ten times a second, thirty while
// the snow swirls (`swirls`); the driver picks the variant and the
// place.
//
// Bare: a house between two trees under a dome, a little snow falling.
// `-k`: a shake now and then (`--every`, 45 s): the snow swirls, falls
// thick, then thins. `-d`: the snow lies, drifts on the ground, then
// the roof and the trees turn white; a shake lifts it all. `-l`: the
// window flickers, the trees wear a garland, a glint slides along the
// glass. `-r`: a scene at random, another at each shake; `--scene`
// names it. `-a`: all. Hover (`Shaker`): the pointer shakes it.
//
// The flakes draw from Python's generator (`Twister`), seeded as the
// twin seeds it: the same seed, the same snow.

import { exit, log, period, random } from "ptyrc";
import { Deck } from "ptyrc/deck";

export const PERIOD = 100; // ms between two draws
export const BRISK = 33; // while the snow swirls
export const EVERY = 45; // seconds between two shakes
const SETTLE = 20; // seconds for the snow to lie, after a shake
const SWIRL = 1.3; // seconds: the swirl dies by e
const STORM = 6; // seconds: the thick snow thins by e
const THIN = 0.4; // share of the flakes that fall when all is calm
export const [GLASS, GLINT, SNOW, BASE] = [110, 231, 255, 94];
export const FLAKES = "··..*";
export const GARLAND = [196, 226, 46, 51, 201];
/** Ink of a scene: a letter of its mask to a colour and bold. */
export const INK = {
  h: [180, false], // walls
  R: [180, false], // roof: white under the snow
  w: [220, true], // window
  t: [71, false], // tree
  T: [71, false], // top of a tree: white under the snow
  s: [255, true], // snowman
  k: [244, false], // hat, buttons
  o: [208, true], // bulbs
  y: [226, true], // star
  c: [250, false], // cross
};

const art = (text) => {
  const rows = text.replace(/^\n+|\n+$/g, "").split("\n");
  const w = Math.max(...rows.map((r) => r.length));
  return rows.map((r) => r.padEnd(w));
};

/** A globe: its `art`, glass over a base; `ground`: the row the scene
 * stands on; `scale`: which art of a scene fits. */
export class Dome {
  constructor(rows, ground, scale) {
    this.art = rows;
    this.ground = ground;
    this.scale = scale;
    this.size = [rows[0].length, rows.length];
    this._free = null;
  }

  /** First and last cell inside the glass on row `y`. */
  span(y) {
    const row = this.art[y];
    const x0 = row.search(/\S/);
    const x1 = row.length - 1 - [...row].reverse().join("").search(/\S/);
    return [x0 + 1, x1 - 1];
  }

  /** The cells a flake may be in, as `x,y` keys. */
  free() {
    if (this._free) return this._free;
    const out = new Set();
    for (let y = 1; y <= this.ground; y++) {
      const [x0, x1] = this.span(y);
      for (let x = x0; x <= x1; x++) if (this.art[y][x] === " ") out.add(`${x},${y}`);
    }
    this._free = out;
    return out;
  }
}

export const DOMES = [
  new Dome(
    art(String.raw`
  .-'''-.
 /       \
|         |
 \_______/
 /_______\
`),
    3,
    "s",
  ),
  new Dome(
    art(String.raw`
    .-'''''-.
  .'         '.
 /             \
|               |
 \             /
  '._________.'
  /___________\
 '-------------'
`),
    4,
    "m",
  ),
  new Dome(
    art(String.raw`
    _.-'''''''-._
  .'             '.
 /                 \
|                   |
|                   |
|                   |
 \                 /
  '._____________.'
  /_______________\
 '-----------------'
`),
    6,
    "l",
  ),
  new Dome(
    art(String.raw`
        _..-'''''''-.._
     .-'               '-.
   .'                     '.
  /                         \
 |                           |
 |                           |
 |                           |
 |                           |
  \                         /
   '.                     .'
     '-._______________.-'
     /___________________\
    '---------------------'
`),
    9,
    "l",
  ),
];
export const SIZES = DOMES.map((d) => d.size);

/** Art and its mask, one over the other, a blank line between: a
 * letter of `INK` under each glyph. */
const pair = (text) => {
  const [a, m] = text.replace(/^\n+|\n+$/g, "").split("\n\n");
  const rows = a.split("\n");
  const inks = m.split("\n");
  const w = Math.max(...[...rows, ...inks].map((r) => r.length));
  return [rows.map((r) => r.padEnd(w)), inks.map((r) => r.padEnd(w))];
};

/** Scenes: for each, the art and the mask at the three scales. */
export const SCENES = {
  house: {
    s: pair(String.raw`
 /\  ♣
/[]\ ♣

 RR  T
hwwh t
`),
    m: pair(String.raw`
     /\
 ♣  /  \  ♣
♣♣♣/_[]_\♣♣♣

     RR
 T  R  R  T
ttthhwwhhttt
`),
    l: pair(String.raw`
      /\
 ♣   /  \   ♣
♣♣♣ /_[]_\ ♣♣♣

      RR
 T   R  R   T
ttt hhwwhh ttt
`),
  },
  snowman: {
    s: pair(String.raw`
 (") ♣
( : )♣

 sks T
s k st
`),
    m: pair(String.raw`
     ▄
 ♣  (")  ♣
♣♣-( : )-♣♣

     k
 T  sks  T
ttks k sktt
`),
    l: pair(String.raw`
      ▄
 ♣   (")   ♣
♣♣♣-( : )-♣♣♣

      k
 T   sks   T
tttks k skttt
`),
  },
  church: {
    s: pair(String.raw`
 †   ♣
/[]\_♣

 c   T
RwwRht
`),
    m: pair(String.raw`
   †
♣ /\___  ♣
♣♣|[]|_|♣♣♣

   c
T RRRRR  T
tthwwhhhttt
`),
    l: pair(String.raw`
    †
   /\_____
♣ /  \_____\ ♣
♣♣|[]|_[]_|♣♣♣

    c
   RRRRRRR
T R  RRRRRRR T
tthwwhhwwhhttt
`),
  },
  fir: {
    s: pair(String.raw`
 /o\ ♣
/.o.\♣

 ToT T
ttottt
`),
    m: pair(String.raw`
     *
 ♣  /o\  ♣
♣♣♣/.o.\♣♣♣

     y
 T  ToT  T
tttttottttt
`),
    l: pair(String.raw`
      *
     /o\
 ♣  /.o.\  ♣
♣♣♣/o.o.o\♣♣♣

      y
     ToT
 T  ttott  T
ttttotototttt
`),
  },
};

/** What is drawn, as the options name it. */
export function look(o = {}) {
  return {
    shake: !!o.shake,
    drift: !!o.drift,
    lights: !!o.lights,
    random: !!o.random,
    scene: o.scene ?? "house",
    seed: o.seed ?? 1,
    every: o.every ?? EVERY,
  };
}

/** A hash of the lattice, 0 to 1 (uint32 arithmetic). */
export function hash(seed, x, y = 0) {
  let n = (Math.imul(x, 374761393) + Math.imul(y, 668265263) + Math.imul(seed, 2246822519 | 0)) >>> 0;
  n = Math.imul((n ^ (n >>> 13)) >>> 0, 1274126177) >>> 0;
  return ((n ^ (n >>> 16)) & 0xffff) / 0xffff;
}

/** Python's `random.Random(n)`, `n` a whole number >= 0 under 2**53:
 * MT19937 seeded by `init_by_array` of the 32-bit words of `n`. */
export class Twister {
  constructor(n) {
    const key = [];
    do {
      key.push(n % 0x100000000);
      n = Math.floor(n / 0x100000000);
    } while (n > 0);
    const mt = new Uint32Array(624);
    mt[0] = 19650218;
    for (let i = 1; i < 624; i++) mt[i] = Math.imul(1812433253, mt[i - 1] ^ (mt[i - 1] >>> 30)) + i;
    let i = 1;
    let j = 0;
    for (let k = Math.max(624, key.length); k; k--) {
      mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1664525)) + key[j] + j;
      i++;
      j++;
      if (i >= 624) {
        mt[0] = mt[623];
        i = 1;
      }
      if (j >= key.length) j = 0;
    }
    for (let k = 623; k; k--) {
      mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1566083941)) - i;
      i++;
      if (i >= 624) {
        mt[0] = mt[623];
        i = 1;
      }
    }
    mt[0] = 0x80000000;
    this.mt = mt;
    this.at = 624;
  }

  /** The next word, 0 to 2**32 - 1. */
  word() {
    const mt = this.mt;
    if (this.at >= 624) {
      for (let k = 0; k < 624; k++) {
        const y = (mt[k] & 0x80000000) | (mt[(k + 1) % 624] & 0x7fffffff);
        mt[k] = mt[(k + 397) % 624] ^ (y >>> 1) ^ (y & 1 ? 0x9908b0df : 0);
      }
      this.at = 0;
    }
    let y = mt[this.at++];
    y ^= y >>> 11;
    y ^= (y << 7) & 0x9d2c5680;
    y ^= (y << 15) & 0xefc60000;
    return (y ^ (y >>> 18)) >>> 0;
  }

  /** `random()`: [0, 1), 53 bits. */
  random() {
    const a = this.word() >>> 5;
    const b = this.word() >>> 6;
    return (a * 67108864 + b) / 9007199254740992;
  }

  uniform(a, b) {
    return a + (b - a) * this.random();
  }

  /** `choice(seq)`, `seq` of 2 to 2**31 members. */
  choice(seq) {
    const k = 32 - Math.clz32(seq.length);
    for (;;) {
      const r = this.word() >>> (32 - k);
      if (r < seq.length) return seq[r];
    }
  }
}

/** Python's `round`: half to even. */
const round = (x) => {
  let r = Math.round(x);
  if (r - x === 0.5 && r % 2 !== 0) r -= 1;
  return r;
};

/** Python's `%` of floats: the sign of `b`. */
const mod = (a, b) => {
  let m = a % b;
  if (m !== 0 && b < 0 !== m < 0) m += b;
  return m;
};

const TAU = 2 * Math.PI;

/** With `shake`: when the globe last shook by itself, at `t`; once an
 * `every`, not on the beat. null: not yet, or never. */
export function auto(t, lk) {
  if (!lk.shake) return null;
  const nth = Math.floor(t / lk.every);
  for (const k of [nth, nth - 1]) {
    const at = (k + 0.5 + 0.4 * (hash(lk.seed, k) - 0.5)) * lk.every;
    if (k >= 0 && at <= t) return at;
  }
  return null;
}

/** The scene shown: the one named; with `random`, one drawn by the
 * seed, another after each shake. */
export function which(lk, shakes = 0) {
  if (!lk.random) return lk.scene;
  const names = Object.keys(SCENES).sort();
  const first = Math.floor(hash(lk.seed, 99) * names.length);
  return names[(first + shakes) % names.length];
}

const drawn = new Map(); // seed: the draws of each flake

/** The draws of flake `i`: they hang on the seed alone. */
function draws(seed, i) {
  let all = drawn.get(seed);
  if (!all) {
    if (drawn.size > 8) drawn.clear();
    drawn.set(seed, (all = []));
  }
  while (all.length <= i) {
    // In the twin's order; `phase`: of `uniform(0, rows + 1)`, `rows`
    // the dome's.
    const rng = new Twister(seed * 7919 + all.length);
    const d = { speed: rng.uniform(0.5, 1.3) }; // rows a second
    d.lane = rng.random();
    d.phase = rng.random();
    d.sway = rng.uniform(0.3, 1);
    d.pace = rng.uniform(0.6, 1.6);
    d.turn = rng.random();
    d.spin = rng.uniform(4, 9) * rng.choice([-1, 1]);
    all.push(d);
  }
  return all[i];
}

/** The cells of the flakes in the air at `t`, as `[x, y]`; `since`:
 * seconds since the last shake. */
export function flakes(dome, t, seed, since) {
  const free = dome.free();
  const top = 1;
  const low = dome.ground;
  const rows = low - top + 1;
  const [w] = dome.size;
  const swirl = since === null ? 0 : Math.exp(-since / SWIRL);
  const storm = since === null ? 0 : Math.exp(-since / STORM);
  const share = THIN + (1 - THIN) * storm;
  const out = new Map();
  const count = Math.max(5, round(free.size / 4.5));
  for (let i = 0; i < count; i++) {
    if (i >= Math.max(3, share * count)) continue;
    const d = draws(seed, i);
    let y = mod((rows + 1) * d.phase + d.speed * t, rows + 1); // past the ground: it rests
    let [x0, x1] = dome.span(top + Math.min(Math.trunc(y), rows - 1));
    let x = x0 + d.lane * (x1 - x0) + d.sway * Math.sin(d.pace * t + d.turn * TAU);
    if (swirl > 0.02 && since !== null) {
      const angle = d.turn * TAU + d.spin * since;
      x += swirl * 0.45 * w * Math.cos(angle);
      y = mod(y - swirl * rows * (0.6 + 0.4 * Math.sin(angle)), rows + 1);
      [x0, x1] = dome.span(top + Math.min(Math.trunc(y), rows - 1));
      x = x0 + mod(x - x0, x1 - x0 + 1);
    }
    const cx = round(x);
    const cy = top + Math.trunc(y);
    if (free.has(`${cx},${cy}`)) out.set(`${cx},${cy}`, [cx, cy]);
  }
  return [...out.values()].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

/** Seconds at `t` since the last shake, by itself or by a hand. */
function since(t, lk, shaken) {
  const ats = [auto(t, lk), shaken].filter((s) => s !== null && s !== undefined);
  if (!ats.length) return null;
  const last = Math.max(...ats);
  return last > t ? null : t - last;
}

/** Whether the snow still swirls at `t`. */
export function swirls(t, lk, shaken = null) {
  const s = since(t, lk, shaken);
  return s !== null && Math.exp(-s / SWIRL) > 0.02;
}

/** The globe of `w` by `h` cells (one of `SIZES`) at `t` seconds.
 * `shaken`: when a hand last shook it; `shakes`: how many times. */
export function scene(t, w, h, lk = look(), shaken = null, shakes = 0) {
  const dome = DOMES.find((d) => d.size[0] === w && d.size[1] === h);
  const s = since(t, lk, shaken);
  if (lk.shake) shakes += Math.floor(t / lk.every) + (auto(t, lk) !== null ? 1 : 0) - 1;
  const grid = dome.art.map((row, y) => {
    const fg = y < h - (h < 6 ? 1 : 2) ? GLASS : BASE;
    return [...row].map((c) => (c !== " " ? { t: c, fg } : { t: " " }));
  });
  // How much snow lies, 0 to 1.
  let lying = 0;
  if (lk.drift) lying = s === null ? 1 : Math.min(1, Math.max(0, s - 1) / SETTLE);
  const [rows, mask] = SCENES[which(lk, Math.max(0, shakes))][dome.scale];
  const [x0, x1] = dome.span(dome.ground);
  const left = x0 + Math.floor((x1 - x0 + 1 - rows[0].length) / 2);
  rows.forEach((row, dy) => {
    const y = dome.ground - rows.length + 1 + dy;
    for (let dx = 0; dx < row.length; dx++) {
      let glyph = row[dx];
      if (glyph === " ") continue;
      const ink = mask[dy][dx];
      const x = left + dx;
      let [fg, bold] = INK[ink];
      if ((ink === "R" && lying > 0.4) || (ink === "T" && lying > 0.7)) [fg, bold] = [SNOW, true];
      if (lk.lights) {
        if (ink === "w") {
          // A fire inside.
          const flick = hash(lk.seed, Math.trunc(t * 5), x);
          fg = flick < 0.6 ? 220 : flick < 0.85 ? 214 : 228;
        } else if ((ink === "t" || ink === "o") && hash(lk.seed, x, y) < (ink === "t" ? 0.3 : 1)) {
          const step = Math.trunc(t * 1.5 + 5 * hash(lk.seed, y, x));
          [fg, bold] = [GARLAND[(step + x) % GARLAND.length], true];
          if (glyph === "♣" && step % 3) glyph = "•";
        }
      }
      grid[y][x] = { t: glyph, fg, bold };
    }
  });
  for (let x = x0; x <= x1; x++) {
    // Drifts: the ground fills, unevenly.
    const cell = grid[dome.ground][x];
    const level = lying * (0.45 + 0.55 * hash(lk.seed + 3, x));
    if (cell.t === "_" && cell.fg === GLASS && level > 0.2) grid[dome.ground][x] = { t: "_", fg: SNOW, bold: true };
    else if (cell.t === " " && level > 0.2) {
      const glyph = level < 0.45 ? "." : level < 0.75 ? "▁" : "▂";
      grid[dome.ground][x] = { t: glyph, fg: SNOW };
    }
  }
  flakes(dome, t, lk.seed, s).forEach(([x, y], i) => {
    if (grid[y][x].t === " ") {
      const glyph = FLAKES[Math.floor(hash(lk.seed, x * 31 + i, y) * FLAKES.length)];
      grid[y][x] = { t: glyph, fg: SNOW, bold: true };
    }
  });
  if (lk.lights) {
    // A glint: it slides along the top of the glass, then waits.
    const at = ((t % 9) / 1.6) * w;
    for (let y = 0; y < Math.min(3, dome.ground); y++) {
      grid[y].forEach((cell, x) => {
        if (cell.t !== " " && cell.fg === GLASS && Math.abs(x - at) <= 1.5) grid[y][x] = { ...cell, fg: GLINT, bold: true };
      });
    }
  }
  return grid.map(runs);
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

/** The hover: a shake when the pointer comes over the globe, and one
 * more each `again` seconds it keeps moving there. Scene seconds. */
export class Shaker {
  constructor(again = 3) {
    this.again = again;
    this.at = null;
    this.count = 0;
    this.over = false;
  }

  move(now) {
    if (!this.over || this.at === null || now - this.at >= this.again) {
      this.at = now;
      this.count += 1;
    }
    this.over = true;
  }

  leave() {
    this.over = false;
  }
}

/** The options of `init`: `--all` spread, `--scene` named turns `-r`
 * off. */
export function options(args) {
  const all = args.all === true;
  const o = {};
  for (const k of ["shake", "drift", "lights", "random"]) o[k] = all || args[k] === true;
  const named = typeof args.scene === "string";
  o.random = o.random && !named;
  o.scene = named ? args.scene : "house";
  o.every = Number.isFinite(args.every) ? args.every : EVERY;
  return o;
}

let deck;
let lk;
let hand;
let brisk = false;

export function init(args) {
  const o = options(args);
  if (!(o.every > 0)) {
    log("globe: --every: more than 0");
    exit(2);
    return;
  }
  const b = random(4);
  const seed = 1 + (((b[0] | (b[1] << 8) | (b[2] << 16)) >>> 0) % 1000000);
  lk = look({ ...o, seed });
  hand = new Shaker();
  const render = (w, h) => scene(deck.now / 1000, w, h, lk, hand.at, hand.count);
  deck = new Deck({ border: "none" });
  // The next draw shows it: no draw of its own.
  deck.add("globe", SIZES, render, {
    hover: () => {
      hand.move(deck.now / 1000);
      return false;
    },
    leave: () => {
      hand.leave();
      return false;
    },
  });
  period(PERIOD);
}

export function tick(now) {
  deck.tick(now);
  const fast = swirls(now / 1000, lk, hand.at);
  if (fast !== brisk) period(fast ? BRISK : PERIOD);
  brisk = fast;
}

export function event(topic, data) {
  deck.event(topic, data);
}
