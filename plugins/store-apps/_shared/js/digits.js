// SPDX-License-Identifier: GPL-3.0-or-later
// Block digits, and rows of runs laid out in a box: the faces of clock
// and timer. Pure. A row is a list of runs `{t, fg, ...}`; a font, 11
// glyphs (0 to 9, the colon), each a list of strings of equal width.

import { cells } from "ptyrc";

// Digits 0 to 9 and the colon, in dots: `#` on. Two dots a cell high.
const TABLE = [
  "### .#. ### ### #.# ### ### ### ### ### .",
  "#.# ##. ..# ..# #.# #.. #.. ..# #.# #.# #",
  "#.# .#. ### ### ### ### ### ..# ### ### .",
  "#.# .#. #.. ..# ..# ..# #.# ..# #.# ..# #",
  "### ### ### ### ..# ### ### ..# ### ### .",
];

function glyphs(table) {
  const cols = table.map((row) => row.split(" "));
  return cols[0].map((_, i) => cols.map((row) => row[i]));
}

/** `glyph` twice as large, the outer corner of each bend cut by one dot. */
function double(glyph) {
  const h = glyph.length;
  const w = glyph[0].length;
  const on = (y, x) => y >= 0 && y < h && x >= 0 && x < w && glyph[y][x] === "#";
  const out = Array.from({ length: 2 * h }, () => new Array(2 * w).fill("."));
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (!on(y, x)) continue;
      for (const dy of [0, 1]) {
        for (const dx of [0, 1]) {
          const v = y + 2 * dy - 1;
          const u = x + 2 * dx - 1;
          const bend = on(2 * y - v, x) && on(y, 2 * x - u);
          const cut = bend && !(on(v, x) || on(y, u) || on(v, u));
          out[2 * y + dy][2 * x + dx] = cut ? "." : "#";
        }
      }
    }
  }
  return out.map((row) => row.join(""));
}

/** Dots to half blocks: two rows of dots a row of cells. */
function halves(glyph) {
  const rows = glyph.length % 2 ? [...glyph, ".".repeat(glyph[0].length)] : glyph;
  const out = [];
  for (let y = 0; y < rows.length; y += 2) {
    let s = "";
    for (let x = 0; x < rows[y].length; x++) s += " ▄▀█"[2 * (rows[y][x] === "#") + (rows[y + 1][x] === "#")];
    out.push(s);
  }
  return out;
}

const small = glyphs(TABLE);
/** Digits 0 to 9 and `:`: 3 cells by 3, half blocks. */
export const SMALL = small.map(halves);
/** Digits 0 to 9 and `:`: 6 cells by 5, half blocks, corners cut. */
export const LARGE = [
  ...small.slice(0, 10).map((g) => halves(double(g))),
  halves(["..", "..", "##", "##", "..", "..", "##", "##", "..", ".."]),
];

/** The digits of `rows`, block digits of `font` one column apart; `:`
 * for a colon, `?` for what is not a glyph of it. */
export function decode(rows, font) {
  let out = "";
  let x = 0;
  while (x < rows[0].length) {
    if (rows.every((row) => row[x] === " ")) {
      x += 1;
      continue;
    }
    const i = font.findIndex((g) => g.every((line, y) => rows[y].slice(x, x + line.length) === line));
    if (i < 0) return out + "?";
    out += "0123456789:"[i];
    x += font[i][0].length;
  }
  return out;
}

/** Columns of a row. */
export function width(row) {
  return row.reduce((n, run) => n + cells(run.t), 0);
}

/** `text` (digits and `:`) in block digits, a column apart, as many
 * about a colon as it is wide; `colors`: one a character. */
export function blocks(text, font, colors) {
  const rows = font[0].map(() => []);
  const chars = Array.from(text);
  chars.forEach((char, i) => {
    const glyph = font[char === ":" ? 10 : Number(char)];
    const colon = char === ":" || chars[i - 1] === ":";
    const gap = " ".repeat(colon ? font[10][0].length : 1);
    rows.forEach((row, y) => {
      if (i) row.push({ t: gap });
      row.push({ t: glyph[y], fg: colors[i] });
    });
  });
  return rows;
}

/** `right` after `left`, `gap` columns apart, bottom lines level. */
export function beside(left, right, gap) {
  const w = Math.max(...left.map(width));
  const lifted = [...Array.from({ length: left.length - right.length }, () => []), ...right];
  return left.map((a, y) => {
    const b = lifted[y];
    return b.length ? [...a, { t: " ".repeat(w - width(a) + gap) }, ...b] : a;
  });
}

/** `row` cut to `w` columns. */
export function clip(row, w) {
  const out = [];
  for (const run of row) {
    let t = "";
    for (const ch of run.t) {
      const n = cells(ch);
      if (n > w) break;
      t += ch;
      w -= n;
    }
    if (t) out.push({ ...run, t });
  }
  return out;
}

/** Groups of rows one under the other, each centred across, the whole
 * centred down; what exceeds `w` by `h` is cut. */
export function fit(groups, w, h) {
  let rows = [];
  for (const group of groups) {
    const pad = Math.max(0, (w - Math.max(...group.map(width))) >> 1);
    for (const row of group) rows.push(pad && row.length ? [{ t: " ".repeat(pad) }, ...row] : row);
  }
  const top = Math.max(0, (h - rows.length) >> 1);
  rows = [...Array.from({ length: top }, () => []), ...rows];
  return rows.slice(0, h).map((row) => clip(row, w));
}
