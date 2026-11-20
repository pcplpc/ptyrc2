// SPDX-License-Identifier: GPL-3.0-or-later
// ptyrc dev test --unit
import { assertEqual } from "ptyrc";
import { clock, face } from "./face.js";

export function test_clock() {
  assertEqual(clock(0), "00:00");
  assertEqual(clock(61999), "01:01");
  assertEqual(clock(3600000), "60:00");
}

export function test_line_face() {
  assertEqual(face(5000, 5, 1, false), [[{ t: "00:05", fg: 14, bold: true }]]);
}

export function test_big_face() {
  const rows = face(72000, /* 01:12 */ 17, 3, false).runs();
  const text = rows.map((row) => row.map((r) => r.t).join(""));
  assertEqual(text, ["█▀█  █     █  ▀▀█", "█ █  █  ▪  █  █▀▀", "▀▀▀  ▀     ▀  ▀▀▀"]);
  assertEqual(rows[0][0], { t: "█▀█", fg: 14, bold: true });
}
