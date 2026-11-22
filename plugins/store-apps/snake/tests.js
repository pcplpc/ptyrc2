// SPDX-License-Identifier: GPL-3.0-or-later
// The twin's tests (`test_apps.py`, `test_snake_*`), and its games
// step for step (`twin.js`).

import { assert, assertEqual } from "ptyrc";
import { BODY, Batch, Board, X, Y, beam, burst, key, pair, spark, tally, wallsOf } from "./arcade.js";
import { BEAMS, Beam, FIRE, FOES, FUSE, Foe, Game, Plan, SNAKE_Z, Stage, due, options, room, route, way } from "./app.js";
import { Twister } from "./twister.js";
import { DRAWS, GAMES, ROOMS, SPARKS } from "./twin.js";


const K = (cells) => cells.map(([x, y]) => key(x, y));
const P = (keys) => keys.map(pair);
const set = (cells) => new Set(K(cells));
const box = (x0, y0, x1, y1) => [x0, y0, x1, y1];

/** The twin's text of a game after a step. */
function state(g) {
  const at = (k) => `${X(k)},${Y(k)}`;
  return [
    g.body.map(at).join(";"),
    g.apple === null ? "-" : at(g.apple),
    g.golden ? "1" : "0",
    String(g.grow),
    String(g.eaten),
    g.foes.map((f) => `${f.n}:${at(f.at)}`).join(";"),
    g.beams.map((b) => `${b.n}:${at(b.cells[0])}:${b.cells.length}:${b.across ? 1 : 0}:${b.fuse}:${b.left}`).join(";"),
    g.events.map((e) => e[0]).join(";"),
    g.lumps.join(","),
    g.ghost ? "1" : "0",
  ].join("|");
}

function fnv(h, text) {
  for (const b of new TextEncoder().encode(text)) h = Math.imul(h ^ b, 16777619) >>> 0;
  return h;
}

function inside(b, cells) {
  return cells.every((c) => b[0] <= X(c) && X(c) < b[2] && b[1] <= Y(c) && Y(c) < b[3]);
}

function joined(cells) {
  for (let i = 1; i < cells.length; i++) {
    if (Math.abs(X(cells[i]) - X(cells[i - 1])) + Math.abs(Y(cells[i]) - Y(cells[i - 1])) !== 1) return false;
  }
  return true;
}

export function test_twin() {
  for (const [rows, x0, y0, b, seed, flags, steps, marks, last] of GAMES) {
    const walls = wallsOf(rows, x0, y0);
    const g = new Game(new Twister(seed), flags);
    let h = 2166136261;
    const got = [];
    for (let i = 0; i < steps; i++) {
      g.step(b, walls);
      h = fnv(h, state(g));
      if ((i + 1) % 100 === 0 || i + 1 === steps) got.push(h);
    }
    const what = `seed ${seed} ${JSON.stringify(flags)}`;
    assertEqual(state(g), last, what);
    assertEqual(got, marks, what);
  }
}

export function test_twin_rooms() {
  const walls = wallsOf(ROOM);
  for (const [start, need, cells] of ROOMS) {
    const at = start && key(...start);
    assertEqual(P(room([0, 0, 20, 7], walls, at, need)), cells, `${start} ${need}`);
    assertEqual(P(room([0, 0, 20, 7], walls, at, need, new Plan([0, 0, 20, 7], walls))), cells, `${start} ${need}, a plan`);
  }
}

export function test_twin_sparks() {
  const r = new Twister(2);
  const got = [spark(r, [1, 2]), spark(r, BODY.slice(2), "o:.", [0.5, 4.0], [0.5, 1.1], 14.0), ...burst(2, r, [7], { glyphs: "x+'.", speed: [3.0, 9.0] })];
  got.forEach((one, i) => {
    const want = SPARKS[i];
    assertEqual(one.frames, want.frames.map((f) => ({ cells: f.cells, ms: f.ms })), `${i}`);
    assertEqual(one.ttl_ms, Math.trunc(want.ttl * 1000), `${i}`);
    for (const m of ["vx", "vy", "ay"]) assert(Math.abs(one.motion[m] - want.motion[m]) < 1e-9, `${i} ${m}`);
  });
}

export function test_snake_game() {
  const rows = ["", "  wall of words  ", "", "   another   one", ""];
  const walls = wallsOf(rows, 10, 100);
  assert(walls.has(key(12, 101)) && !walls.has(key(10, 101)) && !walls.has(key(16, 101)));
  assertEqual(new Set(wallsOf(["a世b"])), set([[0, 0], [1, 0], [2, 0], [3, 0]]));
  // Around a wall: the first step of a shortest path.
  const blocked = set([[1, 0], [1, 1]]);
  assertEqual(route(key(0, 0), key(2, 0), (c) => !blocked.has(c) && Y(c) >= 0 && Y(c) < 3), key(0, 1));
  assertEqual(route(key(0, 0), key(2, 0), () => false), null);
  const g = new Game(new Twister(3));
  const b = box(10, 100, 30, 105);
  for (let i = 0; i < 400; i++) {
    g.step(b, walls);
    assertEqual(new Set(g.body).size, g.body.length); // never over itself
    assert(!g.body.some((c) => walls.has(c))); // never over a word
    assert(inside(b, g.body) && joined(g.body));
  }
  assert(g.eaten >= 5);
  const [at] = g.picture();
  assertEqual(at, [Math.min(...g.body.map(X)), Math.min(...g.body.map(Y))]);
  // Scrolled out of sight: back in from the edge, whole.
  const size = g.body.length;
  g.step(box(10, 200, 30, 205), new Set());
  assert(!g.ghost && Y(g.body[0]) === 200);
  assert(size - 1 <= g.body.length && g.body.length <= size);
}

export function test_snake_steer() {
  // A player: waits for an arrow, goes straight, never back over its
  // neck, stops at a wall, bites itself off.
  const g = new Game(new Twister(3), { player: true });
  const b = box(0, 0, 10, 5);
  const walls = set([[9, 2]]);
  g.step(b, walls);
  [g.body, g.grow, g.apple] = [K([[5, 2]]), 0, key(0, 0)];
  g.step(b, walls);
  assertEqual(P(g.body), [[5, 2]]); // no arrow yet
  g.press("right");
  g.grow = 3;
  for (let i = 0; i < 3; i++) g.step(b, walls);
  assertEqual(P(g.body), [[8, 2], [7, 2], [6, 2], [5, 2]]);
  g.press("left"); // back over the neck: ignored
  g.step(b, walls);
  assertEqual(pair(g.body[0]), [8, 2]); // against the wall: waits
  g.grow = 2;
  for (const k of ["down", "left", "up"]) {
    g.press(k);
    g.step(b, walls);
  }
  // Into its own body: bitten off there.
  assertEqual(P(g.body), [[7, 2], [7, 3], [8, 3]]);
  assert(g.events.some((e) => e[0] === "lost" && JSON.stringify(P(e[1])) === "[[7,2],[6,2],[5,2]]"));
}

const BAND = ["", "", "####################", "####################", "", "", ""];

export function test_snake_ghost() {
  // Out of sight or under text: through anything, unhurt, to a safe cell.
  const walls = wallsOf(BAND);
  const b = box(0, 0, 20, 7);
  let g = new Game(new Twister(2), { enemies: true, lasers: true });
  g.body = Array.from({ length: 6 }, (_, i) => key(5, -40 - i));
  g.beams = [new Beam(1, K([[5, 0], [5, 1]]), false, 0)];
  const safe = new Set(room(b, walls, null, 0));
  const seen = [];
  for (let i = 0; i < 10; i++) {
    g.step(b, walls);
    seen.push(g.ghost);
    assertEqual(g.body.length, 6);
    if (!g.ghost) break;
  }
  // In from the edge, through the beam and the text, into the largest
  // region; solid on its first cell.
  assert(seen[0] && !seen[seen.length - 1]);
  assertEqual(pair(g.body[0]), [5, 4]);
  assert(safe.has(key(5, 4)));
  // Text written on its head: it goes through the text.
  g = new Game(new Twister(2));
  g.body = K([[5, 2], [5, 1], [5, 0]]);
  g.step(b, walls);
  assert(g.ghost);
  const runs = g.picture()[1].flat().filter((r) => r.t !== " ");
  assert(runs.length === 3 && runs.every((r) => r.dim));
  g.step(b, walls);
  assert(!g.ghost);
  assertEqual(P(g.body), [[5, 4], [5, 3], [5, 2]]);
}

export function test_snake_ghost_asks_once() {
  // The largest region is worked out once a map, not at each step of a
  // ghost; the search over sets gives the step of `route`.
  const walls = wallsOf(BAND);
  const b = box(0, 0, 20, 7);
  const g = new Game(new Twister(2));
  g.body = [key(5, -40)];
  let steps = 0;
  let kept = null;
  while (steps === 0 || g.ghost) {
    g.step(b, walls);
    if (steps && g.ghost) assert(g.known[2] === kept, "kept");
    kept = g.known?.[2] ?? null;
    steps += 1;
  }
  assert(steps >= 5 && kept.size === 1);
  const other = set([[0, 0]]);
  g.step(b, other); // another map: asked again when needed
  g.body = [key(5, -40)];
  g.step(b, other);
  assert(g.known[1] === other && g.known[2] !== kept);
  const rng = new Twister(7);
  for (let i = 0; i < 50; i++) {
    const taken = new Set();
    for (let j = 0; j < 14; j++) taken.add(key(rng.randrange(8), rng.randrange(6)));
    const start = key(rng.randrange(8), rng.randrange(6));
    const goal = key(rng.randrange(8), 5);
    const free = (c) => X(c) >= 0 && X(c) < 8 && Y(c) >= 0 && Y(c) < 6 && !taken.has(c);
    const want = route(start, goal, free);
    assertEqual(way(start, goal, box(0, 0, 8, 6), taken), want);
    assertEqual(new Plan(box(0, 0, 8, 6), new Set()).way(start, goal, [taken]), want);
    assertEqual(new Plan(box(0, 0, 8, 6), taken).way(start, goal), want);
  }
}

const ROOM = [
  "          #         ",
  "########  #  ###### ",
  "       #  #  #    # ",
  "       #  #  #### # ",
  "       #  #       # ",
  "########  ######### ",
  "                    ",
];

export function test_snake_room() {
  // Apples go where the head can reach, never in a dead end.
  let walls = wallsOf(ROOM);
  const b = box(0, 0, 20, 7);
  const ok = new Set(room(b, walls, key(0, 6), 10));
  assert(K([[8, 0], [9, 5], [11, 1], [19, 3], [15, 6]]).every((c) => ok.has(c)));
  assert(!ok.has(key(0, 6))); // the head
  assert(!ok.has(key(0, 2)) && !ok.has(key(6, 4))); // shut in
  // One cell wide to nowhere: no way to turn back.
  assert(!K([[0, 0], [7, 6], [14, 2], [13, 4]]).some((c) => ok.has(c)));
  // A long snake: out of a two wide pocket behind a bridge, too.
  const long = new Set(room(b, walls, key(0, 6), 30));
  assert(!long.has(key(11, 1)) && !long.has(key(8, 0)) && long.has(key(19, 3)));
  // No start: the largest region. Shut in: all of it.
  assertEqual(room(b, walls, null, 10).sort(), [...ok].sort());
  const shut = new Set(room(b, walls, key(0, 2), 10));
  assert(shut.size === 20 && shut.has(key(6, 4)) && !shut.has(key(8, 0)));
  assertEqual(room(b, walls, key(0, 1), 10), []);
  const g = new Game(new Twister(1));
  for (let i = 0; i < 300; i++) {
    g.step(b, walls);
    if (g.apple !== null) assert(room(b, walls, null, 1).includes(g.apple));
  }
  assert(g.eaten >= 5);
  // The text shuts the apple in: it goes where the head can reach.
  const [x, y] = pair(g.apple);
  walls = new Set(walls);
  walls.delete(key(x, y));
  for (const dx of [-1, 0, 1]) for (const dy of [-1, 0, 1]) if (dx || dy) walls.add(key(x + dx, y + dy));
  const cut = new Set([...walls].filter((c) => !g.body.includes(c)));
  g.step(b, cut);
  const reach = [...room(b, cut, g.body[0], 1), ...g.body];
  assert(g.apple !== key(x, y) && reach.includes(g.apple));
  // Unchanged map: the apple stays.
  const [kept, eaten] = [g.apple, g.eaten];
  g.step(b, cut);
  assert(g.apple === kept || g.eaten > eaten);
}

export function test_snake_rules() {
  const rows = ["", "  wall of words  ", "", "   another   one", "", "", ""];
  const walls = wallsOf(rows, 10, 100);
  const b = box(10, 100, 40, 107);
  const g = new Game(new Twister(5), { golden: true, enemies: true, lasers: true });
  const seen = {};
  let golden = 0;
  for (let i = 0; i < 3000; i++) {
    const size = g.body.length;
    g.step(b, walls);
    const cells = g.body;
    assertEqual(new Set(cells).size, cells.length);
    assert(!cells.some((c) => walls.has(c)) && inside(b, cells) && joined(cells));
    const foes = g.foes.map((f) => f.at);
    assert(new Set(foes).size === foes.length && foes.length <= FOES);
    assert(!foes.some((c) => walls.has(c) || cells.includes(c)) && inside(b, foes));
    assert(g.beams.length <= BEAMS);
    for (const one of g.beams) {
      assert(one.cells.length >= 5 && !one.cells.some((c) => walls.has(c)));
      assertEqual(new Set(one.cells.map(one.across ? Y : X)).size, 1);
      assert(inside(b, one.cells));
      if (one.firing) assert(!one.cells.some((c) => cells.includes(c) || foes.includes(c))); // nothing stands in a beam that fires
    }
    assert(g.lumps.every((k) => k >= 0 && k < Math.max(1, cells.length)));
    for (const [what, ...data] of g.events) {
      seen[what] = (seen[what] ?? 0) + 1;
      if (what === "eat") {
        assertEqual(data[0], cells[0]);
        golden += data[1] ? 1 : 0;
      } else if (what === "lost") assert(data[0].length && cells.length < size + 1);
      else if (what === "zap") assert(data[0].firing && data[0].left === FIRE);
    }
  }
  assertEqual(Object.keys(seen).sort(), ["eat", "lost", "pop", "zap"]);
  assert(g.eaten === seen.eat && seen.eat >= 20 && golden > 0 && golden < seen.eat);
  // Same seed, same game; the flags off, the plain game.
  const [a, a2] = [0, 1].map(() => new Game(new Twister(9), { lasers: true }));
  const [plain, old] = [0, 1].map(() => new Game(new Twister(9)));
  for (let i = 0; i < 300; i++) {
    for (const one of [a, a2, plain, old]) one.step(b, walls);
    assertEqual(a.body, a2.body);
    assertEqual(a.beams.map((x) => x.cells), a2.beams.map((x) => x.cells));
    assertEqual(plain.body, old.body);
    assert(!plain.foes.length && !plain.beams.length);
    assert(plain.events.every((e) => e[0] === "eat" || e[0] === "lost"));
  }
}

export function test_snake_beam_and_bite() {
  const b = box(0, 0, 20, 6);
  let g = new Game(new Twister(1), { lasers: true });
  g.body = Array.from({ length: 6 }, (_, i) => key(8 - i, 2));
  [g.apple, g.beamDue] = [key(19, 5), 1e9];
  g.beams = [new Beam(1, Array.from({ length: 6 }, (_, y) => key(5, y)), false, 1)];
  g.step(b, new Set());
  // The beam fires on the fourth segment: the tail is lost from there.
  assertEqual(g.events.map((e) => e[0]), ["zap", "lost"]);
  assertEqual(pair(g.events[1][1][0]), [5, 2]);
  assert(!g.body.includes(key(5, 2)));
  assertEqual(P(g.body), [[9, 2], [8, 2], [7, 2], [6, 2]]);
  // Announced beams are walls to the snake when there is a way around.
  g = new Game(new Twister(1), { lasers: true });
  [g.body, g.apple, g.beamDue] = [[key(2, 2)], key(8, 2), 1e9];
  g.beams = [new Beam(1, Array.from({ length: 5 }, (_, y) => key(5, y + 1)), false)];
  for (let i = 0; i < FUSE - 1; i++) {
    g.step(b, new Set());
    assert(X(g.body[0]) !== 5 || Y(g.body[0]) === 0);
  }
  // A foe reaches the middle of the body and bites it off.
  g = new Game(new Twister(1), { enemies: true });
  g.body = Array.from({ length: 8 }, (_, i) => key(12 - i, 2));
  [g.apple, g.foeDue, g.steps] = [key(19, 5), 1e9, 1];
  g.foes = [new Foe(1, key(9, 3))];
  g.step(b, new Set());
  assertEqual(g.events.map((e) => e[0]), ["lost", "pop"]);
  assert(!g.foes.length && g.body.length < 8);
}

export function test_snake_arcade_art() {
  const rng = new Twister(2);
  const one = spark(rng, [1, 2]);
  assert(one.loop === false && one.motion.ay > 0);
  assert(one.ttl_ms > one.frames.reduce((n, f) => n + f.ms, 0));
  assertEqual(burst(7, rng, [1]).length, 7);
  for (const across of [true, false]) {
    for (const firing of [true, false]) {
      for (const f of beam(6, across, firing).frames) {
        const rows = f.cells.map((row) => row.map((r) => r.t).join(""));
        assertEqual([rows.length, [...rows[0]].length], across ? [1, 6] : [6, 1]);
      }
    }
  }
  const g = new Game(new Twister(1), { golden: true });
  [g.body, g.lumps] = [Array.from({ length: 10 }, (_, i) => key(9 - i, 0)), [4]];
  const row = g.picture(true)[1][0].reverse(); // head first
  assertEqual(row.map((r) => r.t).join(""), "@oooOOoooo");
  assert(row[1].fg === BODY[0] && row[row.length - 1].fg === BODY[BODY.length - 1]);
}

/** A host of the test's own: `batch` answers ids, kept in `calls`. */
function host() {
  const calls = [];
  let id = 0;
  const call = (method, params) => {
    calls.push([method, params]);
    if (method !== "batch") return {};
    return { results: params.calls.map((c) => (c.method === "draw.put" ? { result: { id: ++id } } : { result: {} })) };
  };
  return { calls, call };
}

export function test_snake_stage() {
  // Foes, beams and particles in step with the game; the score with
  // `-e` or a hint; the snake over its particles.
  const { calls, call } = host();
  const stage = new Stage(new Twister(4), true, "←↑↓→  ctrl+t");
  const g = new Game(new Twister(4), { golden: true, enemies: true, lasers: true });
  const walls = wallsOf(["", "", "words in the way"]);
  const all = [];
  for (let i = 0; i < 400; i++) {
    g.step(box(0, 0, 30, 8), walls);
    const out = new Batch(call);
    stage.show(out, 0, g, g.events);
    out.send();
  }
  for (const [, p] of calls) all.push(...p.calls);
  const puts = all.filter((c) => c.method === "draw.put").map((c) => c.params);
  assert(puts.some((p) => p.z === 3) && puts.some((p) => p.z === 2) && puts.some((p) => p.z === -1));
  assert(puts.every((p) => (p.z ?? 0) < SNAKE_Z || p.z === 5));
  assert(stage.foes.size === g.foes.length && stage.beams.size === g.beams.length);
  const board = puts.filter((p) => p.z === 5);
  assertEqual(board.length, 1);
  assertEqual(board[0].content.cells[0][0].t, "←↑↓→  ctrl+t  ");
  assert(stage.best >= g.body.length);
}

export function test_snake_options() {
  const o = options({ all: true, speed: 30 });
  assert(o.effects && o.enemies && o.lasers && !o.grab && o.error === null);
  assertEqual(options({ speed: 0 }).error, "--speed must be positive");
  assertEqual(options({}).speed, 12);
  // The twin's step k at (k + 1) / speed seconds.
  assertEqual([0, 82, 83, 84, 1000].map((t) => due(t, 12)), [0, 0, 0, 1, 12]);
}

export function test_board_hides_what_passes_under() {
  // A score: opaque, top right in view cells, sent when it changes.
  const { calls, call } = host();
  const board = new Board();
  let out = new Batch(call);
  board.show(out, 3, tally(7, 12));
  out.send();
  const put = calls[0][1].calls[0].params;
  assertEqual(put.anchor, { at: [-1, 0], space: "view" });
  assert(put.content.opaque && put.frame === 3 && put.z === 5);
  out = new Batch(call);
  board.show(out, 3, tally(7, 12)); // the same: nothing sent
  assertEqual(out.calls, []);
  board.show(out, 3, tally(8, 12));
  assertEqual(out.calls.map((c) => c.method), ["draw.update"]);
}

export function test_twin_draws() {
  for (const [n, ranges, ints, choice, two, many, few, weighted, u, r] of DRAWS) {
    const t = new Twister(n);
    const got = [
      ranges.map(() => t.randrange(10, 30)),
      ints.map(() => t.randint(20, 50)),
      t.choice([0, 1, 2, 3, 4, 5, 6]),
      t.sample([true, false], 2),
      t.sample(Array.from({ length: 100 }, (_, i) => i), 40),
      t.sample(Array.from({ length: 12 }, (_, i) => i), 5),
      t.weighted(["a", "b", "c"], [5, 1, 3]),
      t.uniform(4.0, 14.0),
      t.random(),
    ];
    assertEqual(got, [ranges, ints, choice, two, many, few, weighted, u, r], `${n}`);
  }
}
