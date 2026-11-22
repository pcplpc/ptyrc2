// SPDX-License-Identifier: GPL-3.0-or-later
// git: a repository, in widgets. It reads, it never writes.
//
//   ptyrc app ptyrc/git                 # the head: branch, upstream, counts
//   ptyrc app ptyrc/git -t -l           # and the status, the log
//   ptyrc app ptyrc/git -a              # every tag, the graph, the warnings
//   ptyrc app ptyrc/git --monitor log -g --repo ~/src/linux
//
// Tags (`git.js`): `head`; `status` (-t), `log` (-l; -g as a graph),
// `branches` (-b), `stash` (-S), `worktrees` (-w), `pulse` (-p). A
// tag with nothing to tell is hidden, unless `--monitor` names it.
// `-W`: the border of `head` chases while something wants a look.
// `--merged`: one widget, the tags in turn.
//
// The repository: `--repo`, else the one the program is in, followed
// when it moves. The driver reads it (`probe.git`): every second when
// its git directory changed (the stamp), every `--every` seconds
// whatever. The pointer over a commit of `log` tells it in full.

import { PtyrcError, call, exit, log, period, zone } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { FLAGS, Hover, MERGED, NAMES, chase, look, named, quiet, renderLog, tags, title, trimmed, warnings } from "./git.js";

/** Commits of the log asked (the twin's `COMMITS`). */
export const LOG = 60;
/** Session ms a commit stays told in full after the last move. */
export const HOVER_MS = 3000;
const BORDER = "round";

/** The options of `init`; a `RangeError`, its message a line for the log. */
export function options(args) {
  let names = null;
  if (args.monitor != null) {
    names = [...new Set(String(args.monitor).split(",").map((n) => n.trim()).filter(Boolean))];
    const bad = names.filter((n) => !NAMES.includes(n));
    if (bad.length || !names.length) throw new RangeError(`git: --monitor: unknown tag ${bad.join(",") || JSON.stringify(String(args.monitor))}; choose from ${NAMES.join(", ")}`);
  }
  for (const k of ["every", "cycle", "dirty"]) if (!(args[k] > 0)) throw new RangeError(`git: --${k}: not a positive number: ${args[k]}`);
  const all = args.all === true;
  const isNamed = names !== null;
  if (!isNamed) names = ["head", ...FLAGS.filter((n) => all || args[n] === true || (n === "log" && args.graph === true))];
  return {
    names,
    named: isNamed,
    merged: args.merged === true,
    repo: args.repo?.file ?? null,
    every: args.every * 1000,
    cycle: args.cycle * 1000,
    look: look({ graph: all || args.graph === true, warn: all || args.warn === true, dirty: args.dirty * 3600 }),
  };
}

/** The repository shown, read through `probe.git`: `poll(now)` asks
 * each time it is called, with the last stamp unless `every` ms passed
 * since the last read; true when read. `ask`: `call` (a fake one under
 * `--unit`). */
export class Source {
  constructor(o, ask = call) {
    this.params = { log: LOG, ...(o.repo !== null ? { repo: o.repo } : {}) };
    this.every = o.every;
    this.ask = ask;
    this.repo = null;
    this.stamp = null;
    this.due = 0;
  }

  poll(now) {
    const params = now < this.due && this.stamp !== null ? { ...this.params, since: this.stamp } : this.params;
    let answer;
    try {
      answer = this.ask("probe.git", params);
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      return false; // within the second of a read elsewhere: next time
    }
    if (answer?.unchanged) return false;
    this.repo = answer?.repo ?? null;
    this.stamp = answer?.stamp || null;
    this.due = now + this.every;
    return true;
  }
}

let o;
let lk;
let known;
/** `known` but the log drawn without telling `hover` its rows: for
 * `trimmed`. */
let sized;
let hover;
let source;
let deck;
const shown = new Map();
const sizes = new Map();
const border = new Map();
const hidden = new Set();
/** Closed by the user: not shown again. */
const closed = new Set();
let turn = 0;
let since = 0;
let overUntil = null;
let due = 0;
const offsets = new Map();

/** Seconds east of UTC at `at` (epoch seconds), in the local zone. */
function offset(at) {
  if (!offsets.has(at)) {
    let s = 0;
    try {
      s = zone("local", Math.floor(at) * 1000).offsets[0]?.offsetS ?? 0;
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
    if (offsets.size > 256) offsets.clear();
    offsets.set(at, s);
  }
  return offsets.get(at);
}

const repo = () => source.repo;
const loud = () => {
  const out = o.names.filter((n) => o.named || !quiet(n, repo()));
  return out.length ? out : o.names.slice(0, 1);
};
const current = () => {
  const l = loud();
  return l[turn % l.length];
};

// Memoised: drawn again only when the repository, the focus or the turn
// changed.
function draw(tag) {
  let key = null;
  let rows = null;
  return (w, h) => {
    const n = tag === MERGED ? current() : tag;
    const k = [repo(), n, w, h, n === "log" ? hover.focus : null];
    if (key === null || k.some((v, i) => v !== key[i])) (key = k), (rows = known[n].render(repo(), w, h));
    return rows;
  };
}

function want(tag) {
  if (tag === MERGED) {
    const every = o.names.map((n) => known[n].sizes);
    const most = Math.max(...every.map((s) => s.length));
    return Array.from({ length: most }, (_, r) => [0, 1].map((i) => Math.max(...every.map((s) => s[Math.min(r, s.length - 1)][i]))));
  }
  return trimmed(sized[tag], repo());
}

const edge = () => (lk.warn && warnings(repo(), lk).length ? chase(BORDER) : null);
const nameOf = (tag) => title(tag === MERGED ? current() : tag, repo(), known);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** Shows `tag`, or shows it again: other sizes, another border. */
function put(tag) {
  sizes.set(tag, want(tag));
  border.set(tag, tag === MERGED || tag === o.names[0] ? edge() : null);
  const b = border.get(tag) ?? undefined;
  const w = shown.get(tag);
  if (w === undefined) {
    const opts = { title: nameOf(tag), border: b, closed: () => closed.add(tag) };
    if (tag === "log") {
      opts.hover = (x, y) => {
        overUntil = Date.now() + HOVER_MS;
        return hover.move(x, y);
      };
      opts.leave = () => {
        overUntil = null;
        return hover.leave();
      };
    }
    shown.set(tag, deck.add(tag, named(sizes.get(tag)), draw(tag), opts));
  } else {
    w.options.title = nameOf(tag);
    w.options.border = b;
    deck.resize(w, named(sizes.get(tag)));
  }
  hidden.delete(tag);
}

function retitle(tag) {
  const w = shown.get(tag);
  const t = nameOf(tag);
  if (w.options.title === t) return;
  w.options.title = t;
  w.set(null, { title: t });
}

/** After a read: the tags that come and go, their sizes. */
function sync() {
  if (o.merged) {
    if (closed.has(MERGED)) return;
    if (!shown.has(MERGED) || !same(border.get(MERGED), edge())) put(MERGED);
    retitle(MERGED);
    return;
  }
  for (const n of o.names) {
    if (closed.has(n)) continue;
    if (!o.named && quiet(n, repo())) {
      const w = shown.get(n);
      if (w !== undefined && !hidden.has(n)) w.hide(), hidden.add(n);
      continue;
    }
    const lead = n === o.names[0];
    if (!shown.has(n) || hidden.has(n) || !same(want(n), sizes.get(n)) || (lead && !same(border.get(n), edge()))) put(n);
    else if (n === "head") retitle(n);
  }
}

function schedule(now) {
  const at = overUntil !== null ? Math.min(due, overUntil) : due;
  period(Math.max(33, at - now));
}

export function init(args) {
  try {
    o = options(args);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  lk = o.look;
  hover = new Hover(offset);
  known = tags(lk, (r, w, h, l) => hover.drawn(r, w, h, l));
  sized = tags(lk, (r, w, h, l) => renderLog(r, w, h, l, hover.focus, offset));
  source = new Source(o);
  deck = new Deck({ border: BORDER, hoverTimeout: HOVER_MS });
  source.poll(0);
  sync();
  due = 1000;
  schedule(0);
}

export function tick(now) {
  if (now >= due) {
    due = now + 1000;
    const turned = o.merged && now - since >= o.cycle;
    if (turned) (turn += 1), (since = now);
    if (source.poll(now) || turned) sync();
  }
  if (overUntil !== null && now >= overUntil) (overUntil = null), deck.leave();
  deck.tick(now);
  schedule(now);
}

export function event(topic, data) {
  deck.event(topic, data);
}
