// SPDX-License-Identifier: GPL-3.0-or-later
// The module "ptyrc". `ptyrc.d.ts` is its contract.

import * as n from "ptyrc:native";

// The host's random under `Math.random`; no clock but `Date` (session
// time).
Object.defineProperty(Math, "random", { value: n.mathRandom, writable: true, configurable: true });
delete globalThis.performance;

const BELOW = 0;
const DEFAULT = 0x01000000;
const ATTRS = { bold: 1, dim: 2, italic: 4, underline: 8, inverse: 16, strike: 32, blink: 64 };

export class PtyrcError extends Error {
  constructor(error) {
    super(error.message);
    this.name = "PtyrcError";
    this.code = error.code;
    this.hint = error.hint;
  }
}

export function call(method, params) {
  if (typeof method !== "string") throw new TypeError("call(method, params): method is a string");
  const text = n.call(method, params === undefined ? "" : JSON.stringify(params));
  const answer = JSON.parse(text);
  if (answer.error) throw new PtyrcError(answer.error);
  return answer.result;
}

export function rgb(r, g, b) {
  return 0x03000000 | ((r & 255) << 16) | ((g & 255) << 8) | (b & 255);
}

function color(c, what) {
  if (c === undefined || c === null) return BELOW;
  if (typeof c === "number") {
    if (c >= 0 && c <= 255) return 0x02000000 | (c << 16);
    if (c >>> 24 === 3) return c;
  } else if (c === "default") {
    return DEFAULT;
  } else if (typeof c === "string" && /^#[0-9a-fA-F]{6}$/.test(c)) {
    return 0x03000000 | parseInt(c.slice(1), 16);
  }
  throw new TypeError(
    `${what}: ${JSON.stringify(c)} is no colour: "#rrggbb", rgb(r, g, b), 0..255 (the palette), "default", or null`,
  );
}

export class Style {
  constructor(spec = {}) {
    this.fg = color(spec.fg, "fg");
    this.bg = color(spec.bg, "bg");
    let attrs = 0;
    for (const name in ATTRS) if (spec[name]) attrs |= ATTRS[name];
    this.attrs = attrs;
    Object.freeze(this);
  }
}

const PLAIN = new Style();

export function style(spec) {
  return new Style(spec);
}

function styled(s) {
  if (s === undefined || s === null) return PLAIN;
  return s instanceof Style ? s : new Style(s);
}

function scalar(ch) {
  if (typeof ch === "number") return ch;
  if (typeof ch === "string" && ch.length > 0) return ch.codePointAt(0);
  if (ch === undefined || ch === null || ch === "") return 0;
  throw new TypeError("a glyph is a string of one character, or its scalar value");
}

export class Canvas {
  #id;
  constructor(width, height) {
    if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1)
      throw new RangeError("new Canvas(width, height): two integers, 1 at least");
    this.#id = n.canvas(width, height);
    if (!this.#id) {
      const e = new RangeError(`new Canvas(${width}, ${height}): refused`);
      e.hint = "the canvases of an app hold four screens of cells at most: release() one, or make it smaller";
      throw e;
    }
    this.width = width;
    this.height = height;
  }
  set(x, y, ch, s) {
    s = styled(s);
    n.set(this.#id, x, y, scalar(ch), s.fg, s.bg, s.attrs);
    return this;
  }
  text(x, y, string, s) {
    s = styled(s);
    return n.text(this.#id, x, y, String(string), s.fg, s.bg, s.attrs);
  }
  fill(x, y, width, height, ch, s) {
    s = styled(s);
    n.fill(this.#id, x, y, width, height, scalar(ch), s.fg, s.bg, s.attrs);
    return this;
  }
  clear() {
    n.clear(this.#id);
    return this;
  }
  present(place) {
    return n.present(this.#id, place === undefined ? "" : JSON.stringify(place));
  }
  release() {
    n.release(this.#id);
    this.#id = 0;
  }
}

export function period(ms) {
  n.period(ms);
}

function show(v) {
  if (typeof v === "string") return v;
  if (v instanceof Error) return `${v.name}: ${v.message}`;
  try {
    const text = JSON.stringify(v);
    return text === undefined ? String(v) : text;
  } catch {
    return String(v);
  }
}

export function log(...values) {
  n.log(values.map(show).join(" "));
}

export function random(length) {
  if (!Number.isInteger(length) || length < 0 || length > 65536)
    throw new RangeError("random(length): 0 to 65536 bytes");
  return n.random(length);
}

export function exit(code = 0) {
  n.exit(code | 0);
}

export function manifest() {
  const text = n.manifest();
  return text === null ? null : JSON.parse(text);
}

// An uncaught error as lines of the log, for the interpreter:
// `file:line:column: what`, the frames of the app, the cure when known.

const FRAMES = 6;
const NO_TIMER = "no timer: ask a period (period(50)) and count in tick(now)";
const NO_NETWORK = "no network: a store app has none";
const NO_PAGE = 'no page: draw on a Canvas from "ptyrc"';
const CURES = {
  setTimeout: NO_TIMER,
  setInterval: NO_TIMER,
  requestAnimationFrame: "no timer: ask a period (period(33)) and draw in tick(now)",
  fetch: NO_NETWORK,
  XMLHttpRequest: NO_NETWORK,
  WebSocket: NO_NETWORK,
  require: 'ES modules only: import { x } from "./file.js"',
  process: "no process: arguments come in init(args)",
  console: 'no console: import { log } from "ptyrc"',
  document: NO_PAGE,
  window: NO_PAGE,
};

function describe(e) {
  if (!(e instanceof Error)) return `uncaught: ${show(e)}`;
  const frames = [];
  for (const line of String(e.stack ?? "").split("\n")) {
    const at = /^\s*at (?:.* \()?(.+?):(\d+):(\d+)\)?$/.exec(line);
    if (at && at[1] !== "ptyrc" && at[1] !== "native" && !at[1].startsWith("<")) frames.push({ line: line.trimEnd(), where: `${at[1]}:${at[2]}:${at[3]}` });
  }
  // A file that does not parse: its place, the message.
  const parsing = e.name === "SyntaxError" && frames.length && !frames.some((f) => f.line.includes(" ("));
  let out = parsing ? e.message : `${e.name}: ${e.message}`;
  if (frames.length) out = `${frames[0].where}: ${out}`;
  for (const f of frames.slice(0, FRAMES)) if (f.line.includes(" (")) out += `\n${f.line}`;
  if (!out.includes("\n  hint: ")) {
    const missing = / ?(\w+) is not defined$/.exec(e.message);
    const hint = typeof e.hint === "string" ? e.hint : missing && Object.hasOwn(CURES, missing[1]) ? CURES[missing[1]] : null;
    if (hint) out += `\n  hint: ${hint}`;
  }
  return out;
}

n.hooks(describe);
