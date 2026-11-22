// SPDX-License-Identifier: GPL-3.0-or-later
// Marks: a highlighter for whatever runs in the terminal.
//
// One `tint` sprite a match, in frame cells: it keeps the program's
// glyphs and gives them the colours of its pattern; remembered rows
// too (`screen.find {known}`), 400 at most, nearest to the view first.
// The driver moves them with the text. On `screen`, `move`, `size`,
// after 30 ms of silence (200 ms at most), the app looks again: every
// search in one batch, every change in another. Topic `draw`: the
// sprites the driver removed with their rows are forgotten. A display
// of the counts (in view, `/known`), placed by the driver, in front
// (`--no-hud`: none).
//
// Patterns: the positionals literal, `-e RE` regular expressions; each
// is the user's, found without `text`.

import { call, exit, log, manifest, period } from "ptyrc";

/** Marker pens: background, foreground. */
export const INKS = [
  [226, 16],
  [120, 16],
  [81, 16],
  [213, 16],
  [215, 16],
  [203, 16],
];
export const MOST = 400; // marks at once, at most
const SETTLE = 30; // ms of silence that end a burst of changes
const BURST = 200; // ms a burst may last before a look
const PANEL = 236;

/** The patterns of `init`: `[{pattern, regex}]`, positionals first. */
export function patterns(args) {
  return [
    ...(args.patterns ?? []).map((pattern) => ({ pattern, regex: false })),
    ...(args.regex ?? []).map((pattern) => ({ pattern, regex: true })),
  ];
}

/** A mark's key: frame, x, y, pattern, text. */
const keyOf = (k) => JSON.stringify(k);

/**
 * What to mark, and what to count. `found`: `[frame, pattern, matches]`
 * of each search; `views`: `{frame: [first row, height]}` of each
 * frame's view. The marks `[frame, x, y, pattern, text]`, those nearest
 * to a view first, `most` at most; per pattern `[in view, known]`.
 */
export function wanted(found, views, most = MOST) {
  const n = 1 + Math.max(-1, ...found.map(([, p]) => p));
  const counts = Array.from({ length: n }, () => [0, 0]);
  const ranked = [];
  for (const [frame, p, matches] of found) {
    const [top, height] = views[frame];
    for (const m of matches) {
      if (!m.len) continue;
      counts[p][1] += 1;
      if (m.screen !== null && m.screen !== undefined) counts[p][0] += 1;
      const off = Math.max(top - m.y, m.y - (top + height - 1), 0);
      ranked.push([off, [frame, m.x, m.y, p, m.text]]);
    }
  }
  ranked.sort((a, b) => a[0] - b[0]); // stable: ties in the order found
  return [ranked.slice(0, most).map(([, k]) => k), counts];
}

/** The display: a row per pattern, in its ink, with its count in view,
 * and `/known` when there is more. */
export function hud(names, counts) {
  const shown = names.map((p) => ([...p].length <= 16 ? p : [...p].slice(0, 15).join("") + "…"));
  const width = Math.max(...shown.map((s) => [...s].length));
  const notes = counts.map(([seen, known]) => (seen === known ? `${seen}` : `${seen}/${known}`));
  const room = Math.max(...notes.map((s) => s.length));
  return shown.map((name, i) => {
    const [bg, fg] = INKS[i % INKS.length];
    return [
      { t: " ", bg: PANEL },
      { t: ` ${name.padEnd(width + name.length - [...name].length)} `, bg, fg },
      { t: ` ${notes[i].padStart(room)} `, bg: PANEL, fg: bg, bold: true },
    ];
  });
}

/** The marks on the driver, kept equal to the matches. `call`
 * injected. */
export class Marks {
  constructor(call, pats, ignoreCase, withHud) {
    this.call = call;
    this.pats = pats;
    this.frame = 0; // the frame told: at `init`, by `move`
    this.ignoreCase = ignoreCase;
    this.sprites = new Map(); // key -> sprite id
    this.hud = withHud ? null : false; // its id once put; false: none
    this.counts = null;
  }

  /** The view of the surface: `{id: [first row, height]}`. */
  views() {
    const f = this.call("frame.get", { frame: this.frame }).frame;
    return { [f.id]: [f.origin[1], f.rect[3]] };
  }

  /** Every pattern, in one batch. */
  search(frames) {
    const asked = frames.flatMap((frame) => this.pats.map((_, p) => [frame, p]));
    const calls = asked.map(([frame, p]) => ({
      method: "screen.find",
      params: { ...this.pats[p], ignore_case: this.ignoreCase, frame, known: true },
    }));
    const { results } = this.call("batch", { calls });
    const out = [];
    asked.forEach(([frame, p], i) => {
      const r = results[i];
      if (r.error) {
        if (r.error.code === "no-frame" || r.error.code === "not-usable") return;
        throw new Error(`${r.error.code}: ${r.error.message}`);
      }
      out.push([frame, p, r.result.matches]);
    });
    return out;
  }

  /** Search, then add and remove what differs. */
  look() {
    const views = this.views();
    const [marks, counts] = wanted(this.search(Object.keys(views).map(Number)), views);
    const want = new Set(marks.map(keyOf));
    const calls = [];
    const added = [];
    for (const [key, id] of this.sprites) {
      if (want.has(key)) continue;
      this.sprites.delete(key);
      calls.push({ method: "draw.remove", params: { id } });
    }
    for (const mark of marks) {
      const key = keyOf(mark);
      if (this.sprites.has(key)) continue;
      const [frame, x, y, p, text] = mark;
      const [bg, fg] = INKS[p % INKS.length];
      added.push(key);
      calls.push({
        method: "draw.put",
        params: {
          frame,
          anchor: { at: [x, y], space: "frame" },
          // The glyphs shown are the program's: these only say where.
          content: { rows: [text.replace(/\s/gu, "·")], style: { bg, fg } },
          blend: "tint",
        },
      });
    }
    while (counts.length < this.pats.length) counts.push([0, 0]);
    let hudAt = -1;
    if (this.hud !== false && JSON.stringify(counts) !== JSON.stringify(this.counts)) {
      const content = { cells: hud(this.pats.map((q) => q.pattern), counts), opaque: true };
      if (this.hud === null) {
        hudAt = calls.length;
        calls.push({
          method: "draw.put",
          params: { anchor: { auto: { prefer: "top-right" } }, layer: "front", z: 9, content },
        });
      } else calls.push({ method: "draw.update", params: { id: this.hud, content } });
    }
    this.counts = counts;
    if (!calls.length) return;
    const { results } = this.call("batch", { calls });
    let k = 0;
    results.forEach((r, i) => {
      if (r.error) {
        log(`marks: ${calls[i].method}: ${r.error.code}: ${r.error.message}`);
        return;
      }
      if (i === hudAt) this.hud = r.result.id;
      else if (calls[i].method === "draw.put") this.sprites.set(added[k++], r.result.id);
    });
    if (hudAt >= 0 && results[hudAt].error) this.hud = null;
  }

  /** Sprites the driver removed with the rows they were on. */
  forget(ids) {
    const gone = new Set(ids);
    for (const [key, id] of this.sprites) if (gone.has(id)) this.sprites.delete(key);
    if (gone.has(this.hud)) this.hud = null;
  }
}

let marks;
let first = null; // ms: the first event of a burst
let last = 0; // ms: its last

export function init(args) {
  const pats = patterns(args);
  if (!pats.length) {
    log("marks: a pattern at least: PATTERN or -e RE");
    exit(2);
  }
  marks = new Marks(call, pats, args["ignore-case"] === true, args.hud !== false);
  marks.frame = manifest()?.frame ?? 0;
  call("subscribe", { topics: ["screen", "draw", "move", "size"] });
  try {
    marks.look();
  } catch (e) {
    log(`marks: ${e.message}`); // a regular expression refused
    exit(1);
  }
}

export function tick(now) {
  if (first === null) return period(0);
  if (now - last < SETTLE && now - first < BURST) return;
  first = null;
  period(0);
  marks.look();
}

export function event(topic, data) {
  if (topic === "draw") marks.forget(data.removed);
  if (topic === "move") marks.frame = data.frame;
  const now = Date.now();
  if (first === null) {
    first = now;
    period(33);
  }
  last = now;
}
