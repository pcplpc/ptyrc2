# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's scenes as fixtures of train/twin.js: for each case
(size, naive time, t, trip, pointer), the FNV-1a hash of the rows
normalised ([t, fg, bg] a run); exact floats of `_sky.at`; the draws of
`-r` for a few seeds. Run in plugins/client-apps/python:
uv run python PATH/twin.py > ../../store-apps/train/twin.js
With `dump I`: the rows of case I, as JSON."""
import datetime as dt
import json
import sys
from dataclasses import replace

from ptyrc_apps import _sky, train, window
from ptyrc_apps._strip import Pointer
from ptyrc_apps.train import CYCLE, Trip

NOW = dt.datetime(2026, 7, 20, 12, 30)


class Fixed(dt.datetime):
    @classmethod
    def now(cls, tz=None):
        return cls(2026, 3, 7, 9, 41, 12)


def norm(rows):
    return json.dumps([[[r["t"], r.get("fg"), r["bg"]] for r in row] for row in rows],
                      ensure_ascii=False, separators=(",", ":"))


def fnv(text):
    h = 0x811C9DC5
    for c in text:
        h = ((h ^ ord(c)) * 0x01000193) & 0xFFFFFFFF
    return h


def ms(when):
    return int((when - dt.datetime(1970, 1, 1)).total_seconds() * 1000)


ALL = Trip(seed=4, land=True, poles=True, events=True, seasons=True, weather="day")
TRIPS = [
    Trip(seed=4),
    ALL,
    replace(ALL, deep=True),
    *[replace(ALL, line=line, seed=11) for line in train.LINES[1:]],
    *[replace(ALL, weather=k, seed=7) for k in _sky.WEATHERS],
    Trip(seed=5, poles=True, weather="clouds"),
    Trip(seed=9, land=True, line="city", seasons=True),
]
TIMES = [dt.datetime(2026, 7, 20, 12, 30), dt.datetime(2026, 7, 20, 23, 30),
         dt.datetime(2026, 1, 15, 8, 10), dt.datetime(2026, 10, 3, 19, 5),
         dt.datetime(2026, 4, 2, 5, 55)]
SIZES = [*train.SIZES, (5, 2), (70, 16)]


def moments(trip):
    out = [0.0, 5.3, 41.3]
    for what in train.EVENTS:
        k = next((k for k in range(200) if train.event(trip, k) == what), None)
        if k is None:
            continue
        start = k * CYCLE + train.WHEN
        out += [start - 1, start + 1.5, start + 4.2, start + 9.5, start + 19.7]
    return out


cases = []
for ti, trip in enumerate(TRIPS):
    for mi, t in enumerate(moments(trip)):
        for si, (w, h) in enumerate(SIZES):
            when = TIMES[(ti + mi + si) % len(TIMES)]
            if (ti + mi + si) % 3 and (w, h) != (48, 11):
                continue
            cases.append((w, h, when, t, ti, None))
for when in TIMES:
    cases.append((48, 11, when, 2.0, 0, [(20, 5, 1.0), (21, 5, 1.6)]))


def scene(w, h, when, t, ti, trail):
    pointer = None
    if trail is not None:
        pointer = Pointer(lambda: t, train.KEEP)
        pointer.trail = list(trail)
    return train.scene(w, h, when, t, TRIPS[ti], pointer)


if sys.argv[1:2] == ["dump"]:
    print(norm(scene(*cases[int(sys.argv[2])])))
    raise SystemExit

skies = []
for when in [*TIMES, dt.datetime(2026, 6, 21, 4, 17), dt.datetime(2026, 12, 21, 16, 2)]:
    for lat, cover in ((47.0, 0.0), (-35.0, 0.6), (89.0, 1.0)):
        s = _sky.at(when, lat, cover)
        skies.append([ms(when), lat, cover, s.hour, s.day, s.sun, s.sun_x, s.moon, s.moon_x,
                      s.phase, s.light, s.stars, list(s.ambient), list(s.colour(0.3, 0.7)),
                      list(s.shade((90, 160, 40), 0.5)), s.season,
                      _sky.weather(when.toordinal(), 7, s.season)])

dt.datetime = Fixed
draws = []
for seed in (3, 11, 12, 40, 977):
    for line in (None, "alps"):
        a = dt.datetime.now()
        import argparse
        args = argparse.Namespace(random=True, seed=seed, line=line, season=None,
                                  weather=None, at=None, lat=47.0, seasons=False)
        window.drawn(args, "line", train.LINES)
        import random
        rng = random.Random(args.seed)
        after = rng.randrange(40) * CYCLE + rng.uniform(0.0, train.WHEN - 6.0)
        draws.append([seed, line, args.line, args.season, args.days, args.weather,
                      list(args.at), after])

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's scenes (plan/sessions/22112026-01-migration/decks-train/twin.py).")
print("// CASES: [w, h, naive ms, t, trip, trail, hash]; SKIES: exact floats of")
print("// `_sky.at`; DRAWS: `-r` of a seed, now 2026-03-07T09:41:12.")
trips = [dict(seed=p.seed, line=p.line, lat=p.lat, land=p.land, poles=p.poles,
              events=p.events, seasons=p.seasons, weather=p.weather, deep=p.deep) for p in TRIPS]
print("export const TRIPS = " + json.dumps(trips) + ";")
print("export const CASES = [")
for w, h, when, t, ti, trail in cases:
    rows = scene(w, h, when, t, ti, trail)
    print("  " + json.dumps([w, h, ms(when), t, ti, trail and [list(p) for p in trail], fnv(norm(rows))]) + ",")
print("];")
print("export const SKIES = [")
for s in skies:
    print("  " + json.dumps(s) + ",")
print("];")
print("export const DRAWS = [")
for d in draws:
    print("  " + json.dumps(d) + ",")
print("];")
