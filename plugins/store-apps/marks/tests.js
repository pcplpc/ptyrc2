// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of marks: `ptyrc dev test --unit` (test_apps.py:
// test_marks_wanted, test_marks_hud).

import { assertEqual } from "ptyrc";
import { INKS, Marks, hud, patterns, wanted } from "./app.js";

const match = (y, shown = true, text = "ab") => ({
  frame: 1,
  x: 3,
  y,
  len: text.length,
  text,
  screen: shown ? [3, y - 100] : null,
});

export function test_wanted() {
  const found = [
    [1, 0, [match(90, false), match(100), match(109), match(140, false)]],
    [1, 1, [match(105, true, "xyz"), match(106, true, "")]],
  ];
  const [keys, counts] = wanted(found, { 1: [100, 10] });
  // In view first, then the nearest rows; an empty match is no mark.
  assertEqual(keys.map((k) => k[2]), [100, 109, 105, 90, 140]);
  assertEqual(keys[2], [1, 3, 105, 1, "xyz"]);
  assertEqual(counts, [[2, 4], [1, 1]]);
  assertEqual(wanted(found, { 1: [100, 10] }, 3)[0].map((k) => k[2]), [100, 109, 105]);
}

export function test_hud() {
  const rows = hud(["TODO", "a very long pattern indeed"], [[2, 2], [1, 12]]);
  const text = rows.map((row) => row.map((run) => run.t).join(""));
  assertEqual(text, ["  TODO                 2 ", "  a very long pat…  1/12 "]);
  assertEqual(rows[0][1].bg, INKS[0][0]);
  assertEqual(rows[1][1].bg !== rows[0][1].bg, true);
}

export function test_patterns() {
  assertEqual(patterns({ patterns: ["apple"], regex: ["p[ea]+r"] }), [
    { pattern: "apple", regex: false },
    { pattern: "p[ea]+r", regex: true },
  ]);
  assertEqual(patterns({ patterns: [], regex: [] }), []);
}

/** A driver of one frame: its finds, what is drawn, removed. */
function driver(matches) {
  const log = [];
  let id = 0;
  const one = (method, params) => {
    log.push([method, params]);
    if (method === "frame.get") return { frame: { id: 1, origin: [0, 100], rect: [0, 0, 80, 10] } };
    if (method === "screen.find") return { matches: matches[params.pattern] ?? [] };
    if (method === "draw.put") return { id: ++id };
    return {};
  };
  const call = (method, params) =>
    method === "batch"
      ? { results: params.calls.map((c) => ({ result: one(c.method, c.params) })) }
      : one(method, params);
  return { call, log };
}

export function test_look() {
  const found = { ab: [match(100), match(101)] };
  const d = driver(found);
  const marks = new Marks(d.call, [{ pattern: "ab", regex: false }], true, true);
  marks.look();
  const puts = d.log.filter(([m]) => m === "draw.put");
  assertEqual(puts.length, 3); // two tints and the display
  assertEqual(puts[0][1].blend, "tint");
  assertEqual(puts[0][1].anchor, { at: [3, 100], space: "frame" });
  assertEqual(d.log.find(([m]) => m === "screen.find")[1].ignore_case, true);
  assertEqual(d.log.find(([m]) => m === "frame.get")[1], { frame: 0 });
  // Nothing changed: nothing drawn. One match gone: its tint removed,
  // the display updated.
  d.log.length = 0;
  marks.look();
  assertEqual(d.log.filter(([m]) => m.startsWith("draw.")).length, 0);
  found.ab.pop();
  marks.look();
  assertEqual(d.log.filter(([m]) => m.startsWith("draw.")).map(([m, p]) => [m, p.id]), [
    ["draw.remove", 2],
    ["draw.update", 3],
  ]);
  // Removed by the driver with its row: forgotten, put again.
  marks.forget([1]);
  d.log.length = 0;
  marks.look();
  assertEqual(d.log.filter(([m]) => m === "draw.put").length, 1);
}
