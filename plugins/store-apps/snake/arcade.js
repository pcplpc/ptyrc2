// SPDX-License-Identifier: GPL-3.0-or-later
// The arcade: what `snake` and `bricks` share (the twin's `_arcade`).
//
// - Cells as numbers (`key`): sets and maps of them are cheap, and
//   their order is that of `(x, y)` pairs. `wallsOf`: the map of a view.
// - Particles (`spark`, `burst`, `debris`): each one `draw.put`, a
//   finite animation with a `motion`, played and removed by the driver.
// - The apple, a foe: a looped animation, moved by the app. A beam: one
//   sprite, its frames replaced when it fires.
// - `Board`: a score, top right; `Batch`: the calls of one repaint.
//
// A spec here is `draw.put`'s less the place; no `layer`: behind the
// text, in front with the launcher's `--front`. `rng`: a `Twister`.

import { cells } from "ptyrc";

export const BODY = [118, 82, 46, 40, 34, 28, 22]; // head to tail
export const LUMP = 226; // an apple on its way down
export const RED = [196, 202, 208, 160];
export const GOLD = [226, 220, 214, 231];
export const FOE = [201, 165, 213];
export const HOT = [231, 196, 203];
export const DEBRIS = 40; // particles of a lost tail, at most

const B = 2 ** 24;
const M = 2 ** 25;

/** The cell `(x, y)`, `|x|, |y| < 2**24`, as a number: keys sort as
 * the pairs do, `x` first. `key(x, y) + 1` is `(x, y + 1)`. */
export const key = (x, y) => (x + B) * M + (y + B);
/** One across: `key(x + 1, y) = key(x, y) + ACROSS`. */
export const ACROSS = M;
export const X = (k) => Math.floor(k / M) - B;
export const Y = (k) => (k % M) - B;
/** `[x, y]` of a key. */
export const pair = (k) => [X(k), Y(k)];

/** The cells holding a glyph, `rows` being the shape or the text from
 * `(x0, y0)`: a set of keys. A wide glyph takes two columns. */
export function wallsOf(rows, x0 = 0, y0 = 0) {
  const out = new Set();
  let y = y0;
  for (const row of rows) {
    let x = x0;
    for (const ch of row) {
      const c = ch.charCodeAt(0);
      if (c === 32 || c === 35) {
        // The shape: a blank, or ink.
        if (c === 35) out.add(key(x, y));
        x++;
        continue;
      }
      const wide = c >= 0x1100 && cells(ch) === 2;
      if (!/\s/.test(ch)) {
        out.add(key(x, y));
        if (wide) out.add(key(x + 1, y));
      }
      x += wide ? 2 : 1;
    }
    y++;
  }
  return out;
}

/** A particle: it leaves in a random direction at `speed` (columns a
 * second; a row counts double), goes through `glyphs` in `life`
 * seconds and falls. */
export function spark(rng, colors, glyphs = "*+'.", speed = [4.0, 14.0], life = [0.4, 0.9], gravity = 8.0) {
  const angle = rng.uniform(0, 2 * Math.PI);
  const v = rng.uniform(...speed);
  const seconds = rng.uniform(...life);
  const color = rng.choice(colors);
  const g = [...glyphs];
  const last = g.length - 1;
  const ms = Math.max(40, Math.trunc((seconds * 1000) / g.length));
  const frames = g.map((t, i) => {
    const run = { t, fg: color };
    if (i === 0) run.bold = true;
    if (i === last && last) run.dim = true;
    return { cells: [[run]], ms };
  });
  return {
    frames,
    loop: false,
    motion: { vx: v * Math.cos(angle), vy: (v * Math.sin(angle)) / 2, ax: 0.0, ay: gravity, bounce: false },
    ttl_ms: Math.trunc((seconds + 0.5) * 1000),
    z: 3,
  };
}

/** `n` particles from one cell; `kw`: `glyphs`, `speed`, `life`, `gravity`. */
export function burst(n, rng, colors, kw = {}) {
  const out = [];
  for (let i = 0; i < n; i++) out.push(spark(rng, colors, kw.glyphs, kw.speed, kw.life, kw.gravity));
  return out;
}

/** A segment of a lost tail: it crumbles and falls. */
export function debris(rng) {
  return spark(rng, BODY.slice(2), "o:.", [0.5, 4.0], [0.5, 1.1], 14.0);
}

/** The apple, pulsing; a golden one glitters. */
export function apple(golden = false) {
  const [colors, ms] = golden ? [GOLD, 120] : [[RED[0], RED[3]], 350];
  return {
    frames: colors.map((fg, i) => ({ cells: [[{ t: "●", fg, bold: i === 0 }]], ms })),
    loop: true,
  };
}

/** A foe: a blade that spins. */
export function foe() {
  return {
    frames: [..."x+x+x+"].map((t, i) => ({ cells: [[{ t, fg: FOE[i % 3], bold: true }]], ms: 110 })),
    loop: true,
    z: 2,
  };
}

/** A beam of `length` cells, along a row if `across`, else down a
 * column: dashes that blink, then a line that flickers. */
export function beam(length, across, firing) {
  const [glyph, inks, ms] = firing ? [across ? "━" : "┃", HOT, 60] : [across ? "╌" : "╎", [88, 160], 180];
  const rows = (fg) => {
    const run = firing ? { t: glyph, fg, bold: true } : { t: glyph, fg };
    if (across) return [[{ ...run, t: glyph.repeat(length) }]];
    return Array.from({ length }, () => [{ ...run }]);
  };
  return { frames: inks.map((fg) => ({ cells: rows(fg), ms })), loop: true };
}

/** The score: `length`; with `best`, the most it was; `hint`: the keys
 * of a player, dim, before it. One row of runs. */
export function tally(length, best = null, hint = null) {
  const runs = [{ t: `◆ ${length}`, fg: 10, bold: true }];
  if (best !== null) runs.push({ t: `  best ${best}`, dim: true });
  if (hint) runs.unshift({ t: `${hint}  `, dim: true });
  return [runs];
}

/** Calls gathered for one `batch` request: one repaint. `call`: as
 * `call` of "ptyrc". A handle's `id` is told by `send`; an update or a
 * removal of a handle not drawn yet is dropped. */
export class Batch {
  constructor(call) {
    this.call = call;
    this.calls = [];
    this.puts = [];
  }

  put(spec) {
    const handle = { id: null };
    this.puts.push([this.calls.length, handle]);
    this.calls.push({ method: "draw.put", params: spec });
    return handle;
  }

  update(handle, members) {
    if (handle.id !== null) this.calls.push({ method: "draw.update", params: { id: handle.id, ...members } });
  }

  remove(handle) {
    if (handle.id !== null) this.calls.push({ method: "draw.remove", params: { id: handle.id } });
    handle.id = null;
  }

  /** Sends what was gathered; the results, or an error each. */
  send() {
    if (!this.calls.length) return [];
    const { calls, puts } = this;
    this.calls = [];
    this.puts = [];
    const results = this.call("batch", { calls }).results ?? [];
    for (const [i, handle] of puts) handle.id = results[i]?.result?.id ?? null;
    return results;
  }
}

/** A score: one sprite, top right in view cells, over the game. Its
 * blanks are opaque: what passes under does not show through. Sent
 * when it changes. */
export class Board {
  constructor() {
    this.sprite = null;
    this.shown = null; // its rows, as JSON
  }

  /** `rows`: one row of runs (`tally`), on `frame`, by `batch`. */
  show(batch, frame, rows) {
    const json = JSON.stringify(rows);
    if (json === this.shown && this.sprite?.id != null) return;
    const anchor = { at: [-1, 0], space: "view" }; // its right edge on the last column
    const content = { cells: rows, opaque: true };
    if (this.sprite?.id == null) this.sprite = batch.put({ frame, anchor, content, z: 5 });
    else batch.update(this.sprite, { anchor, content });
    this.shown = json;
  }
}
