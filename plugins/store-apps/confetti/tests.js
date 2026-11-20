// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of confetti, `ptyrc dev test --unit`: the show run on
// a clock of its own, what it draws logged (test_apps.py: the tests
// that patch `confetti.burst`, `pop`, `rocket`).

import { assert, assertEqual } from "ptyrc";
import {
  COLORS,
  PERIOD,
  POP,
  POPS,
  SHAPES,
  Show,
  flight,
  options,
  rocket,
  spot,
} from "./app.js";

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

function kind(s) {
  if (s.effect) return s.effect.name === "confetti" ? "burst" : s.effect.name;
  return s.motion ? "rocket" : "pop";
}

/** The cell a sprite is aimed at. */
function origin(s) {
  if (s.effect) return s.effect.params.origin;
  const [x, y] = s.anchor.at;
  if (s.motion) return [x, y - rise(s)];
  return [x + (s.frames[0].cells[0][0].t.length >> 1), y + (s.frames[0].cells.length >> 1)];
}

function rise(s) {
  const t = s.ttl_ms / 1000;
  return Math.round((-s.motion.vy * t) / 2);
}

/** The show of `args`, run on its own clock in steps of 10 ms over a
 * view `w`x`h` of `classes` (cold by default), until it ends or `stop`
 * sprites (sparkle aside); `[[ms, sprite]]`. */
function run(args, seed, { stop = Infinity, classes = null, w = 80, h = 24 } = {}) {
  const log = [];
  let now = 0;
  const heat = () => classes || Array(h).fill(".".repeat(w));
  const put = (s) => {
    if (kind(s) !== "sparkle") log.push([now, s]);
  };
  const show = new Show(options(args), { put, heat, rnd: seeded(seed) }, 0);
  while (show.step(now) && log.length < stop) now += 10;
  return log;
}

function apart(origins) {
  for (let i = 0; i < origins.length; i++) {
    for (let j = i + 1; j < origins.length; j++) {
      const [a, b] = [origins[i], origins[j]];
      if (Math.max(Math.abs(a[0] - b[0]), Math.abs(a[1] - b[1])) <= 3) return false;
    }
  }
  return true;
}

export function test_bursts() {
  const log = run({ "one-shot": true, bursts: 3, duration: 0.3, fancy: true }, 1);
  const times = new Set(log.map(([t]) => t));
  assertEqual(log.length, 3);
  assertEqual(times.size, 3);
  assert([...times].every((t) => t >= 0 && t < PERIOD));
  assert(apart(log.map(([, s]) => origin(s))));
  assertEqual(options({ bursts: 11 }), null);
  assertEqual(options({ bursts: 0 }), null);
}

export function test_party() {
  const log = run({ bursts: 3, sparkle: true, fancy: true }, 2, { stop: 9 });
  assertEqual(
    log.map(([t]) => Math.floor(t / PERIOD)),
    [0, 0, 0, 1, 1, 1, 2, 2, 2],
  );
  for (let i = 0; i + 3 <= 9; i++) assert(apart(log.slice(i, i + 3).map(([, s]) => origin(s))));
  // Not persistent: they go with the app.
  assert(log.every(([, s]) => !s.persist));
}

export function test_pops() {
  const log = run({ "one-shot": true, bursts: 2 }, 4);
  assertEqual(log.map(([, s]) => kind(s)), Array(8).fill("pop"));
  const colours = log.map(([, s]) => s.frames[0].cells[0][0].fg).sort();
  assertEqual(colours, [...COLORS, ...COLORS].sort());
  const frames = (art) => art.map((rows) => rows.join("\n")).join("|");
  const look = (s) => s.frames.map((f) => f.cells.map((r) => r[0].t).join("\n")).join("|");
  assert(log.every(([, s]) => look(s) === frames(POP) && s.persist));
  assert(apart(log.map(([, s]) => origin(s))));
  const times = log.map(([t]) => t);
  assertEqual(times, [...times].sort((a, b) => a - b));
  assert(times[times.length - 1] < PERIOD + 1010);
  // `--random`: of several kinds, each of frames of one size.
  const kinds = run({ "one-shot": true, bursts: 5, random: true }, 1).map(([, s]) => look(s));
  assert(new Set(kinds).size > 2);
  for (const art of POPS) {
    assertEqual(new Set(art.map((rows) => [rows.length, ...rows.map((r) => r.length)].join())).size, 1);
  }
}

export function test_fireworks() {
  const log = run({ "one-shot": true, bursts: 3, fireworks: true, fancy: true }, 6);
  const rockets = log.filter(([, s]) => kind(s) === "rocket");
  const bursts = log.filter(([, s]) => kind(s) === "burst");
  assertEqual([rockets.length, bursts.length, log.length], [3, 3, 6]);
  for (const [at, r] of rockets) {
    // From the last row of the view to a cell of its upper part.
    const [x, y] = origin(r);
    assert(y < 16 && y + rise(r) === 23, `${x},${y} ${rise(r)}`);
    const [then, b] = bursts.find(([, b]) => b.effect.params.origin.join() === [x, y].join());
    assert(Math.abs(then - (at + r.ttl_ms)) <= 10, `${then} ${at} ${r.ttl_ms}`);
    assertEqual(b.effect.params.shape, "ball");
  }
  const plain = run({ "one-shot": true, fireworks: true }, 6).map(([, s]) => kind(s));
  assertEqual(plain.sort(), [...Array(4).fill("pop"), ...Array(4).fill("rocket")]);
}

export function test_all() {
  const log = run({ all: true, bursts: 4 }, 8, { stop: 40 });
  assertEqual([...new Set(log.map(([, s]) => kind(s)))].sort(), ["burst", "rocket"]);
  const bursts = log.filter(([, s]) => kind(s) === "burst").map(([, s]) => s.effect.params);
  assertEqual([...new Set(bursts.map((p) => p.shape))].sort(), [...SHAPES].sort());
  assert(new Set(bursts.map((p) => p.count)).size > 5);
  assert(bursts.some((p) => p.colors) && !bursts.every((p) => p.colors));
  const looks = log.filter(([, s]) => kind(s) === "rocket").map(([, s]) => s.frames[0].cells[0][0].t);
  assert(new Set(looks).size > 2);
}

export function test_rocket() {
  // Up a column, slower and slower, to its origin.
  const r = rocket([30, 4], 18, 900, 11, ["^", "!:'"]);
  const { vy, ay } = r.motion;
  const y = (ms) => r.anchor.at[1] + vy * (ms / 1000) + (ay * (ms / 1000) ** 2) / 2;
  assertEqual(r.anchor.at, [30, 22]);
  assert(Math.abs(y(900) - 4) < 1e-9);
  const ys = Array.from({ length: 10 }, (_, i) => y(i * 100));
  assert(ys.every((v, i) => i === 0 || v < ys[i - 1]));
  assert(ys[0] - y(450) > y(450) - y(900));
  assertEqual(r.ttl_ms, 900);
  assertEqual(r.frames[0].cells.length, 4); // the head, a trail of three
  assertEqual(flight(999), 1600);
}

export function test_spot() {
  const rnd = seeded(7);
  // Left half hot, right half static text and free cells.
  const classes = Array(10).fill("h".repeat(20) + "t.".repeat(10));
  let avoid = [];
  for (let i = 0; i < 200; i++) {
    const [x, y] = spot(classes, rnd, avoid);
    assert(x >= 20 && x < 40 && y >= 0 && y < 10);
    assert(apart([...avoid, [x, y]]));
    avoid = [...avoid, [x, y]].slice(-2);
  }
  // All hot: still a cell. Smaller than the gaps: still a cell.
  const [x, y] = spot(["hhh", "hhh"], rnd, [[0, 0], [1, 1]]);
  assert(x >= 0 && x < 3 && y >= 0 && y < 2);
  assertEqual(spot([], rnd), [0, 0]);
}

/** `spot` as the plain double loop. */
function plain(classes, rnd, avoid, reach) {
  const h = classes.length;
  const w = Math.max(...classes.map((r) => r.length));
  const cold = classes.map((r) => Array.from(r.padEnd(w, "."), (c) => c !== "h"));
  const cells = [];
  const weights = [];
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (avoid.some(([i, j]) => Math.abs(x - i) <= reach && Math.abs(y - j) <= reach)) continue;
      let n = 0;
      let all = 0;
      for (let j = Math.max(0, y - reach); j < Math.min(h, y + reach + 1); j++) {
        for (let i = Math.max(0, x - reach); i < Math.min(w, x + reach + 1); i++) {
          n += cold[j][i] ? 1 : 0;
          all += 1;
        }
      }
      cells.push([x, y]);
      weights.push(cold[y][x] ? n / all : 0);
    }
  }
  const total = weights.reduce((a, b) => a + b, 0);
  if (!total) return cells[Math.floor(rnd() * cells.length)];
  let r = rnd() * total;
  for (let i = 0; i < cells.length; i++) {
    r -= weights[i];
    if (r < 0) return cells[i];
  }
  return cells[cells.length - 1];
}

export function test_spot_weights() {
  // The pick is the one of the plain double loop, cell for cell.
  const draw = seeded(11);
  const int = (a, b) => a + Math.floor(draw() * (b - a + 1));
  for (let n = 0; n < 60; n++) {
    const w = int(8, 30);
    const h = int(8, 14);
    const classes = Array.from({ length: h }, () =>
      Array.from({ length: int(1, w) }, () => "hht.p"[int(0, 4)]).join(""),
    );
    classes[0] = classes[0].padEnd(w, "h");
    const avoid = Array.from({ length: n % 3 }, () => [int(0, w - 1), int(0, h - 1)]);
    const reach = int(0, 3);
    const seed = int(0, 1 << 30);
    assertEqual(spot(classes, seeded(seed), avoid, reach), plain(classes, seeded(seed), avoid, reach));
  }
}

export function test_avoids_hot() {
  // Rows 0 to 11 hot: every shot below them.
  const classes = [...Array(12).fill("h".repeat(79)), ...Array(12).fill(".".repeat(79))];
  const log = run({ bursts: 2, duration: 0.3, fancy: true }, 2, { stop: 13, classes });
  assertEqual(log.length, 13);
  assert(log.every(([, s]) => origin(s)[1] >= 12));
}
