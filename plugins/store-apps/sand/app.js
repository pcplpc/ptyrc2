// SPDX-License-Identifier: GPL-3.0-or-later
// Sand: a thin stream piles on the text of an idle pane.
//
// The effect `sand` of the driver runs the simulation and reads the
// text under it as its walls; the app starts it once the pane is idle
// (`frame.idle`, topic `idle`; without `shape` the pane is idle at
// once). The pointer is the driver's to tell the effect (`pointer`,
// `bomb` in its params): `--hover` the cell under it, `--bomb` a press
// released on the pile. The app is told nothing.

import { call, exit, log, period, random } from "ptyrc";

const MAX_RATE = 1000;
const LOOK = 500; // ms between two looks at a pane not idle yet

let opts;
let sand = null; // the effect's sprite

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

function pour() {
  const params = { rate: opts.rate, cursor: opts.cursor, seed: u32() };
  if (opts.hover) params.pointer = true;
  if (opts.bomb) params.bomb = true;
  try {
    sand = call("draw.put", { effect: { name: "sand", params } }).id;
  } catch (e) {
    log(`sand: the effect: ${e.code}: ${e.message}`);
    exit(1);
  }
}

export function init(args) {
  opts = options(args);
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

export function event(topic, data) {
  if (topic === "idle" && sand === null && data.idle) {
    pour();
    period(0);
  }
}
