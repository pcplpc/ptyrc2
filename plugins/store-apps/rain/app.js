// SPDX-License-Identifier: GPL-3.0-or-later
// Rain: quiet rain behind the text.
//
// The effect `rain` of the driver plays it, 25 frames a second: drops
// splash on the top of the lines of text, on the cursor and on the
// bottom of the pane, and fall behind the text between two lines; less
// on the columns that changed in the last ten seconds. `-t`: they fall
// behind every line and the cursor, down to the last row. The app puts
// it once; the driver moves it with the frame.

import { call, exit, log, random } from "ptyrc";

function u32() {
  const b = random(4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

/** The params of the effect. */
export function params(args, seed) {
  const out = { rate: args.rate, seed };
  if (args.transparent === true) out.transparent = true;
  return out;
}

export function init(args) {
  try {
    call("draw.put", { effect: { name: "rain", params: params(args, u32()) } });
  } catch (e) {
    log(`rain: the effect: ${e.code}: ${e.message}`);
    exit(1);
  }
}
