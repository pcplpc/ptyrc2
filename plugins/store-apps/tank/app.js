// SPDX-License-Identifier: GPL-3.0-or-later
// Tank: fishes in one widget, from a bowl of 9x4 with one fish to a
// tank of 40x10 (the twin's `fishes --tank`).
//
// A `Tank` a variant, in its own cells: `step(dt)`, then `rows()`, the
// content of the variant shown, a tick `TICK` s. The fish of `fishes`
// swims here (`reef.js`), with no text and no cursor. A row of surface
// over the water from four rows; `-d`: sand, and decorations on it,
// some in front of the fishes; `-b`: bubbles; `-r`: random species.
// Hover: the fishes come to the pointer, as to a finger on the glass.

import { period } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { CALM, CLASSIC, COLORS, DECOR, Fish, SPECIES, choice, choices, copysign, fg, randint, randrange, round, sample, swim, uniform } from "./reef.js";

export const TICK = 0.1; // seconds a tick
export const TANKS = [
  [9, 4],
  [14, 5],
  [20, 6],
  [28, 8],
  [40, 10],
];
export const [WATER, SAND] = [
  [67, 74],
  [180, 143, 137],
];

/** Glass: thin walls, a rim, a thick floor. */
export const GLASS = {
  corners: "╓╖╚╝",
  top: "╌",
  bottom: "═",
  left: "║",
  right: "║",
  style: { fg: 67 },
  title: { style: { fg: 110 } },
};

/** An aquarium of `w` by `h` cells. As many fishes as the water holds,
 * `count` at most: one in a bowl. `rnd()` as `Math.random`. */
export class Tank {
  constructor(w, h, rnd, { count = 8, decorate = false, bubbles = false, species = false } = {}) {
    Object.assign(this, { w, h, rnd });
    this.t = 0.0;
    this.top = h >= 4 ? 1 : 0;
    this.floor = h - 1; // the lowest row of water
    this.sand = decorate && h >= 4;
    this.decor = []; // [shape, x, ink, front, phase]
    if (this.sand) this.plant();
    this.vents = [];
    if (bubbles) {
      const columns = Array.from({ length: Math.max(0, w - 2) }, (_, i) => i + 1);
      this.vents = sample(rnd, columns, Math.max(1, Math.floor(w / 14))).sort((a, b) => a - b);
    }
    this.air = []; // {x, y, speed: rows per second, phase}
    this.finger = null;
    const kinds = species ? SPECIES.filter((k) => k.length <= w - 3) : [CLASSIC];
    // A small tank is a slow one.
    this.pace = 0.3 + 0.5 * Math.min(1.0, w / 40);
    this.school = [];
    const n = Math.max(1, Math.min(count, round((w * h) / 45)));
    for (let i = 0; i < n; i++) {
      const kind = choice(rnd, kinds);
      const x = uniform(rnd, 0, Math.max(0, w - kind.length));
      const y = randint(rnd, this.top, this.low);
      const vx = choice(rnd, [-1, 1]) * uniform(rnd, 0.5, 1.0) * CALM * this.pace;
      const color = species ? choice(rnd, kind.colors) : COLORS[i % 5];
      this.school.push(new Fish({ x, y, vx, color, kind }));
    }
  }

  /** The lowest row a fish swims on: over the sand. */
  get low() {
    return this.sand ? Math.max(this.top, this.floor - 1) : this.floor;
  }

  /** Decorations along the sand, left to right, a gap between. */
  plant() {
    const { rnd, w } = this;
    const room = this.floor - this.top;
    const fit = DECOR.filter((s) => s.h <= room && s.w <= Math.max(3, Math.floor(w / 3)));
    let x = randint(rnd, 0, 3);
    let big = false;
    while (fit.length) {
      const shape = choices(
        rnd,
        fit,
        fit.map((s) => s.weight),
      );
      if (shape.h >= 4 && big) continue; // one castle a tank
      if (x + shape.w > w) break;
      big ||= shape.h >= 4;
      const ink = choice(rnd, shape.inks);
      const front = rnd() < shape.front;
      this.decor.push([shape, x, ink, front, randrange(rnd, 8)]);
      x += shape.w + randint(rnd, 2, 2 + Math.floor(w / 5));
      if (w < 12) break; // a bowl: one thing in it
    }
  }

  /** The pointer on the cell `(x, y)`: the fishes come. */
  onMove(x, y) {
    this.finger = [x, y];
  }

  onLeave() {
    this.finger = null;
  }

  step(dt) {
    this.t += dt;
    const { rnd, finger } = this;
    const box = [0, this.top, this.w, this.low];
    this.school.forEach((fish, i) => {
      fish.home = null;
      if (finger !== null) {
        // Around the pointer, each at its place, nose to it.
        const x = finger[0] + (i % 2 ? -fish.length - 1 : 1);
        const y = finger[1] + Math.floor((i + 1) / 2) * (i % 4 < 2 ? -1 : 1);
        fish.home = [Math.max(0, Math.min(this.w - fish.length, x)), Math.max(this.top, Math.min(this.low, y))];
      }
      swim(fish, dt, box, null, false, rnd);
      if (fish.home !== null && Math.abs(fish.vx) < 0.5) {
        // Still: it looks at the pointer.
        fish.vx = copysign(0.0, finger[0] - (fish.x + fish.length / 2));
      } else if (fish.home === null && Math.abs(fish.vx) < 1.0) {
        fish.vx = copysign(uniform(rnd, 0.5, 1.0) * CALM * this.pace, fish.vx);
      }
    });
    for (const vent of this.vents) {
      if (rnd() < 0.6 * dt) {
        const speed = uniform(rnd, 1.2, 2.2);
        this.air.push({ x: vent, y: this.low, speed, phase: uniform(rnd, 0, 6) });
      }
    }
    for (const b of this.air) b.y -= b.speed * dt;
    this.air = this.air.filter((b) => b.y >= this.top - 0.5);
  }

  /** The tank as rows of runs, `w` cells each. */
  rows() {
    const { w, h, t } = this;
    const grid = Array.from({ length: h }, () => Array.from({ length: w }, () => ({ t: " " })));
    const put = (x, y, glyph, style) => {
      if (0 <= x && x < w && 0 <= y && y < h && glyph !== " ") grid[y][x] = { t: glyph, ...style };
    };
    const stand = (front) => {
      for (const [shape, x0, ink, ahead, phase] of this.decor) {
        if (ahead !== front) continue;
        const frames = shape.frames;
        const art = frames[(Math.floor((t * 1000) / shape.ms) + phase) % frames.length];
        art.forEach((row, dy) => {
          [...row].forEach((glyph, dx) => put(x0 + dx, this.floor - shape.h + dy, glyph, { fg: fg(ink, glyph) }));
        });
      }
    };
    if (this.top) {
      // The surface: ripples that drift.
      for (let x = 0; x < w; x++) {
        const wave = Math.sin(0.9 * x - 1.7 * t) + Math.sin(0.37 * x + 0.9 * t);
        if (wave > -0.6) put(x, 0, wave < 0.9 ? "~" : "≈", { fg: WATER[wave > 0.3 ? 1 : 0], dim: true });
      }
    }
    if (this.sand) {
      for (let x = 0; x < w; x++) {
        const grain = Math.floor((x * 2654435761 + w * 40503) / 128);
        put(x, this.floor, ".,.:. .._"[grain % 9], { fg: SAND[grain % 3], dim: true });
      }
    }
    stand(false);
    const low = this.low;
    for (const b of this.air) {
      const x = round(b.x + 0.9 * Math.sin(b.phase + 2.5 * b.y));
      const high = (low - b.y) / Math.max(1, low - this.top);
      put(x, round(b.y), high < 0.35 ? "." : high < 0.8 ? "o" : "O", { fg: 110, dim: true });
    }
    for (const fish of this.school) {
      const [x, y, glyph] = fish.cell;
      [...glyph].forEach((ch, dx) => put(x + dx, y, ch, { fg: fish.color, bold: true }));
    }
    stand(true);
    return grid.map((row) => {
      const runs = [];
      let key = null;
      for (const cell of row) {
        const k = JSON.stringify({ ...cell, t: "" });
        if (runs.length && k === key) runs[runs.length - 1].t += cell.t;
        else runs.push({ ...cell });
        key = k;
      }
      return runs;
    });
  }
}

/** The options of `init`: `--all` spread. */
export function options(args) {
  const all = args.all === true;
  return {
    count: Number.isInteger(args.count) ? args.count : 8,
    decorate: all || args.decorate === true,
    bubbles: all || args.bubbles === true,
    species: all || args.random === true,
  };
}

/** A `Tank` a variant; the one shown swims. Session ms. */
export class Tanks {
  constructor(o, rnd, now = 0) {
    this.tanks = new Map(TANKS.map(([w, h]) => [`${w}x${h}`, new Tank(w, h, rnd, o)]));
    this.seen = new Map([...this.tanks.keys()].map((k) => [k, now]));
  }

  /** The content of `w` x `h` at `now`: not shown for a while, it was
   * still: no leap. */
  render(w, h, now) {
    const k = `${w}x${h}`;
    const tank = this.tanks.get(k);
    tank.step(Math.max(0, Math.min((now - this.seen.get(k)) / 1000, 3 * TICK)));
    this.seen.set(k, now);
    return tank.rows();
  }

  come(size, x, y) {
    for (const tank of this.tanks.values()) tank.finger = null;
    if (size) this.tanks.get(`${size[0]}x${size[1]}`)?.onMove(x, y);
  }

  go() {
    for (const tank of this.tanks.values()) tank.onLeave();
  }
}

let deck;

export function init(args) {
  const tanks = new Tanks(options(args), Math.random, Date.now());
  deck = new Deck({ border: GLASS });
  // The next tick shows it: no draw of its own.
  deck.add("fishes", TANKS, (w, h) => tanks.render(w, h, deck.now), {
    hover: (x, y, widget) => {
      tanks.come(widget.size, x, y);
      return false;
    },
    leave: () => {
      tanks.go();
      return false;
    },
  });
  period(Math.round(TICK * 1000));
}

export function tick(now) {
  deck.tick(now);
}

export function event(topic, data) {
  deck.event(topic, data);
}
