// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of ask, `ptyrc dev test --unit`: those of the twin
// (test_ask.py) on the pure parts, and the twin's looks (`twin.js`).

import { assert, assertEqual, assertThrows } from "ptyrc";
import { model, options } from "./app.js";
import { Choice, DONE, Line, Text, caret } from "./ui.js";
import { CASES, VIEWS } from "./twin.js";

/** Rows of runs as the twin's script writes them: runs of one style
 * merged, empty runs out, `[t, fg, bold, dim, inverse]`. */
function norm(rows) {
  return rows.map((row) => {
    const out = [];
    for (const r of row) {
      const t = String(r.t);
      if (!t) continue;
      const style = [r.fg ?? null, !!r.bold, !!r.dim, !!r.inverse];
      const last = out[out.length - 1];
      if (last && JSON.stringify(last.slice(1)) === JSON.stringify(style)) last[0] += t;
      else out.push([t, ...style]);
    }
    return out;
  });
}

function fnv(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

const args = (words, rest = {}) => ({ words, multi: false, other: false, text: false, ...rest });

export function test_twin_looks() {
  for (const c of CASES) {
    const m = model(c.o);
    assertEqual(m.sizes(), c.sizes, `${c.argv}: sizes`);
    assertEqual(norm(m.render(...c.sizes[0], false)), c.whole, `${c.argv}: the first render`);
    const views = [...c.sizes, ...VIEWS];
    for (const [step, flow, hashes] of c.steps) {
      let got = null;
      if (Array.isArray(step)) m.paste(step[1]);
      else if (step !== null) got = m.key(step);
      assertEqual(got, flow, `${c.argv}: ${step}: flow`);
      let k = 0;
      for (const [w, h] of views)
        for (const focused of [true, false]) {
          const rows = m.render(w, h, focused);
          const hash = fnv(JSON.stringify(norm(rows)));
          assert(hash === hashes[k], `${c.argv}: after ${step}: ${w}x${h} focused ${focused}: ${JSON.stringify(norm(rows))}`);
          k++;
        }
    }
    assertEqual(m.result, c.result, `${c.argv}: result`);
  }
}

// test_model
export function test_model() {
  const choice = model(options(args(["Deploy?", "yes", "no"], { default: "no" })));
  assert(choice instanceof Choice && choice.question === "Deploy?");
  assertEqual(choice.options, ["yes", "no"]);
  assertEqual(choice.listing.sel, 1);
  assert(!choice.multi && choice.other === null);
  assertEqual(choice.key("enter"), DONE);
  assertEqual(choice.result, "no");
  const some = model(options(args(["Which?", "a", "b"], { multi: true, other: true, default: "b" })));
  assert(some instanceof Choice && some.multi && some.other !== null);
  assertEqual(some.key("enter"), DONE);
  assertEqual(some.result, ["b"]);
  const line = model(options(args(["Name?"], { default: "you" })));
  assert(line instanceof Line && line.field.text === "you");
  const text = model(options(args(["Why?"], { text: true, default: "a\nb" })));
  assert(text instanceof Text && text.text === "a\nb");
  // Options given: a choice, `--text` or not.
  assert(model(options(args(["Q", "a"], { text: true }))) instanceof Choice);
}

// test_options
export function test_options() {
  const o = options(args(["Q", "a"], { timeout: 2.5, title: "t" }));
  assertEqual([o.timeout, o.title, o.question, o.choices], [2.5, "t", "Q", ["a"]]);
  const bare = options(args(["Q"]));
  assertEqual([bare.timeout, bare.title, bare.default], [null, null, ""]);
  for (const t of [0, -1, Infinity, NaN]) assertThrows(() => options(args(["Q"], { timeout: t })), RangeError);
  assertThrows(() => options(args([])), RangeError);
}

export function test_caret() {
  assertEqual(caret("ctrl+x"), "^X");
  assertEqual(caret("esc"), "esc");
}
