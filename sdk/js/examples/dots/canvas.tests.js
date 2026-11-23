// SPDX-License-Identifier: GPL-3.0-or-later
// "ptyrc/canvas" (test_canvas.py, less `ansi`).

import { assert, assertEqual, assertThrows, Canvas, rgb } from "ptyrc";
import { Braille, DOTS, Half, NEAR, channels, hex, mix, named, palette, xterm } from "ptyrc/canvas";

const RED = rgb(255, 0, 0);
const BLUE = rgb(0, 0, 255);
const WHITE = rgb(255, 255, 255);
const text = (row) => row.map((r) => r.t).join("");

export function test_colours() {
  assertEqual(mix(RED, BLUE, 0), RED);
  assertEqual(mix(RED, BLUE, 1), BLUE);
  assertEqual(channels(mix(RED, BLUE, 0.5)), [128, 0, 128]);
  assertEqual([mix(RED, BLUE, -3), mix(RED, BLUE, 7)], [RED, BLUE]);
  assertEqual(mix("#ff0000", "#0000ff", 1), BLUE);
  assertEqual(hex(rgb(1, 171, 255)), "#01abff");
  assertEqual(channels("#01abff"), [1, 171, 255]);
  assertEqual([palette(16), palette(231)], [rgb(0, 0, 0), WHITE]);
  assertEqual([palette(196), palette(232), palette(255)], [RED, rgb(8, 8, 8), rgb(238, 238, 238)]);
  // Every colour of the 240 is its own nearest.
  for (let n = 16; n < 256; n++) assertEqual(xterm(palette(n)), n);
  // A dark green stays green; a grey stays grey.
  const [r, g, b] = channels(palette(xterm(rgb(40, 82, 20))));
  assert(g > r && g > b);
  assert(xterm(rgb(90, 90, 90)) >= 232);
  assertThrows(() => palette(15), RangeError);
  assertThrows(() => mix(14, RED, 0.5), TypeError);
}

// `xterm` as a loop over the 240: the reference.
function xtermLoop(c) {
  const [r, g, b] = channels(c);
  const chroma = Math.max(r, g, b) - Math.min(r, g, b);
  let best = Infinity;
  let n = 0;
  for (let i = 16; i < 256; i++) {
    const [x, y, z] = channels(palette(i));
    const lost = Math.max(0, chroma - (Math.max(x, y, z) - Math.min(x, y, z)));
    const far = 3 * (x - r) ** 2 + 4 * (y - g) ** 2 + 2 * (z - b) ** 2 + 2 * lost ** 2;
    if (far < best) (best = far), (n = i);
  }
  return n;
}

export function test_xterm_sampled() {
  // About the levels of the cube and the greys, and the midpoints
  // between them (the ties); then colours at random, each twice (kept).
  const near = new Set();
  for (const l of [0, 47, 48, 95, 115, 116, 135, 155, 175, 195, 215, 235, 255, 8, 13, 238, 243, 250]) {
    for (let d = -2; d <= 2; d++) if (l + d >= 0 && l + d < 256) near.add(l + d);
  }
  const values = [...near];
  for (const r of values) for (const g of values) for (const b of values) if ((r * 7 + g * 3 + b) % 61 === 0) assertEqual(xterm(rgb(r, g, b)), xtermLoop(rgb(r, g, b)), [r, g, b]);
  let s = 1;
  for (let n = 0; n < 1500; n++) {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    const c = rgb((s >>> 8) & 255, (s >>> 16) & 255, s >>> 24);
    const want = xtermLoop(c);
    assertEqual(xterm(c), want, hex(c));
    assertEqual(xterm(c), want, hex(c));
  }
}

export function test_named() {
  assertEqual([named("black"), named("white"), named("cyan")], [0, 7, 6]);
  assertEqual([named("bright-black"), named("bright-yellow"), named("bright-magenta"), named("bright-cyan")], [8, 11, 13, 14]);
  assertEqual(named("bright-white"), 15);
  assertThrows(() => named("grey"), RangeError);
  assertThrows(() => named("bright-"), RangeError);
}

export function test_braille() {
  const dots = new Braille(3, 2);
  assertEqual([dots.width, dots.height], [3, 2]);
  assertEqual(dots.rows(), ["   ", "   "]);
  assertEqual(dots.cells(), []);
  assertEqual([dots.glyph(0, 0), dots.glyph(9, 9)], ["", ""]);
  dots.set(0, 0);
  dots.set(1, 3);
  dots.set(5, 7);
  dots.set(6, 0); // outside: ignored
  dots.set(0, -1);
  assert(dots.get(0, 0) && dots.get(1, 3) && !dots.get(1, 0));
  assert(!dots.get(99, 0) && !dots.get(-1, 0));
  assertEqual(dots.rows(), ["⢁  ", "  ⢀"]);
  assertEqual(dots.cells(), [
    [0, 0],
    [2, 1],
  ]);
  assertEqual(dots.bits[0], 0x81);
  dots.set(0, 0, false);
  assertEqual(dots.glyph(0, 0), "⢀");
  assertEqual(dots.clear().rows(), ["   ", "   "]);
  const line = new Braille(2, 1);
  line.line(0, 0, 3, 3);
  assertEqual(line.rows(), ["⠑⢄"]);
  line.line(0, 0, 3, 3, false);
  assertEqual(line.rows(), ["  "]);
  // The eight dots are the eight bits.
  for (let bits = 1; bits < 256; bits++) {
    const cell = new Braille(1, 1);
    for (let y = 0; y < 4; y++) for (let x = 0; x < 2; x++) if (bits & DOTS[y][x]) cell.set(x, y);
    assertEqual(cell.rows(), [String.fromCharCode(0x2800 + bits)]);
  }
  assertThrows(() => new Braille(0, 1), RangeError);
}

export function test_braille_draw() {
  const dots = new Braille(3, 1);
  dots.set(2, 0);
  const canvas = new Canvas(4, 2);
  canvas.set(0, 0, "x");
  dots.draw(canvas, 1, 1, { fg: 9 });
  assertEqual(canvas.runs(), [[{ t: "x" }, { t: "   " }], [{ t: " " }, { t: " ⠁ ", fg: 9 }]]);
  canvas.release();
}

export function test_half() {
  const half = new Half(4, 2, BLUE);
  assertEqual(half.rows(), [[{ t: "    ", bg: 21 }], [{ t: "    ", bg: 21 }]]);
  half.set(1, 0, RED);
  half.set(9, 9, RED); // outside: ignored
  assertEqual([half.get(1, 0), half.get(9, 9)], [RED, null]);
  const first = half.rows()[0];
  assertEqual(text(first), " ▀  ");
  // A blank joins the run before it: it shows no foreground.
  assertEqual(first[1], { t: "▀  ", fg: 196, bg: 21 });
  half.set(1, 1, RED);
  assertEqual(half.rows()[0], [
    { t: " ", bg: 21 },
    { t: " ", bg: 196 },
    { t: "  ", bg: 21 },
  ]);
  half.set(0, 2, RED, 0.5);
  assertEqual(channels(half.get(0, 2)), [128, 0, 128]);
  half.rect(2, 2, 2, 2, RED);
  assertEqual([0, 1, 2, 3].map((x) => half.get(x, 3)), [BLUE, BLUE, RED, RED]);
  half.fill(RED);
  assertEqual(half.rows(true), [[{ t: "    ", bg: "#ff0000" }], [{ t: "    ", bg: "#ff0000" }]]);
  const disc = new Half(5, 3);
  disc.disc(2.5, 2.5, 1.2, RED);
  const lit = [];
  for (let y = 0; y < 6; y++) for (let x = 0; x < 5; x++) if (disc.get(x, y) === RED) lit.push([x, y]);
  assertEqual(lit, [
    [2, 1],
    [1, 2],
    [2, 2],
    [3, 2],
    [2, 3],
  ]);
}

export function test_glyphs() {
  const half = new Half(4, 1, BLUE);
  half.put(1, 0, "ab", RED);
  half.put(3, 0, "cd", null, RED); // cut at the edge
  half.put(0, 5, "z"); // outside
  const row = half.rows()[0];
  assertEqual(text(row), " abc");
  assertEqual(row[1], { t: "ab", bg: 21, fg: 196 });
  assertEqual(row[2], { t: "c", bg: 196 });
  // Under a glyph, the mean of the two pixels.
  const two = new Half(1, 1, RED);
  two.set(0, 1, BLUE);
  two.put(0, 0, "x");
  assertEqual(two.rows(true)[0], [{ t: "x", bg: "#800080" }]);
}

export function test_ink() {
  const half = new Half(3, 1, BLUE);
  half.set(1, 0, RED); // an edge in the middle cell
  half.put(2, 0, "x");
  const dots = new Braille(3, 1);
  for (let x = 0; x < 6; x++) dots.set(x, 0);
  half.ink(dots, WHITE, 0.5);
  // Not over the edge, not over a glyph.
  assertEqual(half.rows(true)[0], [
    { t: "⠉", bg: "#0000ff", fg: "#8080ff" },
    { t: "▀", fg: "#ff0000", bg: "#0000ff" },
    { t: "x", bg: "#0000ff" },
  ]);
  // Two pixels nearly alike: the one under the dots.
  const near = new Half(1, 1, rgb(10, 10, 10));
  near.set(0, 1, rgb(14, 14, 14));
  const low = new Braille(1, 1);
  low.set(0, 3);
  near.ink(low, RED);
  assertEqual(near.rows(true)[0], [{ t: "⡀", bg: "#0e0e0e", fg: "#ff0000" }]);
  // Over an edge, `rough`: the pixel under the dots takes the ink.
  const edge = new Half(1, 1, BLUE);
  edge.set(0, 1, RED);
  edge.ink(low, WHITE, 1, 0.5);
  assertEqual([edge.get(0, 0), channels(edge.get(0, 1))], [BLUE, [255, 128, 128]]);
  assertEqual(text(edge.rows()[0]), "▀");
  // Alike, but two colours of the 256: no glyph is shown there. A
  // fill keeps the glyphs.
  near.fill(rgb(100, 100, 100));
  near.set(0, 1, rgb(112, 112, 112));
  near.ink(low, RED);
  assertEqual(near.rows(), [[{ t: "▀", fg: 241, bg: 242 }]]);
  assertEqual(near.rows(true)[0][0].t, "⡀");
  assertEqual(NEAR, 24);
}

export function test_half_draw() {
  const half = new Half(2, 1, BLUE);
  half.set(0, 0, RED);
  const canvas = new Canvas(3, 1);
  half.draw(canvas, 1, 0);
  assertEqual(canvas.runs(), [[{ t: " " }, { t: "▀ ", fg: 196, bg: 21 }]]);
  half.draw(canvas, 1, 0, true);
  assertEqual(canvas.runs()[0][1], { t: "▀ ", fg: "#ff0000", bg: "#0000ff" });
  canvas.release();
}
