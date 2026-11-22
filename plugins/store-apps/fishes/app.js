// SPDX-License-Identifier: GPL-3.0-or-later
// Fishes: an aquarium behind the text of a scrolling pane.
//
// A fish holds a position of the frame (`space: "frame"`): scroll the
// text and the fishes scroll with it. The view is read when the driver
// tells it changed (`ptyrc/sight`); the topic `cursor` tells where the
// cursor is: fishes flee it, farther when it moves. All the draws of a
// tick go in one `batch`, a tick `BEAT` s: a fish that flees goes a
// cell a tick. Fishes swim behind the text (`back`), between the words.
//
// Half of the school, rounded down, drawn at birth, is shy (`--shy N`:
// N). A shy fish swims to the nearest spot of the view where the text
// hides all of it but a cell at most (`hideouts`), out of reach of the
// cursor, slows down and stays there `STAY` s; then it goes to another
// spot, drawn among those at least two lengths away (`lure`). Fear
// comes first.
//
// A fish left out of the view by a scroll stays where it was. After
// `WAIT` s it goes, one fish a tick at most, and comes back in the view
// where the text hides most of it (`entry`). Back in view before its
// time, it carries on.
//
// `-d`: decorations on the text; `-b`: bubbles from the bottom of the
// view; `-r`: a fish that comes back is of a random species, and now
// and then an event (`reef.js`). What was populated is kept in frame
// cells: scroll away and back, the same castle stands on the same line.
//
// The frame is the one told (`init`, `move`; `ptyrc/sight`): `shape`
// is above `layout`. Without `shape` the frame is told unscrolled, its
// cells the view's, and the driver places its cells so.

import { call, period, PtyrcError } from "ptyrc";
import { Sight } from "ptyrc/sight";
import { Batch, CALM, COLORS, Events, Fish, LENGTH, NEAR, NEAR_MOVING, Reef, SPECIES, choice, copysign, inkSums, pad, randint, reach, sample, swim, uniform } from "./reef.js";

export const BEAT = 1 / 30; // seconds a tick
export const WAIT = [2.0, 5.0]; // seconds out of view before a fish goes
export const STAY = [4.0, 10.0]; // seconds a shy fish stays in a hiding place
export const STILL = 1.0; // seconds without a scroll: the view is still

const same = (a, b) => (a === null || b === null ? a === b : a.length === b.length && a.every((v, i) => v === b[i]));

/** The view in frame cells, `[x0, y0, x1, y1]`: a fish's `x` in [x0,
 * x1 - length], its `y` in [y0, y1]. */
export const view = (origin, size) => [origin[0], origin[1], origin[0] + size[0], origin[1] + size[1] - 1];

export const inside = (cell, box, length = LENGTH) =>
  box[0] <= cell[0] && cell[0] <= box[2] - length && box[1] <= cell[1] && cell[1] <= box[3];

/** Who leaves, and when. `cells`: where the fishes are; `due`: when each
 * goes, null while in view; `lengths`: theirs. A fish out of view is
 * given a time `WAIT` from `now`; one in view, none. Returns the new
 * times and the fish that goes now, if any: the most overdue, one a
 * call. */
export function tide(cells, due, box, now, rnd, lengths = null) {
  const n = lengths ?? cells.map(() => LENGTH);
  const out = cells.map((c, i) => (inside(c, box, n[i]) ? null : due[i] === null ? now + uniform(rnd, ...WAIT) : due[i]));
  let going = null;
  out.forEach((d, i) => {
    if (d !== null && d <= now && (going === null || d < out[going])) going = i;
  });
  if (going !== null) out[going] = null;
  return [out, going];
}

/** For each spot of the view, `[x, y, n]`: `n` cells of a fish of
 * `length` there that `rows`, the view's text, hides. */
export function cover(rows, box, length = LENGTH) {
  return spots(rows, box, length, 0, true);
}

/** The spots where the text hides a fish but a cell at most. */
export const hideouts = (rows, box, length = LENGTH) => spots(rows, box, length, length - 1, false);

/** The spots of `cover` where `n` is `least` or more: `[x, y, n]`, or
 * `[x, y]` but for `told`. */
function spots(rows, box, length, least, told) {
  const [x0, y0, x1, y1] = box;
  const last = Math.max(x0, x1 - length);
  const out = [];
  for (let y = y0; y <= y1 && y - y0 < rows.length; y++) {
    // Ink before each column: a sum over a window is a difference.
    const ink = inkSums(rows[y - y0], last - x0 + length);
    for (let x = x0; x <= last; x++) {
      const n = ink[x - x0 + length] - ink[x - x0];
      if (n >= least) out.push(told ? [x, y, n] : [x, y]);
    }
  }
  return out;
}

/** The spot of `spots` that is `cell`, or null. */
function among(spots, cell) {
  if (cell === null) return null;
  const [x, y, n] = [cell[0], cell[1], cell.length];
  for (const s of spots) if (s[0] === x && s[1] === y && s.length === n && (n <= 2 || same(s, cell))) return s;
  return null;
}

/** Where a shy fish heads, kept in `fish.home`; null for a bold one, or
 * when no spot is out of the cursor's reach. `spots`: `hideouts`. The
 * nearest spot first; `STAY` there, then one drawn at least two lengths
 * away. */
export function lure(fish, spots, cursor, dt, rnd) {
  if (!fish.shy || fish.leaving) {
    fish.home = null;
    return null;
  }
  const n = fish.length;
  const safe = cursor === null ? spots : spots.filter((s) => reach(s[0], s[1], cursor, n) >= NEAR_MOVING);
  const home = among(safe, fish.home);
  if (home === null) fish.home = null;
  else if (same(fish.cell.slice(0, 2), home)) {
    fish.stay -= dt;
    if (fish.stay <= 0) {
      const [hx, hy] = home;
      const away = safe.filter((s) => Math.hypot(s[0] - hx, s[1] - hy) > 2 * n);
      fish.home = choice(rnd, away.length ? away : safe);
      fish.stay = uniform(rnd, ...STAY);
    }
  }
  if (fish.home === null && safe.length) {
    // The nearest, a draw a spot to break the ties. A spot farther by
    // its square than the best by more than rounding cannot win.
    const [fx, fy] = [fish.x, fish.y];
    let [best, near, square, tie] = [null, 0, Infinity, 0];
    for (const s of safe) {
      const dx = s[0] - fx;
      const dy = s[1] - fy;
      const d2 = dx * dx + dy * dy;
      const r = rnd();
      if (d2 > square * (1 + 1e-9)) continue;
      const d = Math.hypot(dx, dy);
      if (best === null || d < near || (d === near && r < tie)) [best, near, square, tie] = [s, d, d2, r];
    }
    fish.home = best;
    fish.stay = uniform(rnd, ...STAY);
  }
  return fish.home;
}

/** Where a fish comes back: a spot of the view where `rows`, its text,
 * hides most of the fish, drawn among the best. With no text at all, a
 * side of the view. */
export function entry(rows, box, rnd, length = LENGTH) {
  const [x0, y0, x1, y1] = box;
  const hidden = cover(rows, box, length);
  const best = Math.max(0, ...hidden.map(([, , n]) => n));
  if (!best) return [choice(rnd, [x0, Math.max(x0, x1 - length)]), randint(rnd, y0, y1)];
  const [x, y] = choice(
    rnd,
    hidden.filter(([, , n]) => n === best),
  );
  return [x, y];
}

/** What `fish` flees: a threat within `NEAR`, the nearest; else the
 * cursor. As the `cursor` and `moving` of `swim`. */
export function menace(fish, cursor, moving, threats) {
  let near = null;
  for (const p of threats) if (near === null || reach(fish.x, fish.y, p, fish.length) < reach(fish.x, fish.y, near, fish.length)) near = p;
  if (near !== null && reach(fish.x, fish.y, near, fish.length) < NEAR) return [near, false];
  return [cursor, moving];
}

/** A bold, calm fish of the view heads for the nearest side and leaves;
 * null if there is none. */
export function depart(school, box, rnd) {
  const calm = school.filter((f) => !f.shy && f.scared <= 0 && inside(f.cell, box, f.length));
  if (!calm.length) return null;
  const fish = choice(rnd, calm);
  const middle = (box[0] + box[2] - fish.length) / 2;
  fish.leaving = true;
  fish.vx = copysign(CALM, fish.x - middle);
  return fish;
}

/** The options of `init`. */
export function options(args) {
  const all = args.all === true;
  const count = Number.isInteger(args.count) ? args.count : 8;
  return {
    count,
    shy: Number.isInteger(args.shy) ? args.shy : Math.floor(count / 2),
    decorate: all || args.decorate === true,
    bubbles: all || args.bubbles === true,
    random: all || args.random === true,
  };
}

/** The school, the reef and the events, born on the first view. `env`:
 * `rnd()`, `out` (a `Batch`). Times in seconds. */
export class Aquarium {
  constructor(o, env, box, now) {
    this.o = o;
    this.env = env;
    const { rnd } = env;
    const [x0, y0, x1, y1] = box;
    const shy = new Set(
      sample(
        rnd,
        Array.from({ length: o.count }, (_, i) => i),
        Math.max(0, Math.min(o.shy, o.count)),
      ),
    );
    this.school = Array.from(
      { length: o.count },
      (_, i) =>
        new Fish({
          x: uniform(rnd, x0, Math.max(x0, x1 - LENGTH)),
          y: randint(rnd, y0, y1),
          vx: choice(rnd, [-1, 1]) * uniform(rnd, 0.5, 1.0) * CALM,
          color: COLORS[i % COLORS.length],
          shy: shy.has(i),
        }),
    );
    this.due = this.school.map(() => null);
    this.before = null; // the cursor at the last step
    this.rows = [];
    this.spots = new Map(); // hideouts, by length
    this.safe = new Map(); // those the cursor does not reach
    this.surveyed = null; // the view at the last look at the reef
    this.settle = null; // time of a second look, the view still
    this.reef = new Reef();
    this.events = o.random ? new Events(this.reef, now, rnd) : null;
  }

  /** One tick: `view` `{frame, box, rows, changed}`, `cursor` in frame
   * cells or null; the draws go to `env.out`. */
  step(view, cursor, now, dt) {
    const { o, school, reef } = this;
    const { rnd, out } = this.env;
    const { frame, box } = view;
    const moving = !same(cursor, this.before);
    this.before = cursor;
    if (moving) this.safe = new Map();
    const cells = school.map((f) => f.cell);
    const [due, going] = tide(
      cells,
      this.due,
      box,
      now,
      rnd,
      school.map((f) => f.length),
    );
    this.due = due;
    // The reef is looked at when the text is read, and once more `STILL`
    // after a scroll: the view is still then.
    if (view.changed || (this.settle !== null && now >= this.settle)) {
      const still = same(this.surveyed, box);
      this.settle = still ? null : now + STILL;
      this.surveyed = box;
      if (view.changed) {
        this.rows = pad(view.rows, box[2] - box[0]);
        this.spots = new Map();
        this.safe = new Map();
      }
      reef.prune(this.rows, box, out);
      if (o.decorate) reef.survey(this.rows, box, still, rnd);
      reef.show(out, frame, box);
    }
    const rows = this.rows;
    if (going !== null) {
      const fish = school[going];
      if (o.random) {
        fish.kind = choice(rnd, SPECIES);
        fish.color = choice(rnd, fish.kind.colors);
      }
      [fish.x, fish.y] = entry(rows, box, rnd, fish.length);
      // Calm, towards the middle of the view.
      const inward = (box[0] + box[2] - fish.length) / 2 - fish.x;
      fish.vx = copysign(uniform(rnd, 0.5, 1.0) * CALM, inward);
      fish.vy = fish.scared = 0.0;
      fish.home = null;
      fish.leaving = false;
    }
    let threats = [];
    if (this.events !== null) {
      let leave;
      [threats, leave] = this.events.step(out, frame, rows, box, now, dt);
      if (leave) depart(school, box, rnd);
    }
    if (o.bubbles) reef.bubbles(out, frame, box, now, rnd);
    for (const fish of school) {
      // Out of view, it waits; unless it is on its way out.
      if (inside(fish.cell, box, fish.length) || fish.leaving) {
        const n = fish.length;
        if (fish.shy && !this.safe.has(n)) {
          // Looked for when the text or the cursor changed, not a fish a tick.
          if (!this.spots.has(n)) this.spots.set(n, hideouts(rows, box, n));
          this.safe.set(
            n,
            this.spots.get(n).filter((s) => cursor === null || reach(s[0], s[1], cursor, n) >= NEAR_MOVING),
          );
        }
        lure(fish, this.safe.get(n) ?? [], null, dt, rnd);
        swim(fish, dt, box, ...menace(fish, cursor, moving, threats), rnd);
      }
      const [x, y, glyph] = fish.cell;
      const anchor = { at: [x, y], space: "frame" };
      const content = { rows: [glyph], style: { fg: fish.color, bold: true } };
      if (fish.sprite === null || fish.sprite.id === null) fish.sprite = out.put({ frame, anchor, content, layer: "back" });
      else if (!same(fish.cell, fish.shown)) out.update(fish.sprite, { anchor, content }); // only what changed
      fish.shown = fish.cell;
    }
  }
}

let sight = null;
let o = null;
let tank = null;
let last = null;

/** The cursor in frame cells; null when hidden, not held, or off the view. */
function where() {
  const c = sight.cursor;
  const [w, h] = sight.size;
  if (!c || !(0 <= c[0] && c[0] < w && 0 <= c[1] && c[1] < h)) return null;
  const origin = sight.info?.origin ?? [0, 0];
  return [origin[0] + c[0], origin[1] + c[1]];
}

export function init(args) {
  o = options(args);
  sight = new Sight({ cursor: true });
  period(Math.round(BEAT * 1000));
}

export function tick(now) {
  const dt = last === null ? BEAT : (now - last) / 1000;
  last = now;
  if (!sight.look()) return;
  const frame = sight.info?.id ?? sight.frame;
  const box = view(sight.info?.origin ?? [0, 0], sight.size);
  const out = new Batch(call);
  tank ??= new Aquarium(o, { rnd: Math.random, out }, box, now / 1000);
  tank.env.out = out;
  try {
    tank.step({ frame, box, rows: sight.rows, changed: sight.changed }, where(), now / 1000, dt);
    out.send();
  } catch (e) {
    if (!(e instanceof PtyrcError) || e.code !== "not-usable") throw e;
    sight.touch();
  }
}

export function event(topic, data) {
  sight?.event(topic, data);
}
