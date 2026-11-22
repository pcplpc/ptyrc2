// SPDX-License-Identifier: GPL-3.0-or-later
// The wall: which picture hangs on which nail, and until when. Time is
// an argument, in seconds; the draws are a `Twister`'s (Python's
// `random.Random`: the twin's order for a seed).

/** Python's `random.shuffle`, in place. */
export function shuffle(rng, list) {
  for (let i = list.length - 1; i > 0; i--) {
    const j = rng.below(i + 1);
    [list[i], list[j]] = [list[j], list[i]];
  }
}

/** A nail and the picture on it. `move`, `leave`: the pointer over its
 * inner box; `retitle`: called when the plate changes. */
export class Slot {
  constructor(picture, o = {}) {
    this.picture = picture;
    this.since = 0;
    this.until = 0;
    this.now = 0;
    this.moving = !!o.moving;
    this.author = !!o.author;
    this.hover = !!o.hover;
    this.look = null;
    this.size = o.size ?? null;
    this.retitle = null;
  }

  get label() {
    return this.picture.label(this.author || (this.hover && this.look !== null));
  }

  /** Seconds into the picture's frames. */
  t() {
    return this.moving ? this.now - this.since : 0;
  }

  render(w, h) {
    return this.picture.render(w, h, this.t(), this.hover ? this.look : null);
  }

  #seen() {
    const s = this.size;
    if (s === null || !this.picture.sizes.has(`${s[0]}x${s[1]}`)) return this.label;
    return `${this.label}\n${JSON.stringify(this.picture.gaze(s[0], s[1], this.look, this.t()))}`;
  }

  // Whether what is seen changed; the plate is then set again.
  #point(look) {
    const before = this.#seen();
    this.look = look;
    if (!this.hover || this.#seen() === before) return false;
    this.retitle?.();
    return true;
  }

  move(x, y) {
    return this.#point([x, y]);
  }

  leave() {
    return this.#point(null);
  }
}

export class Hall {
  /** `rng`: `below(n)`, as a `Twister`. */
  constructor(pictures, rng, o = {}) {
    if (!pictures.length) throw new RangeError("no picture to hang");
    this.pictures = [...pictures];
    this.every = o.every ?? 600;
    this.rng = rng;
    this.pile = [];
    this.slots = [];
    const count = Math.max(1, Math.min(o.count ?? 1, this.pictures.length));
    for (let nth = 0; nth < count; nth++) {
      const slot = new Slot(this.#draw(), o);
      // Staggered: the wall never changes all at once.
      slot.until = (this.every * (nth + 1)) / count;
      this.slots.push(slot);
    }
  }

  #draw() {
    const hung = this.slots.map((s) => s.picture);
    if (!this.pile.length) {
      this.pile = [...this.pictures];
      shuffle(this.rng, this.pile);
    }
    for (let nth = 0; nth < this.pile.length; nth++)
      if (!hung.includes(this.pile[nth]) || this.pictures.length <= hung.length) return this.pile.splice(nth, 1)[0];
    this.pile = [];
    return this.#draw();
  }

  /** Whether a picture is ever replaced: more pictures than nails. */
  get changes() {
    return this.pictures.length > this.slots.length;
  }

  /** When the next picture is hung. */
  get due() {
    return Math.min(...this.slots.map((s) => s.until));
  }

  /** The shortest period of the pictures that move; null: all still. */
  period() {
    const ps = this.slots.filter((s) => s.moving && s.picture.frames > 1).map((s) => s.picture.period);
    return ps.length ? Math.min(...ps) : null;
  }

  /** The next time after `t` something is seen to change: a picture
   * hung, a beat of one that moves; Infinity: never. */
  next(t) {
    let out = this.changes ? this.due : Infinity;
    for (const s of this.slots) {
      if (!s.moving || s.picture.frames === 1) continue;
      const p = s.picture.period;
      out = Math.min(out, s.since + (Math.floor((t - s.since) / p) + 1) * p);
    }
    return out;
  }

  /** Another picture on `slot`, now. */
  hang(slot, t) {
    slot.picture = this.#draw();
    slot.since = t;
    slot.until = t + this.every;
    slot.look = null;
  }

  /** Moves to `t`; the slots that took a new picture. */
  at(t) {
    const fresh = [];
    for (const slot of this.slots) {
      slot.now = t;
      if (t >= slot.until && this.changes) {
        this.hang(slot, t);
        fresh.push(slot);
      }
    }
    return fresh;
  }
}
