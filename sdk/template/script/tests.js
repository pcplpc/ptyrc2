// SPDX-License-Identifier: GPL-3.0-or-later
// ptyrc dev test --unit: each export test_*; no screen, no session.
import { assertEqual } from "ptyrc";
export function test_sum() { assertEqual(1 + 1, 2); }
