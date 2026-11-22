// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of arcade.js (test_apps.py: test_snake_arcade_art,
// test_board_hides_what_passes_under; the walls of test_snake_game).

import { assert, assertEqual } from "ptyrc";
import { ACROSS, Batch, Board, X, Y, apple, beam, burst, debris, foe, key, pair, spark, tally, wallsOf } from "./arcade.js";
import { Twister } from "./twister.js";

export function test_arcade_keys() {
  const cells = [[0, 0], [-3, 7], [5, -40], [200, 100000], [5, -41]];
  for (const [x, y] of cells) assertEqual(pair(key(x, y)), [x, y]);
  const keys = cells.map(([x, y]) => key(x, y)).sort((a, b) => a - b);
  assertEqual(keys.map(pair), [[-3, 7], [0, 0], [5, -41], [5, -40], [200, 100000]]);
  assert(key(4, 2) + ACROSS === key(5, 2) && key(4, 2) + 1 === key(4, 3));
  assert(X(key(-1, -1)) === -1 && Y(key(-1, -1)) === -1);
}

export function test_arcade_walls() {
  const walls = wallsOf(["", "  wall of words  "], 10, 100);
  assert(walls.has(key(12, 101)) && !walls.has(key(10, 101)) && !walls.has(key(16, 101)));
  assertEqual([...wallsOf(["a世b"])].map(pair), [[0, 0], [1, 0], [2, 0], [3, 0]]);
  assertEqual(wallsOf(["  ", "\t"]).size, 0);
}

export function test_arcade_art() {
  const rng = new Twister(2);
  const one = spark(rng, [1, 2]);
  assert(one.loop === false && one.motion.ay > 0 && one.layer === undefined);
  assert(one.ttl_ms > one.frames.reduce((n, f) => n + f.ms, 0));
  assertEqual(one.frames.map((f) => f.cells[0][0].t).join(""), "*+'.");
  assert(one.frames[0].cells[0][0].bold && one.frames[3].cells[0][0].dim);
  assertEqual(burst(7, rng, [1]).length, 7);
  assertEqual(debris(rng).frames.length, 3);
  for (const across of [true, false]) {
    for (const firing of [true, false]) {
      for (const f of beam(6, across, firing).frames) {
        const rows = f.cells.map((row) => row.map((r) => r.t).join(""));
        assertEqual([rows.length, [...rows[0]].length], across ? [1, 6] : [6, 1]);
      }
    }
  }
  assertEqual(apple().frames.length, 2);
  assertEqual(apple(true).frames.length, 4);
  assertEqual(foe().z, 2);
  assertEqual(tally(3, 9, "keys").map((row) => row.map((r) => r.t).join("")), ["keys  ◆ 3  best 9"]);
}

export function test_arcade_board() {
  // Opaque, top right in view cells; sent when it changes.
  const calls = [];
  const call = (method, params) => {
    calls.push(params.calls);
    return { results: params.calls.map(() => ({ result: { id: 7 } })) };
  };
  const board = new Board();
  let out = new Batch(call);
  board.show(out, 2, tally(7, 12));
  out.send();
  const put = calls[0][0].params;
  assertEqual(put.anchor, { at: [-1, 0], space: "view" });
  assert(put.content.opaque && put.frame === 2 && put.z === 5 && board.sprite.id === 7);
  out = new Batch(call);
  board.show(out, 2, tally(7, 12));
  assertEqual(out.calls, []);
  board.show(out, 2, tally(8, 12));
  assertEqual(out.calls.map((c) => [c.method, c.params.id]), [["draw.update", 7]]);
  out.remove(board.sprite);
  assertEqual(out.calls.length, 2);
  out.update(board.sprite, {}); // gone: dropped
  assertEqual(out.calls.length, 2);
}
