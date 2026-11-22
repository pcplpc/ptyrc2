// SPDX-License-Identifier: GPL-3.0-or-later
// pace: keys a minute, and the words typed most.
//
//   ptyrc app ptyrc/pace
//   ptyrc app ptyrc/pace -a --stop mine.txt
//
// Every key on its way to the program (topic `input`, `keys-read`) is
// counted, its bytes dropped at once (`keys`); a `Meter` keeps counts
// by slot of time: the pace now, the best, half an hour of history.
// With `-w` the words are read from the screen, never from the keys:
// what is not echoed, a password, is not read. After a change of the
// text or the cursor, the row of the cursor is read; a `Reader` follows
// the word that grows under it, a cell a key. Words too common (the
// asset `stop.txt`, the words of `--stop`), and what does not look like
// a word (`wordy`), are not counted. The counts are kept in the storage
// (`words.0` to `words.15`, a part of the words each: `part`), the
// parts changed written within a second of a change, and at the end.
// The faces: `pace.js`.

import { PtyrcError, call, exit, file, log, now as clock, period, storage, asset } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Board, Meter, Reader, face, keys, ladder, look, view, wordsOf } from "./pace.js";

/** The options of `init`; a `RangeError`, its message a line for the log. */
export function options(args) {
  const all = args.all === true;
  const pick = (k) => all || args[k] === true;
  const window = args.window ?? 15;
  if (!(window > 0 && window < Infinity)) throw new RangeError(`pace: --window: not a positive number: ${window}`);
  const bars = pick("bars");
  return {
    look: look({
      graph: pick("graph"),
      best: pick("best"),
      words: pick("words") || bars,
      bars,
      colour: pick("colour"),
      top: args.top ?? 10,
    }),
    min: args.min ?? 4,
    window,
    stop: args.stop ?? null,
  };
}

const STRICT = new TextDecoder("utf-8", { fatal: true });

/** The text of UTF-8 bytes; null when they are not UTF-8. */
export function utf8(b) {
  try {
    return STRICT.decode(b);
  } catch (e) {
    if (e instanceof TypeError) return null;
    throw e;
  }
}

/** The words never counted: those shipped, and those of `--stop`. */
export function stopWords(o, host = { asset, file }) {
  const out = wordsOf(host.asset("stop.txt"));
  if (o.stop !== null) {
    const f = host.file(o.stop);
    let text;
    try {
      text = utf8(f.bytes());
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      throw new RangeError(`pace: --stop: ${f.name}: ${e.code}`);
    }
    if (text === null) throw new RangeError(`pace: --stop: ${f.name}: not UTF-8`);
    out.push(...wordsOf(text));
  }
  return out;
}

const KEY = "words";
const MOST = 60 * 1024; // a part; 16 of them under the 1 MiB of the storage
const SAVE_MS = 1000;
const IDLE_MS = 30000;

let o;
let meter;
let board;
let reader;
let deck;
let widget;
let hover = null;
let keyed = 0;
let cols = 80;
let kept = false; // the storage is there
let saved = 0;
const best = { version: -1, top: [] }; // board.top, kept until it changes

const wall = () => clock().unixMs / 1000;

function shown() {
  const at = Date.now() / 1000;
  let top = [];
  if (o.look.words) {
    const w = wall();
    if (best.version !== board.version) [best.version, best.top] = [board.version, board.top(o.look.top)];
    top = best.top.map(([word, count, last]) => [word, count, Math.max(0, w - last)]);
  }
  return view(meter.rate(at), meter.best, meter.slots(at), top);
}

function save() {
  if (!kept || !board.dirty) return;
  try {
    for (const p of board.touched) storage.set(`${KEY}.${p}`, board.dump(p, MOST));
    board.touched.clear();
    board.dirty = false;
  } catch (e) {
    if (!(e instanceof PtyrcError)) throw e;
    log(`pace: the counts not kept: ${e.code}`);
    kept = false;
  }
}

/** The row of the cursor, given to the reader. */
function read() {
  let c;
  let text;
  try {
    c = call("screen.cursor");
    if (!c.visible) return;
    text = call("screen.text", { rect: [0, c.y, cols, 1] }).lines[0] ?? "";
  } catch (e) {
    if (!(e instanceof PtyrcError)) throw e;
    return;
  }
  let added = false;
  const at = wall();
  for (const [word, tok] of reader.see(c.y, text, c.x, keyed)) added = board.add(word, tok, at) || added;
  keyed = 0;
  if (added) deck.draw(widget);
}

export function init(args) {
  let stop;
  try {
    o = options(args);
    stop = stopWords(o);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  meter = new Meter(o.window);
  board = new Board(o.min, stop);
  reader = new Reader();
  const words = o.look.words;
  if (words) {
    try {
      for (const k of storage.keys()) if (k.startsWith(`${KEY}.`)) board.load(storage.get(k) ?? "");
      kept = true;
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
    try {
      cols = call("session.info").size[0];
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
  }
  // While the pointer is there: a mouse at rest sends nothing.
  deck = new Deck({ border: "round", hoverTimeout: words ? null : undefined });
  const handlers = words
    ? {
        hover: (x, y) => {
          hover = [x, y];
        },
        leave: () => {
          hover = null;
        },
      }
    : {};
  widget = deck.add("pace", ladder(o.look), (w, h) => face(shown(), w, h, o.look, hover), handlers);
  call("subscribe", { topics: words ? ["input", "cursor", "text", "size"] : ["input"] });
  if (words) read(); // the row the cursor is on, before a key
  period(IDLE_MS);
}

export function tick(now) {
  deck.tick(now);
  if (board.dirty && now >= saved + SAVE_MS) {
    save();
    saved = now;
  }
  // The pace falls by itself: a look a second while it is up.
  period(meter.minute(now / 1000) || board.dirty ? 1000 : IDLE_MS);
}

export function event(topic, data) {
  if (deck.event(topic, data)) return;
  if (topic === "input" && typeof data?.bytes === "string" && !data.grabbed && data.widget == null) {
    const got = keys(Uint8Array.fromBase64(data.bytes));
    if (!got) return;
    meter.add(Date.now() / 1000, got);
    keyed += got;
    deck.draw(widget);
    period(1000);
  } else if (topic === "size") {
    cols = data.size[0];
  } else if ((topic === "cursor" || topic === "text") && o.look.words) {
    read();
  }
}

export function stop() {
  save();
}
