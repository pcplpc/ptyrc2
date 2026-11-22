// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of twister.js: draws of CPython 3.13 `random.Random(n)`.

import { assertEqual, assertThrows } from "ptyrc";
import { Twister } from "./twister.js";

export function test_twister_draws() {
  let r = new Twister(3);
  const got = [[0, 1, 2].map(() => r.randrange(10, 30)), r.randint(20, 50), r.choice([..."abcdefg"])];
  got.push(r.sample([true, false], 2), r.weighted(["a", "b", "c"], [5, 1, 3]), r.random());
  assertEqual(got, [[17, 28, 27], 24, "c", [false, true], "b", 0.9088184001853248]);
  // Two words of key; a sample from a set.
  r = new Twister(2 ** 32 + 7);
  const all = Array.from({ length: 100 }, (_, i) => i);
  assertEqual([r.sample(all, 8), r.uniform(4.0, 14.0)], [[28, 97, 45, 43, 7, 49, 83, 6], 4.6486019194073025]);
}

export function test_twister_bounds() {
  const r = new Twister(1);
  assertThrows(() => r.randrange(3, 3), RangeError);
  assertThrows(() => r.choice([]), RangeError);
  assertThrows(() => r.sample([1], 2), RangeError);
  assertEqual(r.randrange(1), 0);
}
