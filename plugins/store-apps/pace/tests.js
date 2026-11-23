// SPDX-License-Identifier: GPL-3.0-or-later
// Unit tests of pace: the twin's `test_pace.py`, and its looks by
// hashes (`twin.js`).

import { PtyrcError, asset, assert, assertEqual, cells } from "ptyrc";
import { MOST, options, stopWords, utf8 } from "./app.js";
import {
  Board,
  Meter,
  PARTS,
  PASTE,
  Reader,
  SLOT,
  SPAN,
  ago,
  face,
  part,
  graph,
  keys,
  ladder,
  look,
  round,
  view,
  wordsOf,
  wordy,
} from "./pace.js";
import { FACES, LOOKS, NOISE, NOISE_KEPT, TEXT, TEXT_KEPT, VIEWS, WHOLE } from "./twin.js";

const plain = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const bytes = (s) => new TextEncoder().encode(s);
const STOP = wordsOf(asset("stop.txt"));

export function test_round() {
  assertEqual([0.5, 1.5, 2.5, 3.5, 2.4999, 2.5001].map(round), [0, 2, 2, 4, 2, 3]);
}

export function test_keys() {
  const cases = [
    ["", 0],
    ["a", 1],
    ["ab", 2],
    ["\r", 1],
    ["\x7f", 1],
    ["\x03", 1],
    ["é", 1],
    ["日本", 2],
    ["\x1b[A", 1], // an arrow
    ["\x1bOP", 1], // F1
    ["\x1b[1;5C", 1], // ctrl and an arrow
    ["\x1b[15~a", 2],
    ["\x1b", 1], // escape alone
    ["\x1bb", 1], // alt and b
    ["\x1b[200~secret\x1b[201~", 0], // a bracketed paste
    ["\x1b[<35;10;5M", 0], // a mouse report
    ["a".repeat(PASTE), 0], // too many at once
    ["a".repeat(PASTE - 1), PASTE - 1],
  ];
  for (const [data, count] of cases) assertEqual(keys(bytes(data)), count, JSON.stringify(data));
}

export function test_meter() {
  const meter = new Meter(15);
  assertEqual([meter.rate(0), meter.minute(0), meter.best], [0, 0, 0]);
  for (let i = 0; i < 30; i++) meter.add(100 + i / 2, 1); // two keys a second, for 15 s
  assertEqual(meter.rate(114.5), 120);
  assert(meter.minute(114.5) === 30 && meter.total === 30);
  assertEqual(meter.best, 120);
  assertEqual(meter.rate(122), 60); // half the window is quiet
  assert(meter.rate(130) === 0 && meter.minute(130) === 30);
  assertEqual(meter.minute(175), 0);
  assertEqual(meter.best, 120);
  meter.add(200, 0);
  assertEqual(meter.total, 30);
}

export function test_meter_slots() {
  const meter = new Meter();
  const count = SPAN / SLOT;
  assertEqual(meter.slots(5), new Array(count).fill(0));
  meter.add(5, 3);
  meter.add(9, 1);
  meter.add(10, 2);
  const slots = meter.slots(12);
  assert(slots.length === count && slots.reduce((a, b) => a + b) === 6);
  assertEqual(slots.slice(-2), [4, 2]);
  assertEqual(meter.slots(12 + SPAN - SLOT).slice(0, 2), [2, 0]);
  assertEqual(meter.slots(12 + SPAN).reduce((a, b) => a + b), 0);
  meter.add(3 * SPAN, 1); // what is old is dropped
  assertEqual(meter.slots_.size, 1);
}

/** A row of a terminal a reader looks at after each key. */
class Screen {
  constructor() {
    this.reader = new Reader();
    this.row = 0;
    this.text = "$ ";
    this.words = [];
    this.look(0);
  }

  look(keyed = 1) {
    const got = this.reader.see(this.row, this.text.trimEnd(), Array.from(this.text).length, keyed);
    this.words.push(...got.map(([w]) => w));
  }

  type(text) {
    for (const c of text) {
      if (c === "\b") this.text = Array.from(this.text).slice(0, -1).join("");
      else if (c === "\n") [this.row, this.text] = [this.row + 1, "$ "];
      else this.text += c;
      this.look();
    }
    return this;
  }

  /** Text that comes at once: a program, a paste, a completion. */
  write(text, keyed = 0) {
    this.text += text;
    this.look(keyed);
    return this;
  }
}

export function test_reader() {
  assertEqual(new Screen().type("echo hello world\n").words, ["echo", "hello", "world"]);
  assertEqual(new Screen().type("vim notes.md ").words, ["vim", "notes", "md"]);
  assertEqual(new Screen().type("don't stop").words, ["don't"]);
  assertEqual(new Screen().type("été, là\n").words, ["été", "là"]);
  // Nothing ends the word: not counted yet.
  assertEqual(new Screen().type("hello").words, []);
}

export function test_reader_rubbed_out() {
  assertEqual(new Screen().type("helo\b\bllo wrold\b\b\b\borld\n").words, ["hello", "world"]);
  assertEqual(new Screen().type("abc\b\b\b\n").words, []);
  // Seen late: `widgt`, then `widget` at the next look.
  const screen = new Screen().type("widgt");
  screen.text = "$ widget";
  screen.look(4);
  assertEqual(screen.type(" ").words, ["widget"]);
}

export function test_reader_not_typed() {
  // A program writes: no key.
  assertEqual(new Screen().write("total 42 files ").type("\n").words, []);
  // A paste: more cells than keys.
  assertEqual(new Screen().write("pasted words here ", 1).type("\n").words, []);
  // A completion: a word counts when half of it was typed.
  assertEqual(new Screen().type("doc").write("umentation", 1).type(" ").words, []);
  assertEqual(new Screen().type("documen").write("tation", 1).type(" ").words, ["documentation"]);
  // Two keys between two looks: both typed.
  let screen = new Screen().type("hell");
  screen.text += "o ";
  screen.look(2);
  assertEqual(screen.words, ["hello"]);
  // Not echoed: the cursor does not move, nothing is read.
  screen = new Screen();
  for (let i = 0; i < 8; i++) screen.look();
  assertEqual(screen.type("\n").words, []);
}

export function test_reader_token() {
  const reader = new Reader();
  reader.see(0, "", 0, 0);
  let text = "";
  const got = [];
  for (const c of "key=a3f9c2deadbeef ok ") {
    text += c;
    got.push(...reader.see(0, text.trimEnd(), text.length, 1));
  }
  const has = (w) => got.some((g) => JSON.stringify(g) === JSON.stringify(w));
  assert(has(["deadbeef", "key=a3f9c2deadbeef"]) && has(["ok", "ok"]), JSON.stringify(got));
}

const WORDS = wordsOf(`
  because favourite hourglass widget terminal keyboard something through
  rhythm strength February aquarium backdrop documentation hyperlink
  function return import lambda async queue jazz echo grep mkdir rsync
  getUserName FrameId WidgetId HTTPServer README Makefile TODO
  naïve café être garçon français aujourd'hui Straße
  программа 日本語`);

const NOISY = wordsOf(`
  xkcd html ctrl sqrt pgrp zzzz aaaa rrggbb deadbeef abcdefab ffffffff
  QWxhZGRpbjpv aGVsbG8gd29y ZssOSysQ oZnhxC eWUwV xANy LZRoN
  tthwwhhhttt qwrtpsdfgh aeiouaeiou xqzjvkwpb bcdfghjklm
  supercalifragilisticexpialidocious`);

export function test_wordy() {
  assertEqual(WORDS.filter((w) => !wordy(w)), []);
  assertEqual(NOISY.filter((w) => wordy(w)), []);
  // The token tells: digits next to the word, kinds that change.
  assert(wordy("cafe", "cafe"));
  assert(!wordy("cafe", "9f8cafe01b"));
  assert(!wordy("face", "sha256:9f86d081884c7d659a2feaa0c55ad015face"));
  assert(!wordy("Nhbm", "U29tZSBzZWNyZXQ+Nhbm/x9Q=="));
  assert(wordy("local", "/usr/local/share/doc"));
  assert(wordy("widget", "self.widget.rect"));
  assert(wordy("project", "/home/user/project42/notes"));
}

export function test_wordy_on_noise() {
  // Pieces of hashes, of base64 and of base32 (the twin's draws), cut
  // as the reader cuts them: few pass, each as the twin says; the words
  // of the twin's test file: most do.
  let verdicts = "";
  for (const tok of NOISE) {
    let piece = "";
    for (const c of tok + " ") {
      if (/^\p{L}$/u.test(c)) {
        piece += c;
        continue;
      }
      if (piece.length >= 4) verdicts += wordy(piece, tok) ? "1" : "0";
      piece = "";
    }
  }
  assertEqual(verdicts, NOISE_KEPT);
  const kept = [...verdicts].filter((v) => v === "1").length;
  assert(verdicts.length > 1000 && kept < 0.01 * verdicts.length, `${kept} of ${verdicts.length}`);
  assertEqual(TEXT.map((w) => (wordy(w) ? "1" : "0")).join(""), TEXT_KEPT);
  const words = TEXT.filter((w) => !NOISY.includes(w));
  assert(words.filter((w) => wordy(w)).length > 0.95 * words.length);
}

export function test_board() {
  const board = new Board(4, STOP);
  assert(!board.dirty);
  assert(board.add("Hourglass", "", 10) && board.add("hourglass", "", 20));
  assert(board.add("widget", "", 15) && board.add("sand", "", 30));
  assert(!board.add("the") && !board.add("with") && !board.add("AVEC"));
  assert(!board.add("deadbeef") && !board.add("face", "9f8face01b"));
  assertEqual(board.top(2), [["hourglass", 2, 20], ["sand", 1, 30]]);
  assertEqual(board.top(9).map(([w]) => w), ["hourglass", "sand", "widget"]);
  assertEqual(new Board(6, STOP).add("sand"), false);
  assertEqual(new Board(4, ["sand"]).add("sand"), false);
  assertEqual(new Board(4, []).add("with"), true);

  const text = board.dump();
  // A part a value: each word in the one of its part alone.
  assertEqual([...board.touched].sort((a, b) => a - b), [...new Set(["hourglass", "widget", "sand"].map(part))].sort((a, b) => a - b));
  for (let p = 0; p < PARTS; p++) {
    for (const w of Object.keys(JSON.parse(board.dump(p)).words)) assertEqual(part(w), p, w);
  }
  assertEqual(Object.keys(JSON.parse(text).words).sort(), ["hourglass", "sand", "widget"]);
  const again = new Board(4, STOP);
  again.add("sand", "", 5);
  again.load(text);
  assertEqual(again.top(1), [["sand", 2, 30]]);
  again.load("{]");
  again.load('{"words": 3}');
  assertEqual(again.words.size, 3);
  // Too large for a value: the words typed least go.
  for (let i = 0; i < 40; i++) board.add(`word${"abcdefghij"[i % 10]}${"klmnoprstu"[(i / 10) | 0]}`);
  const cut = JSON.parse(board.dump(null, 300)).words;
  assert(Object.keys(cut).length < 43 && "hourglass" in cut, JSON.stringify(cut));
}

export function test_kept_under_the_cap() {
  // A store near the cap: 7000 words of 60 ideographs (180 bytes), the
  // parts over a value's 60 KiB. Saved, each part fits, the whole under
  // the 1 MiB of the storage; the words typed longest ago are gone from
  // the board too, those shown kept whole.
  const words = {};
  let x = 1;
  const han = () => {
    x = (Math.imul(x, 1103515245) + 12345) >>> 0;
    return String.fromCodePoint(0x4e00 + ((x >>> 8) % 20000));
  };
  for (let i = 0; i < 7000; i++) {
    const word = Array.from({ length: 60 }, han).join("");
    words[word] = [1 + (i % 7), 1e9 + i];
  }
  words.ancient = [5000, 1];
  const board = new Board(4, STOP);
  board.load(JSON.stringify({ words }));
  const spare = new Set(board.top(10).map(([w]) => w));
  assert(spare.has("ancient"));
  const parts = Array.from({ length: PARTS }, () => []);
  for (const [w, entry] of Object.entries(words)) parts[part(w)].push([w, entry]);
  let used = 0;
  let kept = 0;
  for (let p = 0; p < PARTS; p++) {
    const value = board.dump(p, MOST, spare);
    const size = bytes(value).length;
    // Cut to its bound: within an entry of it.
    assert(size <= MOST && value.length * 3 > MOST - 600, `${p}: ${size}`);
    used += size;
    const lasts = Object.values(JSON.parse(value).words).map(([, l]) => l);
    kept += lasts.length;
    // The oldest go first: what is left is the newest of its part.
    const dropped = parts[p].filter(([w]) => !board.words.has(w));
    const least = Math.min(...lasts.filter((l) => l > 1));
    assert(dropped.length > 0 && dropped.every(([, [, l]]) => l < least), `${p}`);
  }
  assert(used <= 1024 * 1024 && used > 800 * 1024, `${used}`);
  assertEqual(board.words.size, kept);
  assertEqual(board.top(1), [["ancient", 5000, 1]]);
  // Under the cap, nothing goes.
  const small = new Board(4, STOP);
  small.load(JSON.stringify({ words: { sand: [2, 5], dune: [1, 1] } }));
  small.dump(part("dune"), MOST);
  assertEqual(small.words.size, 2);
}

export function test_stop_words() {
  // The asset: the twin's `STOP`; `--stop`: a file of the user.
  assertEqual(STOP.length, 105);
  assert(STOP.includes("être") && STOP.includes("because"));
  const fake = (text) => ({
    asset,
    file: () => ({ name: "stop", bytes: () => (typeof text === "string" ? bytes(text) : text) }),
  });
  const o = options({ stop: { file: "f0", name: "stop" } });
  assertEqual(stopWords(o, fake("sand\nglass  dune\n")).slice(-3), ["sand", "glass", "dune"]);
  let error = null;
  try {
    stopWords(o, fake(new Uint8Array([0x61, 0xff])));
  } catch (e) {
    error = e;
  }
  assert(error instanceof RangeError, String(error));
  assertEqual(error.message, "pace: --stop: stop: not UTF-8");
  const gone = { asset, file: () => ({ name: "stop", bytes: () => { throw new PtyrcError({ code: "no-file", message: "gone" }); } }) };
  try {
    stopWords(o, gone);
  } catch (e) {
    error = e;
  }
  assertEqual(error.message, "pace: --stop: stop: no-file");
}

export function test_utf8() {
  assertEqual(utf8(bytes("été \ufffd")), "été \ufffd");
  assertEqual(utf8(new Uint8Array([0xef, 0xbb, 0xbf, 0x61])), "a");
  assertEqual(utf8(new Uint8Array([0x61, 0xc3])), null);
}

export function test_options() {
  const all = options({ all: true }).look;
  assert(all.graph && all.best && all.words && all.bars && all.colour);
  assert(options({ bars: true }).look.words);
  assertEqual(options({}).look, look());
  assertEqual([options({}).window, options({}).min, options({}).stop], [15, 4, null]);
  for (const window of [0, -1, Infinity]) {
    let error = null;
    try {
      options({ window });
    } catch (e) {
      error = e;
    }
    assert(error instanceof RangeError, `${window}`);
  }
}

export function test_ago() {
  assertEqual(
    [0, 4, 40, 60, 200, 7300, 200000].map(ago),
    ["now", "now", "40s ago", "1m ago", "3m ago", "2h ago", "2d ago"],
  );
}

export function test_faces() {
  const top = [["hourglass", 40, 3], ["widget", 20, 200], ["sand", 5, 90000]];
  const v = view(312, 345, [...new Array(170).fill(0), ...new Array(10).fill(10)], top);
  const bare = look();
  assertEqual(plain(face(v, 3, 1, bare)), ["312"]);
  assertEqual(plain(face(v, 9, 1, bare)), ["312 k/min"]);
  assertEqual(plain(face(v, 16, 1, look({ best: true }))), ["312 k/min ▲ 345"]);
  const words = look({ words: true });
  assertEqual(plain(face(v, 24, 1, words)), ["312 k/min ♥ hourglass"]);
  assertEqual(plain(face(v, 20, 4, words)), [
    "312 k/min     62 wpm",
    "1 hourglass       40",
    "2 widget          20",
    "3 sand             5",
  ]);
  const barsLook = look({ words: true, bars: true });
  const rows = plain(face(v, 26, 4, barsLook));
  const bars = rows.slice(1).map((r) => [...r].filter((c) => c === "█").length);
  assert(bars[0] > bars[1] && bars[1] > bars[2] && bars[2] >= 1 && rows[1].endsWith(" 40"), JSON.stringify(rows));
  assert(bars[0] === 2 * bars[1] || bars[0] === 2 * bars[1] + 1); // half, an eighth exact
  // Under the pointer: the count and when.
  const over = plain(face(v, 26, 4, barsLook, [5, 2]));
  assert(over[2] === "2 widget      ×20 · 3m ago" && over[1] === rows[1], JSON.stringify(over));
  assertEqual(plain(face(view(), 20, 4, words))[1], "no word yet");
}

export function test_graph() {
  const zeros = (n) => new Array(n).fill(0);
  assertEqual(plain(graph(zeros(180), 10, 2, look())), [" ".repeat(10), "⣀".repeat(10)]);
  const slots = [...zeros(90), ...new Array(45).fill(30), ...new Array(45).fill(60)];
  assertEqual(plain(graph(slots, 10, 2, look())), ["     ⣀⣀⣸⣿⣿", "⣀⣀⣀⣀⣀⣿⣿⣿⣿⣿"]);
  assertEqual(plain(graph(slots, 4, 1, look())), ["⣀⣀⣶⣿"]);
  assertEqual(plain(graph([], 3, 1, look())), ["⣀⣀⣀"]);
  const full = look({ graph: true, words: true });
  const v = twinView(5);
  assertEqual(plain(face(v, 26, 13, full)).slice(1, 3), plain(graph(v.slots, 26, 2, full)));
}

// -- the twin's looks --------------------------------------------------------

/** A face as the twin's fixture: runs of one style merged. */
function norm(rows) {
  return JSON.stringify(
    rows.map((row) => {
      const out = [];
      for (const r of row) {
        if (!r.t) continue;
        const style = [r.fg ?? null, !!r.bold, !!r.dim];
        const last = out[out.length - 1];
        if (last && last[1] === style[0] && last[2] === style[1] && last[3] === style[2]) last[0] += r.t;
        else out.push([r.t, ...style]);
      }
      return out;
    }),
  );
}

function fnv(s) {
  let h = 0x811c9dc5;
  for (const c of s) h = Math.imul(h ^ c.codePointAt(0), 0x01000193) >>> 0;
  return h;
}

function twinView(i) {
  const [rate, best, slots, top] = VIEWS[i];
  return view(rate, best, slots, top);
}

// The bounds of `test_bounds`, and the twin's face, for one view.
function twin(vi) {
  const v = twinView(vi);
  const looks = LOOKS.map((l) => look(l));
  let n = 0;
  for (const [fv, li, w, h, hi, hash] of FACES) {
    if (fv !== vi) continue;
    const lk = looks[li];
    const hover = [null, [0, 0], [w - 1, h - 1]][hi];
    const rows = face(v, w, h, lk, hover);
    assert(rows.length > 0 && rows.length <= h, `${w}x${h}`);
    for (const t of plain(rows)) assert(cells(t) <= w && !/\p{C}/u.test(t), `${w}x${h}: ${t}`);
    if (!lk.colour) assert(rows.every((row) => row.every((r) => r.fg === undefined)));
    assertEqual(fnv(norm(rows)), hash, `view ${vi}, look ${li}, ${w}x${h}, hover ${hi}`);
    n++;
  }
  assert(n > 0);
}

export function test_ladder() {
  for (const l of LOOKS) {
    const lk = look(l);
    const sizes = ladder(lk);
    assert(sizes.length >= 2 && sizes.length <= 16, JSON.stringify(l));
    const seen = FACES.filter(([vi, li, , , hi]) => vi === 0 && hi === 0 && LOOKS[li] === l).map(([, , w, h]) => [w, h]);
    assertEqual(sizes, seen, JSON.stringify(l));
  }
}

export function test_twin_0() {
  twin(0);
}
export function test_twin_1() {
  twin(1);
}
export function test_twin_2() {
  twin(2);
}
export function test_twin_3() {
  twin(3);
}
export function test_twin_4() {
  twin(4);
}
export function test_twin_5() {
  twin(5);
}

export function test_twin_whole() {
  for (const [vi, li, w, h, rows] of WHOLE) {
    assertEqual(JSON.parse(norm(face(twinView(vi), w, h, look(LOOKS[li])))), rows, `${vi} ${li} ${w}x${h}`);
  }
}
