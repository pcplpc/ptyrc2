// SPDX-License-Identifier: GPL-3.0-or-later
// Train: the land going by the window of a train car.
//
// One widget, a variant a size, from 10x3 to 48x11; the border is the
// frame of the glass, rounded, a rail under it. Layers of `strip.js`,
// each at its depth: the far ones barely move, the near ones run.
// `scene(w, h, when, t, trip, pointer)` is pure: `when` the local naive
// time (the light of `sky.js`), `t` the seconds of the trip, of which
// `dist(trip, t)` makes pixels of the nearest plane. `FPS` frames a
// second; each layer scrolls evenly (`even`).
//
// Bare: the sky, two ranges of hills, the verge. `-l`: ridges, trees,
// houses lit at night, what brushes the glass; `-p`: poles, a sagging
// wire; `-e`: one event a `CYCLE`: a tunnel, a truss over a river, a
// train the other way, a level crossing, a station where it stops;
// `-y`: the season of the date; `-w`: the weather of the day; `-a`:
// all, and the line of the day. The pointer on the glass: a breath of
// fog, gone in `KEEP` seconds. `-r`: what is not given, drawn (`drawn`
// of `sky.js`) from the launcher's seed.

import { exit, log, now, period, random } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Braille } from "ptyrc/canvas";
import { Era, at, clockOf, drawn, minute, mix, mod, naive, ordinal, rgb, rnd, weather } from "./sky.js";
import { Half, LAMP, NEAR, Pointer, SEA, WHITE, alike, clouds, fog, heavens, lights, noise, paint, rain, snow, stars, water, wave } from "./strip.js";
import { LINES, land } from "./line.js";
import { Twister } from "./twister.js";

export { LINES };
/** The inner sizes of the variants. */
export const SIZES = [
  [10, 3],
  [16, 4],
  [24, 6],
  [34, 8],
  [48, 11],
];
export const EVENTS = ["tunnel", "station", "train", "bridge", "crossing"];
/** The frame of the glass: rounded above, a rail under. */
export const BORDER = { corners: "╭╮╘╛", top: "─", bottom: "═", left: "│", right: "│", style: { fg: 250 } };

export const FPS = 24;
export const PACE = 24.0; // pixels a second, the nearest plane
export const CYCLE = 70.0; // seconds from an event to the next
export const WHEN = 24.0; // seconds into a cycle, its event
export const BRAKE = 9.0;
export const STAND = 10.0;
export const SPAN = { tunnel: 8.0, bridge: 6.0, crossing: 1.5 };
export const CAR = 48;
export const CARS = 9;
export const RUSH = 6; // pixels a frame a train the other way goes by
const LAMPS = 48;
const PORTAL = 3;
export const POLES = 30;
export const KEEP = 6.0;

export const DARK = rgb(22, 20, 24);
const BRICK = rgb(62, 50, 48);
const BAKED = rgb(88, 58, 50);
export const MORTAR = rgb(40, 36, 38);
const ROCK = rgb(118, 110, 108);
const GLOW = rgb(255, 184, 24);
export const IRON = rgb(160, 40, 36);
export const STONE = rgb(150, 150, 158);
export const LIVERY = [rgb(204, 44, 44), rgb(40, 96, 170), rgb(236, 190, 60), rgb(60, 140, 90)];
export { LAMP };

/** What a trip shows, the time apart. `weather`: null (clear, no
 * cloud), `"day"` (the day's), or one of `WEATHERS`. */
export function trip(o = {}) {
  return { seed: 0, line: "plain", lat: 47.0, land: false, poles: false, events: false, seasons: false, weather: null, deep: false, ...o, memo: [0.0] };
}

/** Python's `a // b` of floats. */
function floorDiv(a, b) {
  const m = a % b;
  let div = (a - m) / b;
  if (m && b < 0 !== m < 0) div -= 1.0;
  if (div) {
    const f = Math.floor(div);
    return div - f > 0.5 ? f + 1.0 : f;
  }
  return 0.0;
}

// ---- the way ----

/** The event of the cycle `k`, one of `EVENTS`, or null. */
export function eventOf(trip, k) {
  if (!trip.events || noise(trip.seed + 500, k) > 0.86) return null;
  return EVENTS[Math.trunc(noise(trip.seed + 501, k) * EVENTS.length)];
}

function within(u) {
  const a = WHEN;
  const b = WHEN + BRAKE;
  const c = WHEN + BRAKE + STAND;
  if (u <= a) return u;
  if (u <= b) return a + (u - a) - (u - a) ** 2 / (2 * BRAKE);
  if (u <= c) return a + BRAKE / 2;
  if (u <= c + BRAKE) return a + BRAKE / 2 + (u - c) ** 2 / (2 * BRAKE);
  return u - BRAKE - STAND;
}

/** Seconds at full pace worth the cycles before `k`. */
function before(trip, k) {
  const memo = trip.memo;
  for (let i = memo.length; i <= k; i++) memo.push(memo[i - 1] + (eventOf(trip, i - 1) === "station" ? within(CYCLE) : CYCLE));
  return k <= 0 ? 0.0 : memo[k];
}

/** Pixels of the nearest plane gone by after `t` seconds: `PACE` a
 * second, less what a station takes. */
export function dist(trip, t) {
  t = Math.max(0.0, t);
  const k = Math.trunc(floorDiv(t, CYCLE));
  const u = t - k * CYCLE;
  return PACE * (before(trip, k) + (eventOf(trip, k) === "station" ? within(u) : u));
}

// ---- what goes by ----

function hung(rows) {
  const top = Math.max(1, rnd(rows * 0.1));
  return [top, 2 * top, Math.max(2.0, rows * 0.24)];
}

/** The cells a wire may cross, as one colour where their two pixels
 * are near one. */
export function cells(px, h) {
  const [, high, sag] = hung(2 * h);
  const end = Math.min(h, Math.floor((high + sag) / 4) + 1);
  for (let cy = Math.floor(high / 4); cy < end; cy++) {
    const up = px[2 * cy];
    const low = px[2 * cy + 1];
    for (let cx = 0; cx < up.length; cx++) if (alike(up[cx], low[cx])) up[cx] = low[cx] = mix(up[cx], low[cx], 0.5);
  }
}

/** Poles on the verge, at its pace, a wire that sags between; none
 * within the spans of `bare`. The poles painted; the wire returned:
 * `[cx, cy, bits, up, low]`, each cell it crosses as it is now. */
export function wires(half, sky, shift, verge, bare = []) {
  const w = half.w;
  const rows = 2 * half.h;
  shift = Math.floor(shift + 1e-6);
  const [top, high, sag] = hung(rows);
  const stands = (i) => !bare.some(([a, b]) => a - 2 <= i * POLES && i * POLES < b + 2);
  const dots = new Braille(w, half.h);
  for (let dx = 0; dx < 2 * w; dx++) {
    const x = (dx / 2 + shift) / POLES;
    const i = Math.floor(x);
    const u = x - i;
    if (!(stands(i) && stands(i + 1))) continue;
    dots.set(dx, rnd(high + sag * (1 - (2 * u - 1) ** 2)));
  }
  const out = dots.cells().map(([cx, cy]) => [cx, cy, dots.bits[cy * w + cx], half.px[2 * cy][cx], half.px[2 * cy + 1][cx]]);
  const wood = sky.shade(rgb(92, 66, 44));
  const last = Math.floor((shift + w) / POLES);
  for (let i = Math.floor(shift / POLES); i <= last; i++) {
    if (!stands(i)) continue;
    const x = i * POLES - shift;
    const foot = Math.min(rows, rnd(verge.top(i * POLES)) + 2);
    for (let y = top - 1; y < foot; y++) half.set(x, y, wood);
    half.set(x - 1, top, wood);
    half.set(x + 1, top, wood);
  }
  return out;
}

/** Draw the wires where the cell is as it was: not behind a thing. */
export function string(half, sky, strung) {
  const ink = mix(sky.shade(rgb(60, 50, 44)), 0, 0.3);
  for (const [cx, cy, bits, up, low] of strung) {
    if (half.px[2 * cy][cx] !== up || half.px[2 * cy + 1][cx] !== low) continue;
    const under = mix(up, low, 0.5);
    half.px[2 * cy][cx] = half.px[2 * cy + 1][cx] = under;
    const k = cy * half.w + cx;
    if (!half.over.has(k)) half.over.set(k, [String.fromCharCode(0x2800 + bits), ink, under]);
  }
}

const whole = (x) => Math.floor(x + 1e-6);
function ease(x) {
  x = Math.min(1.0, Math.max(0.0, x));
  return x * x * (3 - 2 * x);
}

/** Pixels of cutting before the mouth of a tunnel, and after. */
export const approach = (rows) => 2 * rows + 6;

function cutting(half, sky, x, near, at, seed, ground) {
  const rows = 2 * half.h;
  const f = ease(1 - near / approach(rows));
  const bump = (wave(seed + 710, at / 5) - 0.5) * 3 * (1 - f);
  const top = rnd(rows - 1 - (rows + 2) * f + bump);
  const body = mix(mix(ground, sky.shade(ROCK), 0.75 * f), 0, 0.1);
  for (let y = Math.max(0, top); y < rows; y++) {
    let c = body;
    if (y === top) c = mix(body, WHITE, 0.14);
    else if (noise(seed + 711, at, y) < 0.1) c = mix(body, 0, 0.16);
    half.px[y][x] = c;
  }
}

const sum3 = (c) => (c >> 16) + ((c >> 8) & 255) + (c & 255);

function tunnel(half, sky, x0, x1, shift, seed, ground) {
  const w = half.w;
  const rows = 2 * half.h;
  const a = whole(x0);
  const b = whole(x1);
  const s = whole(shift);
  const reach = approach(rows);
  const stone = sky.shade(STONE);
  const joint = sky.shade(mix(STONE, rgb(40, 40, 48), 0.45));
  for (let x = Math.max(0, a - reach); x < Math.min(w, b + reach); x++) {
    if (a - PORTAL <= x && x < b + PORTAL) continue;
    cutting(half, sky, x, x < a ? a - PORTAL - x : x - b - PORTAL, x + s, seed, ground);
  }
  const portals = [];
  for (let x = a - PORTAL; x < a; x++) portals.push(x);
  for (let x = b; x < b + PORTAL; x++) portals.push(x);
  for (const x of portals) {
    if (!(0 <= x && x < w)) continue;
    const inner = x === a - 1 || x === b;
    for (let y = 0; y < rows; y++) {
      const dark = inner || y % 3 === 2 || (mod(x + s + Math.floor(y / 3), 2) === 0 && y % 3 === 0);
      half.px[y][x] = dark ? joint : stone;
    }
  }
  const lampY = Math.max(1, rnd(rows * 0.22));
  const cable = rnd(rows * 0.44);
  const line = rnd(rows * 0.78);
  const ledge = Math.max(line + 1, rnd(rows * 0.88));
  const glow = Math.max(3.0, rows * 0.38);
  const day = sky.shade(rgb(176, 178, 184));
  const lo = Math.max(0, a);
  const hi = Math.min(w, b);
  const bricks = new Map();
  for (let x = lo; x < hi; x++) {
    const at = x + 2 * s;
    const aside = mod(at + LAMPS / 2, LAMPS) - LAMPS / 2;
    const mouth = Math.max(0.0, 1 - Math.min(x - a, b - 1 - x) / 10) * sky.light;
    for (let y = 0; y < rows; y++) {
      const course = Math.floor(y / 3);
      const run = at + 4 * (course % 2);
      let c;
      if (y >= ledge) c = y === ledge ? rgb(40, 38, 40) : DARK;
      else if (y === cable) c = mod(at, 16) === 0 ? rgb(108, 108, 112) : rgb(18, 18, 22);
      else if (y === line && mod(Math.floor(at / 6), 2) === 0) c = rgb(176, 170, 140);
      else if (y === 0) c = rgb(26, 24, 26);
      else if (y % 3 === 2 || mod(run, 8) === 0) c = MORTAR;
      else {
        const k = `${Math.floor(run / 8)},${course}`;
        c = bricks.get(k);
        if (c === undefined) bricks.set(k, (c = mix(BRICK, BAKED, noise(seed + 700, Math.floor(run / 8), course))));
      }
      if (-glow < aside && aside < glow && y < ledge) {
        const near = Math.hypot(aside, y - lampY);
        if (near < glow) c = mix(c, GLOW, 0.74 * (1 - near / glow) ** 1.5);
      }
      if (mouth) {
        const grey = Math.floor(sum3(c) / 3);
        c = mix(mix(c, rgb(grey, grey, grey), mouth), day, 0.4 * mouth);
      }
      half.px[y][x] = c;
    }
    if (aside === 0 || aside === 1) {
      half.set(x, lampY, rgb(255, 238, 186));
      half.set(x, lampY - 1, rgb(70, 66, 62));
    }
  }
  return [lo, hi];
}

function bridge(half, sky, x0, x1, shift, hz, seed, t) {
  const w = half.w;
  const rows = 2 * half.h;
  const a = whole(x0);
  const b = whole(x1);
  const s = whole(shift);
  const iron = sky.shade(IRON);
  const dark = sky.shade(mix(IRON, rgb(26, 18, 22), 0.55));
  const top = Math.max(1, rnd(rows * 0.1));
  const low = Math.max(3, rnd(rows * 0.87));
  const bay = Math.max(6, low - top);
  const line = Math.min(low - 1, Math.max(hz + 1, rnd(rows * 0.62)));
  const bank = Math.max(4, Math.floor(rows / 2));
  const deep = sky.shade(SEA);
  const reeds = sky.shade(rgb(40, 110, 50));
  const earth = sky.shade(rgb(132, 112, 84));
  const pier = sky.shade(STONE);
  const beat = Math.trunc(t * 2);
  for (let x = Math.max(0, a); x < Math.min(w, b); x++) {
    const along = x - a;
    const back = b - 1 - x;
    const edge = line + Math.max(0, rnd((rows - line) * (1 - Math.min(along, back) / bank)));
    for (let y = line; y < rows; y++) {
      if (y < edge) continue;
      let c;
      if (y === edge) c = edge === line ? reeds : earth;
      else {
        const v = (y - line) / Math.max(1, rows - line);
        const murk = (0.3 + 0.15 * v) * (0.35 + 0.65 * sky.light);
        c = mix(sky.colour((x + 0.5) / w, 1 - 0.85 * v), deep, murk);
        const ripple = noise(seed + 9, Math.floor((x + s) / 3) + beat * 7, y) < 0.09;
        if (ripple && sky.light > 0.3) c = mix(c, WHITE, 0.1 + 0.25 * sky.light);
      }
      half.px[y][x] = c;
    }
    const post = along % bay === 0 || along < 2 || back < 2;
    let slope = along % (2 * bay);
    slope = slope < bay ? slope : 2 * bay - slope;
    const brace = top + rnd((slope * (low - top)) / bay);
    for (let y = top - 1; y < rows; y++) {
      let c;
      if (y <= top || y === low) c = iron;
      else if (y > low) {
        const stony = Math.min(along, back) < 3;
        c = stony ? pier : dark;
        if (!stony && along % 4 === 0) c = mix(dark, iron, 0.4);
      } else if (post) c = iron;
      else if (y === brace) c = dark;
      else continue;
      if (y >= 0) half.px[y][x] = c;
    }
  }
}

function other(half, sky, head, n) {
  const w = half.w;
  const rows = 2 * half.h;
  const coat = LIVERY[n % LIVERY.length];
  const body = sky.shade(coat);
  const roof = sky.shade(mix(coat, rgb(40, 40, 48), 0.6));
  const line = sky.shade(mix(coat, WHITE, 0.7));
  const door = sky.shade(mix(coat, 0, 0.25));
  const under = sky.shade(rgb(44, 42, 48));
  const pane = mix(LAMP, sky.shade(rgb(150, 196, 226)), sky.light);
  const top = rnd(rows * 0.2);
  const lintel = rnd(rows * 0.34);
  const sill = rnd(rows * 0.6);
  const skirt = Math.max(sill + 2, rnd(rows * 0.88));
  const long = CAR * CARS;
  for (let x = Math.max(0, head); x < Math.min(w, head + long); x++) {
    const along = x - head;
    const i = along % CAR;
    const nose = Math.max(0, 5 - along) + Math.max(0, along - long + 6);
    for (let y = top + nose; y < rows; y++) {
      let c;
      if (i >= CAR - 2 && along < long - 2) {
        if (y < top + 2) continue;
        c = under;
      } else if (y >= skirt) c = under;
      else if (y === top + nose) c = roof;
      else if (y === sill + 1) c = line;
      else if (lintel <= y && y < sill && 9 <= i && i < CAR - 11 && (i - 9) % 14 < 12) c = pane;
      else if (lintel <= y && y < sill && ((3 <= i && i < 6) || (CAR - 8 <= i && i < CAR - 5))) c = y < lintel + 2 ? pane : door;
      else if (y > lintel && ((2 <= i && i < 7) || (CAR - 9 <= i && i < CAR - 4))) c = door;
      else c = body;
      half.px[y][x] = c;
    }
  }
  for (const [x, lamp] of [
    [head, rgb(255, 246, 206)],
    [head + long - 1, rgb(236, 44, 40)],
  ]) {
    half.set(x, sill + 2, lamp);
    half.set(x, sill + 3, lamp);
  }
}

function crossing(half, sky, x0, x1, n, t) {
  const rows = 2 * half.h;
  if (rows < 12) return;
  const a = whole(x0);
  const b = whole(x1);
  const mid = Math.floor((a + b) / 2);
  const reach = Math.floor((b - a) / 2) - 4;
  const far = rnd(rows * 0.66);
  const boom = rnd(rows * 0.8);
  const tar = sky.shade(rgb(84, 84, 92));
  const white = sky.shade(rgb(226, 226, 214));
  for (let y = far; y < rows; y++) {
    const wide = 3 + rnd(((reach - 3) * (y - far)) / Math.max(1, rows - 1 - far));
    for (let x = mid - wide; x <= mid + wide; x++) half.set(x, y, x === mid && y % 2 ? white : tar);
  }
  const night = sky.light < 0.4;
  const coat = sky.shade(LIVERY[(n + 1) % 3]);
  half.rect(mid - 3, far - 1, 5, 2, coat);
  half.rect(mid - 2, far - 2, 3, 1, sky.shade(rgb(50, 70, 96)));
  for (const x of [mid - 3, mid + 1]) half.set(x, far, night ? LAMP : white);
  const post = sky.shade(rgb(120, 120, 128));
  const red = rgb(226, 44, 40);
  for (let y = boom - 2; y < boom + 2; y++) half.set(mid - reach, y, post);
  for (let x = mid - reach + 1; x < mid + reach; x++) half.set(x, boom - 1, mod(Math.floor((x - mid) / 2), 2) ? red : rgb(244, 244, 240));
  const mast = mid + reach + 1;
  for (let y = far - 4; y < boom + 2; y++) half.set(mast, y, post);
  const beat = Math.trunc(t * 3) % 2;
  half.set(mast - 1, far - 4, beat ? red : rgb(70, 24, 24));
  half.set(mast + 1, far - 4, beat ? rgb(70, 24, 24) : red);
  half.set(mast, far - 5, white);
}

function station(half, sky, at, trip, k) {
  const w = half.w;
  const rows = 2 * half.h;
  const long = w + 150;
  const floor = rnd(rows * 0.76);
  const roof = Math.max(1, rnd(rows * 0.26));
  const dark = sky.light < 0.4;
  const start = Math.ceil(at);
  const lit = (c) => (dark ? mix(sky.shade(c), mix(c, rgb(120, 96, 40), 0.35), 0.6) : sky.shade(c));
  const stone = lit(STONE);
  const edge = lit(rgb(232, 208, 96));
  const beam = lit(rgb(128, 62, 52));
  const tiles = lit(rgb(160, 84, 66));
  const post = lit(rgb(96, 96, 104));
  const wood = lit(rgb(66, 110, 84));
  for (let x = Math.max(0, start); x < Math.min(w, start + long); x++) {
    const i = x - start;
    const under = 24 <= i && i < long - 24;
    const bay = mod(i - 24, 22);
    const pool = dark && under && Math.abs(bay - 11) <= 4;
    for (let y = floor; y < rows; y++) {
      const c = y === floor ? edge : stone;
      half.px[y][x] = pool ? mix(c, rgb(214, 170, 100), 0.5) : c;
    }
    if (!under) continue;
    half.set(x, roof, beam);
    half.set(x, roof - 1, tiles);
    if (bay === 0) for (let y = roof + 1; y < floor; y++) half.set(x, y, post);
    else if (bay === 11) half.set(x, roof + 1, dark ? LAMP : post);
    else if ((bay === 10 || bay === 12) && dark) half.set(x, roof + 1, LAMP, 0.35);
    if (15 <= bay && bay < 19 && rows >= 16) {
      half.set(x, floor - 1, wood);
      half.set(x, floor - 2, wood, 0.7);
      continue;
    }
    const who = noise(trip.seed + 600 + k, Math.floor((i - 24) / 5));
    if (mod(i - 24, 5) === 2 && who < 0.3 && bay !== 0 && bay !== 1 && bay !== 21) {
      const coat = LIVERY[Math.trunc(who * 40) % LIVERY.length];
      half.set(x, floor - 1, lit(mix(coat, rgb(30, 30, 40), 0.4)));
      half.set(x, floor - 2, lit(coat));
      half.set(x, floor - 3, lit(rgb(226, 190, 160)));
    }
  }
  const sign = start + Math.floor(long / 2) - 4;
  half.rect(sign, roof + 2, 8, 3, lit(rgb(40, 60, 130)));
  for (const dx of [1, 2, 3, 5, 6]) half.set(sign + dx, roof + 3, lit(rgb(244, 244, 236)));
}

/** Fog on the glass where the pointer has been, fading. */
export function breath(half, pointer, t) {
  if (!pointer) return;
  const r = 2.6;
  for (const [x, y, when] of pointer.trail) {
    const left = 1 - (t - when) / pointer.keep;
    if (left <= 0) continue;
    const cx = x + 0.5;
    const cy = 2 * y + 1.0;
    for (let py = Math.trunc(cy - r - 1); py < Math.trunc(cy + r + 2); py++) {
      for (let px = Math.trunc(cx - r - 1); px < Math.trunc(cx + r + 2); px++) {
        const d = Math.hypot(px + 0.5 - cx, py + 0.5 - cy) / r;
        if (d <= 1) half.set(px, py, rgb(226, 232, 238), 0.7 * left * (1 - d * d * 0.6));
      }
    }
  }
}

/** The nearest rate that scrolls evenly. */
export const even = (rate) => (rate >= 0.75 ? rnd(rate) : 1 / rnd(1 / Math.max(rate, 0.004)));

const LANDS = new Map();
function landOf(trip, rows) {
  const key = `${trip.line},${trip.seed},${rows},${trip.land}`;
  let out = LANDS.get(key);
  if (!out) {
    out = land(trip.line, trip.seed, rows, trip.land);
    for (const layer of [...out.back, ...out.front]) layer.speed = even(layer.rate());
    if (LANDS.size >= 16) LANDS.clear();
    LANDS.set(key, out);
  }
  return out;
}

const COVER = { clouds: 0.25, rain: 0.85, fog: 0.55, snow: 0.75 };
const copy = (rows) => rows.map((r) => r.slice());

/** The glass of `w` by `h` cells at the naive time `when`, `t` seconds
 * into the trip, the pointer at `pointer`: rows of runs. */
export function scene(w, h, when, t = 0.0, tr = trip(), pointer = null) {
  const seed = tr.seed;
  when = minute(when);
  let sky = at(when, tr.lat);
  let season = tr.seasons ? sky.season : "summer";
  let kind = tr.weather;
  if (kind === "day") kind = weather(ordinal(when), seed, season);
  if (kind === "snow") season = "winter";
  const cover = COVER[kind] ?? 0.0;
  sky = at(when, tr.lat, cover);

  const half = new Half(w, h);
  const rows = 2 * h;
  const { hz, sea } = landOf(tr, rows);
  let { back, front } = landOf(tr, rows);
  const verge = back[back.length - 1];
  const shift = dist(tr, t);
  let clear = heavens(half, sky, hz);
  if (kind !== null) {
    const amount = { clear: 0.14, clouds: 0.6 }[kind] ?? 0.3;
    clouds(half, sky, seed + 7, hz, amount, -shift * 0.012, 2);
    if (cover < 0.5) clear = copy(half.px);
  }

  // What the trip meets, in pixels of the way: `[what, cycle, x0, x1]`.
  const k = Math.trunc(floorDiv(t, CYCLE));
  const met = [];
  for (const n of [k - 1, k]) {
    const what = n >= 0 ? eventOf(tr, n) : null;
    const start = n * CYCLE + WHEN;
    if (what === "station") {
      const x0 = dist(tr, start + BRAKE) - 75;
      met.push([what, n, x0, x0 + w + 150]);
    } else if (what === "train") {
      const end = start + (w + CAR * CARS) / (RUSH * FPS);
      met.push([what, n, dist(tr, start) + w, dist(tr, end)]);
    } else if (what === "tunnel") {
      const x0 = dist(tr, start) + w;
      met.push([what, n, x0, x0 + PACE * SPAN[what] + 2 * approach(rows)]);
    } else if (what !== null) {
      const x0 = dist(tr, start) + w;
      met.push([what, n, x0, x0 + PACE * SPAN[what]]);
    }
  }
  const poles = met.filter(([what]) => what !== "train" && what !== "tunnel").map(([, , x0, x1]) => [x0, x1]);
  for (const layer of front) {
    const reach = layer.size + 3;
    const rate = layer.rate();
    layer.bare = met.map(([, , x0, x1]) => [rate * (x0 - w) - reach, rate * x1 + w + reach]);
  }

  const pad = approach(rows);
  if (met.some(([what, , x0, x1]) => what === "tunnel" && x0 + pad <= shift && x1 - pad >= shift + w)) {
    back = [];
    front = [];
  }
  const marks = [];
  back.forEach((layer, i) => {
    if (sea && i === 1) water(half, sky, hz, seed, t);
    marks.push(...paint(half, layer, sky, shift, season));
  });
  if (tr.land) lights(half, sky, marks, seed, null);
  if (kind === "fog") fog(half, sky, hz);
  let strung = [];
  if (tr.poles && rows >= 8 && back.length) {
    cells(half.px, h);
    cells(clear, h);
    strung = wires(half, sky, shift, back[back.length - 1], poles);
  }
  for (const layer of front) paint(half, layer, sky, shift, season);

  let dry = null;
  for (const [what, n, x0, x1] of met) {
    const a = x0 - shift;
    const b = x1 - shift;
    if (what === "train") {
      const head = w - RUSH * whole((t - n * CYCLE - WHEN) * FPS);
      if (-CAR * CARS < head && head < w) other(half, sky, head, n);
    } else if (a >= w || b <= 0) continue;
    else if (what === "station") station(half, sky, a, tr, n);
    else if (what === "bridge") bridge(half, sky, a, b, shift, hz, seed, t);
    else if (what === "crossing") crossing(half, sky, a, b, n, t);
    else if (what === "tunnel") dry = tunnel(half, sky, a + pad, b - pad, shift, seed, sky.shade(verge.ground(season)));
  }

  stars(half, clear, sky, seed + 3, null, hz);
  string(half, sky, strung);
  const glass = new Set(half.over.keys());
  const was = copy(half.px);
  if (kind === "rain") rain(half, sky, seed, t, 1.5, 0.35);
  else if (kind === "snow") snow(half, sky, seed, t, 0.45);
  if (dry !== null) {
    for (const key of [...half.over.keys()]) {
      const cx = key % w;
      if (!glass.has(key) && dry[0] <= cx && cx < dry[1]) half.over.delete(key);
    }
    // As the twin's slice: an end under 0 counts from the right.
    const lo = dry[0] < 0 ? Math.max(0, dry[0] + w) : Math.min(dry[0], w);
    const hi = dry[1] < 0 ? Math.max(0, dry[1] + w) : Math.min(dry[1], w);
    if (hi > lo) for (let y = 0; y < was.length; y++) half.px[y].set(was[y].subarray(lo, hi), lo);
  }
  breath(half, pointer, t);
  return half.rows(tr.deep);
}

// ---- the app ----

/** The options of `init`, checked: throws a `RangeError`, its message
 * a line for the log. */
export function options(args) {
  const o = {
    land: args.land === true,
    poles: args.poles === true,
    events: args.events === true,
    seasons: args.seasons === true,
    sky: args.sky === true,
    all: args.all === true,
    line: args.line ?? null,
    weather: args.weather ?? null,
    after: args.after ?? 0.0,
    random: args.random === true,
    at: null,
    season: args.season ?? null,
    speed: args.speed ?? 1.0,
    lat: args.lat ?? 47.0,
    rgb: args.rgb === true,
  };
  if (!(o.speed > 0)) throw new RangeError("train: --speed: more than 0");
  if (args.at != null) {
    o.at = clockOf(args.at);
    if (o.at === null) throw new RangeError(`train: --at: not HH:MM: ${args.at}`);
  }
  return o;
}

/** The trip of the options `o` (after `drawn`), from the day `start`
 * (naive ms) unless a seed is given. */
export function tripOf(o, start, seed = null) {
  seed = seed ?? ordinal(start);
  let line = o.line;
  if (line === null || line === "random") {
    const pick = LINES[Math.trunc(noise(seed + 4321) * LINES.length)];
    line = o.all || line === "random" ? pick : "plain";
  }
  return trip({
    seed,
    line,
    lat: o.lat,
    land: o.all || o.land,
    poles: o.all || o.poles,
    events: o.all || o.events,
    seasons: o.all || o.seasons,
    weather: o.weather || (o.all || o.sky ? "day" : null),
    deep: o.rgb,
  });
}

/** `-r` and the start: `[era, trip, after]` of the options `o` at the
 * naive time `wall`; `seed`, the draws' (`-r`). */
export function start(o, wall, seed = null) {
  let s = null;
  if (o.random) {
    s = seed;
    drawn(o, "line", LINES, new Twister(s), wall);
    if (!o.after) {
      const rng = new Twister(s);
      o.after = rng.randrange(40) * CYCLE + rng.uniform(0.0, WHEN - 6.0);
    }
  } else o.days = 0;
  const era = new Era(wall, o.speed, o.at, o.season, o.days);
  return [era, tripOf(o, era.start, s), o.after];
}

let deck;
let frame;
let base; // session seconds of the trip's 0
let era;
let route;
let pointer;

const seconds = () => Date.now() / 1000;

function next(nowS) {
  period(Math.max(1, Math.ceil(((frame + 1) / FPS + base - nowS) * 1000)));
}

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
  let after;
  [era, route, after] = start(o, naive(unixMs, offsetS), seed);
  const s = seconds();
  base = s - after;
  frame = Math.floor(after * FPS);
  pointer = new Pointer(() => seconds() - base, KEEP);
  deck = new Deck({ border: BORDER });
  deck.add(
    "train",
    SIZES,
    (w, h) => {
      const t = frame / FPS;
      return scene(w, h, era.when(t), t, route, pointer);
    },
    {
      title: "",
      hover: (x, y) => {
        pointer.move(x, y);
        return false;
      },
      leave: () => {
        pointer.leave();
        return false;
      },
    },
  );
  next(s);
}

export function tick(ms) {
  const s = ms / 1000;
  // A frame a draw, so that the land scrolls evenly; more than a frame
  // off the clock, it catches up.
  const due = (s - base) * FPS;
  frame = Math.abs(due - frame - 1) < 1.5 ? frame + 1 : Math.floor(due);
  deck.tick(ms);
  next(s);
}

export function event(topic, data) {
  deck.event(topic, data);
}
