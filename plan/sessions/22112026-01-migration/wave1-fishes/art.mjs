// SPDX-License-Identifier: GPL-3.0-or-later
// reef.js's art as JSON, as art.py prints the twin's.
import * as r from "../../../../plugins/store-apps/_shared/js/reef.js";
const shape = (s) => [s.name, s.frames, s.inks, s.ms, s.weight, s.front];
const sorted = (v) => (Array.isArray(v) ? v.map(sorted) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sorted(v[k])])) : v);
const out = {
  decor: r.DECOR.map(shape),
  sunk: r.SUNK.map((s) => [shape(s.fall), shape(s.rest), s.speed]),
  crossers: r.CROSSERS.map((c) => [c.name, c.frames, c.inks, c.speed, c.ms, c.rises, c.scary]),
  species: r.SPECIES.map((k) => [k.name, k.right, k === r.CLASSIC ? null : k.colors]),
  mirror: r.mirror(["/\\()<>[]{}`'ab"]),
};
console.log(JSON.stringify(sorted(out)));
