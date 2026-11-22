// SPDX-License-Identifier: GPL-3.0-or-later
// Bricks: a brick breaker that plays alone, around the words.
//
// - The view as a map: every glyph of the frame is a wall the balls
//   bounce off, with the sides of the view and the bricks. The bottom
//   is open; with two rackets (`-m`), the top too.
// - Rackets that move in two dimensions and play alone: each foresees
//   the balls and, of the places it reaches in time, takes the one
//   whose shot breaks the most bricks.
// - Groups of bricks laid in frame cells where a ball gets, far from
//   each other; a group goes 50 to 100 s after it is laid.
// - The topic `text` (`Sight`): the map is read again when the driver
//   says the text changed or scrolled, and only then.
//
// `-r`: hard, loose, bumper and bomb bricks. `-b`: capsules. `-m`: two
// rackets, two scores. `-e`: particles, a trail, combos, a best score.
// `-a`: all four. `--grab`: the arrows move the racket (`keys-arrows`;
// with `-m` the bottom one), the balls half as fast; ctrl+t gives them
// back and ends; denied, it plays alone.
//
// `Game` (`game.js`) is pure and draws from a `Twister`: the twin's
// game, step for step, for the same seed. Steps are counted from
// session time; between two, the balls shown half way go to their
// place.

import { call, exit, log, period, PtyrcError, random } from "ptyrc";
import { Sight } from "ptyrc/sight";
import { Batch, Board, X, Y, burst, spark, wallsOf } from "./arcade.js";
import { ARROWS, Game, pyRound } from "./game.js";
import { Twister } from "./twister.js";

export const HINT = "←↑↓→  ctrl+t";
const CATCH_UP = 4; // steps a tick at most

export const W = 3; // cells of a brick
export const PALETTE = [196, 208, 226, 46, 51, 33, 201]; // bricks, by row
export const STEEL = { 3: ["≡", 255], 2: ["=", 250], 1: ["-", 245] }; // a hard brick, by lives
export const TEAM = [51, 214]; // two rackets; one alone is white
export const WHITE = "bright-white";
export const FLAME = [226, 208, 196];
export const FLASH = 2; // steps a racket shows a hit
export const LABEL = { wide: ["W", 46], twin: ["T", 51], multi: ["M", 201], small: ["N", 244], fast: ["S", 226], fire: ["F", 196] };

const loop = (frames, ms, style = {}) => ({ frames: frames.map(([t, fg]) => ({ cells: [[{ t, fg, ...style }]], ms })), loop: true });

/** A finite animation of `[text, colour, dim]`. */
function once(frames, ms) {
  return {
    frames: frames.map(([t, fg, dim]) => ({ cells: [[{ t, fg, [dim ? "dim" : "bold"]: true }]], ms })),
    loop: false,
    ttl_ms: frames.length * ms + 500,
  };
}

const ink = (kind, color) => (kind === "plain" || kind === "loose" ? PALETTE[color] : 250);
const still = (run) => ({ content: { cells: [[run]] } });

/** A brick of 3 cells. `dim`: its group is about to go. */
export function brick(kind, lives, color, dim = false) {
  const look = dim ? { dim: true } : {};
  const bold = dim ? look : { bold: true };
  if (kind === "hard") {
    const [sign, fg] = STEEL[Math.max(1, Math.min(3, lives))];
    return still({ t: `⟦${sign}⟧`, fg, ...bold });
  }
  if (kind === "loose") return still({ t: "⟨=⟩", fg: PALETTE[color], ...look });
  if (kind === "bumper") {
    const inks = lives > 1 ? [201, 213] : [213, 225];
    return loop([["⟦o⟧", inks[0]], ["⟦O⟧", inks[1]]], 300, bold);
  }
  if (kind === "bomb") return loop([["⟦*⟧", 196], ["⟦+⟧", 226]], 250, bold);
  return still({ t: "⟦=⟧", fg: PALETTE[color], ...look });
}

/** A loose brick on its way down: it shakes. */
export const faller = (color) => loop([["⟨=⟩", 196], ["⟨~⟩", PALETTE[color]]], 90, { bold: true });

/** A racket; `team` null when alone; `hit`: a ball just left it. */
export function racket(width, team = null, hit = false) {
  const [bar, fg] = hit ? ["═", team === null ? 226 : 231] : ["━", team === null ? WHITE : TEAM[team]];
  return [[{ t: `(${bar.repeat(width - 2)})`, fg, bold: true }]];
}

/** A ball, of its owner's colour, in the upper half of its cell or
 * the `low` one; on fire, a colour of the flame a `step`. */
export function ball(team = null, fire = false, low = false, step = 0) {
  const fg = fire ? FLAME[step % FLAME.length] : team === null ? WHITE : TEAM[team];
  return still({ t: low ? "▄" : "▀", fg, bold: true });
}

export function trail(team = null, low = false) {
  const g = low ? "▄" : "▀";
  return once([[g, team === null ? 250 : TEAM[team], true], [g, 240, true], [g, 236, true]], 60);
}

const brackets = (kind) => (kind === "loose" ? ["⟨", "⟩"] : ["⟦", "⟧"]);

export function pop(kind, color) {
  const [l, r] = brackets(kind);
  const fg = ink(kind, color);
  return once([[`${l}·${r}`, fg, true], [" · ", fg, true]], 70);
}

export function fade(kind, color) {
  const [l, r] = brackets(kind);
  return once([[`${l}·${r}`, ink(kind, color), true], [" · ", 240, true]], 160);
}

export const again = () => once([["· · ·", 244, true], [" · · ", 240, true], ["  ·  ", 236, true]], 220);
export const lock = () => once([["(   )", 231, false], [" ( ) ", 231, false]], 90);
export const splash = () => once([["  x  ", 196, false], [" - - ", 196, false], ["-   -", 160, true]], 90);

/** The pieces of a broken brick, one a cell: the brackets fly apart,
 * the sign up; all fall. */
export function shards(rng, kind, color) {
  const [left, right] = kind === "loose" ? ["⟨<.", "⟩>."] : ["⟦[.", "⟧]."];
  const out = [];
  for (const [glyphs, way] of [[left, -1.0], ["=-.", 0.0], [right, 1.0]]) {
    const one = spark(rng, [ink(kind, color)], glyphs, undefined, [0.35, 0.6]);
    const vx = way * rng.uniform(4.0, 10.0) + rng.uniform(-1.5, 1.5);
    one.motion = { vx, vy: rng.uniform(-6.0, -2.0), ax: 0.0, ay: 16.0, bounce: false };
    out.push(one);
  }
  return out;
}

export function capsule(kind) {
  const [t, fg] = LABEL[kind];
  return {
    frames: [
      { cells: [[{ t, fg: 16, bg: fg, bold: true }]], ms: 220 },
      { cells: [[{ t, fg, bold: true }]], ms: 220 },
    ],
    loop: true,
  };
}

/** A word that rises and goes: bold for `seconds`, then dim half as long. */
export function label(t, fg, seconds = 0.5) {
  const ms = Math.trunc(seconds * 1000);
  return {
    frames: [
      { cells: [[{ t, fg, bold: true }]], ms },
      { cells: [[{ t, fg, dim: true }]], ms: Math.floor(ms / 2) },
    ],
    loop: false,
    motion: { vx: 0.0, vy: -3.0, ax: 0.0, ay: 0.0, bounce: false },
    ttl_ms: Math.trunc((1.5 * seconds + 0.25) * 1000),
  };
}

/** The score; two: the keeper of the top, then of the bottom. */
export function board(score, best = null, hint = null) {
  const runs =
    score.length === 1
      ? [{ t: `◆ ${score[0]}`, fg: WHITE, bold: true }]
      : [{ t: `▲ ${score[0]}`, fg: TEAM[0], bold: true }, { t: "  " }, { t: `▼ ${score[1]}`, fg: TEAM[1], bold: true }];
  if (best !== null) runs.push({ t: `  best ${best.join(" ")}`, dim: true });
  if (hint) runs.unshift({ t: `${hint}  `, dim: true });
  return [runs];
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The sprites of a `Game`; particles and more when `effects`. Layers,
 * from the back: shards, bricks, the trail, capsules and falling
 * bricks, rackets, balls, marks and particles, the score. */
export class Stage {
  constructor(rng, effects, hint = null) {
    this.rng = rng;
    this.effects = effects;
    this.hint = hint;
    this.bricks = new Map(); // serial: [handle, key]
    this.dim = new Set();
    this.moving = new Map(); // serial: [handle, [at, key]]
    this.board = new Board();
    this.steps = 0;
    this.hit = new Map(); // racket: the step its flash ends
    this.was = new Map(); // ball: [x, y]
    this.late = new Map(); // ball: [at, key, look], for `glide`
  }

  #put(n, at, k, z, look) {
    const was = this.moving.get(n);
    if (!was) {
      this.moving.set(n, [this.draw(at, { z, ...look }), [at, k]]);
      return;
    }
    const [h, [from, shown]] = was;
    if (!same(shown, k)) this.out.update(h, look);
    if (!same(from, at)) this.out.update(h, { anchor: { at, space: "frame" } });
    this.moving.set(n, [h, [at, k]]);
  }

  draw(at, spec) {
    return this.out.put({ frame: this.frame, anchor: { at, space: "frame" }, ...spec });
  }

  sparks(at, n, colors, kw = {}) {
    for (const one of burst(n, this.rng, colors, kw)) this.draw(at, { ...one, z: 4 });
  }

  #ball(it, x, y, alone) {
    const team = alone ? null : it.owner;
    const low = y - Math.floor(y) >= 0.5;
    const hot = it.fire ? 1 + (this.steps % FLAME.length) : 0;
    return [[Math.floor(x), Math.floor(y)], [team, hot, low], ball(team, !!hot, low, this.steps)];
  }

  /** The game after a step, `events` those of its steps, by `out` on `frame`. */
  show(out, frame, game, box, events = game.events) {
    this.out = out;
    this.frame = frame;
    this.steps += 1;
    this.late = new Map();
    for (const e of events) this.play(...e);
    const [x0, y0, x1, y1] = box;
    const alone = game.teams === 1;
    const reach = y1 - y0;
    const near = new Map(game.bricks([x0, y0 - reach, x1, y1 + reach]).map((b) => [b.n, b]));
    for (const [n, [h]] of [...this.bricks]) {
      if (!near.has(n)) {
        out.remove(h);
        this.bricks.delete(n);
      }
    }
    for (const n of [...this.dim]) if (!near.has(n)) this.dim.delete(n);
    for (const [n, it] of near) {
      const k = [it.lives, this.dim.has(n)];
      const was = this.bricks.get(n);
      if (!was) this.bricks.set(n, [this.draw([it.x, it.y], { z: 0, ...brick(it.kind, it.lives, it.color, k[1]) }), k]);
      else if (!same(was[1], k)) {
        out.update(was[0], brick(it.kind, it.lives, it.color, k[1]));
        was[1] = k;
      }
    }
    const things = new Map();
    for (const it of game.fallers) things.set(it.n, [[it.x, it.y], "faller", 1, faller(it.color)]);
    for (const it of game.capsules) things.set(it.n, [[it.x, it.y], it.kind, 1, capsule(it.kind)]);
    for (const [n, end] of [...this.hit]) if (end <= this.steps) this.hit.delete(n);
    for (const it of game.rackets) {
      if (it.dead) continue;
      const team = alone ? null : it.team;
      const hit = this.hit.has(it.n);
      things.set(it.n, [[it.x, it.y], [it.w, hit], 2, { content: { cells: racket(it.w, team, hit) } }]);
    }
    const was = this.was;
    this.was = new Map(game.balls.map((b) => [b.n, [b.x, b.y]]));
    for (const it of game.balls) {
      let x = it.x;
      let y = it.y;
      const [px, py] = was.get(it.n) ?? [x, y];
      if (Math.abs(y - py) > 0.55 || Math.abs(x - px) > 1.05) {
        this.late.set(it.n, this.#ball(it, x, y, alone));
        x = (x + px) / 2;
        y = (y + py) / 2;
      }
      const [at, k, look] = this.#ball(it, x, y, alone);
      things.set(it.n, [at, k, 3, look]);
      if (it.fire) this.draw(at, { ...spark(this.rng, FLAME, "*+.", [0.5, 3.0], [0.2, 0.5], -4.0), z: 4 });
      else if (this.effects) this.draw(at, { z: 1, ...trail(k[0], k[2]) });
    }
    for (const [n, [h]] of [...this.moving]) {
      if (!things.has(n)) {
        out.remove(h);
        this.moving.delete(n);
      }
    }
    for (const [n, [at, k, z, look]] of things) this.#put(n, at, k, z, look);
    this.board.show(out, frame, board(game.score, this.effects ? game.best : null, this.hint));
  }

  /** Between two steps: the balls shown half way go to their place. */
  glide(out) {
    this.out = out;
    for (const [n, [at, k, look]] of this.late) if (this.moving.has(n)) this.#put(n, at, k, 3, look);
    this.late = new Map();
  }

  /** An event of the game: marks, and particles when `effects`. */
  play(what, at = null, ...data) {
    const xy = at !== null && typeof at === "number" ? [X(at), Y(at)] : null;
    const rng = this.rng;
    if (what === "fade") for (const it of at) this.dim.add(it.n);
    else if (what === "gone") for (const it of at) this.draw([it.x, it.y], { ...fade(it.kind, it.color), z: 0 });
    else if (what === "reset") this.draw([xy[0] - 2, xy[1]], { ...again(), z: 4 });
    if (what === "boom") this.sparks(xy, 22, [231, ...FLAME], { speed: [6.0, 22.0] });
    else if (what === "kill") this.sparks(xy, 18, [WHITE, 196], { glyphs: "━=-.", gravity: 14.0 });
    else if (what === "break") {
      const it = data[0];
      this.draw([it.x, it.y], { ...pop(it.kind, it.color), z: 4 });
      if (this.effects) shards(rng, it.kind, it.color).forEach((one, i) => this.draw([it.x + i, it.y], { ...one, z: -1 }));
    } else if (what === "bounce") {
      if (data[0]?.n !== undefined) this.hit.set(data[0].n, this.steps + FLASH);
    } else if (what === "serve") this.draw([xy[0] - 2, xy[1]], { ...lock(), z: 4 });
    else if (what === "lost" || what === "out") this.draw([xy[0] - 2, xy[1]], { ...splash(), z: 4 });
    if (!this.effects) return;
    if (what === "hit") this.sparks(xy, 2, [252], { glyphs: "+.", speed: [2.0, 6.0], life: [0.2, 0.4] });
    else if (what === "bounce") this.sparks(xy, 3, [231], { glyphs: "'.", speed: [3.0, 8.0], life: [0.15, 0.3] });
    else if (what === "combo") {
      const n = data[0];
      const fg = n < 4 ? 252 : n < 7 ? 226 : 208;
      this.draw([xy[0], xy[1] - 1], { ...label(`x${n}`, fg, 0.3), z: 4 });
    } else if (what === "bump") this.sparks(xy, 6, [201, 213], { glyphs: "≈~.", gravity: 0.0 });
    else if (what === "crash") shards(rng, "loose", 0).forEach((one, i) => this.draw([xy[0] - 1 + i, xy[1]], { ...one, z: -1 }));
    else if (what === "lost") {
      this.sparks(xy, 10, [196, 160], { glyphs: "▄*.", speed: [4.0, 12.0] });
      this.draw(xy, { ...label("0", 196), z: 4 });
    } else if (what === "serve") this.sparks(xy, 4, [231], { glyphs: "+'.", speed: [2.0, 6.0], life: [0.2, 0.4] });
    else if (what === "catch") {
      const [t, fg] = LABEL[data[0]];
      this.sparks(xy, 8, [fg], { glyphs: `${t}*.`, gravity: 0.0 });
      this.draw(xy, { ...label(data[0], fg), z: 4 });
    } else if (what === "group") {
      for (const it of at) {
        const one = spark(rng, [231], "+'", [0.0, 1.0], [0.2, 0.5], 0.0);
        this.draw([it.x + rng.randrange(W), it.y], { ...one, z: 4 });
      }
    }
  }
}

/** The options as the game takes them; `error`: a refusal, or null. */
export function options(args = {}) {
  const all = args.all === true;
  const o = {
    speed: args.speed ?? 16.0,
    rules: all || args.rules === true,
    bonuses: all || args.bonuses === true,
    multiplayer: all || args.multiplayer === true,
    effects: all || args.effects === true,
    grab: args.grab === true,
  };
  o.error = o.speed > 0 ? null : "--speed must be positive";
  return o;
}

/** Steps due at `now` ms, at `rate` a second. */
export const due = (now, rate) => Math.floor((now * rate) / 1000);

let o;
let game;
let stage;
let sight;
let walls = new Set();
let done = 0; // steps made

export function init(args) {
  o = options(args);
  if (o.error) {
    log(`bricks: ${o.error}`);
    exit(2);
    return;
  }
  const b = random(4);
  const seed = ((b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0) % 1000000007;
  if (o.grab) {
    try {
      call("input.grab", { keys: Object.keys(ARROWS), escape: "ctrl+t" });
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      log(`bricks: the arrows: ${e.code}; it plays alone`);
      o.grab = false;
    }
  }
  const player = o.grab ? (o.multiplayer ? 1 : 0) : null;
  game = new Game(new Twister(seed), { rules: o.rules, bonuses: o.bonuses, multiplayer: o.multiplayer, second: Math.max(1, pyRound(o.speed)), player });
  // Its own draws: the particles do not change the game of a seed.
  stage = new Stage(new Twister(seed), o.effects, o.grab ? HINT : null);
  sight = new Sight();
  // A tick in two is a step; the other puts the balls in place.
  period(Math.max(16, Math.min(1000, Math.round(500 / o.speed))));
}

export function tick(now) {
  if (!sight.look()) return;
  const [w, h] = sight.size;
  const [ox, oy] = sight.info?.origin ?? [0, 0];
  const box = [ox, oy, ox + w, oy + h];
  if (sight.changed) walls = wallsOf(sight.rows, ox, oy);
  const target = due(now, o.speed);
  const n = Math.min(target - done, CATCH_UP);
  const out = new Batch(call);
  const frame = sight.info?.id ?? sight.frame;
  if (target > done) {
    done = target;
    const events = [];
    for (let i = 0; i < n; i++) {
      game.step(box, walls);
      events.push(...game.events);
    }
    stage.show(out, frame, game, box, events);
  } else if (stage.late.size) stage.glide(out);
  try {
    out.send();
  } catch (e) {
    if (!(e instanceof PtyrcError)) throw e;
    if (e.code !== "not-usable") throw e;
    sight.touch();
  }
}

export function event(topic, data) {
  if (sight.event(topic, data)) return;
  if (topic === "input" && data?.released) exit(0);
  else if (topic === "input" && data?.key) game.press(data.key);
}

export function stop() {
  if (o?.grab) {
    try {
      call("input.release");
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
    }
  }
}
