// SPDX-License-Identifier: GPL-3.0-or-later
// Window: a house window on a landscape, by day and by night.
//
// One widget, a variant a size, from a porthole of 7x3 to a bay of
// 40x12: the same landscape, cut wider; the border is the frame of the
// window, a sill under it; the mullions are content.
// `scene(w, h, when, t, look, pointer, cat)` is pure but for `cat`:
// `when` the local naive time (the light of `sky.js`, the season), `t`
// the seconds the app has run (what moves). Pixels of half a cell
// (`Half` of `strip.js`); the land from a seed, one a day.
//
// Bare: the sky, the sun, the moon in its phase, stars, a hill and a
// meadow. `-l`: ridges, trees, houses lit at night; `-y`: the season of
// the date; `-w`: the weather of the day; `-p`: birds, smoke, a
// balloon, a meteor, a plane, the stars twinkle; `-c`: curtains and a
// cat on the sill (`mouser.js`); `-a`: all, and the place of the day.
// Indoors, the mullions, the curtains and the cat: colours that stand
// out from the view behind them and from each other (`room`), lit by a
// lamp at night. The pointer over the window: the curtains move aside,
// a house near it lights up, the cat hunts it along the sill. `-r`:
// what is not given, drawn (`drawn` of `sky.js`) from the launcher's
// seed.

import { exit, log, now, period, random } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Braille, palette } from "ptyrc/canvas";
import { Era, MIDDLE, at, clockOf, drawn, minute, mix, naive, ordinal, rgb, rnd, weather as weatherOf } from "./sky.js";
import { GROUND, Half, LAMP, Layer, Memo, Pointer, Strip, WHITE, clouds, fog, heavens, index, lights, noise, paint, rain, snow, stars, water } from "./strip.js";
import { Cat, SIT, Sill, cells, floorDiv } from "./mouser.js";
import { Twister } from "./twister.js";

/** The inner sizes of the variants. */
export const SIZES = [
  [7, 3],
  [12, 4],
  [18, 6],
  [26, 8],
  [40, 12],
];
export const PLACES = ["countryside", "sea", "mountain", "city"];
/** The frame: thin jambs, a sill. */
export const BORDER = { corners: "┌┐╘╛", top: "─", bottom: "═", left: "│", right: "│", style: { fg: 180 } };

export const FRAME = rgb(196, 156, 116);
/** The paints of the mullions: `FRAME` unless it melts into the view. */
export const FRAMES = [FRAME, rgb(238, 236, 228), rgb(84, 54, 38), rgb(46, 88, 62), rgb(54, 76, 116), rgb(132, 50, 44)];
export const FABRICS = [
  rgb(178, 62, 66),
  rgb(222, 208, 180),
  rgb(70, 110, 140),
  rgb(214, 162, 48),
  rgb(58, 116, 78),
  rgb(118, 58, 108),
  rgb(190, 94, 58),
  rgb(40, 50, 96),
  rgb(36, 126, 128),
  rgb(240, 236, 228),
];
export const COATS = [rgb(26, 24, 28), rgb(206, 126, 56), rgb(120, 118, 122), rgb(238, 234, 226), rgb(218, 196, 160), rgb(104, 72, 48), rgb(76, 78, 92)];
/** The lamp of the room: what lights the inside at night; the edge of
 * the cat towards it. */
const ROOM = [255, 228, 192];
const RIM = rgb(250, 230, 200);
/** The least contrast of a thing indoors with what is behind it, and
 * with the other things indoors (`apart`). */
export const FLOOR = 36.0;
const SLATE = rgb(128, 130, 140);

export const PERIOD = 0.25; // seconds a frame, when something moves
export const BRISK = 1 / 12; // when the cat is up to something
export const STILL = 20.0; // seconds a frame, else

/** What a window shows, the time apart. `weather`: null (clear, no
 * cloud), `"day"` (the day's), or one of `WEATHERS`. */
export function look(o = {}) {
  const out = { seed: 0, place: "countryside", lat: 47.0, land: false, seasons: false, weather: null, passing: false, cosy: false, deep: false, ...o };
  out.key = JSON.stringify([out.seed, out.place, out.lat, out.land, out.seasons, out.weather, out.passing, out.cosy]);
  return out;
}
export const moving = (lk) => lk.passing || lk.cosy || lk.weather !== null;

/** Python's `sum` of floats: compensated (Neumaier), as CPython 3.12+. */
function fsum(xs) {
  let f = 0.0;
  let c = 0.0;
  for (const x of xs) {
    const t = f + x;
    c += Math.abs(f) >= Math.abs(x) ? f - t + x : x - t + f;
    f = t;
  }
  return c && Number.isFinite(c) ? f + c : f;
}

// ---- the land of each place ----

const tall = (rows, share, lo, hi) => Math.min(hi, Math.max(lo, rnd(rows * share)));

function countryside(lk, w, rows) {
  const s = lk.seed;
  const hz = rnd(rows * 0.6);
  const far = new Layer(new Strip(s + 1, "hills", w / 4 + 4), 0.5, hz + rows * 0.12, rows * 0.34);
  const near = new Layer(new Strip(s + 2, "hills", w / 2.5 + 5), 0.0, rows * 0.96, rows * 0.26);
  if (!lk.land || rows < 8) return [hz, rows > 6 ? [far, near] : [far]];
  const ridge = new Layer(new Strip(s, "mountains", w / 2 + 5), 0.82, hz + rows * 0.02, rows * 0.24);
  far.things = ["pine", "tree", "tree", "bush"];
  far.gap = 4;
  far.density = 0.55;
  far.size = tall(rows, 0.14, 2, 4);
  near.things = ["tree", "tree", "house", "tree", "pine", "sheep", "bush", "hay"];
  near.size = tall(rows, 0.32, 3, 9);
  near.gap = rnd(near.size) + 1;
  near.density = 0.8;
  return [hz, [ridge, far, near]];
}

function sea(lk, w, rows) {
  const s = lk.seed;
  const hz = rnd(rows * 0.56);
  const amp = rows * 0.34;
  const isles = new Layer(new Strip(s + 1, "hills", w / 2 + 5), 0.6, hz + amp * 0.56, amp, { colour: rgb(60, 130, 70) });
  if (!lk.land || rows < 8) return [hz, [isles]];
  isles.things = ["pine", "bush"];
  isles.gap = 3;
  isles.size = 2.0;
  const sand = rgb(206, 190, 140);
  const shore = new Layer(new Strip(s + 2, "dunes", w / 1.2 + 8), 0.0, rows * 1.02, rows * 0.34);
  shore.colour = { ...GROUND, summer: sand, spring: sand, autumn: rgb(196, 178, 132) };
  shore.things = ["palm", "rock", "bush", "house", "rock"];
  shore.size = tall(rows, 0.3, 3, 7);
  shore.gap = rnd(shore.size) + 4;
  shore.density = 0.6;
  return [hz, [isles, shore]];
}

function mountain(lk, w, rows) {
  const s = lk.seed;
  const hz = rnd(rows * 0.7);
  const peaks = new Layer(new Strip(s, "mountains", w / 3.5 + 4), 0.7, hz + 1, rows * 0.62, { colour: rgb(112, 112, 124), snow: 0.42 });
  if (rows <= 6) return [hz, [peaks]];
  const near = new Layer(new Strip(s + 2, "hills", w / 1.6 + 6), 0.0, rows * 0.95, rows * 0.2);
  if (!lk.land || rows < 8) return [hz, [peaks, near]];
  const mid = new Layer(new Strip(s + 1, "mountains", w / 2.2 + 4), 0.42, hz + 2, rows * 0.36, {
    colour: { ...GROUND, summer: rgb(78, 112, 84) },
    snow: 0.86,
    things: ["pine"],
    gap: 3,
    density: 0.5,
    size: 2.0,
  });
  near.things = ["pine", "pine", "house", "pine", "rock", "tree"];
  near.size = tall(rows, 0.3, 3, 8);
  near.gap = rnd(near.size * 0.6) + 2;
  near.density = 0.75;
  return [hz, [peaks, mid, near]];
}

function city(lk, w, rows) {
  const s = lk.seed;
  const hz = rnd(rows * 0.8);
  const far = new Layer(new Strip(s, "city", 4), 0.55, rows, rows * 0.5, { colour: SLATE });
  if (rows <= 6) return [hz, [far]];
  const near = new Layer(new Strip(s + 1, "city", w < 26 ? 5 : 6), 0.12, rows, rows * 0.62, { colour: rgb(92, 94, 104) });
  if (!lk.land || rows < 8) return [hz, [far, near]];
  const hills = new Layer(new Strip(s + 2, "hills", w / 2 + 6), 0.85, hz, rows * 0.24, { colour: rgb(96, 110, 128) });
  return [hz, [hills, far, near]];
}

const LAND = { countryside, sea, mountain, city };
const LANDS = new Memo(32);
/** `[horizon, layers]` of the look `lk` at `w` by `rows` pixels. */
export const landOf = (lk, w, rows) => LANDS.get(`${lk.place},${lk.seed},${lk.land},${w},${rows}`, () => LAND[lk.place](lk, w, rows));

// ---- what the scene is made of ----

const WINGS = ["⠑⠊", "⠒⠒", "⠊⠑", "⠒⠒"];

/** The cells `[x, y]` of the birds that cross the sky at `t`. */
export function flock(w, light, seed, t, hz) {
  const slot = 45.0;
  const cross = 32.0;
  const k = Math.trunc(floorDiv(t, slot));
  const p = (t - k * slot) / cross;
  if (p > 1 || noise(seed + 41, k) > 0.8 || light < 0.45) return [];
  const count = w < 12 ? 1 : 1 + Math.trunc(noise(seed + 42, k) * 4);
  const back = noise(seed + 43, k) < 0.5;
  const top = Math.max(1, Math.floor(hz / 2) - 2);
  const row = Math.trunc(noise(seed + 44, k) * top);
  const out = [];
  for (let i = 0; i < count; i++) {
    let x = rnd(-3 + p * (w + 6 + 3 * count)) - 3 * i;
    x = back ? w - 2 - x : x;
    const y = row + Math.floor((i + 1) / 2) * (i % 2 ? 1 : -1) + rnd(Math.sin(t * 0.5 + i));
    out.push([x, y]);
  }
  return out;
}

const parts = { birds, meteor, rain, snow, stars };
/** The painters a test may replace (the twin's monkeypatch). */
export const painters = parts;

function birds(half, sky, seed, t, hz) {
  flock(half.w, sky.light, seed, t, hz).forEach(([x, y], i) => {
    if (!(0 <= y && y < half.h && 0 <= x && x < half.w - 1)) return;
    const ink = mix(half.px[2 * y][x], rgb(24, 26, 36), 0.8);
    half.put(x, y, WINGS[Math.trunc(t * 3 + i * 1.7) % 4], ink);
  });
}

function line(dots, x0, y0, x1, y1) {
  const n = Math.max(1, rnd(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))));
  for (let i = 0; i <= n; i++) dots.set(rnd(x0 + ((x1 - x0) * i) / n), rnd(y0 + ((y1 - y0) * i) / n));
}

function meteor(half, sky, seed, t, hz) {
  const slot = 38.0;
  const burn = 0.9;
  const k = Math.trunc(floorDiv(t, slot));
  const p = (t - k * slot - 3) / burn;
  if (!(0 <= p && p <= 1) || noise(seed + 51, k) > 0.55 || sky.stars < 0.5) return;
  const dots = new Braille(half.w, half.h);
  const wide = 2 * half.w;
  const high = 2 * hz;
  const x0 = (0.15 + 0.7 * noise(seed + 52, k)) * wide;
  const y0 = 0.05 * high;
  const lean = noise(seed + 53, k) < 0.5 ? -1 : 1;
  const run = 0.55 * Math.min(wide, 2 * high);
  const head = p * run;
  const tail = Math.max(0.0, p * run - 0.3 * run);
  line(dots, x0 + lean * tail, y0 + tail * 0.5, x0 + lean * head, y0 + head * 0.5);
  half.ink(dots, rgb(255, 250, 226));
}

function smoke(half, sky, marks, seed, t) {
  const grey = sky.shade(rgb(214, 214, 220));
  for (const mark of marks) {
    if (mark.kind !== "chimney" || noise(seed + 61, mark.n) > 0.75) continue;
    for (let i = 0; i < 4; i++) {
      const age = (t * 0.7 + i) % 4;
      const x = mark.x + rnd(age * 0.6 + Math.sin(t * 0.9 + i * 2.1) * 0.7);
      half.set(x, mark.y - 1 - rnd(age * 1.2), grey, 0.62 * (1 - age / 4.4));
    }
  }
}

function balloon(half, sky, seed, t, hz) {
  const slot = 260.0;
  const cross = 220.0;
  const k = Math.trunc(floorDiv(t + 250, slot));
  const p = (t + 250 - k * slot) / cross;
  if (p > 1 || half.w < 18 || sky.light < 0.6 || noise(seed + 71, k) > 0.7) return;
  let x = rnd(-3 + p * (half.w + 6));
  x = noise(seed + 72, k) < 0.5 ? half.w - x : x;
  const y = rnd(hz * (0.22 + 0.2 * noise(seed + 73, k)) + Math.sin(t * 0.21) * 1.2);
  const [side, band] = k % 2 ? [rgb(224, 62, 62), rgb(250, 206, 76)] : [rgb(66, 120, 210), WHITE];
  for (const [dy, wide] of [
    [-3, 1],
    [-2, 3],
    [-1, 3],
    [0, 1],
  ]) {
    const r = Math.floor(wide / 2);
    for (let dx = -r; dx <= r; dx++) half.set(x + dx, y + dy, sky.shade(dx ? side : band));
  }
  half.set(x, y + 2, sky.shade(rgb(110, 80, 54)));
}

function plane(half, sky, seed, t, hz) {
  const slot = 80.0;
  const cross = 34.0;
  const k = Math.trunc(floorDiv(t + 20, slot));
  const p = (t + 20 - k * slot) / cross;
  if (p > 1 || half.w < 12 || sky.light > 0.25 || noise(seed + 81, k) > 0.6) return;
  let x = rnd(p * (half.w + 2)) - 1;
  x = noise(seed + 82, k) < 0.5 ? half.w - 1 - x : x;
  const y = rnd(hz * (0.1 + 0.3 * noise(seed + 83, k)) - p * 2);
  half.set(x, y, Math.trunc(t * 2) % 2 ? rgb(255, 70, 60) : rgb(190, 196, 220));
}

function boat(half, sky, seed, t, hz) {
  const slot = 150.0;
  const cross = 130.0;
  const k = Math.trunc(floorDiv(t + 40, slot));
  const p = (t + 40 - k * slot) / cross;
  if (p > 1 || half.w < 12 || noise(seed + 91, k) > 0.8) return;
  let x = rnd(-3 + p * (half.w + 6));
  x = noise(seed + 92, k) < 0.5 ? half.w - x : x;
  const y = hz + 1 + Math.trunc(noise(seed + 93, k) * 2);
  const hull = sky.shade(rgb(120, 70, 50));
  const sail = sky.shade(rgb(246, 244, 236));
  half.rect(x - 1, y, 3, 1, hull);
  half.set(x, y - 1, sail);
  half.set(x, y - 2, sail);
  half.set(x + 1, y - 1, sail);
  if (sky.light < 0.3) half.set(x, y - 3, LAMP);
}

// ---- indoors ----

/** `c` indoors: as it is by day, under the lamp of the room at night. */
export function lit(c, light) {
  const lamp = rgb(Math.floor(((c >> 16) * ROOM[0]) / 400), Math.floor((((c >> 8) & 255) * ROOM[1]) / 400), Math.floor(((c & 255) * ROOM[2]) / 400));
  return mix(lamp, c, light);
}

const LAB = new Map();
/** CIE L*a*b* of the colour `n` of the 256. */
function lab(n) {
  let out = LAB.get(n);
  if (out) return out;
  const lin = (v) => {
    const u = v / 255;
    return u <= 0.04045 ? u / 12.92 : ((u + 0.055) / 1.055) ** 2.4;
  };
  const f = (u) => (u > 0.008856 ? u ** (1 / 3) : 7.787 * u + 16 / 116);
  const c = palette(n) & 0xffffff;
  const r = lin(c >> 16);
  const g = lin((c >> 8) & 255);
  const b = lin(c & 255);
  const x = f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047);
  const y = f(0.2126 * r + 0.7152 * g + 0.0722 * b);
  const z = f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883);
  out = [116 * y - 16, 500 * (x - y), 200 * (y - z)];
  LAB.set(n, out);
  return out;
}

const NEAREST = new Map();
/** `index` of `strip.js`, kept for more colours. */
export function nearest(c) {
  let n = NEAREST.get(c);
  if (n === undefined) {
    if (NEAREST.size >= 1 << 16) NEAREST.clear();
    NEAREST.set(c, (n = index(c)));
  }
  return n;
}

const GAPS = new Float64Array(256 * 256).fill(-1);
/** Contrast of the colours `m` and `n` of the 256: CIE76 with the
 * lightness weighed 1.5. */
function gap(m, n) {
  const key = m * 256 + n;
  let d = GAPS[key];
  if (d < 0) {
    const [l0, a0, b0] = lab(m);
    const [l1, a1, b1] = lab(n);
    d = GAPS[key] = Math.sqrt((1.5 * (l0 - l1)) ** 2 + (a0 - a1) ** 2 + (b0 - b1) ** 2);
  }
  return d;
}

/** Contrast of two colours as the 256 show them. */
export const apart = (a, b) => gap(nearest(a), nearest(b));

/** The edge of the cat in `c`: lit by the lamp at night, the more so
 * as the coat is dark. */
export function rim(c, light) {
  const top = Math.max(c >> 16, (c >> 8) & 255, c & 255);
  return mix(lit(c, light), RIM, 0.5 * (1 - light) * (1 - top / 255));
}

/** Contrast of `c` indoors with the pixels behind it, `[[light,
 * pixels], ...]` (the pixels as colours of the 256, `nearest`): the mean of the lowest third of the contrasts per
 * pixel, the worse of day and night. `withRim`: the better of `c` and
 * its rim. */
export function against(c, behind, withRim = false) {
  let worst = Infinity;
  for (const [light, pixels] of behind) {
    let best = 0.0;
    const inks = withRim ? [lit(c, light), rim(c, light)] : [lit(c, light)];
    for (const ink of inks) {
      const m = nearest(ink);
      const seen = Float64Array.from(pixels, (n) => gap(m, n)).sort();
      const low = seen.slice(0, Math.max(1, Math.floor(seen.length / 3)));
      best = Math.max(best, fsum(low) / low.length);
    }
    worst = Math.min(worst, best);
  }
  return worst;
}

/** Contrast of two things indoors, the worse of day and night. */
const between = (a, b) => Math.min(apart(lit(a, 0.0), lit(b, 0.0)), apart(lit(a, 1.0), lit(b, 1.0)));

/** The first of `fits` (`[it, contrast]`, made on demand) of `FLOOR`
 * at least; else the best. */
function first(fits) {
  let best = null;
  for (const make of fits) {
    const it = make();
    if (it[1] >= FLOOR) return it;
    if (best === null || it[1] > best[1]) best = it;
  }
  return best;
}

/** The first of `among` in `order` with `FLOOR` of contrast with what
 * is behind it, and with each of `others` (`[c, share]`) its share of
 * `FLOOR`; else the one of most contrast. With its contrast. `memo`:
 * the contrasts with `behind` known. */
function pick(among, behind, others, order, withRim = false, memo = new Map()) {
  const back = (c) => {
    let v = memo.get(c);
    if (v === undefined) memo.set(c, (v = against(c, behind, withRim)));
    return v;
  };
  const fit = (c) => Math.min(back(c), ...others.map(([o, k]) => between(c, o) / k));
  return first(order.map((i) => () => [among[i], fit(among[i])]));
}

/** The columns of the mullions, and the pixel row of the rail. */
export function bars(w, h) {
  if (w < 18) return [[], null];
  const cols = w < 34 ? [Math.floor(w / 2)] : [Math.floor(w / 3), w - 1 - Math.floor(w / 3)];
  return [cols, h >= 8 ? 2 * Math.floor(h / 3) + 1 : null];
}

function mullions(half, sky, paint) {
  const { w, h } = half;
  const [cols, rail] = bars(w, h);
  const ink = lit(paint, sky.light);
  for (const x of cols)
    for (let y = 0; y < 2 * h; y++) {
      half.px[y][x] = ink;
      half.over.delete((y >> 1) * w + x);
    }
  if (rail !== null)
    for (let x = 0; x < w; x++) {
      half.px[rail][x] = ink;
      half.over.delete((rail >> 1) * w + x);
    }
}

/** Cells of curtain on each side: drawn aside when `ease` is 1. */
export function curtain(w, ease) {
  const full = Math.max(1, Math.floor(w / 8));
  return Math.max(1, rnd(full - ease * (full - Math.max(1, Math.floor(full / 3)))));
}

function curtains(half, sky, cloth, ease) {
  const { w, h } = half;
  cloth = lit(cloth, sky.light);
  const wide = w >= 12 ? curtain(w, ease) : 0; // none on a porthole
  for (let i = 0; i < wide; i++) {
    // Folds: a light cell, a dark one; denser when drawn aside.
    const fold = [0.0, 0.2, 0.36][(i * (ease > 0.5 ? 2 : 1) + (i === wide - 1 ? 1 : 0)) % 3];
    const c = mix(cloth, rgb(20, 14, 24), fold);
    for (let y = 0; y < h; y++)
      for (const x of [i, w - 1 - i]) {
        half.put(x, y, " ", -1, c);
        half.px[2 * y][x] = half.px[2 * y + 1][x] = c;
      }
  }
}

export const CAT = SIT.rows;

/** The columns of the tail of a cat at `left`, to the right if `way`
 * is 1, to the left if -1. */
export const tailOf = (left, way) => [3, 4, 5].map((k) => left + 2 + way * k);

/** Where the cat sits: `[left, way]`, its left column and the way of
 * its tail; null if no cat. The body clear of the mullions and of the
 * drawn curtains, the tail clear of the mullions; a gap to a mullion
 * and a tail in the open preferred. Among those, by `seed`. */
export function perch(w, h, seed) {
  if (w < 18 || h < 6) return null;
  const [cols] = bars(w, h);
  const side = curtain(w, 0.0);
  let fits = [];
  for (let left = side; left < w - side - 4; left++) {
    if (cols.some((x) => left <= x && x <= left + 4)) continue;
    for (const way of [1, -1]) {
      const tail = tailOf(left, way);
      if (tail.some((x) => cols.includes(x) || !(0 <= x && x < w))) continue;
      const hidden = tail.filter((x) => x < side || x >= w - side).length;
      const near = cols.some((x) => left - 1 <= x && x <= left + 5) ? 1 : 0;
      fits.push([hidden, near, left, way]);
    }
  }
  if (!fits.length) return [side + 1, 1]; // not at the sizes there are
  let best = fits[0];
  for (const f of fits) if (f[0] < best[0] || (f[0] === best[0] && (f[1] < best[1] || (f[1] === best[1] && (f[2] < best[2] || (f[2] === best[2] && f[3] < best[3])))))) best = f;
  fits = fits.filter((f) => f[0] === best[0] && f[1] === best[1]);
  const [, , left, way] = fits[Math.trunc(noise(seed + 112) * fits.length)];
  return [left, way];
}

const SILLS = new Memo(64);
/** The sill of the cat at `at` (a perch): it has the run of the
 * window, between the curtains drawn aside. */
export function sillOf(w, h, seed, at) {
  return SILLS.get(`${w},${h},${seed},${at}`, () => {
    const [cols] = bars(w, h);
    const home = at[0] + 2;
    const open = curtain(w, 1.0);
    const shut = curtain(w, 0.0);
    return new Sill(seed, w, 2 * h, home, at[1], cols, Math.min(home, open + 2), Math.max(home, w - 3 - open), Math.min(home, shut + 2), Math.max(home, w - 3 - shut));
  });
}

/** A cat on the sill: alone, it lives its life; it hunts the pointer
 * along the sill. Indoors, before the mullions: a cell it covers loses
 * the glyph from outside. `cat`: the one that hunts, else one from the
 * trail of the pointer; `prey`: the column of what flies outside. */
export function drawCat(half, sky, t, pointer, room, cat = null, seed = 0, prey = null) {
  const w = half.w;
  const rows = 2 * half.h;
  if (room.perch === null) return;
  const sill = sillOf(w, half.h, seed, room.perch);
  const p = (cat || new Cat()).at(t, pointer, sill, sky.light, prey);
  let fur = cells(p, rows).filter(([x, y]) => 0 <= x && x < w && 0 <= y && y < rows);
  // The tail goes behind a mullion.
  if (p.shape.back) fur = fur.filter(([x]) => !sill.cols.includes(x) || Math.abs(x - p.x) < 3);
  // The top of each column: the rim the lamp lights at night.
  const edge = new Map();
  for (const [x, y] of fur) edge.set(x, Math.min(y, edge.get(x) ?? rows));
  const coat = lit(room.coat, sky.light);
  const edged = rim(room.coat, sky.light);
  for (const [x, y] of fur) {
    half.set(x, y, edge.get(x) === y ? edged : coat);
    half.over.delete((y >> 1) * w + x);
  }
}

/** The share of the sky the clouds cover, by weather. */
export const COVER = { clouds: 0.25, rain: 0.85, fog: 0.55, snow: 0.75 };
const copy = (rows) => rows.map((r) => r.slice());

/** The view, no window. Not `live`: the land, the clouds, the fog;
 * nothing that falls, passes or twinkles. */
export function outside(w, h, lk, sky, season, kind, t, pointer, live = true) {
  const seed = lk.seed;
  const half = new Half(w, h);
  const [hz, layers] = landOf(lk, w, 2 * h);
  let clear = heavens(half, sky, hz);
  if (kind !== null) {
    const amount = { clear: 0.14, clouds: 0.6 }[kind] ?? 0.3;
    clouds(half, sky, seed + 7, hz, amount, t * 0.25);
    if (sky.cover < 0.5) clear = copy(half.px); // stars and birds show between the clouds
  }
  const marks = [];
  for (const layer of layers) {
    if (lk.place === "sea" && layer.depth === 0.0) water(half, sky, hz, seed, t);
    marks.push(...paint(half, layer, sky, 0.0, season));
  }
  if (lk.place === "sea" && layers[layers.length - 1].depth !== 0.0) water(half, sky, hz, seed, t);
  if (lk.land) lights(half, sky, marks, seed, pointer);
  if (live && lk.passing) {
    smoke(half, sky, marks, seed, t);
    balloon(half, sky, seed, t, hz);
    plane(half, sky, seed, t, hz);
    if (lk.place === "sea") boat(half, sky, seed, t, hz);
  }
  if (kind === "fog") fog(half, sky, hz);
  if (!live) return half;
  parts.stars(half, clear, sky, seed + 3, lk.passing ? t : null, hz);
  if (lk.passing && kind !== "rain" && kind !== "fog" && kind !== "snow") {
    parts.birds(half, sky, seed, t, hz);
    parts.meteor(half, sky, seed, t, hz);
  }
  if (kind === "rain") parts.rain(half, sky, seed, t);
  else if (kind === "snow") parts.snow(half, sky, seed, t);
  return half;
}

const VIEWS = new Memo(64);
/** The view at noon and at midnight in the middle of `season`, still:
 * what the things indoors are chosen against. `[[light, half], ...]`. */
function views(lk, w, h, season, kind) {
  return VIEWS.get(`${lk.key},${w},${h},${season},${kind}`, () => {
    let month = MIDDLE[season][0];
    if (lk.lat < 0) month = ((month + 5) % 12) + 1;
    return [12, 0].map((hour) => {
      const sky = at(Date.UTC(2001, month - 1, 20, hour, 30), lk.lat, COVER[kind] ?? 0.0);
      return [sky.light, outside(w, h, lk, sky, season, kind, 0.0, null, false)];
    });
  });
}

/** The pixels `cells` (`[x, y]`) of each of `vs`, as colours of the 256. */
const behindOf = (vs, cells) => vs.map(([light, half]) => [light, cells.map(([x, y]) => nearest(half.px[y][x]))]);

const ROOMS = new Memo(256);
/** The things indoors, `{frame, cloth, coat, perch}`, chosen by
 * contrast with the view behind them and with each other; among those,
 * by the seed. A function of the look, the size, the season and the
 * weather: no colour changes as the time runs. */
export function room(lk, w, h, season, kind) {
  return ROOMS.get(`${lk.key},${w},${h},${season},${kind}`, () => {
    const seed = lk.seed;
    const rows = 2 * h;
    const vs = views(lk, w, h, season, kind);
    const order = (among, salt) => [...among.keys()].sort((i, j) => noise(seed + salt, i) - noise(seed + salt, j));
    const [cols, rail] = bars(w, h);
    let cells = [];
    for (const x of cols) for (let y = 0; y < rows; y++) cells.push([x, y]);
    if (rail !== null) for (let x = 0; x < w; x++) cells.push([x, rail]);
    const barsBehind = behindOf(vs, cells);
    const barsFit = new Map();
    const framed = (coat) => {
      if (!cols.length) return [FRAMES[0], Infinity];
      return pick(FRAMES, barsBehind, coat !== null ? [[coat, 1.0]] : [], [...FRAMES.keys()], false, barsFit);
    };

    // The cat first, with a paint of the mullions that suits it.
    const at = lk.cosy ? perch(w, h, seed) : null;
    let coat = null;
    let [frame] = framed(null);
    if (at !== null) {
      const [left, way] = at;
      const xs = new Set();
      for (let x = left - 1; x < left + 6; x++) xs.add(x);
      for (const x of tailOf(left, way)) for (const d of [-1, 0, 1]) xs.add(x + d);
      cells = [];
      for (const x of xs) if (0 <= x && x < w) for (let y = rows - 6; y < rows; y++) cells.push([x, y]);
      const behind = behindOf(vs, cells);
      const both = (i) => () => {
        const fit = against(COATS[i], behind, true);
        const [paint, gap] = framed(COATS[i]);
        return [[COATS[i], paint], Math.min(fit, gap)];
      };
      [[coat, frame]] = first(order(COATS, 111).map(both));
    }
    const near = cols.length ? [[frame, 1.0]] : [];
    if (coat !== null) near.push([coat, 0.6]);
    const side = curtain(w, 0.0);
    cells = [];
    for (let x = 0; x < Math.min(side + 1, w); x++) for (let y = 0; y < rows; y++) cells.push([x, y]);
    cells = [...cells, ...cells.map(([x, y]) => [w - 1 - x, y])];
    const [cloth] = pick(FABRICS, behindOf(vs, cells), near, order(FABRICS, 101));
    return { frame, cloth, coat: coat ?? COATS[0], perch: at };
  });
}

/** The window of `w` by `h` cells at the naive time `when`, `t` seconds
 * after the start, the pointer at `pointer`: rows of runs. `cat`: the
 * cat that hunts it, stepped from a call to the next; else the cat is
 * a function of `t` and of the trail of the pointer. */
export function scene(w, h, when, t = 0.0, lk = look(), pointer = null, cat = null) {
  return painted(w, h, when, t, lk, pointer, cat).rows(lk.deep);
}

export function painted(w, h, when, t, lk, pointer, cat) {
  when = minute(when); // the sky, a minute at a time
  let sky = at(when, lk.lat);
  let season = lk.seasons ? sky.season : "summer";
  let kind = lk.weather;
  if (kind === "day") kind = weatherOf(ordinal(when), lk.seed, season);
  if (kind === "snow") season = "winter";
  sky = at(when, lk.lat, COVER[kind] ?? 0.0);
  const half = outside(w, h, lk, sky, season, kind, t, pointer);
  const r = room(lk, w, h, season, kind);
  mullions(half, sky, r.frame);
  if (lk.cosy) {
    const ease = pointer !== null ? pointer.ease(t) : 0.0;
    let prey = null;
    if (lk.passing && kind !== "rain" && kind !== "fog" && kind !== "snow") {
      const [hz] = landOf(lk, w, 2 * h);
      const xs = flock(w, sky.light, lk.seed, t, hz)
        .map(([x]) => x)
        .filter((x) => 0 <= x && x < w);
      prey = xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null;
    }
    parts.cat(half, sky, t, pointer, r, cat, lk.seed, prey);
    curtains(half, sky, r.cloth, ease);
  }
  return half;
}
parts.cat = drawCat;

// ---- the app ----

/** The options of `init`, checked: throws a `RangeError`, its message
 * a line for the log. */
export function options(args) {
  const o = {
    land: args.land === true,
    seasons: args.seasons === true,
    sky: args.sky === true,
    passing: args.passing === true,
    cosy: args.cosy === true,
    all: args.all === true,
    place: args.place ?? null,
    weather: args.weather ?? null,
    random: args.random === true,
    at: null,
    season: args.season ?? null,
    speed: args.speed ?? 1.0,
    lat: args.lat ?? 47.0,
    rgb: args.rgb === true,
  };
  if (!(o.speed > 0)) throw new RangeError("window: --speed: more than 0");
  if (args.at != null) {
    o.at = clockOf(args.at);
    if (o.at === null) throw new RangeError(`window: --at: not HH:MM: ${args.at}`);
  }
  return o;
}

/** The look of the options `o` (after `drawn`), from the day `start`
 * (naive ms) unless a seed is given. */
export function lookOf(o, start, seed = null) {
  seed = seed ?? ordinal(start);
  let place = o.place;
  if (place === null || place === "random") {
    const pick = PLACES[Math.trunc(noise(seed + 1234) * PLACES.length)];
    place = o.all || place === "random" ? pick : "countryside";
  }
  return look({
    seed,
    place,
    lat: o.lat,
    land: o.all || o.land,
    seasons: o.all || o.seasons,
    weather: o.weather || (o.all || o.sky ? "day" : null),
    passing: o.all || o.passing,
    cosy: o.all || o.cosy,
    deep: o.rgb,
  });
}

/** `-r` and the start: `[era, look]` of the options `o` at the naive
 * time `wall`; `seed`, the draws' (`-r`). */
export function start(o, wall, seed = null) {
  if (o.random) drawn(o, "place", PLACES, new Twister(seed), wall);
  else o.days = 0;
  const era = new Era(wall, o.speed, o.at, o.season, o.days);
  return [era, lookOf(o, era.start, o.random ? seed : null)];
}

/** Seconds a frame: `BRISK` while the cat hunts; `PERIOD` when
 * something moves; else a minute of the scene, within `STILL`. */
export function pace(lk, speed, busy) {
  if (busy) return BRISK;
  return moving(lk) ? PERIOD : Math.min(STILL, Math.max(PERIOD, 60.0 / Math.max(1.0, speed)));
}

let deck;
let base; // session seconds of the start
let era;
let view;
let pointer;
let cat;
let speed;

const seconds = () => Date.now() / 1000;

export function init(args) {
  let o;
  try {
    o = options(args);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  const { unixMs, offsetS } = now();
  const b = random(4);
  const seed = ((b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0) % (1 << 30);
  [era, view] = start(o, naive(unixMs, offsetS), seed);
  speed = o.speed;
  base = seconds();
  pointer = new Pointer(() => seconds() - base);
  cat = new Cat();
  deck = new Deck({ border: BORDER });
  deck.add(
    "window",
    SIZES,
    (w, h) => {
      const t = seconds() - base;
      return scene(w, h, era.when(t), t, view, pointer, cat);
    },
    { title: "", hover: (x, y) => pointer.move(x, y), leave: () => pointer.leave() },
  );
  period(rnd(1000 * pace(view, speed, false)));
}

export function tick(ms) {
  deck.tick(ms);
  period(rnd(1000 * pace(view, speed, cat.busy)));
}

export function event(topic, data) {
  deck.event(topic, data);
}
