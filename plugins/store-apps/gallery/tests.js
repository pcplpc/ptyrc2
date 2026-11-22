// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of gallery, `ptyrc dev test --unit`: those of the twin
// (test_gallery.py) on the pure parts, and the twin's looks (`twin.js`).

import { asset, assertEqual, assert, assertThrows, assets } from "ptyrc";
import { collection, options, shipped, sources, span } from "./app.js";
import { Hall, Slot } from "./hall.js";
import { KINDS, PictureError, parse } from "./picture.js";
import { Twister } from "./twister.js";
import { ANIMALS, ORDER, PICTURES, SHIP, WALL, WHOLE } from "./twin.js";

const PEAR = `# a comment
name: Pear
author: someone
kind: still
period: 1
hold: 3 1
eyes: <o>
ink: g=70 y=220/52 w=255! s=/17
== 5x3
 ,
(o)
 "
::
 g
y@y
 w
++
 ,
(-)
== 3x1
(o)
::
y@y
++
(-)
`;

const text = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const ink = (p, k) => {
  const i = p.inks.get(k);
  return [i.fg, i.bg, i.bold];
};

function fnv(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

const LOOKS = (w, h) => [null, [0, 0], [w - 1, h - 1], [w >> 1, 0], [0, h >> 1], [w >> 1, h - 1]];

/** A host of the test's own: `assets` the tree's, `files` named
 * `{name: text}`, `dirs` `{name: {child: text}}`. */
function host(files = {}, dirs = {}, ship) {
  const entry = (name, body) => ({
    name,
    bytes: () => new TextEncoder().encode(body),
    list: () => Object.keys(body),
    child: (n) => entry(n, body[n]),
  });
  return {
    assets,
    asset: (n) => (n === "SHIPPED" && ship !== undefined ? ship : asset(n)),
    file: (h) => (h.file in files ? entry(h.name, files[h.file]) : entry(h.name, dirs[h.file])),
  };
}

const o = (rest = {}) => options({ ...rest });

// test_parse
export function test_parse() {
  const pear = parse(PEAR, "pear.pic");
  assertEqual([pear.name, pear.author, pear.kind], ["Pear", "someone", "still"]);
  assertEqual([pear.period, pear.hold, pear.eyes], [1, [3, 1], "<o>"]);
  assertEqual(ink(pear, "y"), [220, 52, false]);
  assertEqual(ink(pear, "w"), [255, null, true]);
  assertEqual(ink(pear, "s"), [null, 17, false]);
  assertEqual(pear.boxes, [[5, 3], [3, 1]]);
  assert(pear.frames === 2 && pear.stem === "pear");
  assertEqual(text(pear.render(5, 3)), [" ,   ", "(o)  ", ' "   ']);
  // No `::`: the ink of the frame before.
  assertEqual(pear.sizes.get("5x3")[1].ink, pear.sizes.get("5x3")[0].ink);
  assert(pear.label() === "Pear" && pear.label(true) === "Pear · someone");
}

// test_render_runs
export function test_render_runs() {
  const pear = parse(PEAR);
  const rows = pear.render(5, 3);
  assertEqual(rows[1].slice(0, 3), [{ t: "(", fg: 220, bg: 52 }, { t: "o" }, { t: ")", fg: 220, bg: 52 }]);
  assertEqual(rows[2][1], { t: '"', fg: 255, bold: true });
  assertEqual(rows, parse(PEAR).render(5, 3)); // pure
  assertThrows(() => pear.render(4, 4), RangeError, "no size 4x4");
}

// test_frames_and_hold
export function test_frames_and_hold() {
  const pear = parse(PEAR);
  assertEqual([0, 1, 2.9, 3, 3.9, 4, 7].map((t) => pear.frameAt(t)), [0, 0, 0, 1, 1, 0, 1]);
  assertEqual(text(pear.render(3, 1, 3.5)), ["(-)"]);
  const still = parse("name: x\n== 1x1\n#\n");
  assert(still.frameAt(99) === 0 && still.kind === "abstract");
  assertEqual(still.author, "anonymous");
}

// test_eyes
export function test_eyes() {
  const pear = parse(PEAR);
  assertEqual(text(pear.render(3, 1, 0, [0, 0])), ["(<)"]);
  assertEqual(text(pear.render(3, 1, 0, [1, 0])), ["(o)"]);
  assertEqual(text(pear.render(3, 1, 0, [2, 0])), ["(>)"]);
  // The frame shown has no eye: nothing turns.
  const owl = parse(asset("owl.pic"), "owl.pic");
  assert(text(owl.render(11, 7, 0, [0, 2]))[2].includes("◐"));
  assertEqual(text(owl.render(11, 7, 3.7, [0, 2]))[2], "  █━ ▼ ━█  ");
  const five = parse("name: x\neyes: <o>^v\n== 3x3\n\n o\n::\n\n @\n");
  assertEqual(five.gaze(3, 3, [1, 0]), [[1, 1, "^"]]);
  assertEqual(five.gaze(3, 3, [1, 2]), [[1, 1, "v"]]);
  assertEqual(five.gaze(3, 3, [0, 1]), [[1, 1, "<"]]);
  assertEqual(five.gaze(3, 3, null), [[1, 1, "o"]]);
}

// test_errors
export function test_errors() {
  const cases = [
    ["== 1x1\nx\n", "no name"],
    ["name: x\n", "no size"],
    ["name: x\nkind: cubist\n== 1x1\n", "kind 'cubist'"],
    ["name: x\ncolour: 3\n== 1x1\n", "p:2: not a header line"],
    ["name: x\nperiod: soon\n== 1x1\n", "p:2: period"],
    ["name: x\nink: a=300\n== 1x1\n", "colour '300'"],
    ["name: x\nink: ab=3\n== 1x1\n", "ink 'ab=3'"],
    ["name: x\n== 2x1\nabc\n", "p:3: a row of glyphs of 3 cells in a 2x1"],
    ["name: x\n== 2x1\na\nb\n", "p:4: 2 rows of glyphs in a 2x1"],
    ["name: x\n== 2x1\n中\n", "is not one cell wide"],
    ["name: x\n== 2x1\na\n::\nq\n", "p:5: ink 'q' not in `ink:`"],
    ["name: x\n== 2x1\na\n::\n@\n", "p:5: `@`: no `eyes:`"],
    ["name: x\n== 2x1\na\n::\n\n::\n", "`::` twice"],
    ["name: x\n== 1x1\na\n== 1x1\nb\n", "given twice"],
    ["name: x\n== 1x1\na\n++\nb\n== 2x1\na\n", "want one count"],
    ["name: x\nhold: 1 2\n== 1x1\na\n", "hold: one number a frame"],
    ["name: x\neyes: ab\n== 1x1\na\n", "eyes: want 3 or 5"],
  ];
  for (const [source, what] of cases) {
    const e = assertThrows(() => parse(source, "p"), PictureError);
    assert(e.message.startsWith("p") && e.message.includes(what), `${JSON.stringify(source)}: ${e.message}`);
  }
}

// test_collection_shipped
export function test_collection_shipped() {
  const pictures = collection(o({ candidates: true }));
  const files = assets().filter((n) => n.endsWith(".pic"));
  assert(pictures.length === files.length && files.length >= 20);
  assertEqual([...new Set(pictures.map((p) => p.kind))].sort(), [...KINDS].sort());
  assertEqual(new Set(pictures.map((p) => p.name)).size, pictures.length);
  assert(pictures.some((p) => p.frames > 1));
  assert(pictures.some((p) => p.sizes.size > 1));
  assert(pictures.some((p) => p.eyes));
  const animals = collection(o({ kind: "animal", candidates: true }));
  assert(animals.length && animals.every((p) => p.kind === "animal"));
  assertEqual(collection(o()).map((p) => p.stem), SHIP);
  assertEqual(collection(o({ kind: "animal" })).map((p) => p.stem), ANIMALS);
}

// test_picture: every picture shipped parses, fills its sizes, inks what it draws.
export function test_picture() {
  for (const name of assets().filter((n) => n.endsWith(".pic"))) {
    const body = asset(name);
    assert(body.startsWith("# SPDX-License-Identifier:"), name);
    const p = parse(body, name);
    assert(p.frames <= 6 && p.period >= 0.2, name);
    for (const [key, frames] of p.sizes) {
      const [w, h] = key.split("x").map(Number);
      assert(w >= 3 && w <= 40 && h >= 1 && h <= 14, `${name}: ${key}`);
      frames.forEach((frame, nth) => {
        const hold = p.hold.length ? p.hold : Array(p.frames).fill(1);
        const t = (hold.slice(0, nth).reduce((a, b) => a + b, 0) + 0.5) * p.period;
        assertEqual(p.frameAt(t), nth, `${name}: frame`);
        const rows = p.render(w, h, t, [0, 0]);
        assert(rows.length === h && text(rows).every((r) => Array.from(r).length === w), `${name}: ${key}`);
        frame.rows.forEach((glyphs, y) =>
          glyphs.forEach((g, x) => {
            const k = frame.ink[y][x];
            const bg = p.inks.get(k)?.bg ?? null;
            assert(g === " " || !" .".includes(k), `${name}: ${nth}: ${glyphs.join("")}`);
            assert(g !== " " || " .".includes(k) || bg !== null, `${name}: ${nth}: ${frame.ink[y].join("")}`);
          }),
        );
      });
    }
  }
}

// The twin's looks: every picture, size, frame and look, as the twin renders it.
export function test_twin_looks() {
  const pictures = collection(o({ candidates: true }));
  assertEqual(pictures.map((p) => p.stem), PICTURES.map((p) => p.stem));
  pictures.forEach((p, i) => {
    const f = PICTURES[i];
    assertEqual([p.name, p.author, p.kind, p.period, p.hold, p.eyes], [f.name, f.author, f.kind, f.period, f.hold, f.eyes], f.stem);
    assertEqual(p.boxes, f.sizes.map(([w, h]) => [w, h]), `${f.stem}: sizes`);
    for (const [w, h, frames] of f.sizes)
      for (const [t, nth, hashes] of frames) {
        assertEqual(p.frameAt(t), nth, `${f.stem}: frame at ${t}`);
        LOOKS(w, h).forEach((look, k) => {
          const rows = p.render(w, h, t, look);
          assert(fnv(JSON.stringify(rows)) === hashes[k], `${f.stem} ${w}x${h} at ${t}, look ${look}: ${JSON.stringify(rows)}`);
        });
      }
  });
  const first = pictures.find((p) => p.stem === WHOLE.stem);
  assertEqual(first.render(...WHOLE.size), WHOLE.rows);
}

// test_ship_list
export function test_ship_list() {
  assertEqual(shipped("# best first\nowl\n\nteapot\nnot-there\n"), ["owl", "teapot", "not-there"]);
  const ship = host({}, {}, "# best first\nowl\n\nteapot\nnot-there\n");
  assertEqual(collection(o(), ship).map((p) => p.stem), ["owl", "teapot"]);
  assertEqual(collection(o({ candidates: true }), ship).length, PICTURES.length);
  // A file or a directory named: the ship list is not asked.
  const named = host({ f0: PEAR }, { f1: { "pear.pic": PEAR, "notes.txt": "x", "b.pic": "name: B\n== 1x1\nb\n" } });
  const dir = { file: "f1", name: "mine" };
  assertEqual(collection(o({ dir: [dir] }), named).map((p) => p.name), ["B", "Pear"]);
  assertEqual(collection(o({ file: [{ file: "f0", name: "pear.pic" }] }), named).map((p) => p.name), ["Pear"]);
  assertEqual(collection(o({ kind: "silly", dir: [dir] }), named), []);
  assertEqual(sources(o({ dir: [dir] }), named).map(([s]) => s), ["mine/b.pic", "mine/pear.pic"]);
  // The source names the file of an error.
  const bad = host({ f0: "name: x\n" });
  const e = assertThrows(() => collection(o({ file: [{ file: "f0", name: "bad.pic" }] }), bad), PictureError);
  assertEqual(e.message, "bad.pic: no size: want a line `== WxH`");
  // A file not UTF-8: refused, a picture that parses once replaced.
  const pear = [0x23, 0xe9, 0x0a, ...new TextEncoder().encode(PEAR)];
  const latin = { assets, asset, file: () => ({ name: "l.pic", bytes: () => new Uint8Array(pear) }) };
  const l = assertThrows(() => collection(o({ file: [{ file: "f0", name: "l.pic" }] }), latin), PictureError);
  assertEqual(l.message, "l.pic: not UTF-8");
}

// test_hall
export function test_hall() {
  const all = collection(o({ candidates: true }));
  const pictures = WALL.stems.map((s) => all.find((p) => p.stem === s));
  const hall = new Hall(pictures, new Twister(1), { count: 3, every: 60 });
  const again = new Hall(pictures, new Twister(1), { count: 3, every: 60 });
  assertEqual(hall.slots.map((s) => s.picture.stem), WALL.first);
  assertEqual(again.slots.map((s) => s.picture.stem), WALL.first);
  assertEqual(hall.slots.map((s) => s.until), [20, 40, 60]); // staggered
  assert(hall.due === 20 && hall.at(19).length === 0);
  const seen = new Set();
  for (const [t, fresh] of WALL.steps) {
    const got = hall.at(t);
    assertEqual(got.map((s) => [hall.slots.indexOf(s), s.picture.stem]), fresh, `at ${t}`);
    for (const s of got) {
      seen.add(s.picture);
      assert(s.since === t && s.until === t + 60);
    }
    assertEqual(new Set(hall.slots.map((s) => s.picture)).size, 3); // never twice the same at once
  }
  assert(seen.size === pictures.length && pictures.every((p) => seen.has(p)));
  // Fewer pictures than nails: fewer nails; one picture stays.
  const alone = new Hall(pictures.slice(0, 1), new Twister(0), { count: 3, every: 1 });
  assert(alone.slots.length === 1 && alone.at(5).length === 0);
  assertThrows(() => new Hall([], new Twister(0)), RangeError, "no picture");
  // The ship list hung one after the other, as the twin of seed 7.
  const one = new Hall(collection(o()), new Twister(7));
  const order = [one.slots[0].picture.stem];
  for (let t = 600; t < 600 * 60; t += 600) order.push(...one.at(t).map((s) => s.picture.stem));
  assertEqual(order, ORDER);
}

// test_hall_period
export function test_hall_period() {
  const pear = parse(PEAR);
  assertEqual(new Hall([pear], new Twister(0)).period(), null);
  const hall = new Hall([pear], new Twister(0), { moving: true });
  assertEqual(hall.period(), 1);
  const slot = hall.slots[0];
  hall.at(3.5);
  assertEqual(text(slot.render(3, 1)), ["(-)"]);
  assertEqual(text(new Hall([pear], new Twister(0)).slots[0].render(3, 1)), ["(o)"]);
  // The next tick: a beat of a picture that moves; still and alone: never.
  assertEqual([hall.next(0), hall.next(3.5)], [1, 4]);
  assertEqual(new Hall([pear], new Twister(0)).next(0), Infinity);
  assertEqual(new Hall([pear, parse(PEAR)], new Twister(0), { every: 90 }).next(0), 90);
}

// test_slot_hover
export function test_slot_hover() {
  const pear = parse(PEAR);
  const slot = new Slot(pear, { hover: true, size: [3, 1] });
  const titles = [];
  slot.retitle = () => titles.push(slot.label);
  assertEqual(slot.label, "Pear");
  assertEqual(slot.move(0, 0), true);
  assertEqual(text(slot.render(3, 1)), ["(<)"]);
  assertEqual(titles, ["Pear · someone"]);
  assertEqual(slot.move(0, 0), false); // nothing new to see
  assert(slot.move(2, 0) === true && text(slot.render(3, 1))[0] === "(>)");
  assertEqual(slot.leave(), true);
  assert(text(slot.render(3, 1))[0] === "(o)" && titles[titles.length - 1] === "Pear");
  assertEqual(slot.leave(), false);
  // Without `--hover` the pointer changes nothing.
  const deaf = new Slot(pear, { size: [3, 1] });
  assert(deaf.move(0, 0) === false && text(deaf.render(3, 1))[0] === "(o)");
  assertEqual(new Slot(pear, { author: true }).label, "Pear · someone");
}

// The options: -a, --count, --every, --kind (the twin's argparse types).
export function test_options() {
  assertEqual([span("90"), span("30s"), span("10m"), span("1.5h")], [90, 30, 600, 5400]);
  assertEqual([span("0"), span("1d"), span("m"), span("-1")], [null, null, null, null]);
  const a = o({ all: true });
  assertEqual([a.moving, a.author, a.hover, a.count], [true, true, true, 3]);
  assertEqual([o().count, o({ wall: true }).count, o({ wall: true, count: 2 }).count, o().every], [1, 3, 2, 600]);
  assertEqual(o({ kind: "animal,,silly" }).kinds, ["animal", "silly"]);
  assertThrows(() => o({ kind: "cubist" }), RangeError, "gallery: --kind: cubist: not one of portrait");
  assertThrows(() => o({ every: "soon" }), RangeError, "not a duration: soon");
}
