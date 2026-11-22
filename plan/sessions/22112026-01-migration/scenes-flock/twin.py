# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's flock as digests: the fixtures of flock/src/twin.rs.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/scenes-flock/twin.py
> ../../store-apps/flock/src/twin.rs"""

import argparse
import json
import random
import struct

from ptyrc_apps import flock


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
LOW = [""] * 20 + ["   a line of text low on the pane"]
BLOCK = [""] * 30 + [" " * 40 + "x" * 60] * 6
TEXTS = {"none": [], "words": WORDS, "low": LOW, "block": BLOCK}

out = []
w = out.append
w("// SPDX-License-Identifier: GPL-3.0-or-later")
w("//! The twin's flock, as digests: written by")
w("//! `plan/sessions/22112026-01-migration/scenes-flock/twin.py`.")
w("")
w("#![allow(clippy::type_complexity)]")
w("")

fs = [i / 40 for i in range(-8, 50)]
w("/// `height`, `glow`, of `i / 40` for `i` in -8..50; `stage(t, 300, 0.3)`")
w("/// of `7.5 i`; `sun_at(80, 90, 7, f)` then `sun_at(200, 234, 17.5, f)`.")
w(f"pub const HEIGHT: u32 = {floats([flock.height(f) for f in fs])};")
w(f"pub const GLOW: u32 = {floats([flock.glow(f) for f in fs])};")
w(f"pub const STAGE: u32 = {floats([flock.stage(7.5 * i, 300.0, 0.3) for i in range(-8, 50)])};")
w(f"pub const SUN_AT: u32 = {floats([v for f in fs for v in flock.sun_at(80, 90.0, 7.0, f) + flock.sun_at(200, 234.0, 17.5, f)])};")
w("")

DUSKS = [
    (1, False, [(80, 23, 0.7, 0.0), (80, 23, 0.5, 3.0), (40, 11, 0.75, 0.0)]),
    (1, True, [(80, 23, 0.7, 0.0), (80, 23, 0.7, 60.0)]),
    (2, True, [(200, 58, 0.5, 10.0), (200, 58, 0.82, 300.0)]),
    (3, False, [(40, 11, 0.95, 0.0), (80, 23, 0.0, 0.0)]),
    (4, True, [(80, 20, 0.3, 5.0), (3, 1, 0.75, 1.0)]),
]
w("/// `Dusk(Random(seed), clouds).picture(w, h, f, sun_at(w, 4 h, 7, f), t)`,")
w("/// one after the other: `(w, h, f, t, digest)`.")
w("pub const DUSKS: &[(u64, bool, &[(usize, usize, f64, f64, u32)])] = &[")
for seed, clouds, pics in DUSKS:
    d = flock.Dusk(random.Random(seed), clouds)
    got = []
    for pw, ph, f, t in pics:
        sun = flock.sun_at(pw, 4.0 * ph, 7.0, f)
        got.append(f"({pw}, {ph}, {f!r}, {t!r}, {fnv(text(d.picture(pw, ph, f, sun, t), pw))})")
    w(f"    ({seed}, {str(clouds).lower()}, &[{', '.join(got)}]),")
w("];")
w("")

LANDS = [
    (5, False, False, 1.0, [(80, 24, 0.0, None, 0.3), (80, 24, 9.0, (80.0, 7.0, 208), 0.7), (200, 60, 0.0, None, 0.3)]),
    (5, False, True, 1.0, [(80, 24, 0.0, None, 0.3), (80, 24, 0.4, None, 0.3), (80, 24, 3.3, (80.0, 7.0, 208), 0.7), (120, 30, 7.0, None, 0.9)]),
    (6, False, True, 0.0, [(80, 24, 1.0, None, 0.3), (80, 24, 2.0, None, 0.3)]),
    (7, False, True, -2.5, [(60, 20, 1.0, None, 0.3)]),
    (5, True, False, 1.0, [(80, 24, 0.0, None, 0.7), (80, 24, 0.0, (80.0, 7.0, 208), 0.7), (80, 24, 0.4, (80.0, 7.0, 208), 0.7), (80, 24, 0.0, (80.0, 7.0, 196), 0.99)]),
    (8, True, True, 1.0, [(200, 60, 2.5, (210.0, 17.5, 214), 0.75), (3, 2, 0.0, (3.0, 7.0, 208), 0.7)]),
]
w("/// `Land(Random(seed), water, reeds)`, `wind`; `picture(w, h, t, wind, sun, f)`")
w("/// one after the other, `sun` `(x, r, colour)`, `r` 0: none.")
w("pub const LANDS: &[(u64, bool, bool, f64, &[(usize, usize, f64, (f64, f64, u8), f64, u32)])] = &[")
for seed, water, reeds, wind, pics in LANDS:
    land = flock.Land(random.Random(seed), water, reeds)
    got = []
    for pw, ph, t, sun, f in pics:
        grid = land.picture(pw, ph, t, wind, sun, f)
        s = sun or (0.0, 0.0, 0)
        got.append(f"({pw}, {ph}, {t!r}, ({s[0]!r}, {s[1]!r}, {s[2]}), {f!r}, {fnv(text(grid, pw))})")
    w(f"    ({seed}, {str(water).lower()}, {str(reeds).lower()}, {wind!r}, &[{', '.join(got)}]),")
w("];")
w("")


def sky(pw, over, at, dusk):
    return flock.Air(pw, over, lambda t: flock.stage(t, dusk, at), dusk, lambda px: 4 * over)


# Flocks: seed, keywords, segments (w, over, at, dusk, t0, ticks, dt, scare).
FLOCKS = [
    (3, {}, [(40, 12, 0.4, 3000.0, 5.0, 3, 0.1, None), (80, 23, 0.4, 3000.0, 5.3, 40, 0.1, None)]),
    (3, {}, [(80, 23, 0.4, 3000.0, 10.0, 60, 1 / 30, None), (80, 23, 0.4, 3000.0, 30.0, 20, 0.1, (80.0, 40.0, 1.0)), (100, 58, 0.4, 3000.0, 33.0, 10, 0.1, (80.0, 40.0, 0.35))]),
    (4, {"second": 0.35, "join": True, "hawk": True}, [(80, 23, 0.0, 100.0, 0.0, 120, 0.25, None)]),
    (5, {"hawk": True}, [(80, 23, 0.4, 3000.0, 0.0, 300, 0.5, None)]),
    (6, {"birds": 250.0}, [(200, 58, 0.83, 300.0, 0.0, 100, 0.2, None)]),
    (7, {}, [(80, 23, 0.3, 40.0, 0.0, 200, 0.3, None), (80, 23, 0.3, 40.0, 3.0, 2, 0.1, None)]),
]
w("/// `Flock(Random(seed), **kw)`; segments `(w, over, at, dusk, t0, ticks, dt,")
w("/// scare)` flown one after the other over flat land, `scare.2 < 0`: none;")
w("/// each the digest of the birds then the hawk at each of its `ticks + 1`.")
w("pub const FLOCKS: &[(u64, &str, &[((usize, usize, f64, f64, f64, u32, f64, (f64, f64, f64)), u32)])] = &[")
for seed, kw, segs in FLOCKS:
    f = flock.Flock(random.Random(seed), **kw)
    got = []
    for pw, over, at, dusk, t0, ticks, dt, scare in segs:
        air = sky(pw, over, at, dusk)
        xs = []
        for i in range(ticks + 1):
            for x, y in f.fly(air, t0 + i * dt, scare):
                xs += [x, y]
            hawk = f.hunter()
            xs += [-1.0] if hawk is None else list(hawk)
        s = scare or (0.0, 0.0, -1.0)
        got.append(f"(({pw}, {over}, {at!r}, {dusk!r}, {t0!r}, {ticks}, {dt!r}, ({s[0]!r}, {s[1]!r}, {s[2]!r})), {floats(xs)})")
    w(f'    ({seed}, r#"{json.dumps(kw, separators=(",", ":"))}"#, &[{", ".join(got)}]),')
w("];")
w("")

w("/// `perches(text, w, h)`: the digest of `x,y;`.")
w("pub const PERCHES: &[(&str, usize, usize, u32)] = &[")
for name, pw, ph in (("words", 80, 24), ("low", 40, 24), ("block", 200, 60), ("words", 10, 3)):
    spots = flock.perches(TEXTS[name], pw, ph)
    w(f'    ("{name}", {pw}, {ph}, {fnv("".join(f"{x},{y};" for x, y in spots))}),')
w("];")
w("")

DEFAULTS = dict(birds=100.0, dusk=300.0, at=0.3, sun=7.0, wind=1.0, clouds=False, water=False, reeds=False, join=False, double=False, hawk=False, night=False, roost=False)
ALL = dict(clouds=True, water=True, reeds=True, join=True, double=True, hawk=True, night=True, roost=True)
# Scenes: seed, options, size, text, the first second, ticks of 1/30 s,
# the pointer (tick, x, y; x < 0: it leaves), pictures at ticks.
SCENES = [
    (1, {}, (80, 24), "none", 0.0, 0, [], [0]),
    (1, {}, (40, 12), "none", 0.0, 0, [], [0]),
    (2, {"at": 0.5, "dusk": 100.0}, (80, 24), "words", 0.0, 90, [], [0, 1, 45, 90]),
    (1, ALL | {"at": 0.7}, (80, 24), "words", 0.0, 60, [], [0, 30, 60]),
    (4, ALL | {"dusk": 40.0, "at": 0.9}, (80, 24), "words", 0.0, 120, [], [0, 60, 120]),
    (3, {"hawk": True, "dusk": 3000.0, "at": 0.4}, (80, 24), "none", 20.0, 150, [(30, 40, 10), (31, 41, 10), (100, -1, 0)], [0, 30, 40, 70, 100, 150]),
    (5, {"night": True, "clouds": True, "at": 0.85, "sun": 0.0}, (120, 40), "low", 2.0, 30, [], [0, 30]),
    (6, {"water": True, "reeds": True, "wind": -1.0, "birds": 300.0}, (200, 60), "block", 1.0, 15, [(5, 100, 20)], [0, 15]),
    (7, {"roost": True, "at": 0.98, "dusk": 50.0}, (80, 24), "words", 0.0, 30, [], [0, 30]),
    (2, ALL, (3, 2), "none", 0.0, 3, [], [0, 3]),
]
w("/// Scenes: seed, options, `(w, h)`, text, the first second, ticks of 1/30")
w("/// s, the pointer `(tick, x, y)` (`x < 0`: it leaves), pictures `(tick,")
w("/// above, fore, below)`; `fore` at each tick, the roosts with `roost`.")
w("pub const SCENES: &[(u64, &str, (usize, usize), &str, f64, u32, &[(u32, i64, i64)], &[(u32, u32, u32, u32)])] = &[")
for seed, kw, (pw, ph), name, t0, ticks, touches, at in SCENES:
    args = argparse.Namespace(**(DEFAULTS | kw))
    args.at %= 1.0 + flock.REST
    scene = flock.Scene(args, random.Random(seed))
    roosts = []
    if args.roost:
        roosts = flock.perches(TEXTS[name], pw, ph)
        random.Random(seed).shuffle(roosts)
    pointer = None
    pics = []
    for i in range(ticks + 1):
        t = t0 + i / 30
        for k, x, y in touches:
            if k == i:
                pointer = None if x < 0 else [2.0 * x + 1.0, 4.0 * y + 2.0, t]
        scare = None
        if pointer is not None:
            fright = max(flock.STILL, 1.0 - (t - pointer[2]) / flock.CALM)
            scare = (pointer[0], pointer[1], fright)
        fore = scene.fore(pw, ph, t, roosts, scare)
        if i in at:
            above = scene.above(pw, ph, t)
            below = scene.below(pw, ph, t)
            pics.append(f"({i}, {fnv(text(above, pw))}, {fnv(text(fore, pw))}, {fnv(text(below, pw))})")
    touch = ", ".join(f"({k}, {x}, {y})" for k, x, y in touches)
    w(f'    ({seed}, r#"{json.dumps(kw, separators=(",", ":"))}"#, ({pw}, {ph}), "{name}", {t0!r}, {ticks}, &[{touch}], &[{", ".join(pics)}]),')
w("];")

print("\n".join(out))
