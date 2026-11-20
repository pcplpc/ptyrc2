// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of clock, `ptyrc dev test --unit`: the faces of a time
// of their own (test_clock.py), the wall clock on a host of its own;
// those of braille.js and digits.js.

import { assert, assertEqual, assertThrows, cells, civil } from "ptyrc";
import { DIM, HOURS, MINUTES, SECONDS, Wall, face, ladder, options, week } from "./app.js";
import { LARGE, SMALL, decode } from "./digits.js";

export { test_braille } from "./braille.tests.js";
export { test_blocks, test_fonts, test_layout } from "./digits.tests.js";

/** `civil` of a UTC date. */
const at = (y, mo, d, h = 0, mi = 0, s = 0) => civil(Date.UTC(y, mo - 1, d, h, mi, s), 0);

const TIMES = [at(2026, 11, 13, 0, 0, 0), at(2026, 11, 13, 9, 5, 7), at(2026, 12, 31, 23, 59, 59)];
const MODES = [
  [true, false],
  [true, true],
  [false, false],
  [false, true],
];
const two = (n) => String(n).padStart(2, "0");
const DAY = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** The rows as text, each `w` columns. */
function plain(rows, w) {
  return rows.map((row) => {
    const t = row.map((r) => r.t).join("");
    return t + " ".repeat(Math.max(0, w - cells(t)));
  });
}

export function test_bounds() {
  for (const t of TIMES) {
    for (const [seconds, big] of MODES) {
      for (const [[w, h], draw] of ladder(seconds, big)) {
        const rows = draw(t, w, h);
        const where = `${w}x${h} ${seconds} ${big}`;
        assert(rows.length > 0 && rows.length <= h, where);
        for (const text of plain(rows, w)) {
          assertEqual(cells(text), w, `${where}: ${text}`);
          // One column a character, none a control.
          assert(Array.from(text).every((c) => cells(c) === 1 && !/\p{C}/u.test(c)), `${where}: ${text}`);
        }
        // Nothing cut: in a larger box, the same rows.
        const large = plain(draw(t, w + 20, h + 4), w + 20).map((s) => s.trim());
        const kept = plain(rows, w).map((s) => s.trim());
        assertEqual(
          large.filter((s) => s),
          kept.filter((s) => s),
          where,
        );
      }
    }
  }
}

export function test_digits() {
  for (const t of TIMES) {
    const hm = `${two(t.hour)}:${two(t.minute)}`;
    const s = two(t.second);
    for (const seconds of [true, false]) {
      const faces = ladder(seconds);
      const texts = new Map(faces.map(([[w, h], draw]) => [`${w}x${h}`, plain(draw(t, w, h), w)]));
      assertEqual(texts.get("5x1"), [hm]);
      assertEqual(texts.get("10x2")[0].trim(), seconds ? `${hm}:${s}` : hm);
      assertEqual(texts.get("10x2")[1], `${DAY[t.weekday]} ${t.day} ${MON[t.month - 1]}`);
      for (const size of ["17x3", "17x5"]) assertEqual(decode(texts.get(size).slice(0, 3), SMALL), hm);
      for (const size of ["32x5", "32x7"]) assertEqual(decode(texts.get(size).slice(0, 5), LARGE), hm);
      const [[gw, gh]] = faces[faces.length - 1];
      const grand = texts.get(`${gw}x${gh}`);
      assertEqual(decode(grand.slice(0, 5), LARGE), seconds ? hm + "?" : hm);
      const days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
      assert(grand[6].includes(`${days[t.weekday]} ${t.day} `) && grand[6].includes(` ${t.year}`), grand[6]);
      assert(grand[7].includes(`week ${week(t)}`), grand[7]);
      if (seconds) {
        assertEqual(texts.get("8x1"), [`${hm}:${s}`]);
        const small = grand.slice(2, 5).map((r) => r.slice(-7));
        assertEqual(decode(small, SMALL), s);
      }
    }
  }
}

export function test_blink() {
  const colon = (t, seconds) => face(t, 17, 3, seconds)[0].find((r) => r.t === SMALL[10][0]).fg;
  const even = at(2026, 1, 1, 9, 5, 6);
  const odd = at(2026, 1, 1, 9, 5, 7);
  assert(colon(even, true) !== colon(odd, true));
  assertEqual(colon(even, false), colon(odd, false));
}

/** The cells `[x, y]` of each colour. */
function places(rows) {
  const out = new Map();
  rows.forEach((row, y) =>
    row.forEach((r, x) => {
      if (r.fg === undefined) return;
      if (!out.has(r.fg)) out.set(r.fg, []);
      out.get(r.fg).push([x, y]);
    }),
  );
  return out;
}

export function test_dial() {
  // At 3:00, the hour hand points right, the minute hand up; the second
  // marks the rim.
  const rows = face(at(2026, 1, 1, 3, 0, 30), 17, 8);
  assert(rows.length === 8 && rows.every((row) => row.length === 17));
  const p = places(rows);
  assert(p.get(HOURS).length && p.get(HOURS).every(([x, y]) => x >= 8 && (y === 3 || y === 4)));
  assert(p.get(MINUTES).length && p.get(MINUTES).every(([x, y]) => x === 8 && y < 4));
  assertEqual(p.get(SECONDS), [[8, 7]]);
  assert(p.get(DIM).length > 20);
  const quiet = face(at(2026, 1, 1, 3, 0, 30), 17, 8, false);
  assert(!places(quiet).has(SECONDS));
}

export function test_dial_mirrors() {
  // Rim and marks: the left half mirrors the right, the top the bottom.
  // The hands, at 12:00, keep to the middle column.
  const rows = face(at(2026, 1, 1, 12, 0, 0), 17, 8, false);
  const bits = rows.map((row) => row.map((r) => (r.t === " " ? 0 : r.t.charCodeAt(0) - 0x2800)));
  assert(bits.every((row) => row.length === 17));
  const flip = (b, pairs) => pairs.reduce((n, [p, q]) => n + q * !!(b & p) + p * !!(b & q), 0);
  const across = [
    [0x01, 0x08],
    [0x02, 0x10],
    [0x04, 0x20],
    [0x40, 0x80],
  ];
  const down = [
    [0x01, 0x40],
    [0x02, 0x04],
    [0x08, 0x80],
    [0x10, 0x20],
  ];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 17; x++) {
      if (x === 8) continue;
      assertEqual(bits[y][16 - x], flip(bits[y][x], across), `${x},${y}`);
      assertEqual(bits[7 - y][x], flip(bits[y][x], down), `${x},${y}`);
    }
  }
}

export function test_ladders() {
  const sizes = (...mode) => ladder(...mode).map(([[w, h]]) => `${w}x${h}`);
  assertEqual(sizes(), ["5x1", "8x1", "10x2", "17x3", "17x5", "17x8", "32x5", "32x7", "41x8"]);
  assertEqual(sizes(false), ["5x1", "10x2", "17x3", "17x5", "17x8", "32x5", "32x7", "32x8"]);
  assertEqual(sizes(true, true), ["17x3", "17x5", "32x5", "32x7", "41x8"]);
  assertEqual(sizes(false, true), ["17x3", "17x5", "32x5", "32x7", "32x8"]);
  for (const seconds of [true, false]) {
    const areas = ladder(seconds).map(([[w, h]]) => w * h);
    assertEqual(areas, [...areas].sort((a, b) => a - b));
  }
  assertThrows(() => face(TIMES[0], 8, 1, false), RangeError);
}

export function test_week() {
  assertEqual(week(at(2026, 1, 1)), 1); // a Thursday
  assertEqual(week(at(2026, 11, 13)), 46);
  assertEqual(week(at(2026, 12, 31)), 53);
  assertEqual(week(at(2027, 1, 3)), 53); // a Sunday
  assertEqual(week(at(2027, 1, 4)), 1);
  assertEqual(week(at(2024, 12, 30)), 1);
}

export function test_options() {
  assertEqual(options({ utc: false, tz: null, seconds: true, big: false }), {
    utc: false,
    tz: null,
    seconds: true,
    big: false,
  });
  assertEqual(options({ tz: "Asia/Tokyo", seconds: false }).tz, "Asia/Tokyo");
  assertEqual(options({ seconds: false }).seconds, false);
  assertThrows(() => options({ utc: true, tz: "Asia/Tokyo" }), RangeError);
}

export function test_bad_zone() {
  // test_bad_zone: an unknown zone refused at the start, a line for the
  // log (init: status 2). A case cannot carry it: `dev test --cases`
  // fails a run whose app exits with a status not 0.
  const zone = (name) => {
    if (name !== "Asia/Tokyo") throw Object.assign(new Error("no such zone"), { code: "bad-params" });
    return { zone: name, offsets: [{ at: 0, offsetS: 32400, abbr: "JST", next: null }] };
  };
  assertEqual(options({ tz: "Asia/Tokyo" }, { zone }).tz, "Asia/Tokyo");
  const e = assertThrows(() => options({ tz: "Nowhere/Atlantis" }, { zone }), RangeError);
  assertEqual(e.message, "clock: unknown time zone: Nowhere/Atlantis");
  // Another error of the host is not a zone refused.
  const down = () => {
    throw Object.assign(new Error("down"), { code: "internal" });
  };
  assert(!(assertThrows(() => options({ tz: "Asia/Tokyo" }, { zone: down })) instanceof RangeError));
}

/** A host of its own: `now` at `clock.ms`, the machine at +1 h; the
 * zone `Asia/Tokyo` at +9 h, then +10 h from `change`; each ask logged. */
function host(clock, change = null) {
  const asked = [];
  return {
    asked,
    now: () => ({ unixMs: clock.ms, zone: "Europe/Paris", offsetS: 3600, abbr: "CET", next: null }),
    zone(name, ms) {
      asked.push([name, ms]);
      const late = change !== null && ms >= change;
      return { zone: name, offsets: [{ at: ms, offsetS: late ? 36000 : 32400, abbr: "JST", next: late ? null : change }] };
    },
  };
}

export function test_wall() {
  const clock = { ms: 1000 };
  const local = new Wall(options({}), host(clock));
  assertEqual(local.read(), { unixMs: 1000, offsetS: 3600 });
  assertEqual(new Wall(options({ utc: true }), host(clock)).read(), { unixMs: 1000, offsetS: 0 });
  // The zone asked once, again at its `next`.
  const h = host(clock, 5000);
  const tokyo = new Wall(options({ tz: "Asia/Tokyo" }), h);
  for (const ms of [1000, 2000, 4999]) {
    clock.ms = ms;
    assertEqual(tokyo.read().offsetS, 32400);
  }
  clock.ms = 5000;
  assertEqual(tokyo.read().offsetS, 36000);
  clock.ms = 9000;
  assertEqual(tokyo.read().offsetS, 36000);
  assertEqual(h.asked, [
    ["Asia/Tokyo", 1000],
    ["Asia/Tokyo", 5000],
  ]);
  // Shown in the zone: 00:00 UTC is 09:00 in Tokyo.
  clock.ms = Date.UTC(2026, 10, 13);
  const t = civil(clock.ms, new Wall(options({ tz: "Asia/Tokyo" }), host(clock)).read().offsetS);
  assertEqual(face(t, 5, 1), [[{ t: "09", fg: HOURS, bold: true }, { t: ":", fg: DIM }, { t: "00", fg: MINUTES, bold: true }]]);
}

export function test_print() {
  // test_print: a face a size of the mode, nine plain, five with
  // `--big --no-seconds`; drawn by the deck (cases), not printed.
  assertEqual(ladder().length, 9);
  assertEqual(ladder(false, true).length, 5);
}
