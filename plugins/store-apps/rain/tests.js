// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of rain: `ptyrc dev test --unit`.

import { assertEqual } from "ptyrc";
import { params } from "./app.js";

export function test_params() {
  assertEqual(params({ rate: 20, transparent: false }, 7), { rate: 20, seed: 7 });
  assertEqual(params({ rate: 300, transparent: true }, 1), { rate: 300, seed: 1, transparent: true });
}
