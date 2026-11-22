// SPDX-License-Identifier: GPL-3.0-or-later
// A picture: its file parsed, its frames, its eyes, its rows. Pure.
//
// A picture file:
//
//     # a comment, in the header only
//     name: Pear and fly
//     author: agent
//     kind: still
//     period: 2
//     hold: 8 1
//     eyes: ◐●◑
//     ink: y=220 g=70 k=240/233 s=/52 w=255!
//     == 14x6
//     rows of glyphs
//     ::
//     rows of ink
//     ++
//     rows of glyphs, the next frame
//     == 8x4
//     ...
//
// - `name`; `author` (`anonymous`); `kind`, one of `KINDS`;
// - `period`: seconds a frame (2); `hold`: periods each frame stays (1);
// - `ink`: a key to `fg`, `fg/bg` or `/bg`, colours of the 256; `!`: bold;
// - `== WxH`: a size, the inner box of a variant; then its frames, apart
//   by `++`. A frame is at most H rows of at most W glyphs, each one cell
//   wide; then, after `::`, as many rows of ink keys, a space or `.` for
//   none. Without `::` a frame takes the ink of the one before. Every
//   size has the same number of frames;
// - `eyes`: three glyphs, looking left, ahead, right; or five: up and
//   down too. A cell inked `@` is an eye: it turns to the pointer.

import { cells } from "ptyrc";

export const KINDS = ["portrait", "landscape", "still", "animal", "abstract", "silly"];

const SIZE = /^== (\d+)x(\d+)$/;
const HEAD = ["name", "author", "kind", "period", "hold", "eyes", "ink"];
const PLAIN = Object.freeze({ fg: null, bg: null, bold: false });

/** A picture file that does not parse: `source:line: what`. */
export class PictureError extends Error {}

/** Python's `repr` of a string. */
export function repr(s) {
  const q = s.includes("'") && !s.includes('"') ? '"' : "'";
  let out = "";
  for (const c of s) {
    const n = c.codePointAt(0);
    if (c === "\\" || c === q) out += `\\${c}`;
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (n < 0x20 || n === 0x7f) out += `\\x${n.toString(16).padStart(2, "0")}`;
    else out += c;
  }
  return q + out + q;
}

const glyphsOf = (s) => Array.from(s);
const words = (s) => s.split(/\s+/).filter(Boolean);
const FLOAT = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$|^[+-]?(inf|infinity)$/i;

function float(v) {
  if (!FLOAT.test(v)) return NaN;
  return /inf/i.test(v) ? (v[0] === "-" ? -Infinity : Infinity) : Number(v);
}

/** Not one cell wide: wide, a mark, a control. */
function wide(g) {
  return cells(g) !== 1 || /^[\p{M}\p{C}]/u.test(g);
}

function colour(text, where) {
  if (!text) return null;
  if (!/^\d+$/.test(text) || Number(text) > 255) throw new PictureError(`${where}: colour ${repr(text)}: not one of the 256`);
  return Number(text);
}

function inksOf(value, where) {
  const out = new Map();
  for (const item of words(value)) {
    const at = item.indexOf("=");
    const key = at < 0 ? item : item.slice(0, at);
    if (at < 0 || glyphsOf(key).length !== 1 || key === ".")
      throw new PictureError(`${where}: ink ${repr(item)}: want K=FG, K=FG/BG or K=/BG`);
    const spec = item.slice(at + 1);
    const bold = spec.endsWith("!");
    const bare = spec.replace(/!+$/, "");
    const slash = bare.indexOf("/");
    const fg = slash < 0 ? bare : bare.slice(0, slash);
    const bg = slash < 0 ? "" : bare.slice(slash + 1);
    out.set(key, Object.freeze({ fg: colour(fg, where), bg: colour(bg, where), bold }));
  }
  return out;
}

// Rows of `lines` (`[n, text]`), padded to `w` by `h`: arrays of glyphs.
function block(lines, w, h, source, what) {
  while (lines.length && !lines[lines.length - 1][1]) lines.pop();
  if (lines.length > h) throw new PictureError(`${source}:${lines[h][0]}: ${lines.length} rows of ${what} in a ${w}x${h}`);
  const rows = [];
  for (const [n, row] of lines) {
    const gs = glyphsOf(row);
    if (gs.length > w) throw new PictureError(`${source}:${n}: a row of ${what} of ${gs.length} cells in a ${w}x${h}`);
    const bad = what === "glyphs" ? gs.find(wide) : undefined;
    if (bad !== undefined) throw new PictureError(`${source}:${n}: ${repr(bad)} is not one cell wide`);
    while (gs.length < w) gs.push(" ");
    rows.push(gs);
  }
  while (rows.length < h) rows.push(Array(w).fill(" "));
  return rows;
}

function frameOf(glyphs, ink, [w, h], source, inks, eyes, before) {
  const rows = block(glyphs, w, h, source, "glyphs");
  if (ink === null) return { rows, ink: before.length ? before[before.length - 1].ink : block([], w, h, source, "ink") };
  const keys = block(ink, w, h, source, "ink");
  for (const [n, row] of ink) {
    const bad = glyphsOf(row).find((k) => !inks.has(k) && !" .@".includes(k)) ?? "";
    if (bad || (row.includes("@") && !eyes)) {
      const what = bad ? `ink ${repr(bad)} not in \`ink:\`` : "`@`: no `eyes:`";
      throw new PictureError(`${source}:${n}: ${what}`);
    }
  }
  return { rows, ink: keys };
}

export class Picture {
  constructor(o) {
    this.name = o.name;
    this.author = o.author ?? "anonymous";
    this.kind = o.kind ?? "abstract";
    this.period = o.period ?? 2;
    this.hold = o.hold ?? [];
    this.eyes = o.eyes ?? "";
    this.inks = o.inks ?? new Map();
    /** `"WxH"` -> frames, in the order of the file. */
    this.sizes = o.sizes ?? new Map();
    this.source = o.source ?? "";
    this.memo = new Map();
  }

  /** `[[w, h], ...]`, in the order of the file. */
  get boxes() {
    return [...this.sizes.keys()].map((k) => k.split("x").map(Number));
  }

  get frames() {
    return this.sizes.values().next().value.length;
  }

  /** The file name without its last suffix. */
  get stem() {
    const base = this.source.slice(this.source.lastIndexOf("/") + 1);
    const dot = base.lastIndexOf(".");
    return dot > 0 ? base.slice(0, dot) : base;
  }

  /** The frame shown at `t` seconds. */
  frameAt(t) {
    if (this.frames === 1) return 0;
    const hold = this.hold.length ? this.hold : Array(this.frames).fill(1);
    let beat = Math.floor(Math.max(0, t) / this.period) % hold.reduce((a, b) => a + b, 0);
    for (let nth = 0; nth < hold.length; nth++) {
      if (beat < hold[nth]) return nth;
      beat -= hold[nth];
    }
    return 0;
  }

  /** The eyes of size `w` by `h` in the frame shown at `t`, `look` the
   * cell `[x, y]` of the pointer: `[[x, y, glyph], ...]`, row by row.
   * A cell is twice as high as wide. */
  gaze(w, h, look, t = 0) {
    const out = [];
    if (!this.eyes) return out;
    const e = glyphsOf(this.eyes);
    const [left, ahead, right] = e;
    const [up, down] = e.length === 5 ? e.slice(3) : [ahead, ahead];
    const ink = this.sizes.get(`${w}x${h}`)[this.frameAt(t)].ink;
    for (let y = 0; y < ink.length; y++)
      for (let x = 0; x < ink[y].length; x++) {
        if (ink[y][x] !== "@") continue;
        let glyph = ahead;
        if (look !== null && look !== undefined) {
          const dx = look[0] - x;
          const dy = look[1] - y;
          if (e.length === 5 && dy && Math.abs(dy) * 2 >= Math.abs(dx)) glyph = dy < 0 ? up : down;
          else if (dx) glyph = dx < 0 ? left : right;
        }
        out.push([x, y, glyph]);
      }
    return out;
  }

  /** The rows of runs of size `w` by `h` at `t`. `look`: the eyes turn
   * to that cell; none: the glyphs as drawn. Kept: the same arguments,
   * the same rows. */
  render(w, h, t = 0, look = null) {
    const frames = this.sizes.get(`${w}x${h}`);
    if (!frames) throw new RangeError(`${this.name}: no size ${w}x${h}`);
    const nth = this.frameAt(t);
    const eyes = look === null ? [] : this.gaze(w, h, look, t);
    const key = `${w}x${h}:${nth}:${eyes.map((e) => e[2]).join("")}`;
    const kept = this.memo.get(key);
    if (kept) return kept;
    const at = new Map(eyes.map(([x, y, g]) => [y * w + x, g]));
    const { rows, ink } = frames[nth];
    const out = [];
    for (let y = 0; y < rows.length; y++) {
      const row = [];
      let last = null;
      let run = null;
      for (let x = 0; x < rows[y].length; x++) {
        const k = this.inks.get(ink[y][x]) ?? PLAIN;
        const glyph = at.get(y * w + x) ?? rows[y][x];
        if (last && k.fg === last.fg && k.bg === last.bg && k.bold === last.bold) {
          run.t += glyph;
          continue;
        }
        last = k;
        run = { t: glyph };
        if (k.fg !== null) run.fg = k.fg;
        if (k.bg !== null) run.bg = k.bg;
        if (k.bold) run.bold = true;
        row.push(run);
      }
      out.push(row);
    }
    this.memo.set(key, out);
    return out;
  }

  /** What the plate says. */
  label(author = false) {
    return author ? `${this.name} · ${this.author}` : this.name;
  }
}

/** A picture from the text of its file; a `PictureError` names the line. */
export function parse(text, source = "<picture>") {
  const lines = text.split("\n").map((l) => l.trimEnd());
  const head = new Map();
  let at = 0;
  while (at < lines.length && !SIZE.test(lines[at])) {
    const line = lines[at++];
    if (!line || line.startsWith("#")) continue;
    const colon = line.indexOf(":");
    const key = colon < 0 ? line : line.slice(0, colon);
    if (colon < 0 || !HEAD.includes(key)) throw new PictureError(`${source}:${at}: not a header line: ${repr(line)}`);
    const value = line.slice(colon + 1).trim();
    head.set(key, key === "ink" ? `${head.get(key) ?? ""} ${value}` : value);
    if (key === "period" || key === "hold") {
      const vs = words(value);
      if (!vs.length || !vs.every((v) => float(v) > 0)) throw new PictureError(`${source}:${at}: ${key}: want positive numbers`);
    }
  }
  const where = `${source}: header`;
  if (!head.get("name")) throw new PictureError(`${where}: no name`);
  const kind = head.get("kind") ?? "abstract";
  if (!KINDS.includes(kind)) throw new PictureError(`${where}: kind ${repr(kind)}: one of ${KINDS.join(", ")}`);
  const eyes = (head.get("eyes") ?? "").replaceAll(" ", "");
  const e = glyphsOf(eyes);
  if (![0, 3, 5].includes(e.length) || e.some(wide)) throw new PictureError(`${where}: eyes: want 3 or 5 glyphs`);
  const inks = inksOf(head.get("ink") ?? "", where);
  const holds = words(head.get("hold") ?? "");
  if (!holds.every((v) => /^[+-]?\d+$/.test(v))) throw new PictureError(`${where}: hold: want whole numbers`);
  const hold = holds.map(Number);

  const sizes = new Map();
  while (at < lines.length) {
    const m = SIZE.exec(lines[at]);
    const w = Number(m[1]);
    const h = Number(m[2]);
    const opened = ++at;
    if (w < 1 || h < 1 || sizes.has(`${w}x${h}`)) throw new PictureError(`${source}:${opened}: size ${w}x${h}: null or given twice`);
    const frames = [];
    let glyphs = [];
    let ink = null;
    while (at < lines.length && !SIZE.test(lines[at])) {
      const line = lines[at++];
      if (line === "++") {
        frames.push(frameOf(glyphs, ink, [w, h], source, inks, eyes, frames));
        glyphs = [];
        ink = null;
      } else if (line === "::") {
        if (ink !== null) throw new PictureError(`${source}:${at}: \`::\` twice in a frame`);
        ink = [];
      } else (ink === null ? glyphs : ink).push([at, line]);
    }
    frames.push(frameOf(glyphs, ink, [w, h], source, inks, eyes, frames));
    sizes.set(`${w}x${h}`, frames);
  }
  if (!sizes.size) throw new PictureError(`${source}: no size: want a line \`== WxH\``);
  const counts = [...new Set([...sizes.values()].map((f) => f.length))].sort((a, b) => a - b);
  if (counts.length !== 1) throw new PictureError(`${source}: sizes with [${counts.join(", ")}] frames: want one count`);
  if (hold.length && hold.length !== counts[0]) throw new PictureError(`${where}: hold: one number a frame`);
  return new Picture({
    name: head.get("name"),
    author: head.get("author") || "anonymous",
    kind,
    period: head.has("period") ? float(head.get("period")) : 2,
    hold,
    eyes,
    inks,
    sizes,
    source,
  });
}
