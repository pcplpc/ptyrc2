// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of fishes (`ptyrc dev test --unit`): test_apps.py of
// the twin (test_fishes_tide, _entry, _shy; test_fishes_hide_behind_text
// and _come_back_after_a_scroll on an `Aquarium` of their own), and of
// test_reef.py what is the app's (test_species_swim,
// test_lure_takes_spots_looked_for_once, test_menace_and_depart).
// reef.js has its own, in _shared/js. Draws from `seeded`.

import { assert, assertEqual } from "ptyrc";
import { Aquarium, BEAT, STAY, WAIT, cover, depart, entry, hideouts, inside, lure, menace, options, tide } from "./app.js";
import { CALM, FEAR, Fish, NEAR_MOVING, SPECIES, reach, seeded, swim } from "./reef.js";

const range = (a, b) => Array.from({ length: b - a }, (_, i) => a + i);
const key = (p) => (p === null ? null : `${p[0]},${p[1]}`);

/** Stands for a `Batch`. */
class Canvas {
  constructor() {
    this.drawn = [];
  }
  put(spec) {
    const handle = { id: this.drawn.length + 1, spec: JSON.parse(JSON.stringify(spec)), gone: false };
    this.drawn.push(handle);
    return handle;
  }
  update(handle, members) {
    Object.assign(handle.spec, JSON.parse(JSON.stringify(members)));
  }
  remove(handle) {
    handle.gone = true;
  }
}

export function test_options() {
  assertEqual(options({}), { count: 8, shy: 4, decorate: false, bubbles: false, random: false });
  assertEqual(options({ count: 5, all: true }), { count: 5, shy: 2, decorate: true, bubbles: true, random: true });
  assertEqual(options({ shy: 0, bubbles: true }).shy, 0);
  // A fish that flees goes a cell a tick.
  assert(FEAR * BEAT <= 1.0);
}

export function test_species_swim() {
  const rnd = seeded(4);
  const box = [0, 0, 80, 24];
  assert(inside([71, 3], box, 9) && !inside([72, 3], box, 9));
  assertEqual(tide([[76, 3], [76, 3]], [null, null], box, 0.0, rnd, [3, 9])[0][0], null);
  const sword = SPECIES.find((k) => k.name === "sword");
  const fish = new Fish({ x: 81.0, y: 3.0, vx: 5.0, color: 6, kind: sword, shy: true });
  fish.leaving = true;
  assertEqual(lure(fish, [[1, 1]], null, 0.1, rnd), null);
}

export function test_lure_takes_spots_looked_for_once() {
  // The app keeps the spots the cursor does not reach, and hands them
  // with no cursor: the same fish, the same way.
  const spots = range(0, 8).flatMap((j) => range(0, 14).map((i) => [5 * i, 3 * j]));
  const cursor = [30, 12];
  const safe = spots.filter((s) => reach(s[0], s[1], cursor) >= NEAR_MOVING);
  assert(0 < safe.length && safe.length < spots.length);
  const one = new Fish({ x: 31.0, y: 12.0, vx: 3.0, color: 6, shy: true });
  const two = new Fish({ x: 31.0, y: 12.0, vx: 3.0, color: 6, shy: true });
  const [rnd1, rnd2] = [seeded(7), seeded(7)];
  const box = [0, 0, 80, 24];
  const homes = new Set();
  for (let i = 0; i < 600; i++) {
    const a = lure(one, spots, cursor, 0.1, rnd1);
    const b = lure(two, safe, null, 0.1, rnd2);
    assert(key(a) === key(b) && safe.some((s) => key(s) === key(a)));
    homes.add(key(a));
    swim(one, 0.1, box, null, false, rnd1);
    swim(two, 0.1, box, null, false, rnd2);
    assertEqual([one.x, one.y, one.stay], [two.x, two.y, two.stay]);
  }
  assert(homes.size > 1); // it stayed, and went to another
}

export function test_menace_and_depart() {
  const rnd = seeded(5);
  const box = [0, 0, 80, 24];
  const fish = new Fish({ x: 30.0, y: 10.0, vx: CALM, color: 6 });
  assertEqual(menace(fish, [60, 10], true, []), [[60, 10], true]);
  assertEqual(menace(fish, [60, 10], true, [[50.0, 10.0]]), [[60, 10], true]);
  assertEqual(
    menace(fish, null, false, [
      [50.0, 10.0],
      [38.0, 10.0],
    ]),
    [[38.0, 10.0], false],
  );
  swim(fish, 0.1, box, ...menace(fish, null, false, [[38.0, 10.0]]), rnd);
  assertEqual(fish.vx, -FEAR);
  // A bold, calm fish of the view leaves by the nearest side.
  const shy = new Fish({ x: 10.0, y: 1.0, vx: 1.0, color: 6, shy: true });
  const out = new Fish({ x: 90.0, y: 1.0, vx: 1.0, color: 6 });
  const bold = new Fish({ x: 60.0, y: 1.0, vx: -1.0, color: 6 });
  assertEqual(depart([shy, out, fish], box, rnd), null); // `fish` is scared
  assert(depart([shy, out, bold], box, rnd) === bold);
  assert(bold.leaving && bold.vx === CALM);
}

export function test_fishes_tide() {
  const rnd = seeded(1);
  const box = [0, 100, 80, 123]; // x in [0, 74], y in [100, 123]
  // Three in view, three left out by a scroll.
  const cells = [
    [0, 100],
    [74, 123],
    [40, 110],
    [10, 99],
    [75, 110],
    [40, 0],
  ];
  let [due, going] = tide(cells, new Array(6).fill(null), box, 0.0, rnd);
  assert(going === null);
  assertEqual(due.slice(0, 3), [null, null, null]);
  assert(due.slice(3).every((d) => WAIT[0] <= d && d <= WAIT[1]));
  // They go one a call, in the order of their times, and come back.
  const order = [3, 4, 5].sort((a, b) => due[a] - due[b]);
  const gone = [];
  for (let t = 0; t < 60; t++) {
    [due, going] = tide(cells, due, box, t * 0.1, rnd);
    if (going !== null) {
      gone.push(going);
      cells[going] = [20, 110];
    }
  }
  assertEqual(gone, order);
  assertEqual(due, new Array(6).fill(null));
  // The same time for all: still one a call.
  [due, going] = tide([[0, 0], [0, 0], [0, 0]], [1.0, 1.0, 1.0], box, 2.0, rnd);
  assert(due.filter((d) => d === null).length === 1 && due[going] === null);
  // Back in view before its time: it carries on.
  [due] = tide([[10, 99]], [null], box, 0.0, rnd);
  assertEqual(tide([[10, 100]], due, box, 9.0, rnd), [[null], null]);
}

export function test_fishes_entry() {
  const rnd = seeded(2);
  const box = [10, 100, 30, 102];
  // The only spot where the text hides a whole fish.
  const rows = ["  ab cd", "  ab cdefgh  ab", "x y z"];
  assertEqual([...new Set(range(0, 20).map(() => key(entry(rows, box, rnd))))], ["15,101"]);
  // No text: a side of the view.
  const spots = range(0, 50).map(() => entry(["", "", ""], box, rnd));
  assertEqual([...new Set(spots.map(([x]) => x))].sort(), [10, 24]);
  assert(spots.every(([, y]) => 100 <= y && y <= 102));
}

export function test_fishes_shy() {
  const rnd = seeded(3);
  const box = [0, 100, 80, 123];
  // A block of text, rows 105 to 109, columns 40 to 69; the rest empty.
  const rows = range(0, 24).map((y) => (5 <= y && y < 10 ? " ".repeat(40) + "x".repeat(30) : ""));
  const spots = hideouts(rows, box);
  assertEqual(spots.map(key).sort(), range(39, 66).flatMap((x) => range(105, 110).map((y) => `${x},${y}`)).sort());
  const school = (shy) =>
    [
      [20.0, 102.0],
      [74.0, 123.0],
      [0.0, 100.0],
    ].map(([x, y]) => new Fish({ x, y, vx: CALM, color: 6, shy }));
  const tick = (fish, cursor = null) => {
    lure(fish, spots, cursor, 0.1, rnd);
    swim(fish, 0.1, box, cursor, false, rnd);
  };
  const hidden = new Set(spots.map(key));
  // Shy: over the text within 8 s, and still there after; it moves on.
  for (const fish of school(true)) {
    const homes = new Set();
    for (let t = 0; t < 400; t++) {
      tick(fish);
      homes.add(key(fish.home));
      if (t >= 80) assert(hidden.has(key(fish.cell)), `${t}: ${fish.cell}`);
    }
    assert(homes.size > 2);
  }
  // Bold: no home, never slows down.
  for (const fish of school(false))
    for (let t = 0; t < 400; t++) {
      tick(fish);
      assert(fish.home === null && Math.abs(fish.vx) >= CALM / 2);
    }
  // Scared: it flees first, even with a safe home ahead.
  let fish = new Fish({ x: 30.0, y: 105.0, vx: CALM, color: 6, shy: true });
  tick(fish, [38, 105]);
  assert(hidden.has(key(fish.home)) && reach(...fish.home, [38, 105]) >= NEAR_MOVING);
  assertEqual(fish.vx, -FEAR);
  // A home in the cursor's reach is given up.
  fish = new Fish({ x: 40.0, y: 105.0, vx: 0.0, color: 6, shy: true });
  fish.home = [40, 105];
  assert(key(lure(fish, spots, [43, 106], 0.1, rnd)) !== "40,105");
  assertEqual(lure(fish, [[40, 105]], [43, 106], 0.1, rnd), null);
  assert(STAY[0] > 0);
}

/** An aquarium of `o` born on `box`, and what it draws. */
function aquarium(o, rnd, box) {
  const out = new Canvas();
  return [new Aquarium(o, { rnd, out }, box, 0), out];
}

export function test_fishes_hide_behind_text() {
  // Text on the right of rows 0 to 7; the rest of the view empty.
  const rows = range(0, 24).map((y) => (y < 8 ? " ".repeat(40) + "o".repeat(39) : ""));
  const box = [0, 0, 80, 23];
  const [tank, out] = aquarium(options({}), seeded(6), box);
  for (let i = 0; i < 300; i++) tank.step({ frame: 1, box, rows, changed: i === 0 }, null, i * BEAT, BEAT);
  const hid = new Map(cover(rows, box).map(([x, y, n]) => [`${x},${y}`, n]));
  const shy = tank.school.filter((f) => f.shy);
  const hidden = shy.filter((f) => (hid.get(key(f.shown)) ?? 0) >= 5);
  assertEqual([tank.school.length, shy.length], [8, 4]);
  assert(hidden.length >= 3, shy.map((f) => `${f.shown}`));
  assertEqual(out.drawn.length, 8);
  assert(out.drawn.every((d) => d.spec.layer === "back" && d.spec.anchor.space === "frame"));
}

export function test_fishes_come_back_after_a_scroll() {
  const text = range(0, 300).map((i) => `line ${i}: the quick brown fox`);
  const [tank, out] = aquarium(options({}), seeded(5), [0, 0, 80, 22]);
  const look = (y0, changed) => ({ frame: 1, box: [0, y0, 80, y0 + 22], rows: text.slice(y0, y0 + 23), changed });
  const inView = (y0) => tank.school.filter((f) => inside(f.shown, [0, y0, 80, y0 + 22], f.length)).length;
  let t = 0;
  const run = (y0, seconds, each = () => {}) => {
    for (let i = 0; i < seconds / BEAT; i++, t += BEAT) {
      tank.step(look(y0, i === 0), null, t, BEAT);
      each();
    }
  };
  run(0, 2);
  assertEqual(inView(0), 8);
  run(22, 0.3); // 22 rows down: most are left above
  // Left behind, not removed: they go later, one by one.
  assert(out.drawn.length === 8 && !out.drawn.some((d) => d.gone) && inView(22) < 8);
  const counts = [];
  run(22, 9, () => counts.push(inView(22)));
  assert(counts.includes(8) && new Set(counts.slice(0, counts.indexOf(8))).size > 2, [...new Set(counts)]);
  assertEqual(out.drawn.length, 8);
}
