// SPDX-License-Identifier: GPL-3.0-or-later
// The game of bricks: bricks, balls, rackets and the rules; no drawing.
//
// The twin's `bricks.Game`, step for step for the same `Twister`. Cells
// are keys (`arcade.key`), `box` is `[x0, y0, x1, y1]`, the view in
// frame cells, `walls` a set of keys. Rules count in steps of the game;
// `second` is the steps in a second.
//
// One departure: a racket under a falling brick goes to the first safe
// place of its search, the places of one move sorted as keys; the
// twin's are in the order of a Python set.

import { ACROSS, X, Y, key } from "./arcade.js";

export const BW = 3; // cells of a brick
export const SMALL = 3;
export const RACKET = 5;
export const WIDE = 7; // cells of a racket
export const VY = 0.5; // rows a move
export const VX = 1.0; // columns a move, at most
export const PACE = 12; // eighths of a move a ball makes in a step; sixteenths with a player
export const RALLY = 4;
export const KICK = 6;
export const MOVES = 2; // cells a racket moves in a step
export const REACH_X = 3;
export const HELD = 2;
export const PACED = 4;
export const PACES = [0.5, 3.0];
export const ARROWS = { left: [0, -1], right: [0, 1], up: [1, -1], down: [1, 1] }; // axis, way
export const BRICKS = 240;
export const PLOT = 48;
export const FILL = 12;
export const SLOW = 5;
export const KEPT = 0.55;
export const AGE = [50, 100];
export const LONG = 2;
export const BIG = [[3, 4], [6, 8]];
export const OFTEN = 0.2;
export const MEDIUM = [[[2, 2], [3, 5]], [[2, 2], [3, 3]], [[1, 1], [4, 6]], [[3, 4], [2, 2]]];
export const STYLES = ["full", "full", "full", "checker", "frame", "slope"];
export const FADE = 1;
export const CLEAR = 60;
export const TRIES = 60;
export const PICK = 10;
export const BANDS = 3;
export const ROOM = 40;
export const SHAPES = [
  [[0, 0], [BW, 0]],
  [[0, 0], [BW, 0]],
  [[0, 0], [BW, 0], [2 * BW, 0]],
  [[0, 0], [0, 1]],
  [[0, 0], [BW - 1, 1]],
  [[1, 0], [0, 1], [BW, 1]],
  [[0, 0], [BW, 0], [1, 1]],
];
export const STALE = 30;
export const SERVE = 6;
export const FORESEE = 100;
export const SPARE = 6;
export const AIM = 50;
export const FETCH = 48;
export const CHASE = 40;
export const THINK = 4;
export const SHOTS = 60;
export const EDGE = 0.5;
export const DENT = 0.3;
export const RISK = 0.25;
export const LOSS = 20.0;
export const GAP = 4;
export const REACH = 8;
export const AHEAD = 60;
export const HIDE = 0.02;
export const DUEL = 4;
export const SPAN = 2;
export const NEAR = 6;
export const AWAY = [2, 3];
export const QUIET = 160;
export const POINTS = 10;
export const BONUS = 2;
export const COLORS = 7;
export const KINDS = { plain: [1, 0.66], hard: [3, 0.14], loose: [1, 0.08], bumper: [2, 0.05], bomb: [1, 0.07] };
export const BONUSES = { multi: 12, wide: 2, twin: 2, fire: 2, small: 1, fast: 1 };
export const BAD = ["small", "fast"];
export const DROP = 0.4;
export const CROWD = 4;
export const BALLS = 5;
export const SPLIT = [0.3, 0.8];
export const DEAD = 60;
export const WARN = 6;
export const LAST = { wide: 320, small: 240, twin: 200, fast: 160, kick: 24 };

const STEPS = [ACROSS, -ACROSS, 1, -1];
const floor = Math.floor;
const int = Math.trunc;
const clamp = (v) => Math.max(-VX, Math.min(VX, v));

/** Python's `round` of a float to an integer: half to even. */
export function pyRound(v) {
  const f = floor(v);
  const d = v - f;
  if (d > 0.5) return f + 1;
  if (d < 0.5) return f;
  return f % 2 === 0 ? f : f + 1;
}

/** `a < b` of two keys of the twin's tuples: `[number | key, ...]`. */
function less(a, b) {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] < b[i];
  return false;
}

/** The least of `items` by `rank` (an array, compared as a tuple). */
function least(items, rank) {
  let best;
  let bestRank = null;
  for (const it of items) {
    const r = rank(it);
    if (bestRank === null || less(r, bestRank)) [best, bestRank] = [it, r];
  }
  return best;
}

/** One move of a ball: along x, then along y; an axis that would
 * enter a solid cell is reversed instead. `[x, y, vx, vy, hits]`, a
 * hit `[cell, flat]`: `flat` when hit from above or below. */
export function fly(x, y, vx, vy, solid) {
  const hits = [];
  const cx = floor(x);
  const cy = floor(y);
  let nx = x + vx;
  if (floor(nx) !== cx && solid(key(floor(nx), cy))) {
    hits.push([key(floor(nx), cy), false]);
    vx = -vx;
    nx = x;
  }
  let ny = y + vy;
  if (floor(ny) !== cy && solid(key(floor(nx), floor(ny)))) {
    hits.push([key(floor(nx), floor(ny)), true]);
    vy = -vy;
    ny = y;
  }
  return [nx, ny, vx, vy, hits];
}

const cellsOf = (x, y, w) => {
  const out = [];
  for (let i = 0; i < w; i++) out.push(key(x + i, y));
  return out;
};

const grids = new WeakMap();

/** The room of a racket (`Game.free`), a set of left cells in `box`,
 * on a grid of `box` and a cell around: the index of `(x, y)` is
 * `(x - x0 + 1) * h + y - y0 + 1`, ordered as the keys; `at(c)` that
 * of key `c`, -1 off the grid; `open`: 1 in the room; `keys`: the key
 * of each index. Kept while the room lives. */
function grid(room, box) {
  const [x0, y0, x1, y1] = [box[0] - 1, box[1] - 1, box[2] + 1, box[3] + 1];
  const [w, h] = [x1 - x0, y1 - y0];
  const open = new Uint8Array(w * h);
  const keys = new Float64Array(w * h);
  for (let x = x0, i = 0; x < x1; x++) {
    const k = key(x, y0); // a row down is one more
    for (let y = 0; y < h; y++) keys[i++] = k + y;
  }
  const at = (c) => {
    const x = X(c);
    const y = Y(c);
    return x0 <= x && x < x1 && y0 <= y && y < y1 ? (x - x0) * h + y - y0 : -1;
  };
  for (const c of room) open[at(c)] = 1;
  const n = w * h;
  grids.set(room, { h, at, open, keys, seen: new Int32Array(n), banned: new Int32Array(n), edge: new Int32Array(n), around: new Int32Array(n), stamp: 0 });
}

export class Brick {
  constructor(n, x, y, kind = "plain", lives = 1, color = 0, due = 0) {
    Object.assign(this, { n, x, y, kind, lives, color, due });
  }
  get cells() {
    return cellsOf(this.x, this.y, BW);
  }
}

export class Group {
  constructor(bricks, at, due, size = 0) {
    Object.assign(this, { bricks, at, due, size, dim: false });
  }
  get large() {
    return this.size > 0;
  }
}

export class Ball {
  constructor(n, x, y, vx, vy, owner = 0) {
    Object.assign(this, { n, x, y, vx, vy, owner, fire: 0, fast: 0, quiet: 0, combo: 0, phase: 0 });
  }
  get at() {
    return key(floor(this.x), floor(this.y));
  }
}

export class Racket {
  constructor(n, team, left = null) {
    Object.assign(this, { n, team, x: 0, y: 0, w: RACKET, span: 0, dead: 1, left, plan: null, think: 0, guard: 1, away: 0 });
  }
  get cells() {
    return cellsOf(this.x, this.y, this.w);
  }
}

export class Faller {
  constructor(n, x, y, color = 0) {
    Object.assign(this, { n, x, y, color });
  }
  get cells() {
    return cellsOf(this.x, this.y, BW);
  }
}

export class Capsule {
  constructor(n, x, y, kind) {
    Object.assign(this, { n, x, y, kind, rest: 0 });
  }
}

/** A bounded queue: `deque(maxlen=n)`. */
class Ring {
  constructor(n) {
    this.max = n;
    this.items = [];
  }
  push(v) {
    this.items.push(v);
    if (this.items.length > this.max) this.items.shift();
  }
}

/** Bricks, balls, rackets and the rules. `rules`: kinds of bricks;
 * `bonuses`: capsules; `multiplayer`: two teams; `player`: the team
 * whose racket `press` moves, the balls then half as fast. Each step
 * leaves its events in `events`, as the twin's. */
export class Game {
  constructor(rng, { rules = false, bonuses = false, multiplayer = false, second = 16, player = null } = {}) {
    this.rng = rng;
    this.player = player;
    this.gear = player === null ? 3 : 4;
    this.held = [[0, 0], [0, 0]];
    this.pace = MOVES;
    this.reach = MOVES;
    this.credit = 0.0;
    this.moved = new Ring(PACED * second);
    this.rules = rules;
    this.bonuses = bonuses;
    this.teams = 1 + (multiplayer ? 1 : 0);
    this.second = second;
    this.solid = new Map(); // cell: brick
    this.rows = new Map(); // y: bricks
    this.balls = [];
    this.rackets = [];
    for (let t = 0; t < this.teams; t++) this.rackets.push(new Racket(t + 1, t));
    this.fallers = [];
    this.capsules = [];
    this.score = new Array(this.teams).fill(0);
    this.best = new Array(this.teams).fill(0);
    this.events = [];
    this.steps = 0;
    this.serial = this.teams;
    this.groups = [];
    this.laid = [0, 0];
    this.spawnDue = 0;
    this.last = 0;
    this.home = [0.5, 0.5];
    this.lure = [];
    this.serveDue = SERVE;
    this.server = rng.randrange(this.teams);
    this.version = 0;
    this.shots = 0;
    this.fits = new Map(); // `_free`, by width
    this.spans = new Map(); // of `_room`, by width
    this.rally = new Ring(DUEL);
  }

  // -- the terrain

  bricks(box) {
    const out = [];
    for (let y = box[1]; y < box[3]; y++) for (const b of this.rows.get(y) ?? []) out.push(b);
    return out;
  }

  number() {
    return ++this.serial;
  }

  lay(brick) {
    this.version += 1;
    if (!this.rows.has(brick.y)) this.rows.set(brick.y, []);
    this.rows.get(brick.y).push(brick);
    for (const c of brick.cells) this.solid.set(c, brick);
  }

  lift(brick) {
    if (this.solid.get(key(brick.x, brick.y)) !== brick) return false;
    this.version += 1;
    const row = this.rows.get(brick.y);
    row.splice(row.indexOf(brick), 1);
    for (const c of brick.cells) this.solid.delete(c);
    return true;
  }

  group(box, walls, most = 32) {
    const rng = this.rng;
    const [x0, y0, x1, y1] = box;
    if (x1 - x0 < 3 * BW || y1 - y0 < 4) return [];
    const solid = this.solid;
    const keep = new Set();
    for (const r of this.rackets) if (!r.dead) for (const c of r.cells) for (let dy = -2; dy < 3; dy++) keep.add(c + dy);
    for (const ball of this.balls) {
      for (const p of this.foresee(ball, box, walls).path.slice(0, CLEAR)) {
        for (let dx = -1; dx < 2; dx++) keep.add(key(floor(p[1]) + dx, floor(p[2])));
      }
    }
    const free = (c) => {
      const x = X(c);
      const y = Y(c);
      return x0 <= x && x < x1 && y0 <= y && y < y1 && !walls.has(c) && !solid.has(c);
    };
    let found = null;
    let size = 0;
    const big = rng.random() < OFTEN;
    const all = big ? [BIG, ...MEDIUM] : MEDIUM;
    for (let n = 0; n < all.length; n++) {
      const [[r0, r1], [c0, c1]] = all[n];
      const rows = rng.randint(r0, Math.min(r1, Math.max(r0, y1 - y0 - 4)));
      const cols = rng.randint(c0, Math.max(c0, Math.min(c1, floor(most / rows))));
      const style = rows > 2 && cols > 3 ? rng.choice(STYLES) : "full";
      const shape = Game.wall(rows, cols, style, rng.random() < 0.5);
      if (rows * c0 <= most) found = this.spot([shape], box, free, keep, true);
      if (found !== null) {
        size = big && !n ? 2 : 1;
        break;
      }
    }
    if (found === null) found = this.spot(SHAPES, box, free, keep, false);
    if (found === null) return [];
    const [shape, x, y, at] = found;
    const tint = rng.randrange(COLORS);
    const out = [];
    const names = Object.keys(KINDS);
    const shares = names.map((k) => KINDS[k][1]);
    for (const [dx, dy] of shape) {
      let kind = "plain";
      let lives = 1;
      if (this.rules) {
        kind = rng.weighted(names, shares);
        lives = KINDS[kind][0];
      }
      const brick = new Brick(this.number(), x + dx, y + dy, kind, lives, (tint + dy) % COLORS);
      this.lay(brick);
      out.push(brick);
    }
    const life = rng.uniform(...AGE) + LONG * Math.max(0, out.length - 3);
    const due = this.steps + pyRound(life * this.second);
    for (const b of out) b.due = due;
    this.laid[size > 0 ? 1 : 0] += out.length;
    this.groups.push(new Group(out, at, due, size));
    this.events.push(["group", out]);
    return out;
  }

  /** Left cells `[dx, dy]` of the bricks of a wall of `rows` by
   * `cols`: full, or but for its first row (the last when `up`) a
   * checker, a frame or a slope. */
  static wall(rows, cols, style, up) {
    const out = [];
    for (let j = 0; j < rows; j++) {
      const d = up ? rows - 1 - j : j;
      for (let i = 0; i < cols; i++) {
        if (d && ((style === "checker" && (i + d) % 2) || (style === "frame" && 0 < i && i < cols - 1 && d < rows - 1) || (style === "slope" && !(d <= i && i < cols - d)))) continue;
        out.push([i * BW, j]);
      }
    }
    return out;
  }

  spot(shapes, box, free, keep, large) {
    const rng = this.rng;
    const [x0, y0, x1, y1] = box;
    const solid = this.solid;
    const others = this.groups.map((g) => g.at);
    let best = null;
    let found = 0;
    for (let t = 0; t < TRIES; t++) {
      const shape = rng.choice(shapes);
      const x = rng.randrange(x0, x1);
      const y = rng.randrange(y0 + 1, y1 - 1);
      const cells = [];
      for (const [dx, dy] of shape) for (let i = 0; i < BW; i++) cells.push(key(x + dx + i, y + dy));
      if (!cells.every((c) => free(c) && !keep.has(c))) continue;
      let need = 2;
      if (large) {
        const ay = Y(cells[0]);
        const by = Y(cells[cells.length - 1]);
        const xs = cells.map(X);
        const ax = Math.min(...xs);
        const bx = Math.max(...xs);
        let near = false;
        for (let cy = ay - 1; cy < by + 2 && !near; cy++) for (let cx = ax - 2; cx < bx + 3; cx++) if (solid.has(key(cx, cy))) near = true;
        if (near) continue;
        need = by > ay ? bx - ax + 1 : 2;
      }
      if (!Game.open(cells, free, need)) continue;
      let sx = 0;
      let sy = 0;
      for (const c of cells) (sx += X(c)), (sy += Y(c));
      const mx = sx / cells.length + 0.5;
      const my = sy / cells.length + 0.5;
      let far = 0.0;
      if (others.length) {
        far = Infinity;
        for (const [gx, gy] of others) {
          const a = mx - gx;
          const b = 2 * (my - gy);
          far = Math.min(far, a * a + b * b);
        }
      }
      const off = this.teams > 1 ? Math.abs(2 * my - y0 - y1) / (y1 - y0) : (my - y0) / (y1 - y0);
      const rank = [-Math.min(BANDS - 1, int(BANDS * off)), far];
      if (best === null || less(best[0], rank)) best = [rank, shape, x, y, [mx, my]];
      found += 1;
      if (found >= PICK) break;
    }
    return best ? best.slice(1) : null;
  }

  /** Whether a ball can get to `cells`: `need` free cells or more next
   * to them, that lead to `ROOM` free cells more. */
  static open(cells, free, need = 2) {
    const group = new Set(cells);
    const seen = new Set();
    for (const c of cells) {
      for (const d of STEPS) {
        const n = c + d;
        if (!group.has(n) && free(n)) seen.add(n);
      }
    }
    if (seen.size < need) return false;
    const room = need + ROOM;
    const queue = [...seen].sort((a, b) => a - b);
    for (let q = 0; q < queue.length && seen.size < room; q++) {
      for (const d of STEPS) {
        const n = queue[q] + d;
        if (!seen.has(n) && !group.has(n) && free(n)) {
          seen.add(n);
          queue.push(n);
        }
      }
    }
    return seen.size >= room;
  }

  age() {
    for (const group of [...this.groups]) {
      const left = group.bricks.filter((b) => this.solid.get(key(b.x, b.y)) === b);
      group.bricks = left;
      if (!left.length) this.groups.splice(this.groups.indexOf(group), 1);
      else if (this.steps >= group.due) {
        for (const b of left) this.lift(b);
        this.groups.splice(this.groups.indexOf(group), 1);
        this.events.push(["gone", left]);
      } else if (!group.dim && this.steps >= group.due - FADE * this.second) {
        group.dim = true;
        this.events.push(["fade", left]);
      }
    }
  }

  spawn(box, walls) {
    if (this.steps < this.spawnDue) return;
    const [x0, y0, x1, y1] = box;
    const most = Math.max(8, Math.min(BRICKS, floor(((x1 - x0) * (y1 - y0)) / PLOT)));
    const here = this.bricks(box).length;
    if (here >= most) return;
    const laid = this.group(box, walls, most).length || 2;
    const gap = (laid * FILL * this.second) / most;
    const slow = here >= KEPT * most ? SLOW : 1;
    this.spawnDue = this.steps + Math.max(1, pyRound(gap * slow));
  }

  reset(box) {
    const [x0, y0, x1, y1] = box;
    const gone = this.bricks(box);
    this.solid.clear();
    this.rows.clear();
    this.groups = [];
    this.version += 1;
    this.balls = [];
    this.capsules = [];
    this.fallers = [];
    this.rackets = this.rackets.filter((r) => r.left === null);
    for (const r of this.rackets) (r.dead = 1), (r.plan = null);
    const fx = this.rng.uniform(0.1, 0.9);
    this.home = [fx, this.rng.uniform(0.2, 0.8)];
    this.last = this.spawnDue = this.steps;
    this.serveDue = this.steps + SERVE;
    if (gone.length) this.events.push(["gone", gone]);
    this.events.push(["reset", key(floor((x0 + x1) / 2), floor((y0 + y1) / 2))]);
  }

  // -- a step

  step(box, walls) {
    const [x0, y0, x1, y1] = box;
    const inside = (c) => {
      const x = X(c);
      const y = Y(c);
      return x0 <= x && x < x1 && y0 <= y && y < y1;
    };
    this.events = [];
    this.steps += 1;
    for (const b of this.bricks(box).filter((b) => b.cells.some((c) => walls.has(c)))) this.lift(b);
    this.balls = this.balls.filter((b) => inside(b.at) && !walls.has(b.at));
    this.fallers = this.fallers.filter((f) => inside(key(f.x, f.y)));
    this.capsules = this.capsules.filter((c) => inside(key(c.x, c.y)) && !walls.has(key(c.x, c.y)));
    for (const r of this.rackets) if (!r.dead && !r.cells.every((c) => inside(c) && !this.solid.has(c))) r.dead = 1;
    this.age();
    if (this.steps - this.last >= STALE * this.second) this.reset(box);
    this.spawn(box, walls);
    this.wake(box, walls);
    this.play(box);
    this.steer(box, walls);
    this.serve(box, walls);
    for (const ball of [...this.balls]) this.roll(ball, box, walls);
    if (this.rules) this.fall(box, walls);
    if (this.bonuses) this.drift(box, walls);
    this.best = this.best.map((b, i) => Math.max(b, this.score[i]));
  }

  /** The cells of the rackets something can touch: not under text. */
  cellsOf(walls) {
    const out = new Map();
    for (const r of this.rackets) if (!r.dead) for (const c of r.cells) if (!walls.has(c)) out.set(c, r);
    return out;
  }

  blocked(but = null) {
    const out = new Set();
    for (const b of this.balls) for (let dx = -2; dx < 3; dx++) out.add(b.at + dx * ACROSS);
    for (const o of this.rackets) if (o !== but && !o.dead) for (const c of o.cells) out.add(c);
    return out;
  }

  /** Left cells of the places where a racket of `w` cells fits: in
   * the view, off the bricks and `taken`. */
  room(box, w, taken) {
    const [x0, y0, x1, y1] = box;
    let kept = this.spans.get(w);
    if (!kept || kept[0].some((v, i) => v !== box[i])) {
      const cells = [];
      for (let y = y0; y < y1; y++) for (let x = x0; x < x1 - w + 1; x++) cells.push(key(x, y));
      kept = [box, cells];
      this.spans.set(w, kept);
    }
    const shut = new Set();
    for (let y = y0; y < y1; y++) for (const b of this.rows.get(y) ?? []) for (let x = b.x; x < b.x + BW; x++) for (let i = 0; i < w; i++) shut.add(key(x - i, y));
    for (const c of taken) for (let i = 0; i < w; i++) shut.add(c - i * ACROSS);
    return new Set(kept[1].filter((c) => !shut.has(c)));
  }

  wake(box, walls) {
    const [x0, y0, x1, y1] = box;
    for (const r of [...this.rackets]) {
      if (r.left !== null) {
        r.left -= 1;
        if (r.left <= 0) {
          this.rackets.splice(this.rackets.indexOf(r), 1);
          continue;
        }
      }
      if (r.span) {
        r.span -= 1;
        if (!r.span && !r.dead) r.span = this.size(r, RACKET, box) ? 0 : 1;
      }
      if (!r.dead) continue;
      r.dead -= 1;
      if (r.dead) continue;
      r.w = RACKET;
      r.span = 0;
      r.plan = null;
      const room = this.room(box, r.w, this.blocked(r));
      const [fx, fy] = this.home;
      const hx = x0 + int((x1 - x0 - r.w) * fx);
      let hy = y0 + int(((y1 - y0) * (r.team + fy)) / this.teams);
      if (this.teams === 1) hy = y0 + int(((y1 - y0) * (1 + fy)) / 2);
      if (r.left !== null) {
        const up = this.teams === 1 || !(r.n % 2);
        hy += floor((y1 - y0) / 4) * (up ? -1 : 1);
      }
      if (!room.size) {
        r.dead = 4;
        continue;
      }
      const near = [...room].filter((c) => Math.abs(X(c) - hx) <= 24 && Math.abs(Y(c) - hy) <= 6);
      const at = least(near.length ? near : room, (c) => [Game.far(c, hx, hy, r.w, walls), c]);
      r.x = X(at);
      r.y = Y(at);
    }
  }

  static far(c, hx, hy, w, walls) {
    let hidden = 0;
    for (let i = 0; i < w; i++) if (walls.has(c + i * ACROSS)) hidden += 1;
    return 4 * Math.abs(Y(c) - hy) + Math.abs(X(c) - hx) + 12 * hidden;
  }

  serve(box, walls) {
    if (this.balls.length) {
      this.serveDue = this.steps + SERVE;
      return;
    }
    const rackets = this.rackets.filter((r) => !r.dead);
    if (this.steps < this.serveDue || !rackets.length) return;
    const [x0, y0, x1, y1] = box;
    const taken = this.blocked();
    const free = (x, y) => x0 <= x && x < x1 && y0 < y && y < y1 - 1 && !walls.has(key(x, y)) && !this.solid.has(key(x, y)) && !taken.has(key(x, y));
    rackets.sort((a, b) => (a.team !== this.server) - (b.team !== this.server) || a.n - b.n);
    const mid = floor(rackets[0].w / 2);
    for (const r of rackets) {
      const far = this.teams > 1 && 2 * r.y < y0 + y1 ? 1 : -1;
      const order = [...Array(r.w).keys()].sort((a, b) => Math.abs(a - mid) - Math.abs(b - mid));
      for (const dy of [far, -far]) {
        for (const dx of order) {
          if (free(r.x + dx, r.y + dy)) {
            this.ball(r.x + dx, r.y + dy, dy, r.team);
            return;
          }
        }
      }
    }
    const r = rackets[0];
    const cells = [];
    for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) if (free(x, y)) cells.push(key(x, y));
    if (cells.length) {
      const c = least(cells, (c) => [2 * Math.abs(Y(c) - r.y) + Math.abs(X(c) - r.x - mid), c]);
      this.ball(X(c), Y(c), Y(c) > r.y ? 1 : -1, r.team);
    }
  }

  ball(x, y, dy, team) {
    const rng = this.rng;
    const sign = rng.choice([-1, 1]);
    const vx = sign * rng.uniform(0.2, VX / 2);
    const ball = new Ball(this.number(), x + 0.5, y + 0.5, vx, dy * VY, team);
    this.balls.push(ball);
    this.events.push(["serve", ball.at]);
  }

  // -- the balls

  terrain(box, walls, fire = false) {
    const [x0, y0, x1, y1] = box;
    const solid = this.solid;
    const shut = this.teams === 1;
    return (c) => {
      const y = Y(c);
      if (!(y0 <= y && y < y1)) return shut && y < y0;
      const x = X(c);
      return walls.has(c) || !(x0 <= x && x < x1) || (!fire && solid.has(c));
    };
  }

  roll(ball, box, walls) {
    const [, y0, , y1] = box;
    const terrain = this.terrain(box, walls, !!ball.fire);
    const rackets = this.cellsOf(walls);
    const hard = (c) => terrain(c) || rackets.has(c);
    ball.phase += PACE + Math.min(ball.combo, RALLY) + (ball.fast ? KICK : 0);
    const moves = ball.phase >> this.gear;
    ball.phase &= (1 << this.gear) - 1;
    for (let m = 0; m < moves; m++) {
      let hits;
      [ball.x, ball.y, ball.vx, ball.vy, hits] = fly(ball.x, ball.y, ball.vx, ball.vy, hard);
      for (const [cell, flat] of hits) {
        const r = rackets.get(cell);
        if (r !== undefined) {
          ball.owner = r.team;
          ball.combo = 0;
          if (flat) {
            const half = r.w / 2;
            ball.vx = clamp(((ball.x - r.x - half) / half) * VX);
          }
          this.events.push(["bounce", cell, r]);
          this.duel(r, ball);
        } else if (this.solid.has(cell)) this.strike(this.solid.get(cell), ball, cell);
      }
      if (ball.fire && this.solid.has(ball.at)) this.smash(this.solid.get(ball.at), ball, ball.at);
      const y = Y(ball.at);
      if (!(y0 <= y && y < y1)) {
        this.out(ball, box);
        return;
      }
    }
    ball.fire = Math.max(0, ball.fire - 1);
    ball.fast = Math.max(0, ball.fast - 1);
    ball.quiet += 1;
    if (ball.quiet % QUIET === 0) {
      if (ball.quiet >= 3 * QUIET && this.balls.length > 1) this.balls.splice(this.balls.indexOf(ball), 1);
      else {
        const sign = this.rng.choice([-1, 1]);
        ball.vx = sign * this.rng.uniform(0.3, VX);
      }
    }
  }

  duel(racket, ball) {
    if (this.teams < 2) return;
    const rally = this.rally;
    rally.push([this.steps, racket, ball.n]);
    const all = rally.items;
    const hits = all.map((e) => e[1]);
    if (
      hits.length < DUEL ||
      this.steps - all[0][0] > SPAN * this.second ||
      hits.some((a, i) => i > 0 && hits[i - 1].team === a.team) ||
      all.some((e) => e[2] !== ball.n) ||
      Math.abs(hits[hits.length - 1].y - hits[hits.length - 2].y) > NEAR
    )
      return;
    const mine = this.mine;
    const two = hits.slice(-2).filter((r) => r !== mine);
    const one = this.rng.choice(two.length ? two : hits.slice(-1));
    if (one === mine) return;
    one.away = this.steps + pyRound(this.rng.uniform(...AWAY) * this.second);
    one.plan = null;
    rally.items = [];
    this.events.push(["retreat", key(one.x + floor(one.w / 2), one.y), one]);
  }

  // -- the player

  get mine() {
    if (this.player === null) return null;
    return this.rackets.find((r) => r.team === this.player && r.left === null) ?? null;
  }

  /** An arrow: the racket of the player goes that way for `HELD`
   * steps; the opposite one stops it. Anything else is ignored. */
  press(name) {
    if (!Object.hasOwn(ARROWS, name)) return;
    const [axis, way] = ARROWS[name];
    const [was, left] = this.held[axis];
    this.held[axis] = left && was === -way ? [0, 0] : [way, HELD];
  }

  play(box) {
    const r = this.mine;
    const moves = [];
    this.held.forEach(([way, left], axis) => {
      if (!left) return;
      this.held[axis] = [way, left - 1];
      if (axis === 0) for (let i = 0; i < REACH_X; i++) moves.push(way * ACROSS);
      else moves.push(way);
    });
    let done = 0;
    if (r !== null && !r.dead && moves.length) {
      const room = this.free(box, r.w);
      const bad = this.bad(r);
      for (const d of moves) {
        const there = key(r.x, r.y) + d;
        if (room.has(there) && !bad.has(there)) {
          r.x = X(there);
          r.y = Y(there);
          done += 1;
        }
      }
    }
    this.pacing(done);
  }

  /** Left cells `r` cannot be at: `blocked` by its width. */
  bad(r) {
    const out = new Set();
    for (const c of this.blocked(r)) for (let i = 0; i < r.w; i++) out.add(c - i * ACROSS);
    return out;
  }

  pacing(done) {
    if (this.player !== null && this.teams > 1) {
      this.moved.push(done);
      const m = this.moved.items;
      if (m.length === this.moved.max) {
        let sum = 0;
        for (const v of m) sum += v;
        this.pace = Math.min(PACES[1], Math.max(PACES[0], sum / m.length));
      }
    }
    this.credit += this.pace;
    this.reach = int(this.credit);
    this.credit -= this.reach;
  }

  out(ball, box) {
    this.balls.splice(this.balls.indexOf(ball), 1);
    const top = Y(ball.at) < box[1];
    const team = this.teams === 1 || top ? 0 : 1;
    const at = key(X(ball.at), top ? box[1] : box[3] - 1);
    if (this.balls.length) {
      this.events.push(["out", at, team]);
      return;
    }
    this.score[team] = 0;
    this.server = team;
    this.events.push(["lost", at, team]);
  }

  strike(brick, ball, cell) {
    ball.quiet = 0;
    if (brick.kind === "loose") {
      this.lift(brick);
      this.fallers.push(new Faller(brick.n, brick.x, brick.y, brick.color));
      this.scored(ball);
      if (ball.combo >= 2) this.events.push(["combo", cell, ball.combo]);
      return;
    }
    if (brick.kind === "bumper") {
      const sign = this.rng.choice([-1, 1]);
      ball.vx = sign * this.rng.uniform(0.5, VX);
      ball.fast = Math.max(ball.fast, LAST.kick);
      this.events.push(["bump", cell]);
    }
    brick.lives -= 1;
    if (brick.lives > 0) {
      this.score[ball.owner] += floor(POINTS / 2);
      this.events.push(["hit", cell, brick]);
    } else this.smash(brick, ball, cell);
  }

  scored(ball) {
    ball.combo += 1;
    this.last = this.steps;
    this.score[ball.owner] += POINTS + BONUS * Math.min(ball.combo - 1, 5);
  }

  /** What the blast of `brick` reaches. */
  near(brick) {
    const out = new Set();
    for (let y = brick.y - 1; y < brick.y + 2; y++) {
      for (let x = brick.x - BW; x < brick.x + 2 * BW; x++) {
        const b = this.solid.get(key(x, y));
        if (b) out.add(b);
      }
    }
    return out;
  }

  smash(brick, ball, cell) {
    const at = cell;
    const team = ball.owner;
    const queue = [[brick, cell]];
    for (let q = 0; q < queue.length; q++) {
      const [b, c] = queue[q];
      if (!this.lift(b)) continue;
      this.scored(ball);
      this.events.push(["break", c, b, team]);
      if (b.kind === "bomb") {
        this.events.push(["boom", key(b.x + floor(BW / 2), b.y)]);
        for (const o of [...this.near(b)].sort((p, q) => p.n - q.n)) queue.push([o, key(o.x + floor(BW / 2), o.y)]);
      } else if (this.bonuses && this.rng.random() < DROP) {
        const kind = this.rng.weighted(Object.keys(BONUSES), Object.values(BONUSES));
        if (kind !== "multi" || this.balls.length < CROWD) this.capsules.push(new Capsule(this.number(), X(c), Y(c), kind));
      }
    }
    if (ball.combo >= 2) this.events.push(["combo", at, ball.combo]);
  }

  // -- the rackets

  /** A ball flown `steps` steps from `state` (`x, y, vx, vy, phase,
   * combo, fast, fire`) as `roll` does, on a copy of the terrain.
   * `[gain, side, step, x, y, last bounce]`: the twin's `_trace`. */
  trace(state, box, walls, steps, { at = 0, dents = null, racket = [0, -1, 0], path = null, marks = null } = {}) {
    const [x0, y0, x1, y1] = box;
    let [x, y, vx, vy, phase, combo, fast, fire] = state;
    const solid = this.solid;
    dents = dents ?? new Map();
    const [lx, row, w] = racket;
    const end = lx + w;
    let gain = 0.0;
    let last = 0;
    let index = 0;
    const now = this.steps + at - 1;
    const shut = this.teams === 1;
    const gear = this.gear;
    let k = 0;
    const there = (b) => (dents.get(b) ?? 1) > 0 && !(0 < b.due && b.due <= now + k);
    const hard = (c) => {
      const cy = Y(c);
      if (!(y0 <= cy && cy < y1)) return shut && cy < y0;
      const cx = X(c);
      if (walls.has(c) || !(x0 <= cx && cx < x1) || (cy === row && lx <= cx && cx < end)) return true;
      if (fire) return false;
      const b = solid.get(c);
      return b !== undefined && there(b);
    };
    const smash = (brick, weight) => {
      const queue = [brick];
      for (let q = 0; q < queue.length; q++) {
        const b = queue[q];
        if (!there(b)) continue;
        dents.set(b, 0);
        gain += weight;
        combo += 1;
        if (marks !== null) marks.push([index, b, 0]);
        if (b.kind === "bomb") for (const o of this.near(b)) queue.push(o);
      }
    };
    for (k = 1; k < steps + 1; k++) {
      phase += PACE + Math.min(combo, RALLY) + (fast ? KICK : 0);
      const moves = phase >> gear;
      phase &= (1 << gear) - 1;
      const weight = Math.max(0.25, 1 - (at + k) / 200);
      for (let m = 0; m < moves; m++) {
        // `fly`, its hits kept in two cells, null when none: no list.
        const cx = floor(x);
        const cy = floor(y);
        let across = null;
        let down = null;
        let nx = x + vx;
        if (floor(nx) !== cx && hard(key(floor(nx), cy))) {
          across = key(floor(nx), cy);
          vx = -vx;
          nx = x;
        }
        let ny = y + vy;
        if (floor(ny) !== cy && hard(key(floor(nx), floor(ny)))) {
          down = key(floor(nx), floor(ny));
          vy = -vy;
          ny = y;
        }
        x = nx;
        y = ny;
        index += 1;
        for (let j = 0; j < 2; j++) {
          const cell = j === 0 ? across : down;
          if (cell === null) continue;
          const flat = j === 1;
          last = index;
          const cx = X(cell);
          if (Y(cell) === row && lx <= cx && cx < end && !walls.has(cell)) {
            combo = 0;
            if (flat) vx = clamp(((x - lx - w / 2) / (w / 2)) * VX);
            continue;
          }
          const b = solid.get(cell);
          if (b === undefined || !there(b)) continue;
          let left = (dents.get(b) ?? b.lives) - 1;
          if (b.kind === "loose") left = 0;
          else if (b.kind === "bumper") fast = Math.max(fast, LAST.kick);
          if (left > 0) {
            dents.set(b, left);
            gain += DENT * weight;
            if (marks !== null) marks.push([index, b, left]);
          } else smash(b, weight);
        }
        if (fire) {
          const b = solid.get(key(floor(x), floor(y)));
          if (b !== undefined && there(b)) smash(b, weight);
        }
        if (path !== null) path.push([k, x, y, vx, vy, gain, phase, fast, fire]);
        if (!(y0 <= floor(y) && floor(y) < y1)) return [gain, y < y0 ? -1 : 1, k, x, y, last];
      }
      fast = Math.max(0, fast - 1);
      fire = Math.max(0, fire - 1);
    }
    return [gain, null, steps, x, y, last];
  }

  foresee(ball, box, walls) {
    const path = [[0, ball.x, ball.y, ball.vx, ball.vy, 0.0, 0, 0, 0]];
    const marks = [];
    const state = [ball.x, ball.y, ball.vx, ball.vy, ball.phase, ball.combo, ball.fast, ball.fire];
    const t = this.trace(state, box, walls, FORESEE, { path, marks });
    const entries = [];
    for (let i = 0; i < path.length - 2; i++) if (floor(path[i + 1][2]) !== floor(path[i][2])) entries.push(i);
    return { ball, path, marks, last: t[5], side: t[1], entries };
  }

  shot(r, flight, i, place, box, walls) {
    this.shots += 1;
    const before = flight.path[i];
    const after = flight.path[i + 1];
    const lx = X(place);
    const row = Y(place);
    const half = r.w / 2;
    const vx = clamp(((after[1] - lx - half) / half) * VX);
    const state = [after[1], before[2], vx, -before[4], after[6], 0, after[7], after[8]];
    const dents = new Map();
    for (const m of flight.marks) if (m[0] <= i) dents.set(m[1], m[2]);
    const [gain, side, k, x, y] = this.trace(state, box, walls, AIM, { at: after[0], dents, racket: [lx, row, r.w] });
    if (side === null) {
      if (gain) return gain;
      let far = 0;
      if (this.lure.length) {
        far = Infinity;
        for (const [bx, by] of this.lure) far = Math.min(far, Math.abs(x - bx) + 2 * Math.abs(y - by));
      }
      return -Math.min(0.2, far / 1000);
    }
    if (this.teams > 1 && side > 0 !== r.team > 0) return gain + 0.05;
    const edge = side < 0 ? box[1] : box[3] - 1;
    const far = Math.abs(x - lx - half) + Math.abs(edge - row);
    return gain - (far + SPARE <= int(this.pace * k) ? RISK : LOSS);
  }

  /** `room` with nothing taken; kept while the terrain is. */
  free(box, w) {
    const kept = this.fits.get(w);
    const k = `${box.join(",")}:${this.version}`;
    if (kept && kept[0] === k) return kept[1];
    const room = this.room(box, w, []);
    grid(room, box);
    this.fits.set(w, [k, room]);
    return room;
  }

  /** Moves from `r` to each place it fits, `depth` at most; with
   * `goals` it ends once it has them all, or the `first` of them.
   * `[dist, whole]`. */
  reachOf(r, room, depth, goals = null, first = false) {
    const bad = this.bad(r);
    const start = key(r.x, r.y);
    const dist = new Map([[start, 0]]);
    let left = null;
    if (goals !== null) {
      left = new Set();
      for (const g of goals) if (room.has(g) && !bad.has(g) && g !== start) left.add(g);
      if (!left.size || (first && goals.has(start))) return [dist, false];
    }
    // On the grid of the room: a cell an index, `seen`, `banned` this
    // call's when they hold its stamp.
    const g = grids.get(room);
    const { open, keys, seen, banned, h } = g;
    const stamp = ++g.stamp;
    for (const c of bad) {
      const i = g.at(c);
      if (i >= 0) banned[i] = stamp;
    }
    // A layer sorted in place (no comparator to call): `edge`, then
    // `around`, `n` long.
    let [edge, around] = [g.edge, g.around];
    let ne = 0;
    const i0 = g.at(start);
    if (i0 >= 0) {
      seen[i0] = stamp;
      edge[ne++] = i0;
    }
    for (let moves = 1; moves < depth + 1; moves++) {
      let n = 0;
      for (let e = 0; e < ne; e++) {
        const i = edge[e];
        // As the keys: a row down, up, a column right, left.
        for (let k = 0; k < 4; k++) {
          const c = k === 0 ? i + 1 : k === 1 ? i - 1 : k === 2 ? i + h : i - h;
          if (open[c] === 1 && seen[c] !== stamp && banned[c] !== stamp) {
            seen[c] = stamp;
            around[n++] = c;
          }
        }
      }
      if (!n) break;
      around.subarray(0, n).sort();
      for (let e = 0; e < n; e++) dist.set(keys[around[e]], moves);
      [edge, around, ne] = [around, edge, n];
      if (left !== null) {
        let found = false;
        for (let e = 0; e < ne; e++) if (left.delete(keys[edge[e]])) found = true;
        if (!left.size || (first && found)) return [dist, false];
      }
    }
    return [dist, true];
  }

  static spots(r, flights, steps) {
    const out = new Set();
    for (const f of flights) {
      const path = f.path;
      for (const i of f.entries) {
        const [k, x, y] = path[i + 1];
        if (k > steps) break;
        for (let lx = floor(x) - r.w + 1; lx < floor(x) + 1; lx++) out.add(key(lx, floor(y)));
      }
    }
    return out;
  }

  steer(box, walls) {
    const mine = this.mine;
    const alive = this.rackets.filter((r) => !r.dead && r !== mine);
    if (!alive.length) return;
    this.lure = this.bricks(box).map((b) => [b.x + BW / 2, b.y]);
    this.shots = 0;
    const flights = new Map(this.balls.map((b) => [b.n, this.foresee(b, box, walls)]));
    const reach = new Map(); // racket: [dist, depth, whole, why]
    const ways = (r, depth, goals = null, first = false, why = "") => {
      let kept = reach.get(r);
      if (!kept || !((kept[2] && kept[1] >= depth) || (why && kept[3] === why))) {
        const [dist, whole] = this.reachOf(r, this.free(box, r.w), depth, goals, first);
        kept = [dist, depth, whole, why];
        reach.set(r, kept);
      }
      return kept[0];
    };
    const way = (r, goal, depth) => {
      const kept = reach.get(r);
      const dist = kept && kept[0].has(goal) ? kept[0] : ways(r, depth);
      const out = [];
      let left = dist.get(goal) ?? 0;
      let c = goal;
      while (left) {
        out.push(c);
        left -= 1;
        c = [c - ACROSS, c + ACROSS, c - 1, c + 1].find((n) => dist.get(n) === left);
      }
      return out.reverse();
    };
    const blockedOn = (path, n, w, taken) => path.slice(0, n).some((c) => cellsOf(X(c), Y(c), w).some((t) => taken.has(t)));
    for (const r of alive) {
      const plan = r.plan;
      if (plan === null) continue;
      if (!this.holds(r, flights, box)) {
        r.plan = null;
        continue;
      }
      if (blockedOn(plan.way, this.reach, r.w, this.blocked(r))) {
        const depth = int(this.pace * (plan.step - this.steps));
        if (ways(r, depth, new Set([plan.place])).has(plan.place)) plan.way = way(r, plan.place, depth);
        else r.plan = null;
      }
    }
    const goals = new Map();
    const slack = new Map();
    const minded = new Set();
    const all = [...flights.values()];
    const threats = all.filter((f) => f.side !== null);
    threats.sort((a, b) => a.path[a.path.length - 1][0] - b.path[b.path.length - 1][0] || a.ball.n - b.ball.n);
    const aims = new Map();
    const depth = threats.length ? int(this.pace * threats[threats.length - 1].path[threats[threats.length - 1].path.length - 1][0]) + SPARE : 0;
    for (const flight of threats) {
      const team = this.teams === 1 || flight.side < 0 ? 0 : 1;
      const free = alive.filter((r) => r.team === team && !goals.has(r));
      const keeper = free.filter((r) => r.plan && r.plan.ball === flight.ball.n);
      if (keeper.length) {
        const r = keeper[0];
        const plan = r.plan;
        goals.set(r, plan.way);
        slack.set(r, int(this.pace * (plan.step - this.steps)) - plan.way.length);
        minded.add(flight.ball.n);
        continue;
      }
      const plans = [];
      for (const r of free) {
        if (!aims.has(r)) {
          const own = threats.filter((f) => f.side > 0 === r.team > 0);
          aims.set(r, Game.spots(r, this.teams > 1 ? own : threats, FORESEE));
        }
        const dist = ways(r, depth, aims.get(r), false, "stop");
        const found = this.intercept(r, flight, dist, box, walls);
        if (found !== null) plans.push([found[0], r.n, r, found]);
      }
      if (!plans.length) continue;
      const [, , r, [late, goal, i]] = least(plans, (p) => [p[0], p[1]]);
      goals.set(r, way(r, goal, depth));
      r.plan = null;
      slack.set(r, -late);
      minded.add(flight.ball.n);
      if (late < 0) {
        const [k, x] = flight.path[i + 1];
        r.plan = { ball: flight.ball.n, place: goal, step: this.steps + k, x, w: r.w, way: goals.get(r) };
      }
    }
    const danger = new Set();
    for (const f of this.fallers) for (let x = f.x; x < f.x + BW; x++) for (let y = f.y; y < f.y + WARN; y++) danger.add(key(x, y));
    const good = this.capsules.filter((c) => !BAD.includes(c.kind));
    const treats = (r) => {
      const out = new Set();
      for (const cap of good) for (let y = cap.y; y < cap.y + 4; y++) for (let x = cap.x - r.w + 1; x < cap.x + 1; x++) out.add(key(x, y));
      return out;
    };
    const nearest = (dist, cells) => {
      let best = null;
      for (const c of cells) if (dist.has(c) && (best === null || less([dist.get(c), c], best))) best = [dist.get(c), c];
      return best;
    };
    const endangered = (r) => r.cells.some((c) => danger.has(c));
    for (const r of alive) {
      const away = r.away > this.steps;
      if (goals.has(r)) {
        const spare = slack.get(r) - SPARE;
        if (away || !good.length || spare < 2 || endangered(r)) continue;
        const t = treats(r);
        const spot = nearest(ways(r, floor(spare / 2), t, true), t);
        if (spot) {
          goals.set(r, way(r, spot[1], floor(spare / 2)));
          r.plan = null;
        }
        continue;
      }
      if (endangered(r)) {
        let goal = null;
        for (const c of ways(r, 4 * WARN).keys()) {
          if (!cellsOf(X(c), Y(c), r.w).some((t) => danger.has(t))) {
            goal = c;
            break;
          }
        }
        if (goal !== null) {
          goals.set(r, way(r, goal, 4 * WARN));
          r.plan = null;
          continue;
        }
      } else if (good.length && !away) {
        const t = treats(r);
        const spot = nearest(ways(r, FETCH, t, true), t);
        if (spot) {
          goals.set(r, way(r, spot[1], FETCH));
          r.plan = null;
          continue;
        }
      }
      if (r.plan !== null && minded.has(r.plan.ball)) r.plan = null;
      if (r.plan === null && this.steps >= r.think && !away) {
        r.think = this.steps + THINK;
        const d = int(this.pace * CHASE);
        const rest = all.filter((f) => !minded.has(f.ball.n));
        const dist = ways(r, d, Game.spots(r, rest, CHASE));
        const found = this.chase(r, rest, dist, box, walls);
        if (found !== null) {
          const [flight, goal, i] = found;
          const [k, x] = flight.path[i + 1];
          const path = way(r, goal, d);
          r.plan = { ball: flight.ball.n, place: goal, step: this.steps + k, x, w: r.w, way: path };
        }
      }
      if (r.plan !== null) {
        goals.set(r, r.plan.way);
        minded.add(r.plan.ball);
      }
    }
    for (const r of alive) {
      if (goals.has(r)) continue;
      let rest = all.filter((f) => !minded.has(f.ball.n));
      if (!rest.length) rest = all;
      const goal = r.away > this.steps ? this.den(r, ways(r, REACH), box, walls) : this.wait(r, rest, ways(r, REACH), box, walls);
      if (goal !== null) goals.set(r, way(r, goal, REACH));
    }
    for (const [r, path] of goals) {
      const taken = this.blocked(r);
      let done = 0;
      for (const c of path.slice(0, this.reach)) {
        if (cellsOf(X(c), Y(c), r.w).some((t) => taken.has(t))) break;
        r.x = X(c);
        r.y = Y(c);
        done += 1;
      }
      if (r.plan !== null && r.plan.way === path) path.splice(0, done);
    }
  }

  holds(r, flights, box) {
    const plan = r.plan;
    const flight = flights.get(plan.ball);
    const k = plan.step - this.steps;
    if (!flight || k < 1 || plan.w !== r.w) return false;
    const room = this.free(box, r.w);
    const here = plan.way.length ? plan.way[0] : plan.place;
    if (Math.abs(X(here) - r.x) + Math.abs(Y(here) - r.y) !== (plan.way.length ? 1 : 0) || plan.way.length > int(this.pace * k) || !plan.way.every((c) => room.has(c))) return false;
    const path = flight.path;
    return flight.entries.some((i) => path[i + 1][0] === k && path[i + 1][1] === plan.x && floor(path[i + 1][2]) === Y(plan.place));
  }

  intercept(r, flight, dist, box, walls) {
    const path = flight.path;
    const found = []; // [rank, -i, places in time]
    const late = []; // [moves short, -i, place]
    for (const i of flight.entries) {
      const p = path[i + 1];
      const k = p[0];
      const x = p[1];
      const y = p[2];
      const spots = [];
      for (let lx = floor(x) - r.w + 1; lx < floor(x) + 1; lx++) if (dist.has(key(lx, floor(y)))) spots.push(key(lx, floor(y)));
      if (!spots.length) continue;
      const due = int(this.pace * k) - 1;
      const timely = spots.filter((c) => dist.get(c) <= due);
      if (timely.length) {
        const spare = Math.max(...timely.map((c) => due - dist.get(c)));
        const rank = i < flight.last ? 2 : spare < SPARE ? 1 : 0;
        found.push([rank, -i, timely]);
      } else {
        const c = least(spots, (c) => [dist.get(c), c]);
        late.push([dist.get(c) - due, -i, c]);
      }
    }
    if (!found.length) {
      if (!late.length) return null;
      const [short, i, c] = least(late, (l) => l);
      return [short, c, -i];
    }
    found.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    let best = null;
    const rows = this.shots < SHOTS ? 3 : 1;
    for (const [rank, ni, spots] of found.slice(0, rows)) {
      if (rank !== found[0][0]) continue;
      const i = -ni;
      for (const c of spots) {
        let worth = path[i][5] + this.shot(r, flight, i, c, box, walls);
        worth -= HIDE * Game.hidden(c, r.w, walls);
        const k = [dist.get(c) / 100 - worth, c];
        if (best === null || less(k, best[0])) best = [k, i, c];
      }
    }
    const [, i, goal] = best;
    return [dist.get(goal) - int(this.pace * path[i + 1][0]), goal, i];
  }

  chase(r, flights, dist, box, walls) {
    const near = (f) => [Math.abs(f.ball.x - r.x) + 2 * Math.abs(f.ball.y - r.y), f.ball.n];
    let best = null;
    const order = flights.map((f) => [near(f), f]).sort((a, b) => (less(a[0], b[0]) ? -1 : less(b[0], a[0]) ? 1 : 0));
    for (const [, flight] of order.slice(0, this.shots < SHOTS ? 2 : 0)) {
      const path = flight.path;
      const share = flight.ball.owner === r.team ? 1.0 : 0.5;
      const rows = [];
      for (const i of flight.entries) {
        const [k, x, y] = path[i + 1];
        if (k > CHASE) break;
        const spots = [];
        for (let lx = floor(x) - r.w + 1; lx < floor(x) + 1; lx++) {
          const c = key(lx, floor(y));
          if (dist.has(c) && dist.get(c) < int(this.pace * k) - 1) spots.push(c);
        }
        if (spots.length) rows.push([i, spots]);
      }
      const picks = rows.length ? [...new Set([0, floor(rows.length / 2), rows.length - 1])].sort((a, b) => a - b) : [];
      for (const n of picks) {
        const [i, spots] = rows[n];
        const k = path[i + 1][0] + AIM;
        let j = path.findIndex((p) => p[0] > k);
        j = (j < 0 ? path.length : j) - 1;
        const alone = share * (path[j][5] - path[i][5]);
        for (const c of spots) {
          let worth = this.shot(r, flight, i, c, box, walls) - alone;
          worth -= HIDE * Game.hidden(c, r.w, walls);
          const kk = [dist.get(c) / 100 - worth, c];
          if (worth >= EDGE && (best === null || less(kk, best[0]))) best = [kk, flight, c, i];
        }
      }
    }
    return best ? best.slice(1) : null;
  }

  wait(r, flights, dist, box, walls) {
    const [, y0, , y1] = box;
    let tx = r.x;
    let ty = r.y;
    if (flights.length) {
      const path = least(flights, (f) => [Math.abs(f.ball.x - r.x) + 2 * Math.abs(f.ball.y - r.y), f.ball.n]).path;
      const rows = path.slice(0, AHEAD).map((p) => p[2]);
      r.guard = r.team || this.teams === 1 ? 1 : -1;
      tx = floor(path[Math.min(path.length - 1, 8)][1]) - floor(r.w / 2);
      const edge = r.guard > 0 ? Math.max(...rows) : Math.min(...rows);
      ty = Math.min(Math.max(floor(edge) + r.guard * GAP, y0 + 1), y1 - 2);
    }
    return this.settle(r, tx, ty, dist, walls);
  }

  den(r, dist, box, walls) {
    const [x0, y0, x1, y1] = box;
    const others = this.rackets.filter((o) => !o.dead && o.team !== r.team);
    let tx = r.x;
    if (others.length) {
      const other = least(others, (o) => [Math.abs(o.x - r.x) + Math.abs(o.y - r.y), o.n]);
      const left = 2 * other.x + other.w >= x0 + x1;
      tx = x0 + floor(((x1 - x0) * (left ? 1 : 3)) / 4) - floor(r.w / 2);
    }
    const ty = r.team === 0 ? Math.min(y0 + 2, y1 - 1) : Math.max(y1 - 3, y0);
    return this.settle(r, tx, ty, dist, walls);
  }

  settle(r, tx, ty, dist, walls) {
    const off = (c) => Math.abs(X(c) - tx) + 2 * Math.abs(Y(c) - ty) + 4 * Game.hidden(c, r.w, walls);
    // The least `[off, moves, cell]` within `REACH`, no list made.
    let [goal, best, moves] = [null, 0, 0];
    dist.forEach((m, c) => {
      if (m > REACH) return;
      const o = off(c);
      if (goal === null || o < best || (o === best && (m < moves || (m === moves && c < goal)))) [goal, best, moves] = [c, o, m];
    });
    return goal !== null && off(key(r.x, r.y)) - best >= 3 ? goal : null;
  }

  static hidden(c, w, walls) {
    if (!walls.size) return 0;
    let n = 0;
    for (let i = 0; i < w; i++) if (walls.has(c + i * ACROSS)) n += 1;
    return n;
  }

  // -- rules and bonuses

  fall(box, walls) {
    if (this.steps % 2) return;
    const rackets = this.cellsOf(walls);
    for (const f of [...this.fallers]) {
      const under = f.cells.map((c) => c + 1);
      const hit = new Set();
      for (const c of [...f.cells, ...under]) if (rackets.has(c)) hit.add(rackets.get(c));
      if (hit.size) {
        this.fallers.splice(this.fallers.indexOf(f), 1);
        for (const r of [...hit].sort((a, b) => a.n - b.n)) this.kill(r);
      } else if (under.some((c) => walls.has(c) || this.solid.has(c))) {
        this.fallers.splice(this.fallers.indexOf(f), 1);
        this.events.push(["crash", key(f.x + floor(BW / 2), f.y)]);
      } else f.y += 1;
    }
  }

  kill(r) {
    this.events.push(["kill", key(r.x + floor(r.w / 2), r.y), r]);
    r.dead = DEAD;
    r.span = 0;
    if (r.left !== null) this.rackets.splice(this.rackets.indexOf(r), 1);
  }

  drift(box, walls) {
    const rackets = this.cellsOf(walls);
    for (const cap of [...this.capsules]) {
      const at = key(cap.x, cap.y);
      if (rackets.has(at)) {
        this.capsules.splice(this.capsules.indexOf(cap), 1);
        this.award(cap, rackets.get(at), box);
        continue;
      }
      if (this.steps % 3) continue;
      const under = at + 1;
      if (walls.has(under) || this.solid.has(under)) {
        cap.rest += 1;
        if (cap.rest > 2 * this.second) this.capsules.splice(this.capsules.indexOf(cap), 1);
      } else cap.y += 1;
    }
  }

  size(r, w, box) {
    let mid = r.x + floor((r.w - w) / 2);
    if (w > r.w) {
      const room = this.room(box, w, this.blocked(r));
      const fits = [];
      for (let x = r.x - w + r.w; x < r.x + 1; x++) if (room.has(key(x, r.y))) fits.push(x);
      if (!fits.length) return false;
      const m = mid;
      mid = least(fits, (x) => [Math.abs(x - m), x]);
    }
    r.x = mid;
    r.w = w;
    return true;
  }

  award(cap, racket, box) {
    const team = racket.team;
    this.score[team] += POINTS;
    this.events.push(["catch", key(cap.x, cap.y), cap.kind, team]);
    if (cap.kind === "wide" || cap.kind === "small") {
      for (const r of this.rackets.filter((r) => r.team === team && !r.dead)) {
        const w = cap.kind === "wide" ? WIDE : SMALL;
        if (this.size(r, w, box)) r.span = LAST[cap.kind];
      }
    } else if (cap.kind === "twin") {
      if (!this.rackets.some((r) => r.team === team && r.left !== null)) this.rackets.push(new Racket(this.number(), team, LAST.twin));
    } else if (cap.kind === "multi") {
      for (const ball of [...this.balls]) {
        if (this.balls.length >= BALLS) break;
        const turn = this.rng.uniform(...SPLIT);
        const vx = ball.vx + (ball.vx <= 0 ? turn : -turn);
        const twin = new Ball(this.number(), ball.x, ball.y, vx, ball.vy, ball.owner);
        Object.assign(twin, { fire: ball.fire, fast: ball.fast, combo: ball.combo, phase: ball.phase });
        this.balls.push(twin);
      }
    } else if (cap.kind === "fast") for (const b of this.balls) b.fast = LAST.fast;
    else if (cap.kind === "fire") for (const b of this.balls) b.fire = 5 * this.second;
  }
}
