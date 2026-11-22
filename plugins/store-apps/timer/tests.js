// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of timer, `ptyrc dev test --unit`: those of the twin
// (test_timer.py) on the pure parts, and the twin's faces (`twin.js`);
// digits.js's: `_shared/js/digits.tests.js`.

import { assert, assertEqual, assertThrows, cells } from "ptyrc";
import {
  DIM,
  GLASSES,
  Run,
  TURN,
  bar,
  bulbs,
  duration,
  face,
  fallen,
  glass,
  grains,
  label,
  ladder,
  look,
  options,
  plan,
  pomodoro,
  sand,
  share,
  short,
  spin,
  stamp,
  state,
  title,
  tone,
  until,
  wait,
} from "./app.js";
import { LARGE, SMALL, decode } from "./digits.js";
import { BULBS, FACES, LOOKS, SPINS, STATES, WHOLE } from "./twin.js";

const PLAIN = look();
const FANCY = look({ glass: true, colour: true });
const ALL_LOOKS = [false, true].flatMap((g) => [false, true].map((c) => look({ glass: g, colour: c })));

const text = (rows, w = 0) =>
  rows.map((row) => {
    const t = row.map((r) => r.t).join("");
    return t + " ".repeat(Math.max(0, w - cells(t)));
  });

/** Braille dots of a string. */
function dots(s) {
  let n = 0;
  for (const c of s) {
    const v = c.codePointAt(0) - 0x2800;
    if (v >= 0 && v <= 0xff) for (let b = v; b; b >>= 1) n += b & 1;
  }
  return n;
}

const near = (a, b, eps = 1e-9) => Math.abs(a - b) <= eps;

export function test_duration() {
  const cases = [
    ["25m", 1500],
    ["90s", 90],
    ["1h30m", 5400],
    ["1h", 3600],
    ["1.5h", 5400],
    ["2m30s", 150],
    ["1:30", 90],
    ["1:30:00", 5400],
    ["25", 1500],
    ["0.5", 30],
    [" 5M ", 300],
  ];
  for (const [t, s] of cases) assertEqual(duration(t), s, t);
}

export function test_not_a_duration() {
  for (const t of ["", "m", "0", "0s", "-5m", "5x", "1:3", "m5", "1h1h"]) {
    const e = assertThrows(() => duration(t), RangeError, t);
    assert(e.message.startsWith("not a duration"), e.message);
  }
}

export function test_until() {
  const day = 17 * 3600 + 30; // 17:00:30
  assertEqual(until("17:30", day), 29 * 60 + 30);
  assertEqual(until("17:00", day), 24 * 3600 - 30);
  assertEqual(until("9:05", day), (16 * 60 + 5) * 60 - 30);
  assert(assertThrows(() => until("25:00", day), RangeError).message.includes("HH:MM"));
}

export function test_plan() {
  assertEqual(pomodoro("25/5"), [1500, 300]);
  assertEqual(pomodoro("50m/10m"), [3000, 600]);
  assertThrows(() => pomodoro("25"), RangeError);
  assertEqual(plan(90), [{ name: "timer", length: 90 }]);
  const names = (r) => plan(null, 1500, 300, r).map((p) => p.name);
  assertEqual(names(3), ["work", "break", "work", "break", "work"]);
  assertEqual(names(1), ["work"]);
  assertEqual(names(0), ["work", "break"]);
}

export function test_stamp() {
  assertEqual(stamp(1500), "25:00");
  assertEqual(stamp(1499.2), "25:00"); // a second begun counts
  assertEqual(stamp(0.01), "00:01");
  assertEqual([stamp(0), stamp(-3)], ["00:00", "00:00"]);
  assertEqual(stamp(5999), "99:59");
  assertEqual(stamp(6000), "1:40:00");
  assertEqual(
    [0, 45, 60, 61, 1500, 3600, 5400].map(short),
    ["0s", "45s", "1m", "2m", "25m", "1h", "1.5h"],
  );
}

export function test_run() {
  const run = new Run(plan(100), 10);
  const s = run.state(10);
  assertEqual([s.left, share(s), s.done, s.rounds], [100, 0, false, 0]);
  assertEqual(share(run.state(35)), 0.25);
  assertEqual(run.toggle(35), true);
  assert(run.state(60).left === 75 && run.state(60).paused);
  assertEqual(run.toggle(60), false);
  assertEqual(run.state(134.9).done, false);
  const done = run.state(140);
  assert(done.done && done.left === 0 && share(done) === 1);
  assert(near(done.over, 5));
}

export function test_run_speed() {
  const run = new Run(plan(1500), 0, 60);
  assertEqual(run.state(10).left, 900);
  assert(run.state(25).done);
}

export function test_run_pomodoro() {
  const run = new Run(plan(null, 50, 10, 2));
  const first = run.state(0);
  assertEqual([first.name, first.round, first.rounds, first.turn], ["work", 1, 2, null]);
  const rest = run.state(50 + TURN / 2);
  assertEqual([rest.name, rest.index, rest.round], ["break", 1, 1]);
  assert(near(rest.turn, 0.5));
  assertEqual(run.state(55).turn, null);
  const last = run.state(65);
  assertEqual([last.name, last.index, last.round, last.left], ["work", 2, 2, 45]);
  assert(run.state(111).done);
}

export function test_run_loop() {
  const run = new Run(plan(null, 50, 10, 0), 0, 1, true);
  const s = run.state(60 * 7 + 55);
  assertEqual([s.name, s.index, s.round, s.rounds], ["break", 15, 8, 0]);
  assert(!s.done);
}

export function test_bulbs() {
  for (let k = 2; k < 7; k++) {
    const [upper, lower] = bulbs(k);
    const n = grains(k);
    const keys = (p) => new Set(p.map(([x, y]) => `${x},${y}`));
    assert(n === upper.length && keys(upper).size === n && keys(lower).size === lower.length && lower.length >= n);
    const [w, h] = [2 * k + 1, 2 * k + 2];
    for (const [x, y] of upper) {
      const i = Math.floor(y / 4) - 1;
      assert(i >= 0 && i < k && i + 1 <= x >> 1 && x >> 1 <= 2 * k - 1 - i, `${k}: ${x},${y}`);
    }
    for (const [x, y] of lower) {
      const j = Math.floor(y / 4) - k - 1;
      assert(j >= 0 && j < k && k - j <= x >> 1 && x >> 1 <= k + j, `${k}: ${x},${y}`);
    }
    assert([...upper, ...lower].every(([x, y]) => x >= 0 && x < 2 * w && y >= 0 && y < 4 * h));
    // The last grain to leave is at the neck; the first to land, on the floor.
    assertEqual(upper[upper.length - 1][1], 4 * (k + 1) - 1);
    assertEqual(lower[0][1], 4 * (2 * k + 1) - 1);
    // Mirror images: as much sand on each side of the middle.
    for (const part of [upper, lower]) {
      const mirrored = keys(part.map(([x, y]) => [4 * k + 1 - x, y]));
      assert([...keys(part)].every((d) => mirrored.has(d)), `${k}`);
    }
  }
}

export function test_sand_is_counted() {
  for (let k = 2; k < 7; k++) {
    const n = grains(k);
    let before = null;
    for (let fell = 0; fell <= n; fell++) {
      const [upper, lower] = sand(k, fell);
      assertEqual([upper.length, lower.length], [n - fell, fell]);
      if (before !== null) {
        // One grain left, one landed, none moved.
        assertEqual(before[0].slice(1), upper);
        assertEqual(lower.slice(0, -1), before[1]);
      }
      before = [upper, lower];
    }
  }
}

export function test_the_glass_empties_on_time() {
  for (let k = 2; k < 7; k++) {
    for (const [length, speed] of [
      [60, 1],
      [1500, 1],
      [1500, 60],
      [7, 0.5],
    ]) {
      const n = grains(k);
      let run = new Run(plan(length), 0, speed);
      const end = length / speed;
      const seen = [];
      for (let step = 0; step <= 400; step++) {
        const s = run.state((end * step) / 400);
        const fell = fallen(k, share(s));
        seen.push(fell);
        // A grain every `length / n` seconds.
        assert(fell === Math.min(n, Math.floor(n * share(s) + 1e-9)) || (fell === n - 1 && !s.done), `${k} ${length} ${step}`);
      }
      assertEqual(seen, [...seen].sort((a, b) => a - b));
      assertEqual(seen[0], 0);
      assert(seen[400] === n && seen[399] < n); // the last one as the time ends
      run = new Run(plan(length), 0, speed);
      assertEqual(fallen(k, share(run.state(end * (1 - 1e-6)))), n - 1);
      assertEqual(fallen(k, share(run.state(end))), n);
    }
  }
}

export function test_glass_drawn() {
  for (let k = 2; k < 7; k++) {
    const [w, h, n] = [2 * k + 1, 2 * k + 2, grains(k)];
    for (const part of [0, 0.3, 0.5, 0.99, 1]) {
      const rows = text(glass(k, state({ length: 100, left: 100 * (1 - part), paused: true }), PLAIN), w);
      assert(rows.length === h && rows.every((r) => cells(r) === w));
      assertEqual(rows[0], "▄".repeat(w));
      assertEqual(rows[h - 1], "▀".repeat(w));
      assert(rows[1][0] === "╲" && rows[1][w - 1] === "╱");
      assert(rows[k].trim()[0] === "╲" && rows[k + 1].trim()[0] === "╱");
      const fell = fallen(k, part);
      assertEqual(dots(rows.slice(0, k + 1).join("")), n - fell);
      assertEqual(dots(rows.slice(k + 1).join("")), fell);
    }
    // Running: grains in the air, between the neck and the heap only.
    const s = state({ length: 100, left: 60, beat: 0.3 });
    const rows = text(glass(k, s, PLAIN), w);
    const fell = fallen(k, 0.4);
    assertEqual(dots(rows.slice(0, k + 1).join("")), n - fell);
    assert(dots(rows.slice(k + 1).join("")) > fell);
    const other = text(glass(k, { ...s, beat: 0.4 }, PLAIN), w);
    assert(JSON.stringify(other) !== JSON.stringify(rows));
    assertEqual(other.slice(0, k + 1), rows.slice(0, k + 1));
  }
}

export function test_spin() {
  for (let k = 2; k < 7; k++) {
    const [w, h] = [2 * k + 1, 2 * k + 2];
    const frames = Array.from({ length: 11 }, (_, t) => text(spin(k, t / 10, PLAIN), w));
    for (const rows of frames) assert(rows.length === h && rows.every((r) => cells(r) === w));
    const [start, end] = [frames[0], frames[10]];
    // Half a turn: the sand that lay in the lower bulb is in the upper.
    assert(dots(start.slice(h / 2).join("")) > 1.5 * dots(start.slice(0, h / 2).join("")));
    assert(dots(end.slice(0, h / 2).join("")) > 1.5 * dots(end.slice(h / 2).join("")));
    assertEqual(new Set(frames.map((f) => JSON.stringify(f))).size, 11);
    const s = state({ length: 10, left: 10, index: 1, turn: 0.5 });
    assertEqual(text(glass(k, s, PLAIN), w), frames[5]);
  }
}

const CASES = [
  state({ length: 1500, left: 1500 }),
  state({ length: 1500, left: 612.4, beat: 3.7 }),
  state({ length: 1500, left: 0.4, beat: 8.2 }),
  state({ length: 1500, left: 700, paused: true, beat: 1.6 }),
  state({ length: 1500, left: 0, done: true, over: 75, beat: 2.6 }),
  state({ name: "break", length: 300, left: 290, index: 3, round: 2, rounds: 4, turn: 0.4 }),
  state({ name: "work", length: 20000, left: 17000, index: 2, round: 2 }),
];

export function test_bounds() {
  for (const l of ALL_LOOKS) {
    const faces = ladder(l);
    assert(faces.length <= 16);
    const order = faces.map(([[w, h]]) => w * h);
    assertEqual(order, [...order].sort((a, b) => a - b));
    for (const s of CASES) {
      for (const [[w, h]] of faces) {
        for (const hover of [false, true]) {
          const rows = face(s, w, h, l, hover);
          assert(rows.length > 0 && rows.length <= h, `${w}x${h}`);
          for (const t of text(rows)) {
            assert(cells(t) <= w, `${w}x${h}: ${t}`);
            assert(Array.from(t).every((c) => cells(c) === 1 && !/\p{C}/u.test(c)), t);
          }
          if (!l.colour) {
            const ok = [undefined, 7, 3, 11, DIM];
            assert(rows.every((row) => row.every((r) => ok.includes(r.fg))), `${w}x${h}`);
          }
        }
      }
    }
  }
}

export function test_faces() {
  const s = state({ length: 1500, left: 612.4 });
  assertEqual(text(face(s, 5, 1, PLAIN)), ["10:13"]);
  assertEqual(text(face(s, 12, 1, PLAIN)), ["10:13 ━━╸───"]);
  assertEqual(text(face(s, 20, 2, PLAIN)), ["10:13       of 25:00", "━━━━━━━━────────────"]);
  for (const [[w, h], font] of [
    [[17, 3], SMALL],
    [[32, 5], LARGE],
  ]) {
    assertEqual(decode(text(face(s, w, h, PLAIN), w), font), "10:13");
  }
  const rows = text(face(s, 32, 8, PLAIN), 32);
  assertEqual(decode(rows.slice(0, 5), LARGE), "10:13");
  assert(rows[6].startsWith("━".repeat(13) + "─") && rows[7].includes("of 25:00"));
  const long = state({ length: 20000, left: 17000 });
  assertEqual(text(face(long, 5, 1, PLAIN)), ["4h44m"]);
  assert(text(face(long, 12, 1, PLAIN))[0].startsWith("4:43:20 "));
  assertEqual(decode(text(face(long, 17, 3, PLAIN), 17), SMALL), "04:44");
}

export function test_labels() {
  const said = (s) => text([label(s)])[0];
  assertEqual(said(state({ name: "work", length: 10, left: 5, round: 2, rounds: 4 })), "work 2/4");
  assertEqual(said(state({ name: "break", length: 10, left: 5, round: 7 })), "break 7");
  assertEqual(said(state({ length: 90, left: 5, paused: true })), "paused of 01:30");
  assertEqual(said(state({ left: 0, done: true })), "done");
  assertEqual(said(state({ left: 0, done: true, over: 75 })), "done +1:15");
}

export function test_bar() {
  assertEqual(text([bar(1, 6)]), ["━━━━━━"]);
  assertEqual(text([bar(0, 6)]), ["──────"]);
  assertEqual(text([bar(0.5, 6)]), ["━━━───"]);
  assertEqual(text([bar(0.25, 6)]), ["━╸────"]);
  assertEqual(text([bar(7, 3)]), ["━━━"]);
}

export function test_tone() {
  const at = (left, more = {}) => tone(state({ length: 100, left, ...more }), FANCY);
  assertEqual(tone(state(), PLAIN), null);
  assertEqual([at(90), at(30), at(15), at(5)], [10, 11, 208, 9]);
  assertEqual(at(90, { name: "break" }), 14);
}

export function test_hover_and_title() {
  const l = look({ glass: true });
  const s = state({ length: 1500, left: 612.4, paused: true });
  const quiet = text(face(s, 9, 10, l), 9);
  const over = text(face(s, 9, 10, l, true), 9);
  assert(!quiet.join("").includes("10:13"));
  assertEqual(over[4], "  10:13  ");
  assertEqual(over.slice(0, 4), quiet.slice(0, 4));
  assertEqual(over.slice(5), quiet.slice(5));
  assert(text(face(s, 5, 6, l, true), 5)[2].includes("11m"));
  assertEqual(title(s, [9, 10], l), "10:13");
  assertEqual(title(s, [5, 6], l), "11m");
  assertEqual(title(s, [27, 8], l), "timer");
  assertEqual(title(s, [5, 1], l), "");
  assertEqual(title({ ...s, done: true }, [9, 10], l), "done");
  assertEqual(title(s, [17, 3], look({ name: "tea" })), "tea");
}

export function test_print() {
  // test_print: every face of a look, drawn by the deck (cases): the
  // ladders by size; the largest that fits a box is the driver's.
  const names = (l) => ladder(l).map(([[w, h]]) => `${w}x${h}`);
  assertEqual(names(PLAIN), ["5x1", "12x1", "20x2", "17x3", "17x6", "32x5", "32x8"]);
  assertEqual(names(look({ glass: true })), [
    "5x1",
    "12x1",
    "5x6",
    "20x2",
    "7x8",
    "9x10",
    "11x12",
    "13x14",
    "27x8",
    "29x10",
    "46x12",
  ]);
  assertEqual([...GLASSES.values()], [2, 3, 4, 5, 6]);
  assertThrows(() => face(CASES[0], 2, 1, PLAIN), RangeError);
}

export function test_bad_options() {
  // test_bad_options: refused in `init` (exit 2, a log line); `--rounds
  // -1` by the manifest (`min`). The cases `tests/both`, `tests/zero`.
  const refused = (args) => assertThrows(() => options(args), RangeError).message;
  assertEqual(refused({ duration: ["5m"], until: "17:30" }), "timer: one of DURATION, --until, --pomodoro");
  assertEqual(refused({ duration: [], pomodoro: "25" }), "timer: not WORK/BREAK: '25'");
  assertEqual(refused({ duration: [], rounds: -1 }), "timer: --rounds: 0 or more");
  assertEqual(refused({ duration: ["0m"] }), "timer: not a duration: '0m'");
  assertEqual(refused({ duration: [], until: "25:00" }), "timer: not a time HH:MM: '25:00'");
  assert(refused({ duration: [], hold: 0 }).startsWith("timer: --hold"));
}

export function test_all() {
  const o = options({ duration: [], all: true, modal: false, click: false });
  assert(o.look.glass && o.look.colour && o.ring && o.burst);
  assert(!o.modal && !o.click);
  assertEqual(options({ duration: [] }).phases, [{ name: "timer", length: 1500 }]);
  assertEqual(options({ duration: [], until: "17:30" }, 17 * 3600).phases, [{ name: "timer", length: 1800 }]);
  const p = options({ duration: [], pomodoro: "25/5", rounds: 0 });
  assert(p.loop && p.phases.length === 2);
}

export function test_wait() {
  // The next drawing: as the twin's loop; a glass alone streams.
  const s = state({ length: 100, left: 40.25 });
  assertEqual(wait(s, [5, 1], PLAIN), 260);
  assertEqual(wait(s, [9, 10], look({ glass: true })), 200);
  assertEqual(wait({ ...s, turn: 0.3 }, [9, 10], look({ glass: true })), 80);
  assertEqual(wait({ ...s, paused: true }, [9, 10], look({ glass: true })), 500);
  assertEqual(wait(state({ length: 100, left: 40 }), null, PLAIN), 1010);
}

// -- the twin's faces --------------------------------------------------------

/** A face as the twin's fixture: runs of one style merged. */
function norm(rows) {
  return JSON.stringify(
    rows.map((row) => {
      const out = [];
      for (const r of row) {
        if (!r.t) continue;
        const style = [r.fg ?? null, !!r.bold, !!r.dim];
        const last = out[out.length - 1];
        if (last && JSON.stringify(last.slice(1)) === JSON.stringify(style)) last[0] += r.t;
        else out.push([r.t, ...style]);
      }
      return out;
    }),
  );
}

function fnv(s) {
  let h = 0x811c9dc5;
  for (const c of s) h = Math.imul(h ^ c.codePointAt(0), 0x01000193) >>> 0;
  return h;
}

const twinState = (i) => state(STATES[i]);

export function test_twin_faces() {
  for (const [si, li, w, h, hover, hash, name] of FACES) {
    const s = twinState(si);
    const l = look(LOOKS[li]);
    assertEqual(fnv(norm(face(s, w, h, l, hover))), hash, `state ${si}, look ${li}, ${w}x${h}, hover ${hover}`);
    assertEqual(title(s, [w, h], l), name, `${si} ${li} ${w}x${h}`);
  }
  for (const [si, li, w, h, rows] of WHOLE) {
    assertEqual(JSON.parse(norm(face(twinState(si), w, h, look(LOOKS[li])))), rows, `${si} ${li} ${w}x${h}`);
  }
}

export function test_twin_glass() {
  for (const [k, t, hash] of SPINS) assertEqual(fnv(norm(spin(k, t / 10, PLAIN))), hash, `spin ${k} ${t}`);
  for (const [k, hash] of BULBS) assertEqual(fnv(JSON.stringify(bulbs(k))), hash, `bulbs ${k}`);
}
