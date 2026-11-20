// SPDX-License-Identifier: GPL-3.0-or-later
// ptyrc dev test --unit
import { assertEqual } from "ptyrc";
import { clock, face } from "./face.js";

export function test_clock() {
  assertEqual(clock(0), "00:00");
  assertEqual(clock(61999), "01:01");
  assertEqual(clock(3600000), "60:00");
}

export function test_line_face() {
  assertEqual(face(5000, 5, 1, false), [[{ t: "00:05", fg: 14, bold: true }]]);
}

export function test_big_face() {
  const rows = face(72000, /* 01:12 */ 17, 3, false).runs();
  const text = rows.map((row) => row.map((r) => r.t).join(""));
  assertEqual(text, ["█▀█  █     █  ▀▀█", "█ █  █  ▪  █  █▀▀", "▀▀▀  ▀     ▀  ▀▀▀"]);
  assertEqual(rows[0][0], { t: "█▀█", fg: 14, bold: true });
}

// ---- the deck on a host of its own: `new Deck({call})` ----

import { assert as ok } from "ptyrc";
import { Deck } from "ptyrc/deck";

/** A host that places every widget at `rect`, its inner box `size`. */
function host(rect = [10, 2, 10, 3], size = [8, 1]) {
  const h = { calls: [] };
  h.call = (method, params) => {
    h.calls.push([method, params]);
    if (method === "widget.show")
      return { widget: { id: 1, tag: params.tag, state: "placed", variant: "8x1", size, rect, sprite: 5, variants: [{ name: "8x1", size }] } };
    if (method === "batch") return { results: params.calls.map(() => ({ result: {} })) };
    return {};
  };
  h.sets = () => h.calls.filter(([m]) => m === "widget.set").length;
  return h;
}

const at = (action, x, y, button = "left") => ({ sprite: 5, action, button, x, y, at: [x - 10, y - 2], mods: [] });

export function test_deck_click() {
  const h = host();
  const deck = new Deck({ call: h.call });
  const clicks = [];
  let n = 0;
  deck.add("n", [[8, 1]], () => [String(n)], {
    click: (x, y, button, w) => {
      clicks.push([x, y, button, w.tag]);
      if (button === "right") return false;
      n++;
    },
  });
  assertEqual(h.calls[0], ["subscribe", { topics: ["widgets", "move"] }]);
  assertEqual(h.calls[1], ["subscribe", { topics: ["mouse"] }]);
  // The inner box: the border is not in it.
  ok(deck.event("mouse", at("press", 11, 3)));
  deck.event("mouse", at("press", 18, 3));
  deck.event("mouse", at("press", 10, 3));
  deck.event("mouse", at("press", 19, 3));
  deck.event("mouse", at("press", 11, 2));
  deck.event("mouse", at("release", 11, 3));
  deck.event("mouse", at("drag", 12, 3));
  deck.event("mouse", at("wheel", 12, 3, "wheel-up"));
  deck.event("mouse", { ...at("press", 11, 3), sprite: 6 });
  assertEqual(clicks, [
    [0, 0, "left", "n"],
    [7, 0, "left", "n"],
    [1, 0, "wheel-up", "n"],
  ]);
  // Drawn again after each, but when the handler says nothing changed.
  assertEqual(h.sets(), 3);
  deck.event("mouse", at("press", 13, 3, "right"));
  assertEqual(h.sets(), 3);
  assertEqual(h.calls.filter(([m]) => m === "widget.set").at(-1)[1].content, { rows: ["3"] });
}

export function test_deck_hover_and_leave() {
  const h = host([0, 0, 6, 4], [4, 2]);
  let seen = [];
  const deck = new Deck({ call: h.call, hoverTimeout: 1000 });
  deck.add("h", [[4, 2]], () => ["", ""], {
    hover: (x, y) => void seen.push([x, y]),
    leave: () => void seen.push("leave"),
  });
  deck.event("hover", { sprite: 5, x: 1, y: 1 });
  deck.event("hover", { sprite: 5, x: 4, y: 2 });
  deck.event("hover", { sprite: 5, x: 5, y: 2 }); // the border
  assertEqual(seen, [[0, 0], [3, 1], "leave"]);
  seen = [];
  deck.event("hover", { sprite: 5, x: 2, y: 1 });
  deck.tick(500);
  deck.tick(1000);
  assertEqual(seen, [[1, 0], "leave"]);
  ok(!deck.event("size", {}));
}
