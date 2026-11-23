// SPDX-License-Identifier: GPL-3.0-or-later
// The module "ptyrc/canvas": Braille dots, half blocks, colours. Pure;
// `ptyrc.d.ts` is its contract, `_canvas.py` its twin.

import { rgb } from "ptyrc";

// ---- colours: the numbers of `rgb(r, g, b)` ----

const TAG = 0x03000000;
const CUBE = [0, 95, 135, 175, 215, 255];
const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

// A colour as 0xrrggbb.
function value(c, what) {
  if (typeof c === "number" && c >>> 24 === 3) return c & 0xffffff;
  if (typeof c === "string" && /^#[0-9a-fA-F]{6}$/.test(c)) return parseInt(c.slice(1), 16);
  throw new TypeError(`${what}: ${JSON.stringify(c)} is no colour: rgb(r, g, b) or "#rrggbb"`);
}

export function channels(c) {
  const v = value(c, "channels(c)");
  return [v >> 16, (v >> 8) & 255, v & 255];
}

export function hex(c) {
  return "#" + value(c, "hex(c)").toString(16).padStart(6, "0");
}

function mixed(a, b, t) {
  if (!(t > 0)) return a;
  if (t >= 1) return b;
  const ch = (s) => Math.round(((a >> s) & 255) + (((b >> s) & 255) - ((a >> s) & 255)) * t);
  return (ch(16) << 16) | (ch(8) << 8) | ch(0);
}

export function mix(a, b, t) {
  return TAG | mixed(value(a, "mix(a, b, t)"), value(b, "mix(a, b, t)"), t);
}

function paletteValue(n) {
  if (n >= 232) {
    const v = 8 + 10 * (n - 232);
    return (v << 16) | (v << 8) | v;
  }
  n -= 16;
  return (CUBE[Math.floor(n / 36)] << 16) | (CUBE[Math.floor(n / 6) % 6] << 8) | CUBE[n % 6];
}

export function palette(n) {
  if (!Number.isInteger(n) || n < 16 || n > 255) throw new RangeError(`palette(n): ${n}: 16 to 255`);
  return TAG | paletteValue(n);
}

// The index of the level of the cube nearest each of 0..255.
const LEVEL = Uint8Array.from({ length: 256 }, (_, v) => (v < 48 ? 0 : v < 116 ? 1 : Math.min(5, Math.floor((v - 76) / 40) + 1)));

// The weighed distance of (r, g, b) of chroma `c` to (x, y, z). Eyes
// weigh green most, blue least; a dark colour keeps its hue rather than
// turn grey: the chroma the palette colour lacks is counted.
function far(r, g, b, c, x, y, z) {
  const lost = c - (x > y ? (x > z ? x : z) : y > z ? y : z) + (x < y ? (x < z ? x : z) : y < z ? y : z);
  return 3 * (x - r) * (x - r) + 4 * (y - g) * (y - g) + 2 * (z - b) * (z - b) + (lost > 0 ? 2 * lost * lost : 0);
}

// The nearest of the 240 to 0xrrggbb by `far`, the lowest index of
// equals: as a loop over the 240, for each of the 2^24. The greys: the
// nearest of 8 + 10i to (3r + 4g + 2b) / 9, a tie to the lower. The
// cube: a bound (the nearest level each channel, or the grey), then in
// index order the levels whose partial sum is within it.
function closest(v) {
  const r = v >> 16;
  const g = (v >> 8) & 255;
  const b = v & 255;
  const c = (r > g ? (r > b ? r : b) : g > b ? g : b) - (r < g ? (r < b ? r : b) : g < b ? g : b);
  let t = Math.floor((6 * r + 8 * g + 4 * b - 55) / 180);
  t = t < 0 ? 0 : t > 23 ? 23 : t;
  const grey = far(r, g, b, c, 8 + 10 * t, 8 + 10 * t, 8 + 10 * t);
  let i = LEVEL[r];
  let j = LEVEL[g];
  let k = LEVEL[b];
  let bound = far(r, g, b, c, CUBE[i], CUBE[j], CUBE[k]);
  if (grey < bound) bound = grey;
  while (i > 0 && 3 * (CUBE[i - 1] - r) * (CUBE[i - 1] - r) <= bound) i--;
  while (j > 0 && 4 * (CUBE[j - 1] - g) * (CUBE[j - 1] - g) <= bound) j--;
  while (k > 0 && 2 * (CUBE[k - 1] - b) * (CUBE[k - 1] - b) <= bound) k--;
  let best = Infinity;
  let n = 0;
  for (; i < 6; i++) {
    const x = CUBE[i];
    const fr = 3 * (x - r) * (x - r);
    if (fr > bound) {
      if (x > r) break;
      continue;
    }
    for (let jj = j; jj < 6; jj++) {
      const y = CUBE[jj];
      const fg = fr + 4 * (y - g) * (y - g);
      if (fg > bound) {
        if (y > g) break;
        continue;
      }
      const top = x > y ? x : y;
      const low = x < y ? x : y;
      for (let kk = k; kk < 6; kk++) {
        const z = CUBE[kk];
        const fb = fg + 2 * (z - b) * (z - b);
        if (fb > bound) {
          if (z > b) break;
          continue;
        }
        const lost = c - (top > z ? top : z) + (low < z ? low : z);
        const f = lost > 0 ? fb + 2 * lost * lost : fb;
        if (f < best) {
          best = f;
          n = 16 + 36 * i + 6 * jj + kk;
          if (f < bound) bound = f;
        }
      }
    }
  }
  return grey < best ? 232 + t : n;
}

// `closest`, kept: 4096 colours, one a slot (a Map is slower).
const KEYS = new Int32Array(4096).fill(-1);
const KEPT = new Uint8Array(4096);

function nearest(v) {
  const h = Math.imul(v, 0x9e3779b1) >>> 20;
  if (KEYS[h] !== v) (KEYS[h] = v), (KEPT[h] = closest(v));
  return KEPT[h];
}

export function xterm(c) {
  return nearest(value(c, "xterm(c)"));
}

export function named(name) {
  const bright = typeof name === "string" && name.startsWith("bright-");
  const i = NAMES.indexOf(bright ? name.slice(7) : name);
  if (i < 0) throw new RangeError(`named(name): ${JSON.stringify(name)}: black, red, green, yellow, blue, magenta, cyan, white, or bright- each`);
  return bright ? i + 8 : i;
}

// ---- Braille ----

export const DOTS = [
  [0x01, 0x08],
  [0x02, 0x10],
  [0x04, 0x20],
  [0x40, 0x80],
];
// The bit of a dot by `(y & 3) * 2 + (x & 1)`.
const BIT = new Uint8Array([0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80]);
const GLYPHS = Array.from({ length: 256 }, (_, b) => (b ? String.fromCharCode(0x2800 + b) : " "));

function size(what, width, height) {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1)
    throw new RangeError(`new ${what}(width, height): two integers, 1 at least`);
}

export class Braille {
  constructor(width, height) {
    size("Braille", width, height);
    this.width = width;
    this.height = height;
    this.bits = new Uint8Array(width * height);
  }
  // Inside first: `| 0` is then the floor.
  set(x, y, on) {
    const w = this.width;
    if (!(x >= 0 && y >= 0 && x < 2 * w && y < 4 * this.height)) return;
    x |= 0;
    y |= 0;
    const i = (y >> 2) * w + (x >> 1);
    if (on === false) this.bits[i] &= ~BIT[((y & 3) << 1) | (x & 1)];
    else this.bits[i] |= BIT[((y & 3) << 1) | (x & 1)];
  }
  get(x, y) {
    const w = this.width;
    if (!(x >= 0 && y >= 0 && x < 2 * w && y < 4 * this.height)) return false;
    x |= 0;
    y |= 0;
    return (this.bits[(y >> 2) * w + (x >> 1)] & BIT[((y & 3) << 1) | (x & 1)]) !== 0;
  }
  line(x0, y0, x1, y1, on = true) {
    const n = Math.max(1, Math.round(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))));
    for (let i = 0; i <= n; i++) this.set(Math.round(x0 + ((x1 - x0) * i) / n), Math.round(y0 + ((y1 - y0) * i) / n), on);
  }
  clear() {
    this.bits.fill(0);
    return this;
  }
  glyph(cx, cy) {
    if (!(cx >= 0 && cy >= 0 && cx < this.width && cy < this.height)) return "";
    const b = this.bits[cy * this.width + cx];
    return b ? GLYPHS[b] : "";
  }
  cells() {
    const out = [];
    const { width, bits } = this;
    for (let i = 0; i < bits.length; i++) if (bits[i]) out.push([i % width, Math.floor(i / width)]);
    return out;
  }
  rows() {
    const { width, height, bits } = this;
    const out = new Array(height);
    for (let cy = 0; cy < height; cy++) {
      let s = "";
      for (let i = cy * width, end = i + width; i < end; i++) s += GLYPHS[bits[i]];
      out[cy] = s;
    }
    return out;
  }
  draw(canvas, x = 0, y = 0, style) {
    const rows = this.rows();
    for (let cy = 0; cy < rows.length; cy++) canvas.text(x, y + cy, rows[cy], style);
    return this;
  }
}

// ---- half blocks ----

export const NEAR = 24;

function alike(a, b) {
  return Math.abs((a >> 16) - (b >> 16)) + Math.abs(((a >> 8) & 255) - ((b >> 8) & 255)) + Math.abs((a & 255) - (b & 255)) <= NEAR;
}

function popcount(b) {
  let n = 0;
  for (; b; b &= b - 1) n++;
  return n;
}

function hexOf(v) {
  return "#" + v.toString(16).padStart(6, "0");
}

export class Half {
  #over = new Map(); // cy * width + cx -> [text, fg 0xrrggbb | -1, bg | -1]
  constructor(width, height, color = rgb(0, 0, 0)) {
    size("Half", width, height);
    this.width = width;
    this.height = height;
    this.px = new Int32Array(width * 2 * height).fill(value(color, "new Half(w, h, color)"));
  }
  fill(color) {
    this.px.fill(value(color, "half.fill(color)"));
    return this;
  }
  get(x, y) {
    if (!(x >= 0 && y >= 0 && x < this.width && y < 2 * this.height)) return null;
    return TAG | this.px[Math.floor(y) * this.width + Math.floor(x)];
  }
  #paint(x, y, v, alpha) {
    if (!(x >= 0 && y >= 0 && x < this.width && y < 2 * this.height)) return;
    const i = y * this.width + x;
    this.px[i] = alpha >= 1 ? v : mixed(this.px[i], v, alpha);
  }
  set(x, y, color, alpha = 1) {
    this.#paint(Math.floor(x), Math.floor(y), value(color, "half.set"), alpha);
  }
  rect(x, y, w, h, color, alpha = 1) {
    const v = value(color, "half.rect");
    for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) this.#paint(i, j, v, alpha);
  }
  disc(cx, cy, r, color, alpha = 1) {
    const v = value(color, "half.disc");
    for (let y = Math.trunc(cy - r - 1); y < Math.trunc(cy + r + 2); y++)
      for (let x = Math.trunc(cx - r - 1); x < Math.trunc(cx + r + 2); x++)
        if ((x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r) this.#paint(x, y, v, alpha);
  }
  put(cx, cy, text, fg = null, bg = null) {
    const f = fg == null ? -1 : value(fg, "half.put fg");
    const b = bg == null ? -1 : value(bg, "half.put bg");
    let x = cx;
    for (const ch of String(text)) {
      if (x >= 0 && x < this.width && cy >= 0 && cy < this.height) this.#over.set(cy * this.width + x, [ch, f, b]);
      x++;
    }
  }
  ink(braille, fg = null, tint = 1, rough = 0) {
    const f = fg == null ? -1 : value(fg, "half.ink fg");
    const { width, px } = this;
    const over = this.#over;
    for (const [cx, cy] of braille.cells()) {
      if (cx >= width || cy >= this.height || over.has(cy * width + cx)) continue;
      const ti = 2 * cy * width + cx;
      const li = ti + width;
      const top = px[ti];
      const low = px[li];
      const bits = braille.bits[cy * braille.width + cx];
      if (!alike(top, low)) {
        if (rough && f >= 0) {
          if (bits & 0x1b) px[ti] = mixed(top, f, rough);
          if (bits & 0xe4) px[li] = mixed(low, f, rough);
        }
        continue;
      }
      const bg = popcount(bits & 0x1b) > popcount(bits & 0xe4) ? top : low;
      const ink = f < 0 || tint >= 1 ? f : mixed(bg, f, tint);
      over.set(cy * width + cx, [GLYPHS[bits], ink, bg]);
    }
  }
  rows(deep = false) {
    const name = deep ? hexOf : nearest;
    const { width, px } = this;
    const over = this.#over;
    const out = [];
    for (let cy = 0; cy < this.height; cy++) {
      const row = [];
      let last = null;
      for (let cx = 0; cx < width; cx++) {
        const tv = px[2 * cy * width + cx];
        const lv = px[(2 * cy + 1) * width + cx];
        const a = name(tv);
        const b = name(lv);
        let glyph = over.size ? over.get(cy * width + cx) : undefined;
        // Ink on two pixels alike that the 256 tell apart: the glyph
        // would take them for one, and what is behind would move with it.
        if (glyph && !deep && glyph[2] >= 0 && glyph[0] > "⠀" && glyph[0] < "⤀" && a !== b) glyph = undefined;
        let run;
        if (glyph) {
          run = { t: glyph[0], bg: name(glyph[2] >= 0 ? glyph[2] : mixed(tv, lv, 0.5)) };
          if (glyph[1] >= 0) run.fg = name(glyph[1]);
        } else if (a === b) run = { t: " ", bg: b };
        else run = { t: "▀", fg: a, bg: b };
        if (last && last.fg === run.fg && last.bg === run.bg) last.t += run.t;
        else if (last && run.t === " " && last.bg === run.bg) last.t += " "; // a blank shows no foreground
        else row.push((last = run));
      }
      out.push(row);
    }
    return out;
  }
  draw(canvas, x = 0, y = 0, deep = false) {
    const rows = this.rows(deep);
    for (let cy = 0; cy < rows.length; cy++) {
      let at = x;
      for (const run of rows[cy]) {
        const { t, ...style } = run;
        at = canvas.text(at, y + cy, t, style);
      }
    }
    return this;
  }
}
