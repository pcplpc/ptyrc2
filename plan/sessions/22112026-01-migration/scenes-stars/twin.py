# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's pictures as digests: the fixtures of stars/src/twin.rs.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/scenes-stars/twin.py
> ../../store-apps/stars/src/twin.rs"""

import argparse
import math
import random

from ptyrc_apps import stars


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


def touches(ts):
    return [stars.Touch(*t) for t in ts]


def rs_touches(ts):
    out = ", ".join(
        f"Touch::new({x}, {up}, {way}, {'f64::INFINITY' if until == math.inf else float(until)})"
        for x, up, way, until in ts
    )
    return f"&[{out}]"


out = []
w = out.append
w("// SPDX-License-Identifier: GPL-3.0-or-later")
w("//! The twin's pictures, as digests of `Grid::text`: written by")
w("//! `plan/sessions/22112026-01-migration/scenes-stars/twin.py`.")
w("")
w("#![allow(clippy::type_complexity)]")
w("")
w("use crate::scene::Touch;")
w("")

# Skies: seed, density, galaxy, figures; pictures (w, h, t, twinkle, turn).
SKIES = [
    (1, 70.0, False, False, [(40, 12, 0.0, False, 0.0), (80, 24, 0.0, False, 0.0), (80, 24, 3.3, True, 0.0), (80, 24, 0.0, False, 0.5)]),
    (3, 70.0, True, False, [(80, 24, 0.0, False, 0.0), (200, 60, 1.0, True, 0.3)]),
    (3, 70.0, False, True, [(120, 40, 0.0, False, 0.0), (80, 23, 7.9, True, 0.71)]),
    (9, 150.0, True, True, [(40, 12, 0.0, False, 0.0), (200, 60, 12.5, True, 0.99)]),
]
w("/// Skies: seed, density, galaxy, figures; pictures `(w, h, t, twinkle, turn, digest)`.")
w("pub const SKIES: &[(u64, f64, bool, bool, &[(usize, usize, f64, bool, f64, u32)])] = &[")
for seed, density, galaxy, figures, pics in SKIES:
    sky = stars.Sky(random.Random(seed), density, galaxy, figures)
    rows = []
    for pw, ph, t, tw, turn in pics:
        g = sky.picture(pw, ph, t, tw, turn)
        rows.append(f"({pw}, {ph}, {t!r}, {str(tw).lower()}, {turn!r}, {fnv(text(g, pw))})")
    w(f"    ({seed}, {density!r}, {str(galaxy).lower()}, {str(figures).lower()}, &[{', '.join(rows)}]),")
w("];")
w("")

# Grass: seed, wild; pictures (w, t, wind, scale, touches).
HELD = [(20, 0, 1, math.inf), (22, 1, -1, 3.5), (30, 2, 0, 3.9)]
GRASS = [
    (5, False, [(80, 0.0, 0.0, 1.0, []), (80, 1.5, 1.0, 1.0, []), (80, 2.0, -1.0, 1.4, HELD), (200, 9.25, 2.5, 3.0, [])]),
    (5, True, [(80, 0.0, 1.0, 1.0, []), (80, 4.0, 1.0, 1.5, HELD), (40, 0.5, 0.0, 2.0, [(10, 0, 1, math.inf)])]),
    (11, True, [(200, 17.0, 1.0, 1.0 + 9 / 35, HELD)]),
]
w("/// Grass: seed, wild; pictures `(w, t, wind, scale, touches, digest)`.")
w("pub const GRASS: &[(u64, bool, &[(usize, f64, f64, f64, &[Touch], u32)])] = &[")
for seed, wild, pics in GRASS:
    grass = stars.Grass(random.Random(seed), wild)
    rows = []
    for pw, t, wind, scale, ts in pics:
        g = grass.picture(pw, t, wind, scale, touches(ts))
        rows.append(f"({pw}, {t!r}, {wind!r}, {scale!r}, {rs_touches(ts)}, {fnv(text(g, pw))})")
    w(f"    ({seed}, {str(wild).lower()}, &[{', '.join(rows)}]),")
w("];")
w("")

# Moons over a row of stars: (w, h, cx, cy, r, phase).
MOONS = [(20, 6, 20.0, 12.0, 6.0, p) for p in (0.0, 0.1, 0.25, 0.4, 0.5, 0.75, 0.9)]
MOONS += [(20, 6, 0.0, 12.0, 6.0, 0.5), (20, 6, 14.0, 12.0, 6.0, 0.25), (80, 24, 33.3, 40.7, 5.0, 0.62)]
w("/// Moons over a field of stars: `(w, h, cx, cy, r, phase, digest)`.")
w("pub const MOONS: &[(usize, usize, f64, f64, f64, f64, u32)] = &[")
for mw, mh, cx, cy, r, phase in MOONS:
    grid = [{x: ("✦", 231) for x in range(0, mw, 4)} for _ in range(mh)]
    stars.moon(grid, cx, cy, r, phase)
    w(f"    ({mw}, {mh}, {cx!r}, {cy!r}, {r!r}, {phase!r}, {fnv(text(grid, mw))}),")
w("];")
w("")

w("/// `moon_at(80, 24, 5, f)`: `(f, x, y)`.")
w("pub const WAYS: &[(f64, f64, f64)] = &[")
for f in (0.0, 0.13, 0.5, 0.77, 1.0):
    x, y = stars.moon_at(80, 24, 5.0, f)
    w(f"    ({f!r}, {x!r}, {y!r}),")
w("];")
w("")


def args_of(**kw):
    a = argparse.Namespace(
        stars=70.0, night=300.0, moon=5.0, phase=None, wind=1.0, twinkle=False,
        turn=False, constellations=False, galaxy=False, meteors=False, bugs=False,
        wild=False, all=False, no_grass=False,
    )
    for k, v in kw.items():
        setattr(a, k, v)
    if a.all:
        a.twinkle = a.turn = a.constellations = a.galaxy = True
        a.meteors = a.bugs = a.wild = True
    if a.no_grass:
        a.bugs = a.wild = False
    return a


# Nights: seed, options as JSON, scenes (w, h, t) as `--print` composes.
NIGHTS = [
    (1, '{"phase":0.5}', dict(phase=0.5), [(40, 12, 0.0), (80, 24, 0.0)]),
    (1, '{"all":true,"phase":0.3}', dict(all=True, phase=0.3), [(80, 24, 0.0), (80, 24, 50.0), (200, 60, 120.0)]),
    (2, '{"all":true,"grass":false,"phase":0.7}', dict(all=True, no_grass=True, phase=0.7), [(40, 12, 0.0), (40, 12, 20.0)]),
    (2, '{"all":true,"phase":0.7}', dict(all=True, phase=0.7), [(40, 12, 0.0), (40, 50, 9.0), (40, 2, 1.0)]),
    (4, '{"night":20,"moon":8,"wind":-2,"phase":0.1}', dict(night=20.0, moon=8.0, wind=-2.0, phase=0.1), [(80, 24, 0.0), (80, 24, 3.0), (80, 24, 17.5)]),
]
w("/// Nights: seed, options, scenes `(w, h, t, digest)`; then the first")
w("/// draws of the app's own (`fx`): its first word.")
w("pub const NIGHTS: &[(u64, &str, &[(usize, usize, f64, u32)], u32)] = &[")
for seed, opts, kw, scenes in NIGHTS:
    rng = random.Random(seed)
    night = stars.Night(args_of(**kw), rng)
    fx = random.Random(rng.getrandbits(32))
    rows = []
    for sw, sh, t in scenes:
        grid = night.above(sw, sh, t)
        low = night.below(sw, sh, t)
        for row, grass in zip(grid[sh - len(low):], low, strict=True):
            row.update(grass)
        rows.append(f"({sw}, {sh}, {t!r}, {fnv(text(grid, sw))})")
    w(f"    ({seed}, r#\"{opts}\"#, &[{', '.join(rows)}], {fx.getrandbits(32)}),")
w("];")
w("")

w("/// `Grass.pushes(heights, t, touches)`: heights, t, touches, pushes.")
w("pub const PUSHES: &[(&[i64], f64, &[Touch], &[f64])] = &[")
heights = [0, 3, 5, 7, 11, 4, 2, 9, 12, 6, 1, 8, 0, 10, 5, 3]
for t, ts in ((0.0, [(3, 0, 0, math.inf)]), (5.0, [(4, 1, 1, 4.6)]), (7.3, [(2, 0, -1, 7.0), (5, 2, 1, math.inf)]), (1.0, [(3, 0, 1, math.inf)] * 9)):
    got = stars.Grass.pushes(heights, t, touches(ts))
    w(f"    (&{heights}, {t!r}, {rs_touches(ts)}, &[{', '.join(repr(p) for p in got)}]),")
w("];")
w("")

rng = random.Random(7)
w("/// `meteor(Random(7), 80, 24)` thrice: `(x, y, ms, ttl_ms, vx)`.")
w("pub const METEORS: &[(i64, i64, u64, u64, f64)] = &[")
for _ in range(3):
    m = stars.meteor(rng, 80, 24)
    w(f"    ({m['at'][0]}, {m['at'][1]}, {m['frames'][0]['ms']}, {int(m['ttl'] * 1000)}, {m['motion']['vx']!r}),")
w("];")
bug = stars.Bug(rng, 40, 20, 2)
cells = [bug.step(1 / 6, 40, 20, 2) for _ in range(200)]
dark = bug.frames()[0]["ms"]
w("/// Then `Bug(rng, 40, 20, 2)`: 200 steps of 1/6 s, the digest of the")
w("/// cells `x,y;`, and the dark of its frames.")
w(f"pub const BUG: (u32, i64) = ({fnv(''.join(f'{x},{y};' for x, y in cells))}, {dark});")

print("\n".join(out))
