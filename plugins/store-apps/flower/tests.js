// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of flower, `ptyrc dev test --unit` (test_apps.py:
// test_flower_place, _pool, _not_between_words, _shapes, _paint; the
// garden on a fake host).

import { assert, assertEqual } from "ptyrc";
import { Garden, SHAPES, STEP, colors, covered, found, options, paint, place, pool } from "./app.js";

/** A seeded `Math.random` (mulberry32). */
function seeded(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const root = (s, [x0, y0]) => [x0 + s.root, y0 + s.h - 1];

export function test_place() {
  const rnd = seeded(1);
  const small = SHAPES.small;
  const cold = Array(20).fill(".".repeat(40));
  // The nearest root: 3 columns past an end of the word, a row off it.
  const r = root(small, place(cold, [10, 10, 6], small, new Set(), rnd));
  assert(["7,9", "7,11", "18,9", "18,11"].includes(r.join()), r.join());
  // Each cell must be cold and free.
  const hot = cold.map((row) => row.slice(0, 17) + "h".repeat(23));
  assertEqual(root(small, place(hot, [10, 10, 6], small, new Set(), rnd))[0], 7);
  const [x, y] = root(small, place(hot, [10, 10, 6], small, new Set(["7,9", "7,11"]), rnd));
  assert(x < 7 && [1, 2, 3].includes(Math.abs(y - 10)), `${x},${y}`);
  assertEqual(place(Array(20).fill("t".repeat(40)), [10, 10, 6], small, new Set(), rnd), null);
  // Out of the view: none.
  assertEqual(place(cold.slice(0, 2), [0, 0, 6], SHAPES.large, new Set(), rnd), null);
}

export function test_not_between_words() {
  // Free cells one column wide between words: no room there.
  const gaps = Array(12).fill("tt.tt.tt.tt.tt.tt.tt.tt.tt.tt");
  assertEqual(place(gaps, [10, 6, 3], SHAPES.small, new Set(), seeded(1)), null);
}

export function test_pool() {
  const rnd = seeded(1);
  const rows = ["the quick brown fox, Quick!", "a jumps 1234 over"];
  const words = pool(rows, [], 3, rnd);
  assert(words.length === 3 && words.every((w) => ["quick", "brown", "jumps", "over"].includes(w)), words.join());
  // Kept while in view, the rest drawn anew.
  assertEqual(pool(rows, ["over", "gone"], 2, rnd)[0], "over");
  assert(pool(rows, ["over", "gone"], 2, rnd)[1] !== "gone");
  assertEqual(pool([""], ["over"], 3, rnd), []);
  // Cut to `size`, the oldest kept; `avoid` not drawn.
  assertEqual(pool(rows, ["over", "quick", "fox"], 1, rnd), ["over"]);
  for (let i = 0; i < 10; i++) assert(!pool(rows, [], 4, rnd, ["over"]).includes("over"));
}

export function test_shapes() {
  for (const s of Object.values(SHAPES)) {
    const steps = [...s.grow, ...s.sway];
    assert(steps.every((rows) => rows.length === s.h));
    assert(steps.every((rows) => rows.every((row) => row.length === s.w)));
    assert(steps.every((rows) => rows[rows.length - 1][s.root] !== " ")); // the root
  }
}

export function test_paint() {
  const c = { petals: 1, alt: 2, heart: 3, stem: 4 };
  const rows = paint(SHAPES.large.sway[1], c, 5);
  assert(rows.length === 7 && rows.every((r) => r.length === 7));
  assertEqual(rows[2][3], { t: "o", fg: 3, bold: true });
  assert(rows[0][1].fg === 1 && rows[0][2].fg === 2);
  assertEqual(rows[6][3], { t: "|", fg: 4 });
  assertEqual(rows[0][0], { t: " " });
  assert(paint(["ab"], c, 1, true).flat().every((r) => r.dim));
}

export function test_colors() {
  const rnd = seeded(3);
  const all = Array.from({ length: 40 }, () => colors(rnd));
  // Of two colours or one; a heart and a stem of their lists.
  assert(all.some((c) => c.alt === c.petals) && all.some((c) => c.alt !== c.petals));
  assert(all.every((c) => [220, 226, 214, 229, 231].includes(c.heart) && [28, 34, 40, 70, 76].includes(c.stem)));
}

export function test_found() {
  const rows = ["### flower ##", "  Flower#flowers", "flower"];
  assertEqual(found(rows, ["flower"]), [
    [4, 0, "flower"],
    [2, 1, "Flower"],
    [0, 2, "flower"],
  ]);
}

export function test_covered() {
  const cells = [
    [5, 3],
    [6, 3],
  ];
  assert(!covered(cells, ["", "", "", "     "], [0, 0]));
  assert(covered(cells, ["", "", "", "      #"], [0, 0]));
  assert(covered(cells, ["      #"], [0, 3]));
}

export function test_options() {
  assertEqual(options({}), { word: "flower", anywhere: false, count: 1 });
  assertEqual(options({ anywhere: true, count: null }).count, 5);
  assertEqual(options({ anywhere: true, count: 0, word: "rose" }), { word: "rose", anywhere: true, count: 0 });
}

/** A fake host: the calls logged, sprite ids from 1. */
function host() {
  const log = [];
  let id = 0;
  const call = (method, params) => {
    log.push([method, params]);
    return method === "draw.put" ? { id: ++id } : {};
  };
  return { log, call };
}

export function test_garden() {
  const h = host();
  const g = new Garden(options({}), { call: h.call, rnd: seeded(1) });
  const rows = Array(12).fill(" ".repeat(40));
  rows[8] = "a flower".padEnd(40);
  const heat = () => Array(12).fill(".".repeat(40));
  g.scan({ rows, origin: [0, 0], changed: true, frame: 0 }, heat, 0);
  const puts = h.log.filter(([m]) => m === "draw.put").map(([, p]) => p);
  assertEqual(puts.length, 2);
  // The word in bold, a tint in front, at the match.
  assertEqual(puts[1], {
    content: { rows: ["flower"], style: { bold: true } },
    anchor: { at: [2, 8], space: "frame" },
    blend: "tint",
    layer: "front",
  });
  // One flower: a second scan plants none.
  g.scan({ rows, origin: [0, 0], changed: false, frame: 0 }, heat, 500);
  assertEqual(h.log.filter(([m]) => m === "draw.put").length, 2);
  // It grows, then sways.
  for (let t = 0; t <= 4 * STEP; t += 50) g.tend(t);
  const ups = h.log.filter(([m]) => m === "draw.update").map(([, p]) => p);
  assertEqual(ups.length, 2);
  assert(ups[1].loop === true && ups[1].frames.length === 4);
  // The word gone: it wilts, the bold removed.
  const blank = Array(12).fill(" ".repeat(40));
  g.scan({ rows: blank, origin: [0, 0], changed: true, frame: 0 }, heat, 2000);
  assert(h.log.some(([m, p]) => m === "draw.remove" && p.id === 2));
  const last = h.log.filter(([m]) => m === "draw.update").pop()[1];
  assert(last.loop === false && last.frames.every((f) => f.cells.flat().every((r) => r.t === " " || r.dim)));
  g.tend(2000 + 250 * 3);
  assertEqual(g.flowers.length, 0);
}

export function test_garden_count() {
  // --count 0: a flower a match, one a scan.
  const h = host();
  const g = new Garden(options({ count: 0 }), { call: h.call, rnd: seeded(2) });
  const rows = Array(24).fill(" ".repeat(60));
  rows[5] = "flower".padEnd(60);
  rows[16] = `${" ".repeat(30)}flower`.padEnd(60);
  const heat = () => Array(24).fill(".".repeat(60));
  for (let t = 0; t <= 1500; t += 500) g.scan({ rows, origin: [0, 0], changed: t === 0, frame: 0 }, heat, t);
  assertEqual(g.flowers.length, 2);
  assertEqual(new Set(g.flowers.map((f) => f.key)).size, 2);
}
