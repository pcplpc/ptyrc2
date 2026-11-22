// SPDX-License-Identifier: GPL-3.0-or-later
// gallery: framed pictures, hung one after the other.
//
//   ptyrc app ptyrc/gallery
//   ptyrc app ptyrc/gallery -a --every 2m --kind animal,silly
//   ptyrc app ptyrc/gallery --file mine.pic --dir ~/pictures
//
// A widget a picture hung (`gallery`, `gallery-2`, ...): the sizes of
// the picture its variants, the `painting` border, the name on the
// plate; a new picture declares the tag again. The pictures shipped
// are assets (`pictures/`, the ship list `SHIPPED`); those named are
// files: a `--file`, the `*.pic` of a `--dir`. The format: `picture.js`.

import { PtyrcError, asset, assets, exit, file, log, period, random } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Hall } from "./hall.js";
import { KINDS, PictureError, parse } from "./picture.js";
import { Twister } from "./twister.js";

/** Seconds of `90`, `30s`, `10m`, `1h`; null: not a duration. */
export function span(text) {
  const m = /^(\d+(?:\.\d+)?)([smh]?)$/.exec(text);
  if (!m || !(Number(m[1]) > 0)) return null;
  return Number(m[1]) * { "": 1, s: 1, m: 60, h: 3600 }[m[2]];
}

/** The options of `init`; a `RangeError`, its message a line for the log. */
export function options(args) {
  const every = span(String(args.every ?? "10m"));
  if (every === null) throw new RangeError(`gallery: --every: not a duration: ${args.every} (as 90, 30s, 10m, 1h)`);
  const kinds = String(args.kind ?? "").split(",").filter(Boolean);
  for (const k of kinds) if (!KINDS.includes(k)) throw new RangeError(`gallery: --kind: ${k}: not one of ${KINDS.join(", ")}`);
  const all = args.all === true;
  const wall = all || args.wall === true;
  return {
    every,
    kinds,
    files: args.file ?? [],
    dirs: args.dir ?? [],
    moving: all || args.move === true,
    author: all || args.label === true,
    hover: all || args.hover === true,
    count: args.count ?? (wall ? 3 : 1),
    candidates: args.candidates === true,
  };
}

/** The stems of a ship list, best first. */
export function shipped(text) {
  return text.split("\n").map((l) => l.trim()).filter((s) => s && s[0] !== "#");
}

const STRICT = new TextDecoder("utf-8", { fatal: true });

function read(f, source) {
  try {
    return STRICT.decode(f.bytes());
  } catch (e) {
    if (e instanceof PtyrcError) throw new PictureError(`${source}: ${e.code}`);
    throw new PictureError(`${source}: not UTF-8`);
  }
}

/** The pictures to hang: `[source, text]` of the files and directories
 * named (`*.pic`, by name), or of the collection shipped (`reads`: the
 * ship list's order unless `candidates`, every picture else). */
export function sources(o, host = { asset, assets, file }) {
  const out = [];
  for (const h of o.files) {
    const f = host.file(h);
    out.push(() => [f.name, read(f, f.name)]);
  }
  for (const h of o.dirs) {
    const d = host.file(h);
    for (const name of d.list().filter((n) => n.endsWith(".pic")).sort()) {
      const source = `${d.name}/${name}`;
      out.push(() => [source, read(d.child(name), source)]);
    }
  }
  if (o.files.length || o.dirs.length) return out.map((f) => f());
  let names = host.assets().filter((n) => n.endsWith(".pic") && !n.includes("/")).sort();
  if (!o.candidates) {
    let ship = null;
    try {
      ship = shipped(host.asset("SHIPPED"));
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
    if (ship !== null) {
      const order = new Map(ship.map((s, nth) => [s, nth]));
      const stem = (n) => n.slice(0, -4);
      names = names.filter((n) => order.has(stem(n))).sort((a, b) => order.get(stem(a)) - order.get(stem(b)));
    }
  }
  return names.map((n) => [`pictures/${n}`, host.asset(n)]);
}

/** The pictures of `sources`, of the kinds asked. */
export function collection(o, host) {
  const pictures = sources(o, host).map(([source, text]) => parse(text, source));
  return o.kinds.length ? pictures.filter((p) => o.kinds.includes(p.kind)) : pictures;
}

const HOVER_MS = 1500;
let o;
let hall;
let deck;
let hung = [];
let overUntil = null;

function seed() {
  const b = random(4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

function hang(slot, widget) {
  const sizes = slot.picture.boxes;
  if (widget === undefined) {
    const tag = hung.length ? `gallery-${hung.length + 1}` : "gallery";
    const handlers = { title: slot.label, closed: () => deck.widgets.length || exit(0) };
    if (o.hover) {
      handlers.hover = (x, y, w) => {
        overUntil = Date.now() + HOVER_MS;
        slot.size = w.size;
        const changed = slot.move(x, y);
        schedule(Date.now());
        return changed;
      };
      handlers.leave = () => {
        overUntil = null;
        return slot.leave();
      };
    }
    const w = deck.add(tag, sizes, (vw, vh) => slot.render(vw, vh), handlers);
    slot.retitle = () => {
      w.options.title = slot.label;
      w.set(null, { title: slot.label });
    };
    hung.push(w);
  } else {
    widget.options.title = slot.label;
    deck.resize(widget, sizes);
  }
}

// The next tick: the next change seen, or the end of a hover.
function schedule(now) {
  let at = hall.next(now / 1000) * 1000;
  if (overUntil !== null) at = Math.min(at, overUntil);
  if (at === Infinity) at = now + 3600000;
  period(Math.max(33, Math.ceil(at - now)));
}

export function init(args) {
  let pictures;
  try {
    o = options(args);
    pictures = collection(o);
  } catch (e) {
    if (!(e instanceof RangeError || e instanceof PictureError)) throw e;
    log(e.message.startsWith("gallery: ") ? e.message : `gallery: ${e.message}`);
    exit(2);
  }
  if (!pictures.length) {
    log("gallery: no picture to hang");
    exit(2);
  }
  hall = new Hall(pictures, new Twister(seed()), o);
  deck = new Deck({ border: "painting" });
  for (const slot of hall.slots) hang(slot);
  schedule(0);
}

export function tick(now) {
  hall.slots.forEach((s, nth) => (s.size = hung[nth].size));
  for (const slot of hall.at(now / 1000)) hang(slot, hung[hall.slots.indexOf(slot)]);
  deck.tick(now);
  schedule(now);
}

export function event(topic, data) {
  deck.event(topic, data);
}
