// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of foot.js (test_ui.py: test_foot).

import { assertEqual } from "ptyrc";
import { foot } from "./foot.js";

export function test_foot() {
  const keys = [["a", 0], ["bb", 2], ["cc", 1], ["dd", 2]];
  assertEqual(foot(keys, 20), "a  bb  cc  dd");
  assertEqual(foot(keys, 10), "a  bb  cc");
  assertEqual(foot(keys, 6), "a  cc");
  assertEqual(foot(keys, 2), "a");
  assertEqual(foot([["long text", 0]], 5), "long…");
}
