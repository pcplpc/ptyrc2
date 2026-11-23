// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of strip.js: test_strip.py of the twin. Its pixels to
// the bit, through train's scenes: train/twin.js.

import { assert, assertEqual, assertThrows } from "ptyrc";
import { at, rgb, SEASONS } from "./sky.js";
import { Fields, GROUND, GROUNDS, Half, KINDS, LAMP, Layer, Pointer, SNOW, Strip, clouds, fog, heavens, lights, noise, paint, rain, snow, stars, water, wave } from "./strip.js";

const NOON = at(Date.UTC(2026, 5, 21, 12));
const NIGHT = at(Date.UTC(2026, 5, 21, 0));
const EVENING = at(Date.UTC(2026, 5, 21, 21, 30));
const key = (x) => JSON.stringify(x, (k, v) => (ArrayBuffer.isView(v) ? [...v] : v));
const channels = (c) => [c >> 16, (c >> 8) & 255, c & 255];
const sum = (c) => channels(c).reduce((a, b) => a + b, 0);
const columns = (half) => Array.from({ length: half.w }, (_, x) => half.px.map((row) => row[x]));

export function test_strip_noise() {
  const draws = Array.from({ length: 2000 }, (_, n) => noise(3, n));
  assert(draws.every((d) => d >= 0 && d < 1));
  const mean = draws.reduce((a, b) => a + b) / 2000;
  assert(0.45 < mean && mean < 0.55);
  const all = new Set();
  for (const s of [1, 2]) for (const a of [1, 2]) for (const b of [1, 2]) all.add(noise(s, a, b));
  assertEqual(all.size, 8);
  assertEqual(wave(9, 4.0), noise(9, 4));
  for (let x = 1; x < 500; x++) assert(Math.abs(wave(9, x / 50) - wave(9, (x - 1) / 50)) < 0.05);
  assert(Math.abs(wave(9, 13.3, 5) - wave(9, 3.3, 5)) < 1e-9);
  assert(noise(3, -7, -2) >= 0);
}

export function test_strip_profiles() {
  for (const kind of KINDS) {
    const strip = new Strip(11, kind, 12.0);
    const heights = Array.from({ length: 800 }, (_, i) => strip.height((i - 400) / 2));
    assert(heights.every((h) => h >= 0 && h <= 1), kind);
    if (kind === "flat") assertEqual(new Set(heights).size, 1);
    else {
      assert(Math.max(...heights) - Math.min(...heights) > 0.15, kind);
      const other = new Strip(12, kind, 12.0);
      assert(heights.some((h, i) => h !== other.height((i - 400) / 2)), kind);
    }
    const again = new Strip(11, kind, 12.0, 4);
    for (let x = 0; x < 48; x++) assert(Math.abs(again.height(x) - again.height(x + 48)) < 1e-9, kind);
  }
}

export function test_strip_kinds() {
  assertThrows(() => new Strip(1, "swamp"), RangeError);
  const city = new Strip(5, "city", 6);
  for (let x = 0; x < 200; x++) {
    const [n, left, wide, tall] = city.block(x);
    assert(left <= x && x < left + 6 && 2 <= wide && wide <= 6);
    assertEqual(city.block(left), [n, left, wide, tall]);
    assertEqual(city.height(x), x - left < wide ? tall : 0.0);
  }
}

export function test_strip_standing() {
  const layer = new Layer(new Strip(3), 0.0, 10.0, 4.0, { things: ["tree", "house"], gap: 6, density: 0.7 });
  const things = layer.standing(0, 300);
  assertEqual(key(things), key(layer.standing(0, 300)));
  assert(things.every((t, i) => i === 0 || things[i - 1].x <= t.x));
  assertEqual(new Set(things.map((t) => t.kind)).size, 2);
  assert(things.every((t) => t.size >= 0 && t.size < 1 && t.tint >= 0 && t.tint < 1));
  assert(25 < things.length && things.length < 0.9 * 54);
  const within = (ts) => key(ts.filter((t) => 100 <= t.x && t.x < 160));
  assertEqual(within(layer.standing(100, 160)), within(things));
  assertEqual(new Layer(new Strip(3), 0.0, 10.0, 4.0).standing(0, 300), []);
}

export function test_strip_rate() {
  assertEqual(new Layer(new Strip(1), 0.0, 0, 0).rate(), 1);
  assertEqual(new Layer(new Strip(1), 1.0, 0, 0).rate(), 0);
  assertEqual(new Layer(new Strip(1), 0.5, 0, 0).rate(), 0.25);
  assertEqual(new Layer(new Strip(1), 0.5, 0, 0, { speed: 2.0 }).rate(), 2.0);
}

export function test_strip_paint_scrolls() {
  for (const depth of [0.0, 0.5]) {
    const layer = new Layer(new Strip(8, "hills", 9), depth, 14.0, 9.0, { things: ["tree", "pine"], gap: 7, size: 4.0 });
    const wide = new Half(60, 10);
    const narrow = new Half(30, 10);
    paint(wide, layer, NOON, 0.0, "summer");
    paint(narrow, layer, NOON, 16 / layer.rate(), "summer");
    assertEqual(key(columns(narrow).slice(0, 24)), key(columns(wide).slice(16, 40)));
    const ground = NOON.shade(layer.ground("summer"), depth);
    assert(wide.px.slice(15).every((row) => row.every((c) => c === ground)));
    assert(wide.px.slice(5, 14).some((row) => row.some((c) => c !== 0)));
    assert(wide.px[0].every((c) => c === 0));
  }
}

export function test_strip_paint_seasons() {
  const layer = new Layer(new Strip(8, "hills", 9), 0.0, 14.0, 6.0, { things: ["tree"], gap: 5 });
  const seen = {};
  for (const season of SEASONS) {
    const half = new Half(40, 10);
    paint(half, layer, NOON, 0.0, season);
    seen[season] = key(columns(half));
  }
  assertEqual(new Set(Object.values(seen)).size, 4);
  const white = NOON.shade(GROUND.winter);
  const half = new Half(40, 10);
  paint(half, layer, NOON, 0.0, "winter");
  assert(half.px[19][0] === white && Math.min(...channels(white)) > 200);
}

export function test_strip_houses() {
  const layer = new Layer(new Strip(2, "plain"), 0.0, 16.0, 2.0, { things: ["house"], gap: 10, density: 1.0, size: 6.0 });
  const half = new Half(60, 10);
  let marks = paint(half, layer, NIGHT, 0.0, "summer");
  const windows = marks.filter((m) => m.kind === "window");
  const ns = new Set(windows.map((m) => m.n));
  assert(windows.length >= 5 && marks.filter((m) => m.kind === "chimney").every((m) => ns.has(m.n)));
  const inside = windows.filter((m) => m.x >= 0 && m.x < 60);
  const dark = inside.map((m) => half.get(m.x, m.y));
  lights(half, NIGHT, marks, 4, null);
  assert(inside.some((m) => half.get(m.x, m.y) === LAMP) && !dark.includes(LAMP));
  const day = new Half(60, 10);
  marks = paint(day, layer, NOON, 0.0, "summer");
  lights(day, NOON, marks, 4, null);
  assert(marks.every((m) => day.get(m.x, m.y) !== LAMP));
  const pointer = new Pointer(() => 0.0);
  const here = marks.find((m) => m.kind === "window" && m.x >= 3 && m.x < 57);
  pointer.move(here.x + 1, Math.floor(here.y / 2));
  lights(day, NOON, marks, 4, pointer);
  assert(day.get(here.x, here.y) === LAMP && marks.filter((m) => day.get(m.x, m.y) === LAMP).length === 1);
  const night = new Half(60, 10);
  marks = paint(night, layer, NIGHT, 0.0, "summer");
  const first = marks.find((m) => m.kind === "window" && m.x >= 0 && m.x < 60);
  night.set(first.x, first.y, rgb(0, 90, 0));
  lights(night, NIGHT, marks, 4, null);
  pointer.move(first.x, Math.floor(first.y / 2));
  lights(night, NIGHT, marks, 4, pointer);
  assertEqual(night.get(first.x, first.y), rgb(0, 90, 0));
}

export function test_strip_city() {
  const layer = new Layer(new Strip(6, "city", 5), 0.2, 20.0, 14.0, { colour: rgb(100, 104, 120) });
  const night = new Half(60, 10);
  const day = new Half(60, 10);
  paint(night, layer, EVENING);
  paint(day, layer, NOON);
  const warm = night.px.flatMap((row) => [...row]).filter((c) => channels(c)[0] > 200 && channels(c)[0] > channels(c)[2]);
  assert(warm.length > 10);
  assert(!day.px.some((row) => row.some((c) => channels(c)[0] > 200 && channels(c)[0] > channels(c)[2] + 40)));
}

export function test_strip_heavens() {
  const half = new Half(40, 12);
  let clear = heavens(half, NOON, 14);
  assert(key(clear) === key(half.px) && clear !== half.px);
  assertEqual(half.px[0][2], NOON.colour(2.5 / 40, 0.5 / 14));
  assertEqual(key(half.px[20]), key(half.px[23]));
  const bright = [];
  for (let y = 0; y < 6; y++) for (let x = 0; x < 40; x++) if (Math.min(...channels(half.px[y][x])) > 190) bright.push(x);
  assert(bright.length >= 8 && bright.every((x) => Math.abs(x - 19.5) <= 2));
  stars(half, clear, NOON, 5);
  assertEqual(half.over.size, 0);
  const dark = new Half(40, 12);
  clear = heavens(dark, NIGHT, 14);
  dark.rect(0, 0, 10, 24, rgb(1, 2, 3));
  stars(dark, clear, NIGHT, 5, null, 14);
  assert(dark.over.size > 8 && [...dark.over.keys()].every((k) => k % 40 >= 10 && Math.floor(k / 40) < 7));
  const still = key([...dark.over]);
  dark.over.clear();
  stars(dark, clear, NIGHT, 5, null, 14);
  assertEqual(key([...dark.over]), still);
  const looks = new Set();
  for (let beat = 0; beat < 8; beat++) {
    dark.over.clear();
    stars(dark, clear, NIGHT, 5, beat * 0.7, 14);
    looks.add(key([...dark.over].sort((a, b) => a[0] - b[0])));
  }
  assert(looks.size > 4);
}

export function test_strip_moon_phases() {
  const lit = (when) => {
    const s = at(when);
    const half = new Half(40, 12);
    heavens(half, s, 20);
    const out = [];
    for (let y = 0; y < 24; y++) for (let x = 0; x < 40; x++) if (half.px[y][x] !== s.colour((x + 0.5) / 40, Math.min(1, (y + 0.5) / 20))) out.push(x);
    return out;
  };
  const full = Date.UTC(2026, 10, 24, 23, 30);
  assert(at(full).lit > 0.97);
  const disc = lit(full);
  const quarter = lit(Date.UTC(2026, 10, 17, 19, 0));
  assert(disc.length >= 12 && quarter.length > 0 && quarter.length < disc.length);
  const span = (xs) => Math.max(...xs) - Math.min(...xs);
  assert(span(quarter) < span(disc));
}

export function test_strip_weather() {
  const half = new Half(30, 8);
  heavens(half, NOON, 10);
  const before = half.px.map((r) => r.slice());
  clouds(half, NOON, 3, 10, 0.6, 0.0);
  const changed = [];
  half.px.forEach((row, y) => row.forEach((c, x) => c !== before[y][x] && changed.push(y)));
  assert(changed.length && changed.every((y) => y < 10));
  const moved = new Half(30, 8);
  heavens(moved, NOON, 10);
  clouds(moved, NOON, 3, 10, 0.6, 9.0);
  assert(key(moved.px) !== key(half.px));
  const cells = new Half(30, 8, rgb(40, 60, 90));
  clouds(cells, NOON, 3, 10, 0.6, 0.0, 2);
  assert(cells.px.some((row) => row.some((c) => c !== rgb(40, 60, 90))));
  for (let y = 0; y < 8; y++) assertEqual(key(cells.px[2 * y]), key(cells.px[2 * y + 1]));
  for (const fall of [rain, snow]) {
    const looks = [];
    for (const t of [0.0, 0.3]) {
      const sheet = new Half(30, 8, rgb(40, 60, 90));
      fall(sheet, NOON, 4, t);
      assert(sheet.over.size && [...sheet.over.values()].every(([g]) => g > "⠀" && g < "⤀"));
      looks.push(key([...sheet.over]));
    }
    assert(looks[0] !== looks[1]);
  }
  const misty = new Half(30, 8, rgb(0, 90, 0));
  fog(misty, NOON, 10);
  assert(misty.px.every((row) => row.every((c) => Math.min(...channels(c)) > 40)));
  assert(sum(misty.px[10][0]) > sum(misty.px[0][0]));
}

export function test_strip_water() {
  const half = new Half(30, 8);
  heavens(half, EVENING, 8);
  water(half, EVENING, 8, 3, 0.0);
  const far = (a, b) => channels(a).reduce((n, v, i) => n + Math.abs(v - channels(b)[i]), 0);
  assert(far(half.px[8][15], half.px[7][15]) < far(half.px[15][15], half.px[7][15]));
  const noon = new Half(30, 8);
  heavens(noon, NOON, 8);
  water(noon, NOON, 8, 3, 0.0);
  assert(noon.px.slice(9).every((row) => row.every((c) => Math.min(...channels(c)) >= 200 || channels(c)[2] > channels(c)[0])));
  const again = new Half(30, 8);
  assertEqual(key(heavens(again, NOON, 8)), key(heavens(new Half(30, 8), NOON, 8)));
  water(again, NOON, 8, 3, 0.0);
  assertEqual(key(again.px), key(noon.px));
  again.px[0][0] = rgb(1, 2, 3);
  assert(heavens(new Half(30, 8), NOON, 8)[0][0] !== rgb(1, 2, 3));
}

export function test_strip_pointer() {
  const now = [10.0];
  const pointer = new Pointer(() => now[0], 4.0);
  assert(pointer.at === null && pointer.ease(10.0) === 0 && pointer.trail.length === 0);
  pointer.leave();
  assertEqual(pointer.ease(99.0), 0);
  pointer.move(3, 1);
  assert(key(pointer.at) === "[3,1]" && pointer.since === 10.0);
  assert(pointer.ease(10.0) === 0 && pointer.ease(11) === 1 && Math.abs(pointer.ease(10.3) - 0.5) < 1e-9);
  now[0] = 11.0;
  pointer.move(3, 1);
  pointer.move(4, 1);
  assert(pointer.since === 10.0 && key(pointer.trail) === "[[3,1,11],[4,1,11]]");
  now[0] = 13.0;
  pointer.leave();
  assert(pointer.at === null && pointer.since === 13.0);
  assert(pointer.ease(13.0) === 1 && pointer.ease(15) === 0 && Math.abs(pointer.ease(13.6) - 0.5) < 1e-9);
  now[0] = 16.0;
  pointer.move(0, 0);
  assert(key(pointer.trail) === "[[0,0,16]]" && pointer.since === 16.0);
}

export function test_strip_ground() {
  const crops = Object.fromEntries(SEASONS.map((s) => [s, [rgb(200, 180, 90), rgb(90, 160, 40), rgb(140, 100, 70)]]));
  const tones = (layer, shift = 0.0, sky = NOON) => {
    const half = new Half(60, 10);
    paint(half, layer, sky, shift, "summer");
    return half.px.slice(8).map((r) => [...r]);
  };
  const flat = new Layer(new Strip(2, "flat"), 0.0, 4.0, 0.0);
  assertEqual(new Set(tones(flat).flat()).size, 1);
  for (const knob of [{ fields: new Fields(crops, 9.0, 2, 1.0, 0.5) }, { strata: [rgb(200, 110, 70), rgb(150, 80, 60), rgb(230, 150, 100)] }, { streak: 0.5 }]) {
    const layer = new Layer(new Strip(2, "flat"), 0.0, 4.0, 0.0, knob);
    const rows = tones(layer);
    assert(new Set(rows.flat()).size >= 2);
    const moved = tones(layer, 7.0);
    assertEqual(key(rows.map((r) => r.slice(7))), key(moved.map((r) => r.slice(0, -7))));
  }
  const fields = new Fields(crops, 10.0, 2, 1.0);
  assertEqual(Array.from({ length: 9 }, (_, d) => fields.at(0, 3.0, d)[0]), [0, 0, 1, 1, 1, 2, 2, 2, 2]);
  assertEqual(fields.at(0, 3.0, 0)[1], fields.at(0, 2.0, 1)[1]);
  assertEqual(Array.from({ length: 100 }, (_, x) => fields.at(0, x, 0)[2]).filter(Boolean).length, 10);
  const layer = new Layer(new Strip(2, "flat"), 0.0, 4.0, 0.0, { fields });
  const spread = [NOON, NIGHT].map((s) => {
    const gs = [...new Set(tones(layer, 0.0, s).flat())].map((c) => channels(c)[1]);
    return Math.max(...gs) - Math.min(...gs);
  });
  assert(spread[0] > 4 * Math.max(1, spread[1]));
}

export function test_strip_things() {
  const kinds = ["tree", "pine", "poplar", "house", "barn", "chalet", "church", "lighthouse", "sheep", "cow", "bush", "rock", "hay", "cactus", "palm", "pylon", "hut"];
  for (const kind of kinds) {
    for (const size of [2.0, 5.0, 20.0]) {
      const o = { gap: 24, density: 1.0, size };
      const empty = new Half(80, 22);
      const half = new Half(80, 22);
      paint(empty, new Layer(new Strip(2, "flat"), 0.0, 40.0, 0.0, o), NOON, 0.0, "summer");
      const marks = paint(half, new Layer(new Strip(2, "flat"), 0.0, 40.0, 0.0, { ...o, things: [kind] }), NOON, 0.0, "summer");
      assert(key(half.px) !== key(empty.px) && half.over.size === 0, kind);
      const lit = new Set(marks.map((m) => m.kind));
      assertEqual(lit.has("window"), ["house", "chalet", "church", "lighthouse"].includes(kind), kind);
      assertEqual(lit.has("chimney"), (kind === "house" || kind === "chalet") && size > 2, kind);
      for (const m of marks) if (m.kind === "window" && m.x >= 0 && m.x < 80) assertEqual(half.get(m.x, m.y), m.pane);
    }
  }
}

export function test_strip_pale_at_night() {
  const out = [SNOW, rgb(90, 160, 40)].map((colour) => {
    const half = new Half(4, 4);
    paint(half, new Layer(new Strip(2, "flat"), 0.0, 4.0, 0.0, { colour }), NIGHT, 0.0, "summer");
    return half.px[7][0];
  });
  assert(sum(out[0]) > 2.5 * sum(out[1]) && channels(out[0])[2] > channels(out[0])[0]);
  const dark = channels(NIGHT.shade(rgb(90, 160, 40)));
  assert(channels(out[1]).every((v, i) => Math.abs(v - dark[i]) <= 4));
}

export function test_strip_paint_remembers() {
  const fields = new Fields(Object.fromEntries(SEASONS.map((s) => [s, [rgb(200, 180, 60), rgb(90, 140, 50)]])));
  const layer = (o = {}) => new Layer(o.strip ?? new Strip(4, "hills"), o.depth ?? 0.0, 10, o.amp ?? 6, { streak: 0.3, snow: 0.6, fields, ...o });
  const others = [
    [layer(), NOON, "summer", 6],
    [layer({ strip: new Strip(5, "hills") }), NOON, "summer", 6],
    [layer({ strip: new Strip(4, "mountains") }), NOON, "summer", 6],
    [layer({ amp: 4 }), NOON, "summer", 6],
    [layer({ colour: rgb(120, 90, 60) }), NOON, "summer", 6],
    [layer({ fields: new Fields(fields.crops, 5.0) }), NOON, "summer", 6],
    [layer({ strata: [rgb(90, 60, 40), rgb(150, 110, 80)] }), NOON, "summer", 6],
    [layer({ streak: 0.0 }), NOON, "summer", 6],
    [layer({ depth: 0.5, speed: 1.0 }), NOON, "summer", 6],
    [layer(), EVENING, "summer", 6],
    [layer(), NOON, "winter", 6],
    [layer(), NOON, "summer", 4],
  ];
  const shifts = [0, 7, 14, 21, 28, 35];
  const draw = ([l, sky, season, h], shift) => {
    const half = new Half(30, h);
    paint(half, l, sky, shift, season);
    return key(half.px);
  };
  const want = others.map((o) => shifts.map((s) => (GROUNDS.map.clear(), draw(o, s))));
  GROUNDS.map.clear();
  for (let pass = 0; pass < 2; pass++) others.forEach((o, i) => shifts.forEach((s, j) => assertEqual(draw(o, s), want[i][j])));
  assertEqual(GROUNDS.map.size, others.length);
  for (let seed = 0; seed < GROUNDS.size + 5; seed++) paint(new Half(4, 2), new Layer(new Strip(seed), 0.0, 3, 2), NOON);
  assertEqual(GROUNDS.map.size, GROUNDS.size);
}
