// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of window, `ptyrc dev test --unit`: the twin's scenes,
// rooms, a cat stepped and draws to the bit (`twin.js`), test_window.py;
// sky.js's and strip.js's: `_shared/js/sky.tests.js`, `strip.tests.js`.

import { assert, assertEqual, assertThrows } from "ptyrc";
import {
  COVER,
  PLACES,
  SIZES,
  against,
  apart,
  bars,
  CAT,
  curtain,
  drawCat,
  lit,
  look,
  nearest,
  moving,
  options,
  outside,
  pace,
  painters,
  perch,
  room,
  scene,
  sillOf,
  start,
  tailOf,
} from "./app.js";
import { Era, MIDDLE, SEASONS, WEATHERS, at, blue, fields, green, moment, red, seasonOf, yday } from "./sky.js";
import { Half, LAMP, Pointer, hexa } from "./strip.js";
import { Cat } from "./mouser.js";
import { CASES, DRAWS, LOOKS, POINTERS, ROOMS, STEPPED } from "./twin.js";

export function fnv(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}
export const norm = (rows) => JSON.stringify(rows.map((row) => row.map((r) => [r.t, r.fg ?? null, r.bg])));
const tuple = (c) => [red(c), green(c), blue(c)];

/** The pointer of the events `evs` at `t`, its clock in `clock[0]`. */
export function pointerOf(evs, t, clock = [0.0]) {
  const p = new Pointer(() => clock[0]);
  for (const ev of evs) {
    if (ev[ev.length - 1] > t) break;
    clock[0] = ev[ev.length - 1];
    if (ev[0] === null) p.leave();
    else p.move(ev[0], ev[1]);
  }
  return p;
}

/** The cases of the twin of the looks `lo` to `hi` that differ, by index. */
function differ(lo, hi, pointed = false) {
  const bad = [];
  let seen = 0;
  CASES.forEach(([w, h, when, t, li, pi, hash], k) => {
    if (li < lo || li >= hi || (pi !== null) !== pointed) return;
    seen++;
    const pointer = pi === null ? null : pointerOf(POINTERS[pi], t);
    if (fnv(norm(scene(w, h, when, t, look(LOOKS[li]), pointer))) !== hash) bad.push(k);
  });
  assert(seen > 5, `looks ${lo}..${hi}: ${seen} cases`);
  return bad;
}
export function test_twin_0() {
  assertEqual(differ(0, 2), []);
}
export function test_twin_2() {
  assertEqual(differ(2, 4), []);
}
export function test_twin_4() {
  assertEqual(differ(4, 6), []);
}
export function test_twin_6() {
  assertEqual(differ(6, 8), []);
}
export function test_twin_8() {
  assertEqual(differ(8, 10), []);
}
export function test_twin_10() {
  assertEqual(differ(10, 12), []);
}
export function test_twin_12() {
  assertEqual(differ(12, 14), []);
}
export function test_twin_14() {
  assertEqual(differ(14, 16), []);
}
export function test_twin_16() {
  assertEqual(differ(16, 18), []);
}
export function test_twin_18() {
  assertEqual(differ(18, 20), []);
}
export function test_twin_20() {
  assertEqual(differ(20, 22), []);
}
export function test_twin_pointer() {
  assertEqual(differ(0, 22, true), []);
}

function rooms(lo, hi) {
  const bad = [];
  for (const [li, w, h, season, kind, frame, cloth, coat, at] of ROOMS) {
    if (li < lo || li >= hi) continue;
    const r = room(look(LOOKS[li]), w, h, season, kind);
    const got = [tuple(r.frame), tuple(r.cloth), tuple(r.coat), r.perch];
    if (JSON.stringify(got) !== JSON.stringify([frame, cloth, coat, at])) bad.push([li, w, h, season, kind]);
  }
  return bad;
}
export function test_twin_rooms_0() {
  assertEqual(rooms(0, 11), []);
}
export function test_twin_rooms_11() {
  assertEqual(rooms(11, 22), []);
}

export function test_twin_stepped() {
  for (const [li, w, h, far, frames] of STEPPED) {
    const lk = look(LOOKS[li]);
    const clock = [0.0];
    const pointer = new Pointer(() => clock[0]);
    const cat = new Cat();
    const got = [];
    for (let n = 0; n < frames.length; n++) {
      const t = (clock[0] = n / 12);
      if (n % 6 === 0 && n < 90) pointer.move(far + (Math.floor(n / 6) % 2), h - 5);
      if (n === 90) pointer.leave();
      got.push([fnv(norm(scene(w, h, Date.UTC(2026, 6, 20, 12, 30), t, lk, pointer, cat))), cat.busy]);
    }
    const bad = got.map((g, i) => (g[0] === frames[i][0] && g[1] === frames[i][1] ? -1 : i)).filter((i) => i >= 0);
    assertEqual(bad, []);
  }
}

export function test_twin_draws() {
  const wall = Date.UTC(2026, 2, 7, 9, 41, 12);
  for (const [seed, place, drawnPlace, season, days, kind, hm] of DRAWS) {
    const o = options({ random: true, place });
    const [, lk] = start(o, wall, seed);
    assertEqual([o.place, o.season, o.days, o.weather, o.at, lk.place, lk.seed], [drawnPlace, season, days, kind, hm, drawnPlace, seed]);
  }
}

// ---- test_window.py ----

const NOON = Date.UTC(2026, 6, 20, 12, 30);
const NIGHT = Date.UTC(2026, 6, 20, 23, 30);
const hourOf = (when, hour, minute = 30) => when - (fields(when).hour * 60 + fields(when).minute) * 60000 + (hour * 60 + minute) * 60000;
const ALL = { seed: 5, land: true, seasons: true, weather: "day", passing: true, cosy: true };
const with_ = (o, more) => look({ ...o, ...more });

/** `rows` fill a box of `w` by `h`: narrow glyphs, colours of the 256. */
function check(rows, w, h, deep = false) {
  assertEqual(rows.length, h);
  for (const row of rows) {
    const text = row.map((r) => r.t).join("");
    assertEqual([...text].length, w, text);
    for (const c of text) {
      const n = c.codePointAt(0);
      assert(n >= 0x20 && !(n >= 0x7f && n < 0xa0) && !(n >= 0x1100 && n < 0x115f) && !(n >= 0x2e80 && n < 0xa4d0) && n < 0xac00, `wide or control: ${n}`);
    }
    for (const run of row) {
      assert(Object.keys(run).every((k) => k === "t" || k === "fg" || k === "bg") && "bg" in run, JSON.stringify(run));
      for (const key of ["fg", "bg"]) {
        if (!(key in run)) continue;
        if (deep) assert(typeof run[key] === "string" && run[key].length === 7, run[key]);
        else assert(run[key] >= 16 && run[key] <= 255, `${run[key]}`);
      }
    }
  }
}

/** A cell a triple `[glyph, fg, bg]`, row by row. */
function pixels(rows) {
  const out = [];
  for (const row of rows) for (const run of row) for (const c of run.t) out.push([c, c !== " " ? (run.fg ?? null) : null, run.bg]);
  return out;
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function fits(place, base) {
  const lk = with_(base, { place });
  for (const [w, h] of [...SIZES, [1, 1], [3, 2], [61, 17]]) {
    for (let hour = 0; hour < 24; hour += 9) {
      const when = hourOf(NOON, hour);
      const rows = scene(w, h, when, 12.5, lk);
      check(rows, w, h);
      assert(same(rows, scene(w, h, when, 12.5, lk)), "pure"); // pure
    }
  }
}
export function test_fits_bare() {
  for (const place of PLACES) fits(place, { seed: 5 });
}
export function test_fits_all_0() {
  for (const place of PLACES.slice(0, 2)) fits(place, ALL);
}
export function test_fits_all_2() {
  for (const place of PLACES.slice(2)) fits(place, ALL);
}

export function test_weathers() {
  const plain = pixels(scene(40, 12, NOON, 3.0, with_(ALL, { weather: null })));
  for (const kind of WEATHERS) {
    const lk = with_(ALL, { weather: kind });
    const rows = scene(40, 12, NOON, 3.0, lk);
    check(rows, 40, 12);
    assert(kind === "clear" || !same(pixels(rows), plain), kind); // one cloud, or none
    const falls = kind === "rain" || kind === "snow";
    assertEqual(pixels(rows).some(([c]) => c.charCodeAt(0) > 0x2800 && c.charCodeAt(0) < 0x2900), falls, kind);
    if (falls) assert(!same(pixels(rows), pixels(scene(40, 12, NOON, 3.5, lk))), "it falls");
  }
}

export function test_bare_is_still() {
  const lk = look({ seed: 5 });
  assert(!moving(lk) && moving(look(ALL)));
  assert(same(scene(40, 12, NOON, 0.0, lk), scene(40, 12, NOON, 99, lk)));
  assert(!same(scene(40, 12, NOON, 0.0, lk), scene(40, 12, NIGHT, 0, lk)));
  const day = new Set(pixels(scene(40, 12, NOON, 0.0, lk)).map(([, , bg]) => bg));
  assert(day.size >= 5);
  assert(pixels(scene(40, 12, NIGHT, 0.0, lk)).some(([c]) => "·✦⠁⠂⠄⠈⠐⠠".includes(c)), "stars");
  assert(!pixels(scene(40, 12, NOON, 0, lk)).some(([c]) => "·✦".includes(c)));
  assert(!same(pixels(scene(40, 12, NOON, 0.0, lk)), pixels(scene(40, 12, NOON, 0.0, look({ seed: 6 })))));
}

export function test_sky_by_the_minute() {
  const lk = with_(ALL, { weather: null });
  const rows = scene(40, 12, NOON + 5000, 3.0, lk);
  assert(same(rows, scene(40, 12, NOON + 50000, 3.0, lk)));
  assert(!same(rows, scene(40, 12, hourOf(NOON, 19), 3.0, lk)));
}

export function test_options_add() {
  const bare = { seed: 5 };
  const dusk = hourOf(NOON, 21, 40);
  const seen = new Set([JSON.stringify(pixels(scene(40, 12, dusk, 2.0, look(bare))))]);
  for (const more of [{ land: true }, { weather: "clouds" }, { cosy: true }, { land: true, passing: true }, { deep: true }])
    seen.add(JSON.stringify(pixels(scene(40, 12, dusk, 2.0, with_(bare, more)))));
  assertEqual(seen.size, 6);
  // The season shows with `seasons` only.
  const winter = Date.UTC(2026, 0, 20, 12, 30);
  assert(!same(scene(40, 12, winter, 0.0, look(bare)), scene(40, 12, winter, 0.0, with_(bare, { seasons: true }))));
  check(scene(40, 12, NOON, 0.0, with_(ALL, { deep: true })), 40, 12, true);
}

export function test_passing() {
  const lk = look({ seed: 5, land: true, passing: true });
  const frames = new Set();
  for (let t = 0; t < 200; t += 3) frames.add(JSON.stringify(pixels(scene(40, 12, NOON, t, lk))));
  assert(frames.size > 20, `${frames.size}`);
  let birds = false;
  for (let t = 0; t < 400 && !birds; t++) birds = pixels(scene(40, 12, NOON, t, lk)).some(([c]) => "⠑⠊⠒".includes(c));
  assert(birds, "no bird");
  let meteor = false;
  for (let t = 0; t < 600 && !meteor; t++) meteor = pixels(scene(40, 12, NIGHT, t / 5, lk)).some(([c, fg]) => fg === 230 && c.charCodeAt(0) > 0x2800 && c.charCodeAt(0) < 0x2900);
  assert(meteor, "no meteor in two minutes");
}

export function test_hover() {
  const clock = [0.0];
  const pointer = new Pointer(() => clock[0]);
  const lk = look({ seed: 5, land: true, cosy: true, deep: true });
  const away = scene(40, 12, NOON, 0.0, lk, pointer);
  assert(same(away, scene(40, 12, NOON, 0.0, lk)));
  assert(curtain(40, 0.0) > curtain(40, 1.0) && curtain(40, 1.0) >= 1);
  assert(curtain(7, 0.0) === 1 && curtain(7, 1.0) === 1);
  pointer.move(30, 8);
  clock[0] = 5.0;
  const here = scene(40, 12, NOON, 5.0, lk, pointer);
  assert(!same(here, away));
  assert(!same(pixels(here)[4], pixels(away)[4]), "the curtain went aside");
  pointer.leave();
  assert(!same(scene(40, 12, NOON, 5.0, lk, pointer), away), "it closes");
  assert(same(scene(40, 12, NOON, 30.0, lk, pointer), scene(40, 12, NOON, 30.0, lk)), "the curtains drawn, the cat home");
}

function fur(lk, t, pointer, cat = null) {
  const half = new Half(40, 12);
  drawCat(half, at(NOON), t, pointer, room(lk, 40, 12, "summer", null), cat, lk.seed);
  const out = new Set();
  half.px.forEach((row, y) => row.forEach((c, x) => c !== 0 && out.add(`${x},${y}`)));
  return out;
}
const xy = (set) => [...set].map((k) => k.split(",").map(Number));

export function test_cat_hunts() {
  const lk = look({ seed: 5, cosy: true });
  const r = room(lk, 40, 12, "summer", null);
  const home = r.perch[0] + 2;
  const sill = sillOf(40, 12, lk.seed, r.perch);
  assert(sill.home === home && sill.lo < home && home < sill.hi);
  assert(sill.lo <= sill.left && sill.right <= sill.hi);
  const far = home > 20 ? sill.lo : sill.hi;
  assert(sill.cols.some((c) => Math.min(home, far) < c && c < Math.max(home, far)));
  const clock = [0.0];
  const pointer = new Pointer(() => clock[0]);
  const cat = new Cat();
  assert(same([...fur(lk, 0.0, pointer, cat)], [...fur(lk, 0.0, null)]));
  const mids = [];
  const tops = [];
  for (let n = 0; n < 72; n++) {
    const t = (clock[0] = n / 12);
    if (n % 6 === 0) pointer.move(far + (Math.floor(n / 6) % 2), 7);
    const f = fur(lk, t, pointer, cat);
    assert(same([...f].sort(), [...fur(lk, t, pointer)].sort()), `${t}`);
    const cells = xy(f);
    mids.push(cells.reduce((a, [x]) => a + x, 0) / cells.length);
    tops.push(Math.min(...cells.map(([, y]) => y)));
  }
  assert(Math.abs(mids[0] - home) < 2 && Math.abs(mids[mids.length - 1] - far) < 5);
  assert(Math.min(...tops) < tops[0] - 2, "it leaps");
  assert(cat.busy);
  pointer.leave();
  clock[0] = 40.0;
  assert(same([...fur(lk, 40.0, pointer, cat)].sort(), [...fur(lk, 40.0, null)].sort()));
  assert(!cat.busy);
}

export function test_cat_alone() {
  const lk = look({ seed: 5, land: true, passing: true, cosy: true });
  const shots = [];
  for (let t = 0; t < 240; t += 8) shots.push(fnv(norm(scene(40, 12, NOON, t / 2, lk))));
  for (let t = 0, i = 0; t < 240; t += 8, i++) assertEqual(fnv(norm(scene(40, 12, NOON, t / 2, lk))), shots[i]);
  const seen = [];
  const cat = painters.cat;
  painters.cat = (...args) => {
    seen.push(args[args.length - 1]);
    cat(...args);
  };
  try {
    for (let t = 0; t < 240; t += 2) scene(40, 12, NOON, t / 2, lk);
  } finally {
    painters.cat = cat;
  }
  const preys = seen.filter((x) => x !== null);
  assert(preys.length && preys.length < seen.length && preys.every((x) => 0 <= x && x < 40));
}

function indoors(kind) {
  const saved = { ...painters };
  const cat = painters.cat;
  let cells = [];
  let under = 0;
  painters.cat = (half, ...args) => {
    const before = half.px.map((r) => r.slice());
    const over = new Set(half.over.keys());
    cat(half, ...args);
    const now = new Set();
    half.px.forEach((row, y) => row.forEach((c, x) => c !== before[y][x] && now.add((y >> 1) * 40 + x)));
    cells = [...now];
    under += cells.filter((k) => over.has(k)).length;
  };
  const lk = look({ seed: 5, land: true, passing: true, cosy: true, weather: kind });
  const pointer = new Pointer(() => 0.0);
  pointer.move(20, 11); // the eyes
  const frames = [];
  try {
    for (let n = 0; n < 240; n += 4) {
      const t = n / 4;
      frames.push([t, pixels(scene(40, 12, NOON, t, lk, pointer)), cells]);
    }
    assert(under > 20, `${under}: it falls on the cat`);
    for (const name of ["rain", "snow", "stars", "birds", "meteor"]) painters[name] = () => {};
    for (const [t, seen, painted] of frames) {
      assert(painted.length);
      const plain = pixels(scene(40, 12, NOON, t, lk, pointer));
      for (const k of painted) assert(same(seen[k], plain[k]), `${t} ${k}`);
    }
  } finally {
    Object.assign(painters, saved);
  }
}
export function test_cat_is_indoors_rain() {
  indoors("rain");
}
export function test_cat_is_indoors_snow() {
  indoors("snow");
}

const CATS = SIZES.filter(([w, h]) => w >= 18 && h >= 6);
const ODD = [
  [18, 6],
  [19, 7],
  [25, 9],
  [33, 8],
  [34, 6],
  [35, 11],
  [61, 17],
  [200, 60],
];

export function test_cat_clear_of_the_bars() {
  for (const [w, h] of [...CATS, ...ODD]) {
    const [cols, rail] = bars(w, h);
    const side = curtain(w, 0.0);
    const seen = new Set();
    for (let seed = 0; seed < 201; seed++) {
      const p = perch(w, h, seed);
      assert(p !== null);
      const [left, way] = p;
      const body = [0, 1, 2, 3, 4].map((i) => left + i);
      assert(side <= left && left + 4 < w - side);
      assert(![...body, ...tailOf(left, way)].some((x) => cols.includes(x)));
      assert(tailOf(left, way).every((x) => 0 <= x && x < w));
      assert(rail === null || rail < 2 * h - CAT.length - 1);
      seen.add(`${p}`);
    }
    assert(seen.size >= 2 || w < 19, `${w}x${h}`); // drawn from the seed; one place at 18
  }
}

export function test_cat_clear_of_the_bars_drawn() {
  for (const [w, h] of [...CATS, [19, 7], [35, 11]]) {
    const [cols] = bars(w, h);
    for (let seed = 0; seed < 200; seed += 7) {
      const half = new Half(w, h);
      const r = room(look({ seed, cosy: true, deep: true }), w, h, "summer", null);
      drawCat(half, at(NOON), 0.0, null, r);
      const xs = new Set();
      for (const row of half.px) row.forEach((c, x) => c !== 0 && xs.add(x));
      assert(xs.size && !cols.some((x) => xs.has(x)), `${w}x${h} ${seed}`);
    }
  }
}

/** The cat against what is behind it and against the mullions, by day
 * and by night, as the 256 show them; seed 0, 40x12 and 18x6, no rain (the twin: seeds
 * 0..7, 26x8, rain too). */
function standsOut(place) {
  const low = [];
  for (const season of SEASONS) {
    const month = MIDDLE[season][0];
    for (const kind of [null, "clouds", "fog"]) {
      for (let seed = 0; seed < 1; seed++) {
        for (const [w, h] of [
          [40, 12],
          [18, 6],
        ]) {
          const lk = look({ seed, place, land: true, seasons: true, cosy: true, weather: kind });
          const r = room(lk, w, h, season, kind);
          const [left] = r.perch;
          for (const hour of [12, 0]) {
            const sky = at(Date.UTC(2026, month - 1, 20, hour, 30), lk.lat, COVER[kind] ?? 0);
            const out = outside(w, h, lk, sky, season, kind, 0, null);
            const rows = 2 * h;
            const px = [];
            for (let x = left - 1; x < left + 6; x++) for (let y = rows - 6; y < rows; y++) px.push(nearest(out.px[y][x]));
            const fit = against(r.coat, [[sky.light, px]], true);
            low.push(Math.min(fit, apart(lit(r.coat, sky.light), lit(r.frame, sky.light))));
          }
        }
      }
    }
  }
  assert(Math.min(...low) >= 18, `${Math.min(...low)}`);
  assert(low.filter((v) => v < 25).length <= Math.floor(low.length / 50));
}
export function test_cat_stands_out_0() {
  standsOut(PLACES[0]);
  standsOut(PLACES[1]);
}
export function test_cat_stands_out_2() {
  standsOut(PLACES[2]);
  standsOut(PLACES[3]);
}

export function test_indoors_vary() {
  for (const place of PLACES) {
    const coats = new Set();
    const cloths = new Set();
    for (let seed = 0; seed < 40; seed++) {
      const r = room(look({ seed, place, land: true, seasons: true, cosy: true }), 40, 12, "summer", null);
      coats.add(r.coat);
      cloths.add(r.cloth);
    }
    assert(coats.size >= 4 && cloths.size >= 4, place);
  }
}

export function test_indoors_still() {
  for (let seed = 0; seed < 8; seed++) {
    const lk = with_(ALL, { seed, weather: "clouds", deep: true });
    const [left] = room(lk, 40, 12, "summer", "clouds").perch;
    const bar = bars(40, 12)[0][0];
    // A cell of the curtain, of a mullion, of the cat: it sits, or lies.
    const cells = [0, bar, 40 * 11 + left + 2];
    const frames = [10, 13, 40].map((t) => pixels(scene(40, 12, NOON, t, lk)));
    assert(!same(frames[0], frames[2]), "the clouds move");
    for (const seen of frames) assert(same(cells.map((i) => seen[i]), cells.map((i) => frames[0][i])));
  }
}

export function test_house_lit_under_the_pointer() {
  const lamp = hexa(LAMP);
  const evening = hourOf(NOON, 22, 0);
  const lamps = (rows) => rows.flat().filter((r) => r.fg === lamp || r.bg === lamp).length;
  // A land with a house: its window is lit at night, by itself.
  let lk = null;
  for (let n = 0; n < 40 && lk === null; n++) {
    const it = look({ seed: n, land: true, deep: true });
    if (lamps(scene(40, 12, evening, 0.0, it))) lk = it;
  }
  assert(lk !== null);
  assertEqual(lamps(scene(40, 12, NOON, 0.0, lk)), 0);
  let found = 0;
  for (let x = 0; x < 40 && !found; x++)
    for (let y = 0; y < 12 && !found; y++) {
      const pointer = new Pointer(() => 0.0);
      pointer.move(x, y);
      found = lamps(scene(40, 12, NOON, 0.0, lk, pointer));
    }
  assert(found >= 1);
}

export function test_era() {
  const begin = Date.UTC(2026, 10, 13, 9, 5, 7, 0);
  assertEqual(new Era(begin + 0.123).when(60), Date.UTC(2026, 10, 13, 9, 6, 7));
  assertEqual(new Era(begin, 1440.0).when(60), Date.UTC(2026, 10, 14, 9, 5, 7));
  assertEqual(new Era(begin, 1.0, [21, 30], "winter").start, Date.UTC(2026, 0, 20, 21, 30));
  for (const [season, [month, day]] of Object.entries(MIDDLE)) assertEqual(seasonOf(yday(Date.UTC(2026, month - 1, day))), season);
}

export function test_random() {
  const draw = (more, seed = 3) => {
    const o = options(more);
    const [, lk] = start(o, NOON, seed);
    return { ...o, seed: lk.seed, place: o.place };
  };
  const plain = draw({});
  assertEqual([plain.place, plain.season, plain.at, plain.weather, plain.days, plain.seasons], [null, null, null, null, 0, false]);
  const seen = [];
  for (let n = 0; n < 120; n++) seen.push(draw({ random: true }, n));
  assert(same(seen[7], draw({ random: true }, 7)), "a seed: the same draws");
  assertEqual(new Set(seen.map((a) => a.place)).size, PLACES.length);
  assertEqual(new Set(seen.map((a) => a.season)).size, SEASONS.length);
  assertEqual(new Set(seen.map((a) => a.weather)).size, WEATHERS.length);
  assert(seen.every((a) => a.weather !== "snow" || a.season === "winter"));
  assert(seen.every((a) => a.seasons && -28 <= a.days && a.days <= 28));
  assert(new Set(seen.map((a) => `${a.at}`)).size > 60);
  assert(seen.filter((a) => !(7 <= a.at[0] && a.at[0] < 18)).length > 30, "low suns, evenings");
  // What is given stays.
  const kept = draw({ random: true, place: "sea", at: "07:10", season: "spring" });
  assertEqual([kept.place, kept.at, kept.season, kept.days], ["sea", [7, 10], "spring", 0]);
  assertEqual(draw({ random: true, weather: "fog" }).weather, "fog");
  // The moments, by the sun.
  const dawn = moment("dawn", 5.0, 21.0, 0.5);
  assert(4.5 < dawn && dawn < 5.5);
  assert(19.9 < moment("golden", 5.0, 21.0, 0.0) && moment("golden", 5.0, 21.0, 0.0) < moment("golden", 5.0, 21.0, 1.0) && moment("golden", 5.0, 21.0, 1.0) < 21.0);
  assert(moment("night", 8.0, 16.5, 0.9) < 8.0);
  const f = fields(new Era(NOON, 1.0, [6, 0], "winter", -3).start);
  assertEqual([f.month, f.day, f.hour], [1, 17, 6]);
}

export function test_options() {
  assertThrows(() => options({ at: "25:00" }), RangeError);
  assertThrows(() => options({ speed: 0 }), RangeError);
  assertEqual(options({ at: "07:10" }).at, [7, 10]);
  const lk = look();
  assertEqual(pace(lk, 1.0, false), 20.0);
  assertEqual(pace(lk, 600.0, false), 0.25);
  assertEqual(pace(look({ cosy: true }), 1.0, false), 0.25);
  assertEqual(pace(lk, 1.0, true), 1 / 12);
}
