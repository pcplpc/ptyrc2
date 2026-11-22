// SPDX-License-Identifier: GPL-3.0-or-later
// Snow: it falls; with `--cover` it settles on the lines of text and
// melts where they change.
//
// The effect `snow` of the driver plays it: flakes pass behind the
// text; with `--cover` snow lies over a line of text, a cell away from
// the words of its row, and on the floor, `--max-depth` rows deep (3;
// 0: no limit); it melts where the screen changed lately and around
// the cursor. `--hover`: the driver tells the effect the pointer
// (`"pointer": true`); the snow under it is taken. Every 6 to 15 s the
// app tunes a new wind in place; the snow stays.

import { call, exit, log, period, random } from "ptyrc";

/** Ms between two winds. */
export const GUST = [6000, 15000];
/** Rows a pile holds. */
export const DEEP = 3;
/** The wind the effect takes, either way. */
const MAX_WIND = 20;

function rnd() {
  const b = random(4);
  return ((b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0) / 4294967296;
}

/** The options of `init`, `--all` spread. */
export function options(args) {
  const all = args.all === true;
  const depth = args["max-depth"];
  return {
    rate: args.rate,
    wind: args.wind,
    cover: all || args.cover === true,
    hover: all || args.hover === true,
    maxDepth: Number.isInteger(depth) ? depth : all ? 0 : DEEP,
  };
}

/** The params of the effect. */
export function params(o, seed) {
  const out = { rate: o.rate, wind: o.wind, cover: o.cover, max_depth: o.maxDepth, seed };
  if (o.hover) out.pointer = true;
  return out;
}

/** Another wind: the same way most of the time, up to twice as strong. */
export function gust(wind, rnd) {
  const way = rnd() < 0.8 ? 1 : -0.5;
  const w = Math.round(way * wind * (0.2 + 1.8 * rnd()) * 100) / 100;
  return Math.max(-MAX_WIND, Math.min(MAX_WIND, w));
}

/** The winds: `next()` a wait in ms, then at each call another wind
 * told to `tune` before the wait. */
export class Gusts {
  constructor(wind, rnd, tune) {
    this.wind = wind;
    this.rnd = rnd;
    this.tune = tune;
    this.started = false;
  }

  next() {
    if (this.started) this.tune(gust(this.wind, this.rnd));
    this.started = true;
    const [lo, hi] = GUST;
    return Math.round(lo + (hi - lo) * this.rnd());
  }
}

let gusts = null;

export function init(args) {
  const o = options(args);
  let id;
  try {
    id = call("draw.put", { effect: { name: "snow", params: params(o, Math.floor(rnd() * 4294967296)) } }).id;
  } catch (e) {
    log(`snow: the effect: ${e.code}: ${e.message}`);
    exit(1);
  }
  if (o.wind === 0) return;
  const tune = (wind) => call("draw.update", { id, effect: { name: "snow", params: { wind } } });
  gusts = new Gusts(o.wind, rnd, tune);
  period(gusts.next());
}

export function tick() {
  period(gusts.next());
}
