# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's scenes as fixtures of window/twin.js: for each case (size,
naive time, t, look, pointer), the FNV-1a hash of the rows normalised
([t, fg, bg] a run); the rooms; a cat stepped; the draws of `-r` for a
few seeds. Run in plugins/client-apps/python:
uv run python PATH/twin.py > ../../store-apps/window/twin.js
With `dump I`: the rows of case I, as JSON."""
import argparse
import datetime as dt
import json
import sys
from dataclasses import asdict, replace

from ptyrc_apps import _mouser, _sky, window
from ptyrc_apps._strip import Pointer
from ptyrc_apps.window import Look


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


ALL = Look(seed=5, land=True, seasons=True, weather="day", passing=True, cosy=True)
LOOKS = [
    *[Look(seed=5, place=p) for p in window.PLACES],
    *[replace(ALL, place=p) for p in window.PLACES],
    replace(ALL, deep=True),
    *[replace(ALL, weather=k, seed=7) for k in _sky.WEATHERS],
    *[replace(ALL, place=p, seed=12, weather="clouds") for p in window.PLACES],
    Look(seed=9, land=True),
    Look(seed=5, land=True, passing=True, place="sea"),
    Look(seed=3, cosy=True),
    replace(ALL, lat=-35.0, seed=2),
]
TIMES = [dt.datetime(2026, 7, 20, 12, 30), dt.datetime(2026, 7, 20, 23, 30),
         dt.datetime(2026, 1, 15, 8, 10), dt.datetime(2026, 10, 3, 19, 5),
         dt.datetime(2026, 4, 2, 5, 55, 40)]
TS = [0.0, 3.4, 12.5, 41.0, 77.7, 130.2]
SIZES = [*window.SIZES, (3, 2), (19, 7), (61, 17)]
# Pointer events: [x, y, time], or [None, time] for a leave.
POINTERS = [
    [[30, 8, 1.0]],
    [[30, 8, 1.0], [31, 8, 1.5], [3, 10, 2.5], [None, 4.0]],
    [[5, 11, 0.5], [6, 11, 1.0], [7, 10, 1.6], [12, 9, 2.4], [20, 11, 3.5], [21, 11, 4.0]],
    [[2, 2, 0.2], [8, 3, 0.9], [None, 1.4]],
]
QUERY = [0.5, 1.2, 2.0, 3.1, 4.4, 6.0, 9.5, 30.0]


def pointer_of(events, t):
    now = [0.0]
    p = Pointer(lambda: now[0])
    for ev in events:
        if ev[-1] > t:
            break
        now[0] = ev[-1]
        if ev[0] is None:
            p.leave()
        else:
            p.move(ev[0], ev[1])
    return p


cases = []
for li, look in enumerate(LOOKS):
    for mi, t in enumerate(TS):
        for si, (w, h) in enumerate(SIZES):
            when = TIMES[(li + mi + si) % len(TIMES)]
            if (li + mi + si) % 2 and (w, h) != (40, 12):
                continue
            cases.append((w, h, when, t, li, None))
PLOOKS = [len(window.PLACES) + 0, len(window.PLACES) + 1, len(LOOKS) - 2]
for li in PLOOKS:
    for pi in range(len(POINTERS)):
        for qi, t in enumerate(QUERY):
            w, h = ((40, 12), (26, 8), (18, 6), (61, 17))[(pi + qi) % 4]
            cases.append((w, h, TIMES[(li + qi) % 2], t, li, pi))


def scene(w, h, when, t, li, pi):
    pointer = None if pi is None else pointer_of(POINTERS[pi], t)
    return window.scene(w, h, when, t, LOOKS[li], pointer)


if sys.argv[1:2] == ["dump"]:
    print(norm(scene(*cases[int(sys.argv[2])])))
    raise SystemExit

rooms = []
for li, look in enumerate(LOOKS):
    for w, h in SIZES:
        for season in _sky.SEASONS:
            for kind in (None, "clouds", "rain", "fog", "snow"):
                if (li + w + h + len(season) + len(kind or "")) % 24:
                    continue
                r = window._room(look, w, h, season, kind)
                rooms.append([li, w, h, season, kind, list(r.frame), list(r.cloth), list(r.coat),
                              r.perch and list(r.perch)])

# A cat stepped at 12 frames a second, the pointer moved every half second.
stepped = []
for li, (w, h) in ((len(LOOKS) - 2, (40, 12)), (len(window.PLACES) + 1, (26, 8))):
    look = LOOKS[li]
    room = window._room(look, w, h, "summer", None)
    home = room.perch[0] + 2
    sill = window._sill(w, h, look.seed, room.perch)
    far = sill.lo if home > w // 2 else sill.hi
    now = [0.0]
    pointer = Pointer(lambda: now[0])
    cat = _mouser.Cat()
    hashes = []
    for n in range(120):
        now[0] = t = n / 12
        if n % 6 == 0 and n < 90:
            pointer.move(far + n // 6 % 2, h - 5)
        if n == 90:
            pointer.leave()
        rows = window.scene(w, h, TIMES[0], t, look, pointer, cat)
        hashes.append([fnv(norm(rows)), cat.busy])
    stepped.append([li, w, h, far, hashes])

dt.datetime = Fixed
draws = []
for seed in (3, 11, 12, 40, 977):
    for place in (None, "sea"):
        args = argparse.Namespace(random=True, seed=seed, place=place, season=None,
                                  weather=None, at=None, lat=47.0, seasons=False)
        window.drawn(args, "place", window.PLACES)
        draws.append([seed, place, args.place, args.season, args.days, args.weather,
                      list(args.at)])

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's scenes (plan/sessions/22112026-01-migration/decks-window/twin.py).")
print("// CASES: [w, h, naive ms, t, look, pointer, hash]; POINTERS: [x, y, time],")
print("// [null, time] a leave; ROOMS: [look, w, h, season, kind, frame, cloth, coat,")
print("// perch]; STEPPED: [look, w, h, far, [hash, busy] a frame]; DRAWS: `-r` of a")
print("// seed, now 2026-03-07T09:41:12.")
looks = [asdict(look) for look in LOOKS]
print("export const LOOKS = " + json.dumps(looks) + ";")
print("export const POINTERS = " + json.dumps(POINTERS) + ";")
print("export const CASES = [")
for w, h, when, t, li, pi in cases:
    print("  " + json.dumps([w, h, ms(when), t, li, pi, fnv(norm(scene(w, h, when, t, li, pi)))]) + ",")
print("];")
print("export const ROOMS = [")
for r in rooms:
    print("  " + json.dumps(r) + ",")
print("];")
print("export const STEPPED = [")
for s in stepped:
    print("  " + json.dumps(s) + ",")
print("];")
print("export const DRAWS = [")
for d in draws:
    print("  " + json.dumps(d) + ",")
print("];")
