// SPDX-License-Identifier: GPL-3.0-or-later
// The reef: the water of `fishes` and `tank` (the twin's `_reef`, and
// the fish of `fishes` both swim).
//
// - Draws: `rnd()` in [0, 1), as `Math.random`; `uniform`, `randint`,
//   `choice`, ... as Python's `random` names them.
// - Fishes: `Species`, `Fish` (frame cells, cells per second), `swim`.
// - Art: `Shape`s planted on text (`DECOR`), objects that sink
//   (`SUNK`), creatures that cross (`CROSSERS`); `mirror`, `pad`, `paint`.
// - `Reef`: what stands on the text, kept in frame cells (`seen`,
//   `decor`), and the vents of each area of `AREA` rows; `Events`: a
//   crosser, a sinking object, a hook, a fish leaving.
//
// Drawing goes through `out`, a `Batch` or its like: `put(spec)` a
// handle `{id}` (the id told once sent), `update(handle, members)`,
// `remove(handle)`. Specs are `draw.put`'s. Times in seconds.
// Layers, from the back: bubbles, crossers, decorations, then the
// fishes (`back`, -1) with the other decorations in front of them.

export const AREA = 12; // rows of an area: one set of vents
export const DENSITY = 0.3; // share of the lines of text that get a decoration
export const EVERY = [8.0, 20.0]; // seconds between two events
export const [BUBBLES, FAR, BEHIND, FRONT] = [-4, -3, -2, -1];

// Draws, `rnd` as `Math.random`.
export const uniform = (rnd, a, b) => a + (b - a) * rnd();
export const randrange = (rnd, n) => Math.floor(rnd() * n);
export const randint = (rnd, a, b) => a + randrange(rnd, b - a + 1);
export const choice = (rnd, seq) => seq[randrange(rnd, seq.length)];
export function shuffle(rnd, list) {
  for (let i = list.length - 1; i > 0; i--) {
    const j = randrange(rnd, i + 1);
    [list[i], list[j]] = [list[j], list[i]];
  }
  return list;
}
export function sample(rnd, seq, k) {
  const pool = [...seq];
  const out = [];
  for (let i = 0; i < k; i++) out.push(pool.splice(randrange(rnd, pool.length), 1)[0]);
  return out;
}
/** One of `seq`, by `weights`. */
export function choices(rnd, seq, weights) {
  let r = rnd() * weights.reduce((a, b) => a + b, 0);
  for (let i = 0; i < seq.length; i++) if ((r -= weights[i]) < 0) return seq[i];
  return seq[seq.length - 1];
}

/** A `rnd` of its own, from the seed `n` (mulberry32): for unit tests. */
export function seeded(n) {
  let a = n >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Python's `round`: half to even. */
export function round(x) {
  const r = Math.round(x);
  return r - x === 0.5 && r % 2 !== 0 ? r - 1 : r;
}
/** Python's `math.copysign`: -0 is negative. */
export const copysign = (a, b) => (b < 0 || Object.is(b, -0) ? -Math.abs(a) : Math.abs(a));

const SWAP = { "/": "\\", "\\": "/", "(": ")", ")": "(", "<": ">", ">": "<", "[": "]", "]": "[", "{": "}", "}": "{", "`": "'", "'": "`" };

/** `rows` seen from the other side: what faced right faces left. */
export const mirror = (rows) => rows.map((row) => [...row].reverse().map((c) => SWAP[c] ?? c).join(""));

/** Rows of exactly `width` cells. */
export const pad = (rows, width) => rows.map((row) => row.padEnd(width).slice(0, width));

/** The colour of `ch` by `ink`: glyphs to a colour, `""` the others. */
export function fg(ink, ch) {
  for (const k in ink) if (k && k.includes(ch)) return ink[k];
  return ink[""];
}

/** Rows of text to rows of styled runs, one a colour. A space is transparent. */
export function paint(rows, ink, style = {}) {
  return rows.map((row) => {
    const runs = [];
    let key = null;
    for (const ch of row) {
      const run = ch === " " ? { t: ch } : { t: ch, fg: fg(ink, ch), ...style };
      const k = JSON.stringify({ ...run, t: "" });
      if (runs.length && k === key) runs[runs.length - 1].t += ch;
      else runs.push(run);
      key = k;
    }
    return runs;
  });
}

/** Frames padded to a common box: rows to the right, frames from the top. */
function box(frames) {
  const w = Math.max(...frames.flat().map((r) => r.length));
  const h = Math.max(...frames.map((rows) => rows.length));
  return frames.map((rows) => [...Array(h - rows.length).fill(""), ...rows].map((r) => r.padEnd(w)));
}

/** Art in a box: `frames` of the same size, played `ms` apart, in one
 * of `inks`. `weight`: how often it is planted; `front`: the share
 * planted in front of the fishes. */
export class Shape {
  constructor(name, frames, inks, { ms = 700, weight = 1, front = 0.5 } = {}) {
    Object.assign(this, { name, frames, inks, ms, weight, front });
    this.w = frames[0][0].length;
    this.h = frames[0].length;
  }
}

/** A stalk of `h` rows, swaying; `twin`: a shorter one next to it. */
function kelp(h, twin = false) {
  const stalk = (n, phase) => Array.from({ length: n }, (_, i) => ((i + phase) % 2 ? " )" : "( "));
  const rows = (phase) => {
    if (!twin) return stalk(h, phase);
    const short = ["  ", ...stalk(h - 1, phase + 1)];
    return stalk(h, phase).map((a, i) => a + short[i]);
  };
  return box([rows(0), rows(1)]);
}

const inks = (colors, more = {}) => colors.map((c) => ({ "": c, ...more }));
const GREENS = inks([28, 29, 34, 35, 70, 71]);
const STONES = inks([244, 246, 248]);
const TREES = inks([203, 211, 209, 177]);
const flags = (rows, glyphs) => [...glyphs].map(rows);

export const DECOR = [
  new Shape("kelp-2", kelp(2), GREENS, { weight: 4 }),
  new Shape("kelp-3", kelp(3), GREENS, { weight: 4 }),
  new Shape("kelp-4", kelp(4), GREENS, { weight: 3 }),
  new Shape("kelp-5", kelp(5), GREENS, { weight: 2 }),
  new Shape("weed-3", kelp(3, true), GREENS, { weight: 3 }),
  new Shape("weed-4", kelp(4, true), GREENS, { weight: 2 }),
  new Shape("grass", box([["\\|/"]]), GREENS, { weight: 3 }),
  new Shape("tuft", box([["\\\\|//"]]), GREENS, { weight: 2 }),
  new Shape(
    "shell",
    box([["(@)"]]),
    [
      { "": 223, "@": 216 },
      { "": 224, "@": 211 },
      { "": 189, "@": 147 },
    ],
    { weight: 3 },
  ),
  new Shape("pebbles", box([[".oOo."]]), STONES),
  new Shape("pebble", box([["o."]]), STONES),
  new Shape("stones", box([[".o"]]), STONES),
  new Shape(
    "crab",
    box([["V(°°)V"], ["V(°°)V"], ["v(°°)V"], ["v(°°)v"], ["V(°°)v"]]),
    [
      { "": 203, "°": 231 },
      { "": 209, "°": 231 },
    ],
    { ms: 600, weight: 2 },
  ),
  new Shape(
    "snail",
    box([['_@/"'], ['_@/"'], ["_@/'"]]),
    [
      { "": 180, "@": 173 },
      { "": 187, "@": 137 },
    ],
    { ms: 1200 },
  ),
  new Shape("starfish", box([["_\\/_", " /\\ "]]), inks([208, 203, 221]), { weight: 2 }),
  new Shape("coral", box([["\\Y/", " | "]]), TREES, { weight: 2 }),
  new Shape("coral-tree", box([["y Y y", " \\|/ ", "  Y  "]]), TREES, { weight: 2 }),
  new Shape(
    "anemone",
    box([
      ["((|))", " \\_/ "],
      ["))|((", " \\_/ "],
    ]),
    [
      { "": 213, "\\_/": 132 },
      { "": 219, "\\_/": 97 },
      { "": 216, "\\_/": 131 },
    ],
    { ms: 900, weight: 2 },
  ),
  new Shape(
    "castle",
    box(flags((f) => ["     |" + f, " _   _   _ ", "| |_| |_| |", "|    _    |", "|___|_|___|"], ">>~")),
    STONES.map((ink) => ({ ...ink, ">~": 196 })),
    { ms: 450, weight: 4, front: 0.0 },
  ),
  new Shape(
    "tower",
    box(flags((f) => [" |" + f, "/^\\", "| |", "|n|"], ">>~")),
    STONES.map((ink) => ({ ...ink, ">~": 208 })),
    { ms: 450, weight: 3, front: 0.0 },
  ),
  new Shape("column", box([[" === ", " | | ", "_|_|_"]]), inks([250, 252]), { front: 0.0 }),
  new Shape(
    "chest",
    box(flags((lock) => [" ___ ", `/_${lock}_\\`, "|___|"], "ooooo*")),
    [
      { "": 130, o: 178, "*": 229 },
      { "": 94, o: 178, "*": 229 },
    ],
    { ms: 400, weight: 2, front: 0.0 },
  ),
  new Shape("anchor", box([["  o  ", " -|- ", "\\_|_/"]]), STONES),
];
export const TALL = Math.max(...DECOR.map((s) => s.h));

/** An object that falls in the water: `fall` on its way down, at
 * `speed` rows per second, `rest` once on the text. */
const sunk = (fall, speed, rest = fall) => ({ fall, rest, speed });

export const SUNK = [
  sunk(
    new Shape("coin", box([["O"], ["0"], ["|"], ["0"]]), inks([220]), { ms: 140 }),
    4.0,
    new Shape("coin", box([["o"]]), inks([220])),
  ),
  sunk(new Shape("key", box([["o-w"]]), inks([178, 250])), 5.0),
  sunk(new Shape("gem", box([["<>"], ["<>"], ["<>"], ["()"]]), inks([51, 201, 46]), { ms: 300 }), 4.5),
  sunk(DECOR[DECOR.length - 1], 9.0),
];

/** A creature that crosses the view, alone: an animation with a
 * motion. `frames` face right; `speed` in cells per second. `rises`:
 * from the bottom to the top instead. `scary`: fishes flee its mouth. */
export class Crosser {
  constructor(name, frames, inks, speed, { ms = 450, rises = false, scary = false } = {}) {
    Object.assign(this, { name, frames, inks, speed, ms, rises, scary });
    this.w = frames[0][0].length;
    this.h = frames[0].length;
  }
}

export const CROSSERS = [
  new Crosser("shoal", box([["   ><>   ><>", "><>   ><>   ><>", "   ><>   ><>"]]), inks([81, 117, 229, 216]), [5.0, 8.0]),
  new Crosser(
    "turtle",
    box(flags((f) => ['    .-"""-.  _  ', "~--/_|_|_|_\\( o>", `    \`${f}'   \`${f}'`], "U-")),
    inks([71, 107, 65], { o: 231 }),
    [2.0, 3.5],
    { ms: 600 },
  ),
  new Crosser(
    "octopus",
    box([
      ["  .---.  ", " ( o o ) ", " /(/|\\)\\ "],
      ["  .---.  ", " ( o o ) ", " \\)\\|/(/ "],
    ]),
    inks([169, 205, 173], { o: 231 }),
    [2.5, 4.0],
    { ms: 500 },
  ),
  new Crosser(
    "submarine",
    box(flags((s) => ["        _|_", "   ____/___\\_____", `${s}=(  o   o   o   )`, "  `--------------'"], "*+x")),
    [{ "": 220, o: 117, "*+x": 250 }],
    [4.0, 6.0],
    { ms: 120 },
  ),
  new Crosser("grouper", box([["     ,--,_", "|\\  /  o  \\", "| ><   ))  >", "|/  \\_____/"]]), inks([208, 173], { o: 231 }), [5.0, 7.0], {
    scary: true,
  }),
  new Crosser(
    "jellyfish",
    box([
      ["  ,--,  ", " (____) ", "  |)(|  ", "  ( )(  "],
      [" ,----, ", "(______)", "  )||(  ", "  /)(\\  "],
    ]),
    inks([213, 177, 219]),
    [0.7, 1.1],
    { ms: 700, rises: true },
  ),
];

// Fishes.

export const [LEFT, RIGHT] = ["<*//><", "><\\\\*>"];
export const LENGTH = LEFT.length;
export const COLORS = [14, 11, 13, 10, 6]; // bright cyan, yellow, magenta, green; cyan
export const [CALM, FEAR] = [6.0, 30.0]; // cells per second
export const [NEAR, NEAR_MOVING] = [8.0, 16.0]; // fear radius, in columns; rows count double

/** A kind of fish, one row: `right`, what it looks like swimming
 * right, one glyph a column it is on, in turn; `colors` to draw from. */
export class Species {
  constructor(name, right, colors = COLORS) {
    Object.assign(this, { name, right, colors });
    this.length = [...right[0]].length;
  }

  glyph(right, x) {
    const g = this.right[((x % this.right.length) + this.right.length) % this.right.length];
    return right ? g : mirror([g])[0];
  }
}

export const CLASSIC = new Species("classic", [RIGHT]);
export const SPECIES = [
  CLASSIC,
  new Species("minnow", ["><>"], [81, 117, 159, 229]),
  new Species("tetra", [">(°>"], [203, 208, 214, 51]),
  new Species("guppy", ["><{{°>"], [213, 141, 219, 177]),
  new Species("bream", ["><(((°>"], [220, 215, 150, 111]),
  new Species("sword", ["><(((°>--"], [75, 250, 153]),
  // It slithers: the body shifts with each column.
  new Species("eel", ["~-~-~-~:>", "-~-~-~-:>"], [107, 143, 179]),
];

/** A fish: position in frame cells, speed in cells per second. */
export class Fish {
  constructor({ x, y, vx, color, vy = 0.0, shy = false, kind = CLASSIC }) {
    Object.assign(this, { x, y, vx, vy, color, shy, kind });
    this.scared = 0.0; // seconds of fear left
    this.home = null; // [x, y]: where a shy fish hides
    this.stay = 0.0; // seconds left there
    this.leaving = false; // the walls do not turn it
    this.sprite = null;
    this.shown = null;
  }

  get length() {
    return this.kind.length;
  }

  /** `[x, y, glyph]`: where it is drawn. */
  get cell() {
    const x = round(this.x);
    return [x, round(this.y), this.kind.glyph(copysign(1, this.vx) > 0, x)];
  }
}

/** Distance from a fish of `length` cells at (x, y) to `at`, in
 * columns: a row is about two columns tall. */
export const reach = (x, y, at, length = LENGTH) => Math.hypot(x + length / 2 - at[0], (y - at[1]) * 2);

/** Speed towards a point `d` cells away: slower as it nears. */
export const approach = (d, top) => (Math.abs(d) > 0.3 ? Math.max(-top, Math.min(top, 2 * d)) : 0.0);

/** One step of `fish` inside `box` ([x0, y0, x1, y1], frame cells):
 * away from `cursor` within `NEAR` (`NEAR_MOVING` when `moving`), to
 * its `home`, or on its way, turning at the walls. */
export function swim(fish, dt, box, cursor, moving, rnd) {
  const [x0, y0, x1, y1] = box;
  const n = fish.length;
  if (cursor !== null) {
    const dx = fish.x + n / 2 - cursor[0];
    const dy = (fish.y - cursor[1]) * 2;
    if (reach(fish.x, fish.y, cursor, n) < (moving ? NEAR_MOVING : NEAR)) {
      // Away from the cursor, fast, for a moment.
      fish.scared = 0.8;
      fish.vx = copysign(FEAR, dx ? dx : fish.vx);
      fish.vy = Math.abs(dy) < 4 && dy ? copysign(4.0, dy) : 0.0;
    }
  }
  if (fish.scared > 0) {
    fish.scared -= dt;
    if (fish.scared <= 0) {
      fish.vx = copysign(uniform(rnd, 0.5, 1.0) * CALM, fish.vx);
      fish.vy = 0.0;
    }
  } else if (fish.home !== null) {
    // Towards its hiding place; still once there, facing as it came.
    fish.vx = approach(fish.home[0] - fish.x, CALM) || copysign(0.0, fish.vx);
    fish.vy = approach(fish.home[1] - fish.y, 2.0);
  } else if (rnd() < 0.2 * dt) {
    fish.vy = choice(rnd, [-1.5, 0.0, 0.0, 1.5]); // change depth, or not
  }
  fish.x += fish.vx * dt;
  fish.y += fish.vy * dt;
  if (fish.leaving) {
    // The walls do not turn it.
  } else if (fish.x < x0) [fish.x, fish.vx] = [x0, Math.abs(fish.vx)];
  else if (fish.x > x1 - n) [fish.x, fish.vx] = [x1 - n, -Math.abs(fish.vx)];
  if (!(y0 <= fish.y && fish.y <= y1)) [fish.y, fish.vy] = [Math.min(Math.max(fish.y, y0), y1), 0.0];
}

// The reef.

/** For each column, the first row with ink; `rows.length` if none. */
export function depths(rows, width) {
  const out = new Array(width).fill(rows.length);
  for (let y = rows.length - 1; y >= 0; y--) {
    const row = rows[y];
    for (let x = 0; x < Math.min(width, row.length); x++) if (row[x].trim()) out[x] = y;
  }
  return out;
}

const inked = (row, x) => x < row.length && row[x].trim() !== "";

/** `inked` of the cells of `rows`, `width` a row: what `stand` takes
 * as `ink`, read once for all its calls on the same rows. */
export function inkOf(rows, width) {
  return rows.map((row) => Array.from({ length: width }, (_, x) => inked(row, x)));
}

/** Columns where a box of `w` x `h` cells stands on row `g` of `rows`,
 * the view's text: ink under two cells of its base out of three, the
 * box free, and a free column on each side. */
export function stand(rows, g, width, w, h, ink = null) {
  if (!(h <= g && g < rows.length)) return [];
  const at = ink ? (y, x) => ink[y][x] : (y, x) => inked(rows[y], x);
  const shut = new Array(width).fill(false);
  for (let y = g - h; y < g; y++) for (let x = 0; x < width; x++) shut[x] ||= at(y, x);
  const out = [];
  for (let x = 0; x <= width - w; x++) {
    let ground = 0;
    for (let i = x; i < x + w; i++) ground += at(g, i);
    if (3 * ground < 2 * w) continue;
    let free = true;
    for (let i = Math.max(0, x - 1); i < Math.min(width, x + w + 1) && free; i++) free = !shut[i];
    if (free) out.push(x);
  }
  return out;
}

/** A bubble that rises `height` rows, as `draw.put` less the place: a
 * dot that wobbles in a box of 3 x 1 cells. */
export function bubble(height, rnd) {
  const speed = uniform(rnd, 1.2, 2.2); // rows per second
  const ms = 300;
  const n = Math.max(1, Math.ceil(((height / speed) * 1000) / ms));
  const phase = uniform(rnd, 0, 2 * Math.PI);
  const pace = uniform(rnd, 0.3, 0.6);
  const ink = { "": choice(rnd, [67, 110, 109, 245]) };
  const frames = [];
  for (let i = 0; i < n; i++) {
    const col = round(1 + 0.9 * Math.sin(phase + i * pace));
    frames.push({ cells: paint([" ".repeat(col) + "." + " ".repeat(2 - col)], ink, { dim: true }), ms });
  }
  return {
    frames,
    loop: false,
    motion: { vx: 0.0, vy: -speed, ax: 0.0, ay: 0.0, bounce: false },
    ttl_ms: n * ms + 1000,
    layer: BUBBLES,
  };
}

/** A place in frame cells. */
const at = (x, y) => ({ at: [x, y], space: "frame" });

/** A shape planted at `at`, the top left of its box, in frame cells. */
export class Decor {
  constructor(shape, place, ink, { front = false, phase = 0, ms = 700 } = {}) {
    Object.assign(this, { shape, at: place, ink, front, phase, ms });
    this.sprite = null;
  }

  /** The row it stands on. */
  get ground() {
    return this.at[1] + this.shape.h;
  }

  /** Its box and two columns on each side, as `"x,y"` keys. */
  cells() {
    const [x, y] = this.at;
    const out = [];
    for (let dx = -2; dx < this.shape.w + 2; dx++) for (let dy = 0; dy < this.shape.h; dy++) out.push(`${x + dx},${y + dy}`);
    return out;
  }

  /** Its content: `content`, or `frames` from its phase. */
  look() {
    const frames = this.shape.frames;
    if (frames.length === 1) return { content: { cells: paint(frames[0], this.ink) } };
    const k = this.phase % frames.length;
    const order = [...frames.slice(k), ...frames.slice(0, k)];
    return { frames: order.map((rows) => ({ cells: paint(rows, this.ink), ms: this.ms })), loop: true };
  }

  /** `draw.put` on `frame`. */
  spec(frame) {
    return { frame, anchor: at(...this.at), layer: this.front ? FRONT : BEHIND, z: 1, ...this.look() };
  }
}

/** A column of bubbles, in frame cells: `left` more in this burst, the
 * next at `due`. */
export class Vent {
  constructor(x, due) {
    Object.assign(this, { x, due, left: 0 });
  }
}

function plant(shape, place, rnd) {
  return new Decor(shape, place, choice(rnd, shape.inks), {
    front: rnd() < shape.front,
    phase: randrange(rnd, shape.frames.length),
    ms: round(shape.ms * uniform(rnd, 0.8, 1.25)),
  });
}

/** What was populated, in frame cells. `seen`: the rows of text
 * already surveyed; `decor`: what stands on them; `vents`: the columns
 * of bubbles of each area. `density`: `DENSITY`. */
export class Reef {
  constructor(density = DENSITY) {
    this.density = density;
    this.seen = new Set();
    this.decor = [];
    this.vents = new Map();
  }

  /** Populate the rows of the view not `seen` yet; returns what was
   * planted. A row within `TALL` of the top waits until the view is
   * `still`: the rows above it may come. The last row of the view waits
   * too. A share `density` of the lines of text gets a decoration:
   * drawn by weight among those that fit, the tall ones favoured, at a
   * random place off the others. */
  survey(rows, box, still, rnd) {
    const [x0, y0, x1, y1] = box;
    const taken = new Set();
    for (const d of this.decor) if (y0 - TALL <= d.ground && d.ground <= y1 + TALL) for (const c of d.cells()) taken.add(c);
    const planted = [];
    let ink = null;
    // Not the last row: the status line of an editor or a pager.
    for (let g = y0 + 1; g < Math.min(y1, y0 + rows.length - 1); g++) {
      if (this.seen.has(g) || (g - y0 < TALL && !still)) continue;
      this.seen.add(g);
      const row = rows[g - y0];
      if (!row.trim() || rnd() >= this.density) continue;
      // Weighted order, the tall ones first: the first that fits.
      const keys = DECOR.map((s) => -(rnd() ** (1 / (s.weight * s.h * s.h))));
      const order = DECOR.map((s, i) => i).sort((a, b) => keys[a] - keys[b]);
      for (const i of order) {
        const shape = DECOR[i];
        ink ??= inkOf(rows, x1 - x0);
        const xs = shuffle(rnd, stand(rows, g - y0, x1 - x0, shape.w, shape.h, ink));
        let found = null;
        for (const x of xs) {
          const d = plant(shape, [x0 + x, g - shape.h], rnd);
          if (!d.cells().some((c) => taken.has(c))) {
            found = d;
            break;
          }
        }
        if (found) {
          for (const c of found.cells()) taken.add(c);
          planted.push(found);
          break;
        }
      }
    }
    this.decor.push(...planted);
    return planted;
  }

  /** Forget what stands on nothing: the text under it is gone. Its row
   * is surveyed again. Returns what was removed. */
  prune(rows, box, out) {
    const [x0, y0, , y1] = box;
    const gone = this.decor.filter((d) => {
      const g = d.ground - y0;
      if (!(0 <= d.ground && d.ground <= y1) || !(0 <= g && g < rows.length)) return false;
      const from = Math.max(0, d.at[0] - x0);
      return !rows[g].slice(from, Math.max(0, d.at[0] - x0 + d.shape.w)).trim();
    });
    this.decor = this.decor.filter((d) => !gone.includes(d));
    for (const d of gone) {
      this.seen.delete(d.ground);
      if (d.sprite !== null) out?.remove(d.sprite);
    }
    return gone;
  }

  /** Sprites for what is within a view's height of the view; none for
   * the rest. */
  show(out, frame, box) {
    const [, y0, , y1] = box;
    const reach = y1 - y0 + 1;
    for (const d of this.decor) {
      const near = y0 - reach <= d.ground && d.ground <= y1 + reach + d.shape.h;
      if (near && d.sprite === null) d.sprite = out.put(d.spec(frame));
      else if (!near && d.sprite !== null) {
        out.remove(d.sprite);
        d.sprite = null;
      }
    }
  }

  /** The vents under the view: those of the area of its last row, one
   * every 20 columns, drawn once. */
  area(box, now, rnd) {
    const [x0, , x1, y1] = box;
    const key = Math.floor(y1 / AREA);
    if (!this.vents.has(key)) {
      const xs = [];
      for (let x = x0 + 1; x < Math.max(x0 + 2, x1 - 1); x++) xs.push(x);
      const n = Math.min(xs.length, Math.max(1, Math.floor((x1 - x0) / 20)));
      const picked = sample(rnd, xs, n).sort((a, b) => a - b);
      this.vents.set(
        key,
        picked.map((x) => new Vent(x, now + uniform(rnd, 0.0, 4.0))),
      );
    }
    return this.vents.get(key);
  }

  /** Let the vents that are due blow one bubble: a burst of 2 to 5,
   * then 2 to 7 s of rest. */
  bubbles(out, frame, box, now, rnd) {
    const [, y0, , y1] = box;
    for (const vent of this.area(box, now, rnd)) {
      if (now < vent.due) continue;
      out.put({ frame, anchor: at(vent.x - 1, y1), ...bubble(y1 - y0 + 1, rnd) });
      if (vent.left > 0) {
        vent.left -= 1;
        vent.due = now + uniform(rnd, 0.5, 1.2);
      } else {
        vent.left = randint(rnd, 1, 4);
        vent.due = now + uniform(rnd, 2.0, 7.0);
      }
    }
  }
}

const motion = (vx, vy) => ({ vx, vy, ax: 0.0, ay: 0.0, bounce: false });

/** The rows of a hook `n` cells long: a line, the hook at its end. */
export function hook(n) {
  const line = [{ t: "|", fg: 244, dim: true }];
  return [...Array(n - 1).fill(line), [{ t: "J", fg: 231, bold: true }]];
}

/** One event at a time, `every` seconds after the last (`EVERY`).
 *
 * - A crosser: one request, the driver plays it.
 * - A sunk object falls from the top of the view down a free column and
 *   lands on the first text below; it joins the reef.
 * - A hook comes down a free column, waits, goes back up.
 * - A fish leaves: the tide brings another one. */
export class Events {
  constructor(reef, now, rnd, every = EVERY) {
    Object.assign(this, { reef, rnd, every });
    this.due = now + uniform(rnd, ...every) / 2;
    this.cross = null;
    this.fall = null;
    this.hook = null;
  }

  get busy() {
    return this.cross !== null || this.fall !== null || this.hook !== null;
  }

  /** Move what is going on, start what is due. Returns what the fishes
   * fear, as frame positions, and whether one is to leave. */
  step(out, frame, rows, box, now, dt) {
    const { rnd } = this;
    const threats = [];
    let leave = false;
    if (this.cross !== null) {
      if (now >= this.cross.ends) this.cross = null;
      else if (this.cross.kind.scary) threats.push(this.mouth(now));
    }
    if (this.fall !== null) this.sink(out, frame, dt);
    if (this.hook !== null) {
      this.reel(out, dt);
      if (this.hook !== null) threats.push([this.hook.x, this.hook.top + this.hook.shown - 1]);
    }
    if (this.busy) this.due = now + uniform(rnd, ...this.every);
    else if (now >= this.due) {
      const what = choice(rnd, "ccccfffhhl");
      if (what === "c") this.crosses(out, frame, box, now);
      else if (what === "f") this.drop(out, frame, rows, box);
      else if (what === "h") this.cast(out, frame, rows, box);
      else {
        leave = true;
        this.due = now + uniform(rnd, ...this.every);
      }
    }
    return [threats, leave];
  }

  /** The mouth of the crosser, at `now`. */
  mouth(now) {
    const c = this.cross;
    const t = now - c.born;
    const front = c.vel[0] > 0 ? c.kind.w : 0;
    return [c.at[0] + c.vel[0] * t + front, c.at[1] + c.vel[1] * t + Math.floor(c.kind.h / 2)];
  }

  crosses(out, frame, box, now) {
    const { rnd } = this;
    const [x0, y0, x1, y1] = box;
    const kind = choice(rnd, CROSSERS);
    const speed = uniform(rnd, ...kind.speed);
    let frames = kind.frames;
    let place, vel, way;
    if (kind.rises) {
      place = [randint(rnd, x0, Math.max(x0, x1 - kind.w)), y1 + 1];
      vel = [0.0, -speed];
      way = y1 - y0 + 1 + kind.h;
    } else {
      const right = rnd() < 0.5;
      if (!right) frames = frames.map(mirror);
      place = [right ? x0 - kind.w : x1, randint(rnd, y0, Math.max(y0, y1 - kind.h))];
      vel = [right ? speed : -speed, 0.0];
      way = x1 - x0 + kind.w;
    }
    const life = way / speed;
    const ink = choice(rnd, kind.inks);
    out.put({
      frame,
      anchor: at(...place),
      frames: frames.map((rows) => ({ cells: paint(rows, ink), ms: kind.ms })),
      loop: true,
      layer: FAR,
      motion: motion(...vel),
      ttl_ms: Math.trunc(life * 1000),
    });
    this.cross = { kind, at: place, vel, born: now, ends: now + life };
  }

  /** A column where the object falls 3 rows or more, onto text above
   * the last row. */
  drop(out, frame, rows, box) {
    const { rnd } = this;
    const [x0, y0, x1] = box;
    const kind = choice(rnd, SUNK);
    const [w, h] = [kind.fall.w, kind.fall.h];
    const first = depths(rows, x1 - x0);
    // A free column on each side, all the way down.
    const ways = [];
    for (let x = 1; x < x1 - x0 - w; x++) {
      const r = Math.min(...first.slice(x - 1, x + w + 1)) - h;
      if (3 <= r && r < rows.length - h - 1) ways.push([x, r]);
    }
    if (!ways.length) return;
    const [x, rest] = choice(rnd, ways);
    const ink = choice(rnd, kind.fall.inks);
    const falling = new Decor(kind.fall, [x0 + x, y0], ink, { front: true, ms: kind.fall.ms });
    const sprite = out.put(falling.spec(frame));
    this.fall = { kind, ink, x: x0 + x, y: y0, rest: y0 + rest, sprite, shown: y0 };
  }

  sink(out, frame, dt) {
    const fall = this.fall;
    fall.y = Math.min(fall.y + fall.kind.speed * dt, fall.rest);
    const y = round(fall.y);
    if (y !== fall.shown) {
      out.update(fall.sprite, { anchor: at(fall.x, y) });
      fall.shown = y;
    }
    if (y >= fall.rest) {
      // On the text: one more thing the reef remembers.
      const landed = new Decor(fall.kind.rest, [fall.x, fall.rest], fall.ink, { front: true });
      landed.sprite = fall.sprite;
      if (fall.kind.rest !== fall.kind.fall) out.update(fall.sprite, landed.look());
      this.reef.decor.push(landed);
      this.fall = null;
    }
  }

  /** A column free for 4 rows or more from the top of the view. */
  cast(out, frame, rows, box) {
    const { rnd } = this;
    const [x0, y0, x1] = box;
    const first = depths(rows, x1 - x0);
    const ways = [];
    for (let x = 2; x < x1 - x0 - 2; x++) if (first[x] >= 4) ways.push(x);
    if (!ways.length) return;
    const x = choice(rnd, ways);
    const most = randint(rnd, 3, Math.min(first[x], Math.max(3, Math.floor((rows.length * 2) / 3))));
    const sprite = out.put({ frame, anchor: at(x0 + x, y0), content: { cells: hook(1) } });
    this.hook = { x: x0 + x, top: y0, most, wait: uniform(rnd, 5.0, 9.0), sprite, n: 1.0, shown: 1 };
  }

  reel(out, dt) {
    const hk = this.hook;
    if (hk.wait > 0 && hk.n < hk.most) hk.n = Math.min(hk.n + 5.0 * dt, hk.most); // down
    else if (hk.wait > 0) hk.wait -= dt;
    else hk.n -= 8.0 * dt; // up
    const n = round(hk.n);
    if (n < 1) {
      out.remove(hk.sprite);
      this.hook = null;
    } else if (n !== hk.shown) {
      out.update(hk.sprite, { content: { cells: hook(n) } });
      hk.shown = n;
    }
  }
}

/** Calls gathered for one `batch` request: one repaint. `call` as
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
