// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of snow: `ptyrc dev test --unit` (test_snow.py:
// test_gust, test_gusts, test_max_depth, test_all).

import { assert, assertEqual } from "ptyrc";
import { DEEP, GUST, Gusts, gust, options, params } from "./app.js";

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

const base = { rate: 14, wind: -1, cover: false, hover: false, all: false };

export function test_gust() {
  const rnd = seeded(4);
  const winds = Array.from({ length: 200 }, () => gust(1, rnd));
  assert(winds.every((w) => w >= -1 && w <= 2 && w !== 0), winds);
  assert(winds.filter((w) => w > 0).length > 130);
  assert(winds.some((w) => w < 0));
  // The effect's bound.
  assert(Array.from({ length: 50 }, () => gust(20, rnd)).every((w) => Math.abs(w) <= 20));
}

export function test_gusts() {
  const winds = [];
  const g = new Gusts(1, seeded(4), (w) => winds.push(w));
  const [lo, hi] = GUST;
  const a = g.next();
  assert(lo <= a && a <= hi);
  assertEqual(winds.length, 0);
  const b = g.next();
  assert(lo <= b && b <= hi);
  assertEqual(winds.length, 1);
}

export function test_max_depth() {
  assertEqual(DEEP, 3);
  const o = options({ ...base, cover: true });
  assertEqual(params(o, 7), { rate: 14, wind: -1, cover: true, max_depth: 3, seed: 7 });
  assertEqual(params(options({ ...base, cover: true, "max-depth": 0 }), 7).max_depth, 0);
}

export function test_all() {
  const seen = (args) => {
    const o = options({ ...base, ...args });
    return [o.cover, o.hover, o.maxDepth];
  };
  assertEqual(seen({ all: true }), [true, true, 0]);
  assertEqual(seen({ all: true, "max-depth": 2 }), [true, true, 2]);
  assertEqual(seen({}), [false, false, DEEP]);
  assertEqual(seen({ hover: true }), [false, true, DEEP]);
  assertEqual(params(options({ ...base, hover: true }), 1).pointer, true);
}
