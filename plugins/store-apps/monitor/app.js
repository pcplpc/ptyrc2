// SPDX-License-Identifier: GPL-3.0-or-later
// monitor: the state of this machine, in widgets.
//
//   ptyrc app ptyrc/monitor                          # every monitor
//   ptyrc app ptyrc/monitor --monitor glance,tcp-listen
//   ptyrc app ptyrc/monitor --merged --cycle 10
//
// Monitors, each one tag of variants (`monitor.js`): `glance` (CPU,
// cores, memory, swap, load, uptime, network, top processes),
// `connected` (who is logged in, by user and origin), `tcp-listen`
// (TCP sockets listening on an address seen from outside),
// `disk-space` (used and free space of the local disks). The driver
// reads the machine (`probe.system`) every `--every` seconds. A tag's
// heights follow its rows. `--merged`: one tag, `monitor`, the
// monitors in turn, every `--cycle` seconds; its title follows.

import { PtyrcError, call, exit, log, period, zone } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { MERGED, MONITORS, NAMES, mergedSizes, named, trimmed } from "./monitor.js";

/** Processes of `glance` (the twin's `TOP`). */
export const TOP = 5;

/** The options of `init`; a `RangeError`, its message a line for the log. */
export function options(args) {
  let names = NAMES;
  if (args.monitor != null) {
    names = [...new Set(String(args.monitor).split(",").map((n) => n.trim()).filter(Boolean))];
    const bad = names.filter((n) => !NAMES.includes(n));
    if (bad.length || !names.length) throw new RangeError(`monitor: --monitor: unknown monitor ${bad.join(",") || JSON.stringify(String(args.monitor))}; choose from ${NAMES.join(", ")}`);
  }
  for (const k of ["every", "cycle"]) if (!(args[k] > 0)) throw new RangeError(`monitor: --${k}: not a positive number of seconds: ${args[k]}`);
  return { names, merged: args.merged === true, every: args.every * 1000, cycle: args.cycle * 1000 };
}

/** The machine through `probe.system`: `read(names)` asks the parts of
 * those monitors and keeps them, a new object a part each; an error
 * keeps the last. `ask`: `call` (a fake one under `--unit`). */
export class Source {
  constructor(ask = call) {
    this.ask = ask;
    this.data = Object.fromEntries(NAMES.map((n) => [n, null]));
  }

  read(names) {
    let answer;
    try {
      answer = this.ask("probe.system", { parts: names.map((n) => MONITORS[n].part), top: TOP });
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      return false;
    }
    for (const n of names) this.data[n] = answer?.[MONITORS[n].part] ?? null;
    return true;
  }
}

let o;
let source;
let deck;
const shown = new Map();
const sizes = new Map();
/** Closed by the user: not shown again. */
const closed = new Set();
let turn = 0;
let since = 0;
let due = 0;
const offsets = new Map();

/** Seconds east of UTC at `at` (epoch seconds), in the local zone. */
function offset(at) {
  const k = Math.floor(at);
  if (!offsets.has(k)) {
    let s = 0;
    try {
      s = zone("local", k * 1000).offsets[0]?.offsetS ?? 0;
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
    if (offsets.size > 256) offsets.clear();
    offsets.set(k, s);
  }
  return offsets.get(k);
}

// Memoised: drawn again only when the data or the size changed.
const faces = new Map();
function face(n, w, h) {
  const data = source.data[n];
  let f = faces.get(n);
  if (f === undefined || f.data !== data) faces.set(n, (f = { data, rows: new Map() }));
  const k = `${w}x${h}`;
  if (!f.rows.has(k)) f.rows.set(k, MONITORS[n].render(data, w, h, offset));
  return f.rows.get(k);
}

const current = () => o.names[turn % o.names.length];
const draw = (tag) => (w, h) => face(tag === MERGED ? current() : tag, w, h);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function show() {
  if (o.merged) {
    shown.set(MERGED, deck.add(MERGED, named(mergedSizes(o.names)), draw(MERGED), { title: MONITORS[o.names[0]].title, closed: () => closed.add(MERGED) }));
    return;
  }
  for (const n of o.names) {
    sizes.set(n, trimmed(n, (w, h) => face(n, w, h)));
    shown.set(n, deck.add(n, named(sizes.get(n)), draw(n), { title: MONITORS[n].title, closed: () => closed.add(n) }));
  }
}

/** After a read: a row more or less of data, other heights. */
function sync() {
  if (o.merged) {
    if (closed.has(MERGED)) return;
    const w = shown.get(MERGED);
    const t = MONITORS[current()].title;
    if (w.options.title !== t) (w.options.title = t), w.set(null, { title: t });
    return;
  }
  for (const n of o.names) {
    if (closed.has(n)) continue;
    const s = trimmed(n, (w, h) => face(n, w, h));
    if (same(s, sizes.get(n))) continue;
    sizes.set(n, s);
    deck.resize(shown.get(n), named(s));
  }
}

export function init(args) {
  try {
    o = options(args);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  source = new Source();
  deck = new Deck({ border: "plain" });
  source.read(o.names);
  show();
  due = o.merged ? Math.min(o.every, o.cycle) : o.every;
  period(due);
}

export function tick(now) {
  if (now >= due) {
    if (o.merged) {
      due = now + Math.min(o.every, o.cycle);
      if (now - since >= o.cycle) (turn += 1), (since = now);
      source.read([current()]);
    } else {
      due = now + o.every;
      source.read(o.names);
    }
    sync();
  }
  deck.tick(now);
  period(Math.max(33, due - now));
}

export function event(topic, data) {
  deck.event(topic, data);
}
