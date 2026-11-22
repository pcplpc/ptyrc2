# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of pace/twin.js: the views of the tests
(`sample` drawn here), the FNV-1a hash of each face normalised (runs of
one style merged, colours as numbers) for each look, size and hover; a
few faces whole; the verdicts of `wordy` on the noise of
`test_wordy_on_noise` and on the words of `test_pace.py`. Run: uv run
python twin.py > twin.js (in plugins/client-apps/python)."""
import base64
import hashlib
import json
import random
from pathlib import Path

from ptyrc_apps import pace

COLOURS = {"bright-yellow": 11, "bright-cyan": 14}
LOOKS = [
    dict(graph=g, best=b, words=w or r, bars=r, colour=c, top=top)
    for g in (False, True)
    for b in (False, True)
    for w, r in ((False, False), (True, False), (True, True))
    for c in (False, True)
    for top in (10, 4)
]
TOP = (("hourglass", 40, 3.0), ("widget", 20, 200.0), ("sand", 5, 90000.0))
VIEWS = [
    pace.View(),
    pace.sample(1, 10),
    pace.sample(2, 4),
    pace.View(99999, 99999, (5,) * 180, (("a" * 24, 12345, 0.0), ("bb", 1, 9e6))),
    pace.View(312, 345, (0,) * 170 + (10,) * 10, TOP),
    pace.sample(3, 10),
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


def view(v):
    return [v.rate, v.best, list(v.slots), [list(t) for t in v.top]]


faces, whole = [], []
for vi, v in enumerate(VIEWS):
    for li, kw in enumerate(LOOKS):
        look = pace.Look(**kw)
        for w, h in pace.ladder(look):
            hovers = [None, (0, 0), (w - 1, h - 1)] if look.words else [None]
            for hi, hover in enumerate(hovers):
                rows = pace.face(v, w, h, look, hover)
                faces.append([vi, li, w, h, hi, fnv(norm(rows))])
                if vi in (1, 4) and li in (23, 47) and hi == 0:
                    whole.append([vi, li, w, h, json.loads(norm(rows))])

# test_wordy_on_noise: the tokens, and the twin's verdict on each piece.
rng = random.Random(1)
tokens = []
for _ in range(300):
    raw = rng.randbytes(24)
    tokens += [
        base64.b64encode(raw).decode(),
        hashlib.sha256(raw).hexdigest(),
        base64.b32encode(raw).decode().lower(),
    ]
verdicts = ""
for token in tokens:
    piece = ""
    for char in token + " ":
        if char.isalpha():
            piece += char
            continue
        if len(piece) >= 4:
            verdicts += "1" if pace.wordy(piece, token) else "0"
        piece = ""
tests = Path(__file__).resolve().parents[4] / "plugins/client-apps/python/tests/test_pace.py"
text = tests.read_text(encoding="utf-8").split('"""')[-1]
words = sorted({w for w in "".join(c if c.isalpha() else " " for c in text).split()})
words = [w for w in words if len(w) >= 4]

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's looks (plan/sessions/22112026-01-migration/keys-pace/twin.py):")
print("// VIEWS [rate, best, slots, top]; FACES [view, look, w, h, hover, hash],")
print("// hover 0 none, 1 the cell (0, 0), 2 the last; NOISE: wordy on its pieces.")
print("export const LOOKS = " + json.dumps(LOOKS) + ";")
print("export const VIEWS = " + json.dumps([view(v) for v in VIEWS]) + ";")
print("export const FACES = [")
for f in faces:
    print("  " + json.dumps(f) + ",")
print("];")
print("export const WHOLE = [")
for f in whole:
    print("  " + json.dumps(f, ensure_ascii=False) + ",")
print("];")
print("export const NOISE = " + json.dumps(tokens) + ";")
print(f'export const NOISE_KEPT = "{verdicts}";')
print("export const TEXT = " + json.dumps(words, ensure_ascii=False) + ";")
print(f'export const TEXT_KEPT = "{"".join("1" if pace.wordy(w) else "0" for w in words)}";')
