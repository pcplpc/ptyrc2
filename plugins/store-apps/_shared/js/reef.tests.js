// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of reef.js: test_reef.py of the twin (its art, places,
// memory, bubbles, events; the swim of test_species_swim and
// test_swim_changes_depth_by_the_clock), test_fish_flees_the_cursor
// of test_apps.py. Draws from `seeded`, not the twin's.

import { assert, assertEqual } from "ptyrc";
import {
  BEHIND,
  CALM,
  CLASSIC,
  CROSSERS,
  DECOR,
  Decor,
  Events,
  FEAR,
  FRONT,
  Fish,
  LEFT,
  RIGHT,
  Reef,
  SPECIES,
  SUNK,
  TALL,
  depths,
  inkOf,
  mirror,
  pad,
  paint,
  seeded,
  stand,
  swim,
} from "./reef.js";

const BOX = [0, 100, 60, 123]; // 60 x 24 cells, scrolled to row 100
const FRAME = 1;

/** Stands for a `Batch`: what was drawn, and how it ended. */
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
  get live() {
    return this.drawn.filter((d) => !d.gone);
  }
}

/** A view of `h` empty rows with `text` at the given rows. */
function water(text, h = 24, w = 60) {
  const rows = new Array(h).fill("");
  for (const [y, line] of text) rows[y] = line;
  return pad(rows, w);
}

const range = (a, b) => Array.from({ length: b - a }, (_, i) => a + i);
const sorted = (s) => [...s].sort((a, b) => a - b);

export function test_art() {
  for (const art of [...DECOR, ...CROSSERS, ...SUNK.map((s) => s.fall), ...SUNK.map((s) => s.rest)]) {
    const sizes = new Set(art.frames.flatMap((rows) => rows.map((row) => `${row.length}x${rows.length}`)));
    assertEqual([...sizes], [`${art.w}x${art.h}`], art.name);
    assert(art.inks.length && art.inks.every((ink) => "" in ink), art.name);
    // It stands on its last row, in every frame.
    assert(
      art.frames.every((rows) => rows[rows.length - 1].trim()),
      art.name,
    );
    for (const rows of art.frames) assertEqual(mirror(mirror(rows)), rows, art.name);
  }
  assertEqual(new Set(DECOR.map((s) => s.name)).size, DECOR.length);
  assertEqual(mirror([RIGHT]), [LEFT]);
  assertEqual(CLASSIC.glyph(false, 3), LEFT);
  for (const kind of SPECIES) {
    assert(new Set(kind.right.map((g) => g.length)).size === 1 && kind.colors.length, kind.name);
    assertEqual(kind.glyph(false, 0).length, kind.length);
  }
  const sword = SPECIES.find((k) => k.name === "sword");
  const eel = SPECIES.find((k) => k.name === "eel");
  assertEqual([sword.glyph(true, 0), sword.glyph(false, 0)], ["><(((°>--", "--<°)))><"]);
  // The eel slithers: another body at each column.
  assertEqual(new Set([eel.glyph(true, 4), eel.glyph(true, 5)]), new Set(eel.right));
  assertEqual(eel.glyph(false, 4), "<:~-~-~-~");
  // A space is transparent; one run per colour.
  assertEqual(paint(["a  bc"], { "": 1, c: 2 }, { dim: true }), [
    [
      { t: "a", fg: 1, dim: true },
      { t: "  " },
      { t: "b", fg: 1, dim: true },
      { t: "c", fg: 2, dim: true },
    ],
  ]);
}

export function test_stand() {
  const rows = water(
    [
      [2, "    xxxx xxxx"],
      [5, "x"],
      [6, "  xxxxxxxxxx   xx"],
    ],
    24,
    20,
  );
  // On row 2: ink under 2 cells of 3; nothing above.
  assertEqual(stand(rows, 2, 20, 3, 2), range(3, 12));
  assertEqual(stand(rows, 2, 20, 1, 1), [4, 5, 6, 7, 9, 10, 11, 12]);
  assertEqual(stand(rows, 2, 20, 3, 3), []); // out of the view
  // On row 6: the x of row 5 and a column each side are in the way.
  assertEqual(stand(rows, 6, 20, 3, 1), [...range(2, 11), 14, 15]);
  assertEqual(stand(rows, 6, 20, 3, 4), [14, 15]); // row 2 is in the way too
  assertEqual(stand(rows, 7, 20, 1, 1), []); // no text to stand on
  assertEqual(depths(rows, 6), [5, 24, 6, 6, 2, 2]);
  // The ink read once: the same columns.
  const ink = inkOf(rows, 20);
  for (const [g, w, h] of [[2, 3, 2], [2, 1, 1], [6, 3, 1], [6, 3, 4], [7, 1, 1]]) assertEqual(stand(rows, g, 20, w, h, ink), stand(rows, g, 20, w, h));
}

export function test_reef_remembers() {
  const rnd = seeded(1);
  const line = "the quick brown fox jumps over the lazy dog";
  const lines = [
    [3, line],
    [9, line.slice(0, 9)],
    [10, line],
    [20, line],
  ];
  const rows = water(lines);
  const reef = new Reef(1.0);
  // A view on the move: the rows near its top wait.
  const planted = reef.survey(rows, BOX, false, rnd);
  assertEqual(sorted(new Set(planted.map((d) => d.ground))), [109, 110, 120]);
  assertEqual(sorted(reef.seen), range(100 + TALL, 123));
  planted.push(...reef.survey(rows, BOX, true, rnd));
  assertEqual(sorted(new Set(planted.map((d) => d.ground))), [103, 109, 110, 120]);
  assertEqual(sorted(reef.seen), range(101, 123));
  for (const d of planted) {
    const [x, y] = d.at;
    for (let r = y; r < y + d.shape.h; r++) assert(!rows[r - 100].slice(x, x + d.shape.w).trim(), d.shape.name);
    assert(rows[d.ground - 100].slice(x, x + d.shape.w).trim(), d.shape.name);
  }
  const cells = planted.flatMap((d) => d.cells());
  assertEqual(new Set(cells).size, cells.length);
  // Room above row 120: something tall.
  assert(planted.find((d) => d.ground === 120).shape.h > 1);
  // Populated once: nothing new there, whatever the text and the view.
  const before = reef.decor.map((d) => [d.shape.name, d.at]);
  assertEqual(reef.survey(rows, BOX, true, rnd), []);
  const up = pad([...new Array(5).fill(""), ...rows.slice(0, -5)], 60); // the view 5 rows up
  assertEqual(reef.survey(up, [0, 95, 60, 118], true, rnd), []);
  const below = reef.survey(rows, [0, 124, 60, 147], true, rnd); // a page down
  assert(below.length && below.every((d) => d.ground > 124));
  assertEqual(
    reef.decor.slice(0, before.length).map((d) => [d.shape.name, d.at]),
    before,
  );
  // The text under one goes: it is forgotten, its row is surveyed again.
  const gone = reef.prune(water(lines.slice(1)), BOX);
  assertEqual(
    gone.map((d) => d.ground),
    [103],
  );
  assert(!reef.seen.has(103));
  assertEqual(reef.prune(water(lines.slice(1)), BOX), []);
}

export function test_reef_shows_what_is_near() {
  const rnd = seeded(2);
  const rows = water(range(0, 5).map((i) => [6 + 4 * i, "xxxx xxxx xxxx xxxx"]));
  const reef = new Reef(1.0);
  const canvas = new Canvas();
  reef.survey(rows, BOX, true, rnd);
  reef.show(canvas, FRAME, BOX);
  const first = canvas.live.map((d) => d.spec);
  assertEqual([first.length, reef.decor.length], [5, 5]);
  assert(first.every((s) => s.anchor.space === "frame" && s.layer < 0 && s.frame === FRAME));
  reef.show(canvas, FRAME, [0, 124, 60, 147]); // a page down: still near
  assertEqual([canvas.live.length, canvas.drawn.length], [5, 5]);
  reef.show(canvas, FRAME, [0, 400, 60, 423]); // far: no sprite, all kept
  assertEqual([canvas.live.length, reef.decor.length], [0, 5]);
  reef.show(canvas, FRAME, BOX); // back: the same, redrawn
  assertEqual(
    canvas.live.map((d) => d.spec),
    first,
  );
}

export function test_bubbles() {
  const rnd = seeded(3);
  const reef = new Reef();
  const canvas = new Canvas();
  const vents = reef.area(BOX, 0.0, rnd);
  assert(vents.length === 3 && vents.every((v) => 0 < v.x && v.x < 59));
  // An area keeps its vents; another has its own; back, the same.
  assert(reef.area([0, 103, 60, 126], 0.0, rnd) === vents);
  assert(reef.area([0, 112, 60, 135], 0.0, rnd) !== vents);
  assert(reef.area(BOX, 9.0, rnd) === vents);
  for (let tick = 0; tick < 600; tick++) reef.bubbles(canvas, FRAME, BOX, tick * 0.1, rnd);
  // A minute, three vents: some 0.5 a second each (the twin: 80 to 95).
  assert(60 < canvas.drawn.length && canvas.drawn.length < 120, canvas.drawn.length);
  assertEqual(
    [...new Set(canvas.drawn.map((d) => `${d.spec.anchor.at}`))].sort(),
    vents.map((v) => `${v.x - 1},123`).sort(),
  );
  for (const { spec } of canvas.drawn) {
    assert(spec.motion.vy < 0 && spec.loop === false);
    // It reaches the top of the view, 24 rows up, as it ends.
    const life = spec.frames.reduce((n, f) => n + f.ms, 0) / 1000;
    const way = -spec.motion.vy * life;
    assert(24 <= way && way < 25 && spec.ttl_ms > life * 1000, way);
    const runs = spec.frames.flatMap((f) => f.cells[0]);
    assertEqual([...new Set(runs.map((r) => r.t.trim()).join(""))], ["."]);
    assert(runs.every((r) => r.dim ?? true));
  }
}

export function test_sunk_object_lands_and_stays() {
  const rnd = seeded(6);
  const rows = water([
    [0, "x".repeat(20)],
    [12, "x".repeat(50)],
    [20, "x".repeat(60)],
  ]);
  const reef = new Reef();
  const canvas = new Canvas();
  const events = new Events(reef, 0.0, rnd);
  events.drop(canvas, FRAME, rows, BOX);
  const fall = events.fall;
  assert(fall !== null && events.busy);
  const [w, h] = [fall.kind.fall.w, fall.kind.fall.h];
  // Down a free column, onto the first text below.
  assert(21 <= fall.x && fall.rest === 100 + (fall.x < 50 ? 12 : 20) - h, fall);
  assertEqual(canvas.drawn[0].spec.anchor.at, [fall.x, 100]);
  for (let i = 0; i < 100; i++) events.step(canvas, FRAME, rows, BOX, 0.0, 0.1);
  assert(events.fall === null && canvas.drawn.length === 1);
  assertEqual(reef.decor.length, 1);
  const [landed] = reef.decor;
  assert(landed.sprite === canvas.drawn[0] && !canvas.drawn[0].gone);
  assertEqual(canvas.drawn[0].spec.anchor.at, [fall.x, fall.rest]);
  assertEqual(landed.at, [fall.x, fall.rest]);
  assert(landed.shape === fall.kind.rest && landed.shape.w === w);
  assertEqual(reef.prune(rows, BOX), []);
  // No free column 3 rows deep: nothing falls.
  events.drop(canvas, FRAME, water([[2, "x".repeat(60)]]), BOX);
  assert(events.fall === null);
}

export function test_hook() {
  const rnd = seeded(7);
  const rows = water([
    [0, "x".repeat(30)],
    [8, "x".repeat(60)],
  ]);
  const canvas = new Canvas();
  const events = new Events(new Reef(), 0.0, rnd);
  events.cast(canvas, FRAME, rows, BOX);
  const hook = events.hook;
  assert(hook !== null && hook.x >= 30 && 3 <= hook.most && hook.most <= 8, hook);
  let deepest = 0;
  let threats = [];
  const feared = [];
  for (let i = 0; i < 400; i++) {
    [threats] = events.step(canvas, FRAME, rows, BOX, 0.0, 0.1);
    if (events.hook === null) break;
    const cells = canvas.drawn[0].spec.content.cells;
    assertEqual(
      cells.map((r) => r[0].t),
      [...new Array(cells.length - 1).fill("|"), "J"],
    );
    deepest = Math.max(deepest, cells.length);
    feared.push(...threats.map((p) => `${p}`));
  }
  // Down to its depth, then up and gone; fishes fear its tip.
  assert(events.hook === null && canvas.drawn[0].gone && deepest === hook.most);
  assert(feared.includes(`${hook.x},${100 + hook.most - 1}`) && !threats.length);
}

export function test_crossers() {
  const rnd = seeded(8);
  const canvas = new Canvas();
  const events = new Events(new Reef(), 0.0, rnd);
  const seen = new Set();
  for (let i = 0; i < 60; i++) {
    events.crosses(canvas, FRAME, BOX, 0.0);
    const cross = events.cross;
    const { spec } = canvas.drawn[canvas.drawn.length - 1];
    seen.add(cross.kind.name);
    const { vx, vy } = spec.motion;
    const [x, y] = spec.anchor.at;
    const { w, h } = cross.kind;
    const life = cross.ends - cross.born;
    assert(Math.abs(spec.ttl_ms - life * 1000) < 1);
    // It starts out of the view and is gone once across.
    if (cross.kind.rises) {
      assert(vy < 0 && vx === 0 && y === 124 && 0 <= x && x <= 60 - w);
      assert(Math.abs(-vy * life - (24 + h)) < 1e-6);
    } else {
      assert(x === (vx > 0 ? -w : 60) && vy === 0 && 100 <= y && y <= 124 - h);
      assert(Math.abs(Math.abs(vx) * life - (60 + w)) < 1e-6);
    }
    // Its mouth leads.
    const [mx] = events.mouth(1.0);
    assert(Math.abs(mx - (x + vx + (vx > 0 ? w : 0))) < 1e-9);
    const [threats] = events.step(canvas, FRAME, [], BOX, 1.0, 0.1);
    assertEqual(threats.length > 0, cross.kind.scary);
    events.due = Infinity; // nothing else starts
    events.step(canvas, FRAME, [], BOX, cross.ends, 0.1);
    assert(events.cross === null);
  }
  assertEqual([...seen].sort(), CROSSERS.map((k) => k.name).sort());
}

export function test_decor_spec() {
  const castle = DECOR.find((s) => s.name === "castle");
  const shell = DECOR.find((s) => s.name === "shell");
  let spec = new Decor(castle, [4, 9], castle.inks[0], { phase: 1, ms: 500 }).spec(FRAME);
  assert(spec.layer === BEHIND && spec.loop === true);
  assertEqual(spec.anchor, { at: [4, 9], space: "frame" });
  assertEqual(
    spec.frames.map((f) => f.ms),
    [500, 500, 500],
  );
  const flags = spec.frames.map((f) => f.cells[0].filter((r) => r.fg === 196).map((r) => r.t));
  assertEqual(flags, [[">"], ["~"], [">"]]);
  spec = new Decor(shell, [0, 0], shell.inks[0], { front: true }).spec(FRAME);
  assert(spec.layer === FRONT && spec.z === 1 && !("frames" in spec));
}

export function test_fish_flees_the_cursor() {
  const rnd = seeded(1);
  const box = [0, 0, 80, 24];
  const fish = new Fish({ x: 30.0, y: 10.0, vx: -CALM, color: 6 });
  // Swimming left, towards a cursor at its nose: it turns and bolts.
  swim(fish, 0.1, box, [28, 10], false, rnd);
  assert(fish.vx === FEAR && fish.cell[2] === RIGHT);
  for (let i = 0; i < 20; i++) swim(fish, 0.1, box, [28, 10], false, rnd);
  assert(fish.scared <= 0 && 0 < fish.vx && fish.vx <= CALM, fish); // calm again, away
  // A far cursor frightens only when it moves.
  const far = new Fish({ x: 40.0, y: 10.0, vx: -1.0, color: 6 });
  swim(far, 0.1, box, [28, 10], false, rnd);
  assertEqual(far.vx, -1.0);
  swim(far, 0.1, box, [28, 10], true, rnd);
  assertEqual(far.vx, FEAR);
  // Walls turn it around; a long fish turns earlier at the right one.
  const edge = new Fish({ x: 0.2, y: 0.0, vx: -5.0, color: 6 });
  swim(edge, 0.1, box, null, false, rnd);
  assertEqual([edge.x, edge.vx], [0, 5.0]);
  const sword = new Fish({ x: 70.9, y: 3.0, vx: 5.0, color: 6, kind: SPECIES.find((k) => k.name === "sword") });
  swim(sword, 0.1, box, null, false, rnd);
  assertEqual([sword.x, sword.vx], [71, -5.0]);
  // One that leaves goes through the wall.
  sword.vx = 5.0;
  sword.leaving = true;
  for (let i = 0; i < 40; i++) swim(sword, 0.1, box, null, false, rnd);
  assert(sword.x > 80);
}

export function test_swim_changes_depth_by_the_clock() {
  // A fish changes depth as often a second at any tick.
  const dice = () => 0.01;
  const box = [0, 0, 80, 24];
  const slow = new Fish({ x: 10.0, y: 5.0, vx: 3.0, color: 6 });
  swim(slow, 0.1, box, null, false, dice);
  assertEqual(slow.vy, -1.5); // 0.01 < 0.2 a second, over 0.1 s
  const fast = new Fish({ x: 10.0, y: 5.0, vx: 3.0, color: 6 });
  swim(fast, 1 / 30, box, null, false, dice);
  assertEqual(fast.vy, 0.0);
}
