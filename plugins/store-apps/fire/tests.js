// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of fire, `ptyrc dev test --unit` (test_fire.py but
// test_print and test_fire: cases). `TWIN`: scenes of the twin at seed
// 1, their text and a sum of their colours (fg + 3 bg a cell).

import { assert, assertEqual } from "ptyrc";
import { COALS, FULL, GLYPHS, LIT, POKED, Poke, SIZES, fuel, heat, look, options, scene, sparks } from "./app.js";

const FLAGS = ["blocks", "logs", "cycle", "sparks", "glow"];
const ALL = look(Object.fromEntries(FLAGS.map((k) => [k, true])));
const plain = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
/** Cells drawn; of `glyphs` only when given. */
const ink = (rows, glyphs = null) =>
  [...plain(rows).join("")].filter((c) => c !== " " && (glyphs === null || glyphs.includes(c))).length;
const colours = (rows) => rows.flat().reduce((n, r) => n + ((r.fg ?? 0) + 3 * (r.bg ?? 0)) * [...r.t].length, 0);

export const TWIN = [
  [{}, 3.0, 15, 7, 5977, [
    "               ",
    "               ",
    "               ",
    "               ",
    "    .:....     ",
    "  .:*^^:^^^^:  ",
    " ^*x#x**^x#xx* ",
  ]],
  [{}, 3.0, 5, 4, 1933, [
    "  .  ",
    " :#  ",
    " ##x ",
    " ### ",
  ]],
  [{"blocks": true}, 3.0, 23, 9, 50363, [
    "                       ",
    "                       ",
    "                       ",
    "                       ",
    "                       ",
    "     ▄  ▄▄▀▀▀▀▄▄▀▄     ",
    "    ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀    ",
    "  ▄▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▄ ",
    " ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ ",
  ]],
  [{"logs": true}, 1.0, 33, 11, 23948, [
    "                                 ",
    "                                 ",
    "                                 ",
    "                                 ",
    "                ::               ",
    "            .. .^^.  :..         ",
    "       .   .::::^*^.:^^:.        ",
    "     ...:..^^:^^***:***^*.       ",
    "    .:^**:^**xxxxxx**###x^.      ",
    " ╥ .*xx##x(-===--==-(@)x###x^  ╥ ",
    " ╨ (@):=#=**===)(@):==-=#=~==) ╨ ",
  ]],
  [{"logs": true}, 1.0, 45, 13, 36849, [
    "                                             ",
    "                                             ",
    "                                             ",
    "                                             ",
    "                                             ",
    "                ..   ...                     ",
    "               .:^   ::.:      .             ",
    "            .. .^^: :^:^^     ::.            ",
    "           .::::^x*:^**^^:    :^^:...        ",
    "       .. .:^^**xx*^*xx*x*::::*x*::^::       ",
    "     .^**:^**x#xx##x*####x^^^*x***x**^:      ",
    " ╥  :*x##(-===--==-(@)(--*#~-===(@)*xx*^.  ╥ ",
    " ╨ (@):=#=**===)(@):==-=#=~=)(@)-*#*:-===) ╨ ",
  ]],
  [{"logs": true, "blocks": true}, 1.0, 33, 11, 81029, [
    "                                 ",
    "                                 ",
    "                                 ",
    "                ▀▀               ",
    "            ▄▄ ▄▀▀   ▀▀          ",
    "           ▄▀▀▄▀▀▀▀ ▄▀▀▀▄        ",
    "       ▄▄  ▀▀▀▀▀▀▀▀▄▀▀▀▀▀        ",
    "     ▄▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀       ",
    "    ▄▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▄▄     ",
    " ╥ ▄▀▀▐▒▓▓▓▒▒▓▓▒▒▒▓▒▒▒▒▌▀▀▀▀▀▄▄╥ ",
    " ╨ ▐▒▒▒▓▒▓▒▒▓▓▓▓▓▒▒▒▓▓▒▓▒▓▒▓▓▌ ╨ ",
  ]],
  [{"logs": true, "blocks": true}, 1.0, 9, 5, 10837, [
    "         ",
    "    ▄▄▄  ",
    "▄▄▀▀▀▀▀▀▄",
    "◥███▀▀▀█◤",
    " /     \\ ",
  ]],
  [{"logs": true, "blocks": true}, 1.0, 5, 4, 4079, [
    "  ▄  ",
    " ▀▀▀ ",
    " ▐█▌ ",
    " ▀▀▀ ",
  ]],
  [{"logs": true, "glow": true}, 1.0, 33, 11, 76110, [
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴::┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──..─.^^.──:..┴──┬──┴──",
    "┬──┴──┬.─┴─.::::^*^.:^^:.──┴──┬──",
    "┴──┬─...:..^^:^^***:***^*.─┬──┴──",
    "┬──┴.:^**:^**xxxxxx**###x^.┴──┬──",
    "┴╥─.*xx##x(-===--==-(@)x###x^─┴╥─",
    " ╨ (@):=#=**===)(@):==-=#=~==) ╨ ",
  ]],
  [{"glow": true}, 2.0, 23, 9, 16722, [
    "                       ",
    "                       ",
    "                       ",
    "        ░░░░░░.        ",
    "       .....░.:::      ",
    "      ░^^:^^:::^^^     ",
    "  .: ░^^^^^^*^**^^:    ",
    " .^*^^******xx*****^:. ",
    " xxxxx#x###x##*x##xx#x ",
  ]],
  [{"logs": true, "cycle": true, "burn": 100.0, "blocks": true, "glow": true}, 95.0, 33, 11, 87665, [
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──",
    "┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──",
    "┴╥─┬▄▄▐▒▓▒▒▒▒▒▒▒▒▒▒▓▒▒▓▌▄▄▄▄▄▄▄╥─",
    " ╨ ▐▒▒▒▓▒▓▒▒▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▓▌ ╨ ",
  ]],
];

export function test_twin() {
  for (const [kw, t, w, h, sum, text] of TWIN) {
    const rows = scene(t, w, h, look(kw));
    assertEqual(plain(rows), text, `${JSON.stringify(kw)} ${w}x${h}`);
    assertEqual(colours(rows), sum, `${JSON.stringify(kw)} ${w}x${h}`);
  }
}

export function test_bounds() {
  const looks = [look(), ...FLAGS.map((k) => look({ [k]: true })), ALL];
  for (const lk of looks) {
    for (const [w, h] of SIZES) {
      for (const t of [0, 1.3, 200, 3500]) {
        const rows = scene(t, w, h, lk, 0.5);
        assertEqual(rows.length, h);
        for (const text of plain(rows)) assertEqual([...text].length, w, `${w}x${h} ${text}`);
        for (const r of rows.flat()) {
          assert(Object.keys(r).every((k) => ["t", "fg", "bg", "bold", "dim"].includes(k)), JSON.stringify(r));
          assert(["fg", "bg"].every((k) => Number.isInteger(r[k] ?? 0)));
        }
      }
    }
  }
}

export function test_pure() {
  for (const lk of [look(), ALL]) {
    assertEqual(scene(7.3, 23, 9, lk), scene(7.3, 23, 9, lk));
    assert(JSON.stringify(scene(7.3, 23, 9, lk)) !== JSON.stringify(scene(7.4, 23, 9, lk)));
  }
  assert(JSON.stringify(scene(7.3, 23, 9)) !== JSON.stringify(scene(7.3, 23, 9, look({ seed: 2 }))));
}

export function test_bare_is_glyphs() {
  for (const [w, h] of SIZES) {
    const rows = scene(3, w, h);
    assert([...plain(rows).join("")].every((c) => GLYPHS.includes(c)));
    assert(ink(rows) > 0);
    assert(rows.flat().every((r) => !("bg" in r)));
    // It stands on the bottom of the box and thins upwards.
    assert(ink(rows.slice(-1)) >= ink(rows.slice(0, 1)));
  }
}

export function test_blocks() {
  const rows = scene(3, 23, 9, look({ blocks: true }));
  assert([...plain(rows).join("")].every((c) => " ▀▄".includes(c)));
  assert(rows.flat().some((r) => "bg" in r));
}

export function test_heat() {
  // None beside the bed or over the flames; the most at the foot.
  assertEqual(heat(-1, 0.5, 1, 23, 18, 1, 1), 0);
  assertEqual(heat(11, 40, 1, 23, 18, 1, 1), 0);
  let foot = 0;
  let high = 0;
  for (let t = 0; t < 50; t++) {
    foot += heat(11, 0.5, t / 7, 23, 18, 1, 1);
    high += heat(11, 9, t / 7, 23, 18, 1, 1);
  }
  assert(foot > high && high > 0);
  for (let t = 0; t < 50; t++)
    for (let x = 0; x < 23; x++)
      for (let y = 0; y < 18; y++) {
        const v = heat(x, y + 0.5, t / 7, 23, 18, 1, 1);
        assert(v >= 0 && v <= 1);
      }
}

export function test_fuel() {
  assertEqual(fuel(123, look()), FULL);
  const lk = look({ cycle: true, burn: 100 });
  const levels = [];
  for (let t = 2; t < 100; t++) levels.push(fuel(t, lk));
  assertEqual(levels, [...levels].sort((a, b) => b - a)); // it burns down
  assert(levels[0] === 1 && levels[levels.length - 1] < 0.1);
  assertEqual(fuel(102, lk), fuel(2, lk)); // a log is put on
  for (let t = 0; t < 2000; t++) assert(fuel(t / 10, lk) > 0 && fuel(t / 10, lk) <= 1);
  const flames = (t0) => {
    let n = 0;
    for (let i = 0; i < 30; i++) n += ink(scene(t0 + i / 3, 33, 11, lk));
    return n;
  };
  assert(flames(5) > 3 * flames(92));
}

export function test_logs() {
  let lk = look({ logs: true, blocks: true });
  const [candle, brazier, hearth] = [
    [5, 4],
    [9, 5],
    [33, 11],
  ].map(([w, h]) => plain(scene(1, w, h, lk)));
  assertEqual(candle.slice(-2), [" ▐█▌ ", " ▀▀▀ "]);
  assert(brazier.at(-2)[0] === "◥" && brazier.at(-2).at(-1) === "◤");
  const n = (row, s) => [...row].filter((c) => s.includes(c)).length;
  assert(n(hearth.at(-1), "╨") === 2 && n(hearth.at(-2), "╥") === 2);
  assert(n(hearth.at(-1), "▓▒") > n(hearth.at(-2), "▓▒") && n(hearth.at(-2), "▓▒") > 4);
  // Without the blocks: glyphs, no background.
  for (const [w, h] of SIZES) {
    const rows = scene(1, w, h, look({ logs: true, sparks: true }));
    assert(![...plain(rows).join("")].some((c) => "█▐▌▀▄▓▒◥◤".includes(c)));
    assert(rows.flat().every((r) => !("bg" in r)));
  }
  lk = look({ logs: true });
  const [c2, b2, small, h2, wide] = [
    [5, 4],
    [9, 5],
    [23, 9],
    [33, 11],
    [45, 13],
  ].map(([w, h]) => plain(scene(1, w, h, lk)));
  assertEqual(c2.slice(-2), [" |_| ", " === "]);
  assert(b2.at(-2)[0] === "\\" && b2.at(-2).at(-1) === "/");
  assert(n(h2.at(-1), "╨") === 2 && n(h2.at(-2), "╥") === 2);
  // A pile: a log each dozen cells, one fewer across them.
  const ends = [small, h2, wide].map((rows) => rows.slice(-2).map((r) => r.split("(@)").length - 1));
  assertEqual(ends, [
    [1, 1],
    [1, 2],
    [2, 3],
  ]);
  assert(h2.at(-1).slice(3, 6) === "(@)" && h2.at(-1).at(-4) === ")");
  // With the light, bricks behind the logs, brighter near the flames.
  const lit = scene(1, 33, 11, look({ logs: true, glow: true }));
  assert(ink(lit, "┬┴─") > 100);
  const shades = new Set(lit.flat().filter((r) => [...r.t].every((c) => "┬┴─".includes(c))).map((r) => r.fg));
  assert(shades.size >= 3 && [...shades].every((s) => LIT.includes(s)));
}

export function test_embers() {
  // Burnt down: no flame, the logs glow more.
  for (const blocks of [true, false]) {
    const lk = look({ logs: true, cycle: true, burn: 100, blocks });
    const coals = (t) => {
      const rows = scene(t, 33, 11, lk);
      if (blocks) return rows.slice(-2).flat().filter((r) => "bg" in r).length;
      return ink(rows.slice(-2), COALS);
    };
    let dead = 0;
    let alive = 0;
    for (let i = 0; i < 10; i++) {
      dead += coals(95 + i / 5);
      alive += coals(5 + i / 5);
    }
    assert(dead > 0);
    if (!blocks) assert(dead > alive); // with them, a log in the flames has a background too
    assert(ink(scene(95, 33, 11, lk).slice(0, -2)) < 12);
  }
}

export function test_sparks() {
  const count = (poke) => {
    let n = 0;
    for (let t = 0; t < 300; t++) n += sparks(t / 10, 33, 9, FULL, 1, poke).size;
    return n;
  };
  assert(count(null) > 30);
  assert(count(0.6) > 2 * count(null)); // a poke throws a burst
  for (let t = 0; t < 300; t++)
    for (const key of sparks(t / 10, 33, 9, FULL, 1, 0.6).keys()) {
      const [x, y] = key.split(",").map(Number);
      assert(x >= 0 && x < 33 && y >= 0 && y < 9);
    }
  assertEqual(sparks(1, 2, 2, 1, 1).size, 0);
  let seen = 0;
  for (let t = 0; t < 100; t++) seen += ink(scene(t / 10, 33, 11, look({ sparks: true })), "*·");
  assert(seen > 0);
}

export function test_poke() {
  const poke = new Poke(4000);
  assertEqual(poke.age(0), null);
  poke.move(10000);
  assertEqual(poke.age(10500), 0.5);
  poke.move(11000); // still over it: the same poke
  assertEqual(poke.age(11500), 1.5);
  poke.move(14500); // and again, later
  assertEqual(poke.age(15000), 0.5);
  poke.leave();
  poke.move(15500); // back over it
  assertEqual(poke.age(15500), 0);
  assertEqual(poke.age(15500 + POKED * 1000), null);
  // The flames rise.
  let calm = 0;
  let poked = 0;
  for (let t = 0; t < 40; t++) {
    calm += ink(scene(t / 5, 33, 11));
    poked += ink(scene(t / 5, 33, 11, look(), 0.3));
  }
  assert(poked > calm);
}

export function test_options() {
  assertEqual(options({ burn: 3600 }), { blocks: false, logs: false, cycle: false, sparks: false, glow: false, burn: 3600 });
  const all = options({ all: true, burn: 60 });
  assert(FLAGS.every((k) => all[k]) && all.burn === 60);
}
