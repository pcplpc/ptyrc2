# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's scenes as text, a sum of colours and the bold cells: the
fixtures of globe/tests.js `TWIN`; the first draws of the generator:
`DRAWS`. Run: uv run python twin.py (in plugins/client-apps/python)."""
import json
import random

from ptyrc_apps import globe

ALL = {"shake": True, "drift": True, "lights": True, "random": True}
CASES = [
    ({}, 0.0, 11, 5, None, 0),
    ({}, 3.7, 17, 8, None, 0),
    ({}, 12.0, 21, 10, None, 0),
    ({}, 7.3, 30, 13, None, 0),
    ({}, 101.0, 30, 13, 100.0, 1),
    ({}, 100.4, 21, 10, 100.0, 1),
    ({"drift": True}, 5.0, 21, 10, None, 0),
    ({"drift": True}, 64.0, 30, 13, 50.0, 1),
    ({"lights": True}, 1.0, 21, 10, None, 0),
    ({"lights": True, "scene": "fir"}, 2.2, 30, 13, None, 0),
    ({"scene": "church"}, 9.0, 17, 8, None, 0),
    ({"scene": "snowman"}, 4.0, 11, 5, None, 0),
    (ALL, 9.0, 21, 10, None, 0),
    (ALL, 80.0, 30, 13, None, 0),
    (ALL, 200.5, 17, 8, 199.0, 2),
    ({**ALL, "every": 10.0, "seed": 1234567}, 31.0, 30, 13, None, 0),
]
out = []
for kw, t, w, h, shaken, shakes in CASES:
    rows = globe.scene(t, w, h, globe.Look(**kw), shaken, shakes)
    text = ["".join(str(r["t"]) for r in row) for row in rows]
    fg = sum(int(r.get("fg", 0)) * len(str(r["t"])) for row in rows for r in row)
    bold = sum(len(str(r["t"])) for row in rows for r in row if r.get("bold"))
    out.append([kw, t, w, h, shaken, shakes, fg, bold, text])
draws = []
for n in (0, 1, 7919, 7919 * 1000000 + 44, 2**40 + 5):
    rng = random.Random(n)
    draws.append([n, rng.random(), rng.uniform(4.0, 9.0) * rng.choice((-1, 1))])
print("export const TWIN = [")
for case in out:
    print("  " + json.dumps(case, ensure_ascii=False) + ",")
print("];\n\nexport const DRAWS = " + json.dumps(draws) + ";")
