// SPDX-License-Identifier: GPL-3.0-or-later
// Sand: a thin stream piles on the text of the frame.
//
// One request: a sprite whose content is the effect `sand` of the
// driver. The simulation runs there; it reads the text under it as its
// walls at each render and answers nothing (`plan/store-model.md`,
// principle 2). The app holds no permission and then only waits. The
// twin of `sdk/rust/examples/sand`.

import { call, exit, log, random } from "ptyrc";

function u32() {
  const b = random(4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

export function init(args) {
  let rate = 60;
  if (Number.isFinite(args.rate) && args.rate > 0) rate = Math.min(args.rate, 1000);
  const params = { rate, cursor: args.cursor === true, seed: u32() };
  try {
    call("draw.put", { effect: { name: "sand", params } });
  } catch (e) {
    log(`sand: the effect: ${e.code}: ${e.message}`);
    exit(1);
  }
}
