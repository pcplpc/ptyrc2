// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of sand: `ptyrc dev test --unit`.

import { assertEqual } from "ptyrc";
import { event, idle, options } from "./app.js";

export function test_options() {
  const o = options({ rate: 60, cursor: false, hover: false, bomb: false, now: false, all: false });
  assertEqual(o, { rate: 60, cursor: false, hover: false, bomb: false, now: false });
  assertEqual(options({ all: true }).cursor, true);
  assertEqual(options({ all: true }).bomb, true);
  assertEqual(options({ all: true }).now, true);
  assertEqual(options({ all: true }).hover, false);
  assertEqual(options({ rate: 5000 }).rate, 1000);
  assertEqual(options({ rate: -1 }).rate, 60);
}

// The frame told (`init`, `move`) named in each read.
export function test_frame_told() {
  const log = [];
  const c = (m, p) => (log.push([m, p]), { idle: true });
  assertEqual(idle(c), true);
  event("move", { frame: 7 });
  idle(c);
  assertEqual(log, [["frame.idle", { frame: 0 }], ["frame.idle", { frame: 7 }]]);
}
