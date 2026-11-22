// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of monitor, `ptyrc dev test --unit`: those of the twin
// (test_monitor.py) on the pure parts, `Source` on a fake probe, and
// the twin's looks by hashes (`twin.js`). The parsers and the reads of
// the machine are the probe's (`rust/ptyrc-probe`); the units, `bar`,
// `fit`, `table` the tests of `_shared/js/rows.js`.

import { PtyrcError, assert, assertEqual, assertThrows } from "ptyrc";
import { Source, TOP, options } from "./app.js";
import { ALERT, WARN, len } from "./rows.js";
import { MONITORS, NAMES, mergedSizes, named, renderConnected, renderDisks, renderGlance, renderListen, trimmed } from "./monitor.js";
import { FACES, MERGED, STATES, TRIMS, WHOLE } from "./twin.js";

const S = Object.keys(STATES);
const data = (s, n) => STATES[s][MONITORS[n].part];
const render = (s, n, w, h) => MONITORS[n].render(data(s, n), w, h);
const text = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const split = (t) => t.split(/\s+/).filter(Boolean);

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

export function test_twin_faces() {
  let bad = 0;
  const first = [];
  for (const [si, mi, w, h, hash] of FACES) {
    const rows = render(S[si], NAMES[mi], w, h);
    if (fnv(JSON.stringify(norm(rows))) !== hash) {
      bad += 1;
      if (first.length < 3) first.push(`${S[si]} ${NAMES[mi]} ${w}x${h}: ${JSON.stringify(norm(rows))}`);
    }
  }
  assertEqual(bad, 0, `${bad} of ${FACES.length} faces differ: ${first.join("\n")}`);
}

export function test_twin_whole_trims_merged() {
  for (const [si, mi, w, h, runs] of WHOLE) assertEqual(norm(render(S[si], NAMES[mi], w, h)), runs, `${S[si]} ${NAMES[mi]} ${w}x${h}`);
  for (const [si, mi, sizes] of TRIMS) assertEqual(trimmed(NAMES[mi], (w, h) => render(S[si], NAMES[mi], w, h)), sizes, `${S[si]} ${NAMES[mi]}`);
  const lists = [NAMES.slice(0, 1), NAMES.slice(0, 2), NAMES.slice(0, 3), NAMES, NAMES.slice(-2)];
  lists.forEach((l, i) => assertEqual(mergedSizes(l), MERGED[i], l.join(",")));
}

// test_every_variant
export function test_every_variant() {
  for (const n of NAMES) {
    const m = MONITORS[n];
    assert(3 <= m.sizes.length && m.sizes.length <= 4);
    assert(m.sizes.every(([w, h]) => w <= 56 && h <= 14));
    for (const s of ["full", "empty"]) {
      for (const [w, h] of [...m.sizes, [1, 1], [5, 2], [12, 3], [25, 7], [80, 20]]) {
        const rows = render(s, n, w, h);
        assert(rows.length <= h && rows.length > 0 && rows[0].length > 0, `${n} ${s} ${w}x${h}`);
        for (const row of rows) {
          assert(row.reduce((k, r) => k + len(r.t), 0) <= w, `${n} ${w}x${h}`);
          for (const r of row) assert(Object.keys(r).every((k) => ["t", "fg", "bold"].includes(k)));
        }
      }
      for (const [w, h] of trimmed(n, (w, h) => render(s, n, w, h))) assert(render(s, n, w, h).length > 0);
    }
  }
}

export function test_not_held() {
  // Every part empty: as the twin's empty data.
  const none = { glance: {}, logins: {}, listeners: [], disks: [], degree: "none" };
  for (const n of NAMES) for (const [w, h] of MONITORS[n].sizes) assertEqual(MONITORS[n].render(none[MONITORS[n].part], w, h), render("empty", n, w, h));
  assertEqual(text(renderGlance(null, 18, 1)), ["cpu 0%  mem 0%"]);
}

export function test_tiny_variants() {
  const shown = Object.fromEntries(NAMES.map((n) => [n, text(render("full", n, ...MONITORS[n].sizes[0]))]));
  assertEqual(shown.glance, ["cpu 50%  mem 62%"]);
  assertEqual(shown.connected, ["bob 1  alice 2  +2"]);
  assertEqual(shown["tcp-listen"], ["22 443 8000 8080"]);
  assertEqual(shown["disk-space"], ["/ 98%  /boot 25%  +2"]);
}

export function test_glance_large() {
  const [w, h] = MONITORS.glance.sizes[3];
  const rows = text(render("full", "glance", w, h));
  assertEqual(rows.length, h);
  assert(rows[0].startsWith("cpu  ━") && rows[0].endsWith("50%"));
  assertEqual(rows[1], "core ▅▃");
  assert(rows[4].startsWith("load 0.52 0.48 0.40") && rows[4].endsWith("up 3d 4h"));
  assertEqual(rows[5], "net  ↓ 410K/s   ↑ 205K/s");
  assertEqual(split(rows[7]), ["pid", "process", "cpu", "mem"]);
  assertEqual(split(rows[8]), ["1", "p1", "150%", "3.9M"]);
}

export function test_warnings() {
  const full = renderDisks(data("full", "disk-space"), 44, 5);
  assertEqual(full[1].find((r) => r.t === "98%")?.fg, ALERT);
  const loaded = { cpu: 0.1, cores: [0.1, 0.1], load: [5, 1, 1], memory: { total: 1, available: 1 }, uptime: 0, rx: 0, tx: 0, top: [] };
  assert(renderGlance(loaded, 30, 4).flat().some((r) => r.t === "5.00" && r.fg === ALERT));
  const calm = renderGlance(data("full", "glance"), 30, 4).flat();
  assert(!calm.some((r) => r.fg === ALERT || r.fg === WARN));
}

export function test_tables() {
  let rows = text(renderListen(data("full", "tcp-listen"), 56, 10));
  assertEqual(split(rows[0]), ["port", "address", "user", "process"]);
  assertEqual(split(rows[1]), ["22", "*", "root", "sshd"]);
  assertEqual(split(rows[3]), ["8000", "0.0.0.0", "alice", "?"]);
  rows = text(renderConnected(data("full", "connected"), 32, 3));
  assertEqual([rows.length, rows[2]], [3, "+2 more"]);
  rows = text(renderConnected(data("full", "connected"), 56, 10));
  assertEqual(split(rows[0]), ["user", "from", "tty", "since", "idle", "what"]);
  assertEqual(split(rows[2]).slice(0, 3), ["alice", "tmux", "2"]);
  assertEqual(rows[1].indexOf("pts/5"), rows[0].indexOf("tty"));
  assertEqual(rows[1].indexOf("30s") + 3, rows[0].indexOf("idle") + 4);
  assert(rows.find((r) => r.startsWith("carol")).includes("…"));
  // The login time in the local zone.
  const one = { now: 1_800_000_000, seats: [{ user: "a", origin: "local", ttys: ["tty1"], login: 1_800_000_000 - 3600, idle: 1, what: null }] };
  assert(text(renderConnected(one, 56, 10, () => 3600))[1].includes("08:00"));
}

export function test_empty_states() {
  assertEqual(text(renderListen([], 30, 3)), ["nothing listens outside"]);
  assertEqual(text(renderListen([], 18, 1)), ["none outside"]);
  assertEqual(text(renderConnected({ now: 0, seats: [] }, 20, 1)), ["no one logged in"]);
  assertEqual(text(renderDisks([], 18, 1)), ["no local disks"]);
}

export function test_trimmed() {
  assertEqual(trimmed("tcp-listen", (w, h) => renderListen([], w, h)), MONITORS["tcp-listen"].sizes.map(([w]) => [w, 1]));
  const one = data("full", "disk-space").slice(0, 1);
  const t = trimmed("disk-space", (w, h) => renderDisks(one, w, h));
  assertEqual(t.map(([, h]) => h), [1, 2, 2, 2]);
  MONITORS["disk-space"].sizes.forEach(([w, h], i) => assertEqual(renderDisks(one, ...t[i]), renderDisks(one, w, h)));
  assertEqual(named([[1, 2], [3, 4]]), { v0: [1, 2], v1: [3, 4] });
}

export function test_merged() {
  const names = ["glance", "disk-space", "tcp-listen"];
  const sizes = mergedSizes(names);
  assertEqual(sizes.length, 4);
  for (const n of names) MONITORS[n].sizes.forEach(([w, h], i) => assert(w <= sizes[i][0] && h <= sizes[i][1]));
  assertEqual(names.map((n) => MONITORS[n].title), ["glance", "disks", "listening"]);
}

export function test_options() {
  const base = { every: 2, cycle: 8, merged: false, monitor: null };
  assertEqual(options(base), { names: NAMES, merged: false, every: 2000, cycle: 8000 });
  assertEqual(options({ ...base, monitor: "disk-space,glance, glance", merged: true }).names, ["disk-space", "glance"]);
  const e = assertThrows(() => options({ ...base, monitor: "glance,cpu" }), RangeError);
  assert(e.message.includes("unknown monitor cpu") && e.message.includes("glance, connected, tcp-listen, disk-space"));
  assertThrows(() => options({ ...base, monitor: " , " }), RangeError);
  assertThrows(() => options({ ...base, every: 0 }), RangeError);
  assertThrows(() => options({ ...base, cycle: -1 }), RangeError);
}

export function test_source() {
  const asked = [];
  let fail = false;
  const s = new Source((method, params) => {
    asked.push([method, params]);
    if (fail) throw new PtyrcError({ code: "not-usable", message: "x" });
    return STATES.full;
  });
  assertEqual(s.data.glance, null);
  assert(s.read(["glance", "disk-space"]));
  assertEqual(asked[0], ["probe.system", { parts: ["glance", "disks"], top: TOP }]);
  assertEqual(s.data.glance, STATES.full.glance);
  assertEqual(s.data.connected, null);
  const kept = s.data.glance;
  fail = true;
  assert(!s.read(["glance"]));
  assertEqual(s.data.glance, kept);
}
