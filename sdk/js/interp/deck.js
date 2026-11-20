// SPDX-License-Identifier: GPL-3.0-or-later
// The module "ptyrc/deck": widgets kept fresh, and the pieces of an
// interactive one. `ptyrc.d.ts` is its contract.

import { call, cells, Canvas, PtyrcError } from "ptyrc";

// What of a widget's info a hover stands on.
const PLACE = ["sprite", "variant", "rect"];
// Members of `widget.show`.
const SHOW = ["border", "title", "frame", "prefer", "fill", "margin", "layer", "modal"];

function bad(what) {
  return new TypeError(what);
}

// ---- content ----

// What `render` returned, as the `content` of `widget.set`.
function content(c) {
  if (c instanceof Canvas) return { cells: c.runs() };
  if (typeof c === "string") c = c.split("\n");
  if (!Array.isArray(c)) throw bad("a widget's content: a string, rows (strings or runs), or a Canvas");
  if (c.every((r) => typeof r === "string")) return { rows: c };
  return { cells: c.map((r) => (typeof r === "string" ? [{ t: r }] : Array.isArray(r) ? r : [r])) };
}

// `[[w, h], ...]` or `{name: [w, h]}`: `[name, [w, h]]`, name null for `WxH`.
function variants(sizes) {
  const list = Array.isArray(sizes) ? sizes.map((s) => [null, s]) : Object.entries(sizes ?? {});
  if (!list.length) throw bad("sizes: [[w, h], ...] or {name: [w, h]}, one at least");
  for (const [, s] of list)
    if (!Array.isArray(s) || s.length !== 2 || !s.every(Number.isInteger))
      throw bad(`sizes: ${JSON.stringify(s)} is not [w, h]`);
  return list;
}

// ---- keys ----

const MODIFIED = /^\x1b\[(\d+);([2-8])([A-DFH~])$/;
const LETTERS = { A: "up", B: "down", C: "right", D: "left", H: "home", F: "end" };
const TILDES = { 2: "insert", 3: "delete", 5: "pageup", 6: "pagedown" };
const START = "\x1b[200~";
const END = "\x1b[201~";

function latin1(bytes) {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return s;
}

export class Keys {
  #paste = null;
  static name(bytes) {
    const m = MODIFIED.exec(typeof bytes === "string" ? bytes : latin1(bytes));
    if (!m) return null;
    const code = Number(m[1]);
    const mods = Number(m[2]) - 1;
    const key = m[3] === "~" ? TILDES[code] : code === 1 ? LETTERS[m[3]] : undefined;
    if (!key) return null;
    const held = [[4, "ctrl"], [2, "alt"], [1, "shift"]].filter(([bit]) => mods & bit).map(([, n]) => n);
    return [...held, key].join("+");
  }
  reset() {
    this.#paste = null;
  }
  feed(data) {
    const nl = (t) => t.replace(/\r\n?/g, "\n");
    if (typeof data?.paste === "string") return { paste: nl(data.paste) };
    const bytes = Uint8Array.fromBase64(data?.bytes ?? "");
    const raw = latin1(bytes);
    if (this.#paste !== null) {
      if (raw !== END) {
        this.#paste.push(bytes);
        return null;
      }
      const all = new Uint8Array(this.#paste.reduce((n, b) => n + b.length, 0));
      let at = 0;
      for (const b of this.#paste) all.set(b, at), (at += b.length);
      this.#paste = null;
      return { paste: nl(new TextDecoder().decode(all)) };
    }
    if (raw === START) {
      this.#paste = [];
      return null;
    }
    const key = data?.key ?? Keys.name(bytes);
    return key ? { key } : null;
  }
}

// ---- widgets ----

export class Widget {
  #deck;
  constructor(deck, tag, render, options) {
    this.#deck = deck;
    this.tag = tag;
    this.info = {};
    this.hidden = false;
    this.render = render;
    this.options = options;
  }
  get id() {
    return this.info.id ?? null;
  }
  get variant() {
    return this.info.variant ?? null;
  }
  get size() {
    return this.info.size ?? null;
  }
  get rect() {
    return this.info.rect ?? null;
  }
  get placed() {
    return this.info.state === "placed";
  }
  get over() {
    return !!this.info.over;
  }
  get asks() {
    return this.info.asks ?? null;
  }
  get focused() {
    return !!this.info.focused;
  }
  set(rows, opts = {}) {
    const p = { id: this.id };
    if (opts.variant != null) p.variant = opts.variant;
    if (rows != null) p.content = content(rows);
    if (opts.title != null) p.title = opts.title;
    call("widget.set", p);
  }
  hide() {
    try {
      call("widget.hide", { id: this.id });
    } catch (e) {
      if (!(e instanceof PtyrcError) || (e.code !== "no-sprite" && e.code !== "gone")) throw e;
    }
    this.info = { ...this.info, state: "hidden", variant: null, size: null, rect: null, sprite: null };
    this.hidden = true;
  }
  ask(opts = {}) {
    const p = { id: this.id };
    for (const k of ["quiet", "escape", "close"]) if (opts[k] != null) p[k] = opts[k];
    const answer = call("widget.ask", p);
    if (answer?.widget) this.info = answer.widget;
  }
  release() {
    try {
      call("widget.release", { id: this.id });
    } catch (e) {
      if (!(e instanceof PtyrcError) || (e.code !== "no-sprite" && e.code !== "gone")) throw e;
    }
    this.info = { ...this.info, asks: null, focused: false };
  }
  toString() {
    return `Widget(${this.id}, ${this.tag}, ${this.variant})`;
  }
}

export class Deck {
  #defaults;
  #every;
  #batch = true;
  #subscribed = new Set();
  #sent = new Map(); // widget id + variant -> content JSON
  #over = null;
  #overUntil = null;
  #keys = new Map();
  constructor(options = {}) {
    const { every, hoverTimeout, ...show } = options;
    for (const k of Object.keys(show)) if (!SHOW.includes(k)) throw bad(`new Deck(options): ${k}: not one of ${SHOW.join(", ")}, every, hoverTimeout`);
    this.#defaults = show;
    this.#every = !!every;
    this.widgets = [];
    this.now = 0;
    this.hoverTimeout = hoverTimeout === undefined ? 1500 : hoverTimeout;
  }

  #subscribe(...topics) {
    const fresh = topics.filter((t) => !this.#subscribed.has(t));
    if (!fresh.length) return;
    fresh.forEach((t) => this.#subscribed.add(t));
    try {
      call("subscribe", { topics: fresh });
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      // One topic refused refuses all: each alone.
      for (const t of fresh)
        try {
          call("subscribe", { topics: [t] });
        } catch (e2) {
          if (!(e2 instanceof PtyrcError)) throw e2;
        }
    }
  }

  // One request for all: `batch`, or one by one where it is not served.
  #send(calls) {
    if (!calls.length) return;
    if (calls.length > 1 && this.#batch) {
      try {
        const out = call("batch", { calls });
        for (const r of out.results ?? []) if (r.error) throw new PtyrcError(r.error);
        return;
      } catch (e) {
        if (!(e instanceof PtyrcError) || e.code !== "headless") throw e;
        this.#batch = false;
      }
    }
    for (const c of calls) call(c.method, c.params);
  }

  #declare(w, sizes) {
    w.sizes = sizes;
    const list = variants(sizes).map(([name, [vw, vh]]) => {
      const v = { tag: w.tag, size: [vw, vh], content: content(w.render(vw, vh, w)) };
      if (name !== null) v.name = name;
      return v;
    });
    call("widget.declare", { widgets: list });
    const show = { tag: w.tag };
    for (const k of SHOW) {
      const v = w.options[k] ?? this.#defaults[k];
      if (v != null) show[k] = v;
    }
    this.#forget(w);
    w.hidden = false;
    const answer = call("widget.show", show);
    if (answer?.widget) w.info = answer.widget;
    for (const v of list) this.#sent.set(`${w.id}:${v.name ?? `${v.size[0]}x${v.size[1]}`}`, JSON.stringify(v.content));
    if (w.options.key || w.options.paste) {
      const ask = w.options.ask ?? {};
      try {
        w.ask({ quiet: ask.quiet ?? true, escape: ask.escape, close: ask.close });
      } catch (e) {
        // A test run that does not focus: the widget is drawn all the same.
        if (!(e instanceof PtyrcError) || e.code !== "headless") throw e;
      }
    }
  }

  #forget(w) {
    const p = `${w.id}:`;
    for (const k of [...this.#sent.keys()]) if (k.startsWith(p)) this.#sent.delete(k);
  }

  add(tag, sizes, render, options = {}) {
    if (typeof tag !== "string" || !tag) throw bad("deck.add(tag, sizes, render): tag is a string");
    if (typeof render !== "function") throw bad("deck.add(tag, sizes, render): render(w, h, widget) is a function");
    if (this.widgets.some((w) => w.tag === tag)) throw bad(`deck.add: ${tag}: already in the deck`);
    this.#subscribe("widgets", "move");
    const w = new Widget(this, tag, render, options);
    if (options.key || options.paste) this.#keys.set(w, new Keys());
    if (options.hover || options.leave) this.#subscribe("hover");
    this.widgets.push(w);
    this.#declare(w, sizes);
    return w;
  }

  resize(widget, sizes) {
    this.#declare(widget, sizes);
  }

  move(frame) {
    this.#defaults.frame = frame;
    for (const w of this.widgets) {
      w.options = { ...w.options, frame };
      this.#declare(w, w.sizes);
    }
  }

  drop(widget) {
    if (widget === this.#over) this.#over = this.#overUntil = null;
    this.widgets = this.widgets.filter((w) => w !== widget);
    this.#keys.delete(widget);
    this.#forget(widget);
    widget.options.closed?.(widget);
  }

  draw(only, every = false) {
    const calls = [];
    for (const w of this.widgets) {
      if (only && w !== only) continue;
      // Not told yet which variant shows (a `widgets` event): each.
      const all = every || (!w.variant && !w.hidden && w.info.state !== "gone");
      const shown = (w.info.variants ?? []).filter((v) => all || v.name === w.variant);
      const list = shown.length || !w.size ? shown : [{ name: w.variant, size: w.size }];
      for (const v of list) {
        const c = content(w.render(v.size[0], v.size[1], w));
        const key = `${w.id}:${v.name}`;
        const text = JSON.stringify(c);
        if (this.#sent.get(key) === text) continue;
        this.#sent.set(key, text);
        calls.push({ method: "widget.set", params: { id: w.id, variant: v.name, content: c } });
      }
    }
    this.#send(calls);
  }

  tick(now) {
    if (typeof now === "number") this.now = now;
    if (this.#overUntil !== null && this.now >= this.#overUntil) this.leave();
    this.draw(undefined, this.#every);
  }

  leave() {
    const w = this.#over;
    this.#over = this.#overUntil = null;
    if (w && w.options.leave && w.options.leave(w) !== false) this.draw(w);
  }

  #hover(data) {
    let found = null;
    for (const w of this.widgets) {
      const h = w.options.hover;
      const { rect, size } = w;
      if (!h || data.sprite == null || w.info.sprite !== data.sprite || !rect || !size) continue;
      const x = data.x - rect[0] - ((rect[2] - size[0]) >> 1);
      const y = data.y - rect[1] - ((rect[3] - size[1]) >> 1);
      if (x >= 0 && x < size[0] && y >= 0 && y < size[1]) found = [w, x, y];
    }
    if (!found || found[0] !== this.#over) this.leave();
    if (!found) return;
    const [w, x, y] = found;
    this.#over = w;
    if (this.hoverTimeout != null) this.#overUntil = this.now + this.hoverTimeout;
    if (w.options.hover(x, y, w) !== false) this.draw(w);
  }

  #input(data) {
    const w = this.widgets.find((w) => this.#keys.has(w) && w.id === data.widget);
    if (!w) return false;
    const keys = this.#keys.get(w);
    if (data.released === "close") {
      w.info = { ...w.info, state: "gone", asks: null, focused: false };
      this.drop(w);
      return true;
    }
    let changed;
    if ("released" in data) {
      keys.reset();
      changed = w.focused;
      w.info = { ...w.info, focused: false };
    } else {
      // The key may come before the `widgets` event of the focus.
      changed = !w.focused;
      w.info = { ...w.info, focused: true };
      const fed = keys.feed(data);
      if (fed?.key !== undefined && w.options.key) changed = w.options.key(fed.key, w) !== false || changed;
      else if (fed?.paste !== undefined && w.options.paste) changed = w.options.paste(fed.paste, w) !== false || changed;
    }
    if (changed) this.draw(w);
    return true;
  }

  #widgets(data) {
    const w = this.widgets.find((w) => w.id === data?.id);
    if (!w) return false;
    const was = [w.variant, w.focused, w.asks];
    const before = w.info;
    w.info = data;
    if (w === this.#over && PLACE.some((k) => JSON.stringify(before[k]) !== JSON.stringify(data[k]))) this.leave();
    if (this.#keys.has(w) && !w.focused) this.#keys.get(w).reset();
    if (data.state === "gone") this.#forget(w);
    const now = [w.variant, w.focused, w.asks];
    if (now.some((v, i) => v !== was[i])) this.draw(w);
    return true;
  }

  event(topic, data) {
    switch (topic) {
      case "widgets":
        return this.#widgets(data);
      case "move":
        this.move(data.frame);
        return true;
      case "hover":
        this.#hover(data);
        return true;
      case "input":
        return this.#input(data);
      default:
        return false;
    }
  }
}

// ---- pieces of an interactive widget ----

function width(ch) {
  return cells(ch);
}

function head(text, n) {
  let used = 0;
  let i = 0;
  for (const ch of text) {
    used += width(ch);
    if (used > n) return text.slice(0, i);
    i += ch.length;
  }
  return text;
}

export function cut(text, n) {
  text = String(text);
  if (cells(text) <= n) return text;
  return n > 0 ? head(text, n - 1) + "…" : "";
}

export function fit(row, w) {
  if (typeof row === "string") row = [{ t: row }];
  const out = [];
  let room = Math.max(w, 0);
  for (const piece of row) {
    const t = head(piece.t, room);
    if (t) out.push({ ...piece, t }), (room -= cells(t));
    if (t !== piece.t) break;
  }
  if (room || !out.length) out.push({ t: " ".repeat(room) });
  return out;
}

export function wrap(text, w, n) {
  const lines = [];
  const words = String(text).split(/\s+/).filter(Boolean);
  while (words.length && lines.length < n) {
    let line = words.shift();
    while (words.length && cells(line) + 1 + cells(words[0]) <= w) line += " " + words.shift();
    lines.push(line);
  }
  if (words.length && lines.length) lines[lines.length - 1] += " " + words.join(" ");
  return lines.map((l) => cut(l, w));
}

function printable(name) {
  return [...name].length === 1 && !/\p{Cc}/u.test(name);
}

export class Field {
  #off = 0;
  constructor(text = "") {
    this.text = String(text);
    this.cursor = this.text.length;
  }
  insert(text) {
    text = String(text).replace(/\p{Cc}/gu, " ");
    this.text = this.text.slice(0, this.cursor) + text + this.text.slice(this.cursor);
    this.cursor += text.length;
  }
  // The code units of the character before and at the cursor.
  #back(at) {
    return at >= 2 && /[\udc00-\udfff]/.test(this.text[at - 1]) && /[\ud800-\udbff]/.test(this.text[at - 2]) ? 2 : 1;
  }
  #ahead(at) {
    return /[\ud800-\udbff]/.test(this.text[at] ?? "") ? 2 : 1;
  }
  key(name) {
    const { text } = this;
    const at = this.cursor;
    switch (name) {
      case "left":
      case "ctrl+b":
        this.cursor = Math.max(0, at - this.#back(at));
        break;
      case "right":
      case "ctrl+f":
        this.cursor = Math.min(text.length, at + this.#ahead(at));
        break;
      case "home":
      case "ctrl+a":
        this.cursor = 0;
        break;
      case "end":
      case "ctrl+e":
        this.cursor = text.length;
        break;
      case "backspace":
      case "ctrl+h": {
        const to = Math.max(0, at - this.#back(at));
        this.text = text.slice(0, to) + text.slice(at);
        this.cursor = to;
        break;
      }
      case "delete":
      case "ctrl+d":
        this.text = text.slice(0, at) + text.slice(at + this.#ahead(at));
        break;
      case "ctrl+u":
        this.text = text.slice(at);
        this.cursor = 0;
        break;
      case "ctrl+k":
        this.text = text.slice(0, at);
        break;
      case "ctrl+w": {
        const start = text.lastIndexOf(" ", text.slice(0, at).trimEnd().length - 1) + 1;
        this.text = text.slice(0, start) + text.slice(at);
        this.cursor = start;
        break;
      }
      case "space":
        this.insert(" ");
        break;
      default:
        if (!printable(name)) return false;
        this.insert(name);
    }
    return true;
  }
  render(w, focused, placeholder = "", style = {}) {
    if (!focused) {
      if (!this.text) return fit([{ t: placeholder, dim: true }], w);
      return fit([{ t: cut(this.text, w), ...style }], w);
    }
    const { text, cursor: at } = this;
    const under = at < text.length ? text.slice(at, at + this.#ahead(at)) : " ";
    this.#off = Math.min(this.#off, at);
    while (this.#off < at && cells(text.slice(this.#off, at)) + cells(under) > w) this.#off += this.#ahead(this.#off);
    const row = [
      { t: text.slice(this.#off, at), ...style },
      { t: under, ...style, inverse: true },
      { t: text.slice(at + under.length), ...style },
    ];
    if (!text) row.push({ t: placeholder, dim: true });
    return fit(row.filter((r) => r.t), w);
  }
}

export class Listing {
  #page = 1;
  constructor(count = 0, sel = 0) {
    this.count = count;
    this.sel = sel;
    this.top = 0;
  }
  key(name, letters = true) {
    if (letters && (name === "j" || name === "k")) name = name === "j" ? "down" : "up";
    const step = { up: -1, down: 1, tab: 1, pageup: -this.#page, pagedown: this.#page, home: -this.count, end: this.count }[name];
    if (step === undefined) return false;
    this.sel = Math.max(0, Math.min(this.count - 1, this.sel + step));
    return true;
  }
  window(h) {
    h = Math.max(h, 0);
    this.#page = Math.max(h, 1);
    this.sel = Math.max(0, Math.min(this.count - 1, this.sel));
    const top = Math.min(this.top, this.sel, Math.max(0, this.count - h));
    this.top = Math.max(top, this.sel - h + 1, 0);
    const out = [];
    for (let i = this.top; i < Math.min(this.count, this.top + h); i++) out.push(i);
    return out;
  }
  mark(h) {
    return this.count > Math.max(h, 0) ? `${this.sel + 1}/${this.count}` : "";
  }
}
