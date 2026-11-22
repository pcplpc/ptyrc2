// SPDX-License-Identifier: GPL-3.0-or-later
// The note, pure: the markdown reduced (`parse`), the faces (`render`),
// the keys of a focused note (`Desk`), a change made to the text
// (`change`). No host call.
//
// Markdown reduced to what a note needs: `- [ ]` and `- [x]` are boxes,
// `-` and `*` bullets, `#` a heading; emphasis and quote marks go. The
// faces, from `3/7` in five cells to the note wrapped: when the note
// does not fit, without what is done; then the open boxes alone; then
// cut, with `…`.

import { cells } from "ptyrc";
import { Field, Listing, cut, fit } from "ptyrc/deck";
import { foot } from "./foot.js";

export const SIZES = [[5, 1], [18, 1], [24, 3], [30, 6], [40, 10], [48, 16]];
/** The ground of a post-it, its ink, its faded ink; dim, green. */
export const PAPER = 221;
export const INK = 235;
export const FADED = 136;
export const DIM = 8;
const GREEN = 2;
/** A border of blocks in the colour of the paper, the top right corner
 * folded, the title on the top edge. */
export const POSTIT = {
  corners: "█◣▀▀",
  top: "█",
  bottom: "▀",
  left: "█",
  right: "█",
  style: { fg: PAPER },
  title: { side: "top", align: "left", style: { fg: INK, bg: PAPER, bold: true } },
};
const BOX = "☐";
const TICKED = "☑";
const BULLET = "•";
const MARK = { open: BOX, done: TICKED, bullet: BULLET };

/** A line of a note. `kind`: `head`, `open`, `done`, `bullet`, `text` or
 * `blank`; `indent`: its depth in a list; `line`: its line in the file,
 * from 0; `raw`: its words as written there. */
export function item(kind, text = "", indent = 0, line = -1, raw = "") {
  return { kind, text, indent, line, raw };
}

/** What an item is, not where it is written. */
export function same(a, b) {
  return a !== undefined && b !== undefined && a.kind === b.kind && a.text === b.text && a.indent === b.indent;
}

/** `count`: the bar of the boxes; `done`: what is done dimmed; `open`:
 * what is done left out; `postit`: the yellow ground. */
export function look({ count = false, done = false, open = false, postit = false } = {}) {
  return { count, done, open, postit };
}
export const PLAIN = look();

const BOX_RE = /^(\s*)(?:[-*+]|\d+[.)])\s+\[([ xX])\]\s*(.*)$/;
const BULLET_RE = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/;
const HEAD_RE = /^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$/;
const MARKS_RE = /(\*\*|__|~~|`)/g;
const LINK_RE = /\[([^\]]+)\]\([^)]*\)/g;
/** Python's line ends (`str.splitlines`). */
const ENDS = /\r\n|[\n\r\v\f\x1c\x1d\x1e\x85\u2028\u2029]/g;

/** Code points: Python's `len`. */
const len = (t) => [...t].length;

function plain(text) {
  return text.replace(LINK_RE, "$1").replace(MARKS_RE, "").trim();
}

/** `str.splitlines(keepends)`. */
export function splitlines(text, keepends = false) {
  const out = [];
  let at = 0;
  for (const m of text.matchAll(ENDS)) {
    out.push(text.slice(at, keepends ? m.index + m[0].length : m.index));
    at = m.index + m[0].length;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}

function expandtabs(line) {
  let out = "";
  for (const c of line) out += c === "\t" ? " ".repeat(4 - (len(out) % 4)) : c;
  return out;
}

/** The items of a note; runs of blank lines are one, none at the ends. */
export function parse(text) {
  const out = [];
  splitlines(text).forEach((raw, n) => {
    const line = expandtabs(raw);
    let m;
    if (!line.trim()) {
      if (out.length && out[out.length - 1].kind !== "blank") out.push(item("blank", "", 0, n));
    } else if ((m = BOX_RE.exec(line))) {
      out.push(item(m[2] === " " ? "open" : "done", plain(m[3]), Math.floor(m[1].length / 2), n, m[3]));
    } else if ((m = BULLET_RE.exec(line))) {
      out.push(item("bullet", plain(m[2]), Math.floor(m[1].length / 2), n, m[2]));
    } else if ((m = HEAD_RE.exec(line))) {
      out.push(item("head", plain(m[1]), 0, n, m[1]));
    } else {
      let words = line.trimStart();
      if (words.startsWith(">")) words = words.slice(1);
      out.push(item("text", plain(words), 0, n, words.trim()));
    }
  });
  while (out.length && out[out.length - 1].kind === "blank") out.pop();
  return out;
}

/** A note: `name`, `items`; `missing` when it cannot be read. */
export function note(name, text = null) {
  return text === null ? { name, items: [], missing: true } : { name, items: parse(text), missing: false };
}

/** Boxes ticked, boxes. */
export function tally(n) {
  const done = n.items.filter((i) => i.kind === "done").length;
  return [done, done + n.items.filter((i) => i.kind === "open").length];
}

export function title(n, lk = PLAIN) {
  const [done, boxes] = tally(n);
  return lk.count && boxes ? `${n.name} ${done}/${boxes}` : n.name;
}

/** `text` in lines of `w` cells, cut at spaces; a longer word is cut. */
export function wrap(text, w) {
  const lines = [""];
  for (let word of text.split(/\s+/).filter(Boolean)) {
    while (len(word) > w && w > 0) {
      if (lines[lines.length - 1]) lines.push("");
      const cps = [...word];
      lines[lines.length - 1] = cps.slice(0, w).join("");
      word = cps.slice(w).join("");
      lines.push("");
    }
    const last = lines[lines.length - 1];
    if (last && len(last) + 1 + len(word) > w) lines.push("");
    lines[lines.length - 1] += (lines[lines.length - 1] ? " " : "") + word;
  }
  const kept = lines.filter(Boolean);
  return kept.length ? kept : [""];
}

/** `text` in `n` code points at most; cut with `…` (the twin's `clip`). */
export function clip(text, n) {
  const cps = [...text];
  if (cps.length <= n) return text;
  return n > 0 ? cps.slice(0, n - 1).join("") + "…" : "";
}

const width = (row) => row.reduce((s, r) => s + len(r.t), 0);
const blanks = (n) => " ".repeat(Math.max(0, n));

function style(kind, lk) {
  const faded = { fg: lk.postit ? FADED : DIM };
  const base = lk.postit ? { fg: INK, bg: PAPER } : {};
  if (kind === "head") return { ...base, bold: true };
  if ((kind === "done" && lk.done) || kind === "faded") return { ...base, ...faded };
  return base;
}

/** The items laid out: a mark, the text wrapped under itself. */
function lines(items, w, lk) {
  const rows = [];
  for (const it of items) {
    if (it.kind === "blank") {
      rows.push([]);
      continue;
    }
    const mark = MARK[it.kind] ?? "";
    const lead = "  ".repeat(Math.min(it.indent, 2)) + (mark ? mark + " " : "");
    const room = Math.max(4, w - len(lead));
    const st = style(it.kind, lk);
    wrap(it.text, room).forEach((line, n) => rows.push([{ t: (n === 0 ? lead : blanks(len(lead))) + line, ...st }]));
  }
  return rows;
}

/** Each row's first run cut to `w`; on the paper, filled to `w`. */
function ground(rows, w, lk) {
  return rows.map((row) => {
    let out = [...row.slice(0, 1).map((r) => ({ ...r, t: clip(r.t, w) })), ...row.slice(1)];
    if (lk.postit) out = [...out, { t: blanks(w - width(out)), bg: PAPER }];
    return out;
  });
}

/** Python's `round`: half to even. */
function round(x) {
  const r = Math.round(x);
  return Math.abs(x % 1) === 0.5 && r % 2 !== 0 ? r - 1 : r;
}

/** A thin bar of `n` cells, by half cells: `━━━╾───`; a share above zero
 * shows (the twin's `monitor.bar`). */
export function bar(share, n, fg) {
  let halves = round(Math.min(1, Math.max(0, share)) * n * 2);
  if (share > 0 && halves === 0) halves = 1;
  const full = Math.floor(halves / 2);
  const half = halves % 2;
  const out = [{ t: "━".repeat(full) + "╾".repeat(half), fg }, { t: "─".repeat(n - full - half), fg: DIM }].filter((r) => r.t);
  return out.length ? out : [{ t: "", fg: DIM }];
}

/** The items a focused note lists: no blank, and with `lk.open` nothing
 * done. */
export function listed(n, lk = PLAIN) {
  return n.items.filter((i) => i.kind !== "blank" && !(lk.open && i.kind === "done"));
}

/** A key of a move, and its step among the items. */
const MOVES = { "shift+up": -1, K: -1, "shift+down": 1, J: 1 };

/** The keys of a focused note. `key` answers the change to make to the
 * file, `change` makes it. `sel`: the item under the bar, among
 * `listed`. `mode`: null, `add`, `edit` (the line typed is `editor`;
 * `enter` ends it, an empty line or the same words change nothing) or
 * `delete` (asked: `y` does it, any other key keeps). */
export class Desk {
  constructor() {
    this.sel = 0;
    this.mode = null;
    this.editor = new Field();
    this.list = new Listing();
    this.view = new Listing();
  }

  /** Leave what was under way (the focus was lost); whether something
   * was. */
  rest() {
    const was = this.mode;
    this.mode = null;
    return was !== null;
  }

  /** A key; `items`: those listed. Null, or a change: `["tick", line]`,
   * `["add", line, words]` (under that line; -1: at the end), `["words",
   * line, words]`, `["delete", line]`, `["swap", line, line]`. */
  key(items, name) {
    this.sel = Math.max(0, Math.min(items.length - 1, this.sel));
    const it = items[this.sel] ?? null;
    if (this.mode === "add" || this.mode === "edit") {
      const mode = this.mode;
      const words = this.editor.text.trim();
      if (name === "enter") {
        this.mode = null;
        if (words && mode === "add") {
          this.sel += it ? 1 : 0;
          return ["add", it ? it.line : -1, words];
        }
        if (words && it && words !== it.raw) return ["words", it.line, words];
      } else this.editor.key(name);
      return null;
    }
    if (this.mode === "delete") {
      this.mode = null;
      return name === "y" && it ? ["delete", it.line] : null;
    }
    this.list.count = items.length;
    this.list.sel = this.sel;
    if (this.list.key(name)) this.sel = this.list.sel;
    else if (name === "a") {
      this.mode = "add";
      this.editor = new Field();
    } else if (it === null) {
      // Nothing under the bar.
    } else if ((name === "space" || name === "enter") && (it.kind === "open" || it.kind === "done")) {
      return ["tick", it.line];
    } else if (name === "e") {
      this.mode = "edit";
      this.editor = new Field(it.raw);
    } else if (name === "d") this.mode = "delete";
    else if (name in MOVES && this.sel + MOVES[name] >= 0 && this.sel + MOVES[name] < items.length) {
      this.sel += MOVES[name];
      return ["swap", it.line, items[this.sel].line];
    }
    return null;
  }

  paste(text) {
    if (this.mode === "add" || this.mode === "edit") this.editor.insert(text.split("\n").join(" ").trim());
  }

  /** The foot: the keys of the mode, the least useful ranked highest. */
  keys() {
    if (this.mode === "add" || this.mode === "edit") return [["enter ok", 0], ["empty: cancel", 1]];
    if (this.mode === "delete") return [["y delete", 0], ["any key keeps", 1]];
    return [["space tick", 1], ["a add", 2], ["e edit", 3], ["d delete", 4], ["⇧↑↓ move", 5], ["^X close", 6], ["esc leave", 0]];
  }
}

/** A focused note: an item a row, cut; the bar; under it the line being
 * added; a foot of keys from 3 rows. */
function focused(n, w, h, lk, desk) {
  const base = style("text", lk);
  const faded = style("faded", lk);
  const items = listed(n, lk);
  desk.sel = Math.max(0, Math.min(items.length - 1, desk.sel));
  // What each row is: an item, or null for the line added.
  const entries = [...items];
  let at = desk.sel;
  if (desk.mode === "add") {
    at = items.length ? desk.sel + 1 : 0;
    entries.splice(at, 0, null);
  }
  const feet = h >= 3 ? 1 : 0;
  desk.view.count = entries.length;
  desk.view.sel = at;
  let rows = [];
  for (const i of desk.view.window(h - feet)) {
    const entry = entries[i];
    const on = i === at;
    const near = entry ?? items[desk.sel] ?? item("open");
    const kind = entry === null ? "open" : entry.kind;
    const mark = MARK[kind] ?? "";
    const lead = "  ".repeat(Math.min(near.indent, 2)) + (mark ? mark + " " : "");
    const st = style(kind, lk);
    let row;
    if (on && (desk.mode === "add" || desk.mode === "edit")) {
      row = [{ t: lead, ...st }, ...desk.editor.render(Math.max(1, w - len(lead)), true, "", base)];
    } else {
      const text = cut(lead + (entry ? entry.text : ""), w);
      row = [{ t: text + blanks(w - cells(text)), ...st }];
      if (on) row = [{ ...row[0], inverse: true, ...(desk.mode === "delete" ? { bold: true } : {}) }];
    }
    rows.push(row);
  }
  if (!entries.length) rows.push([{ t: cut("empty", w), ...faded }]);
  while (rows.length < h - feet) rows.push([]);
  if (feet) rows.push([{ t: foot(desk.keys(), w), ...faded }]);
  rows = rows.map((row) => fit(row, w).map((r) => (lk.postit ? { ...base, ...r } : r)));
  return rows;
}

/** The rows of runs of `n` in `w` by `h`; `desk`: the note is focused,
 * shown as the list the keys act on. */
export function render(n, w, h, lk = PLAIN, desk = null) {
  if (desk !== null) return focused(n, w, h, lk, desk);
  const base = style("text", lk);
  const faded = style("faded", lk);
  if (n.missing) {
    const text = [`no ${n.name}`, "no file", "—"].find((t) => len(t) <= w) ?? "";
    return ground([[{ t: text, ...faded }]], w, lk);
  }
  const items = n.items.filter((i) => !(lk.open && i.kind === "done"));
  const [done, boxes] = tally(n);
  if (!items.length) return ground([[{ t: clip(boxes ? "all done" : "empty", w), ...faded }]], w, lk);
  if (h === 1) {
    const first = items.find((i) => i.kind === "open") ?? null;
    if (!boxes) {
      const some = items.find((i) => i.kind !== "blank");
      return ground([[{ t: clip(some.text, w), ...base }]], w, lk);
    }
    const row = [{ t: `${done}/${boxes}`, ...base, bold: true }];
    if (w - width(row) - 1 >= 6) {
      row.push({ t: " " + clip(first ? first.text : "all done", w - width(row) - 1), ...(first ? base : faded) });
    }
    return ground([row], w, lk);
  }
  let top = [];
  if (lk.count && boxes && h >= 3) {
    const k = Math.max(3, w - len(` ${done}/${boxes}`));
    const [fg, rest] = lk.postit ? [INK, FADED] : [GREEN, DIM];
    const full = bar(done / boxes, k, fg);
    const runs = full.map((r) => ({ ...r, ...base, fg: r.fg === fg ? fg : rest }));
    runs.push({ t: blanks(k - width(full)), ...base });
    top = [[...runs, { t: ` ${done}/${boxes}`, ...base, bold: true }]];
  }
  const room = h - top.length;
  let rows = lines(items, w, lk);
  // Too long: without what is done; then what is left to do, alone.
  for (const keep of [["head", "open", "bullet", "text", "blank"], ["open"]]) {
    if (rows.length <= room || !boxes) break;
    const still = [];
    for (const it of items) {
      const last = still[still.length - 1];
      if (keep.includes(it.kind) && !(it.kind === "blank" && (!last || last.kind === "blank"))) still.push(it);
    }
    while (still.length && ["blank", "head"].includes(still[still.length - 1].kind)) still.pop();
    if (still.some((i) => i.kind === "open")) rows = lines(still, w, lk);
  }
  if (rows.length > room) {
    rows = rows.slice(0, room);
    const last = rows[rows.length - 1].length ? rows[rows.length - 1] : [{ t: "", ...base }];
    rows[rows.length - 1] = [{ ...last[0], t: clip(last[0].t, w - 2).replace(/…+$/, "") + " …" }];
  }
  return ground([...top, ...rows], w, lk);
}

/** `SIZES` cut to the rows the note fills: one a size, two may be equal.
 * `foot`: a row more from two rows on, for the keys of a focused note. */
export function trimmed(n, lk = PLAIN, withFoot = false) {
  return SIZES.map(([w, h]) => {
    const rows = Math.max(1, Math.min(h, render(n, w, h, lk).length));
    return [w, rows + (withFoot && h > 1 ? 1 : 0)];
  });
}

/** Variants by rank, two of one size allowed. */
export function named(sizes) {
  return Object.fromEntries(sizes.map((s, i) => [`v${i}`, s]));
}

/** The sizes of one widget for every note (`--merged`): by rank, the
 * largest of each. */
export function merged(every) {
  const ranks = Math.max(...every.map((s) => s.length));
  const out = [];
  for (let r = 0; r < ranks; r++) {
    const at = every.map((s) => s[Math.min(r, s.length - 1)]);
    out.push([Math.max(...at.map((s) => s[0])), Math.max(...at.map((s) => s[1]))]);
  }
  return out;
}

// -- written -----------------------------------------------------------------

/** A line of the file: what is written, its end. */
function parts(line) {
  const m = /(\r\n|[\n\r\v\f\x1c\x1d\x1e\x85\u2028\u2029])$/.exec(line);
  return m ? [line.slice(0, m.index), m[1]] : [line, ""];
}

/** A line as what stands before its words, and them. */
function lead(body) {
  for (const [re, group] of [[BOX_RE, 3], [BULLET_RE, 2], [HEAD_RE, 1]]) {
    const m = re.exec(body);
    if (m) return [body.slice(0, start(body, m, group)), m[group]];
  }
  let words = body.trimStart();
  if (words.startsWith(">")) words = words.slice(1).trimStart();
  return [body.slice(0, body.length - words.length), words];
}

/** Where group `g` of a match starts: a box's words and a bullet's run
 * to the end of the line; a heading's follow its `#`s and blanks. */
function start(body, m, g) {
  if (g === 3 || g === 2) return body.length - m[g].length;
  return /^\s{0,3}#{1,6}\s+/.exec(body)[0].length;
}

/** `text` after a change of `Desk.key`. Lines are those of
 * `str.splitlines`; each keeps its end, its indent, its mark. A line
 * added takes the indent and the list mark of the line it is under, as
 * an open box. */
export function change(text, what) {
  const ls = splitlines(text, true);
  const [op] = what;
  let at = what[1];
  let [body, end] = at >= 0 && at < ls.length ? parts(ls[at]) : ["", "\n"];
  if (op === "tick") {
    const m = BOX_RE.exec(body);
    if (m) {
      const i = m[1].length + body.slice(m[1].length).indexOf("[") + 1;
      ls[at] = body.slice(0, i) + (m[2] === " " ? "x" : " ") + body.slice(i + 1) + end;
    }
  } else if (op === "words") ls[at] = lead(body)[0] + what[2] + end;
  else if (op === "delete") ls.splice(at, 1);
  else if (op === "swap") {
    const [other, otherEnd] = parts(ls[what[2]]);
    ls[at] = other + end;
    ls[what[2]] = body + otherEnd;
  } else if (op === "add") {
    const m = BOX_RE.exec(body) ?? BULLET_RE.exec(body);
    let mark = "- ";
    if (m) {
      const gap = body.slice(m[1].length).search(/\s/);
      mark = body.slice(0, m[1].length + gap + 1);
    }
    if (!(at >= 0 && at < ls.length)) {
      at = ls.length - 1;
      if (ls.length && !parts(ls[at])[1]) ls[at] += "\n";
    } else if (!end) {
      ls[at] = body + "\n";
      end = "\n";
    }
    ls.splice(at + 1, 0, `${mark}[ ] ${what[2]}${end || "\n"}`);
  }
  return ls.join("");
}

/** Whether a change may be made to `text`: each line it names is the
 * item it was when `n` was read. */
export function stands(n, text, what) {
  const was = new Map(n.items.map((i) => [i.line, i]));
  const now = new Map(parse(text).map((i) => [i.line, i]));
  return what.slice(1).filter((x) => Number.isInteger(x) && x >= 0).every((x) => same(now.get(x), was.get(x)));
}
