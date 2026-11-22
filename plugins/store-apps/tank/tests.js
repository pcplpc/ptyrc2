// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of tank (`ptyrc dev test --unit`): test_tank.py of the
// twin (test_bounds, test_school, test_swims, test_water, test_decor,
// test_bubbles, test_finger); test_print and test_tank as cases. Draws
// from `seeded`.

import { assert, assertEqual } from "ptyrc";
import { TANKS, TICK, Tank, Tanks, options } from "./app.js";
import { CLASSIC, LEFT, RIGHT, seeded } from "./reef.js";

const MORE = { decorate: true, bubbles: true, species: true };
const range = (n) => Array.from({ length: n }, (_, i) => i);
const plain = (tank) => tank.rows().map((row) => row.map((run) => run.t).join(""));
const only = (text, glyphs) => [...text].every((c) => glyphs.includes(c));

function swum(tank, steps = 100) {
  for (let i = 0; i < steps; i++) tank.step(TICK);
  return tank;
}

export function test_options() {
  assertEqual(options({}), { count: 8, decorate: false, bubbles: false, species: false });
  assertEqual(options({ all: true, count: 3 }), { count: 3, decorate: true, bubbles: true, species: true });
  assertEqual(options({ random: true }).species, true);
}

export function test_bounds() {
  for (const more of [{}, MORE])
    for (const [w, h] of TANKS)
      for (const seed of range(4)) {
        const tank = new Tank(w, h, seeded(seed), more);
        const low = more.decorate ? h - 2 : h - 1;
        for (let i = 0; i < 150; i++) {
          tank.step(TICK);
          assertEqual(
            plain(tank).map((t) => [...t].length),
            Array(h).fill(w),
          );
          for (const fish of tank.school) {
            const [x, y, glyph] = fish.cell;
            assert(0 <= x && x <= w - glyph.length && 1 <= y && y <= low, JSON.stringify([w, h, x, y]));
          }
        }
      }
}

export function test_school() {
  // One fish in a bowl; as many as the water holds, `count` at most.
  const sizes = TANKS.map(([w, h]) => new Tank(w, h, seeded(1)).school.length);
  assertEqual(sizes[0], 1);
  assertEqual(sizes, [...sizes].sort((a, b) => a - b));
  assertEqual(sizes[sizes.length - 1], 8);
  assertEqual(new Tank(40, 10, seeded(1), { count: 3 }).school.length, 3);
  assert(new Tank(40, 10, seeded(1)).school.every((f) => f.kind === CLASSIC));
  const kinds = new Set();
  for (const seed of range(10)) for (const f of new Tank(40, 10, seeded(seed), { species: true }).school) kinds.add(f.kind.name);
  assert(kinds.size >= 5, [...kinds].join());
  assert(new Tank(9, 4, seeded(1), { species: true }).school.every((f) => f.length <= 6));
}

export function test_swims() {
  const tank = new Tank(28, 8, seeded(2));
  const before = JSON.stringify(tank.school.map((f) => f.cell));
  const looks = new Set(range(30).map(() => JSON.stringify(plain(swum(tank, 5)))));
  assert(looks.size > 20, `${looks.size}`);
  assert(JSON.stringify(tank.school.map((f) => f.cell)) !== before);
  // The same seed, the same tank.
  const a = swum(new Tank(28, 8, seeded(7), MORE));
  const b = swum(new Tank(28, 8, seeded(7), MORE));
  assertEqual(a.rows(), b.rows());
}

export function test_water() {
  const bare = plain(swum(new Tank(28, 8, seeded(3))));
  assert(only(bare[0], "~≈ ") && bare[0].trim() !== "", bare[0]);
  assert(only(bare.slice(1).join(""), LEFT + RIGHT + " "));
  // Too low for a surface.
  assertEqual(new Tank(12, 3, seeded(3)).top, 0);
}

export function test_decor() {
  const tank = new Tank(40, 10, seeded(4), { decorate: true });
  assert(tank.decor.length >= 2);
  assert(tank.decor.filter(([shape]) => shape.h >= 4).length <= 1);
  const spans = tank.decor.map(([shape, x]) => [x, x + shape.w]);
  for (let i = 1; i < spans.length; i++) assert(spans[i - 1][1] < spans[i][0], JSON.stringify(spans));
  assert(spans.every(([a, b]) => 0 <= a && b <= 40));
  const sand = plain(tank)[tank.h - 1];
  assert(only(sand, ".,:_ ") && [...sand].filter((c) => c === " ").length < 10, sand);
  assertEqual(new Tank(9, 4, seeded(4), { decorate: true }).decor.length, 1);
  // A decoration in front hides the fish behind it.
  const nth = tank.decor.findIndex(([shape]) => shape.frames.length === 1);
  assert(nth >= 0);
  const [shape, x] = tank.decor[nth];
  tank.decor[nth] = [...tank.decor[nth].slice(0, 3), true, 0];
  const feet = shape.frames.map((frame) => frame[frame.length - 1]);
  const dx = [...feet[0]].findIndex((c) => c !== " ");
  const fish = tank.school[0];
  [fish.x, fish.y, fish.vx] = [x + dx, tank.floor - 1, 0.0];
  assert(feet.map((row) => row[dx]).includes(plain(tank)[tank.floor - 1][x + dx]));
  tank.decor[nth] = [...tank.decor[nth].slice(0, 3), false, 0];
  assertEqual(plain(tank)[tank.floor - 1][x + dx], fish.cell[2][0]);
}

export function test_bubbles() {
  const tank = new Tank(40, 10, seeded(5), { bubbles: true });
  assertEqual(tank.vents.length, 2);
  let seen = 0;
  for (let i = 0; i < 300; i++) {
    tank.step(TICK);
    seen += tank.air.length;
    assert(tank.air.every((b) => tank.top - 0.5 <= b.y && b.y <= tank.floor));
  }
  assert(seen > 50, `${seen}`);
  assertEqual(swum(new Tank(40, 10, seeded(5))).air, []);
}

export function test_finger() {
  const tank = swum(new Tank(40, 10, seeded(6)), 20);
  const cells = () => JSON.stringify(tank.school.map((f) => f.cell));
  const far = () => tank.school.reduce((s, f) => s + Math.abs(f.x + f.length / 2 - 30) + 2 * Math.abs(f.y - 5), 0) / tank.school.length;
  const roaming = far();
  tank.onMove(30, 5);
  swum(tank, 80);
  assert(far() < roaming && far() < 8, `${far()}`); // they came
  const spots = tank.school.map((f) => `${f.cell[0]},${f.cell[1]}`);
  assertEqual(new Set(spots).size, spots.length); // each at its place
  for (const fish of tank.school) {
    // Nose to the pointer.
    const right = fish.cell[2] === fish.kind.glyph(true, fish.cell[0]);
    assertEqual(right, fish.x + fish.length / 2 < 30);
  }
  const still = cells();
  swum(tank, 10);
  assertEqual(cells(), still);
  tank.onLeave();
  swum(tank, 60);
  assert(cells() !== still); // and went
}

export function test_tanks() {
  // A variant not shown for a while was still: no leap; the finger is
  // the shown one's.
  const tanks = new Tanks({ count: 8 }, seeded(8), 0);
  const big = tanks.tanks.get("40x10");
  tanks.render(40, 10, 0);
  tanks.render(40, 10, 60000);
  assert(Math.abs(big.t - 3 * TICK) < 1e-9, `${big.t}`);
  tanks.come([40, 10], 3, 4);
  assertEqual(big.finger, [3, 4]);
  assertEqual(tanks.tanks.get("9x4").finger, null);
  tanks.come([9, 4], 1, 1);
  assertEqual(big.finger, null);
  tanks.go();
  assert([...tanks.tanks.values()].every((t) => t.finger === null));
}
