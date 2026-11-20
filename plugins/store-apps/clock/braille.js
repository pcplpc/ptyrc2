// SPDX-License-Identifier: GPL-3.0-or-later
// Braille dots: `2w` by `4h` dots, on or off, in `w` by `h` cells; the
// twin of `_canvas.Braille`. Pure; `x` right, `y` down, from 0; a dot
// outside is ignored.

/** The bit of each dot of a cell, by row and column. */
export const DOTS = [
  [0x01, 0x08],
  [0x02, 0x10],
  [0x04, 0x20],
  [0x40, 0x80],
];

export class Braille {
  constructor(w, h) {
    this.w = w;
    this.h = h;
    /** The bits of each cell, a row of numbers a row of cells. */
    this.bits = Array.from({ length: h }, () => new Array(w).fill(0));
  }

  set(x, y, on = true) {
    if (x < 0 || y < 0 || x >= 2 * this.w || y >= 4 * this.h) return;
    const bit = DOTS[y & 3][x & 1];
    if (on) this.bits[y >> 2][x >> 1] |= bit;
    else this.bits[y >> 2][x >> 1] &= ~bit;
  }

  get(x, y) {
    if (x < 0 || y < 0 || x >= 2 * this.w || y >= 4 * this.h) return false;
    return (this.bits[y >> 2][x >> 1] & DOTS[y & 3][x & 1]) !== 0;
  }

  /** Every dot from `(x0, y0)` to `(x1, y1)`. */
  line(x0, y0, x1, y1) {
    const n = Math.max(1, Math.round(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))));
    for (let i = 0; i <= n; i++) {
      this.set(Math.round(x0 + ((x1 - x0) * i) / n), Math.round(y0 + ((y1 - y0) * i) / n));
    }
  }

  /** The cell `(cx, cy)`; `""` when no dot of it is on. */
  glyph(cx, cy) {
    const bits = cx >= 0 && cy >= 0 && cx < this.w && cy < this.h ? this.bits[cy][cx] : 0;
    return bits ? String.fromCharCode(0x2800 + bits) : "";
  }

  /** The cells `[cx, cy]` that have a dot on, row by row. */
  cells() {
    const out = [];
    this.bits.forEach((row, cy) => row.forEach((b, cx) => b && out.push([cx, cy])));
    return out;
  }

  /** A string a row of cells, a space where no dot is on. */
  rows() {
    return this.bits.map((row) => row.map((b) => (b ? String.fromCharCode(0x2800 + b) : " ")).join(""));
  }
}
