// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of globe, `ptyrc dev test --unit` (test_globe.py but
// test_print and test_globe: cases). `TWIN`: scenes of the twin, their
// text, a sum of their colours (fg a cell) and their bold cells;
// `DRAWS`: Python's `random.Random(n)`, its first draws. Both written by
// plan/sessions/22112026-01-migration/wave1-globe/twin.py.

import { assert, assertEqual } from "ptyrc";
import { DOMES, FLAKES, GARLAND, GLINT, INK, PERIOD, BRISK, SCENES, SIZES, SNOW, Shaker, Twister, auto, look, options, scene, swirls, which } from "./app.js";

const FLAGS = ["shake", "drift", "lights", "random"];
const ALL = look(Object.fromEntries(FLAGS.map((k) => [k, true])));
const LOOKS = [look(), ALL, ...FLAGS.map((k) => look({ [k]: true }))];
const plain = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const len = (s) => [...s].length;
/** Flakes in the air. */
const snow = (rows) =>
  rows.flat().reduce((n, r) => n + (r.fg === SNOW && [...r.t].every((c) => FLAKES.includes(c)) ? len(r.t) : 0), 0);
const dome = (w, h) => DOMES.find((d) => d.size[0] === w && d.size[1] === h);

export const TWIN = [
  [{}, 0.0, 11, 5, null, 0, 3798, 2, ["  .-'''-.  ", " /       \\ ", "|  /\\  ♣  |", " \\/[]\\_♣_/ ", " /_______\\ "]],
  [{}, 3.7, 17, 8, null, 0, 9365, 5, ["    .-'''''-.    ", "  .'         '.  ", " /     /\\    . \\ ", "|  ♣. /  \\  ♣  .|", " \\♣♣♣/_[]_\\♣♣♣ / ", "  '._________.'  ", "  /___________\\  ", " '-------------' "]],
  [{}, 12.0, 21, 10, null, 0, 12457, 9, ["    _.-'''''''-._    ", "  .'            *'.  ", " /              ·  \\ ", "|    .              |", "|        /\\  ·   .  |", "|   ♣·  /  \\   ♣  . |", " \\ ♣♣♣ /_[]_\\ ♣♣♣  / ", "  '._____________.'  ", "  /_______________\\  ", " '-----------------' "]],
  [{}, 7.3, 30, 13, null, 0, 17229, 17, ["        _..-'''''''-.._       ", "     .-' .   .         '-.    ", "   .' .                   '.  ", "  /        *           ·    \\ ", " |       ·         ·         |", " |  .           ·            |", " |  *           .         .  |", " |      ·     /\\          .  |", "  \\      ♣  ·/  \\   ♣       / ", "   '.   ♣♣♣ /_[]_\\ ♣♣♣    .'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
  [{}, 101.0, 30, 13, 100.0, 1, 22074, 36, ["        _..-'''''''-.._       ", "     .-'       · .  .  '-.    ", "   .'   ·.      .         '.  ", "  /*·   .     ·    . .·     \\ ", " | *.    .          .  .     |", " |         . · .  ·       .· |", " |                .          |", " | ·.         /\\   .        ·|", "  \\ ·    ♣ · / *\\   ♣     · / ", "   '.   ♣♣♣ /_[]_\\*♣♣♣    .'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
  [{}, 100.4, 21, 10, 100.0, 1, 15007, 19, ["    _.-'''''''-._    ", "  .'.       .·   '.  ", " /       .        .\\ ", "|·        .  .      |", "|     .  /\\.  ·..   |", "| * ♣·  / *\\   ♣ *  |", " \\ ♣♣♣ /_[]_\\ ♣♣♣  / ", "  '._____________.'  ", "  /_______________\\  ", " '-----------------' "]],
  [{"drift": true}, 5.0, 21, 10, null, 0, 14400, 15, ["    _.-'''''''-._    ", "  .'  *          '.  ", " /              ·  \\ ", "|   .        .      |", "|        /\\       · |", "|   ♣ * /  \\   ♣   ·|", " \\▂♣♣♣▁/_[]_\\▂♣♣♣▂▂/ ", "  '._____________.'  ", "  /_______________\\  ", " '-----------------' "]],
  [{"drift": true}, 64.0, 30, 13, 50.0, 1, 20334, 23, ["        _..-'''''''-.._       ", "     .-' .  *          '-.    ", "   .'         * ·        ·'.  ", "  /                         \\ ", " |  *.  *                 *  |", " |               ·       .·  |", " |     .   *   ·             |", " |            /\\   ·         |", "  \\      ♣   /  \\   ♣.      / ", "   '.▁.▁♣♣♣▁/_[]_\\▁♣♣♣.▁▁▁.'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
  [{"lights": true}, 1.0, 21, 10, null, 0, 12728, 12, ["    _.-'''''''-._    ", "  .'   ·     .   '.  ", " /               . \\ ", "|   .               |", "|   *    /\\         |", "|   ♣ * /  \\   ♣    |", " \\ ♣♣♣ /_[]_\\ ♣♣•  / ", "  '._____________.'  ", "  /_______________\\  ", " '-----------------' "]],
  [{"lights": true, "scene": "fir"}, 2.2, 30, 13, null, 0, 16670, 24, ["        _..-'''''''-.._       ", "     .-'.             .'-.    ", "   .'    .    .      .   ·'.  ", "  /        *             *  \\ ", " |        ·                  |", " |    ·                      |", " |  .          *             |", " |            /o\\            |", "  \\      .♣  /.o.\\  ♣       / ", "   '. .  ♣•♣/o.o.o\\♣♣♣  . .'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
  [{"scene": "church"}, 9.0, 17, 8, null, 0, 9469, 4, ["    .-'''''-.    ", "  .'         '.  ", " /    †      . \\ ", "|  ♣ /\\___  ♣   |", " \\ ♣♣|[]|_|♣♣♣·/ ", "  '._________.'  ", "  /___________\\  ", " '-------------' "]],
  [{"scene": "snowman"}, 4.0, 11, 5, null, 0, 4256, 4, ["  .-'''-.  ", " /       \\ ", "|  (\") ♣  |", " \\(_:_)♣_/ ", " /_______\\ "]],
  [{"shake": true, "drift": true, "lights": true, "random": true}, 9.0, 21, 10, null, 0, 17073, 28, ["    _.-'''''''-._    ", "  .'  *          '.  ", " /    .           .\\ ", "|      †            |", "| ·   /\\_____ ·     |", "|  ♣ /  \\_____\\ ♣   |", " \\▂♣♣|[]|_[]_|♣♣•▂▂/ ", "  '._____________.'  ", "  /_______________\\  ", " '-----------------' "]],
  [{"shake": true, "drift": true, "lights": true, "random": true}, 80.0, 30, 13, null, 0, 19735, 25, ["        _..-'''''''-.._       ", "     .-'     .     ·   '-.    ", "   .'       .  .          '.  ", "  /   ·                   · \\ ", " |              ·            |", " |  .      ·       *         |", " |             *      .      |", " |      *     /o\\            |", "  \\      ·♣· /.o.\\  ♣    *  / ", "   '.....♣•♣/o.o.o\\♣♣♣.....'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
  [{"shake": true, "drift": true, "lights": true, "random": true}, 200.5, 17, 8, 199.0, 2, 10279, 10, ["    .-'''''-.    ", "  .'  *.     '.  ", " /  .  /\\      \\ ", "|  ♣  /  \\  ♣ · |", " \\••♣/_[]_\\♣♣•./ ", "  '._________.'  ", "  /___________\\  ", " '-------------' "]],
  [{"shake": true, "drift": true, "lights": true, "random": true, "every": 10.0, "seed": 1234567}, 31.0, 30, 13, null, 0, 20097, 23, ["        _..-'''''''-.._       ", "     .-'  ·            '-.    ", "   .'           .  ·* .   '.  ", "  /   .      ·   .       ·  \\ ", " |   ·              *        |", " |           ·         ..    |", " |                   ..    · |", " |            /\\             |", "  \\ .    ♣   /  \\   ♣ .     / ", "   '.  .♣♣♣ /_[]_\\.•♣•.....'  ", "     '-._______________.-'    ", "     /___________________\\    ", "    '---------------------'   "]],
];

export const DRAWS = [[0, 0.8444218515250481, 7.789772014701512], [1, 0.13436424411240122, -8.237168684686164], [7919, 0.6392620987771694, 8.78653997864116], [7919000044, 0.7305419976356579, -8.522658134297988], [1099511627781, 0.5043802970418443, -5.343022199861641]];

export function test_twin() {
  for (const [kw, t, w, h, shaken, shakes, fg, bold, text] of TWIN) {
    const rows = scene(t, w, h, look(kw), shaken, shakes);
    const what = `${JSON.stringify(kw)} ${t} ${w}x${h}`;
    assertEqual(plain(rows), text, what);
    assertEqual(rows.flat().reduce((n, r) => n + (r.fg ?? 0) * len(r.t), 0), fg, what);
    assertEqual(rows.flat().reduce((n, r) => n + (r.bold ? len(r.t) : 0), 0), bold, what);
  }
}

export function test_draws() {
  for (const [n, u, spin] of DRAWS) {
    const rng = new Twister(n);
    assertEqual(rng.random(), u, `${n}`);
    assertEqual(rng.uniform(4, 9) * rng.choice([-1, 1]), spin, `${n}`);
  }
}

export function test_art() {
  assertEqual(SIZES, [...SIZES].sort((a, b) => a[0] - b[0] || a[1] - b[1]));
  for (const d of DOMES) {
    const [w] = d.size;
    assert(d.art.every((row) => row.length === w));
    const free = [...d.free()].map((k) => k.split(",").map(Number));
    assert(free.length && free.every(([x, y]) => 0 < x && x < w - 1 && 0 < y && y <= d.ground));
    for (const [art] of Object.values(SCENES).map((s) => s[d.scale])) {
      // The scene stands inside the glass.
      const [x0, x1] = d.span(d.ground);
      assert(art[0].length <= x1 - x0 + 1 && art.length <= d.ground);
    }
  }
  for (const [name, scales] of Object.entries(SCENES)) {
    assertEqual(Object.keys(scales).sort(), ["l", "m", "s"], name);
    for (const [art, mask] of Object.values(scales)) {
      assertEqual(art.length, mask.length, name);
      art.forEach((row, y) => {
        assertEqual(row.length, mask[y].length, name);
        [...row].forEach((glyph, x) => assert(glyph === " " || mask[y][x] in INK, `${name} ${row}`));
      });
    }
  }
}

export function test_bounds() {
  for (const lk of LOOKS) {
    for (const [w, h] of SIZES) {
      for (const t of [0, 3.7, 30, 31, 500]) {
        const rows = scene(t, w, h, lk, 29.5, 1);
        assertEqual(plain(rows).map(len), Array(h).fill(w), `${w}x${h} ${t}`);
        // The dome stands whatever falls in it.
        const d = dome(w, h);
        plain(rows).forEach((text, y) => {
          if (y !== d.ground) assert([...text].every((c, x) => d.art[y][x] === " " || d.art[y][x] === c), text);
        });
        for (const r of rows.flat()) assert(Object.keys(r).every((k) => ["t", "fg", "bold"].includes(k)), JSON.stringify(r));
      }
    }
  }
}

export function test_pure() {
  for (const lk of [look(), ALL]) {
    assertEqual(scene(7.3, 21, 10, lk), scene(7.3, 21, 10, lk));
    const now = JSON.stringify(scene(7.3, 21, 10, lk));
    assert([0.5, 1, 1.5].some((dt) => JSON.stringify(scene(7.3 + dt, 21, 10, lk)) !== now));
  }
  assert(JSON.stringify(scene(7.3, 21, 10)) !== JSON.stringify(scene(7.3, 21, 10, look({ seed: 2 }))));
}

export function test_bare() {
  for (const [w, h] of SIZES) {
    const frames = Array.from({ length: 60 }, (_, t) => scene(t / 2, w, h));
    assert(Math.max(...frames.map(snow)) > 0 && new Set(frames.map((f) => JSON.stringify(f))).size > 20); // it falls
    const text = plain(frames[0]).join("\n");
    assert(text.includes("[]") && text.includes("♣")); // the house, a tree
  }
}

export function test_auto() {
  assertEqual(auto(1000, look()), null);
  const lk = look({ shake: true, every: 40 });
  assertEqual(auto(1, lk), null);
  const times = [...new Set(Array.from({ length: 390 }, (_, i) => auto(10 + i, lk)))].filter((a) => a !== null).sort((a, b) => a - b);
  assertEqual(times.length, 10); // one an `every`
  assert(times.every((at, k) => 40 * k < at && at < 40 * (k + 1)));
  for (let t = 40; t < 400; t++) assert(auto(t, lk) <= t);
}

export function test_shake() {
  const mean = (since) => {
    let n = 0;
    for (let i = 0; i < 8; i++) n += snow(scene(100 + since + i / 4, 30, 13, look(), 100));
    return n / 8;
  };
  let calm = 0;
  for (let i = 0; i < 8; i++) calm += snow(scene(100 + i / 4, 30, 13)) / 8;
  assert(mean(0.2) > 1.5 * calm); // thick, then thin again
  assert(mean(0.2) > mean(8) && mean(8) > mean(60) - 3);
  assert(Math.abs(mean(60) - calm) < 4);
  // The flakes stay under the glass while they swirl.
  const free = DOMES[DOMES.length - 1].free();
  for (let i = 0; i < 60; i++) {
    plain(scene(100 + i / 10, 30, 13, look(), 100)).forEach((text, y) => {
      [...text].forEach((c, x) => {
        if ("·*".includes(c)) assert(free.has(`${x},${y}`), `${x},${y}`);
      });
    });
  }
}

export function test_swirls() {
  // The draws are brisk while a shake throws the flakes, no longer.
  const calm = look();
  const shaking = look({ shake: true });
  assert(!swirls(50, calm));
  assert(swirls(50, calm, 49));
  assert(!swirls(50, calm, 40));
  assert(!swirls(50, calm, 51)); // not yet
  const at = auto(100, shaking);
  assert(at !== null && swirls(at + 0.5, shaking));
  assert(!swirls(at + 8, shaking) && !swirls(at - 0.5, shaking));
  assert(BRISK < PERIOD);
}

export function test_drift() {
  const lk = look({ drift: true });
  const lying = (since) => {
    const rows = scene(50 + since, 21, 10, lk, 50);
    const ground = [...plain(rows)[6]].filter((c) => "▁▂".includes(c)).length;
    const white = [...rows[4], ...rows[5]].filter((r) => r.fg === SNOW).length;
    return [ground, white];
  };
  assertEqual(lying(0.5)[0], 0); // lifted by the shake
  assert(lying(14)[0] > 0);
  assert(lying(40)[0] >= lying(14)[0]);
  const roofs = [51, 90].map((t) => scene(t, 21, 10, lk, 50)[4]);
  assertEqual(
    roofs.map((row) => row.some((r) => r.fg === SNOW && r.t.includes("/"))),
    [false, true],
  );
  // Never shaken: it lies.
  assertEqual(scene(5, 21, 10, lk), scene(5, 21, 10, lk));
  assert(plain(scene(5, 21, 10, lk)).some((text) => text.includes("▂")));
}

export function test_lights() {
  const lk = look({ lights: true });
  const colours = new Set();
  const glint = new Set();
  for (let i = 0; i < 90; i++) {
    const rows = scene(i / 5, 21, 10, lk);
    for (const r of [...rows[5], ...rows[6]]) if (GARLAND.includes(r.fg)) colours.add(r.fg);
    if (rows[0].some((r) => r.fg === GLINT)) glint.add(i);
  }
  assert(colours.size >= 3);
  assert(glint.size > 0 && glint.size < 60); // it passes, then waits
  assert(scene(1, 21, 10).flat().every((r) => ![...GARLAND, GLINT].includes(r.fg)));
}

export function test_scenes() {
  assertEqual(which(look({ scene: "fir" }), 3), "fir");
  const lk = look({ random: true });
  const names = Object.keys(SCENES).map((_, n) => which(lk, n));
  assertEqual(names.sort(), Object.keys(SCENES).sort()); // another at each shake
  const marks = { house: "[]", snowman: '(")', church: "†", fir: "/o\\" };
  for (const [name, mark] of Object.entries(marks)) {
    for (const [w, h] of SIZES) assert(plain(scene(0, w, h, look({ scene: name }))).join("\n").includes(mark), `${name} ${w}x${h}`);
  }
}

export function test_shaker() {
  const hand = new Shaker(3);
  assertEqual([hand.at, hand.count], [null, 0]);
  hand.move(10);
  hand.move(11); // still over it: the same shake
  assertEqual([hand.at, hand.count], [10, 1]);
  hand.move(13.5);
  assertEqual([hand.at, hand.count], [13.5, 2]);
  hand.leave();
  hand.move(14); // back over it
  assertEqual([hand.at, hand.count], [14, 3]);
}

export function test_options() {
  const bare = options({});
  assertEqual(bare, { shake: false, drift: false, lights: false, random: false, scene: "house", every: 45 });
  const all = options({ all: true, every: 10 });
  assertEqual([all.shake, all.drift, all.lights, all.random, all.every], [true, true, true, true, 10]);
  // A scene named: none at random.
  assertEqual(options({ all: true, scene: "fir" }).random, false);
  assertEqual(options({ scene: "fir" }).scene, "fir");
}
