// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of train, `ptyrc dev test --unit`: the twin's scenes,
// skies and draws to the bit (`twin.js`), test_train.py; sky.js's and
// strip.js's: `_shared/js/sky.tests.js`, `strip.tests.js`.

import { assert, assertEqual, assertThrows } from "ptyrc";
import {
  BRAKE,
  CAR,
  CARS,
  CYCLE,
  DARK,
  EVENTS,
  FPS,
  IRON,
  KEEP,
  LAMP,
  LIVERY,
  LINES,
  MORTAR,
  PACE,
  RUSH,
  SIZES,
  SPAN,
  STAND,
  STONE,
  WHEN,
  approach,
  breath,
  cells,
  dist,
  eventOf,
  options,
  scene,
  start,
  string,
  trip,
  wires,
} from "./app.js";
import { at, blue, green, mix, ordinal, red, rgb, weather } from "./sky.js";
import { Half, Pointer, hexa, index } from "./strip.js";
import { land } from "./line.js";
import { CASES, DRAWS, SKIES, TRIPS } from "./twin.js";

export function fnv(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}
export const norm = (rows) => JSON.stringify(rows.map((row) => row.map((r) => [r.t, r.fg ?? null, r.bg])));

const tuple = (c) => [red(c), green(c), blue(c)];

/** The cases of the twin of the trip `ti` that differ, by index. */
function differ(ti) {
  const tr = trip(TRIPS[ti]);
  const bad = [];
  let seen = 0;
  CASES.forEach(([w, h, when, t, i, trail, hash], k) => {
    if (i !== ti) return;
    seen++;
    let pointer = null;
    if (trail) {
      pointer = new Pointer(() => t, 6.0);
      pointer.trail = trail.map((p) => [...p]);
    }
    if (fnv(norm(scene(w, h, when, t, tr, pointer))) !== hash) bad.push(k);
  });
  assert(seen > 5, `trip ${ti}: ${seen} cases`);
  return bad;
}

export function test_twin_0() {
  assertEqual(differ(0), []);
}
export function test_twin_1() {
  assertEqual(differ(1), []);
}
export function test_twin_2() {
  assertEqual(differ(2), []);
}
export function test_twin_3() {
  assertEqual(differ(3), []);
}
export function test_twin_4() {
  assertEqual(differ(4), []);
}
export function test_twin_5() {
  assertEqual(differ(5), []);
}
export function test_twin_6() {
  assertEqual(differ(6), []);
}
export function test_twin_7() {
  assertEqual(differ(7), []);
}
export function test_twin_8() {
  assertEqual(differ(8), []);
}
export function test_twin_9() {
  assertEqual(differ(9), []);
}
export function test_twin_10() {
  assertEqual(differ(10), []);
}
export function test_twin_11() {
  assertEqual(differ(11), []);
}
export function test_twin_12() {
  assertEqual(differ(12), []);
}
export function test_twin_13() {
  assertEqual(differ(13), []);
}

export function test_twin_skies() {
  for (const [when, lat, cover, hour, day, sun, sunX, moon, moonX, phase, light, stars, ambient, colour, shade, season, kind] of SKIES) {
    const s = at(when, lat, cover);
    const got = [s.hour, s.day, s.sun, s.sunX, s.moon, s.moonX, s.phase, s.light, s.stars];
    const want = [hour, day, sun, sunX, moon, moonX, phase, light, stars];
    // The libm of the interpreter is not the twin's: an ulp or so.
    assert(got.every((v, i) => Math.abs(v - want[i]) <= 1e-12 * Math.max(1, Math.abs(want[i]))), `${got} != ${want}`);
    assertEqual([tuple(s.ambient), tuple(s.colour(0.3, 0.7)), tuple(s.shade(0x5aa028, 0.5)), s.season], [ambient, colour, shade, season]);
    assertEqual(weather(ordinal(when), 7, s.season), kind);
  }
}

export function test_twin_draws() {
  const wall = Date.UTC(2026, 2, 7, 9, 41, 12);
  for (const [seed, line, drawnLine, season, days, kind, hm, after] of DRAWS) {
    const o = options({ random: true, line });
    const [, tr, a] = start(o, wall, seed);
    assertEqual([o.line, o.season, o.days, o.weather, o.at, a, tr.line, tr.seed], [drawnLine, season, days, kind, hm, after, drawnLine, seed]);
  }
}

// ---- test_train.py ----

const NOON = Date.UTC(2026, 6, 20, 12, 30);
const NIGHT = Date.UTC(2026, 6, 20, 23, 30);
const hourOf = (h) => Date.UTC(2026, 6, 20, h, 30);
const ALL = { seed: 4, land: true, poles: true, events: true, seasons: true, weather: "day" };
const all = (o = {}) => trip({ ...ALL, ...o });

/** The first cycle of `tr` with the event `what`. */
function cycle(tr, what) {
  for (let k = 0; k < 200; k++) if (eventOf(tr, k) === what) return k;
  throw new Error(`no ${what}`);
}

/** A cell `[glyph, fg, bg]`, row by row. */
function pixels(rows) {
  const out = [];
  for (const row of rows) for (const run of row) for (const c of run.t) out.push([c, c === " " ? null : (run.fg ?? null), run.bg]);
  return out;
}
const key = (cells) => JSON.stringify(cells);
/** The background of each cell. */
const colours = (rows) => pixels(rows).map((p) => p[2]);
function grid(rows, w = 48) {
  const flat = colours(rows);
  const out = [];
  for (let i = 0; i < flat.length; i += w) out.push(flat.slice(i, i + w));
  return out;
}
const braille = (c) => c > "⠀" && c < "⤀";

function check(rows, w, h, deep = false) {
  assertEqual(rows.length, h);
  for (const row of rows) {
    const text = row.map((r) => r.t).join("");
    assertEqual([...text].length, w, text);
    for (const run of row) {
      assert(Object.keys(run).every((k) => ["t", "fg", "bg"].includes(k)) && "bg" in run);
      for (const k of ["fg", "bg"]) {
        if (!(k in run)) continue;
        if (deep) assert(typeof run[k] === "string" && run[k].length === 7);
        else assert(run[k] >= 16 && run[k] <= 255, `${k}: ${run[k]}`);
      }
    }
  }
}

export function test_events() {
  const bare = trip({ seed: 4 });
  for (let k = 0; k < 100; k++) assertEqual(eventOf(bare, k), null);
  const drawn = Array.from({ length: 300 }, (_, k) => eventOf(all(), k));
  assertEqual(new Set(drawn).size, EVENTS.length + 1);
  assert(drawn.some((e, k) => e !== eventOf(all({ seed: 5 }), k)));
  const none = drawn.filter((e) => e === null).length / 300;
  assert(0.05 < none && none < 0.3, `${none}`);
}

export function test_dist() {
  const bare = trip({ seed: 4 });
  assert(dist(bare, 0) === 0 && dist(bare, -5) === 0);
  assert(Math.abs(dist(bare, 1000) - 1000 * PACE) < 1e-6);
  const tr = all();
  let last = 0;
  for (let i = 1; i < 40 * CYCLE; i++) {
    const d = dist(tr, i / 4);
    assert(d - last >= 0 && d - last <= PACE / 4 + 1e-6, `${i}`);
    last = d;
  }
  const k = cycle(tr, "station");
  const begin = k * CYCLE + WHEN;
  const halt = begin + BRAKE;
  assertEqual(dist(tr, halt), dist(tr, halt + STAND));
  assert(dist(tr, halt - 0.5) < dist(tr, halt) && dist(tr, halt + STAND + 0.5) > dist(tr, halt));
  const after = (k + 1) * CYCLE;
  assert(Math.abs(dist(tr, after) - dist(tr, begin) - PACE * (after - begin - BRAKE - STAND)) < 1e-6);
  assert(dist(tr, 30000 * CYCLE) > 0);
}

// test_fits: every third hour, not every fourth.
export function test_fits() {
  for (const line of LINES) {
    for (const tr of [trip({ seed: 4, line }), all({ line })]) {
      for (const [w, h] of [...SIZES, [1, 1], [5, 2], [70, 16]]) {
        const when = hourOf((w * 7 + h) % 24);
        const rows = scene(w, h, when, 41.3, tr);
        check(rows, w, h);
        assertEqual(key(rows), key(scene(w, h, when, 41.3, tr)));
      }
    }
  }
}

export function test_it_goes() {
  const bare = trip({ seed: 4 });
  const frames = Array.from({ length: 30 }, (_, t) => pixels(scene(48, 11, NOON, t / 10, bare)));
  assert(new Set(frames.map(key)).size >= 25);
  assertEqual(new Set(frames.map((f) => key(f.slice(0, 48)))).size, 1);
  assert(new Set(frames.map((f) => key(f.slice(5 * 48, 9 * 48)))).size >= 15);
  assertEqual(key(scene(48, 11, NOON, 3.0, bare)), key(scene(48, 11, NOON + 40000, 3.0, bare)));
  assert(key(scene(48, 11, NOON, 3.0, bare)) !== key(scene(48, 11, NIGHT, 3.0, bare)));
}

export function test_options_add() {
  const seen = new Set([key(scene(48, 11, NOON, 5.0, trip({ seed: 4 })))]);
  for (const more of [{ land: true }, { poles: true }, { weather: "rain" }, { line: "alps" }, { deep: true }]) seen.add(key(scene(48, 11, NOON, 5.0, trip({ seed: 4, ...more }))));
  assertEqual(seen.size, 6);
  const winter = Date.UTC(2026, 0, 20, 12, 30);
  assert(key(scene(48, 11, winter, 0.0, trip({ seed: 4 }))) !== key(scene(48, 11, winter, 0.0, trip({ seed: 4, seasons: true }))));
  check(scene(48, 11, NOON, 0.0, all({ deep: true })), 48, 11, true);
}

export function test_wires() {
  const tr = all({ weather: "clouds", events: false });
  const off = all({ weather: "clouds", events: false, poles: false });
  const wood = index(at(NOON, 47.0, 0.25).shade(rgb(92, 66, 44)));
  let glyphs = 0;
  for (let step = 0; step < 240; step += 7) {
    const a = pixels(scene(48, 11, NOON, step / FPS, tr));
    const b = pixels(scene(48, 11, NOON, step / FPS, off));
    a.forEach((cell, i) => {
      const old = b[i];
      if (key(cell) === key(old) || cell[1] === wood || cell[2] === wood) return;
      assert(i < 6 * 48, `${step} ${i}`);
      assert(braille(cell[0]), `${step} ${i}`);
      assert(old[0] !== " " || cell[2] === old[2], `${step} ${i}`);
      glyphs++;
    });
  }
  assert(glyphs > 1000, `${glyphs}`);
  // A thing by the glass hides them; a cell of two colours takes their mean.
  let half = new Half(4, 2);
  const sky = rgb(90, 120, 230);
  const leaf = rgb(0, 90, 0);
  const cloud = rgb(240, 240, 240);
  half.fill(sky);
  half.set(3, 1, cloud);
  const strung = [0, 1, 2, 3].map((x) => [x, 0, 0x09, sky, x === 3 ? cloud : sky]);
  half.set(1, 0, leaf);
  string(half, at(NOON), strung);
  assertEqual([...half.over.keys()].sort(), [0, 2, 3]);
  assert(half.over.get(0)[2] === sky && half.px[0][1] === leaf);
  const grey = mix(sky, cloud, 0.5);
  assert(half.over.get(3)[2] === grey && half.px[0][3] === grey && half.px[1][3] === grey);
  // One wire: a dot a column of dots.
  half = new Half(48, 11);
  const noon = at(NOON);
  const verge = land("plain", 0, 22, false).back.at(-1);
  const dots = wires(half, noon, 0.0, verge).reduce((n, [, , bits]) => n + bits.toString(2).replace(/0/g, "").length, 0);
  assertEqual(dots, 96);
  // The cells it may cross: one colour where their pixels are near.
  half = new Half(2, 11);
  half.fill(noon.colour(0.5, 0.5));
  const near = rgb(100, 100, 200);
  for (const [y, c] of [
    [2, near],
    [3, mix(near, 0, 0.05)],
    [4, rgb(240, 240, 240)],
  ])
    half.set(0, y, c);
  cells(half.px, 11);
  assert(half.px[2][0] === half.px[3][0] && half.px[4][0] !== half.px[5][0]);
  // The poles stand on the verge: they go by at its pace, a pixel a frame.
  const bare = all({ weather: "clouds", events: false, land: false, deep: true });
  const brown = hexa(at(NOON, 47.0, 0.25).shade(rgb(92, 66, 44)));
  const poles = (t) =>
    pixels(scene(48, 11, NOON, t, bare))
      .slice(3 * 48, 4 * 48)
      .flatMap(([, , bg], x) => (bg === brown ? [x] : []));
  const first = poles(2.0);
  assert(first.length > 0);
  assertEqual(poles(2.0 + 5 / FPS), first.filter((x) => x >= 5).map((x) => x - 5));
}

// test_planes: a frame in nine, not one in three.
export function test_planes() {
  const tr = all({ weather: null, events: false, poles: false, deep: true });
  const lamp = hexa(LAMP);
  let lit = 0;
  let hidden = 0;
  for (let step = 0; step < 1500; step += 9) {
    const rows = grid(scene(48, 11, hourOf(21), step / FPS, tr));
    for (let y = 1; y < 10; y++) {
      for (let x = 1; x < 47; x++) {
        if (rows[y][x] !== lamp) continue;
        lit++;
        const around = new Set();
        for (const j of [-1, 1]) for (const i of [-1, 0, 1]) around.add(rows[y + j][x + i]);
        if (around.size === 1) hidden++;
      }
    }
  }
  assert(lit > 15 && hidden === 0, `${lit} ${hidden}`);
}

export function test_tunnel() {
  const tr = all({ weather: null, deep: true });
  const begin = cycle(tr, "tunnel") * CYCLE + WHEN;
  const mouth = begin + approach(22) / PACE;
  const wall = [hexa(MORTAR), hexa(DARK)];
  const inside = scene(48, 11, NOON, mouth + 4.0, tr);
  const set = new Set(colours(inside));
  assert(wall.every((c) => set.has(c)) && set.size > 8);
  assertEqual(key(inside), key(scene(48, 11, NIGHT, mouth + 4.0, tr)));
  assertEqual(key(scene(48, 11, NOON, mouth + 4.0, all({ weather: "rain", deep: true }))), key(inside));
  const a = grid(inside);
  const b = grid(scene(48, 11, NOON, mouth + 4.0 + 1 / FPS, tr));
  assert(a.every((row, y) => key(row.slice(2)) === key(b[y].slice(0, -2))) && key(a) !== key(b));
  const none = (cs) => !cs.some((c) => wall.includes(c));
  assert(none(colours(scene(48, 11, NOON, begin - 1.0, tr))));
  assert(none(colours(scene(48, 11, NOON, mouth + SPAN.tunnel + 5, tr))));
  const rising = grid(scene(48, 11, NOON, mouth - 0.3, tr));
  const sky = grid(scene(48, 11, NOON, begin - 1.0, tr))[0][47];
  assert(rising[0][0] === sky && rising[0][47] !== sky);
  assert(none(rising.flat()));
  const entering = grid(scene(48, 11, NOON, mouth + 0.9, tr));
  assert(!none(entering.map((row) => row[47])) && none(entering.map((row) => row[0])));
}

export function test_station() {
  const tr = all({ weather: null, deep: true });
  const k = cycle(tr, "station");
  const halt = k * CYCLE + WHEN + BRAKE;
  const stone = hexa(STONE);
  const standing = scene(48, 11, NOON, halt + 1, tr);
  assertEqual(key(standing), key(scene(48, 11, NOON, halt + STAND - 1, tr)));
  assert(standing.at(-1).every((run) => run.bg === stone));
  assert(key(standing) !== key(scene(48, 11, NOON, halt - 3, tr)));
  assert(!colours(scene(48, 11, NOON, k * CYCLE + 2.0, tr)).includes(stone));
  assert(!pixels(standing).some(([c]) => braille(c)));
}

export function test_other_train() {
  const tr = all({ weather: null, deep: true });
  const k = cycle(tr, "train");
  const begin = k * CYCLE + WHEN;
  const coat = hexa(LIVERY[k % LIVERY.length]);
  const passing = grid(scene(48, 11, NOON, begin + 1.5, tr));
  assert(passing[8].filter((c) => c === coat).length > 24 && !passing[0].includes(coat));
  const then = grid(scene(48, 11, NOON, begin + 1.5 + 1 / FPS, tr));
  for (let y = 3; y < 11; y++) assertEqual(key(passing[y].slice(6)), key(then[y].slice(0, -6)));
  const first = colours(scene(48, 11, NOON, begin + 2 / FPS, tr));
  assert(first.includes(hexa(rgb(255, 246, 206))) && first.filter((c) => c === coat).length < 48);
  const long = (48 + CAR * CARS) / (RUSH * FPS);
  for (const t of [begin - 1, begin + long + 0.1]) assert(!colours(scene(48, 11, NOON, t, tr)).includes(coat));
  assert(colours(scene(48, 11, NOON, begin + long - 0.2, tr)).includes(coat));
}

export function test_bridge() {
  const tr = all({ weather: null, deep: true });
  const begin = cycle(tr, "bridge") * CYCLE + WHEN;
  const iron = hexa(IRON);
  const mid = begin + SPAN.bridge / 2 + 1;
  const on = scene(48, 11, NOON, mid, tr);
  assertEqual(grid(on)[0].filter((c) => c === iron).length, 48);
  assert(colours(on).filter((c) => c === iron).length > 80);
  const before = scene(48, 11, NOON, begin - 1, tr);
  const banks = new Set(grid(before)[8]);
  assert(!grid(on)[8].some((c) => banks.has(c)));
  assert(!pixels(on).some(([c]) => braille(c)) && !colours(before).includes(iron));
  assert(!colours(scene(48, 11, NOON, begin + SPAN.bridge + 3, tr)).includes(iron));
  const a = grid(on);
  const b = grid(scene(48, 11, NOON, mid + 1 / FPS, tr));
  for (const y of [0, 10]) assertEqual(key(a[y].slice(1)), key(b[y].slice(0, -1)));
}

// test_events_fit: every third step, by day and by night in turn.
export function test_events_fit() {
  const tr = all({ weather: "snow" });
  for (const what of EVENTS) {
    const begin = cycle(tr, what) * CYCLE + WHEN;
    for (const [w, h] of [...SIZES, [1, 1], [5, 2], [70, 16]]) {
      for (let step = 0; step < 34; step += 3) check(scene(w, h, step % 2 ? NOON : NIGHT, begin - 1 + step * 0.9, tr), w, h);
    }
  }
}

export function test_breath() {
  const now = [10.0];
  const pointer = new Pointer(() => now[0], KEEP);
  const tr = trip({ seed: 4, deep: true });
  const clear = pixels(scene(48, 11, NOON, 10.0, tr));
  assertEqual(key(pixels(scene(48, 11, NOON, 10.0, tr, pointer))), key(clear));
  pointer.move(20, 5);
  const fogged = pixels(scene(48, 11, NOON, 10.0, tr, pointer));
  const changed = [];
  fogged.forEach((c, i) => key(c) !== key(clear[i]) && changed.push([i % 48, Math.floor(i / 48)]));
  assert(changed.length && changed.every(([x, y]) => Math.abs(x - 20) <= 3 && Math.abs(y - 5) <= 2));
  pointer.leave();
  const half = new Half(48, 11);
  const faint = new Half(48, 11);
  breath(half, pointer, 10.0);
  breath(faint, pointer, 10.0 + KEEP * 0.8);
  const sum = (c) => red(c) + green(c) + blue(c);
  assert(sum(half.px[11][20]) > sum(faint.px[11][20]) && sum(faint.px[11][20]) > 0);
  const late = 10.0 + KEEP + 0.1;
  assertEqual(key(scene(48, 11, NOON, late, tr, pointer)), key(scene(48, 11, NOON, late, tr)));
  breath(half, null, 0.0);
}

// test_random: the draws of a seed, the same each time; others apart.
export function test_random() {
  const wall = Date.UTC(2026, 2, 7, 9, 41, 12);
  const look = (seed) => {
    const [era, tr, after] = start(options({ all: true, random: true }), wall, seed);
    return key(scene(34, 8, era.when(after), after, tr));
  };
  assertEqual(look(11), look(11));
  assertEqual(new Set([11, 12, 13, 14, 15, 16, 17, 18, 19].map(look)).size, 9);
  const [, tr, after] = start(options({ random: true, line: "alps", after: 5 }), wall, 3);
  assert(tr.line === "alps" && after === 5);
}

export function test_options() {
  for (const bad of [{ speed: -1 }, { speed: 0 }, { at: "25:00" }, { at: "noon" }]) assertThrows(() => options(bad), RangeError);
  assertEqual(options({ at: "08:05" }).at, [8, 5]);
}
