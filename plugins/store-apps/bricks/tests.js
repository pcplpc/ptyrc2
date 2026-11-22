// SPDX-License-Identifier: GPL-3.0-or-later
// Unit tests of bricks: the twin's games step for step (`twin.js`),
// then the twin's tests of `test_bricks.py` and `test_bricks_look.py`.

import { assert, assertEqual } from "ptyrc";
import { Batch, X, Y, key, wallsOf } from "./arcade.js";
import { FLAME, PALETTE, Stage, TEAM, ball, board, brick, faller, lock, options, pop, racket, shards, splash } from "./app.js";
import { BALLS, Ball, Game, KINDS, POINTS, RACKET, SMALL, WIDE, fly, pyRound } from "./game.js";
import { Twister } from "./twister.js";
import { GAMES } from "./twin.js";

const F = 2 ** 30;
const q = (v) => String(Math.floor(v * F));

/** The game after a step, as `twin.py` writes it. */
function state(g) {
  const balls = g.balls.map((b) => `${b.n}:${q(b.x)},${q(b.y)},${q(b.vx)},${q(b.vy)},${b.owner},${b.fire},${b.fast},${b.combo},${b.phase}`).join(";");
  const rackets = g.rackets.map((r) => `${r.n}:${r.x},${r.y},${r.w},${r.dead},${r.span}`).join(";");
  const laid = [...g.rows.keys()]
    .sort((a, b) => a - b)
    .flatMap((y) => g.rows.get(y))
    .map((b) => `${b.n}:${b.x},${b.y},${b.kind},${b.lives},${b.color},${b.due}`)
    .join(";");
  const caps = g.capsules.map((c) => `${c.n}:${c.x},${c.y},${c.kind}`).join(";");
  const falls = g.fallers.map((f) => `${f.n}:${f.x},${f.y}`).join(";");
  const events = g.events.map((e) => e[0]).join(";");
  return [balls, rackets, laid, caps, falls, events, g.score.join(",")].join("|");
}

function fnv(h, text) {
  // The text is ASCII: its bytes are its codes.
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619) >>> 0;
  return h;
}


function twin(n, cap = Infinity) {
  {
    const [rows, x0, y0, b, seed, flags, steps, presses, marks, last] = GAMES[n];
    const walls = wallsOf(rows, x0, y0);
    const g = new Game(new Twister(seed), flags);
    let h = 2166136261;
    const got = [];
    for (let i = 0; i < Math.min(steps, cap); i++) {
      if (presses[i]) g.press(presses[i]);
      g.step(b, walls);
      h = fnv(h, state(g));
      if ((i + 1) % 100 === 0 || i + 1 === steps) {
        got.push(h);
        if (h !== marks[got.length - 1]) assertEqual(state(g), "", `seed ${seed} ${JSON.stringify(flags)}: off by step ${i + 1}`);
      }
    }
    assertEqual(got, marks, `seed ${seed}`);
    assertEqual(state(g), last, `seed ${seed}`);
  }
}

export const test_twin_plain = () => twin(0);
export const test_twin_rules_bonuses = () => twin(1);
export const test_twin_two = () => twin(2);
export const test_twin_two_rules_bonuses = () => twin(3);
export const test_twin_player = () => twin(4);
export const test_twin_two_player = () => twin(5);

export function test_round() {
  assertEqual([0.5, 1.5, 2.5, 3.5, 2.4, 2.6].map(pyRound), [0, 2, 2, 4, 2, 3]);
}

const ROWS = ["", "  wall of words  ", "", "", "", "", "   another   one", ...Array(13).fill("")];
const BOX = [10, 100, 70, 120];
const WALLS = wallsOf(ROWS, 10, 100);

/** What holds after every step (the twin's `checks`). */
function checks(game, box, walls) {
  const [x0, y0, x1, y1] = box;
  const cells = game.bricks(box).flatMap((b) => b.cells);
  assert(new Set(cells).size === cells.length && !cells.some((c) => walls.has(c)));
  assert(cells.every((c) => game.solid.get(c).y === Y(c)));
  const rackets = game.rackets.filter((r) => !r.dead);
  const taken = rackets.flatMap((r) => r.cells);
  assert(new Set(taken).size === taken.length, "never one on another");
  assert(!taken.some((c) => cells.includes(c)), "nor on a brick");
  assert(taken.every((c) => x0 <= X(c) && X(c) < x1 && y0 <= Y(c) && Y(c) < y1));
  assert(rackets.every((r) => [SMALL, RACKET, WIDE].includes(r.w)));
  assert(game.balls.length <= BALLS);
  for (const b of game.balls) {
    assert(x0 <= X(b.at) && X(b.at) < x1 && y0 <= Y(b.at) && Y(b.at) < y1);
    assert(!walls.has(b.at) && !taken.includes(b.at));
    assert(b.fire || !game.solid.has(b.at));
  }
}

export function test_fly() {
  const hard = (c) => c === key(3, 0) || Y(c) === 2;
  assertEqual(fly(0.5, 0.5, 0.5, 0.5, hard).slice(0, 4), [1.0, 1.0, 0.5, 0.5]);
  assertEqual(fly(2.5, 0.25, 0.9, 0.5, hard), [2.5, 0.75, -0.9, 0.5, [[key(3, 0), false]]]);
  assertEqual(fly(0.5, 1.75, 0.5, 0.5, hard), [1.0, 1.75, 0.5, -0.5, [[key(1, 2), true]]]);
}

export function test_game() {
  const game = new Game(new Twister(3));
  const seen = {};
  for (let i = 0; i < 1500; i++) {
    const score = game.score[0];
    game.step(BOX, WALLS);
    checks(game, BOX, WALLS);
    assert(game.rackets.length === 1 && !game.fallers.length && !game.capsules.length);
    assert(game.bricks(BOX).every((b) => b.kind === "plain" && b.lives === 1));
    const whats = game.events.map((e) => e[0]);
    for (const w of whats) seen[w] = (seen[w] ?? 0) + 1;
    if (whats.includes("lost")) assertEqual(game.score, [0]);
    else {
      const won = game.score[0] - score;
      const n = whats.filter((w) => w === "break").length;
      assert(POINTS * n <= won && won <= 2 * POINTS * n);
    }
    assert(!whats.includes("combo") || whats.includes("break"));
  }
  for (const w of ["group", "break", "serve", "bounce", "combo"]) assert(seen[w], w);
  assert(seen.break >= 40 && game.best[0] >= 100);
  assert(!seen.reset, "bricks are broken: no need to start again");
}

export function test_bricks_options() {
  assertEqual(options({}), { speed: 16.0, rules: false, bonuses: false, multiplayer: false, effects: false, grab: false, error: null });
  const all = options({ all: true });
  assert(all.rules && all.bonuses && all.multiplayer && all.effects && !all.grab);
  assert(options({ speed: 0 }).error);
}

const text = (look) => (look.content ? [look.content.cells[0][0].t] : look.frames.map((f) => f.cells[0][0].t));

export function test_glyphs() {
  const seen = new Set();
  for (const [kind, [lives]] of Object.entries(KINDS)) {
    for (let left = 1; left <= lives; left++) {
      for (const one of text(brick(kind, left, 2))) {
        const g = [...one];
        assert(g.length === 3 && ["⟦⟧", "⟨⟩"].includes(g[0] + g[2]));
        seen.add(one);
      }
    }
  }
  assertEqual(text(brick("plain", 1, 0)), ["⟦=⟧"]);
  assertEqual(text(brick("loose", 1, 0)), ["⟨=⟩"]);
  assertEqual([3, 2, 1].map((n) => [...text(brick("hard", n, 0))[0]][1]), ["≡", "=", "-"]);
  assert(!PALETTE.includes(brick("hard", 2, 0).content.cells[0][0].fg));
  assertEqual(seen.size, 8);
  assert(text(faller(1)).every((t) => t[0] + t[2] === "⟨⟩"));
  for (const w of [3, 5, 7]) {
    const [calm, hit] = [false, true].map((h) => racket(w, 0, h)[0][0]);
    assert(calm.t === `(${"━".repeat(w - 2)})` && calm.fg === TEAM[0]);
    assert(hit.t === `(${"═".repeat(w - 2)})` && hit.fg !== calm.fg);
  }
  assertEqual(text(ball()), ["▀"]);
  assertEqual(text(ball(null, false, true)), ["▄"]);
  assertEqual(new Set([0, 1, 2].map((i) => ball(null, true, false, i).content.cells[0][0].fg)), new Set(FLAME));
  for (const look of [pop("plain", 0), lock(), splash()]) assert(look.loop === false && new Set(text(look).map((t) => [...t].length)).size === 1);
  assertEqual(shards(new Twister(1), "loose", 0).map((g) => g.frames[0].cells[0][0].t), ["⟨", "=", "⟩"]);
}

/** A stage on a recording `call`: the batches it sends. */
function staged(effects = false) {
  const sent = [];
  let id = 0;
  const call = (method, params) => {
    sent.push(params.calls);
    return { results: params.calls.map(() => ({ result: { id: ++id } })) };
  };
  return [new Stage(new Twister(1), effects), call, sent];
}

export function test_ball_by_halves() {
  // Half a row a step: the ball changes at each step.
  const [st, call] = staged();
  const g = new Game(new Twister(1));
  const it = new Ball(9, 5.5, 3.5, 0.9, 0.5);
  g.balls = [it];
  g.rackets = [];
  const shown = [];
  for (let i = 0; i < 4; i++) {
    const out = new Batch(call);
    st.show(out, 0, g, [0, 0, 40, 10]);
    out.send();
    const [, [at]] = st.moving.get(9);
    shown.push([at, st.late.size]);
    it.x += 0.9;
    it.y += 0.5;
  }
  assertEqual(shown, [[[5, 3], 0], [[6, 4], 0], [[7, 4], 0], [[8, 5], 0]]);
  g.balls = [];
  st.show(new Batch(call), 0, g, [0, 0, 40, 10]);
  assert(!st.moving.size);
}

export function test_board() {
  assertEqual(board([12])[0][0].t, "◆ 12");
  assertEqual(board([1, 2], [3, 4], "keys")[0].map((r) => r.t), ["keys  ", "▲ 1", "  ", "▼ 2", "  best 3 4"]);
}
