// SPDX-License-Identifier: GPL-3.0-or-later
// Clock: one widget, faces from 5x1 to 41x8.
//
// A variant per face, the same time laid out for each box, from `HH:MM`
// to block digits with the date, and a dial; the driver picks the face
// and the place, in a quiet room. Every face is drawn at each tick (the
// deck's `every`): a face the driver changes to is never late. A tick on
// the wall second (`--no-seconds`: the minute), `until`.
//
// The time: the wall clock (`now`, permission `clock`) at the machine's
// offset; `--utc`: at 0; `--tz ZONE`: at that zone's, asked again when it
// changes (`next`). Without `clock`: session time as the epoch, UTC.

import { civil, exit, log, now, period, until, zone } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Braille } from "ptyrc/canvas";
import { LARGE, SMALL, beside, blocks, fit } from "./digits.js";

export const HOURS = 14; // bright cyan
export const MINUTES = 13; // bright magenta
export const SECONDS = 11; // bright yellow
export const DIM = 8; // bright black
const PLAIN = 7; // white
const DAY = 6; // cyan

export const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
export const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const two = (n) => String(n).padStart(2, "0");

/** The ISO week of the day of `t` (`civil`). */
export function week(t) {
  const day = Date.UTC(t.year, t.month - 1, t.day);
  const thursday = day + (3 - ((t.weekday + 6) % 7)) * 86400000;
  const year = new Date(thursday).getUTCFullYear();
  return 1 + Math.floor((thursday - Date.UTC(year, 0, 1)) / (7 * 86400000));
}

function time(t, seconds) {
  const colon = { t: ":", fg: DIM };
  const out = [{ t: two(t.hour), fg: HOURS, bold: true }, colon, { t: two(t.minute), fg: MINUTES, bold: true }];
  if (seconds) out.push(colon, { t: two(t.second), fg: SECONDS, bold: true });
  return out;
}

function date(t, full) {
  const day = DAYS[t.weekday];
  const month = MONTHS[t.month - 1];
  if (!full) return [{ t: `${day.slice(0, 3)} ${t.day} ${month.slice(0, 3)}` }];
  return [{ t: day, fg: DAY }, { t: ` ${t.day} ${month} ${t.year}` }];
}

/** `HH:MM` or `HH:MM:SS`. */
export function line(t, w, h, seconds) {
  return fit([[time(t, seconds)]], w, h);
}

/** The time over a short date. */
export function dated(t, w, h, seconds) {
  return fit([[time(t, seconds)], [date(t, false)]], w, h);
}

/** `HH:MM` in block digits of `font`, the colon dim on odd seconds when
 * `blink`; the short date under it when `withDate`. */
export function block(t, w, h, font, blink, withDate) {
  const colon = blink && t.second % 2 ? DIM : PLAIN;
  const digits = blocks(`${two(t.hour)}:${two(t.minute)}`, font, [HOURS, HOURS, colon, MINUTES, MINUTES]);
  return fit(withDate ? [digits, [[]], [date(t, false)]] : [digits], w, h);
}

/** Large `HH:MM`, small seconds after it; weekday, date, week. */
export function grand(t, w, h, seconds) {
  const colon = seconds && t.second % 2 ? DIM : PLAIN;
  let digits = blocks(`${two(t.hour)}:${two(t.minute)}`, LARGE, [HOURS, HOURS, colon, MINUTES, MINUTES]);
  if (seconds) digits = beside(digits, blocks(two(t.second), SMALL, [SECONDS, SECONDS]), 2);
  return fit([digits, [[]], [date(t, true)], [[{ t: `week ${week(t)}`, fg: DIM }]]], w, h);
}

/** `v` on the middle of its dot, the centre between two: the dots of
 * `v` and `-v` mirror each other. 0 goes right, and down. */
function snap(v) {
  const k = Math.floor(Math.abs(v)) + 0.5;
  return v < -1e-9 ? -k : k;
}

/** The dots of a ray of `rank` from the centre of `w` by `h` cells,
 * `start` to `end` dots away, at `turn` (0 up, 0.25 right) into
 * `dots`: a map from `y * 2w + x` to the highest rank. */
function ray(dots, w, h, turn, start, end, rank) {
  const cx = w - 0.5; // the centre, in dots: between four
  const cy = 2 * h - 0.5;
  const n = Math.max(1, Math.trunc(4 * (end - start)));
  for (let i = 0; i <= n; i++) {
    const d = start + ((end - start) * i) / n;
    const x = Math.trunc(cx + snap(d * Math.sin(2 * Math.PI * turn)));
    const y = Math.trunc(cy + snap(-d * Math.cos(2 * Math.PI * turn)));
    if (x >= 0 && x < 2 * w && y >= 0 && y < 4 * h) {
      const k = y * 2 * w + x;
      dots.set(k, Math.max(dots.get(k) ?? 0, rank));
    }
  }
}

const rims = new Map();

/** The rim and the hour marks: what no hand moves. */
function rim(w, h) {
  const key = `${w}x${h}`;
  if (rims.has(key)) return rims.get(key);
  const r = Math.min(w - 0.5, 2 * h - 0.5);
  const dots = new Map();
  for (let i = 0; i < 400; i++) ray(dots, w, h, i / 400, r, r, 1);
  for (let hour = 0; hour < 12; hour++) {
    // Either side of the turn: a mark on an axis is two dots thick.
    for (const turn of [hour / 12 - 1e-6, hour / 12 + 1e-6]) {
      ray(dots, w, h, turn, r - (hour % 3 === 0 ? 4 : 2.5), r - 2.5, 1);
    }
  }
  rims.set(key, dots);
  return dots;
}

/** A round dial in Braille, 17 cells by 8: rim and hour marks dim, the
 * hour hand, the minute hand, the second a tick inside the rim. The
 * left half mirrors the right, the top the bottom. A cell has one
 * colour: that of its foremost dot. */
export function dial(t, w, h, seconds) {
  const [dw, dh] = [17, 8];
  const r = Math.min(dw - 0.5, 2 * dh - 0.5);
  const dots = new Map(rim(dw, dh));
  const minute = t.minute + (seconds ? t.second / 60 : 0);
  ray(dots, dw, dh, ((t.hour % 12) + minute / 60) / 12, 0, 0.5 * r, 3);
  ray(dots, dw, dh, minute / 60, 0, 0.8 * r, 2);
  if (seconds) ray(dots, dw, dh, t.second / 60, r - 1, r, 4);
  const braille = new Braille(dw, dh);
  const ranks = new Map(); // cell `cy * dw + cx`: the highest rank
  for (const [k, rank] of dots) {
    const x = k % (2 * dw);
    const y = Math.floor(k / (2 * dw));
    braille.set(x, y);
    const c = (y >> 2) * dw + (x >> 1);
    ranks.set(c, Math.max(ranks.get(c) ?? 0, rank));
  }
  const colors = [DIM, MINUTES, HOURS, SECONDS];
  const rows = [];
  for (let cy = 0; cy < dh; cy++) {
    const row = [];
    for (let cx = 0; cx < dw; cx++) {
      const g = braille.glyph(cx, cy);
      row.push(g ? { t: g, fg: colors[ranks.get(cy * dw + cx) - 1] } : { t: " " });
    }
    rows.push(row);
  }
  return fit([rows], w, h);
}

/** The faces of a mode, `[[w, h], draw(t, w, h)]`, smallest first.
 * `seconds`: with them, else without; `big`: block digits only. */
export function ladder(seconds = true, big = false) {
  const faces = [
    [[5, 1], (t, w, h) => line(t, w, h, false), false],
    [[10, 2], (t, w, h) => dated(t, w, h, seconds), false],
    [[17, 3], (t, w, h) => block(t, w, h, SMALL, seconds, false), true],
    [[17, 5], (t, w, h) => block(t, w, h, SMALL, seconds, true), true],
    [[17, 8], (t, w, h) => dial(t, w, h, seconds), false],
    [[32, 5], (t, w, h) => block(t, w, h, LARGE, seconds, false), true],
    [[32, 7], (t, w, h) => block(t, w, h, LARGE, seconds, true), true],
    [[seconds ? 41 : 32, 8], (t, w, h) => grand(t, w, h, seconds), true],
  ];
  if (seconds) faces.splice(1, 0, [[8, 1], (t, w, h) => line(t, w, h, true), false]);
  return faces.filter(([, , digits]) => digits || !big).map(([size, draw]) => [size, draw]);
}

/** The face of size `w` by `h` of a mode, as `ladder`. */
export function face(t, w, h, seconds = true, big = false) {
  const found = ladder(seconds, big).find(([[fw, fh]]) => fw === w && fh === h);
  if (!found) throw new RangeError(`clock: no face of ${w}x${h}`);
  return found[1](t, w, h);
}

/** The options of `init`; `host.zone` (as `zone` of `"ptyrc"`), when
 * given, asked for `--tz`. Throws a `RangeError`, its message a line for
 * the log, for `--utc` with `--tz` or a zone unknown. */
export function options(args, host = null) {
  const tz = typeof args.tz === "string" && args.tz !== "" ? args.tz : null;
  const utc = args.utc === true;
  if (utc && tz !== null) throw new RangeError("clock: --utc or --tz, not both");
  if (tz !== null && host !== null) {
    try {
      host.zone(tz);
    } catch (e) {
      if (e?.code === "bad-params") throw new RangeError(`clock: unknown time zone: ${tz}`);
      throw e;
    }
  }
  return { utc, tz, seconds: args.seconds !== false, big: args.big === true };
}

/** The wall time to show, `{unixMs, offsetS}`. `host`: `now()` and
 * `zone(name, at)` as those of `"ptyrc"`; the zone is asked once, and
 * again at its `next`. */
export class Wall {
  constructor(o, host) {
    this.o = o;
    this.host = host;
    this.offset = null;
    this.next = null;
  }

  read() {
    const { unixMs, offsetS } = this.host.now();
    if (this.o.utc) return { unixMs, offsetS: 0 };
    if (this.o.tz === null) return { unixMs, offsetS };
    if (this.offset === null || (this.next !== null && unixMs >= this.next)) {
      const [at] = this.host.zone(this.o.tz, unixMs).offsets;
      this.offset = at.offsetS;
      this.next = at.next;
    }
    return { unixMs, offsetS: this.offset };
  }
}

let deck;
let wall;
let every; // ms between two ticks of the wall clock
let shown; // the wall time of the last tick

export function init(args) {
  let o;
  try {
    o = options(args, { now, zone });
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  wall = new Wall(o, { now, zone });
  every = o.seconds ? 1000 : 60000;
  shown = wall.read();
  const faces = ladder(o.seconds, o.big);
  const draws = new Map(faces.map(([[w, h], draw]) => [`${w}x${h}`, draw]));
  const render = (w, h) => draws.get(`${w}x${h}`)(civil(shown.unixMs, shown.offsetS), w, h);
  deck = new Deck({ border: "round", every: true });
  deck.add("clock", faces.map(([size]) => size), render);
  period(until(shown.unixMs, every));
}

export function tick(ms) {
  shown = wall.read();
  deck.tick(ms);
  period(until(shown.unixMs, every));
}

export function event(topic, data) {
  deck.event(topic, data);
}
