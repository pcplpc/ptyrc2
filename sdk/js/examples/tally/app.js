// SPDX-License-Identifier: GPL-3.0-or-later
// Tally: counts its runs in storage, shows the local date and time,
// gives the count as its result (`ptyrc app tally --wait`). The twin of
// `sdk/rust/examples/tally`. Storage denied: the count is not kept; the
// clock denied: session time from 1970, UTC.

import { Canvas, civil, now, period, result, storage } from "ptyrc";

let runs = 1;
let kept = true;
let canvas;

const two = (n) => String(n).padStart(2, "0");

function line() {
  const note = kept ? "" : " (not kept)";
  try {
    const w = now();
    const c = civil(w.unixMs, w.offsetS);
    return `run ${runs}${note} · ${c.year}-${two(c.month)}-${two(c.day)} ${two(c.hour)}:${two(c.minute)}:${two(c.second)} ${w.abbr}`;
  } catch (e) {
    return `run ${runs}${note} · ${e.code}: ${e.message}`;
  }
}

export function init() {
  let last = null;
  try {
    last = storage.get("runs");
  } catch {}
  runs = (Number.parseInt(last ?? "0", 10) || 0) + 1;
  try {
    storage.set("runs", String(runs));
  } catch {
    kept = false;
  }
  try {
    result({ runs }, 0);
  } catch {}
  period(1000);
  canvas = new Canvas(48, 1);
  tick();
}

export function tick() {
  canvas.fill(0, 0, 48, 1, null, { fg: 10 });
  canvas.text(0, 0, line(), { fg: 10 });
  canvas.present({ anchor: { at: [0, 0] } });
}
