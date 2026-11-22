// SPDX-License-Identifier: GPL-3.0-or-later
// Rows of styled runs, in a box of cells (the twin's `monitor` helpers:
// `lab`, `val`, `fit`, `items`, `plainly`; the units `size`, `rate`,
// `percent`, `span`, `ago`, `since`; the marks `level`, `bar`, `spark`;
// `Col`, `table`, `listed`). A run is `{t, fg, bold}`; a row a list of
// runs. Lengths are code points, as the twin's `len`; numbers rounded
// half to even, as the twin's `round` and `format`.

export const DIM = 8;
export const ALERT = 1;
export const WARN = 3;
/** Cells between two columns. */
export const GAP = 2;
export const SPARK = "▁▂▃▄▅▆▇█";

const ASTRAL = /[\uD800-\uDFFF]/;

/** Code points of `s`. */
export function len(s) {
  return ASTRAL.test(s) ? [...s].length : s.length;
}

/** Code points `a` to `b` of `s` (`b` absent: the end). */
export function slice(s, a, b) {
  return ASTRAL.test(s) ? [...s].slice(a, b).join("") : s.slice(a, b);
}

/** A label: dim. */
export function lab(t) {
  return { t, fg: DIM };
}

/** A value: bold, in `fg` when given. */
export function val(t, fg = null) {
  return fg === null ? { t, bold: true } : { t, bold: true, fg };
}

export function sp(n = 1) {
  return { t: " ".repeat(Math.max(0, n)) };
}

export function width(row) {
  let n = 0;
  for (const r of row) n += len(r.t);
  return n;
}

/** `t` in `n` cells at most; cut with `…`. */
export function clip(t, n) {
  if (len(t) <= n) return t;
  return n > 0 ? slice(t, 0, n - 1) + "…" : "";
}

const trimEnd = (t) => t.replace(/\s+$/u, "");

function rstrip(row) {
  const out = row.filter((r) => r.t);
  while (out.length && !trimEnd(out[out.length - 1].t)) out.pop();
  if (out.length) out[out.length - 1] = { ...out[out.length - 1], t: trimEnd(out[out.length - 1].t) };
  return out;
}

/** `row` in `w` cells, trailing blanks dropped; cut with a dim `…`. */
export function fit(row, w) {
  row = rstrip(row);
  if (width(row) <= w) return row;
  const out = [];
  let room = w - 1;
  for (const r of row) {
    const n = len(r.t);
    if (n >= room) {
      out.push({ ...r, t: slice(r.t, 0, Math.max(0, room)) });
      break;
    }
    out.push(r);
    room -= n;
  }
  return w > 0 ? [...rstrip(out), lab("…")] : [];
}

/** Those of `parts` that fit in `w`, in order, `gap` apart; `+N` for
 * the others when `more`. */
export function items(parts, w, gap = GAP, more = true) {
  let out = [];
  let hidden = 0;
  parts.forEach((part, n) => {
    const left = parts.length - n - 1 + hidden;
    const tail = more && left ? len(`+${left}`) + gap : 0;
    if (width(out) + (out.length ? gap : 0) + width(part) + tail <= w) out = [...out, ...(out.length ? [sp(gap)] : []), ...part];
    else hidden += 1;
  });
  if (!out.length) return parts.length ? fit(parts[0], w) : [];
  return fit(more && hidden ? [...out, sp(gap), lab(`+${hidden}`)] : out, w);
}

/** An empty state: the longest of `phrases` that fits. */
export function plainly(phrases, w) {
  for (const t of phrases) if (len(t) <= w) return [[lab(t)]];
  return [fit([lab(phrases[phrases.length - 1])], w)];
}

/** `x` rounded half to even (Python's `round`). */
export function round(x) {
  const r = Math.round(x);
  return r - x === 0.5 && r % 2 !== 0 ? r - 1 : r;
}

/** `x` with `d` decimals, a tie to even (Python's `f"{x:.{d}f}"`). */
export function fixed(x, d) {
  const y = x * 10 ** d;
  // A tie is exact: `x` a multiple of 2^-(d+1) (exact to test).
  if (Number.isInteger(x * 2 ** (d + 1)) && Math.abs(y - Math.trunc(y)) === 0.5) return (round(y) / 10 ** d).toFixed(d);
  return x.toFixed(d);
}

/** Bytes, binary, three digits at most: `512B`, `3.2G`, `120G`. */
export function size(n) {
  for (const unit of "BKMGTPE") {
    if (n < 999.5 || unit === "E") {
      if (unit === "B") return `${fixed(n, 0)}B`;
      return n < 9.95 ? `${fixed(n, 1)}${unit}` : `${fixed(n, 0)}${unit}`;
    }
    n /= 1024;
  }
}

export const rate = (n) => `${size(n)}/s`;

export const percent = (share) => `${round(100 * share)}%`;

/** Two units: `3d 4h`, `4h 12m`, `12m`. */
export function span(seconds) {
  const all = Math.floor(seconds / 60);
  const d = Math.floor(all / 1440);
  const h = Math.floor(all / 60) % 24;
  const m = all % 60;
  if (d) return `${d}d ${h}h`;
  return h ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
}

/** One unit: `40s`, `5m`, `2h`, `3d`; empty when unknown. */
export function ago(seconds) {
  if (seconds == null) return "";
  for (const [unit, n] of [["d", 86400], ["h", 3600], ["m", 60]]) if (seconds >= n) return `${Math.floor(seconds / n)}${unit}`;
  return `${Math.trunc(seconds)}s`;
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const two = (n) => String(n).padStart(2, "0");

/** A login time: `14:09` today, `Tue 14:09` this week, else `Nov 02`.
 * Epoch seconds; `offset(at)`: seconds east of UTC at `at`, local. */
export function since(login, now, offset = () => 0) {
  const at = new Date(Math.floor(login + offset(login)) * 1000);
  const day = (t) => Math.floor((t + offset(t)) / 86400);
  const hm = `${two(at.getUTCHours())}:${two(at.getUTCMinutes())}`;
  if (now - login < 86400 && day(login) === day(now)) return hm;
  if (now - login < 6 * 86400) return `${DAYS[at.getUTCDay()]} ${hm}`;
  return `${MONTHS[at.getUTCMonth()]} ${two(at.getUTCDate())}`;
}

/** `fg`, or `ALERT` from `warn` on. */
export const level = (share, fg, warn = 0.9) => (share >= warn ? ALERT : fg);

/** A thin bar of `n` cells, by half cells: `━━━╾───`; a share above
 * zero shows. */
export function bar(share, n, fg) {
  let halves = round(Math.min(1, Math.max(0, share)) * n * 2);
  if (share > 0 && halves === 0) halves = 1;
  const full = Math.floor(halves / 2);
  const half = halves % 2;
  const out = rstrip([{ t: "━".repeat(full) + "╾".repeat(half), fg }, { t: "─".repeat(Math.max(0, n - full - half)), fg: DIM }]);
  return out.length ? out : [lab("")];
}

/** One cell, eighths high; zero dim. */
export function spark(share, fg) {
  if (share <= 0) return lab(SPARK[0]);
  return { t: SPARK[Math.min(7, Math.max(0, Math.ceil(share * 8) - 1))], fg };
}

/** A column of `table`. `flex`: cut to fit, the widest first, down to
 * `least`; `grow`: takes the room left, `least` at least (its cells are
 * functions of the width); `drop`: when the table is still too wide,
 * the highest goes first (0: never); `most`: the widest it gets (0: no
 * limit). */
export function Col(o = {}) {
  return { head: "", right: false, flex: false, grow: false, drop: 0, least: 4, most: 0, ...o };
}

/** Rows of aligned columns, `GAP` apart, in `w` cells. A cell:
 * `[text, style]`, or a function of its width giving a row. */
export function table(cols, cells, w, head) {
  const live = cols.map((_, i) => i);
  const shrink = (widths, excess, least) => {
    while (excess > 0) {
      let best = -1;
      for (const i of live) if (cols[i].flex && widths[i] > least(i) && (best < 0 || widths[i] > widths[best])) best = i;
      if (best < 0) break;
      widths[best] -= 1;
      excess -= 1;
    }
    return excess;
  };
  const natural = (i) => {
    if (cols[i].grow) return cols[i].least;
    const lens = cells.filter((row) => typeof row[i] !== "function").map((row) => len(row[i][0]));
    return Math.min(cols[i].most || w, Math.max(...lens, head ? len(cols[i].head) : 0));
  };
  let widths;
  let excess;
  for (;;) {
    widths = {};
    for (const i of live) widths[i] = natural(i);
    excess = live.reduce((n, i) => n + widths[i], 0) + GAP * (live.length - 1) - w;
    excess = shrink(widths, excess, (i) => cols[i].least);
    const drops = live.filter((i) => cols[i].drop);
    if (excess <= 0 || !drops.length) break;
    let worst = drops[0];
    for (const i of drops) if (cols[i].drop > cols[worst].drop) worst = i;
    live.splice(live.indexOf(worst), 1);
  }
  excess = shrink(widths, excess, (i) => Math.min(3, cols[i].least));
  const grow = live.filter((i) => cols[i].grow);
  if (excess < 0 && grow.length) widths[grow[0]] -= excess;
  const line = (texts) => {
    const out = [];
    live.forEach((i, n) => {
      if (n) out.push(sp(GAP));
      const cell = texts[i];
      if (typeof cell === "function") {
        const runs = cell(widths[i]);
        out.push(...runs, sp(widths[i] - width(runs)));
        return;
      }
      const t = clip(cell[0], widths[i]);
      const pad = sp(widths[i] - len(t));
      const run = { t, ...cell[1] };
      if (cols[i].right) out.push(pad, run);
      else out.push(run, pad);
    });
    return fit(out, w);
  };
  const rows = head ? [line(cols.map((c) => [c.head, { fg: DIM }]))] : [];
  return [...rows, ...cells.map(line)];
}

/** `table` in `h` rows: a head from 4 rows on, or when all fits with
 * it; `+N more` for the rows left out. */
export function listed(cols, cells, w, h) {
  const head = 0 < cells.length && (h >= 4 || cells.length < h);
  const room = h - head;
  if (cells.length > room) return [...table(cols, cells.slice(0, Math.max(0, room - 1)), w, head), fit([lab(`+${cells.length - room + 1} more`)], w)];
  return table(cols, cells, w, head);
}
