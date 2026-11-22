// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of rows.js; the expected rows are the twin's
// (`monitor.table`, `listed`, ... on the same input).

import { assertEqual } from "ptyrc";
import { ALERT, Col, ago, bar, clip, fit, fixed, items, lab, len, level, listed, percent, plainly, rate, round, since, size, sp, span, spark, table, val } from "./rows.js";

const text = (rows) => rows.map((row) => row.map((r) => r.t).join(""));

export function test_rows_runs() {
  assertEqual(len("a😀b"), 3);
  assertEqual(clip("abcdef", 4), "abc…");
  assertEqual(clip("abc", 0), "");
  assertEqual(clip("😀😀😀", 2), "😀…");
  assertEqual(text([fit([{ t: "abc  " }, sp(3)], 10)]), ["abc"]);
  assertEqual(text([fit([{ t: "abcdef" }, { t: "gh" }], 5)]), ["abcd…"]);
  assertEqual(fit([{ t: "abc" }], 0), []);
  assertEqual(sp(-2).t, "");
  assertEqual(val("x", 2), { t: "x", bold: true, fg: 2 });
}

export function test_rows_items() {
  const parts = [[{ t: "aaaa" }], [{ t: "bb" }], [{ t: "cccccc" }]];
  assertEqual(text([items(parts, 30)]), ["aaaa  bb  cccccc"]);
  assertEqual(text([items(parts, 12)]), ["aaaa  bb  +1"]);
  assertEqual(text([items(parts, 12, 2, false)]), ["aaaa  bb"]);
  assertEqual(text([items(parts, 2)]), ["a…"]);
  assertEqual(text(plainly(["not here", "none", "—"], 5)), ["none"]);
  assertEqual(text(plainly(["long one"], 3)), ["lo…"]);
}

export function test_rows_ago() {
  assertEqual([ago(null), ago(5.9), ago(60), ago(7199), ago(86400 * 3)], ["", "5s", "1m", "1h", "3d"]);
}

export function test_rows_table() {
  const cols = [Col({ flex: true, least: 4 }), Col({ right: true }), Col({ drop: 1 })];
  const cells = [
    [["alpha-beta", {}], ["12", {}], ["x", {}]],
    [["g", {}], ["3", {}], ["yy", {}]],
  ];
  assertEqual(text(table(cols, cells, 40, false)), ["alpha-beta  12  x", "g            3  yy"]);
  assertEqual(text(table(cols, cells, 14, false)), ["alpha…  12  x", "g        3  yy"]);
  assertEqual(text(table(cols, cells, 10, false)), ["alpha…  12", "g        3"]);
  const grow = [Col({ grow: true, least: 3 }), Col()];
  assertEqual(text(table(grow, [[(n) => [lab("#".repeat(n))], ["ab", {}]]], 10, false)), ["######  ab"]);
  const head = [Col({ head: "name" }), Col({ head: "n", right: true })];
  const many = [["a", {}], ["1", {}]];
  assertEqual(text(listed(head, [many, many, many, many, many], 20, 4)), ["name  n", "a     1", "a     1", "+3 more"]);
  assertEqual(text(listed(head, [many], 20, 2)), ["name  n", "a     1"]);
}

export function test_rows_units() {
  const G = 2 ** 30;
  assertEqual([0, 512, 1000, 3.2 * G, 120 * G, 1.5 * 2 ** 40].map(size), ["0B", "512B", "1.0K", "3.2G", "120G", "1.5T"]);
  assertEqual([size(2.5), size(1023 * 1024), rate(2048)], ["2B", "1.0M", "2.0K/s"]);
  assertEqual([round(0.5), round(1.5), round(2.5), round(-0.5), round(2.4)], [0, 2, 2, -0, 2]);
  assertEqual([fixed(0.25, 1), fixed(0.35, 1), fixed(1.005, 2), fixed(0.125, 2)], ["0.2", "0.3", "1.00", "0.12"]);
  assertEqual([percent(0.625), percent(0.635), percent(0.005), percent(1)], ["62%", "64%", "0%", "100%"]);
  assertEqual([span(277000), span(4000), span(59), span(720)], ["3d 4h", "1h 06m", "0m", "12m"]);
}

export function test_rows_since() {
  const now = 1_800_000_000; // 2027-01-15 08:00 UTC, a Friday
  assertEqual(since(now - 3600, now), "07:00");
  assertEqual(since(now - 9 * 3600, now), "Thu 23:00");
  assertEqual(since(now - 3600, now, () => -7.5 * 3600), "Thu 23:30");
  assertEqual(since(now - 5 * 86400, now), "Sun 08:00");
  assertEqual(since(now - 9e5, now), "Jan 04");
}

export function test_rows_marks() {
  const t = (row) => row.map((r) => r.t).join("");
  assertEqual([t(bar(0.5, 4, 1)), t(bar(0.3, 5, 1)), t(bar(0.001, 3, 1)), t(bar(2, 3, 1)), t(bar(0, 0, 1))], ["━━──", "━╾───", "╾──", "━━━", ""]);
  assertEqual(bar(0.25, 2, 5)[0], { t: "╾", fg: 5 });
  assertEqual([spark(0, 2), spark(0.5, 2), spark(1, 2), spark(0.01, 2)].map((r) => r.t).join(""), "▁▄█▁");
  assertEqual(spark(0, 2).fg, 8);
  assertEqual([level(0.89, 5), level(0.9, 5), level(0.5, 5, 0.4)], [5, ALERT, ALERT]);
}
