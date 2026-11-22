// SPDX-License-Identifier: GPL-3.0-or-later
// The tags of git, pure: `render*(repo, w, h, look)` give rows of
// styled runs, at most `h` of at most `w` cells (the twin's `git.py`).
// `repo`: the `repo` of a `probe.git` answer (`sdk/template/PROTOCOL.md`);
// null: no repository.

import { ALERT, Col, DIM, SPARK, WARN, ago, clip, fit, items, lab, len, plainly, slice, sp, table, val, width } from "./rows.js";
import { civil } from "ptyrc";

const BRANCH = 14;
const AHEAD = 2;
const BEHIND = 3;
const STAGED = 2;
const UNSTAGED = 3;
const OID = 3;
const STATE = 13;
const PULSE = 2;
/** The lanes of the graph, a colour each, in turn. */
const LANES = [6, 5, 4, 2, 3];

const NONE = ["not a git repository", "no repository", "no repo", "—"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** How the tags are drawn. `graph`: the log as a graph, with its refs;
 * `warn`: the warnings told in `head`; a tree is dirty after `dirty`
 * seconds, an operation left open after `open`. */
export function look(o = {}) {
  return { graph: false, warn: false, dirty: 4 * 3600, open: 15 * 60, ...o };
}

export const PLAIN = look();

/** Paths of `kind`; every one, where `changes` is cut. */
export function count(repo, kind) {
  return repo.counts?.[kind] ?? repo.changes.filter((c) => c.kind === kind).length;
}

const commits = (repo) => repo.log ?? [];

/** When `HEAD` was committed. */
export function last(repo) {
  return commits(repo).length ? commits(repo)[0].at : null;
}

/** `t` in `n` cells at most, its end kept: `…pps/git.py`. */
export function left(t, n) {
  if (len(t) <= n) return t;
  return n > 0 ? "…" + slice(t, len(t) - n + 1) : "";
}

/** What wants a look, the most pressing first. */
export function warnings(repo, lk = PLAIN) {
  if (repo == null) return [];
  const out = [];
  const n = count(repo, "conflict");
  if (n) out.push(`${n} conflict${n > 1 ? "s" : ""}`);
  let since = repo.state ? repo.state_since : null;
  if (since != null && repo.now - since >= lk.open) out.push(`${repo.state.split(/\s+/)[0]} open for ${ago(repo.now - since)}`);
  if (repo.head.gone) out.push("upstream gone");
  if (repo.head.behind) out.push(`${repo.head.behind} behind ${repo.head.upstream}`);
  since = repo.dirty_since;
  if (since != null && repo.now - since >= lk.dirty) out.push(`dirty for ${ago(repo.now - since)}`);
  return out;
}

/** `border` with the chase of a warning. */
export function chase(border) {
  return { base: border, chase: { len: 4, ms: 90, runs: 1, style: { fg: 11, bold: true } } };
}

// head

function name(head, n) {
  if (head.branch == null) return [lab("@"), val(clip(head.oid || "?", Math.max(1, n - 1)), OID)];
  return [val(clip(head.branch, n), BRANCH)];
}

function track(head, zero = false) {
  const out = [];
  for (const [mark, n, fg] of [["↑", head.ahead, AHEAD], ["↓", head.behind, BEHIND]]) {
    if (n || (zero && head.upstream && !head.gone)) {
      if (out.length) out.push(sp());
      out.push(n ? val(`${mark}${n}`, fg) : lab(`${mark}0`));
    }
  }
  return out;
}

function counts(repo, words = false) {
  const out = [];
  for (const [mark, kind, fg, word] of [
    ["✖", "conflict", ALERT, "conflict"],
    ["●", "staged", STAGED, "staged"],
    ["+", "unstaged", UNSTAGED, "changed"],
    ["?", "untracked", null, "new"],
  ]) {
    const n = count(repo, kind);
    if (!n) continue;
    if (out.length) out.push(sp(words ? 2 : 1));
    out.push(fg !== null ? val(`${mark}${n}`, fg) : lab(`${mark}${n}`));
    if (words) out.push(lab(` ${word}`));
  }
  return out.length ? out : [{ t: "✓", fg: STAGED }, ...(words ? [lab(" clean")] : [])];
}

function lastLine(repo, w) {
  if (!commits(repo).length) return [lab("no commit yet")];
  const c = commits(repo)[0];
  return fit([{ t: c.oid, fg: OID }, sp(), lab(ago(repo.now - c.at)), sp(), { t: c.subject }], w);
}

export function renderHead(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  const head = repo.head;
  const state = repo.state ? [val(repo.state, STATE)] : [];
  const alerts = lk.warn ? warnings(repo, lk) : [];
  if (h === 1) {
    // Told in order: name, state, track, counts. An operation left open
    // and the counts weigh more than the name; else the name has six
    // cells kept for it.
    const parts = { 1: state, 2: track(head), 3: counts(repo) };
    const least = Math.min(6, len(head.branch || "@" + head.oid));
    const kept = new Map();
    let room = state.length ? w : w - least - 1;
    for (const n of state.length ? [1, 3, 0, 2] : [2, 3]) {
      if (n === 0) {
        if (room >= least) kept.set(0, []), (room -= least + 1);
      } else if (parts[n].length && width(parts[n]) <= room) {
        kept.set(n, parts[n]);
        room -= width(parts[n]) + 1;
      }
    }
    if (!state.length || !kept.size) kept.set(0, []);
    if (!kept.has(3) && repo.changes.length && room >= 1) kept.set(3, [val("*", UNSTAGED)]);
    if (kept.has(0)) {
      let others = 0;
      for (const [n, p] of kept) if (n) others += width(p) + 1;
      kept.set(0, name(head, Math.max(1, w - others)));
    }
    const row = [];
    for (const n of [...kept.keys()].sort()) row.push(...kept.get(n), sp());
    return [fit(row, w)];
  }
  let first = name(head, w);
  if (head.upstream) {
    let tail = [lab(" → "), lab(head.upstream)];
    if (head.gone) tail.push(sp(), val("gone", ALERT));
    const tr = track(head, true);
    if (tr.length) tail.push(sp(2), ...tr);
    if (width(first) + width(tail) > w) tail = [sp(), ...(tr.length ? tr : [val("gone", ALERT)])];
    first = [...name(head, Math.max(6, w - width(tail))), ...tail];
  } else if (head.branch != null && width(first) + 13 <= w) first.push(lab("  no upstream"));
  const rows = [fit(first, w)];
  const words = width(counts(repo, true)) + (state.length ? width(state) + 2 : 0) <= w;
  const second = counts(repo, words);
  rows.push(fit(state.length ? [...state, sp(2), ...second] : second, w));
  for (const a of alerts) rows.push(fit([{ t: "▲ ", fg: WARN }, { t: a, fg: WARN }], w));
  rows.push(lastLine(repo, w));
  const extra = [];
  if (repo.stashes.length) extra.push(val(`≡${repo.stashes.length}`), lab(" stashed"));
  if (repo.worktrees.length > 1) extra.push(...(extra.length ? [sp(2)] : []), val(`${repo.worktrees.length}`), lab(" worktrees"));
  if (extra.length) rows.push(fit(extra, w));
  return rows.slice(0, h);
}

// status

const KINDS = { conflict: 0, staged: 1, unstaged: 2, untracked: 3 };

function more(cols, cells, w, h) {
  if (cells.length <= h) return table(cols, cells, w, false);
  return [...table(cols, cells.slice(0, h - 1), w, false), fit([lab(`+${cells.length - h + 1} more`)], w)];
}

export function renderStatus(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  if (!repo.changes.length) return plainly(["working tree clean", "clean", "✓"], w);
  let added = 0;
  let deleted = 0;
  for (const c of repo.changes) (added += c.added || 0), (deleted += c.deleted || 0);
  const total = [];
  if (added) total.push({ t: `+${added}`, fg: STAGED });
  if (deleted) total.push(...(total.length ? [sp()] : []), { t: `−${deleted}`, fg: ALERT });
  if (h === 1) return [items(total.length ? [counts(repo), total] : [counts(repo)], w, undefined, false)];
  const paths = new Map();
  for (const c of repo.changes) {
    if (!paths.has(c.path)) paths.set(c.path, {});
    paths.get(c.path)[c.kind] = c;
  }
  const rank = (p) => Math.min(...Object.keys(paths.get(p)).map((k) => KINDS[k]));
  const order = [...paths.keys()].sort((a, b) => rank(a) - rank(b));
  const cells = order.map((path) => {
    const got = paths.get(path);
    let x;
    let y;
    if (got.conflict) x = y = ["U", { fg: ALERT, bold: true }];
    else if (got.untracked) x = y = ["?", { fg: DIM }];
    else {
      x = [got.staged ? got.staged.code : " ", { fg: STAGED }];
      y = [got.unstaged ? got.unstaged.code : " ", { fg: UNSTAGED }];
    }
    let plus = 0;
    let minus = 0;
    for (const c of Object.values(got)) (plus += c.added || 0), (minus += c.deleted || 0);
    const style = got.untracked ? { fg: DIM } : {};
    return [
      () => [{ t: x[0], ...x[1] }, { t: y[0], ...y[1] }],
      (n) => [{ t: left(path, n), ...style }],
      [plus ? `+${plus}` : "", { fg: STAGED }],
      [minus ? `−${minus}` : "", { fg: ALERT }],
    ];
  });
  return statusRows(cells, w, h, repo);
}

// A row a path: its two letters, the path with its end kept, the lines
// added and deleted in two columns at the right.
function statusRows(cells, w, h, repo) {
  let cut = -repo.changes.length;
  for (const k of Object.keys(KINDS)) cut += count(repo, k);
  const room = cells.length <= h && !cut ? h : h - 1;
  const shown = cells.slice(0, Math.max(0, room));
  let wide = [2, 3].map((i) => Math.max(0, ...shown.map((c) => len(c[i][0]))));
  if (w < 24) wide = [0, 0];
  const tail = wide.reduce((s, n) => s + (n ? n + 1 : 0), 0);
  const rows = [];
  for (const line of shown) {
    const row = [...line[0](2), sp(), ...line[1](Math.max(1, w - 3 - tail))];
    if (tail) {
      row.push(sp(w - tail - width(row)));
      [line[2], line[3]].forEach(([t, style], i) => {
        if (wide[i]) row.push(sp(1 + wide[i] - len(t)), { t, ...style });
      });
    }
    rows.push(fit(row, w));
  }
  if (cells.length + cut > rows.length) rows.push(fit([lab(`+${cells.length + cut - rows.length} more`)], w));
  return rows;
}

// log

function refs(c) {
  const out = [];
  for (const ref of c.refs) {
    let run;
    if (ref === "HEAD") run = val("HEAD", BRANCH);
    else if (ref.startsWith("HEAD -> ")) run = val(ref.slice(8), BRANCH);
    else if (ref.startsWith("tag: ")) run = { t: ref.slice(5), fg: 11 };
    else if (ref.includes("/")) run = { t: clip(ref, 16), fg: 1 };
    else run = { t: clip(ref, 16), fg: 2 };
    if (out.length) out.push(sp());
    out.push(run);
  }
  return out;
}

const GRAPH = { "*": "●", "|": "│", "/": "╱", "\\": "╲", _: "─" };

function lanes(t) {
  return [...t].map((ch, x) => (ch === " " ? { t: " " } : { t: GRAPH[ch] ?? ch, fg: LANES[Math.floor(x / 2) % LANES.length] }));
}

const pad2 = (n) => String(n).padStart(2, "0");

/** `%b %d %H:%M` of `at` (epoch seconds), `offset(at)` east of UTC. */
export function stamp(at, offset = () => 0) {
  const c = civil(Math.floor(at) * 1000, offset(at));
  return `${MONTHS[c.month - 1]} ${pad2(c.day)} ${pad2(c.hour)}:${pad2(c.minute)}`;
}

/** The rows of the log, each with the index of its commit; the commit
 * `focus` told in full: its subject wrapped, its author and date under
 * it. `offset(at)`: seconds east of UTC at `at`, for the date. */
export function logRows(repo, w, h, lk = PLAIN, focus = null, offset = undefined) {
  const out = [];
  const log = commits(repo);
  let lane = Math.max(0, ...log.slice(0, h).map((c) => len(c.graph[0])));
  if (!log.slice(0, h).length) lane = 1;
  lane = Math.min(lane, Math.max(1, Math.floor(w / 4)));
  for (let i = 0; i < log.length && out.length < h; i++) {
    const c = log[i];
    const age = ago(repo.now - c.at);
    const row = [];
    if (lk.graph) row.push(...lanes(slice(c.graph[0], 0, lane)), sp(lane - len(c.graph[0]) + 1));
    row.push({ t: c.oid, fg: OID }, sp(Math.max(1, 4 - len(age))), lab(age), sp());
    const room = w - width(row);
    if (i === focus && room >= 8) {
      const lines = [""];
      for (const word of c.subject.split(/\s+/).filter(Boolean)) {
        if (lines[lines.length - 1] && len(lines[lines.length - 1]) + 1 + len(word) > room) lines.push("");
        lines[lines.length - 1] += (lines[lines.length - 1] ? " " : "") + word;
      }
      const pad = sp(w - room);
      out.push([i, fit([...row, val(lines[0])], w)]);
      for (const t of lines.slice(1, 3)) out.push([i, fit([pad, val(t)], w)]);
      out.push([i, fit([pad, lab(`${c.author} · ${stamp(c.at, offset)}`)], w)]);
      const r = refs(c);
      if (r.length) out.push([i, fit([pad, ...r], w)]);
      continue;
    }
    const r = lk.graph ? refs(c) : [];
    if (r.length && 2 * width(r) <= room) row.push(...r, sp());
    out.push([i, fit([...row, { t: c.subject }], w)]);
    if (lk.graph) for (const extra of c.graph.slice(1)) out.push([i, fit(lanes(slice(extra, 0, lane)), w)]);
  }
  return out.slice(0, h);
}

export function renderLog(repo, w, h, lk = PLAIN, focus = null, offset = undefined) {
  if (repo == null) return plainly(NONE, w);
  if (!commits(repo).length) return plainly(["no commit yet", "no commit", "—"], w);
  if (h === 1) return [lastLine(repo, w)];
  return logRows(repo, w, h, lk, focus, offset).map(([, row]) => row);
}

/** The commit under the pointer, for `renderLog(focus)`. `move`,
 * `leave`: true when the log is to be drawn again. */
export class Hover {
  constructor(offset = undefined) {
    this.focus = null;
    this.rows = [];
    this.offset = offset;
  }

  /** `renderLog` with the focus; the rows drawn kept, to tell which
   * commit a cell is of. */
  drawn(repo, w, h, lk) {
    if (repo == null || !commits(repo).length || h === 1) {
      this.rows = [];
      return renderLog(repo, w, h, lk);
    }
    const rows = logRows(repo, w, h, lk, this.focus, this.offset);
    this.rows = rows.map(([i]) => i);
    return rows.map(([, row]) => row);
  }

  move(x, y) {
    const focus = y >= 0 && y < this.rows.length ? this.rows[y] : null;
    const changed = focus !== this.focus;
    this.focus = focus;
    return changed;
  }

  leave() {
    const changed = this.focus !== null;
    this.focus = null;
    return changed;
  }
}

// branches, stash, worktrees

const mark = (current) => (current ? "● " : "  ");

function tracked(b) {
  if (b.gone) return ["gone", { fg: ALERT }];
  const t = [["↑", b.ahead], ["↓", b.behind]].filter(([, n]) => n).map(([m, n]) => `${m}${n}`).join(" ");
  return [t, { fg: b.behind ? BEHIND : AHEAD }];
}

export function renderBranches(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  if (!repo.branches.length) return plainly(["no branch yet", "no branch", "—"], w);
  if (h === 1) {
    const others = repo.branches.length - (repo.head.branch != null ? 1 : 0);
    const tail = others ? [lab(`+${others}`)] : [];
    const room = w - (tail.length ? width(tail) + 1 : 0);
    return [fit([...name(repo.head, Math.max(4, room)), sp(), ...tail], w)];
  }
  const any = repo.branches.some((b) => b.gone || b.ahead || b.behind);
  const cols = [Col({ flex: true, least: 10 }), Col({ right: true }), ...(any ? [Col({ drop: 1 })] : []), Col({ flex: true, drop: 2, least: 10 })];
  const cells = repo.branches.map((b) => [
    [mark(b.current) + b.name, b.current ? { fg: BRANCH, bold: true } : {}],
    [ago(repo.now - b.at), { fg: DIM }],
    ...(any ? [tracked(b)] : []),
    [b.subject, { fg: DIM }],
  ]);
  return more(cols, cells, w, h);
}

/** `WIP on topic: 1a2b3c4 subject` as `topic: subject`; the branch
 * `current` is not named. */
export function stashed(subject, current = null) {
  for (const lead of ["WIP on ", "On "]) {
    if (!subject.startsWith(lead)) continue;
    const body = subject.slice(lead.length);
    const at = body.indexOf(": ");
    if (at < 0) return subject;
    const branch = body.slice(0, at);
    let rest = body.slice(at + 2);
    if (lead === "WIP on ") {
      const sp1 = rest.indexOf(" ");
      rest = (sp1 < 0 ? "" : rest.slice(sp1 + 1)) || rest;
    }
    return branch === current ? rest : `${branch}: ${rest}`;
  }
  return subject;
}

export function renderStash(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  if (!repo.stashes.length) return plainly(["nothing stashed", "no stash", "—"], w);
  if (h === 1) {
    const n = repo.stashes.length;
    const age = ago(repo.now - repo.stashes[0].at);
    return [items([[val(`≡${n}`), lab(" stashed")], [lab(`last ${age}`)]], w, undefined, false)];
  }
  const cols = [Col({ right: true, least: 1 }), Col({ right: true }), Col({ flex: true, least: 8 })];
  const cells = repo.stashes.map((s) => [
    [String(s.n), { fg: OID }],
    [ago(repo.now - s.at), { fg: DIM }],
    [stashed(s.subject, repo.head.branch), {}],
  ]);
  return more(cols, cells, w, h);
}

export function renderWorktrees(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  const trees = repo.worktrees;
  if (trees.length < 2) return plainly(["no other worktree", "one worktree", "—"], w);
  if (h === 1) return [fit([val(String(trees.length)), lab(" worktrees")], w)];
  const cols = [Col({ flex: true, least: 8 }), Col({ flex: true, least: 6 }), Col({ drop: 1 })];
  const cells = trees.map((t) => {
    const flags = t.flags.filter((f) => f !== "detached").join(" ");
    const at = t.branch ? [t.branch, { fg: 2 }] : ["@" + t.oid, { fg: OID }];
    return [
      [mark(t.current) + t.name, t.current ? { bold: true } : {}],
      t.flags.includes("bare") ? ["", {}] : at,
      [flags, { fg: t.flags.includes("prunable") ? WARN : DIM }],
    ];
  });
  return more(cols, cells, w, h);
}

// pulse

/** Dots of a braille cell, by column then row from the bottom. */
const DOTS = [
  [0x40, 0x04, 0x02, 0x01],
  [0x80, 0x20, 0x10, 0x08],
];

const ceilDiv = (a, b) => Math.floor((a + b - 1) / b);

/** Commits a day: one cell a day in one row, eighths high; a dot column
 * a day in more, on a base line, the last day at the right. */
export function renderPulse(repo, w, h, lk = PLAIN) {
  if (repo == null) return plainly(NONE, w);
  if (last(repo) == null) return plainly(["no commit yet", "no commit", "—"], w);
  const all = repo.days;
  const age = ago(repo.now - last(repo));
  if (h === 1) {
    const n = Math.max(0, Math.min(all.length, w - len(age) - 1));
    const days = n ? all.slice(all.length - n) : [];
    const top = Math.max(1, ...days);
    const cells = days.map((d) => (d ? { t: SPARK[Math.min(7, ceilDiv(d * 7, top))], fg: PULSE } : lab(SPARK[0])));
    return [fit(cells.length ? [...cells, sp(), lab(age)] : [lab(age)], w)];
  }
  const rowsN = h - 1;
  const n = Math.min(all.length, 2 * w);
  const days = all.slice(all.length - n);
  const top = Math.max(1, ...days);
  const levels = 4 * rowsN;
  const heights = [...new Array(Math.max(0, 2 * w - n)).fill(0), ...days.map((d) => 1 + (d ? ceilDiv(d * (levels - 1), top) : 0))];
  const chart = [];
  for (let r = rowsN - 1; r >= 0; r--) {
    let t = "";
    for (let x = 0; x < w; x++) {
      let bits = 0;
      for (const side of [0, 1]) {
        const filled = heights[2 * x + side] - 4 * r;
        for (let dot = 0; dot < Math.max(0, Math.min(4, filled)); dot++) bits |= DOTS[side][dot];
      }
      t += bits ? String.fromCharCode(0x2800 + bits) : " ";
    }
    chart.push(t.trim() ? [{ t: t.trimEnd(), fg: PULSE }] : []);
  }
  const week = all.slice(-7).reduce((s, d) => s + d, 0);
  const caption = [
    [val(String(week)), lab(" this week")],
    [lab("last "), val(age)],
    [lab(`${n} days, top `), val(String(top))],
  ];
  return [...chart, items(caption, w, undefined, false)];
}

// The tags

/** `[name, title, sizes, render]`. */
export const TAGS = [
  ["head", "git", [[12, 1], [24, 1], [34, 3], [46, 5]], renderHead],
  ["status", "status", [[16, 1], [28, 4], [40, 8], [56, 12]], renderStatus],
  ["log", "log", [[24, 1], [34, 4], [48, 8], [64, 14]], renderLog],
  ["branches", "branches", [[16, 1], [30, 3], [44, 6], [60, 10]], renderBranches],
  ["stash", "stash", [[16, 1], [30, 3], [44, 5]], renderStash],
  ["worktrees", "worktrees", [[14, 1], [30, 3], [44, 5]], renderWorktrees],
  ["pulse", "pulse", [[14, 1], [24, 3], [40, 4], [56, 5]], renderPulse],
];
export const NAMES = TAGS.map(([n]) => n);
/** The option that adds each tag to `head`. */
export const FLAGS = ["status", "log", "branches", "stash", "worktrees", "pulse"];
/** The tag of `--merged`. */
export const MERGED = "git";

/** The tags drawn with `lk`: `{name: {name, title, sizes, render(repo, w, h)}}`;
 * `log`: in place of `renderLog` (a `Hover`'s `drawn`). */
export function tags(lk = PLAIN, log = null) {
  const out = {};
  for (const [n, title, sizes, render] of TAGS) {
    const r = n === "log" && log !== null ? log : render;
    out[n] = { name: n, title, sizes, render: (repo, w, h) => r(repo, w, h, lk) };
  }
  return out;
}

/** The sizes of `tag` cut to the rows `repo` fills; one row left: cut
 * to its cells too, no narrower than the smallest size. */
export function trimmed(tag, repo) {
  return tag.sizes.map(([w, h]) => {
    const rows = tag.render(repo, w, h);
    if (rows.length === 1 && 1 < h) w = Math.min(w, Math.max(width(rows[0]), tag.sizes[0][0]));
    return [w, Math.max(1, Math.min(h, rows.length))];
  });
}

/** Variants by rank: `v0`, `v1`, ...; two of one size allowed. */
export function named(sizes) {
  const out = {};
  sizes.forEach((s, n) => (out[`v${n}`] = s));
  return out;
}

/** A tag with nothing to tell: hidden, unless named by `--monitor`. */
export function quiet(n, repo) {
  if (repo == null) return n !== "head";
  if (n === "stash") return !repo.stashes.length;
  return n === "worktrees" && repo.worktrees.length < 2;
}

export function title(n, repo, known) {
  return n === "head" && repo != null ? repo.name : known[n].title;
}
