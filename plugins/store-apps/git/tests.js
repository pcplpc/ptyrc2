// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of git, `ptyrc dev test --unit`: those of the twin
// (test_git.py) on the pure parts, `Source` on a fake probe, and the
// twin's looks by hashes (`twin.js`). The parsers and the reads of a
// real repository are the probe's (`rust/ptyrc-probe/tests`).

import { PtyrcError, assert, assertEqual, assertThrows } from "ptyrc";
import { LOG, Source, options } from "./app.js";
import { Hover, NAMES, chase, last, logRows, look, named, quiet, renderBranches, renderHead, renderLog, renderPulse, renderStash, renderStatus, renderWorktrees, stamp, stashed, tags, trimmed, warnings } from "./git.js";
import { FACES, FOCUS, LOOKS, QUIET, STATES, TRIMS, WARNS, WHOLE } from "./twin.js";

const NOW = 1_800_000_000;
const S = Object.keys(STATES);
const state = (n) => STATES[n].repo;
const text = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const cps = (t) => [...t].length;

/** Rows as the twin's script writes them: runs of one style merged,
 * empty runs out, `[t, fg, bold]`. */
function norm(rows) {
  return rows.map((row) => {
    const out = [];
    for (const r of row) {
      const t = String(r.t);
      if (!t) continue;
      const fg = r.fg ?? null;
      const bold = !!r.bold;
      const l = out[out.length - 1];
      if (l && l[1] === fg && l[2] === bold) l[0] += t;
      else out.push([t, fg, bold]);
    }
    return out;
  });
}

function fnv(t) {
  let h = 0x811c9dc5;
  for (const c of t) h = Math.imul(h ^ c.codePointAt(0), 0x01000193) >>> 0;
  return h;
}

const digest = (rows) => fnv(JSON.stringify(norm(rows)));

function face(si, li, ti, w, h, fi) {
  const r = state(S[si]);
  const lk = LOOKS[li];
  const n = NAMES[ti];
  if (n === "log") return renderLog(r, w, h, lk, FOCUS[fi]);
  return tags(lk)[n].render(r, w, h);
}

// -- the twin's looks ---------------------------------------------------------

export function test_twin_faces() {
  let bad = 0;
  const first = [];
  for (const [si, li, ti, w, h, fi, hash] of FACES) {
    const rows = face(si, li, ti, w, h, fi);
    if (digest(rows) !== hash) {
      bad += 1;
      if (first.length < 3) first.push(`${S[si]} look ${li} ${NAMES[ti]} ${w}x${h} focus ${FOCUS[fi]}: ${JSON.stringify(norm(rows))}`);
    }
  }
  assertEqual(bad, 0, `${bad} of ${FACES.length} faces differ: ${first.join("\n")}`);
}

export function test_twin_whole() {
  for (const [si, li, ti, w, h, fi, runs] of WHOLE) assertEqual(norm(face(si, li, ti, w, h, fi)), runs, `${S[si]} ${NAMES[ti]} ${w}x${h}`);
}

export function test_twin_trims_warnings_quiet() {
  for (const [si, li, ti, sizes] of TRIMS) assertEqual(trimmed(tags(LOOKS[li])[NAMES[ti]], state(S[si])), sizes, `${S[si]} ${li} ${NAMES[ti]}`);
  for (const [si, li, list] of WARNS) assertEqual(warnings(state(S[si]), LOOKS[li]), list, `${S[si]} ${li}`);
  QUIET.forEach((row, si) => assertEqual(NAMES.map((n) => quiet(n, state(S[si]))), row, S[si]));
}

// -- test_git.py ----------------------------------------------------------------

const repo = (over = {}) => ({ ...state("base"), ...over });
const head = (o = {}) => ({ oid: "", branch: null, upstream: null, ahead: 0, behind: 0, gone: false, initial: false, ...o });

export function test_head() {
  const r = repo();
  assertEqual(text(renderHead(r, 24, 1)), ["main ↑2 ↓1 ✖1 ●3 +2 ?1"]);
  assertEqual(text(renderHead(r, 12, 1)), ["main ↑2 ↓1 *"]);
  const rows = text(renderHead(r, 46, 5, look({ warn: true })));
  assertEqual(rows[0], "main → origin/main  ↑2 ↓1");
  assertEqual(rows[1], "✖1 conflict  ●3 staged  +2 changed  ?1 new");
  assertEqual(rows.slice(2, 5), ["▲ 1 conflict", "▲ 1 behind origin/main", "aaaaaaa 2m Merge side"]);
  const busy = repo({ state: "rebase 3/7" });
  assertEqual(text(renderHead(busy, 12, 1)), ["rebase 3/7 *"]);
  assert(text(renderHead(busy, 34, 3))[1].includes("rebase 3/7"));
  const lost = repo({ head: head({ oid: "abcdef1" }), changes: [], counts: {} });
  assertEqual(text(renderHead(lost, 24, 1)), ["@abcdef1 ✓"]);
  const long = repo({ head: head({ oid: "abcdef1", branch: "feature/" + "long-".repeat(12) }), changes: [], counts: {} });
  assertEqual(text(renderHead(long, 12, 1)), ["feature/l… ✓"]);
  assert(text(renderHead(long, 34, 3))[0].endsWith("…"));
}

export function test_status() {
  const r = state("lines");
  assertEqual(text(renderStatus(r, 28, 1)), ["✖1 ●3 +2 ?1  +12 −4"]);
  const rows = text(renderStatus(r, 40, 8));
  assertEqual(rows[0], "UU clash.py");
  assert(rows[1].startsWith("M  src/staged.py  ") && rows[1].endsWith("  +10 −2"), rows[1]);
  assertEqual(cps(rows[1]), 40);
  assert(rows[2].startsWith("MM both.py") && rows[2].endsWith(" +2 −2"), rows[2]);
  assertEqual(rows[rows.length - 1], "?? notes/");
  assertEqual(text(renderStatus(r, 16, 3)), ["UU clash.py", "M  src/staged.py", "+4 more"]);
  assertEqual(text(renderStatus(r, 12, 6))[4], " M … file.py");
  assertEqual(text(renderStatus(repo({ changes: [] }), 30, 4)), ["working tree clean"]);
  const many = text(renderStatus(state("many"), 30, 9));
  assertEqual(many[many.length - 1], "+999 more");
}

export function test_log() {
  const r = repo();
  assertEqual(text(renderLog(r, 24, 1)), ["aaaaaaa 2m Merge side"]);
  const plain = text(renderLog(r, 34, 4));
  assertEqual(plain[0], "aaaaaaa  2m Merge side");
  assert(plain[1].startsWith("bbbbbbb  1h a side commit") && plain[1].endsWith("…"), plain[1]);
  assertEqual(plain.slice(2), ["ccccccc  1d base"]);
  assertEqual(text(renderLog(r, 64, 8, look({ graph: true }))), [
    "●   aaaaaaa  2m main v1.0 origin/main Merge side",
    "│╲",
    "│ ● bbbbbbb  1h side a side commit with a subject long enough t…",
    "│╱",
    "●   ccccccc  1d base",
  ]);
  assertEqual(renderLog(r, 64, 2, look({ graph: true })).length, 2);
  assertEqual(text(renderLog(repo({ log: [] }), 30, 4)), ["no commit yet"]);
}

export function test_hover() {
  const r = repo();
  const lk = look();
  const hover = new Hover();
  assertEqual(hover.drawn(r, 40, 8, lk).length, 3);
  assertEqual(hover.rows, [0, 1, 2]);
  assert(!hover.leave() && !hover.move(3, 9));
  assert(hover.move(3, 1) && !hover.move(9, 1));
  assertEqual(text(hover.drawn(r, 40, 8, lk)), [
    "aaaaaaa  2m Merge side",
    "bbbbbbb  1h a side commit with a subject",
    "            long enough to wrap twice",
    "            Bob · Jan 15 07:00",
    "            side",
    "ccccccc  1d base",
  ]);
  assertEqual(hover.rows, [0, 1, 1, 1, 1, 2]);
  assert(hover.move(0, 5) && hover.focus === 2);
  assert(hover.leave() && hover.focus === null);
  assertEqual(text(hover.drawn(r, 24, 1, lk)), ["aaaaaaa 2m Merge side"]);
  assertEqual(hover.rows, []);
  // The date in the local zone: `offset(at)`.
  assertEqual(stamp(NOW - 3600, () => 3600), "Jan 15 08:00");
  const east = new Hover(() => -5 * 3600);
  east.move(0, 0);
  east.drawn(r, 40, 8, lk);
  east.move(0, 1);
  assertEqual(text(east.drawn(r, 40, 8, lk))[3], "            Bob · Jan 15 02:00");
  assertEqual(logRows(r, 40, 2, lk, 1).map(([i]) => i), [0, 1]);
}

export function test_lists() {
  const r = repo();
  assertEqual(text(renderBranches(r, 16, 1)), ["main +2"]);
  assertEqual(text(renderBranches(r, 60, 10)), ["● main   2m  ↑2 ↓1  Merge side", "  side   1h         a side commit", "  old   10d  gone   forgotten"]);
  assertEqual(text(renderBranches(r, 30, 2))[1], "+2 more");
  assertEqual(text(renderStash(r, 16, 1)), ["≡2 stashed"]);
  assertEqual(text(renderStash(r, 30, 1)), ["≡2 stashed  last 5m"]);
  assertEqual(text(renderStash(r, 44, 5)), ["0  5m  Merge side", "1  1d  side: tried another way"]);
  assertEqual(text(renderWorktrees(r, 14, 1)), ["3 worktrees"]);
  assertEqual(text(renderWorktrees(state("lines"), 44, 5)), ["● app      main", "  app-fix  @abcdef1  locked", "  gone     side      prunable"]);
  assertEqual(text(renderStash(repo({ stashes: [] }), 30, 3)), ["nothing stashed"]);
  assertEqual(stashed("WIP on main: abc x", "main"), "x");
  assertEqual(stashed("WIP on main: abc", "side"), "main: abc");
  assertEqual(stashed("On x"), "On x");
}

export function test_pulse() {
  const r = repo();
  assertEqual(text(renderPulse(r, 14, 1)), ["▁▃▁▂▆▁▁▃█▂▄ 2m"]);
  const rows = text(renderPulse(r, 24, 3));
  assertEqual(rows.length, 3);
  assertEqual(rows[2], "26 this week  last 2m");
  for (const row of rows.slice(0, 2)) for (const c of row) assert(c === " " || (c.codePointAt(0) >= 0x2800 && c.codePointAt(0) <= 0x28ff), row);
  assertEqual(rows[1].slice(-5), "⣆⣼⣀⣾⣼");
  assertEqual(rows[0].trim(), "⢠ ⢸");
  assertEqual(text(renderPulse(repo({ log: [] }), 24, 3)), ["no commit yet"]);
  assertEqual(last(repo({ log: [] })), null);
}

/** In its box: at most `h` rows of at most `w` cells. */
export function test_every_variant() {
  for (const lk of [look(), look({ graph: true, warn: true })]) {
    const known = tags(lk);
    for (const n of NAMES) {
      const tag = known[n];
      for (const data of [repo(), repo({ state: "merge", state_since: NOW - 7200 }), null]) {
        for (const [w, h] of [...tag.sizes, [1, 1], [3, 2], [7, 3], [200, 50]]) {
          const rows = tag.render(data, w, h);
          assert(rows.length > 0 && rows.length <= h, `${n} ${w}x${h}`);
          for (const row of text(rows)) assert(cps(row) <= w, `${n} ${w}x${h}: ${row}`);
        }
        const sizes = trimmed(tag, data);
        assertEqual(Object.keys(named(sizes)).length, tag.sizes.length);
        assert(sizes.every(([, h]) => h >= 1));
      }
    }
  }
}

export function test_warnings() {
  const lk = look();
  assertEqual(warnings(null), []);
  assertEqual(warnings(repo({ changes: [], counts: {}, head: head() })), []);
  const r = repo({ state: "rebase 3/7", state_since: NOW - 7200, dirty_since: NOW - 5 * 3600 });
  assertEqual(warnings(r, lk), ["1 conflict", "rebase open for 2h", "1 behind origin/main", "dirty for 5h"]);
  const fresh = repo({ changes: [], counts: {}, state: "merge", state_since: NOW - 60, dirty_since: NOW - 60 });
  assertEqual(warnings(fresh, lk), ["1 behind origin/main"]);
  assertEqual(warnings(state("gone"), lk), ["upstream gone"]);
  assertEqual(chase("round").base, "round");
  assert(chase("round").chase.len > 0);
}

export function test_quiet() {
  assert(!quiet("stash", repo()) && quiet("stash", repo({ stashes: [] })));
  assert(quiet("worktrees", repo({ worktrees: repo().worktrees.slice(0, 1) })));
  assert(!quiet("head", null) && quiet("log", null));
  assert(!quiet("log", repo({ log: [] })));
}

const ARGS = { every: 5, cycle: 8, dirty: 4, monitor: null, repo: null };

export function test_options() {
  let o = options(ARGS);
  assertEqual(o.names, ["head"]);
  assert(!o.named && !o.merged && o.repo === null);
  assertEqual(o.look, look());
  assertEqual(o.every, 5000);
  assertEqual(options({ ...ARGS, log: true, pulse: true }).names, ["head", "log", "pulse"]);
  assert(!options({ ...ARGS, log: true }).look.graph);
  assertEqual(options({ ...ARGS, graph: true }).names, ["head", "log"]);
  o = options({ ...ARGS, all: true, dirty: 2 });
  assertEqual(o.names, NAMES);
  assertEqual(o.look, look({ graph: true, warn: true, dirty: 7200 }));
  o = options({ ...ARGS, monitor: "pulse,log,pulse", merged: true, repo: { file: "f0", name: "src" } });
  assertEqual(o.names, ["pulse", "log"]);
  assert(o.named && o.merged && o.repo === "f0");
  assertThrows(() => options({ ...ARGS, monitor: "head,ci" }), RangeError);
  assertThrows(() => options({ ...ARGS, monitor: "," }), RangeError);
  assertThrows(() => options({ ...ARGS, every: 0 }), RangeError);
  assertThrows(() => options({ ...ARGS, cycle: -1 }), RangeError);
  try {
    options({ ...ARGS, monitor: "head,ci" });
  } catch (e) {
    assert(e.message.includes("unknown tag ci"), e.message);
  }
}

// -- Source: the probe asked as the twin read ---------------------------------

export function test_source() {
  const asked = [];
  const answers = [];
  const ask = (method, params) => {
    asked.push([method, params]);
    const a = answers.shift();
    if (a instanceof PtyrcError) throw a;
    return a;
  };
  const src = new Source(options({ ...ARGS, repo: { file: "f0", name: "x" } }), ask);
  answers.push(STATES.base);
  assert(src.poll(0));
  assertEqual(src.repo.name, "app");
  assertEqual(asked[0], ["probe.git", { log: LOG, repo: "f0" }]);
  // Within `every`: with the stamp; unchanged: not read.
  answers.push({ unchanged: true, stamp: "s0" });
  assert(!src.poll(1000));
  assertEqual(asked[1][1], { log: LOG, repo: "f0", since: "s0" });
  // A change of the git directory: a new stamp, read.
  answers.push({ ...STATES.busy, stamp: "s1" });
  assert(src.poll(2000));
  assertEqual(src.repo.state, "rebase 3/7");
  // Past `every` from the last read: without the stamp.
  answers.push(STATES.base);
  assert(src.poll(7000));
  assert(!("since" in asked[3][1]));
  // Refused within the second: kept, next time.
  answers.push(new PtyrcError({ code: "not-usable", message: "probe.git: one read a second" }));
  assert(!src.poll(8000));
  assertEqual(src.repo.name, "app");
  // Out of a repository, or not held: none.
  answers.push({ repo: null, stamp: "" });
  assert(src.poll(9000));
  assertEqual(src.repo, null);
  answers.push({ repo: null, degree: "none" });
  assert(src.poll(10000));
  assert(!("since" in asked[6][1]));
  // The program's directory: no `repo`.
  const child = new Source(options(ARGS), () => STATES.none);
  child.poll(0);
  assertEqual(child.params, { log: LOG });
}
