// SPDX-License-Identifier: GPL-3.0-or-later
// Landscapes from a seed, in layers, on a `Half` (the twin's `_strip`):
// `train`, `window`. Needs `sky.js` beside it.
//
// Pure: a seed, a place along the strip and a `Sky` give the pixels.
//
// - `Strip(seed, kind, scale, period)`: an endless profile; `height(x)`
//   in 0..1, `x` in pixels of its layer. Kinds: `KINDS`.
// - `Layer`: a strip at a `depth`, 0 nearest to 1 farthest: its ground
//   line `base`, relief `amp`, colour (a number, or one a season), the
//   things on it (`things`, one a `gap`, `density`; none within the
//   spans of `bare`). It scrolls at `rate()`: `(1 - depth)²` unless
//   `speed`. The ground: one colour, or `fields`, `strata`, `streak`.
// - `heavens(half, sky, horizon)`, `clouds`, then `paint` far to near,
//   then `stars`: only in the cells still as `heavens` left them.
//   `paint` returns `Mark`s (`window`, `chimney`) for `lights`.
// - `water`, `fog`, `rain`, `snow`: over what is painted.
// - `Pointer`: the host pointer over a scene, for the hovers.
// - `Half`: the twin's `_canvas.Half`: `px`, rows of pixels
//   (`Int32Array`, 0xrrggbb), and `over`, the glyphs over them, a `Map`
//   of `cy * w + cx` to `[text, fg, bg]` (-1: none); `rows(deep)`.
//
// Colours are given for full daylight; `Sky.shade` lights and hazes
// them. At night what is pale keeps a glow (`NIGHT`).

import { Braille, xterm } from "ptyrc/canvas";
import { mix, mod, rgb, rnd } from "./sky.js";

export const KINDS = ["hills", "mountains", "plain", "dunes", "city", "mesas", "flat"];
const ALT_TOP = 76.0;

export const WHITE = 0xffffff;
export const SNOW = rgb(232, 236, 242);
export const TRUNK = rgb(125, 85, 30);
export const LAMP = rgb(255, 214, 120);
export const SEA = rgb(0, 92, 140);
export const DROP = rgb(150, 172, 208);
export const MIST = rgb(206, 210, 214);
export const NIGHT = rgb(98, 104, 146);
/** The ground of a season, in daylight. */
export const GROUND = { spring: rgb(135, 190, 70), summer: rgb(90, 160, 40), autumn: rgb(150, 156, 60), winter: rgb(226, 231, 238) };
const WALLS = [rgb(234, 224, 200), rgb(216, 196, 168), rgb(204, 150, 128)];
const ROOFS = [rgb(170, 72, 54), rgb(124, 84, 70), rgb(96, 96, 110)];
export const NEAR = 24;

// ---- draws ----

function mix32(n) {
  n >>>= 0;
  n = (n ^ (n >>> 16)) >>> 0;
  n = Math.imul(n, 0x7feb352d) >>> 0;
  n = (n ^ (n >>> 15)) >>> 0;
  n = Math.imul(n, 0x846ca68b) >>> 0;
  return (n ^ (n >>> 16)) >>> 0;
}

/** A fixed draw in [0, 1) for the integers `(seed, a, b)`. */
export function noise(seed, a = 0, b = 0) {
  return mix32(mix32(mix32(seed) ^ Math.imul(a, 0x9e3779b1)) ^ Math.imul(b, 0x85ebca6b)) / 4294967296;
}

/** Smooth noise in 0..1: a draw an integer of `x`, eased between. */
export function wave(seed, x, period = null) {
  const i = Math.floor(x);
  let t = x - i;
  t = t * t * (3 - 2 * t);
  const a = period === null ? i : mod(i, period);
  const b = period === null ? i + 1 : mod(i + 1, period);
  return noise(seed, a) * (1 - t) + noise(seed, b) * t;
}

const times = (p, k) => (p === null ? null : p * k);

/** An endless profile from a seed. `scale`: pixels a feature;
 * `period`: in features, the profile repeats. */
export class Strip {
  constructor(seed, kind = "hills", scale = 16.0, period = null) {
    if (!KINDS.includes(kind)) throw new RangeError(`unknown strip: ${kind}`);
    this.seed = seed;
    this.kind = kind;
    this.scale = Math.max(1.0, scale);
    this.period = period;
  }

  /** `city`, `mesas`: `[n, left, wide, tall]` of the block at `x`. */
  block(x) {
    const cell = Math.max(3, rnd(this.scale));
    const i = Math.floor(x / cell);
    const n = this.period === null ? i : mod(i, this.period);
    const wide = Math.max(2, cell - Math.trunc(noise(this.seed, n, 1) * 3));
    let tall = noise(this.seed, n, 2);
    tall = tall < 0.12 ? 0.0 : 0.25 + 0.75 * tall ** 1.7;
    return [n, i * cell, wide, tall];
  }

  height(x) {
    const s = this.seed;
    const p = this.period;
    const u = x / this.scale;
    switch (this.kind) {
      case "flat":
        return 0.0;
      case "city":
      case "mesas": {
        const [, left, wide, tall] = this.block(x);
        return x - left < wide ? tall : 0.0;
      }
      case "mountains": {
        const ridge = 1 - Math.abs(2 * wave(s, u, p) - 1);
        const fine = 1 - Math.abs(2 * wave(s + 1, u * 3, times(p, 3)) - 1);
        return Math.min(1.0, 0.78 * ridge ** 1.35 + 0.22 * fine * ridge);
      }
      case "dunes":
        return wave(s, u, p) ** 2 * 0.8 + 0.1 * wave(s + 1, u * 2, times(p, 2));
    }
    const base = 0.68 * wave(s, u, p) + 0.32 * wave(s + 1, u * 2, times(p, 2));
    return base * (this.kind === "plain" ? 0.3 : 1.0);
  }
}

/** A patchwork over a ground: `crops`, the colours a season; a field
 * `wide` pixels; bands `deep` rows, then a row more each; a border
 * leans `lean` a row; `hedge`: the share of fields with one. */
export class Fields {
  constructor(crops, wide = 16.0, deep = 2, lean = 0.8, hedge = 0.0) {
    Object.assign(this, { crops, wide, deep, lean, hedge });
  }

  /** `[band, n, border]` of the field `d` rows under the line at `x`. */
  at(seed, x, d) {
    let band = 0;
    let tall = this.deep;
    let low = this.deep;
    while (d >= low) {
      band += 1;
      tall += 1;
      low += tall;
    }
    const u = (x + this.lean * d) / this.wide + noise(seed + 41, band) * 9;
    const n = Math.floor(u);
    return [band, n, (u - n) * this.wide < 1.0];
  }

  get key() {
    return JSON.stringify([this.crops, this.wide, this.deep, this.lean, this.hedge]);
  }
}

export class Layer {
  constructor(strip, depth, base, amp, o = {}) {
    this.strip = strip;
    this.depth = depth;
    this.base = base;
    this.amp = amp;
    this.colour = o.colour ?? { ...GROUND };
    this.things = o.things ?? [];
    this.gap = o.gap ?? 8;
    this.density = o.density ?? 0.6;
    this.size = o.size ?? 5.0;
    this.snow = o.snow ?? null;
    this.speed = o.speed ?? null;
    this.bare = o.bare ?? [];
    this.fields = o.fields ?? null;
    this.strata = o.strata ?? [];
    this.streak = o.streak ?? 0.0;
  }

  rate() {
    return this.speed === null ? (1 - this.depth) ** 2 : this.speed;
  }

  /** The pixel row of the ground at `x` of the layer. */
  top(x) {
    return this.base - this.amp * this.strip.height(x);
  }

  ground(season) {
    const c = this.colour;
    return typeof c === "number" ? c : c[season];
  }

  /** What its ground is a function of. */
  get key() {
    const s = this.strip;
    return JSON.stringify([s.seed, s.kind, s.scale, s.period, this.depth, this.base, this.amp, this.snow, this.streak, this.colour, this.strata, this.fields && this.fields.key]);
  }

  /** The things between `x0` and `x1`, left to right:
   * `{x, kind, size, tint, n}`. */
  standing(x0, x1) {
    const out = [];
    if (!this.things.length) return out;
    const seed = this.strip.seed + 77;
    const period = this.strip.period;
    const end = Math.floor(x1 / this.gap) + 2;
    for (let i = Math.floor(x0 / this.gap) - 1; i < end; i++) {
      const n = period === null ? i : mod(i, Math.max(1, rnd(period * this.strip.scale)));
      if (noise(seed, n, 0) >= this.density) continue;
      const kind = this.things[Math.trunc(noise(seed, n, 1) * this.things.length)];
      const x = i * this.gap + noise(seed, n, 2) * Math.max(1, this.gap - 2);
      if (this.bare.some(([a, b]) => a <= x && x < b)) continue;
      out.push({ x, kind, size: noise(seed, n, 3), tint: noise(seed, n, 4), n });
    }
    return out;
  }
}

/** The host pointer over a scene, in cells of its inner box. `move`,
 * `leave`: the handlers of a hover. `at`: `[x, y]` or null; `since`:
 * when it came in, or went; `trail`: `[x, y, time]` of the last `keep`
 * seconds. `now`: the clock, in seconds. */
export class Pointer {
  constructor(now, keep = 6.0) {
    this.now = now;
    this.keep = keep;
    this.at = null;
    this.since = -Infinity;
    this.trail = [];
  }

  move(x, y) {
    const t = this.now();
    if (this.at === null) this.since = t;
    this.at = [x, y];
    this.trail = this.trail.filter((p) => t - p[2] < this.keep);
    const last = this.trail[this.trail.length - 1];
    if (!last || last[0] !== x || last[1] !== y) this.trail.push([x, y, t]);
    else this.trail[this.trail.length - 1] = [x, y, t];
  }

  leave() {
    if (this.at !== null) {
      this.at = null;
      this.since = this.now();
    }
  }

  /** 0 away to 1 here, eased: `come` seconds in, `go` out. */
  ease(t, come = 0.6, go = 1.2) {
    if (this.since === -Infinity) return 0.0;
    let x = (t - this.since) / (this.at !== null ? come : go);
    x = Math.min(1.0, Math.max(0.0, x));
    x = x * x * (3 - 2 * x);
    return this.at !== null ? x : 1 - x;
  }
}

// ---- the canvas ----

function popcount(b) {
  let n = 0;
  for (; b; b &= b - 1) n++;
  return n;
}

export function alike(a, b) {
  return Math.abs((a >> 16) - (b >> 16)) + Math.abs(((a >> 8) & 255) - ((b >> 8) & 255)) + Math.abs((a & 255) - (b & 255)) <= NEAR;
}

const HEX = new Map();
export function hexa(c) {
  let s = HEX.get(c);
  if (s === undefined) {
    s = "#" + c.toString(16).padStart(6, "0");
    if (HEX.size >= 4096) HEX.clear();
    HEX.set(c, s);
  }
  return s;
}

const TAG = 0x03000000;
/** `xterm(c)` of `c` (0xrrggbb). */
export const index = (c) => xterm(TAG | c);

/** `w` by `2h` pixels in `w` by `h` cells, glyphs over them. */
export class Half {
  constructor(w, h, colour = 0) {
    this.w = w;
    this.h = h;
    this.px = Array.from({ length: 2 * h }, () => new Int32Array(w).fill(colour));
    this.over = new Map();
  }

  fill(colour) {
    for (const row of this.px) row.fill(colour);
  }

  get(x, y) {
    return x >= 0 && x < this.w && y >= 0 && y < 2 * this.h ? this.px[y][x] : null;
  }

  set(x, y, colour, alpha = 1.0) {
    if (x >= 0 && x < this.w && y >= 0 && y < 2 * this.h) {
      const row = this.px[y];
      row[x] = alpha >= 1.0 ? colour : mix(row[x], colour, alpha);
    }
  }

  rect(x, y, w, h, colour, alpha = 1.0) {
    for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) this.set(i, j, colour, alpha);
  }

  disc(cx, cy, r, colour, alpha = 1.0) {
    const y1 = Math.trunc(cy + r + 2);
    const x1 = Math.trunc(cx + r + 2);
    for (let y = Math.trunc(cy - r - 1); y < y1; y++)
      for (let x = Math.trunc(cx - r - 1); x < x1; x++) if ((x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r) this.set(x, y, colour, alpha);
  }

  /** `text` from the cell `(cx, cy)`, a cell a character. */
  put(cx, cy, text, fg = -1, bg = -1) {
    let x = cx;
    for (const ch of text) {
      if (x >= 0 && x < this.w && cy >= 0 && cy < this.h) this.over.set(cy * this.w + x, [ch, fg, bg]);
      x++;
    }
  }

  /** The cells of `dots` with a dot on, where no glyph is yet and the
   * two pixels are alike: the background the pixel under most dots,
   * the ink `fg` mixed over it by `tint`. Where they differ, `fg` is
   * mixed by `rough` into each pixel with a dot over it. */
  ink(dots, fg = -1, tint = 1.0, rough = 0.0) {
    const { w, px, over } = this;
    for (const [cx, cy] of dots.cells()) {
      if (cx >= w || cy >= this.h || over.has(cy * w + cx)) continue;
      const top = px[2 * cy][cx];
      const low = px[2 * cy + 1][cx];
      const bits = dots.bits[cy * dots.width + cx];
      if (!alike(top, low)) {
        if (rough && fg >= 0) {
          if (bits & 0x1b) px[2 * cy][cx] = mix(top, fg, rough);
          if (bits & 0xe4) px[2 * cy + 1][cx] = mix(low, fg, rough);
        }
        continue;
      }
      const bg = popcount(bits & 0x1b) > popcount(bits & 0xe4) ? top : low;
      const ink = fg < 0 || tint >= 1.0 ? fg : mix(bg, fg, tint);
      over.set(cy * w + cx, [String.fromCharCode(0x2800 + bits), ink, bg]);
    }
  }

  /** Rows of runs, `{t, fg, bg}`: colours of the 256, or `#rrggbb`
   * when `deep`; cells alike are one run. */
  rows(deep = false) {
    const name = deep ? hexa : index;
    const { w, over } = this;
    const out = [];
    for (let cy = 0; cy < this.h; cy++) {
      const top = this.px[2 * cy];
      const low = this.px[2 * cy + 1];
      const row = [];
      let last = null;
      for (let cx = 0; cx < w; cx++) {
        const a = name(top[cx]);
        const b = name(low[cx]);
        let glyph = over.size ? over.get(cy * w + cx) : undefined;
        if (glyph && !deep && glyph[2] >= 0 && glyph[0] > "⠀" && glyph[0] < "⤀" && a !== b) glyph = undefined;
        let run;
        if (glyph) {
          run = { t: glyph[0], bg: name(glyph[2] >= 0 ? glyph[2] : mix(top[cx], low[cx], 0.5)) };
          if (glyph[1] >= 0) run.fg = name(glyph[1]);
        } else if (a === b) run = { t: " ", bg: b };
        else run = { t: "▀", fg: a, bg: b };
        if (last && last.fg === run.fg && last.bg === run.bg) last.t += run.t;
        else if (last && run.t === " " && last.bg === run.bg) last.t += " ";
        else row.push((last = run));
      }
      out.push(row);
    }
    return out;
  }
}

/** A bounded memo: the oldest goes. */
export class Memo {
  constructor(size) {
    this.size = size;
    this.map = new Map();
  }
  get(key, make) {
    let v = this.map.get(key);
    if (v === undefined) {
      v = make();
      this.map.set(key, v);
      if (this.map.size > this.size) this.map.delete(this.map.keys().next().value);
    }
    return v;
  }
}

const copy = (rows) => rows.map((r) => r.slice());

// ---- the sky ----

const HEAVENS = new Memo(16);

/** Paint the sky down to the pixel row `horizon` (and under it, as at
 * it), the sun and the moon. Returns the pixels as painted. */
export function heavens(half, sky, horizon) {
  const rows = HEAVENS.get(`${half.w},${half.h},${sky.key},${horizon}`, () => paintHeavens(half.w, half.h, sky, horizon));
  half.px = copy(rows);
  return copy(rows);
}

function paintHeavens(w, h, sky, horizon) {
  const half = new Half(w, h);
  horizon = Math.max(1, horizon);
  for (let y = 0; y < 2 * h; y++) {
    const row = half.px[y];
    for (let x = 0; x < w; x++) row[x] = sky.colour((x + 0.5) / w, Math.min(1.0, (y + 0.5) / horizon));
  }
  const r = Math.max(1.0, Math.min(w / 20, horizon / 7));
  if (sky.sun > -6) {
    const cx = Math.floor(sky.sunX * w) + (rnd(2 * r) % 2 ? 0.5 : 0.0);
    const cy = Math.floor(horizon * (1 - sky.sun / ALT_TOP)) + mod(cx, 1);
    const low = 1 - Math.min(1.0, Math.max(0.0, sky.sun / 20));
    const disc = mix(rgb(255, 246, 200), rgb(255, 150, 70), low);
    const seen = 1 - 0.8 * sky.cover;
    half.disc(cx, cy, r * 2.2, disc, 0.3 * seen * low);
    half.disc(cx, cy, r, disc, seen);
  }
  if (sky.moon > -4 && sky.lit > 0.03) {
    const cx = Math.floor(sky.moonX * w) + 0.5;
    const cy = Math.floor(horizon * (1 - sky.moon / ALT_TOP)) + 0.5;
    moonDisc(half, sky, cx, cy, Math.max(1.5, Math.floor(r * 1.25) + 0.5));
  }
  return half.px;
}

function moonDisc(half, sky, cx, cy, r) {
  const seen = (0.45 + 0.55 * (1 - sky.light)) * (1 - 0.85 * sky.cover);
  const bright = mix(rgb(244, 242, 220), WHITE, sky.light);
  const k = Math.cos(2 * Math.PI * sky.phase);
  const waxing = sky.phase < 0.5;
  for (let y = Math.trunc(cy - r - 1); y < Math.trunc(cy + r + 2); y++) {
    for (let x = Math.trunc(cx - r - 1); x < Math.trunc(cx + r + 2); x++) {
      const dx = x + 0.5 - cx;
      const dy = y + 0.5 - cy;
      if (dx * dx + dy * dy > r * r) continue;
      const edge = Math.sqrt(Math.max(0.0, r * r - dy * dy)) * k;
      if ((waxing ? dx : -dx) >= edge - 0.25) half.set(x, y, bright, seen);
    }
  }
}

/** Clouds, `amount` 0..1; `drift`: pixels gone by, to the right;
 * `tall`: pixels a row of a cloud (2: whole cells). */
export function clouds(half, sky, seed, horizon, amount = 0.4, drift = 0.0, tall = 1) {
  const w = half.w;
  const count = Math.max(1, rnd(amount * (2 + w / 7)));
  const lap = w + 24;
  const tone = sky.shade(mix(WHITE, rgb(150, 154, 166), 0.75 * sky.cover));
  for (let i = 0; i < count; i++) {
    const far = noise(seed, i, 2);
    const long = Math.max(4, rnd((w / 5) * (0.6 + 0.8 * noise(seed, i, 1)) * (1 - 0.4 * far)));
    const left = Math.trunc(mod(noise(seed, i, 0) * lap + drift * (1.2 - 0.7 * far), lap)) - 12;
    const base = Math.floor(rnd(horizon * (0.14 + 0.5 * far)) / tall) * tall;
    const body = mix(tone, sky.colour(0.5, 0.9), 0.3 * far);
    const under = mix(body, rgb(70, 76, 96), 0.2 + 0.3 * sky.cover);
    let tiers = long < 6 ? 1 : long < 11 ? 2 : 3;
    tiers = Math.max(1, rnd(tiers / tall + 0.4));
    for (let tier = 0; tier < tiers; tier++) {
      const a = rnd(tier * long * (0.1 + 0.2 * noise(seed, i, 5 + tier)));
      const b = rnd(tier * long * (0.1 + 0.2 * noise(seed, i, 8 + tier)));
      const tint = tier === 0 && tiers > 1 ? under : body;
      half.rect(left + a, base - tier * tall, long - a - b, tall, tint, 0.94);
    }
  }
}

const SPARKS = "⠁⠂⠄⠈⠐⠠";
const TINTS = [rgb(200, 215, 255), rgb(255, 244, 214), WHITE];

/** Stars in the cells still as `clear` (the pixels of `heavens`);
 * with `t`, they twinkle. None by day. */
export function stars(half, clear, sky, seed, t = null, horizon = null) {
  const seen = sky.stars;
  if (seen <= 0.05) return;
  const rows = horizon === null ? half.h : Math.min(half.h, Math.floor(horizon / 2));
  const { px, over, w } = half;
  for (let cy = 0; cy < rows; cy++) {
    for (let cx = 0; cx < w; cx++) {
      if (noise(seed, cx * 7919 + 13, cy) >= 0.11 || over.has(cy * w + cx)) continue;
      if (px[2 * cy][cx] !== clear[2 * cy][cx] || px[2 * cy + 1][cx] !== clear[2 * cy + 1][cx]) continue;
      const rank = noise(seed, cx, cy * 104729 + 7);
      let bright = (0.45 + 0.55 * rank) * seen;
      if (t !== null) {
        const beat = Math.trunc(t / 0.7 + rank * 9);
        bright *= 0.55 + 0.45 * noise(seed, cx * 131 + cy, beat);
      }
      if (bright < 0.2) continue;
      let glyph = rank > 0.93 && bright > 0.7 ? "✦" : "·";
      if (rank < 0.3) glyph = SPARKS[Math.trunc(rank * 20) % 6];
      const tint = TINTS[Math.trunc(rank * 97) % 3];
      const under = mix(px[2 * cy][cx], px[2 * cy + 1][cx], 0.5);
      half.put(cx, cy, glyph, mix(under, tint, Math.min(1.0, bright)));
    }
  }
}

// ---- the land ----

/** The leaves of a tree; null: bare. */
export function crown(season, tint) {
  if (season === "winter") return null;
  if (season === "spring") {
    if (tint < 0.3) return mix(rgb(244, 176, 204), rgb(252, 234, 240), tint / 0.3);
    return mix(rgb(104, 170, 74), rgb(140, 190, 90), tint);
  }
  if (season === "autumn") {
    if (tint > 0.72) return rgb(224, 184, 62);
    return mix(rgb(214, 126, 44), rgb(172, 56, 44), tint / 0.72);
  }
  return mix(rgb(0, 125, 20), rgb(44, 142, 10), tint);
}

function tree(half, x, y, s, thing, lit) {
  const trunk = lit(TRUNK);
  const leaves = crown(lit.season, thing.tint);
  if (leaves === null) {
    for (let j = 1; j < s; j++) half.set(x, y - j, trunk);
    if (s >= 3) {
      half.set(x - 1, y - s + 1, trunk);
      half.set(x + 1, y - s, trunk);
      half.set(x - 1, y - s, lit(SNOW), 0.8);
    }
    return;
  }
  const stem = Math.max(1, Math.floor(s / 3));
  for (let j = 1; j <= stem; j++) half.set(x, y - j, trunk);
  const r = Math.max(0.8, (s - stem) / 2);
  const cx = x + 0.5;
  const cy = y - stem - r;
  half.disc(cx, cy, r, lit(leaves));
  if (r >= 1.4) {
    half.disc(cx + r * 0.35, cy + r * 0.4, r * 0.55, lit(mix(leaves, rgb(0, 30, 20), 0.3)));
    half.disc(cx - r * 0.3, cy - r * 0.35, r * 0.45, lit(mix(leaves, WHITE, 0.22)));
  }
}

function pine(half, x, y, s, thing, lit) {
  const base = mix(rgb(0, 84, 30), rgb(0, 112, 40), thing.tint);
  const greenC = lit(base);
  const dark = lit(mix(base, rgb(0, 24, 20), 0.35));
  const snow = lit(SNOW);
  half.set(x, y - 1, lit(TRUNK));
  if (s >= 9) half.set(x, y - 2, lit(TRUNK));
  const tier = s < 9 ? s : Math.max(3, Math.floor(s / 4));
  for (let j = 0; j < s - 1; j++) {
    const wide = s < 9 ? Math.floor((j + 1) / 2) : rnd(j * 0.3 + (j % tier) * 0.5);
    const white = lit.season === "winter" && (j < 1 || (j % 2 === 1 && thing.tint < 0.6));
    for (let dx = -wide; dx <= wide; dx++) {
      const edge = white && (j < 1 || Math.abs(dx) === wide);
      half.set(x + dx, y - s + j, edge ? snow : dx > 0 ? dark : greenC);
    }
  }
}

function poplar(half, x, y, s, thing, lit) {
  half.set(x, y - 1, lit(TRUNK));
  const leaves = crown(lit.season, 0.4 + 0.5 * thing.tint);
  if (leaves === null) {
    for (let j = 2; j <= s; j++) half.set(x, y - j, lit(TRUNK));
    return;
  }
  for (let j = 2; j < s + 2; j++) {
    half.set(x, y - j, lit(leaves));
    if (2 < j && j < s + 1 && s >= 4) half.set(x + 1, y - j, lit(mix(leaves, rgb(0, 30, 20), 0.3)));
  }
}

function house(half, x, y, s, thing, lit) {
  let wide = Math.max(3, s);
  const tall = Math.max(2, rnd(s * 0.5));
  let wall = WALLS[Math.trunc(thing.tint * 3) % 3];
  let roof = ROOFS[Math.trunc(thing.tint * 7) % 3];
  if (thing.kind === "barn") [wide, wall, roof] = [wide + 1, rgb(158, 60, 48), rgb(84, 70, 70)];
  else if (thing.kind === "chalet") [wall, roof] = [rgb(132, 92, 60), rgb(92, 70, 60)];
  wall = lit(wall);
  roof = lit(lit.season === "winter" ? SNOW : roof);
  if (thing.kind === "church") {
    const tx = thing.tint < 0.5 ? x - 2 : x + wide;
    half.rect(tx, y - tall - 3, 2, tall + 3, wall);
    half.rect(tx, y - tall - 4, 2, 1, roof);
    half.set(tx + (thing.tint < 0.5 ? 1 : 0), y - tall - 5, roof);
  }
  half.rect(x, y - tall, wide, tall, wall);
  const rows = Math.max(1, rnd(s * 0.34));
  for (let j = 0; j < rows; j++) {
    const inset = rows > 1 ? j : 0;
    half.rect(x - 1 + inset, y - tall - 1 - j, wide + 2 - 2 * inset, 1, roof);
  }
  const marks = [];
  if (thing.kind === "barn") {
    half.rect(x + Math.floor(wide / 2) - 1, y - Math.min(2, tall), 2, Math.min(2, tall), roof);
    return marks;
  }
  const wx = x + (thing.tint < 0.5 ? 1 : wide - 2);
  const wy = y - Math.max(1, tall - 1);
  const pane = lit(mix(rgb(70, 90, 120), wall, 0.2));
  half.set(wx, wy, pane);
  marks.push({ kind: "window", x: wx, y: wy, n: thing.n, pane });
  if (wide >= 4) {
    const px = x + (thing.tint < 0.5 ? wide - 2 : 1);
    half.set(px, y - 1, lit(mix(TRUNK, rgb(40, 30, 30), 0.4)));
    if (thing.kind === "house" || thing.kind === "chalet") {
      const top = y - tall - rows - 1;
      half.set(px, top, lit(mix(ROOFS[1], rgb(40, 40, 40), 0.3)));
      marks.push({ kind: "chimney", x: px, y: top, n: thing.n, pane: null });
    }
  }
  return marks;
}

function small(half, x, y, s, thing, lit) {
  switch (thing.kind) {
    case "sheep":
      if (lit.season === "spring" || lit.season === "summer") {
        half.set(x, y - 1, lit(rgb(244, 242, 234)));
        half.set(x + 1, y - 1, lit(rgb(244, 242, 234)));
        half.set(x + (thing.tint < 0.5 ? 2 : -1), y - 1, lit(rgb(60, 54, 52)));
      }
      break;
    case "bush": {
      const leaf = crown(lit.season, thing.tint) ?? mix(TRUNK, SNOW, 0.5);
      half.rect(x, y - 1, 2 + (thing.size > 0.5 ? 1 : 0), 1, lit(leaf));
      if (thing.size > 0.7) half.set(x + 1, y - 2, lit(leaf));
      break;
    }
    case "rock":
      half.rect(x, y - 1, 2, 1, lit(rgb(128, 126, 130)));
      if (thing.size > 0.5) half.set(x, y - 2, lit(rgb(150, 148, 150)));
      break;
    case "hay":
      if (lit.season === "summer" || lit.season === "autumn") {
        half.rect(x, y - 1, 3, 1, lit(rgb(218, 184, 84)));
        half.set(x + 1, y - 2, lit(rgb(232, 200, 100)));
      }
      break;
    case "cactus": {
      const leaf = lit(rgb(62, 122, 74));
      for (let j = 1; j <= s; j++) half.set(x, y - j, leaf);
      if (s >= 3) half.set(x + (thing.tint < 0.5 ? 1 : -1), y - s + 1, leaf);
      break;
    }
    case "palm": {
      for (let j = 1; j < s; j++) half.set(x + (j > Math.floor(s / 2) ? 1 : 0), y - j, lit(rgb(140, 110, 76)));
      const leaf = lit(rgb(0, 135, 60));
      const dark = lit(rgb(0, 104, 56));
      const cx = x + 1;
      half.rect(cx - 2, y - s, 5, 1, leaf);
      half.rect(cx - 1, y - s - 1, 3, 1, leaf);
      half.set(cx - 2, y - s + 1, dark);
      half.set(cx + 2, y - s + 1, dark);
      half.set(cx + 1, y - s, dark);
      break;
    }
    case "cow":
      if (lit.season !== "winter") {
        const coat = lit(thing.tint < 0.5 ? rgb(250, 246, 240) : rgb(124, 78, 50));
        half.rect(x, y - 2, 3, 1, coat);
        half.set(x, y - 1, coat);
        half.set(x + 2, y - 1, coat);
        half.set(x + (thing.size < 0.5 ? 3 : -1), y - 2, lit(rgb(60, 50, 46)));
      }
      break;
    case "pylon": {
      const steel = lit(rgb(150, 154, 164));
      for (let j = 1; j < s + 2; j++) half.set(x, y - j, steel);
      half.rect(x - 1, y - s, 3, 1, steel);
      if (s >= 4) half.rect(x - 1, y - s + 2, 3, 1, steel);
      break;
    }
    case "hut": {
      const coat = [rgb(70, 130, 190), rgb(220, 90, 80), rgb(240, 200, 90)][Math.trunc(thing.tint * 3) % 3];
      half.rect(x, y - 2, 2, 2, lit(coat));
      half.rect(x, y - 3, 2, 1, lit(rgb(244, 240, 230)));
      break;
    }
  }
}

function lighthouse(half, x, y, s, thing, lit) {
  const tall = s + 2;
  for (let j = 1; j <= tall; j++) {
    const band = lit(Math.floor(j / 2) % 2 ? rgb(206, 62, 54) : rgb(244, 240, 232));
    half.set(x, y - j, band);
    if (j <= tall - 2) half.set(x + 1, y - j, band);
  }
  const pane = lit(rgb(60, 70, 90));
  half.set(x, y - tall, pane);
  half.set(x, y - tall - 1, lit(rgb(70, 64, 70)));
  return [{ kind: "window", x, y: y - tall, n: thing.n, pane }];
}

const minOf = (c) => Math.min(c >> 16, (c >> 8) & 255, c & 255);

/** `Sky.shade` at one depth, what is pale kept at night; `.season`. */
export function lighting(sky, depth, season) {
  let memo = sky.lits?.get(depth);
  if (!sky.lits) sky.lits = new Map();
  if (!memo) sky.lits.set(depth, (memo = new Map()));
  const lit = (c) => {
    let out = memo.get(c);
    if (out === undefined) {
      const shaded = sky.shade(c, depth);
      const pale = (minOf(c) / 255) ** 2 * (1 - sky.light);
      out = pale ? mix(shaded, NIGHT, 0.75 * pale) : shaded;
      memo.set(c, out);
    }
    return out;
  };
  lit.sky = sky;
  lit.season = season;
  return lit;
}

function city(half, layer, sky, shift, lit) {
  const strip = layer.strip;
  const night = 1 - sky.light;
  const late = sky.hour < 5.5 || sky.hour >= 23.5 ? 0.35 : 1.0;
  for (let x = 0; x < half.w; x++) {
    const [n, left, wide, tall] = strip.block(x + shift);
    const dx = Math.floor(x + shift) - left;
    if (!tall || dx >= wide || (dx % 2 === 0 && wide > 2)) continue;
    const top = rnd(layer.base - layer.amp * tall);
    const end = rnd(layer.base);
    for (let y = top + 1; y < end; y += 2) {
      const draw = noise(strip.seed + 5, n * 64 + dx, y - top);
      if (night > 0.4) {
        if (draw < 0.42 * late) {
          const warm = mix(LAMP, rgb(255, 240, 200), noise(strip.seed, n, y));
          half.set(x, y, mix(warm, sky.colour(0.5, 1), 0.5 * layer.depth));
        }
      } else if (draw < 0.5) half.set(x, y, lit(mix(rgb(120, 150, 180), rgb(60, 70, 90), draw)));
    }
  }
}

function soil(layer, lit, season, x, y, d, ground) {
  const seed = layer.strip.seed;
  let c = ground;
  if (layer.strata.length) {
    const bed = Math.floor((rnd(layer.base) - y) / 2);
    c = lit(layer.strata[Math.trunc(noise(seed + 43, bed) * layer.strata.length)]);
  }
  if (layer.fields !== null) {
    const [band, n, border] = layer.fields.at(seed, x, d);
    const crops = layer.fields.crops[season];
    let raw = crops[Math.trunc(noise(seed + 42, n, band) * crops.length)];
    if (border && noise(seed + 44, n, band) < layer.fields.hedge) raw = season !== "winter" ? mix(raw, rgb(30, 60, 30), 0.45) : TRUNK;
    c = lit(mix(raw, layer.ground(season), 0.8 * (1 - lit.sky.light)));
  }
  if (layer.streak) {
    const run = Math.floor(x / 4 + noise(seed + 45, y) * 16);
    const draw = noise(seed + 46, run, y);
    if (draw < layer.streak) c = mix(c, draw < layer.streak / 2 ? 0 : WHITE, 0.14);
  }
  return c;
}

/** The columns of ground painted, by what they are a function of. */
export const GROUNDS = new Memo(24);
const COLUMNS = 4096;

function columnsOf(layer, sky, season, rows) {
  const key = `${layer.key}|${sky.key}|${season}|${rows}`;
  let columns = GROUNDS.map.get(key);
  if (columns !== undefined) GROUNDS.map.delete(key);
  if (columns === undefined || columns.size >= COLUMNS) columns = new Map();
  GROUNDS.map.set(key, columns);
  while (GROUNDS.map.size > GROUNDS.size) GROUNDS.map.delete(GROUNDS.map.keys().next().value);
  return columns;
}

/** Paint `layer`, its ground then what stands on it; `shift`: pixels
 * the nearest plane has scrolled, to the left. The marks. */
export function paint(half, layer, sky, shift = 0.0, season = null) {
  season = season || sky.season;
  const lit = lighting(sky, layer.depth, season);
  shift = Math.floor(shift * layer.rate() + 1e-6);
  const w = half.w;
  const rows = 2 * half.h;
  const raw = layer.ground(season);
  const ground = lit(raw);
  const rim = lit(mix(raw, WHITE, 0.16));
  const snow = lit(SNOW);
  const blocky = layer.strip.kind === "city";
  const plain = !(layer.fields || layer.strata.length || layer.streak);
  const columns = columnsOf(layer, sky, season, rows);
  const px = half.px;
  for (let x = 0; x < w; x++) {
    let column = columns.get(x + shift);
    if (column === undefined) {
      const high = layer.strip.height(x + shift);
      const top = rnd(layer.base - layer.amp * high);
      let white = 0;
      if (layer.snow !== null && high > layer.snow) {
        const jag = 0.5 + noise(layer.strip.seed, Math.floor(x + shift), 9);
        white = rnd((high - layer.snow) * layer.amp * jag) + 1;
      }
      const from = Math.max(0, top);
      const under = new Int32Array(Math.max(0, rows - from));
      for (let y = from; y < rows; y++) {
        let c;
        if (y - top < white) c = snow;
        else if (y === top && !blocky && high > 0) c = rim;
        else if (plain) c = ground;
        else c = soil(layer, lit, season, x + shift, y, y - top, ground);
        under[y - from] = c;
      }
      column = [from, under];
      columns.set(x + shift, column);
    }
    const [from, under] = column;
    for (let i = 0; i < under.length; i++) px[from + i][x] = under[i];
  }
  if (blocky) city(half, layer, sky, shift, lit);
  const marks = [];
  const reach = rnd(layer.size) + 3;
  for (const thing of layer.standing(shift - reach, shift + w + reach)) {
    const x = Math.floor(thing.x) - shift;
    let y = rnd(layer.top(thing.x));
    const s = Math.max(2, rnd(layer.size * (0.6 + 0.4 * thing.size)));
    switch (thing.kind) {
      case "tree":
        tree(half, x, y, s, thing, lit);
        break;
      case "pine":
        pine(half, x, y, s + 1, thing, lit);
        break;
      case "poplar":
        poplar(half, x, y, s + 1, thing, lit);
        break;
      case "house":
      case "barn":
      case "chalet":
      case "church":
        y = rnd(Math.max(layer.top(thing.x), layer.top(thing.x + s)));
        marks.push(...house(half, x, y, s, thing, lit));
        break;
      case "lighthouse":
        marks.push(...lighthouse(half, x, y, s, thing, lit));
        break;
      default:
        small(half, x, y, s, thing, lit);
    }
  }
  return marks;
}

// ---- water, weather, lamps ----

const MIRRORS = new Memo(16);

function mirror(w, rows, sky, hz) {
  return MIRRORS.get(`${w},${rows},${sky.key},${hz}`, () => {
    const deep = sky.shade(SEA);
    const out = [];
    for (let y = hz; y < rows; y++) {
      const v = (y - hz + 0.5) / Math.max(1, rows - hz);
      const murk = (0.3 + 0.15 * v) * (0.35 + 0.65 * sky.light);
      const row = new Int32Array(w);
      for (let x = 0; x < w; x++) row[x] = mix(sky.colour((x + 0.5) / w, 1 - 0.85 * v), deep, murk);
      out.push(row);
    }
    return out;
  });
}

/** A sea from the pixel row `hz` down: the sky mirrored, glints, the
 * sun's and the moon's path. */
export function water(half, sky, hz, seed, t) {
  const w = half.w;
  const rows = 2 * half.h;
  const still = mirror(w, rows, sky, hz);
  const beat = Math.trunc(t * 1.3);
  for (let y = hz; y < rows; y++) {
    const row = half.px[y];
    const from = still[y - hz];
    for (let x = 0; x < w; x++) {
      let c = from[x];
      if (sky.light > 0.3 && noise(seed + 9, x + beat + y * 3, y) < 0.04) c = mix(c, WHITE, 0.3 * sky.light);
      row[x] = c;
    }
  }
  for (const [body, acrossX, shine, moon] of [
    [sky.sun, sky.sunX, rgb(255, 226, 160), false],
    [sky.moon, sky.moonX, rgb(236, 236, 220), true],
  ]) {
    if (!(-2 < body && body < 50) || (moon && sky.moonlight < 0.2)) continue;
    for (let y = hz; y < rows; y++) {
      const reach = 0.5 + (y - hz) * 0.22;
      const end = Math.trunc(acrossX * w + reach + 2);
      for (let x = Math.trunc(acrossX * w - reach - 1); x < end; x++) {
        const near = Math.abs(x + 0.5 - acrossX * w) <= reach;
        if (near && noise(seed + 4, x * 31 + y, Math.trunc(t * 2.2)) < 0.45) half.set(x, y, shine, 0.7 * (1 - 0.7 * sky.cover));
      }
    }
  }
}

export function fog(half, sky, hz) {
  const grey = sky.shade(MIST);
  const rows = 2 * half.h;
  for (let y = 0; y < rows; y++) {
    const d = (y - hz) / rows;
    const thick = Math.max(0.25, 0.82 - (d > 0 ? 1.5 * d : -0.9 * d));
    for (let x = 0; x < half.w; x++) half.set(x, y, grey, thick);
  }
}

/** Drops over the scene; `lean`: dots aside a dot fallen; `rough`: how
 * much a drop tints a pixel where a cell has two colours. */
export function rain(half, sky, seed, t, lean = 0.0, rough = 0.0) {
  const dots = new Braille(half.w, half.h);
  const tall = 4 * half.h;
  const wide = 2 * half.w;
  for (let col = 0; col < wide; col++) {
    if (noise(seed + 21, col) > 0.42) continue;
    const fall = 9 + 6 * noise(seed + 22, col);
    const y = Math.trunc(t * fall + noise(seed + 23, col) * 97) % (tall + 5);
    for (let k = 0; k < (lean ? 4 : 2); k++) {
      const aside = lean ? Math.trunc((y - k) * lean) : mod(Math.floor((y - k) / 5), 2);
      dots.set(mod(col - aside, wide), y - k);
    }
  }
  half.ink(dots, sky.shade(DROP), 0.75, rough);
}

/** Flakes over the scene; `rough`: as for `rain`. */
export function snow(half, sky, seed, t, rough = 0.0) {
  const dots = new Braille(half.w, half.h);
  const wide = 2 * half.w;
  const tall = 4 * half.h;
  const n = Math.max(3, Math.floor((half.w * half.h) / 7));
  for (let i = 0; i < n; i++) {
    const fall = 1.6 + 1.8 * noise(seed + 31, i);
    const y = Math.trunc(t * fall + noise(seed + 32, i) * tall) % tall;
    const sway = Math.sin(t * 0.7 + i) * 1.6;
    dots.set(mod(Math.trunc(noise(seed + 33, i) * wide + sway + t * 0.4), wide), y);
  }
  half.ink(dots, mix(WHITE, sky.shade(WHITE), 0.5), 1.0, rough);
}

/** The windows of the houses: lit at night, fewer late; lit under the
 * pointer at any time. Not those painted over since. */
export function lights(half, sky, marks, seed, pointer) {
  const late = sky.hour >= 23.25 || sky.hour < 5.5;
  for (const mark of marks) {
    if (mark.kind !== "window") continue;
    if (mark.pane !== null && half.get(mark.x, mark.y) !== mark.pane) continue;
    const home = noise(seed + 95, mark.n);
    let on = sky.light < 0.4 && home < (late ? 0.25 : 0.85);
    if (pointer && pointer.at !== null) {
      const [px, py] = pointer.at;
      on = on || (Math.abs(px - mark.x) <= 3 && Math.abs(py - Math.floor(mark.y / 2)) <= 2);
    }
    if (on) half.set(mark.x, mark.y, LAMP);
  }
}
