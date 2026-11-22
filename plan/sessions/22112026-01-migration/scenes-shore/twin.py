# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's shore as digests: the fixtures of shore/src/twin.rs.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/scenes-shore/twin.py
> ../../store-apps/shore/src/twin.rs"""

import json
import random
import struct

from ptyrc_apps import shore
from ptyrc_apps._strip import Pointer


def fnv(text):
    h = 2166136261
    for b in text.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def floats(xs):
    return fnv(",".join(str(bits(x)) for x in xs))


def text(grid, w):
    return "\n".join(
        "".join(f"{x} {g} {ink}," for x, (g, ink) in sorted(row.items()) if 0 <= x < w)
        for row in grid
    )


WORDS = ["", "", "words in the way", "", "   more words here"]
LOW = [""] * 20 + ["   a line of text low on the pane, in the water"]
BLOCK = [""] * 30 + [" " * 40 + "x" * 60] * 6
TEXTS = {"none": [], "words": WORDS, "low": LOW, "block": BLOCK}

out = []
w = out.append
w("// SPDX-License-Identifier: GPL-3.0-or-later")
w("//! The twin's shore, as digests: written by")
w("//! `plan/sessions/22112026-01-migration/scenes-shore/twin.py`.")
w("")
w("#![allow(clippy::type_complexity)]")
w("")

net = shore.lace()
w("/// `lace()`: the digest of `rank,` a dot; `grit()`: of the bits of each.")
w(f"pub const LACE: u32 = {fnv(''.join(f'{round(v * 4096)},' for row in net for v in row))};")
w(f"pub const GRIT: u32 = {floats([v for row in shore.grit() for v in row])};")
w("")

w("/// `Shore(Random(seed), period, ebb).wave(k, across, h)`: digests of")
w("/// `late`, `run`, `[at, bend]`; then `edge(px, t, across, h)` at 12 times.")
w("pub const WAVES: &[(u64, f64, f64, i64, usize, usize, u32, u32, u32, u32)] = &[")
for seed, period, ebb, k, across, h in ((1, 8.0, 300.0, 2, 160, 24), (5, 3.0, 50.0, -4, 400, 60), (9, 8.0, 300.0, 77, 2, 1)):
    s = shore.Shore(random.Random(seed), period=period, ebb=ebb)
    one = s.wave(k, across, h)
    edges = [s.edge(px, k * period + 0.7 * i, across, h) for i in range(12) for px in (0, across // 3, across - 1)]
    w(f"    ({seed}, {period!r}, {ebb!r}, {k}, {across}, {h}, {floats(one.late)}, {floats(one.run)}, {floats([one.at, one.bend])}, {floats(edges)}),")
w("];")
w("")

w("/// `rocks_of(text, w, h)`: the digest of `x,y;`.")
w("pub const ROCKS: &[(&str, usize, usize, u32)] = &[")
for name, rw, rh in (("words", 80, 24), ("low", 40, 24), ("block", 200, 60)):
    rocks = shore.rocks_of(TEXTS[name], rw, rh)
    w(f'    ("{name}", {rw}, {rh}, {fnv("".join(f"{x},{y};" for x, y in rocks))}),')
w("];")
w("")

# Runs: seed, keywords, size, text, the first time, ticks of 1/8 s, the
# pointer (tick, x, y; x < 0: it leaves), pictures at ticks.
RUNS = [
    (1, {}, (80, 24), "none", 0.0, 0, [], [0]),
    (1, {}, (80, 24), "none", 3.0, 80, [], [0, 40, 80]),
    (2, {"grains": 0}, (60, 20), "low", 13.0, 8, [], [0, 8]),
    (3, {"drift": True, "ripples": True, "walker": True}, (80, 24), "words", 30.0, 400, [], [0, 130, 400]),
    (4, {"period": 2.0, "ebb": 60.0, "grains": 1000.0}, (40, 12), "low", 0.0, 64, [], [5, 64]),
    (5, {"day": 240.0, "drift": True}, (120, 40), "words", 100.0, 120, [], [0, 30, 60, 120]),
    (6, {"day": 60.0, "ripples": True}, (200, 60), "block", 7.5, 1, [], [0, 1]),
    (3, {"birds": True, "crabs": True}, (80, 24), "none", 14.0, 200, [(8, 40, 12), (20, 41, 12), (60, 12, 18), (64, 13, 18), (100, -1, 0)], [0, 8, 12, 20, 30, 60, 70, 90, 140, 200]),
    (7, {"birds": True, "crabs": True, "drift": True, "ripples": True, "walker": True, "day": 240.0}, (160, 40), "words", 90.0, 240, [(16, 30, 20), (17, 31, 21), (40, 100, 25), (120, -1, 0), (150, 5, 30)], [0, 16, 30, 50, 120, 180, 240]),
    (8, {"birds": True, "crabs": True}, (30, 10), "none", 5.0, 30, [(4, 15, 5)], [10, 30]),
    (1, {}, (1, 1), "none", 5.0, 1, [], [0, 1]),
    (2, {"birds": True, "crabs": True, "walker": True}, (3, 2), "none", 200.0, 4, [], [4]),
]
w("/// Runs: seed, keywords, `(w, h)`, text, the first second, ticks of 1/8 s,")
w("/// the pointer `(tick, x, y)` (`x < 0`: it leaves), pictures `(tick, digest)`.")
w("pub const RUNS: &[(u64, &str, (usize, usize), &str, f64, u32, &[(u32, i64, i64)], &[(u32, u32)])] = &[")
for seed, kw, (rw, rh), name, t0, ticks, touches, at in RUNS:
    s = shore.Shore(random.Random(seed), **kw)
    rocks = shore.rocks_of(TEXTS[name], rw, rh)
    now = [t0]
    pointer = Pointer(lambda: now[0])
    pics = []
    for i in range(ticks + 1):
        now[0] = t = t0 + i / 8
        for k, x, y in touches:
            if k == i:
                if x < 0:
                    pointer.leave()
                else:
                    pointer.move(x, y)
        grid = s.picture(rw, rh, t, rocks, pointer)
        if i in at:
            pics.append(f"({i}, {fnv(text(grid, rw))})")
    touch = ", ".join(f"({k}, {x}, {y})" for k, x, y in touches)
    w(f'    ({seed}, r#"{json.dumps(kw, separators=(",", ":"))}"#, ({rw}, {rh}), "{name}", {t0!r}, {ticks}, &[{touch}], &[{", ".join(pics)}]),')
w("];")

print("\n".join(out))
