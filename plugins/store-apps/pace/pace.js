// SPDX-License-Identifier: GPL-3.0-or-later
// pace, pure: the keys of a burst, the pace by time (`Meter`), the
// words typed read from the row of the cursor (`Reader`), what looks
// like a word (`wordy`), the counts (`Board`), the faces of a `View`.
// No call: `app.js` gives the time, the screen, the storage.

import { Braille } from "ptyrc/canvas";

/** Seconds of a slot of the history. */
export const SLOT = 10;
/** Seconds of history kept. */
export const SPAN = 1800;
/** Keys in one burst from which it is a paste: not counted. */
export const PASTE = 12;

/** Python's `round`: half to even. */
export function round(x) {
  const f = Math.floor(x);
  const d = x - f;
  if (d !== 0.5) return d < 0.5 ? f : f + 1;
  return f % 2 === 0 ? f : f + 1;
}

const cps = (s) => Array.from(s);
const len = (s) => cps(s).length;

// -- keys --------------------------------------------------------------------

function holds(data, seq) {
  outer: for (let i = 0; i + seq.length <= data.length; i++) {
    for (let j = 0; j < seq.length; j++) if (data[i + j] !== seq[j]) continue outer;
    return true;
  }
  return false;
}

const PASTED = [0x1b, 0x5b, 0x32, 0x30, 0x30, 0x7e];
const SGR_MOUSE = [0x1b, 0x5b, 0x3c];
const X10_MOUSE = [0x1b, 0x5b, 0x4d];

/** The keys of a burst of bytes: a character, a control, an escape
 * sequence are one each; a bracketed paste, a mouse report, `PASTE`
 * keys or more are none. */
export function keys(data) {
  if (holds(data, PASTED) || holds(data, SGR_MOUSE) || holds(data, X10_MOUSE)) return 0;
  let n = 0;
  let i = 0;
  while (i < data.length) {
    const byte = data[i++];
    if (byte === 0x1b && i < data.length && (data[i] === 0x5b || data[i] === 0x4f)) {
      i++;
      while (i < data.length && !(data[i] >= 0x40 && data[i] <= 0x7e)) i++;
      i++;
    } else if (byte === 0x1b && i < data.length) {
      i++; // alt and a key
    } else if (byte >= 0x80 && byte < 0xc0) {
      continue; // inside a character
    }
    n++;
  }
  return n < PASTE ? n : 0;
}

/** Keys counted by time: `add(now, n)`, seconds. Nothing else kept. */
export class Meter {
  constructor(window = 15) {
    this.window = window;
    this.best = 0;
    this.total = 0;
    this.recent = []; // [at, n] of the last minute
    this.slots_ = new Map(); // slot: keys
  }

  add(now, n) {
    if (n <= 0) return;
    this.total += n;
    this.recent.push([now, n]);
    const slot = Math.floor(now / SLOT);
    this.slots_.set(slot, (this.slots_.get(slot) ?? 0) + n);
    for (const s of [...this.slots_.keys()]) if (s < slot - SPAN / SLOT) this.slots_.delete(s);
    this.best = Math.max(this.best, this.rate(now));
  }

  /** Keys of the last minute. */
  minute(now) {
    let k = 0;
    while (k < this.recent.length && this.recent[k][0] <= now - 60) k++;
    if (k) this.recent.splice(0, k);
    return this.recent.reduce((a, [, n]) => a + n, 0);
  }

  /** Keys a minute, over the last `window` seconds. */
  rate(now) {
    this.minute(now);
    let got = 0;
    for (const [at, n] of this.recent) if (at > now - this.window) got += n;
    return round((got * 60) / this.window);
  }

  /** Keys of each slot of the last `SPAN`, oldest first. */
  slots(now) {
    const last = Math.floor(now / SLOT);
    const count = SPAN / SLOT;
    const out = [];
    for (let s = last - count + 1; s <= last; s++) out.push(this.slots_.get(s) ?? 0);
    return out;
  }
}

// -- words -------------------------------------------------------------------

const ALPHA = /^\p{L}$/u;
const isAlpha = (c) => ALPHA.test(c);
const letter = (c) => isAlpha(c) || c === "'" || c === "’";

/** The letters a list of characters ends with. */
function tail(chars) {
  let n = chars.length;
  while (n && letter(chars[n - 1])) n--;
  return chars.slice(n);
}

/** What `text` ends with, back to a blank. */
function token(text) {
  if (!text.trim() || /\s$/u.test(text)) return "";
  const parts = text.split(/\s+/u);
  return parts[parts.length - 1];
}

function stripQuotes(chars) {
  let a = 0;
  let b = chars.length;
  while (a < b && (chars[a] === "'" || chars[a] === "’")) a++;
  while (b > a && (chars[b - 1] === "'" || chars[b - 1] === "’")) b--;
  return chars.slice(a, b);
}

function common(a, b) {
  let n = 0;
  while (n < a.length && n < b.length && a[n] === b[n]) n++;
  return n;
}

const startsWith = (a, b) => b.length <= a.length && common(a, b) === b.length;

/** The words typed, from what the row of the cursor shows.
 *
 * `see(row, text, x, keyed)`: the cursor on `row`, at the cell `x` of
 * `text`; `keyed`: the keys since the last call. It gives the
 * `[word, token]` that ended: a word that grew a cell a key, at least
 * half of it, a separator typed after it, or the cursor gone to a new
 * line. `token`: what stands around it, for `wordy`. More cells than
 * keys: a completion, a paste, a program. */
export class Reader {
  constructor() {
    this.row = null;
    this.left = []; // the row up to the cursor, a character each
    this.typed = 0; // letters of the word under way that were typed
  }

  word() {
    const word = stripQuotes(tail(this.left));
    const typed = this.typed;
    this.typed = 0;
    if (!word.length || 2 * typed < word.length) return null;
    return [word.join(""), token(this.left.join(""))];
  }

  see(row, text, x, keyed) {
    const left = cps(text).slice(0, x);
    while (left.length < x) left.push(" ");
    const out = [];
    const same = row === this.row;
    if (same && left.length === this.left.length && common(left, this.left) === left.length) return [];
    if (same && keyed) {
      // Rubbed out and typed again between two looks: the cells left
      // of the change stay, the rest is rubbed out.
      const keep = common(this.left, left);
      if (keep < this.left.length && keep < left.length) {
        this.typed = Math.min(this.typed, tail(this.left.slice(0, keep)).length);
        this.left = this.left.slice(0, keep);
      }
    }
    if (same && keyed && startsWith(left, this.left)) {
      let more = left.slice(this.left.length);
      if (more.length > keyed) {
        if (!more.every(letter)) this.typed = 0;
        more = [];
      }
      for (const c of more) {
        if (letter(c)) this.typed = tail(this.left).length ? this.typed + 1 : 1;
        else out.push(this.word());
        this.left.push(c);
      }
    } else if (same && startsWith(this.left, left)) {
      this.typed = Math.min(this.typed, tail(left).length); // rubbed out
    } else if (keyed) {
      out.push(this.word());
    } else {
      this.typed = 0;
    }
    this.row = row;
    this.left = left;
    return out.filter((w) => w !== null);
  }
}

const VOWELS = new Set("aeiouy");
/** The 195 pairs of letters English and French words are most made of. */
const PAIRS = new Set(
  `th he in er an re on at en nd ti es or te of ed is it al ar st
  to nt ng se ha as ou io le ve co me de hi ri ro ic ne ea ra ce
  li ch ll be ma si om ur ca el ta la ns di fo ho pe ec pr no ct
  us ac ot il tr ly nc et ut ss so rs un lo wa ge ie wh ee wi em
  ad ol rt po we na ul ni ts mo ow pa im mi ai sh ir su id os iv
  ia am fi ci vi pl ig tu ev ld ry mp fe bl ab gh ty op wo sa ay
  ex ke fr oo av ag if ap gr od bo sp rd do uc bu ei ov by rm ep
  tt oc fa ef cu rn sc gi da yo cr cl du ga qu ue ff ba ey ls va
  um pp ua up lu go ht ru ug ds lt pi rc rr eg au ck br bi pt pu
  ui ib ki eb ob mm`.split(/\s+/),
);

const lower = (s) => s.toLowerCase();
const isLower = (s) => s === s.toLowerCase() && s !== s.toUpperCase();
const isUpper = (s) => s === s.toUpperCase() && s !== s.toLowerCase();

/** Lowered, the accents off. */
export function bare(word) {
  return lower(word).normalize("NFD").replace(/\p{Mn}/gu, "");
}

/** Whether `word` looks like a word of a language, not a piece of a
 * hash, a key, base64, a setting. `token`: what it was cut from.
 *
 * Left out: over 24 letters; no vowel; six consonants or four of one
 * letter in a row; seven letters and more, all of `a` to `f`; mixed
 * case with a hump under three letters, two for the first and the last
 * (`Word`, `WORD`, `camelCase`, `FrameId` pass); from 8 letters,
 * vowels under an eighth or over four fifths, or under half of the
 * pairs among `PAIRS`; a word in a run of 8 letters and digits and more
 * with two numbers in it; a token of 12 letters and digits and more
 * where the kind (lower, upper, digit) changes 3 times in 10 or more. */
export function wordy(word, tok = "") {
  const b = cps(bare(word)).filter(isAlpha).join("");
  const n = len(b);
  const ascii = /^[\x00-\x7f]*$/.test(b);
  if (!(n >= 1 && n <= 24) || !ascii) return n >= 1 && n <= 24 && !ascii; // another script: kept
  let vowels = 0;
  for (const c of b) if (VOWELS.has(c)) vowels++;
  if (!vowels) return false;
  let run = 1;
  let same = 1;
  for (let i = 1; i < n; i++) {
    const [p, c] = [b[i - 1], b[i]];
    same = p === c ? same + 1 : 1;
    run = !VOWELS.has(p) && !VOWELS.has(c) ? run + 1 : 1;
    if (same >= 4 || run >= 6) return false;
  }
  if (n >= 7 && /^[a-f]*$/.test(b)) return false;
  // Case: `word`, `WORD`, `Word`, or humps of three letters and more.
  if (!(isLower(word) || isUpper(word) || isLower(cps(word).slice(1).join("")))) {
    const humps = word.match(/[A-Z]+[a-z]*|[a-z]+/g) ?? [];
    if (humps.length) {
      const ends = Math.min(humps[0].length, humps[humps.length - 1].length) < 2;
      if (ends || humps.slice(1, -1).some((h) => h.length < 3)) return false;
    }
  }
  if (n >= 8) {
    if (!(vowels / n >= 0.125 && vowels / n <= 0.8)) return false;
    let pairs = 0;
    for (let i = 1; i < n; i++) if (PAIRS.has(b[i - 1] + b[i])) pairs++;
    if (2 * pairs < n - 1) return false;
  }
  // The token: digits next to the word, or kinds that change all the time.
  const alnum = (c) => /^[\p{L}\p{N}]$/u.test(c);
  const runs = cps(tok)
    .map((c) => (alnum(c) ? c : " "))
    .join("")
    .split(" ")
    .filter(Boolean);
  const near = runs.find((t) => t.includes(word)) ?? "";
  if (len(near) >= 8 && (near.match(/\p{Nd}+/gu) ?? []).length >= 2) return false;
  const kinds = cps(tok)
    .filter(alnum)
    .map((c) => (/^\p{Nd}$/u.test(c) ? 1 : 0) + (/^\p{Lu}$/u.test(c) ? 2 : 0));
  let turns = 0;
  for (let i = 1; i < kinds.length; i++) if (kinds[i] !== kinds[i - 1]) turns++;
  return !(kinds.length >= 12 && turns >= 0.3 * kinds.length);
}

/** The words of a file of words: blanks between them. */
export function wordsOf(text) {
  return text.split(/\s+/u).filter(Boolean);
}

/** The count of each word, and when it was last typed (seconds of the epoch). */
export class Board {
  constructor(least = 4, stop = []) {
    this.least = least;
    this.stop = new Set(stop.map(bare));
    this.words = new Map(); // word: [count, last]
    this.dirty = false;
    this.version = 0; // one more at each change
    this.kept = null; // the top of `top(n)`: {n, list}
    this.parts = Array.from({ length: PARTS }, () => new Set()); // the words of each part
    this.touched = new Set(); // the parts changed since `dump`
  }

  /** Count `word`, unless it is short, common or not a word. */
  add(word, tok = "", at = 0) {
    const low = lower(word);
    if (len(low) < this.least || this.stop.has(bare(low)) || !wordy(word, tok)) return false;
    this.words.set(low, [(this.words.get(low)?.[0] ?? 0) + 1, at]);
    this.dirty = true;
    this.version++;
    const p = part(low);
    this.parts[p].add(low);
    this.touched.add(p);
    if (this.kept) {
      const list = this.kept.list.filter(([w]) => w !== low);
      list.push([low, this.words.get(low)]);
      this.kept.list = ranked(list, this.kept.n);
    }
    return true;
  }

  /** The `n` words typed most, `[word, count, last]`; of two as many
   * times, the last typed first. Kept for `n`: an `add` changes one
   * count, the top of before and that word make the top after. */
  top(n) {
    if (this.kept?.n !== n) this.kept = { n, list: ranked([...this.words], n) };
    return this.kept.list.map(([w, [c, l]]) => [w, c, l]);
  }

  /** Adds the counts of a `dump`; what is not one adds nothing more. */
  load(text) {
    try {
      const words = JSON.parse(text).words;
      for (const [word, [count, last]] of Object.entries(words)) {
        const c = Math.trunc(Number(count));
        const l = Number(last);
        if (!Number.isFinite(c) || Number.isNaN(l)) throw new TypeError("not a count");
        const mine = this.words.get(word) ?? [0, 0];
        this.words.set(word, [mine[0] + c, Math.max(mine[1], l)]);
        this.version++;
        this.kept = null;
        this.parts[part(word)].add(word);
      }
    } catch (e) {
      if (!(e instanceof SyntaxError || e instanceof TypeError)) throw e;
    }
  }

  /** The counts as JSON, `{"words": {word: [count, last]}}`: of the
   * part `p` (0 to `PARTS - 1`), or all. Over `most` bytes (3 a code
   * unit at most), the words typed longest ago are dropped from the
   * board until it fits, those of `spare` kept: the counts shown stay
   * whole, and what is shown is what is kept. */
  dump(p = null, most = Infinity, spare = new Set()) {
    const of = () => (p === null ? [...this.words] : [...this.parts[p]].map((w) => [w, this.words.get(w)]));
    const text = JSON.stringify({ words: Object.fromEntries(of()) });
    if (text.length * 3 <= most) return text;
    const old = of()
      .filter(([w]) => !spare.has(w))
      .sort(([a, [ca, la]], [b, [cb, lb]]) => la - lb || ca - cb || (a < b ? -1 : a > b ? 1 : 0));
    // An entry is `"word":[count,last]` and a comma.
    let over = text.length - Math.floor(most / 3);
    for (const [w, entry] of old) {
      if (over <= 0) break;
      over -= JSON.stringify(w).length + JSON.stringify(entry).length + 2;
      this.words.delete(w);
      this.parts[part(w)].delete(w);
      this.touched.add(part(w));
    }
    this.version++;
    this.kept = null;
    return JSON.stringify({ words: Object.fromEntries(of()) });
  }
}

/** Parts of a board, a value of the storage each: a change rewrites one. */
export const PARTS = 16;

/** The part of a word: its FNV-1a hash, modulo `PARTS`. */
export function part(word) {
  let h = 0x811c9dc5;
  for (const c of word) h = Math.imul(h ^ c.codePointAt(0), 0x01000193) >>> 0;
  return h % PARTS;
}

/** `[word, [count, last]]` entries, the `n` first by count, then last, then word. */
function ranked(entries, n) {
  return entries
    .sort(([a, [ca, la]], [b, [cb, lb]]) => cb - ca || lb - la || (a < b ? -1 : a > b ? 1 : 0))
    .slice(0, n);
}

// -- the faces ---------------------------------------------------------------

/** What the options ask of a face. */
export function look(o = {}) {
  return { graph: false, best: false, words: false, bars: false, colour: false, top: 10, ...o };
}

/** What a face shows: the pace now, the highest, keys of each slot
 * (oldest first), the top as `[word, count, seconds ago]`. */
export function view(rate = 0, best = 0, slots = [], top = []) {
  return { rate, best, slots, top };
}

const BRIGHT_YELLOW = 11;
const BRIGHT_CYAN = 14;
/** Colours of the 256, with `-c`: a rank of the board each. */
export const RANKS = [220, 214, 208, 209, 204, 170, 141, 105, 75, 80];
/** The rows of the graph, the lowest first. */
export const GRAPH = [37, 44, 87];

function run(t, fg = null, style = {}) {
  const out = { t };
  if (style.bold) out.bold = true;
  if (style.dim) out.dim = true;
  if (fg !== null) out.fg = fg;
  return out;
}

const width = (row) => row.reduce((a, r) => a + len(r.t), 0);

/** `now`, `40s ago`, `3m ago`, `2h ago`, `5d ago`. */
export function ago(seconds) {
  if (seconds < 5) return "now";
  for (const [unit, size] of [["d", 86400], ["h", 3600], ["m", 60]]) {
    if (seconds >= size) return `${Math.floor(seconds / size)}${unit} ago`;
  }
  return `${Math.trunc(seconds)}s ago`;
}

/** The history in braille, `w` cells by `h`: two columns a cell, the
 * newest on the right, each the mean of its share of the time; full
 * height the highest; a column one dot at least. */
export function graph(slots, w, h, lk) {
  const columns = 2 * w;
  const values = [];
  for (let i = 0; i < columns; i++) {
    const a = Math.floor((i * slots.length) / columns);
    const b = Math.floor(((i + 1) * slots.length) / columns);
    const part = slots.slice(a, Math.max(b, a + 1));
    values.push(part.length ? part.reduce((s, v) => s + v, 0) / part.length : 0);
  }
  const high = Math.max(0, ...values) || 1;
  const canvas = new Braille(w, h);
  values.forEach((value, x) => {
    const n = 1 + round((value / high) * (4 * h - 1));
    for (let y = 0; y < n; y++) canvas.set(x, 4 * h - 1 - y);
  });
  return canvas.rows().map((text, y) => {
    const fg = lk.colour ? GRAPH[Math.min(h - 1 - y, GRAPH.length - 1)] : null;
    return [run(text.padEnd(w), fg, { dim: !lk.colour })];
  });
}

/** `312 k/min ▲ 345  62 wpm`, as much as fits. */
function head(v, w, lk) {
  const top = v.best && v.rate >= 0.95 * v.best;
  const fg = lk.colour ? (top ? BRIGHT_YELLOW : v.rate ? BRIGHT_CYAN : null) : null;
  const row = [run(String(v.rate), fg, { bold: !!v.rate, dim: !v.rate })];
  if (w >= width(row) + 6) row.push(run(" k/min", null, { dim: true }));
  const best = [run(" ▲ ", lk.colour ? BRIGHT_YELLOW : null), run(String(v.best))];
  if (lk.best && w >= width(row) + width(best)) row.push(...best);
  const wpm = [run(`${round(v.rate / 5)} wpm`, null, { dim: true })];
  if (w >= width(row) + width(wpm) + 2) row.push(run(" ".repeat(w - width(row) - width(wpm))), ...wpm);
  return row;
}

function clip(text, n) {
  const c = cps(text);
  return c.length <= n ? text : c.slice(0, Math.max(0, n - 1)).join("") + "…";
}

const padEnd = (s, n) => s + " ".repeat(Math.max(0, n - len(s)));
const padStart = (s, n) => " ".repeat(Math.max(0, n - len(s))) + s;

const EIGHTHS = " ▏▎▍▌▋▊▉█";

/** A bar of `n` cells, an eighth of a cell exact; never empty. */
function bar(share, n) {
  const eighths = Math.max(1, round(share * 8 * n));
  return padEnd("█".repeat(eighths >> 3) + EIGHTHS[eighths % 8].trim(), n);
}

/** The board, `h` rows: rank, word, a bar with `-r`, count; the row
 * `over`, under the pointer: the word, its count, when last typed. */
function board(v, w, h, lk, over) {
  const top = v.top.slice(0, h);
  if (!top.length) return [[run(clip("no word yet", w), null, { dim: true })]];
  const ranks = String(top.length).length;
  const counts = Math.max(...top.map(([, c]) => String(c).length));
  const room = w - ranks - counts - 2;
  let name = Math.min(room, Math.max(...top.map(([word]) => len(word))));
  if (lk.bars) name = Math.min(name, Math.max(4, room - 5, Math.floor((room * 3) / 5)));
  const barN = lk.bars ? room - name - 1 : 0;
  return top.map(([word, count, last], i) => {
    const fg = lk.colour ? RANKS[Math.min(i, RANKS.length - 1)] : null;
    const rank = run(`${padStart(String(i + 1), ranks)} `, null, { dim: true });
    if (i === over) {
      const tip = `×${count} · ${ago(last)}`;
      const text = clip(word, Math.max(1, w - ranks - 2 - len(tip)));
      const gap = " ".repeat(Math.max(1, w - ranks - 1 - len(text) - len(tip)));
      return [rank, run(text, fg, { bold: true }), run(gap + tip)];
    }
    const row = [rank, run(padEnd(clip(word, name), name), fg, { bold: i === 0 })];
    if (barN >= 2) {
      row.push(run(" " + bar(count / top[0][1], barN), fg, { dim: !lk.colour }));
      row.push(run(` ${padStart(String(count), counts)}`));
    } else {
      row.push(run(padStart(String(count), w - ranks - 1 - name)));
    }
    return row;
  });
}

/** Rows of the graph in a face `h` high. */
function graphRows(h, lk) {
  if (!lk.graph || h < 2) return 0;
  return h < 3 || (lk.words && h < 7) ? 1 : 2;
}

function cutRow(row, w) {
  const out = [];
  for (const r of row) {
    const t = cps(r.t)
      .slice(0, w - width(out))
      .join("");
    if (t) out.push({ ...r, t });
  }
  return out;
}

/** A face of `w` by `h`: the pace; under it the graph (`-g`), then the
 * board (`-w`), as many words as rows are left; one row with `-w`,
 * from 20 cells: the pace and the word typed most. `hover`: the cell
 * `[x, y]` under the pointer, or null. Cut to the box. */
export function face(v, w, h, lk, hover = null) {
  return faces(v, w, h, lk, hover)
    .slice(0, h)
    .map((row) => cutRow(row, w));
}

function faces(v, w, h, lk, hover) {
  if (h === 1) {
    let row = head(v, w, lk);
    if (lk.words && v.top.length && w >= 20) {
      row = head(v, Math.min(w, 9), look({ colour: lk.colour }));
      const fg = lk.colour ? RANKS[0] : null;
      const word = clip(v.top[0][0], w - width(row) - 3);
      row.push(run(" ♥ ", fg), run(word, null, { bold: true }));
    }
    return [row];
  }
  const rows = [head(v, w, lk)];
  const g = graphRows(h, lk);
  if (g) rows.push(...graph(v.slots, w, g, lk));
  if (lk.words && h > rows.length) {
    const first = rows.length;
    const over = hover === null ? null : hover[1] - first;
    rows.push(...board(v, w, h - first, lk, over));
  }
  return rows;
}

/** The sizes of the faces of a look, smallest first. */
export function ladder(lk) {
  const sizes = [[3, 1], [9, 1]];
  if (lk.best) sizes.push([16, 1]);
  if (lk.words) sizes.push([24, 1]);
  if (lk.graph) sizes.push([12, 2], [20, 3]);
  if (lk.words) {
    const hd = lk.graph ? 3 : 1;
    const wide = lk.bars ? 6 : 0;
    for (const [n, w] of [[3, 20], [5, 22], [10, 26]]) {
      if (n <= lk.top || n === 3) sizes.push([w + wide, hd + Math.min(n, lk.top)]);
    }
    if (![3, 5, 10].includes(lk.top)) sizes.push([26 + wide, hd + lk.top]);
  }
  const seen = new Map(sizes.map((s) => [`${s[0]}x${s[1]}`, s]));
  return [...seen.values()].sort((a, b) => a[0] * a[1] - b[0] * b[1] || a[0] - b[0] || a[1] - b[1]);
}
