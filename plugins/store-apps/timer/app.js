// SPDX-License-Identifier: GPL-3.0-or-later
// Timer: a countdown, a pomodoro, an hourglass, in one widget.
//
// One tag, `timer`, a variant per face: `MM:SS`, the time and a bar,
// block digits (`digits.js`, those of clock); the driver picks the
// face. A face is a function of a state: `face(state, w, h, look,
// hover)`; the state is that of a `Run` at a time of the session clock.
//
// `-g`: an hourglass. Its sand is counted: `sand(k, n)` is the glass
// after `n` grains fell, and a grain falls every `length / N` seconds:
// the last one lands as the time ends. A new phase turns it over.
//
// The end: `-r`, the border `alert` (and a while at a new phase); `-b`,
// an explosion of the effect `confetti` on the widget; `--modal`, the
// widget shown again as a modal. `--click`: a press on the widget
// pauses, and goes on. `--until HH:MM`: the wall clock (`clock`).

import { PtyrcError, call, civil, exit, log, now as wall, period } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { LARGE, SMALL, blocks as digitRows, fit, width } from "./digits.js";

/** Session seconds a glass takes to turn over. */
export const TURN = 1.2;
/** Session seconds the border rings at a change of phase. */
export const RING = 6.0;
/** Session seconds between two drawings of a glass that runs. */
export const STREAM = 0.2;

// Colours: the twin's names.
const WHITE = 7;
const YELLOW = 3;
const BRIGHT_YELLOW = 11;
const BRIGHT_RED = 9;
const BRIGHT_GREEN = 10;
const BRIGHT_CYAN = 14;
const ORANGE = 208;
export const DIM = 8; // bright black
/** Colours of the 256, with `-c`: the sand, a grain in the air, the
 * plates, the glass. */
export const SAND = 221;
export const FALL = 229;
export const WOOD = 137;
export const GLASS = 110;

/** Python's `round`: half to even. */
function round(x) {
  const r = Math.round(x);
  return Math.abs(x % 1) === 0.5 && r % 2 ? r - 1 : r;
}

// -- the time ----------------------------------------------------------------

const UNITS = /^(?:(\d+(?:\.\d+)?)h)?(?:(\d+(?:\.\d+)?)m)?(?:(\d+(?:\.\d+)?)s)?$/;

/** Seconds of `25m`, `90s`, `1h30m`, `1.5h`, `1:30` (minutes and
 * seconds), `1:30:00`, or `25` (minutes). Throws a `RangeError`. */
export function duration(text) {
  text = text.trim().toLowerCase();
  let seconds = -1;
  let found;
  if (/^\d+(:\d\d){1,2}$/.test(text)) {
    seconds = text.split(":").reduce((s, part) => 60 * s + Number(part), 0);
  } else if (/^\d+(\.\d+)?$/.test(text)) {
    seconds = 60 * Number(text);
  } else if (text && (found = UNITS.exec(text))) {
    const [h, m, s] = found.slice(1).map((g) => Number(g ?? 0));
    seconds = 3600 * h + 60 * m + s;
  }
  if (!(seconds > 0)) throw new RangeError(`not a duration: '${text}'`);
  return seconds;
}

/** Seconds from `day` (seconds since midnight) to the next `HH:MM`. */
export function until(text, day) {
  const m = /^(\d{1,2}):(\d{1,2})$/.exec(text.trim());
  if (!m || Number(m[1]) > 23 || Number(m[2]) > 59) throw new RangeError(`not a time HH:MM: '${text}'`);
  let then = 3600 * Number(m[1]) + 60 * Number(m[2]);
  if (then <= day) then += 86400;
  return then - day;
}

/** `25/5`: the work and the break, as `duration`. */
export function pomodoro(text) {
  const at = text.indexOf("/");
  if (at < 0) throw new RangeError(`not WORK/BREAK: '${text}'`);
  return [duration(text.slice(0, at)), duration(text.slice(at + 1))];
}

/** The phases `{name, length}`: one `timer`, or `rounds` of `work` with
 * a `break` between two; `rounds` 0: one work and one break, to loop. */
export function plan(length = null, work = null, rest = 0, rounds = 4) {
  if (work === null) return [{ name: "timer", length: length || 0 }];
  const pair = [
    { name: "work", length: work },
    { name: "break", length: rest },
  ];
  if (!rounds) return pair;
  return [...Array.from({ length: rounds - 1 }, () => pair).flat(), pair[0]];
}

/** A run at a moment: the fields of `state()`, the defaults beside. */
export function state(fields = {}) {
  return {
    name: "timer",
    length: 1,
    left: 1, // seconds of the phase
    index: 0, // phases begun before this one
    round: 1, // work phases begun
    rounds: 0, // 0: one phase, or no end
    paused: false,
    done: false,
    over: 0, // seconds since the end
    beat: 0, // session seconds: what moves by itself
    turn: null, // the glass turning over, 0 to 1
    ...fields,
  };
}

/** The share of the phase that is gone. */
export function share(s) {
  return Math.min(1, Math.max(0, 1 - s.left / s.length));
}

/** Phases run on a clock given (seconds): `state(now)`. `speed`:
 * seconds of the timer a second of the clock. `loop`: no end. */
export class Run {
  constructor(phases, now = 0, speed = 1, loop = false) {
    this.phases = [...phases];
    this.speed = speed;
    this.loop = loop;
    this.elapsed = 0;
    this.paused = false;
    this.at = now;
  }

  get total() {
    return this.phases.reduce((n, p) => n + p.length, 0);
  }

  advance(now) {
    if (!this.paused) this.elapsed += Math.max(0, now - this.at) * this.speed;
    this.at = now;
  }

  /** Pause, or go on; whether it is paused now. */
  toggle(now) {
    this.advance(now);
    this.paused = !this.paused;
    return this.paused;
  }

  /** Move `seconds` of the timer ahead. */
  skip(now, seconds) {
    this.advance(now);
    this.elapsed += seconds;
  }

  state(now) {
    this.advance(now);
    const works = this.phases.filter((p) => p.name === "work").length;
    const rounds = this.loop || works < 2 ? 0 : works;
    let elapsed = this.elapsed;
    let lap = 0;
    if (this.loop) {
      lap = Math.floor(elapsed / this.total);
      elapsed -= lap * this.total;
    }
    if (elapsed >= this.total) {
      const last = this.phases[this.phases.length - 1];
      return state({
        name: last.name,
        length: last.length,
        left: 0,
        index: this.phases.length - 1,
        round: Math.max(1, works),
        rounds,
        done: true,
        over: elapsed - this.total,
        beat: now,
      });
    }
    let start = 0;
    let index = 0;
    for (; index < this.phases.length; index++) {
      if (elapsed < start + this.phases[index].length) break;
      start += this.phases[index].length;
    }
    const phase = this.phases[index];
    const begun = lap * this.phases.length + index;
    const since = (elapsed - start) / this.speed;
    return state({
      name: phase.name,
      length: phase.length,
      left: start + phase.length - elapsed,
      index: begun,
      round: lap * works + this.phases.slice(0, index + 1).filter((p) => p.name === "work").length,
      rounds,
      paused: this.paused,
      beat: now,
      turn: begun && since < TURN ? since / TURN : null,
    });
  }
}

const pad2 = (n) => String(n).padStart(2, "0");

/** `MM:SS` under 100 minutes, else `H:MM:SS`; a second begun counts. */
export function stamp(left) {
  const s = Math.ceil(Math.max(0, left) - 1e-9);
  if (s < 6000) return `${pad2(Math.floor(s / 60))}:${pad2(s % 60)}`;
  return `${Math.floor(s / 3600)}:${pad2(Math.floor(s / 60) % 60)}:${pad2(s % 60)}`;
}

/** Three cells or so: `45s`, `24m`, `2h`. */
export function short(left) {
  const s = Math.ceil(Math.max(0, left) - 1e-9);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.ceil(s / 60)}m`;
  return `${Math.ceil(s / 360) / 10}h`;
}

// -- the look ----------------------------------------------------------------

/** What the options ask of a face. */
export function look(fields = {}) {
  return { glass: false, colour: false, name: "timer", ...fields };
}

/** The colour of the digits and of the bar: none without `-c`; else by
 * the share left, the break in cyan. */
export function tone(s, l) {
  if (!l.colour) return null;
  if (s.done) return BRIGHT_RED;
  if (s.name === "break") return BRIGHT_CYAN;
  const left = 1 - share(s);
  if (left > 0.5) return BRIGHT_GREEN;
  if (left > 0.2) return BRIGHT_YELLOW;
  return left > 0.1 ? ORANGE : BRIGHT_RED;
}

function run(t, fg = null, style = {}) {
  const out = { t };
  for (const [k, v] of Object.entries(style)) if (v) out[k] = v;
  if (fg !== null) out.fg = fg;
  return out;
}

/** A bar `w` cells long that empties from the right: the share `left`,
 * half a cell exact. */
export function bar(left, w, fg = null) {
  const halves = Math.max(0, round(Math.min(1, Math.max(0, left)) * 2 * w));
  const full = "━".repeat(halves >> 1) + "╸".repeat(halves % 2);
  const out = full ? [run(full, fg, { bold: true })] : [];
  if (full.length < w) out.push(run("─".repeat(w - full.length), null, { dim: true }));
  return out;
}

/** `work 2/4`, `paused`, `done +0:42`. */
export function label(s) {
  if (s.done) {
    const over = s.over >= 1 ? [run(" +" + stamp(s.over).replace(/^0/, ""), null, { dim: true })] : [];
    return [run("done", null, { bold: true }), ...over];
  }
  let out = [run(s.name)];
  if (s.name === "timer") out = [run("of " + stamp(s.length), null, { dim: true })];
  else if (s.rounds) out.push(run(` ${s.round}/${s.rounds}`, null, { dim: true }));
  else out.push(run(` ${s.round}`, null, { dim: true }));
  if (s.paused) out = [run("paused", null, { bold: true }), run(" "), ...out];
  return out;
}

/** The dim half of a blink: the end, and a pause. */
function blink(s) {
  return (s.done || s.paused) && Math.floor(2 * s.beat) % 2 === 1;
}

function stampRow(s, l) {
  return [run(stamp(s.left), tone(s, l), { bold: true, dim: blink(s) })];
}

/** `MM:SS`; in 5 cells an hour and more is `1h24m`. */
function line(s, w, h, l) {
  let text = stamp(s.left);
  if (text.length > w) {
    const minutes = Math.ceil(s.left / 60);
    text = `${Math.floor(minutes / 60)}h${pad2(minutes % 60)}m`;
  }
  return fit([[[run(text, tone(s, l), { bold: true, dim: blink(s) })]]], w, h);
}

/** The time and a bar, one line. */
function lined(s, w, h, l) {
  const text = stampRow(s, l);
  const room = w - width(text) - 1;
  return [[...text, run(" "), ...bar(1 - share(s), room, tone(s, l))]];
}

/** The time and what runs, over a bar. */
function plain(s, w, h, l) {
  const text = stampRow(s, l);
  const what = label(s);
  const gap = Math.max(0, w - width(text) - width(what));
  return [[...text, run(" ".repeat(gap)), ...what], bar(1 - share(s), w, tone(s, l))];
}

/** Block digits: `MM:SS`, or `HH:MM` from 100 minutes. */
function digits(s, font, l) {
  let text = stamp(s.left);
  if (text.length > 5) {
    const minutes = Math.ceil(s.left / 60);
    text = `${pad2(Math.floor(minutes / 60))}:${pad2(minutes % 60)}`;
  }
  const fg = tone(s, l) ?? WHITE;
  const colon = Math.floor(s.beat) % 2 && !s.paused ? DIM : fg;
  const colours = blink(s) ? [DIM, DIM, DIM, DIM, DIM] : [fg, fg, colon, fg, fg];
  return digitRows(text, font, colours);
}

/** A bar and the label, under block digits. */
function under(s, w, l) {
  const what = label(s);
  const pad = Math.max(0, (w - width(what)) >> 1);
  return [bar(1 - share(s), w, tone(s, l)), [run(" ".repeat(pad)), ...what]];
}

/** Block digits; `more`: a bar and the label under them. */
function blocks(s, w, h, l, font, more) {
  const rows = digits(s, font, l);
  const wide = Math.max(...rows.map(width));
  return fit(more ? [rows, [[]], under(s, wide, l)] : [rows], w, h);
}

// -- the hourglass -----------------------------------------------------------

// Braille: the bit of each dot of a cell, by row and column.
const BRAILLE = [
  [0x01, 0x08],
  [0x02, 0x10],
  [0x04, 0x20],
  [0x40, 0x80],
];

/** Dot rows a dot across: the dip of the sand that leaves, the side of
 * the heap. */
export const SLOPE = 0.6;

/** A fixed share of a dot row, by dot: the surface is not a line. */
function grain(x, y) {
  return ((((x * 73856093) ^ (y * 19349663)) % 89) / 89) * 0.9;
}

const key = (x, y) => y * 1024 + x;
const bulbCache = new Map();

/** The dots `[x, y]` of a glass of `k` rows a bulb, `2k + 1` cells by
 * `2k + 2`: those of the upper bulb in the order they leave, those of
 * the lower one in the order they are taken. A cell is 2 dots by 4; the
 * plates are the first and last rows of cells, a wall takes a cell. */
export function bulbs(k) {
  if (bulbCache.has(k)) return bulbCache.get(k);
  const cx = 2 * k + 0.5;
  const dots = (row, first, last) => {
    const out = [];
    for (let y = 4 * row; y < 4 * row + 4; y++) for (let x = 2 * first; x < 2 * last + 2; x++) out.push([x, y]);
    return out;
  };
  let upper = [];
  for (let i = 0; i < k; i++) upper.push(...dots(1 + i, i + 1, 2 * k - 1 - i));
  const lower = [];
  for (let j = 0; j < k; j++) lower.push(...dots(k + 1 + j, k - j, k + j));
  if (k > 2) upper = upper.filter(([, y]) => y > 4); // the sand does not touch the plate
  const floor = 4 * (2 * k + 1) - 1;
  const by = (f) => (a, b) => f(a) - f(b);
  upper.sort(by(([x, y]) => y + SLOPE * Math.abs(x - cx) + grain(x, y)));
  lower.sort(by(([x, y]) => floor - y + SLOPE * Math.abs(x - cx) + grain(x, y)));
  const out = [upper, lower];
  bulbCache.set(k, out);
  return out;
}

/** The grains of a glass. */
export function grains(k) {
  return bulbs(k)[0].length;
}

/** The grains fallen when `part` of the time is gone: one every
 * `1 / grains(k)`, the last as it ends. */
export function fallen(k, part) {
  const n = grains(k);
  return part >= 1 ? n : Math.min(n - 1, Math.floor(n * Math.max(0, part)));
}

/** The sand of a glass after `n` grains fell: the dots of the upper
 * bulb, those of the lower. */
export function sand(k, n) {
  const [upper, lower] = bulbs(k);
  return [upper.slice(n), lower.slice(0, n)];
}

/** Grains in the air: one dot in three down the middle, from the neck
 * to the heap, a dot lower every tenth of a second. */
function stream(k, lower, beat) {
  const [x, top, floor] = [2 * k, 4 * (k + 1), 4 * (2 * k + 1)];
  const heap = new Set(lower.map(([u, v]) => key(u, v)));
  let end = floor;
  for (let y = top; y < floor; y++) {
    if (heap.has(key(x, y))) {
      end = y;
      break;
    }
  }
  const out = [];
  const step = Math.floor(10 * beat);
  for (let y = top; y < end; y++) if ((((y - step) % 3) + 3) % 3 === 0) out.push([x, y]);
  return out;
}

/** A grid of `[glyph, rank]`: Braille from `dots` (a map of `key` to a
 * rank; a cell has that of its highest dot), `glyphs` (a map of `key`
 * of a cell to `[glyph, rank]`) over it. */
function cellsOf(w, h, dots, glyphs) {
  const bits = new Map();
  for (const [k, rank] of dots) {
    const x = k % 1024;
    const y = Math.floor(k / 1024);
    if (x < 0 || x >= 2 * w || y < 0 || y >= 4 * h) continue;
    const c = key(x >> 1, y >> 2);
    const cell = bits.get(c) ?? [0, 0];
    cell[0] |= BRAILLE[y % 4][x % 2];
    cell[1] = Math.max(cell[1], rank);
    bits.set(c, cell);
  }
  const grid = [];
  for (let y = 0; y < h; y++) {
    const row = [];
    for (let x = 0; x < w; x++) {
      const c = key(x, y);
      if (glyphs.has(c)) row.push(glyphs.get(c));
      else if (bits.has(c)) row.push([String.fromCodePoint(0x2800 + bits.get(c)[0]), bits.get(c)[1]]);
      else row.push([" ", 0]);
    }
    grid.push(row);
  }
  return grid;
}

// Ranks of a cell of a glass, and their styles: plain, with `-c`.
const [NONE, WALL, PLATE, SANDY, FALLING] = [0, 1, 2, 3, 4];
const PLAIN = [{}, { dim: true }, {}, { fg: YELLOW }, { fg: BRIGHT_YELLOW }];
const FANCY = [{}, { fg: GLASS }, { fg: WOOD }, { fg: SAND }, { fg: FALL }];

/** Runs of cells of one rank. `last`: the sand is nearly out, red with
 * `-c`. */
function rowsOf(grid, l, last = false) {
  const styles = [...(l.colour ? FANCY : PLAIN)];
  if (last && l.colour) styles[SANDY] = { fg: 209 };
  return grid.map((cells) => {
    const row = [];
    let rank = -1;
    for (const [glyph, r] of cells) {
      if (r === rank) row[row.length - 1].t += glyph;
      else {
        row.push({ t: glyph, ...styles[r] });
        rank = r;
      }
    }
    return row;
  });
}

/** An hourglass of `k` rows a bulb, `2k + 1` cells by `2k + 2`: the
 * sand of the state, a stream while it runs; turning while `s.turn`. */
export function glass(k, s, l) {
  if (s.turn !== null) return spin(k, s.turn, l);
  const [w, h] = [2 * k + 1, 2 * k + 2];
  const n = fallen(k, share(s));
  const [upper, lower] = sand(k, n);
  const dots = new Map();
  for (const [x, y] of [...upper, ...lower]) dots.set(key(x, y), SANDY);
  if (n > 0 && n < grains(k) && !s.paused) for (const [x, y] of stream(k, lower, s.beat)) dots.set(key(x, y), FALLING);
  const glyphs = new Map();
  for (let x = 0; x < w; x++) {
    glyphs.set(key(x, 0), ["▄", PLATE]);
    glyphs.set(key(x, h - 1), ["▀", PLATE]);
  }
  for (let i = 0; i < k; i++) {
    glyphs.set(key(i, 1 + i), ["╲", WALL]);
    glyphs.set(key(k + 1 + i, k + 1 + i), ["╲", WALL]);
    glyphs.set(key(2 * k - i, 1 + i), ["╱", WALL]);
    glyphs.set(key(k - 1 - i, k + 1 + i), ["╱", WALL]);
  }
  return rowsOf(cellsOf(w, h, dots, glyphs), l, share(s) > 0.9 && !s.done);
}

/** A segment from `a` to `b`, and the box `reach` around it. */
function segment([ax, ay], [bx, by], reach) {
  const dx = bx - ax;
  const dy = by - ay;
  const r = reach + 1e-6;
  return { ax, ay, dx, dy, d2: dx * dx + dy * dy, reach, x0: Math.min(ax, bx) - r, x1: Math.max(ax, bx) + r, y0: Math.min(ay, by) - r, y1: Math.max(ay, by) + r };
}

/** `(px, py)` within the reach of segment `s`. */
function near(px, py, s) {
  if (px < s.x0 || px > s.x1 || py < s.y0 || py > s.y1) return false;
  const t = Math.min(1, Math.max(0, ((px - s.ax) * s.dx + (py - s.ay) * s.dy) / s.d2));
  return Math.hypot(px - s.ax - t * s.dx, py - s.ay - t * s.dy) <= s.reach;
}

const spins = new Map(); // the last turn of each glass and look

/** The glass turning over, all in Braille: half a turn as `turn` goes
 * from 0 to 1, the sand of the end in its bulb; made smaller while it
 * does not fit the box. */
export function spin(k, turn, l) {
  const kept = spins.get(`${k}:${l.colour}`);
  if (kept && kept[0] === turn) return kept[1];
  const [w, h] = [2 * k + 1, 2 * k + 2];
  const t = Math.min(1, Math.max(0, turn));
  const angle = Math.PI * t * t * (3 - 2 * t);
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const hw = w;
  const hh = 2 * h;
  const scale = Math.min(1, hw / (Math.abs(hw * cos) + Math.abs(hh * sin)), hh / (Math.abs(hw * sin) + Math.abs(hh * cos)));
  const cx = hw - 0.5;
  const cy = hh - 0.5;
  const heap = new Set(sand(k, grains(k))[1].map(([x, y]) => key(x, y)));
  const [right, low, mid] = [2 * w - 1, 4 * h - 4, 4 * (k + 1)];
  const reach = 0.5 / scale;
  const plates = [
    [[0, 3], [right, 3]],
    [[0, low], [right, low]],
  ].map(([a, b]) => segment(a, b, reach));
  const walls = [
    [[0, 4], [2 * k - 1, mid]],
    [[2 * k - 1, mid], [0, low - 1]],
    [[right, 4], [2 * k + 2, mid]],
    [[2 * k + 2, mid], [right, low - 1]],
  ].map(([a, b]) => segment(a, b, reach));
  const dots = new Map();
  for (let y = 0; y < 4 * h; y++) {
    for (let x = 0; x < 2 * w; x++) {
      const dx = (x - cx) / scale;
      const dy = (y - cy) / scale;
      const u = cx + dx * cos + dy * sin;
      const v = cy - dx * sin + dy * cos;
      const ru = round(u);
      const rv = round(v);
      if (ru >= 0 && rv >= 0 && heap.has(key(ru, rv))) dots.set(key(x, y), SANDY);
      else if (near(u, v, plates[0]) || near(u, v, plates[1])) dots.set(key(x, y), PLATE);
      else if (walls.some((s) => near(u, v, s))) dots.set(key(x, y), WALL);
    }
  }
  const out = rowsOf(cellsOf(w, h, dots, new Map()), l);
  spins.set(`${k}:${l.colour}`, [turn, out]);
  return out;
}

/** `text` centred on row `y` of `rows`, a blank on each side, over what
 * is there. */
function over(rows, w, y, text) {
  const wide = width(text) + 2;
  if (wide > w || y < 0 || y >= rows.length) return rows;
  const start = (w - wide) >> 1;
  const cells = [];
  for (const r of rows[y]) for (const c of r.t) cells.push({ ...r, t: c });
  while (cells.length < w) cells.push({ t: " " });
  const middle = [{ t: " " }, ...text, { t: " " }];
  return [...rows.slice(0, y), [...cells.slice(0, start), ...middle, ...cells.slice(start + wide)], ...rows.slice(y + 1)];
}

/** The glass alone: the time is the title of the widget. Under the
 * pointer, the time in digits over the neck. */
function alone(s, w, h, l, hover, k) {
  const rows = glass(k, s, l);
  if (!hover) return rows;
  const text = w >= 7 ? stamp(s.left) : short(s.left);
  return over(rows, w, k, [run(text, tone(s, l), { bold: true })]);
}

/** The glass, and block digits, a bar and the label next to it. */
function beside(s, w, h, l, k, font) {
  const rows = digits(s, font, l);
  const wide = Math.max(...rows.map(width));
  let right = [...rows, [], ...under(s, wide, l)];
  const left = glass(k, s, l);
  const above = (left.length - right.length) >> 1;
  right = [...Array.from({ length: above }, () => []), ...right];
  while (right.length < left.length) right.push([]);
  return left.map((a, i) => (right[i].length ? [...a, run("   "), ...right[i]] : a));
}

/** The sizes of a glass alone, and its rows a bulb. */
export const GLASSES = new Map([2, 3, 4, 5, 6].map((k) => [`${2 * k + 1}x${2 * k + 2}`, k]));

/** The faces of a look, `[[w, h], draw(s, w, h, l, hover)]`, smallest
 * first. */
export function ladder(l) {
  const faces = [
    [[5, 1], line],
    [[12, 1], lined],
    [[20, 2], plain],
  ];
  if (!l.glass) {
    faces.push(
      [[17, 3], (s, w, h, l) => blocks(s, w, h, l, SMALL, false)],
      [[17, 6], (s, w, h, l) => blocks(s, w, h, l, SMALL, true)],
      [[32, 5], (s, w, h, l) => blocks(s, w, h, l, LARGE, false)],
      [[32, 8], (s, w, h, l) => blocks(s, w, h, l, LARGE, true)],
    );
  } else {
    for (const [size, k] of GLASSES) {
      const [w, h] = size.split("x").map(Number);
      faces.push([[w, h], (s, w, h, l, hover) => alone(s, w, h, l, hover, k)]);
    }
    faces.push(
      [[27, 8], (s, w, h, l) => beside(s, w, h, l, 3, SMALL)],
      [[29, 10], (s, w, h, l) => beside(s, w, h, l, 4, SMALL)],
      [[46, 12], (s, w, h, l) => beside(s, w, h, l, 5, LARGE)],
    );
  }
  const area = ([[w, h]]) => w * h;
  return faces.sort((a, b) => area(a) - area(b) || a[0][0] - b[0][0] || a[0][1] - b[0][1]);
}

/** The face of size `w` by `h` of a look. `hover`: the pointer is on
 * the widget. */
export function face(s, w, h, l, hover = false) {
  const found = ladder(l).find(([[fw, fh]]) => fw === w && fh === h);
  if (!found) throw new RangeError(`timer: no face of ${w}x${h}`);
  return found[1](s, w, h, l, hover);
}

/** Whether the face of `size` moves by itself: a stream, a turn. */
export function moves(size, s, l) {
  const wide = size !== null && (GLASSES.has(`${size[0]}x${size[1]}`) || size[0] > 20);
  return l.glass && wide && !s.done && !s.paused;
}

/** The title of the widget: its name; the time left on a glass alone,
 * the only place for it. */
export function title(s, size, l) {
  if (!l.glass || size === null || !GLASSES.has(`${size[0]}x${size[1]}`)) {
    return size === null || l.name.length <= size[0] - 2 ? l.name : "";
  }
  if (s.done) return "done";
  return size[0] >= 9 ? stamp(s.left) : short(s.left);
}

/** Session ms to the next drawing: when a second of the timer ends;
 * sooner while a glass runs; a blink on a pause and at the end. */
export function wait(s, size, l, speed = 1) {
  let out = ((s.left % 1 || 1) / speed + 0.01) * 1000;
  if (s.done || s.paused) out = 500;
  else if (s.turn !== null && moves(size, s, l)) out = 80;
  else if (moves(size, s, l)) out = Math.min(out, STREAM * 1000);
  return Math.max(50, out);
}

// -- options -----------------------------------------------------------------

/** The options of `init`; `day`: the seconds since midnight of the wall
 * clock, for `--until`. Throws a `RangeError`, its message a line for
 * the log. */
export function options(args, day = 0) {
  const given = Array.isArray(args.duration) ? (args.duration[0] ?? null) : (args.duration ?? null);
  const set = [given, args.until ?? null, args.pomodoro ?? null].filter((v) => v !== null);
  if (set.length > 1) throw new RangeError("timer: one of DURATION, --until, --pomodoro");
  const rounds = args.rounds ?? 4;
  if (rounds < 0) throw new RangeError("timer: --rounds: 0 or more");
  const hold = args.hold ?? null;
  if (hold !== null && !(hold > 0 && hold < Infinity)) throw new RangeError(`timer: --hold: not a positive number: ${hold}`);
  const all = args.all === true;
  const flag = (k) => all || args[k] === true;
  const wrap = (f) => {
    try {
      return f();
    } catch (e) {
      if (e instanceof RangeError) throw new RangeError(`timer: ${e.message}`);
      throw e;
    }
  };
  let phases;
  let loop = false;
  if (args.pomodoro != null) {
    const [work, rest] = wrap(() => pomodoro(args.pomodoro));
    phases = plan(null, work, rest, rounds);
    loop = rounds === 0;
  } else if (args.until != null) {
    phases = plan(wrap(() => until(args.until, day)));
  } else {
    phases = plan(wrap(() => duration(given ?? "25m")));
  }
  return {
    phases,
    loop,
    look: look({ glass: flag("glass"), colour: flag("colour"), name: args.name ?? "timer" }),
    ring: flag("ring"),
    burst: flag("burst"),
    modal: args.modal === true,
    click: args.click === true,
    hold,
  };
}

// -- live --------------------------------------------------------------------

let o;
let runner;
let deck;
let widget;
let sizes;
let hovered = false;
let before;
let ring = null; // session s: the border stops ringing
let leave = null; // session s: the app ends, `--hold`
let named = null; // the title sent

const seconds = () => Date.now() / 1000;

/** The widget shown again: in `border` (`alert`), or as declared; a
 * modal with `modal`. */
function shown(border = null, modal = false) {
  const { border: _b, modal: _m, ...rest } = widget.options;
  widget.options = { ...rest, ...(border ? { border } : {}), ...(modal ? { modal } : {}) };
  deck.resize(widget, sizes);
  named = null;
}

/** An explosion of `count` pieces of confetti on the centre of the
 * widget: its rectangle is in cells of the frame's view (below `layout`,
 * frame 0). */
function burst(count) {
  const rect = widget.rect;
  if (rect === null) return;
  const origin = [rect[0] + (rect[2] >> 1), rect[1] + (rect[3] >> 1)];
  const params = { count, duration_ms: 3000, seed: Math.floor(Math.random() * 2 ** 32), origin };
  try {
    call("draw.put", { effect: { name: "confetti", params }, z: 100 });
  } catch (e) {
    if (!(e instanceof PtyrcError) || e.code !== "not-usable") throw e;
  }
}

function retitle(s) {
  const name = title(s, widget.size, o.look);
  if (widget.size === null || name === named) return;
  widget.set(null, { title: name });
  named = name;
}

export function init(args) {
  try {
    let day = 0;
    if (args.until != null) {
      const t = wall();
      day = ((((t.unixMs + 1000 * t.offsetS) % 86400000) + 86400000) % 86400000) / 1000;
    }
    o = options(args, day);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  runner = new Run(o.phases, seconds(), 1, o.loop);
  before = runner.state(seconds());
  const faces = ladder(o.look);
  sizes = faces.map(([size]) => size);
  const draws = new Map(faces.map(([[w, h], draw]) => [`${w}x${h}`, draw]));
  const render = (w, h) => draws.get(`${w}x${h}`)(runner.state(seconds()), w, h, o.look, hovered);
  // While the pointer is there: a mouse at rest sends nothing.
  deck = new Deck({ border: "round", hoverTimeout: o.look.glass ? null : 1500 });
  const handlers = {};
  if (o.look.glass) {
    handlers.hover = () => {
      const was = hovered;
      hovered = true;
      return !was;
    };
    handlers.leave = () => {
      const was = hovered;
      hovered = false;
      return was;
    };
  }
  if (o.click) {
    handlers.click = (x, y, button) => {
      if (button.startsWith("wheel")) return false;
      runner.toggle(seconds());
    };
  }
  widget = deck.add("timer", sizes, render, { title: o.look.name, ...handlers });
  period(wait(before, widget.size, o.look));
}

export function tick(ms) {
  const at = ms / 1000;
  if (leave !== null && at >= leave) exit(0);
  const s = runner.state(at);
  if (s.done && !before.done) {
    if (o.ring || o.modal) {
      shown(o.ring ? "alert" : null, o.modal);
      ring = null;
    }
    if (o.burst) burst(96);
    if (o.hold !== null) leave = at + o.hold;
  } else if (s.index !== before.index) {
    if (o.ring) {
      shown("alert");
      ring = at + RING;
    }
    if (o.burst) burst(32);
  }
  if (ring !== null && at >= ring) {
    shown();
    ring = null;
  }
  before = s;
  deck.tick(ms);
  retitle(s);
  const due = [wait(s, widget.size, o.look)];
  for (const d of [ring, leave]) if (d !== null) due.push(Math.max(33, (d - at) * 1000));
  period(Math.max(33, Math.min(...due)));
}

export function event(topic, data) {
  deck.event(topic, data);
  if (topic === "widgets" || topic === "move") retitle(runner.state(seconds()));
}
