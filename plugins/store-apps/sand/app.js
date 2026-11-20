// SPDX-License-Identifier: GPL-3.0-or-later
// Sand: a thin stream piles on the text of an idle pane.
//
// The effect `sand` of the driver runs the simulation and reads the
// text under it as its walls; the app starts it once the pane is idle
// (`frame.idle`, topic `idle`; without `shape` the pane is idle at
// once), then only relays the pointer to it: `--hover` the cell under
// it, `--bomb` a press released on the pile. Both are told on a pad, a
// sprite of blanks the size of the view under the effect.

import { call, exit, log, period, random } from "ptyrc";

const MAX_RATE = 1000;
const MAX_HOLD = 1500; // ms: the longest press that still grows a bomb
const LOOK = 500; // ms between two looks at a pane not idle yet

let opts;
let sand = null; // the effect's sprite
let pad = null; // the blanks that take the pointer
let size = [80, 24];
let at = null; // the cell last told as hovered
let down = null; // a press: {at, since, start, last}

function u32() {
  const b = random(4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

/** The options of `init`, `--all` spread. */
export function options(args) {
  const all = args.all === true;
  let rate = 60;
  if (Number.isFinite(args.rate) && args.rate > 0) rate = Math.min(args.rate, MAX_RATE);
  return {
    rate,
    cursor: all || args.cursor === true,
    hover: args.hover === true,
    bomb: all || args.bomb === true,
    now: all || args.now === true,
  };
}

/** The `bomb` of a press at `at`, held `ms`, from screen cell `start`
 * to `last`: `[x, y, ms]`, or `[x, y, ms, dx, dy]` when aimed. */
export function bomb(at, ms, start, last) {
  const b = [at[0], at[1], Math.min(Math.max(0, ms), MAX_HOLD)];
  if (start && last && (last[0] !== start[0] || last[1] !== start[1])) {
    b.push(last[0] - start[0], last[1] - start[1]);
  }
  return b;
}

function blank() {
  return Array(size[1]).fill(" ".repeat(size[0]));
}

function tune(params) {
  call("draw.update", { id: sand, effect: { name: "sand", params } });
}

function pour() {
  const params = { rate: opts.rate, cursor: opts.cursor, seed: u32() };
  try {
    sand = call("draw.put", { effect: { name: "sand", params } }).id;
  } catch (e) {
    log(`sand: the effect: ${e.code}: ${e.message}`);
    exit(1);
  }
  if (!opts.hover && !opts.bomb) return;
  const anchor = { at: [0, 0], space: "view" };
  pad = call("draw.put", { content: { rows: blank() }, anchor, z: -1 }).id;
  const topics = ["size"];
  if (opts.hover) topics.push("hover");
  if (opts.bomb) topics.push("mouse");
  call("subscribe", { topics });
}

export function init(args) {
  opts = options(args);
  try {
    size = call("session.info").size;
  } catch {}
  if (opts.now) return pour();
  // The event, and a look at each tick: a host may tell no `idle`.
  call("subscribe", { topics: ["idle"] });
  if (call("frame.idle", {}).idle) pour();
  else period(LOOK);
}

export function tick() {
  if (sand !== null) period(0);
  else if (call("frame.idle", {}).idle) pour();
}

function hover(data) {
  const here = data.sprite === pad ? data.at : null;
  if (JSON.stringify(here) === JSON.stringify(at)) return;
  at = here;
  tune({ hover: at });
}

function mouse(data) {
  if (data.sprite !== pad && down === null) return;
  const here = [data.x, data.y];
  const now = Date.now();
  if (data.action === "press" && data.sprite === pad && data.at) {
    down = { at: data.at, since: now, start: here, last: here };
  } else if ((data.action === "drag" || data.action === "release") && down) {
    if (Number.isInteger(here[0]) && Number.isInteger(here[1])) down.last = here;
    if (data.action === "release") {
      tune({ bomb: bomb(down.at, now - down.since, down.start, down.last) });
      down = null;
    }
  }
}

export function event(topic, data) {
  if (topic === "idle" && sand === null && data.idle) {
    pour();
    period(0);
  }
  else if (topic === "size" && Array.isArray(data.size)) {
    size = data.size;
    if (pad !== null) call("draw.update", { id: pad, content: { rows: blank() } });
  } else if (topic === "hover" && sand !== null) hover(data);
  else if (topic === "mouse" && sand !== null) mouse(data);
}
