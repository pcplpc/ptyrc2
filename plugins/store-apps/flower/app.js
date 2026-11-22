// SPDX-License-Identifier: GPL-3.0-or-later
// Flower: flowers planted next to a word.
//
// The view is read when the driver tells it changed (`ptyrc/sight`):
// its shape with `--word` shown (default `flower`), a match a whole
// word, case ignored. Every `SCAN` ms the garden is tended: a match
// without a flower gets one, while there are fewer than `--count`. It
// is planted at the nearest spot (`place`) where its root is `GAP`
// columns from the word and `RISE` rows off its row, each of its cells
// cold in the heat classes (`frame.heat`). The match is redrawn in
// bold over the text (a tint, in front). At most one flower a scan.
//
// A flower has a random size (`SHAPES`) and colours (`colors`). It
// grows in steps `STEP` ms apart, then sways, an animation played by
// the driver. It wilts when its match leaves the view or text covers
// it; with matches to spare, after `LIFE` ms. Flowers and bold words
// hold frame positions: they scroll with the text.
//
// `-a`: the words are a pool of `--count` (default `POOL`) drawn from
// the text of the view (`pool`), 4 letters or more, matched whole
// (`screen.find`). A word leaves the pool when it leaves the view or no
// flower fits next to it, and every `SWAP` ms the oldest one leaves.
// `--count 0`: the size drawn from `CROWD` every `RECOUNT` ms. No bold.

import { call, period, PtyrcError } from "ptyrc";
import { Sight } from "ptyrc/sight";

export const GAP = [3, 10]; // columns from the word to the root
export const RISE = [1, 3]; // rows from the word to the root
export const LIFE = [20000, 40000]; // ms, while matches are left without a flower
export const SCAN = 500; // ms between two looks at the garden
export const STEP = 350; // ms between two growth steps
export const POOL = 5; // words, with -a
export const CROWD = [10, 30]; // words, with -a --count 0
export const SWAP = 10000; // ms between two changes of the pool
export const RECOUNT = 60000; // ms between two draws from CROWD
const HEARTS = "o@*";
const WORD = /\p{L}{4,}/gu;
const SHOWN = /[^\s#]+/g;

// Draws; `rnd` as `Math.random`.
export const uniform = (rnd, a, b) => a + (b - a) * rnd();
export const randint = (rnd, a, b) => a + Math.floor(rnd() * (b - a + 1));
export const choice = (rnd, list) => list[Math.floor(rnd() * list.length)];
export function sample(rnd, list, k) {
  const rest = [...list];
  const out = [];
  while (out.length < k && rest.length) out.push(rest.splice(Math.floor(rnd() * rest.length), 1)[0]);
  return out;
}
export function shuffle(rnd, list) {
  for (let i = list.length - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    [list[i], list[j]] = [list[j], list[i]];
  }
  return list;
}

/** A flower in a box of `w` x `h` cells, its root at the bottom of
 * column `root`. `grow`: the steps before it opens; `sway`: leaning
 * left, upright, right; `head`: rows of the head, the rest is stem;
 * `mask`: `[x, y]` of the cells with a glyph in some step. Steps padded
 * to the box: rows to the right, from the top. Spaces are transparent. */
export function shape(head, grow, sway) {
  const all = [...grow, ...sway];
  const w = Math.max(...all.flat().map((r) => r.length));
  const h = Math.max(...all.map((rows) => rows.length));
  const box = (rows) => [...Array(h - rows.length).fill(""), ...rows].map((r) => r.padEnd(w));
  const s = { grow: grow.map(box), sway: sway.map(box), head, w, h };
  s.root = s.sway[1][h - 1].indexOf("|");
  const mask = new Set();
  for (const rows of [...s.grow, ...s.sway])
    rows.forEach((row, y) => [...row].forEach((ch, x) => ch !== " " && mask.add(`${x},${y}`)));
  s.mask = [...mask].map((k) => k.split(",").map(Number));
  return s;
}

export const SHAPES = {
  small: shape(
    1,
    [["  .  "], ["  o  ", "  |  "]],
    [
      ["(@)  ", "  \\  ", "  |  "],
      [" (@) ", "  |  ", "  |  "],
      ["  (@)", "  /  ", "  |  "],
    ],
  ),
  daisy: shape(
    3,
    [["   .   "], ["   o   ", "  \\|   ", "   |   "]],
    [
      [" \\|/   ", " -*-   ", " /|\\   ", "   \\   ", "  \\|   "],
      ["  \\|/  ", "  -*-  ", "  /|\\  ", "   |   ", "  \\|   "],
      ["   \\|/ ", "   -*- ", "   /|\\ ", "   /   ", "  \\|   "],
    ],
  ),
  large: shape(
    5,
    [
      ["   .   ", "   |   "],
      ["  ,-,  ", " ( o ) ", "  `-'  ", "  \\|/  ", "   |   "],
    ],
    [
      [",-.-,  ", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
      [" ,-.-, ", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
      ["  ,-.-,", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
    ],
  ),
};

// 256-colour indices.
export const PETALS = [196, 197, 198, 201, 205, 207, 213, 219, 129, 135, 141, 99, 208, 214, 39];
export const CENTRES = [220, 226, 214, 229, 231];
export const STEMS = [28, 34, 40, 70, 76];

/** Two petal colours, alternating (the same 60 % of the time), a heart, a stem. */
export function colors(rnd) {
  const [a, b] = sample(rnd, PETALS, 2);
  const alt = rnd() < 0.4 ? b : a;
  return { petals: a, alt, heart: choice(rnd, CENTRES), stem: choice(rnd, STEMS) };
}

/** Rows of text to rows of runs, a run a glyph. The first `head` rows
 * are the head: hearts bold, petals in two colours. */
export function paint(rows, c, head, dim = false) {
  return rows.map((row, y) =>
    [...row].map((ch, x) => {
      let run;
      if (ch === " ") return { t: " " }; // transparent
      if (y >= head) run = { t: ch, fg: c.stem };
      else if (HEARTS.includes(ch)) run = { t: ch, fg: c.heart, bold: true };
      else run = { t: ch, fg: (x + y) % 2 ? c.petals : c.alt };
      if (dim) run.dim = true;
      return run;
    }),
  );
}

/** Top left of the box, in view cells, of the flower planted next to
 * `word` (`[x, y, len]`, view cells): its root `GAP` columns from an
 * end of the word and `RISE` rows off its row; each cell of its mask in
 * the view, cold (`.` in `classes`), not in `taken` (a set of `"x,y"`),
 * its neighbours left and right cold or out of the view: never in a gap
 * between two words. The nearest, rows counting double, ties at random;
 * null if none. */
export function place(classes, word, s, taken, rnd) {
  const [wx, wy, n] = word;
  const h = classes.length;
  const cold = (x, y) => !(x >= 0 && x < classes[y].length) || classes[y][x] === ".";
  let best = null;
  for (let gap = GAP[0]; gap <= GAP[1]; gap++) {
    for (let rise = RISE[0]; rise <= RISE[1]; rise++) {
      for (const rx of [wx - gap, wx + n - 1 + gap]) {
        for (const ry of [wy - rise, wy + rise]) {
          const x0 = rx - s.root;
          const y0 = ry - s.h + 1;
          const fits = s.mask.every(([dx, dy]) => {
            const x = x0 + dx;
            const y = y0 + dy;
            return (
              y >= 0 &&
              y < h &&
              x >= 0 &&
              x < classes[y].length &&
              classes[y][x] === "." &&
              !taken.has(`${x},${y}`) &&
              cold(x - 1, y) &&
              cold(x + 1, y)
            );
          });
          if (!fits) continue;
          const key = [Math.hypot(gap, 2 * rise), rnd(), x0, y0];
          if (!best || key[0] < best[0] || (key[0] === best[0] && key[1] < best[1])) best = key;
        }
      }
    }
  }
  return best && [best[2], best[3]];
}

/** The matches of `words` in `rows`, a shape: `[x, y, text]` in its
 * cells, in reading order. A shape shows letters only where a word
 * matched. */
export function found(rows, words) {
  const wanted = new Set(words.map((w) => w.toLowerCase()));
  const out = [];
  rows.forEach((row, y) => {
    for (const m of row.matchAll(SHOWN)) if (wanted.has(m[0].toLowerCase())) out.push([m.index, y, m[0]]);
  });
  return out;
}

/** The first `size` of `words` still in `rows`, the view's text, then
 * words drawn from it up to `size`, none in `avoid`. Lower case, 4
 * letters or more. */
export function pool(rows, words, size, rnd, avoid = []) {
  const seen = [...new Set(rows.flatMap((r) => (r.match(WORD) ?? []).map((w) => w.toLowerCase())))].sort();
  const out = words.filter((w) => seen.includes(w)).slice(0, size);
  const rest = seen.filter((w) => !out.includes(w) && !avoid.includes(w));
  return [...out, ...sample(rnd, rest, Math.max(0, size - out.length))];
}

/** Text shows in one of the flower's cells (frame cells; `rows` the
 * view at `origin`). */
export function covered(cells, rows, origin) {
  const [ox, oy] = origin;
  return cells.some(([x, y]) => {
    const r = rows[y - oy];
    const ch = r?.[x - ox];
    return ch !== undefined && ch.trim() !== "";
  });
}

/** The options of `init`. */
export function options(args) {
  const anywhere = args.anywhere === true;
  const count = Number.isInteger(args.count) ? args.count : anywhere ? POOL : 1;
  return { word: args.word ?? "flower", anywhere, count };
}

/** The garden: what is kept between two scans, and what it draws by
 * `env`: `call(method, params)`, `rnd()`. */
export class Garden {
  constructor(o, env, now = 0) {
    this.o = o;
    this.env = env;
    this.flowers = [];
    this.words = []; // with -a, oldest first
    this.size = o.count;
    this.swap = now + SWAP;
    this.recount = 0;
    this.sought = null; // the pattern last looked for
    this.matches = []; // its matches: {x, y, len, text}, frame cells
  }

  #try(method, params) {
    try {
      return this.env.call(method, params);
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      return null;
    }
  }

  /** Flowers gone dropped, the next growth step of each when due. */
  tend(now) {
    this.flowers = this.flowers.filter((f) => f.dies === null || f.dies > now);
    for (const f of this.flowers) this.grow(f, now);
  }

  plant(m, s, at, now) {
    const { rnd } = this.env;
    const c = colors(rnd);
    const anchor = (p) => ({ at: p, space: "frame" });
    const sprite = this.#try("draw.put", { content: { cells: paint(s.grow[0], c, 0) }, anchor: anchor(at) });
    const bold = this.o.anywhere
      ? null
      : this.#try("draw.put", {
          content: { rows: [m.text], style: { bold: true } },
          anchor: anchor([m.x, m.y]),
          blend: "tint",
          layer: "front",
        });
    this.flowers.push({
      key: `${m.x},${m.y},${m.text}`,
      word: m.text.toLowerCase(),
      shape: s,
      colors: c,
      sprite: sprite?.id ?? null,
      bold: bold?.id ?? null,
      born: now,
      life: uniform(rnd, ...LIFE),
      step: 0,
      next: now + STEP,
      dies: null,
      cells: s.mask.map(([dx, dy]) => [at[0] + dx, at[1] + dy]),
    });
  }

  grow(f, now) {
    if (f.dies !== null || f.next > now) return;
    const { shape: s, colors: c } = f;
    f.step += 1;
    f.next = now + STEP;
    if (f.step < s.grow.length) {
      this.#try("draw.update", { id: f.sprite, content: { cells: paint(s.grow[f.step], c, 0) } });
      return;
    }
    f.next = Infinity;
    // Left, upright, right, upright, from a random phase, at its own pace.
    const order = [0, 1, 2, 1].map((i) => s.sway[i]);
    const k = Math.floor(this.env.rnd() * 4);
    const ms = randint(this.env.rnd, 450, 800);
    const frames = [...order.slice(k), ...order.slice(0, k)].map((r) => ({ cells: paint(r, c, s.head), ms }));
    this.#try("draw.update", { id: f.sprite, frames, loop: true });
  }

  /** Close and fade; the driver removes it after the last frame. */
  wilt(f, now) {
    if (f.dies !== null) return;
    const { shape: s, colors: c } = f;
    const steps = [s.sway[1], ...[...s.grow].reverse()];
    const frames = steps.map((r, i) => ({ cells: paint(r, c, i === 0 ? s.head : 0, true), ms: 250 }));
    this.#try("draw.update", { id: f.sprite, frames, loop: false });
    if (f.bold !== null) this.#try("draw.remove", { id: f.bold });
    f.dies = now + 250 * frames.length;
  }

  /** Wilt the flowers that lost their place, plant one if wanted.
   * `view`: `{rows, origin, changed, frame}`, `heat()` its classes. */
  scan(view, heat, now) {
    const { o } = this;
    const { rnd, call } = this.env;
    const { rows, origin } = view;
    const [ox, oy] = origin;
    let sought;
    if (o.anywhere) {
      if (o.count === 0 && now >= this.recount) {
        this.size = randint(rnd, ...CROWD);
        this.recount = now + RECOUNT;
      }
      let gone = [];
      if (now >= this.swap) {
        gone = this.words.slice(0, 1);
        this.words = this.words.slice(1);
        this.swap = now + SWAP;
      }
      this.words = pool(rows, this.words, this.size, rnd, gone);
      if (!this.words.length) return;
      sought = `\\b(?:${this.words.join("|")})\\b`;
    } else sought = o.word;
    if (view.changed || sought !== this.sought) {
      this.sought = null; // void, should the frame leave meanwhile
      if (o.anywhere) {
        const r = call("screen.find", { pattern: sought, regex: true, ignore_case: true, frame: view.frame });
        this.matches = (r.matches ?? []).filter((m) => m.screen);
      } else {
        this.matches = found(rows, [o.word]).map(([x, y, text]) => ({ x: x + ox, y: y + oy, len: text.length, text }));
      }
      this.sought = sought;
    }
    const matches = this.matches;
    const byKey = new Map(matches.map((m) => [`${m.x},${m.y},${m.text}`, m]));
    let living = this.flowers.filter((f) => f.dies === null);
    const spare = matches.length > living.length;
    for (const f of living) {
      const old = spare && now - f.born > f.life && f.step >= f.shape.grow.length;
      if (!byKey.has(f.key) || covered(f.cells, rows, origin) || old) this.wilt(f, now);
    }
    living = this.flowers.filter((f) => f.dies === null);
    const want = o.anywhere ? this.size : o.count === 0 ? matches.length : o.count;
    const keys = new Set(living.map((f) => f.key));
    const free = [...byKey].filter(([k]) => !keys.has(k)).map(([, m]) => m);
    if (living.length >= want || !free.length) return;
    const classes = heat();
    const taken = new Set();
    for (const f of this.flowers) for (const [x, y] of f.cells) taken.add(`${x - ox},${y - oy}`);
    for (const m of matches) {
      // The words, and a cell around them.
      const [x, y] = [m.x - ox, m.y - oy];
      for (let i = -1; i <= m.len; i++) for (let j = -1; j <= 1; j++) taken.add(`${x + i},${y + j}`);
    }
    // Words with the fewest flowers first, then at random.
    const perWord = new Map();
    for (const f of living) perWord.set(f.word, (perWord.get(f.word) ?? 0) + 1);
    const n = (m) => perWord.get(m.text.toLowerCase()) ?? 0;
    shuffle(rnd, free);
    free.sort((a, b) => n(a) - n(b));
    const failed = new Set();
    const all = Object.values(SHAPES);
    const bySize = [...all].sort((a, b) => a.h - b.h);
    for (const m of free) {
      // A random size; smaller ones where it does not fit.
      for (const s of [choice(rnd, all), ...bySize]) {
        const at = place(classes, [m.x - ox, m.y - oy, m.len], s, taken, rnd);
        if (at) {
          this.plant(m, s, [at[0] + ox, at[1] + oy], now);
          return;
        }
      }
      failed.add(m.text.toLowerCase());
    }
    // No room next to any of its free matches, none flowered: drawn anew.
    this.words = this.words.filter((w) => !failed.has(w) || perWord.has(w));
  }
}

let sight = null;
let garden = null;
let scanned = -Infinity;

export function init(args) {
  const o = options(args);
  garden = new Garden(o, { call, rnd: Math.random }, Date.now());
  sight = o.anywhere ? new Sight({ text: true }) : new Sight({ words: [o.word] });
  period(50);
}

export function tick(now) {
  garden.tend(now);
  if (now - scanned < SCAN || !sight.look()) return;
  scanned = now;
  const frame = sight.info?.id ?? sight.frame;
  const view = { rows: sight.rows, origin: sight.info?.origin ?? [0, 0], changed: sight.changed, frame };
  try {
    garden.scan(view, () => call("frame.heat", { frame }).classes, now);
  } catch (e) {
    if (!(e instanceof PtyrcError) || e.code !== "not-usable") throw e;
    sight.touch();
  }
}

export function event(topic, data) {
  sight?.event(topic, data);
}
