// SPDX-License-Identifier: GPL-3.0-or-later
// The module "ptyrc/sight": the shape of the view of a frame, kept by
// the topics (`_cli.Sight`). `ptyrc.d.ts` is its contract.

import { call as hostCall, PtyrcError } from "ptyrc";

// Topics a sight takes whole.
const OWN = ["frames", "text", "cursor"];

function pad(rows, w, h) {
  const out = new Array(h);
  for (let y = 0; y < h; y++) {
    const r = rows[y] ?? "";
    out[y] = r.length >= w ? r : r + " ".repeat(w - r.length);
  }
  return out;
}

export class Sight {
  #call;
  #stale = true;
  #at = null; // the last cursor told
  constructor(options = {}) {
    const { frame = "raw", words = [], text = false, cursor = false, call = hostCall } = options;
    if (typeof call !== "function") throw new TypeError("new Sight({call}): call(method, params) is a function");
    this.#call = call;
    this.frame = frame;
    this.words = [...words];
    this.text = !!text;
    this.wantsCursor = !!cursor;
    this.rows = [];
    this.size = [0, 0];
    this.rect = null;
    this.cursor = null;
    this.degree = null;
    this.changed = false;
    this.info = null;
    const topics = ["frames", "text", "size", "move"];
    if (cursor) topics.push("cursor");
    this.#subscribe(topics);
    this.#read();
    if (cursor) {
      try {
        this.#at = this.#call("screen.cursor");
      } catch (e) {
        if (!(e instanceof PtyrcError)) throw e;
      }
    }
  }

  #subscribe(topics) {
    try {
      this.#call("subscribe", { topics });
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      // One refused refuses all: each alone.
      for (const t of topics)
        try {
          this.#call("subscribe", { topics: [t] });
        } catch (e2) {
          if (!(e2 instanceof PtyrcError)) throw e2;
        }
    }
  }

  // The frame's info: where it is, whether it is on screen.
  #read() {
    try {
      const info = this.#call("frame.get", { frame: this.frame })?.frame ?? null;
      this.#take(info);
    } catch (e) {
      if (!(e instanceof PtyrcError)) throw e;
      if (e.code !== "no-frame" && e.code !== "not-usable") throw e;
      this.#take(null);
    }
  }

  #take(info) {
    const was = this.info;
    this.info = info;
    if (!info || !was || JSON.stringify([info.rect, info.origin]) !== JSON.stringify([was.rect, was.origin])) this.#stale = true;
    this.rect = info?.rect ?? null;
  }

  get usable() {
    return !!this.info && this.info.usable !== false && this.info.state !== "inactive";
  }

  #mine(id) {
    return id === this.frame || (this.info && id === this.info.id);
  }

  event(topic, data) {
    switch (topic) {
      case "frames": {
        const list = Array.isArray(data?.frames) ? data.frames : [];
        const info = list.find((f) => this.#mine(f.id) || f.name === this.frame);
        this.#take(info ?? (this.info ? { ...this.info, usable: false } : null));
        return true;
      }
      case "text":
        if (this.#mine(data?.frame)) {
          if (this.info) this.info = { ...this.info, origin: data.origin, rect: data.rect };
          this.rect = data.rect ?? this.rect;
          this.#stale = true;
        }
        return true;
      case "cursor":
        this.#at = data;
        return true;
      case "size":
        this.#stale = true;
        return false;
      case "move":
        if (data?.frame != null) this.frame = data.frame;
        this.#read();
        this.#stale = true;
        return false;
      default:
        return OWN.includes(topic);
    }
  }

  touch() {
    this.#stale = true;
  }

  look() {
    this.changed = false;
    if (!this.usable) {
      this.#read();
      if (!this.usable) return false;
    }
    if (this.#stale) {
      const params = { frame: this.info?.id ?? this.frame };
      if (!this.text && this.words.length) params.words = this.words;
      let r;
      try {
        r = this.#call(this.text ? "screen.text" : "screen.shape", params);
      } catch (e) {
        if (!(e instanceof PtyrcError) || e.code !== "not-usable") throw e;
        if (this.info) this.info = { ...this.info, usable: false };
        return false;
      }
      const [w, h] = r.rect ? [r.rect[2], r.rect[3]] : r.size;
      this.size = [w, h];
      this.rows = pad(r.lines ?? [], w, h);
      this.degree = r.degree ?? null;
      this.rect = r.rect ? [...r.rect] : this.rect;
      this.changed = true;
      this.#stale = false;
    }
    this.cursor = this.#cursor();
    return true;
  }

  #cursor() {
    const at = this.#at;
    if (!at || !at.visible || this.degree === "none") return null;
    let p;
    if (this.info && at.frame === this.info.id && Array.isArray(at.pos)) p = at.pos;
    else if (this.rect) p = [at.x - this.rect[0], at.y - this.rect[1]];
    else return null;
    return [p[0], p[1]];
  }

  ink(x, y) {
    const r = this.rows[y];
    return r !== undefined && x >= 0 && x < r.length && r[x] !== " ";
  }
}
