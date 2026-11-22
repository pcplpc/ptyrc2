// SPDX-License-Identifier: GPL-3.0-or-later
// tank/app.js's Tank: rows after steps, as twin.py prints the twin's.
// The imports of "ptyrc" stubbed: a copy, stub/app.mjs (not kept).
import { readFileSync, writeFileSync } from "node:fs";
const here = new URL(".", import.meta.url).pathname;
const tree = here + "../../../../plugins/store-apps/tank/";
const src = readFileSync(tree + "app.js", "utf8")
  .replace('from "ptyrc";', `from "${here}stub/ptyrc.mjs";`)
  .replace('from "ptyrc/deck";', `from "${here}stub/deck.mjs";`)
  .replace('from "./reef.js";', `from "${tree}reef.js";`);
writeFileSync(here + "stub/app.mjs", src);
const { Tank, TANKS, TICK } = await import(here + "stub/app.mjs");
const { seeded } = await import(tree + "reef.js");
const out = [];
for (const more of [{}, { decorate: true, bubbles: true, species: true }])
  for (const [w, h] of [...TANKS, [30, 9]])
    for (const seed of [1, 2, 3]) {
      const tank = new Tank(w, h, seeded(seed), more);
      const frames = [];
      for (let i = 0; i < 80; i++) {
        if (i === 30) tank.onMove(Math.floor(w / 2), Math.floor(h / 2));
        if (i === 60) tank.onLeave();
        tank.step(TICK);
        if (i % 10 === 9) frames.push(tank.rows());
      }
      out.push(frames);
    }
console.log(JSON.stringify(out));
