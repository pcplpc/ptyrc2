// SPDX-License-Identifier: GPL-3.0-or-later
// The monitors of `monitor`: each the rows of a part of the
// `probe.system` answer in a box of `w` by `h` cells (the twin's
// `render_*`), its sizes, `trimmed`, `mergedSizes`.

import { ALERT, Col, DIM, GAP, WARN, ago, bar, fit, fixed, items, lab, level, listed, percent, plainly, rate, since, size, sp, span, spark, table, val, width } from "./rows.js";

const GLANCE = 14;
const CONNECTED = 13;
const LISTEN = 11;
const DISKS = 10;

/** An answer's glance, every field there (not held: `{}`). */
export function glance(g) {
  const m = { total: 0, available: 0, swap_total: 0, swap_free: 0, ...(g?.memory ?? {}) };
  return { cpu: 0, cores: [], load: [0, 0, 0], uptime: 0, rx: 0, tx: 0, top: [], ...(g ?? {}), memory: m };
}

const load = (g) => {
  const cores = Math.max(1, g.cores.length);
  const one = g.load[0];
  return one > 2 * cores ? ALERT : one > cores ? WARN : GLANCE;
};

export function renderGlance(g, w, h) {
  g = glance(g);
  const mm = g.memory;
  const mem = mm.total ? (mm.total - mm.available) / mm.total : 0;
  const swap = mm.swap_total ? (mm.swap_total - mm.swap_free) / mm.swap_total : 0;
  const up = [lab("up "), val(span(g.uptime))];
  const short = [
    [lab("cpu "), val(percent(g.cpu), level(g.cpu, GLANCE))],
    [lab("mem "), val(percent(mem), level(mem, GLANCE))],
    [lab("load "), val(fixed(g.load[0], 2), load(g))],
    up,
  ];
  if (h === 1) return [items(short, w, GAP, false)];
  if (w < 20) return short.map((p) => fit(p, w)).slice(0, h);

  const extras = {
    mem: [size(mm.total - mm.available), size(mm.total)],
    swap: [size(mm.swap_total - mm.swap_free), size(mm.swap_total)],
  };
  let extra = Math.max(...Object.values(extras).map(([a, b]) => [...`${a}/${b}`].length));
  let n = w - 10 - (GAP + extra);
  if (n < 12) (n = w - 10), (extra = 0);

  const meter = (name, share) => {
    const row = [lab(name.padEnd(5)), ...bar(share, n, level(share, GLANCE))];
    row.push(sp(n - width(row) + 5), val(percent(share).padStart(5), level(share, GLANCE)));
    if (extra && name in extras) row.push(sp(GAP), { t: extras[name][0] }, lab(`/${extras[name][1]}`));
    return row;
  };
  const loads = () => {
    const left = [lab("load "), val(fixed(g.load[0], 2), load(g)), sp(), { t: fixed(g.load[1], 2) }, sp(), { t: fixed(g.load[2], 2) }];
    for (const row of [left, left.slice(0, 2)]) if (width(row) + GAP + width(up) <= w) return [...row, sp(w - width(row) - width(up)), ...up];
    return left;
  };
  const net = () => [lab("net  "), lab("↓ "), val(rate(g.rx)), sp(3), lab("↑ "), val(rate(g.tx))];
  const cores = () => [lab("core "), ...g.cores.map((c) => spark(c, level(c, GLANCE)))];

  // Rows by priority, shown in reading order.
  const parts = { cpu: () => meter("cpu", g.cpu), mem: () => meter("mem", mem), load: loads, net };
  if (mm.swap_total) parts.swap = () => meter("swap", swap);
  if (g.cores.length > 1) parts.cores = cores;
  const chosen = Object.keys(parts).slice(0, h);
  const rows = ["cpu", "cores", "mem", "swap", "load", "net"].filter((k) => chosen.includes(k)).map((k) => fit(parts[k](), w));
  const room = h - rows.length;
  if (room >= 2 && g.top.length) {
    const procs = g.top.slice(0, room - 1);
    if (room - 1 > procs.length) rows.push([]);
    const cols = [Col({ head: "pid", right: true, drop: 2 }), Col({ head: "process", flex: true }), Col({ head: "cpu", right: true }), Col({ head: "mem", right: true, drop: 1 })];
    const cells = procs.map((p) => [[String(p.pid), { fg: DIM }], [p.name, {}], [percent(p.cpu), { fg: GLANCE }], [size(p.mem), {}]]);
    rows.push(...table(cols, cells, w, true));
  }
  return rows.slice(0, h);
}

/** `offset(at)`: seconds east of UTC at `at`, in the local zone. */
export function renderConnected(data, w, h, offset = () => 0) {
  const seats = data?.seats ?? [];
  if (!seats.length) return plainly(["no one logged in", "no one"], w);
  if (h === 1) {
    const counts = new Map();
    for (const s of seats) counts.set(s.user, (counts.get(s.user) ?? 0) + s.ttys.length);
    return [items([...counts].map(([u, n]) => [val(u, CONNECTED), lab(` ${n}`)]), w)];
  }
  const cols = [
    Col({ head: "user", flex: true, least: 5 }),
    Col({ head: "from", flex: true, least: 12 }),
    Col({ head: "tty", drop: 1 }),
    Col({ head: "since", drop: 2 }),
    Col({ head: "idle", right: true }),
    Col({ head: "what", flex: true, drop: 3, least: 6 }),
  ];
  const cells = seats.map((s) => [
    [s.user, { fg: CONNECTED, bold: true }],
    [s.origin, s.origin === "local" ? { fg: DIM } : {}],
    [s.ttys.length === 1 ? s.ttys[0] : `${s.ttys.length} ttys`, {}],
    [since(s.login, data.now, offset), { fg: DIM }],
    [ago(s.idle), {}],
    [s.what || "", { fg: DIM }],
  ]);
  return listed(cols, cells, w, h);
}

export function renderListen(data, w, h) {
  data = data ?? [];
  if (!data.length) return plainly(["nothing listens outside", "none outside", "none"], w);
  if (h === 1) {
    const ports = [...new Set(data.map((x) => x.port))].sort((a, b) => a - b);
    return [items(ports.map((p) => [val(String(p), LISTEN)]), w, 1)];
  }
  let cols = [Col({ head: "port", right: true }), Col({ head: "address", flex: true, least: 9 }), Col({ head: "user", drop: 2 }), Col({ head: "process", flex: true, drop: 1, least: 5 })];
  let cells = data.map((x) => [[String(x.port), { fg: LISTEN, bold: true }], [x.addr, {}], [x.user, { fg: DIM }], [x.process || "?", x.process ? {} : { fg: DIM }]]);
  if (!data.some((x) => x.process)) (cols = cols.slice(0, 3)), (cells = cells.map((r) => r.slice(0, 3)));
  return listed(cols, cells, w, h);
}

/** Used, as `df` counts it: of what is used and free to users. */
export const share = (d) => (d.used + d.free ? d.used / (d.used + d.free) : 0);

export function renderDisks(data, w, h) {
  data = data ?? [];
  if (!data.length) return plainly(["no local disks", "no disks"], w);
  if (h === 1) {
    const full = data.map((d, i) => [share(d), i, d]).sort((a, b) => b[0] - a[0] || a[1] - b[1]);
    return [items(full.map(([s, , d]) => [{ t: `${d.point} ` }, val(percent(s), level(s, DISKS))]), w)];
  }
  const cols = [
    Col({ head: "mount", flex: true, least: 8, most: Math.max(12, Math.floor(w / 3)) }),
    Col({ grow: true, drop: 2, least: 6 }),
    Col({ head: "used", right: true }),
    Col({ head: "free", right: true, drop: 1 }),
    Col({ head: "size", right: true, drop: 3 }),
  ];
  const cells = data.map((d) => {
    const s = share(d);
    return [[d.point, { fg: DISKS, bold: true }], (n) => bar(s, n, level(s, DISKS)), [percent(s), { fg: level(s, DISKS), bold: true }], [size(d.free), {}], [size(d.size), { fg: DIM }]];
  });
  return listed(cols, cells, w, h);
}

/** The monitors: `part` of the answer, `render(data, w, h, offset)`. */
export const MONITORS = {
  glance: { title: "glance", part: "glance", sizes: [[18, 1], [30, 4], [44, 9], [56, 13]], render: renderGlance },
  connected: { title: "connected", part: "logins", sizes: [[20, 1], [32, 3], [44, 6], [56, 10]], render: renderConnected },
  "tcp-listen": { title: "listening", part: "listeners", sizes: [[18, 1], [30, 3], [44, 6], [56, 10]], render: renderListen },
  "disk-space": { title: "disks", part: "disks", sizes: [[20, 1], [32, 3], [44, 5], [56, 8]], render: renderDisks },
};
export const NAMES = Object.keys(MONITORS);
/** The tag of `--merged`. */
export const MERGED = "monitor";

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The sizes of `name` cut to the rows `data` fills, when the render in
 * fewer rows is the same; widths stay. `render(w, h)`: of the data. */
export function trimmed(name, render) {
  const out = [];
  for (let [w, h] of MONITORS[name].sizes) {
    const rows = render(w, h);
    if (0 < rows.length && rows.length < h && same(render(w, rows.length), rows)) h = rows.length;
    if (!out.some(([a, b]) => a === w && b === h)) out.push([w, h]);
  }
  return out;
}

/** The n-th size of the merged tag: the largest width and height of
 * the n-th sizes of `names`. */
export function mergedSizes(names) {
  const every = names.map((n) => MONITORS[n].sizes);
  const ranks = Math.max(...every.map((s) => s.length));
  return Array.from({ length: ranks }, (_, r) => [0, 1].map((i) => Math.max(...every.map((s) => s[Math.min(r, s.length - 1)][i]))));
}

/** Variants by rank: `v0`, `v1`, ... */
export function named(sizes) {
  const out = {};
  sizes.forEach((s, n) => (out[`v${n}`] = s));
  return out;
}
