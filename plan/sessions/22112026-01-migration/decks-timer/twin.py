# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's faces as fixtures of timer/twin.js: for each state, look,
size and hover, the FNV-1a hash of the face normalised (runs of one
style merged, colours as numbers); a few faces whole; the order of the
sand of each glass. Run: uv run python twin.py > twin.js (in
plugins/client-apps/python)."""
import json
from dataclasses import replace

from ptyrc_apps import timer

COLOURS = {"bright-red": 9, "bright-cyan": 14, "bright-green": 10,
           "bright-yellow": 11, "yellow": 3, "white": 7, "bright-black": 8}
LOOKS = [dict(glass=g, colour=c) for g in (False, True) for c in (False, True)]
STATES = [
    dict(length=1500.0, left=1500.0),
    dict(length=1500.0, left=612.4, beat=3.7),
    dict(length=1500.0, left=0.4, beat=8.2),
    dict(length=1500.0, left=700.0, paused=True, beat=1.6),
    dict(length=1500.0, left=0.0, done=True, over=75.0, beat=2.6),
    dict(name="break", length=300.0, left=290.0, index=3, round=2, rounds=4, turn=0.4),
    dict(name="work", length=20000.0, left=17000.0, index=2, round=2),
    dict(length=100.0, left=5.0, beat=12.25),
    dict(length=100.0, left=40.0, beat=0.55),
]


def norm(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            t = str(r["t"])
            if not t:
                continue
            fg = r.get("fg")
            style = [COLOURS.get(fg, fg), bool(r.get("bold")), bool(r.get("dim"))]
            if runs and runs[-1][1:] == style:
                runs[-1][0] += t
            else:
                runs.append([t, *style])
        out.append(runs)
    return json.dumps(out, ensure_ascii=False, separators=(",", ":"))


def fnv(text):
    h = 0x811C9DC5
    for c in text:
        h = ((h ^ ord(c)) * 0x01000193) & 0xFFFFFFFF
    return h


faces = []
whole = []
for si, s in enumerate(STATES):
    state = timer.State(**s)
    for li, kw in enumerate(LOOKS):
        look = timer.Look(**kw)
        for (w, h) in timer.ladder(look):
            for hover in (False, True):
                rows = timer.face(state, w, h, look, hover)
                faces.append([si, li, w, h, hover, fnv(norm(rows)), timer.title(state, (w, h), look)])
                if si in (1, 5) and li == 3 and not hover:
                    whole.append([si, li, w, h, json.loads(norm(rows))])
spins = []
for k in range(2, 7):
    for t in range(11):
        spins.append([k, t, fnv(norm(timer.spin(k, t / 10, timer.Look())))])
bulbs = [[k, fnv(json.dumps(timer.bulbs(k), separators=(",", ":")).replace("(", "[").replace(")", "]"))] for k in range(2, 7)]
print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's faces (plan/sessions/22112026-01-migration/decks-timer/twin.py):")
print("// [state, look, w, h, hover, hash, title], hashes of spin(k, t / 10), of bulbs(k).")
print("export const STATES = " + json.dumps(STATES) + ";")
print("export const LOOKS = " + json.dumps(LOOKS) + ";")
print("export const FACES = [")
for f in faces:
    print("  " + json.dumps(f) + ",")
print("];")
print("export const WHOLE = [")
for f in whole:
    print("  " + json.dumps(f, ensure_ascii=False) + ",")
print("];")
print("export const SPINS = " + json.dumps(spins) + ";")
print("export const BULBS = " + json.dumps(bulbs) + ";")
