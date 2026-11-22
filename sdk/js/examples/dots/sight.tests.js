// SPDX-License-Identifier: GPL-3.0-or-later
// "ptyrc/sight" on a host of its own (_cli.Sight, test_apps.py).

import { assert, assertEqual, PtyrcError } from "ptyrc";
import { Sight } from "ptyrc/sight";
import { spare } from "./app.js";
import { Braille } from "ptyrc/canvas";

const FRAME = { id: 0, kind: "raw", state: "stable", usable: true, rect: [0, 0, 6, 3], origin: [0, 0] };

/** A host: the answers of `screen.shape`, the calls made. */
function host({ lines = ["ab cd", "#"], degree, cursor = null, info = FRAME } = {}) {
  const h = { calls: [], lines, degree, info };
  h.call = (method, params) => {
    h.calls.push(params === undefined ? [method] : [method, params]);
    if (method === "frame.get") return { frame: h.info };
    if (method === "screen.cursor") return cursor;
    if (method === "screen.shape" || method === "screen.text") {
      if (!h.info.usable) throw new PtyrcError({ code: "not-usable", message: "off screen" });
      const r = { lines: h.lines, size: [h.info.rect[2], h.info.rect[3]], rect: h.info.rect };
      if (h.degree) r.degree = h.degree;
      return r;
    }
    return {};
  };
  h.reads = () => h.calls.filter(([m]) => m.startsWith("screen.")).length;
  return h;
}

export function test_sight_reads_when_told() {
  const h = host();
  const sight = new Sight({ call: h.call, words: ["cd"] });
  assertEqual(h.calls[0], ["subscribe", { topics: ["frames", "text", "size", "move"] }]);
  assert(sight.look() && sight.changed);
  assertEqual(h.calls.at(-1), ["screen.shape", { frame: 0, words: ["cd"] }]);
  assertEqual(sight.rows, ["ab cd ", "#     ", "      "]);
  assertEqual([sight.size, sight.rect, sight.degree, sight.cursor], [[6, 3], [0, 0, 6, 3], null, null]);
  assert(sight.ink(0, 0) && !sight.ink(2, 0) && !sight.ink(9, 0) && !sight.ink(0, 9));
  // Nothing told: nothing read.
  assert(sight.look() && !sight.changed);
  assertEqual(h.reads(), 1);
  // A row of its frame changed: read again; another frame's: not.
  assertEqual(sight.event("text", { frame: 3, rows: [0], origin: [0, 0], rect: [0, 0, 6, 3] }), true);
  sight.look();
  assertEqual(h.reads(), 1);
  h.lines = ["", "", "x"];
  sight.event("text", { frame: 0, rows: [2], origin: [0, 0], rect: [0, 0, 6, 3] });
  assert(sight.look() && sight.changed);
  assertEqual(sight.rows, ["      ", "      ", "x     "]);
  // A size: read again, and the deck or the app hears it too.
  assertEqual(sight.event("size", { size: [6, 3] }), false);
  sight.look();
  assertEqual(h.reads(), 3);
  assertEqual(sight.event("widgets", {}), false);
}

export function test_sight_text_and_cursor() {
  const h = host({ lines: ["hello"], cursor: { x: 3, y: 1, visible: true, frame: 0, pos: [3, 1] } });
  const sight = new Sight({ call: h.call, text: true, cursor: true, words: ["x"] });
  assertEqual(h.calls[0][1].topics, ["frames", "text", "size", "move", "cursor"]);
  sight.look();
  assertEqual(h.calls.at(-1), ["screen.text", { frame: 0 }]);
  assertEqual([sight.rows[0], sight.cursor], ["hello ", [3, 1]]);
  // In another frame: by the rect.
  sight.event("cursor", { x: 4, y: 2, visible: true, frame: 7, pos: [0, 0] });
  sight.look();
  assertEqual(sight.cursor, [4, 2]);
  sight.event("cursor", { x: 4, y: 2, visible: false, frame: 0, pos: [4, 2] });
  sight.look();
  assertEqual(sight.cursor, null);
}

export function test_sight_cursor_in_view_cells() {
  // Its frame scrolled, the view at row 1: the frame cell less the origin.
  const info = { ...FRAME, id: 3, rect: [0, 1, 6, 3], origin: [0, 5] };
  const h = host({ info, cursor: { x: 3, y: 2, visible: true, frame: 3, pos: [3, 6] } });
  const sight = new Sight({ call: h.call, cursor: true, frame: 3 });
  sight.look();
  assertEqual(sight.cursor, [3, 1]);
  sight.event("cursor", { x: 4, y: 3, visible: true, frame: 9, pos: [0, 0] });
  sight.look();
  assertEqual(sight.cursor, [4, 2]);
}

export function test_sight_lowered() {
  // `shape` refused: blank rows of the size, no cursor.
  const h = host({ lines: ["", "", ""], degree: "none", cursor: { x: 0, y: 0, visible: false, frame: 0, pos: [0, 0] } });
  const sight = new Sight({ call: h.call, cursor: true });
  sight.look();
  assertEqual([sight.rows, sight.degree, sight.cursor], [["      ", "      ", "      "], "none", null]);
  // `words` held, not `shape`: the words alone.
  const w = host({ lines: ["   cd"], degree: "words" });
  const seen = new Sight({ call: w.call, words: ["cd"] });
  seen.look();
  assertEqual([seen.rows[0], seen.degree], ["   cd ", "words"]);
}

export function test_sight_out_of_sight() {
  const h = host();
  const sight = new Sight({ call: h.call });
  assert(sight.look());
  // The frame left the screen: no look, no read, until it is back.
  sight.event("frames", { frames: [{ ...FRAME, usable: false, state: "inactive" }] });
  h.info = { ...FRAME, usable: false };
  assert(!sight.look() && !sight.usable);
  h.info = { ...FRAME, rect: [0, 0, 4, 2] };
  sight.event("frames", { frames: [h.info] });
  assert(sight.look() && sight.changed);
  assertEqual(sight.size, [4, 2]);
  // Off screen in the middle of a read: false, read again after.
  h.info = { ...FRAME, usable: false };
  sight.touch();
  assert(!sight.look());
  // A move: the frame named is read.
  h.info = { ...FRAME, id: 2 };
  assertEqual(sight.event("move", { frame: 2 }), false);
  assert(sight.look());
  assertEqual([sight.frame, h.calls.at(-1)], [2, ["screen.shape", { frame: 2 }]]);
}

export function test_sight_moved_below_layout() {
  // Told 0 at `init` and by `move`: the raw frame read again.
  const h = host();
  const sight = new Sight({ call: h.call, frame: 0 });
  assert(sight.look());
  const n = h.reads();
  assertEqual(sight.event("move", { frame: 0 }), false);
  assert(sight.look() && sight.frame === 0);
  assertEqual([h.reads(), h.calls.at(-1)], [n + 1, ["screen.shape", { frame: 0 }]]);
}

export function test_spare() {
  const dots = new Braille(5, 3);
  dots.bits.fill(255);
  spare(dots, ["", "a"]);
  assertEqual(dots.rows(), ["  ⣿⣿⣿", "  ⣿⣿⣿", "  ⣿⣿⣿"]);
}
