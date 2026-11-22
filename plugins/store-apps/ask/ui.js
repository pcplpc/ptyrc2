// SPDX-License-Identifier: GPL-3.0-or-later
// The models of a question (the twin's `ptyrc.ui`: `Model`, `Line`,
// `Text`, `Choice`; `foot` is `_shared/js/foot.js`). Pure: keys in, rows
// of runs out, for a given size; no clock, no host call.
//
// Keys are named as the driver names them: a character is itself, then
// `space`, `enter`, `tab`, `up`, `ctrl+a`, `alt+enter`... The escape
// key (esc) and the close key (ctrl+x) never come: the driver takes
// them.

import { cells } from "ptyrc";
import { Field, Listing, cut, fit, wrap } from "ptyrc/deck";
import { foot } from "./foot.js";

export { foot };

/** What `key` answers when the model is over. */
export const DONE = "done";

/** A run of a row; false styles left out. */
export function run(t, style = {}) {
  const out = { t };
  for (const [k, v] of Object.entries(style)) if (v) out[k] = v;
  return out;
}

/** A key as a foot shows it: `ctrl+x` is `^X`. */
export function caret(key) {
  return key.startsWith("ctrl+") && key.length === 6 ? "^" + key.slice(5).toUpperCase() : key;
}

/** `↑`, `↓` on the first and last rows of `listing` shown in `h` rows,
 * when rows hide beyond them. */
export function arrow(listing, i, h) {
  const shown = listing.window(h);
  if (!shown.length) return "";
  if (i === shown[0] && i > 0) return "↑";
  if (i === shown[shown.length - 1] && i < listing.count - 1) return "↓";
  return "";
}

/** A screen: the question, the rows, a foot line of keys. A kind gives
 * `body(w, h, focused)`, `key(name)`, `natural()`, and as needed
 * `paste`, `keys`, `note`. `render` is exact at every size: `h` rows of
 * `w` cells. One row: the body when focused, else the question. Two: no
 * foot. `result` is the answer once `key` said `DONE`. */
export class Model {
  question = "";
  result = null;
  escape = "esc";
  close = "ctrl+x";
  hint = [["ctrl+t ctrl+t", 0], ["to answer", 1]];

  key() {
    return null;
  }
  paste() {}
  body() {
    return [];
  }
  natural() {
    return [24, 1];
  }
  keys() {
    const out = [["enter ok", 1]];
    if (this.escape) out.push([`${this.escape} later`, 0]);
    if (this.close) out.push([`${caret(this.close)} close`, 4]);
    return out;
  }
  note() {
    return "";
  }

  /** Inner sizes of the variants, the best first. */
  sizes() {
    let [w, h] = this.natural();
    w = Math.min(Math.max(w, cells(this.question) + 1, 24), 78);
    h = Math.min(h, 20);
    const all = [[w, h + 2], [Math.max(Math.floor((w * 2) / 3), 16), Math.min(h, 4) + 2], [Math.min(w, 24), 3], [Math.min(w, 18), 1]];
    const seen = new Set();
    return all.filter(([a, b]) => !seen.has(`${a}x${b}`) && seen.add(`${a}x${b}`));
  }

  render(w, h, focused) {
    w = Math.max(w, 1);
    h = Math.max(h, 1);
    const feet = h >= 3 ? 1 : 0;
    let head = [];
    if (this.question && (h >= 2 || !focused)) {
      head = wrap(this.question, w, Math.max(1, Math.floor((h - feet) / 3)));
      if (!head.length) head = [""];
    }
    const room = h - head.length - feet;
    let rows = head.map((line) => [run(line, { bold: true })]);
    const mark = room > 0 ? this.note(room) : "";
    if (rows.length && mark && cells(mark) + 2 <= w) {
      const first = cut(head[0], w - cells(mark) - 1);
      const gap = w - cells(first) - cells(mark);
      rows[0] = [run(first, { bold: true }), run(" ".repeat(gap)), run(mark, { dim: true })];
    }
    rows = rows.concat(this.body(w, room, focused).slice(0, Math.max(room, 0)));
    while (rows.length < h - feet) rows.push([]);
    if (feet) rows.push([run(foot(focused ? this.keys() : this.hint, w), { dim: true })]);
    return rows.slice(0, h).map((row) => fit(row, w));
  }
}

/** One line typed. `result`: the text. */
export class Line extends Model {
  constructor(question = "", { text = "", placeholder = "" } = {}) {
    super();
    this.question = question;
    this.placeholder = placeholder;
    this.field = new Field(text);
  }
  key(name) {
    if (name === "enter") {
      this.result = this.field.text;
      return DONE;
    }
    this.field.key(name);
    return null;
  }
  paste(text) {
    this.field.insert(text.replace(/\n+$/, ""));
  }
  body(w, h, focused) {
    return [this.field.render(w, focused, this.placeholder)];
  }
  natural() {
    return [Math.max(32, cells(this.field.text) + 2), 1];
  }
}

/** Several lines typed. `enter` validates; `alt+enter` or `ctrl+j` is a
 * new line; a paste keeps its lines. `result`: the lines parted by
 * `\n`. */
export class Text extends Model {
  #top = 0;
  constructor(question = "", { text = "" } = {}) {
    super();
    this.question = question;
    this.lines = text.split("\n").map((l) => new Field(l));
    this.row = this.lines.length - 1;
  }
  get text() {
    return this.lines.map((l) => l.text).join("\n");
  }
  #go(row, cursor) {
    this.row = row;
    this.lines[row].cursor = Math.min(cursor, this.lines[row].text.length);
  }
  #newline() {
    const line = this.lines[this.row];
    const rest = new Field(line.text.slice(line.cursor));
    rest.cursor = 0;
    line.text = line.text.slice(0, line.cursor);
    this.lines.splice(this.row + 1, 0, rest);
    this.row += 1;
  }
  #join(row) {
    const line = this.lines[row];
    const [after] = this.lines.splice(row + 1, 1);
    line.cursor = line.text.length;
    line.text += after.text;
    this.row = row;
  }
  key(name) {
    const line = this.lines[this.row];
    const last = this.lines.length - 1;
    const end = line.cursor === line.text.length;
    if (name === "enter") {
      this.result = this.text;
      return DONE;
    }
    if (name === "alt+enter" || name === "ctrl+j") this.#newline();
    else if (name === "up" || name === "down") {
      if (name === "up" && this.row > 0) this.#go(this.row - 1, line.cursor);
      if (name === "down" && this.row < last) this.#go(this.row + 1, line.cursor);
    } else if ((name === "backspace" || name === "ctrl+h") && line.cursor === 0) {
      if (this.row > 0) this.#join(this.row - 1);
    } else if ((name === "delete" || name === "ctrl+d") && end) {
      if (this.row < last) this.#join(this.row);
    } else if (name === "left" && line.cursor === 0 && this.row > 0) this.#go(this.row - 1, this.lines[this.row - 1].text.length);
    else if (name === "right" && end && this.row < last) this.#go(this.row + 1, 0);
    else line.key(name);
    return null;
  }
  paste(text) {
    const [first, ...rest] = text.split("\n");
    this.lines[this.row].insert(first);
    for (const piece of rest) {
      this.#newline();
      this.lines[this.row].insert(piece);
    }
  }
  body(w, h, focused) {
    h = Math.max(h, 1);
    this.#top = Math.max(Math.min(this.#top, this.row), this.row - h + 1, 0);
    const out = [];
    for (let i = this.#top; i < Math.min(this.lines.length, this.#top + h); i++) out.push(this.lines[i].render(w, focused && i === this.row));
    return out;
  }
  note(h) {
    const n = this.lines.length;
    return n > Math.max(h, 1) ? `${this.row + 1}/${n}` : "";
  }
  keys() {
    return [["enter ok", 1], ["alt+enter new line", 2], ...super.keys().slice(1)];
  }
  natural() {
    const width = Math.max(...this.lines.map((l) => cells(l.text))) + 2;
    return [Math.max(40, width), Math.max(4, this.lines.length)];
  }
}

/** One of `options`, or with `multi` some of them. Arrows, `j` `k`,
 * `tab`, page keys move the bar; a digit goes to that row; `space` ticks
 * with `multi`; `enter` validates. `other`: a last row typed in place
 * (there letters and digits are typed). `result`: the option (the text
 * typed for `other`), or with `multi` the list of those ticked, the
 * text of `other` last when not empty. */
export class Choice extends Model {
  constructor(question = "", options = [], { multi = false, other = false } = {}) {
    super();
    this.question = question;
    this.options = [...options];
    this.multi = multi;
    this.other = other ? new Field() : null;
    this.ticked = new Set();
    this.listing = new Listing(this.options.length + (other ? 1 : 0));
  }
  get typing() {
    return this.other !== null && this.listing.sel === this.options.length;
  }
  key(name) {
    const sel = this.listing.sel;
    if (name === "enter") {
      const typed = this.other?.text ?? "";
      if (this.multi) {
        const picked = [...this.ticked].sort((a, b) => a - b).map((i) => this.options[i]);
        this.result = typed ? [...picked, typed] : picked;
      } else if (this.typing) {
        if (!typed) return null;
        this.result = typed;
      } else if (this.options.length) this.result = this.options[sel];
      else return null;
      return DONE;
    }
    if (this.typing) {
      const keep = name === "home" || name === "end";
      if (!(!keep && this.listing.key(name, false))) this.other.key(name);
      return null;
    }
    if (this.listing.key(name)) return null;
    if (/^[0-9]$/.test(name) && Number(name) >= 1 && Number(name) <= this.listing.count) this.listing.sel = Number(name) - 1;
    else if (name === "space" && this.multi && sel < this.options.length) {
      if (this.ticked.has(sel)) this.ticked.delete(sel);
      else this.ticked.add(sel);
    }
    return null;
  }
  paste(text) {
    if (this.typing) this.other.insert(text.replace(/\n+$/, ""));
  }
  body(w, h, focused) {
    const rows = [];
    for (const i of this.listing.window(h)) {
      const on = focused && i === this.listing.sel;
      const digit = i < 9 ? String(i + 1) : " ";
      let row = [run(digit + " ", { dim: !on, inverse: on })];
      let room = w - 2;
      if (this.multi && i < this.options.length) {
        row.push(run(this.ticked.has(i) ? "[x] " : "[ ] ", { inverse: on }));
        room -= 4;
      }
      const up = arrow(this.listing, i, h);
      room -= up ? 2 : 0;
      if (i < this.options.length) {
        row.push(run(cut(this.options[i], room), { inverse: on }));
        row = fit(row, w - (up ? 2 : 0));
        if (on) row[row.length - 1] = { ...row[row.length - 1], inverse: true };
      } else if (this.other !== null) row = row.concat(this.other.render(room, on, "other…"));
      if (up) row.push(run(" "), run(up, { dim: true }));
      rows.push(row);
    }
    return rows;
  }
  note(h) {
    return this.listing.mark(h);
  }
  keys() {
    const out = [["↑↓ move", 2]];
    if (this.multi) out.push(["space tick", 2]);
    return out.concat(super.keys());
  }
  natural() {
    let width = Math.max(0, ...this.options.map((o) => cells(o)));
    width += this.multi ? 6 : 2;
    return [Math.max(width, this.other !== null ? 24 : 0), this.listing.count];
  }
}
