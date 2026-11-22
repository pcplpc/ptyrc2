// SPDX-License-Identifier: GPL-3.0-or-later
// The module "ptyrc". `ptyrc.d.ts` is its contract.

import * as n from "ptyrc:native";

// The host's random under `Math.random`; no clock but `Date` (session
// time). Base64 is the engine's: `Uint8Array.fromBase64`, `toBase64`,
// `atob`, `btoa`.
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

export function cells(text) {
  return n.cells(String(text));
}

// A cell's colour (4 bytes at `i`) as a run's member; undefined: absent.
function colorOf(b, i) {
  if (b[i] === 1) return "default";
  if (b[i] === 2) return b[i + 1];
  if (b[i] === 3) return "#" + ((b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]).toString(16).padStart(6, "0");
  return undefined;
}

function styleKey(b, p) {
  let k = "";
  for (let i = p + 4; i < p + 14; i++) k += String.fromCharCode(b[i]);
  return k;
}

function runOf(b, p) {
  const run = { t: "" };
  const fg = colorOf(b, p + 4);
  const bg = colorOf(b, p + 8);
  if (fg !== undefined) run.fg = fg;
  if (bg !== undefined) run.bg = bg;
  const a = b[p + 12] | (b[p + 13] << 8);
  for (const name in ATTRS) if (a & ATTRS[name]) run[name] = true;
  return run;
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
  runs() {
    const b = n.bytes(this.#id);
    const w = this.width;
    const rows = [];
    for (let y = 0; y < this.height; y++) {
      const row = [];
      let run = null;
      let key = -1;
      for (let x = 0; x < w; x++) {
        const p = (y * w + x) * 16;
        const s = (b[p] | (b[p + 1] << 8) | (b[p + 2] << 16) | (b[p + 3] << 24)) >>> 0;
        const ch = s > 0 && s <= 0x10ffff ? String.fromCodePoint(s) : " ";
        const k = s > 0 ? styleKey(b, p) : "";
        if (run === null || k !== key) {
          run = s > 0 ? runOf(b, p) : { t: "" };
          row.push(run);
          key = k;
        }
        run.t += ch;
        if (s > 0 && n.cells(ch) === 2) x++;
      }
      rows.push(row);
    }
    return rows;
  }
  present(place) {
    return n.present(this.#id, place === undefined ? "" : JSON.stringify(place));
  }
  sprite() {
    return n.sprite(this.#id);
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

// ---- text and bytes: UTF-8 only ----

class TextEncoder {
  get encoding() {
    return "utf-8";
  }
  encode(text = "") {
    const s = String(text);
    const out = [];
    for (let i = 0; i < s.length; i++) {
      let c = s.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
        const d = s.charCodeAt(i + 1);
        if (d >= 0xdc00 && d <= 0xdfff) (c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00)), i++;
      }
      if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return new Uint8Array(out);
  }
}

class TextDecoder {
  #fatal;
  constructor(label = "utf-8", options = {}) {
    if (!/^utf-?8$/i.test(String(label))) throw new RangeError(`TextDecoder: ${label}: UTF-8 only`);
    this.#fatal = Boolean(options?.fatal);
  }
  get encoding() {
    return "utf-8";
  }
  get fatal() {
    return this.#fatal;
  }
  decode(input) {
    if (input === undefined) return "";
    const b = input instanceof Uint8Array ? input : ArrayBuffer.isView(input) ? new Uint8Array(input.buffer, input.byteOffset, input.byteLength) : new Uint8Array(input);
    let s = "";
    let i = b[0] === 0xef && b[1] === 0xbb && b[2] === 0xbf ? 3 : 0;
    while (i < b.length) {
      const x = b[i];
      const n = x < 0x80 ? 0 : x >= 0xc2 && x < 0xe0 ? 1 : x >= 0xe0 && x < 0xf0 ? 2 : x >= 0xf0 && x < 0xf5 ? 3 : -1;
      let c = n === 0 ? x : n === 1 ? x & 31 : n === 2 ? x & 15 : x & 7;
      let k = 1;
      for (; n > 0 && k <= n; k++) {
        const y = b[i + k];
        if (y === undefined || (y & 0xc0) !== 0x80) break;
        c = (c << 6) | (y & 63);
      }
      const whole = n >= 0 && k === n + 1;
      const shortest = n < 2 || (n === 2 ? c >= 0x800 && (c < 0xd800 || c > 0xdfff) : c >= 0x10000 && c <= 0x10ffff);
      if (whole && shortest) {
        s += String.fromCodePoint(c);
        i += n + 1;
      } else {
        if (this.#fatal) throw new TypeError(`TextDecoder: invalid UTF-8 at byte ${i}`);
        // The lead byte and the continuation bytes before the fault.
        s += "�";
        i += n < 0 || whole ? 1 : k;
      }
    }
    return s;
  }
}

globalThis.TextEncoder = TextEncoder;
globalThis.TextDecoder = TextDecoder;

// ---- host functions: clock, storage, assets, files, result ----

const PIECE = 1 << 20;

function offsetOf(o) {
  return { at: o.at, offsetS: o.offset_s, abbr: o.abbr, next: o.next ?? null };
}

export function now() {
  const r = call("clock.now");
  const out = { unixMs: r.unix_ms, zone: r.zone, offsetS: r.offset_s, abbr: r.abbr, next: r.next ?? null };
  if (r.degree) out.degree = r.degree;
  return out;
}

export function zone(name, at) {
  const p = { zone: name };
  if (at !== undefined) p.at = Array.isArray(at) ? at : [at];
  const r = call("clock.zone", p);
  const out = { zone: r.zone, offsets: r.offsets.map(offsetOf) };
  if (r.degree) out.degree = r.degree;
  return out;
}

export function until(unixMs, everyMs) {
  const every = Math.max(1, Math.floor(everyMs));
  let left = every - (((unixMs % every) + every) % every);
  while (left < 33) left += every;
  return left;
}

export function civil(unixMs, offsetS = 0) {
  const d = new Date((Math.floor(unixMs / 1000) + offsetS) * 1000);
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: d.getUTCSeconds(),
    weekday: d.getUTCDay(),
  };
}

export const storage = Object.freeze({
  get(key) {
    return call("storage.get", { key }).value ?? null;
  },
  set(key, value) {
    if (typeof value !== "string") throw new TypeError("storage.set(key, value): value is a string (JSON.stringify it)");
    return call("storage.set", { key, value }).used;
  },
  remove(key) {
    call("storage.remove", { key });
  },
  keys() {
    return call("storage.list").keys.map((k) => k.key);
  },
});

// Bytes of `method` whole, by pieces of 1 MiB, and the `mtime_ms` of
// the answers (0: none); `not-usable` if it changes between two pieces.
function readAll(method, params) {
  const pieces = [];
  let size = 0;
  let mtime = null;
  for (;;) {
    const r = call(method, { ...params, as: "base64", offset: size, len: PIECE });
    const m = r.mtime_ms ?? 0;
    if ((mtime ??= m) !== m) throw new PtyrcError({ code: "not-usable", message: `${method}: changed while read` });
    const b = Uint8Array.fromBase64(r.base64);
    pieces.push(b);
    size += b.length;
    if (r.eof !== false || !b.length) break;
  }
  const all = new Uint8Array(size);
  let at = 0;
  for (const b of pieces) all.set(b, at), (at += b.length);
  return { bytes: all, mtimeMs: mtime };
}

function readText(method, params) {
  const r = call(method, { ...params, as: "text" });
  if (r.eof !== false) return { text: r.text, mtimeMs: r.mtime_ms ?? 0 };
  const { bytes, mtimeMs } = readAll(method, params);
  return { text: new TextDecoder().decode(bytes), mtimeMs };
}

export function asset(name) {
  return readText("asset.read", { name }).text;
}

export function assetBytes(name) {
  return readAll("asset.read", { name }).bytes;
}

export function assets() {
  return call("asset.list").assets.map((a) => a.name);
}

class HostFile {
  #child;
  constructor(handle, name, child) {
    this.handle = handle;
    this.name = name;
    this.#child = child;
  }
  get #params() {
    return this.#child === null ? { file: this.handle } : { file: this.handle, name: this.#child };
  }
  text() {
    return readText("file.read", this.#params).text;
  }
  bytes() {
    return readAll("file.read", this.#params).bytes;
  }
  textWithMtime() {
    return readText("file.read", this.#params);
  }
  bytesWithMtime() {
    return readAll("file.read", this.#params);
  }
  stat() {
    const r = call("file.stat", this.#params);
    return { exists: !!r.exists, size: r.size ?? 0, mtimeMs: r.mtime_ms ?? 0 };
  }
  write(content, { ifMtime } = {}) {
    const p = this.#params;
    if (typeof content === "string") p.text = content;
    else if (content instanceof Uint8Array) p.base64 = content.toBase64();
    else throw new TypeError("file.write(content): a string or a Uint8Array");
    if (ifMtime != null) p.if_mtime_ms = ifMtime;
    const r = call("file.write", p);
    return { size: r.size, mtimeMs: r.mtime_ms };
  }
  list() {
    return call("file.list", this.#params).entries.map((e) => e.name);
  }
  child(name) {
    return new HostFile(this.handle, name, name);
  }
}

export function file(handle) {
  if (Array.isArray(handle)) handle = handle[0];
  if (typeof handle === "string") return new HostFile(handle, "", null);
  if (typeof handle?.file === "string") return new HostFile(handle.file, handle.name ?? "", null);
  throw new TypeError('file(handle): "f0", or the value of a file option in init(args): {file, name}');
}

export function result(value, status = 0) {
  call("app.result", { value, status });
}

// ---- unit tests ----

export class AssertionError extends Error {
  constructor(message) {
    super(message);
    this.name = "AssertionError";
  }
}

export function assert(value, message) {
  if (!value) throw new AssertionError(message ?? `${show(value)} is not true`);
}

function same(a, b) {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  if (ArrayBuffer.isView(a)) return a.length === b.length && Array.prototype.every.call(a, (v, i) => Object.is(v, b[i]));
  if (a instanceof Map || a instanceof Set) return a.size === b.size && same([...a], [...b]);
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && same(a[k], b[k]));
}

function shown(v) {
  if (ArrayBuffer.isView(v)) return `${v.constructor.name} [${Array.prototype.join.call(v, ", ")}]`;
  if (v instanceof Map || v instanceof Set) return `${v.constructor.name} ${show([...v])}`;
  return v === undefined ? "undefined" : typeof v === "number" || typeof v === "bigint" ? String(v) : typeof v === "string" ? JSON.stringify(v) : show(v);
}

export function assertEqual(actual, expected, message) {
  if (!same(actual, expected))
    throw new AssertionError(`${message ? message + ": " : ""}${shown(actual)}, expected ${shown(expected)}`);
}

export function assertThrows(f, kind, message) {
  try {
    f();
  } catch (e) {
    if (kind === undefined || e instanceof kind) return e;
    throw new AssertionError(`${message ? message + ": " : ""}threw ${show(e)}, expected a ${kind.name}`);
  }
  throw new AssertionError(`${message ? message + ": " : ""}nothing thrown`);
}

// An uncaught error as lines of the log, for the interpreter:
// `file:line:column: what`, the frames of the app, the cure when known.

const FRAMES = 6;
// The modules of the SDK beside "ptyrc": their frames are left out.
const SDK = ["ptyrc/deck", "ptyrc/canvas", "ptyrc/sight"];
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
    if (at && at[1] !== "ptyrc" && !SDK.includes(at[1]) && at[1] !== "native" && !at[1].startsWith("<")) frames.push({ line: line.trimEnd(), where: `${at[1]}:${at[2]}:${at[3]}` });
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
