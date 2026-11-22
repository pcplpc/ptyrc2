# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's pond as digests: the fixtures of pond/src/twin.rs.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/scenes-pond/twin.py
> ../../store-apps/pond/src/twin.rs"""

import json
import random

from ptyrc_apps import pond


def fnv(text):
    h = 2166136261
    for b in text.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


def text(grid, w):
    return "\n".join(
        "".join(f"{x} {g} {ink}," for x, (g, ink) in sorted(row.items()) if 0 <= x < w)
        for row in grid
    )


def f6(x):
    return f"{x:.6f}"


def ripples(rs):
    return ";".join(
        f"{r.key},{f6(r.x)},{f6(r.y)},{f6(r.t0)},{f6(r.life)},{r.rings},{f6(r.reach)}"
        for r in rs
    )


WORDS = ["", "", "words in the way", "", "   more words here"]
LINE = [""] * 9 + ["x" * 80]
BLOCK = [""] * 8 + [" " * 20 + "x" * 30] * 3
TEXTS = {"none": [], "words": WORDS, "line": LINE, "block": BLOCK}

out = []
w = out.append
w("// SPDX-License-Identifier: GPL-3.0-or-later")
w("//! The twin's pond, as digests: written by")
w("//! `plan/sessions/22112026-01-migration/scenes-pond/twin.py`.")
w("")
w("#![allow(clippy::type_complexity)]")
w("")

w("/// `drops(seed, t, w, h, rate)`: the digest of the ripples `key,x,y,t0,life,rings,reach;`.")
w("pub const DROPS: &[(i64, f64, usize, usize, f64, u32)] = &[")
for seed, t, dw, dh, rate in ((3, 50.0, 80, 24, 240.0), (4, 50.0, 80, 24, 240.0), (3, 7.25, 160, 48, 24.0), (977, 1234.5, 200, 60, 10.0)):
    w(f"    ({seed}, {t!r}, {dw}, {dh}, {rate!r}, {fnv(ripples(pond.drops(seed, t, dw, dh, rate)))}),")
w("];")
w("")

w("/// Showers: `drops(seed + 1, t, 80, 24, shower, *SHOWERS)` of the pond of `Random(seed)`.")
w("pub const SHOWERS: &[(u64, f64, u32)] = &[")
for seed, t in ((1, 9.0), (1, 31.0), (6, 77.7)):
    p = pond.Pond(random.Random(seed), rain=True)
    rs = pond.drops(p.seed + 1, t, 80, 24, p.shower, *pond.SHOWERS)
    w(f"    ({seed}, {t!r}, {fnv(ripples(rs))}),")
w("];")
w("")

w("/// `shadow(x, y, reach, index(text, 40, 12))`: the digest of `near;far` at 6 decimals, 0 for None.")
w("pub const SHADOWS: &[(&str, f64, f64, f64, u32)] = &[")
for name, x, y, reach in (("words", 36.0, 26.0, 40.0), ("words", 13.0, 26.0, 30.0), ("words", 40.0, 2.0, 30.0), ("line", 36.0, 26.0, 40.0), ("block", 30.0, 20.0, 30.0), ("block", 60.0, 50.0, 12.0)):
    walls = pond.index(TEXTS[name], 40, 12)
    s = pond.shadow(x, y, reach, walls)
    d = 0 if s is None else fnv(";".join(",".join(f6(v) if v != float("inf") else "inf" for v in part) for part in s))
    w(f"    ({name!r}, {x!r}, {y!r}, {reach!r}, {d}),".replace("'", '"'))
w("];")
w("")

# Runs: seed, the twin's keywords, size, text, ticks of 1/20 s, the
# pointer (tick, dot x, dot y), pictures at ticks.
RUNS = [
    (1, {}, (40, 12), "none", 0, [], [0]),
    (1, {}, (80, 24), "none", 160, [], [0, 40, 160]),
    (2, {"pads": 30, "drops": 200}, (80, 24), "words", 60, [], [20, 60]),
    (3, {"koi": True, "pads": 0, "drops": 0}, (80, 24), "block", 400, [], [100, 400]),
    (4, {"koi": True, "lotus": True, "rain": True, "moon": True, "adrift": True, "striders": True, "phase": 0.5, "season": "spring"}, (80, 24), "words", 200, [(50, 81, 50), (60, 90, 52)], [0, 55, 120, 200]),
    (5, {"koi": True, "striders": True, "pads": 0, "drops": 0, "season": "autumn", "adrift": True}, (120, 40), "none", 120, [(30, 100, 80)], [31, 90]),
    (6, {"moon": True, "phase": 0.3, "night": 20.0, "current": 2.0}, (80, 24), "none", 100, [], [0, 50, 100]),
    (7, {"lotus": True, "pads": 30, "moon": True, "phase": 0.8, "season": "winter", "adrift": True}, (200, 60), "words", 20, [], [20]),
    (8, {"koi": True, "rain": True}, (40, 12), "line", 300, [(100, 40, 20)], [150, 300]),
]
w("/// Runs: seed, keywords, `(w, h)`, text, ticks of 1/20 s, the pointer")
w("/// `(tick, x, y)`, pictures `(tick, digest)`; then the koi's last kisses' digest.")
w("pub const RUNS: &[(u64, &str, (usize, usize), &str, u32, &[(u32, f64, f64)], &[(u32, u32)], u32)] = &[")
for seed, kw, (rw, rh), name, ticks, touches, at in RUNS:
    p = pond.Pond(random.Random(seed), **kw)
    walls = pond.index(TEXTS[name], rw, rh)
    pics = []
    for i in range(ticks + 1):
        t = i / 20
        for k, x, y in touches:
            if k == i:
                p.touch(x, y, t)
        p.step(1 / 20, rw, rh, t, walls)
        if i in at:
            pics.append(f"({i}, {fnv(text(p.picture(rw, rh, t, walls), rw))})")
    touch = ", ".join(f"({k}, {float(x)!r}, {float(y)!r})" for k, x, y in touches)
    w(f"    ({seed}, r#\"{json.dumps(kw, separators=(',', ':'))}\"#, ({rw}, {rh}), {name!r}, {ticks}, &[{touch}], &[{', '.join(pics)}], {fnv(ripples(p.kisses))}),".replace("'", '"'))
w("];")
w("")

rng = random.Random(7)
fly = pond.Fly(rng, 80, 24)
cells = [fly.step(1 / 8, 80, 24) for _ in range(800)]
cells += [fly.step(1 / 20, 80, 24, (40.0, 12.0, 1.0)) for _ in range(60)]
w("/// `Fly(Random(7), 80, 24)`: 800 steps of 1/8 s, then 60 of 1/20 s by the")
w("/// pointer at (40, 12): the digest of the cells `x,y;`; its inks.")
w(f"pub const FLY: (u32, (u8, u8)) = ({fnv(''.join(f'{x},{y};' for x, y in cells))}, {fly.inks});")

print("\n".join(out))
