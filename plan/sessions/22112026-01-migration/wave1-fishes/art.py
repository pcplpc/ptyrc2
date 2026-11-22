# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's art as JSON, to compare with reef.js (art.mjs)."""
import json
from ptyrc_apps import _reef, fishes
def shape(s):
    return [s.name, [list(f) for f in s.frames], list(s.inks), getattr(s, "ms", None), getattr(s, "weight", None), getattr(s, "front", None)]
out = {
    "decor": [shape(s) for s in _reef.DECOR],
    "sunk": [[shape(s.fall), shape(s.rest), s.speed] for s in _reef.SUNK],
    "crossers": [[c.name, [list(f) for f in c.frames], list(c.inks), list(c.speed), c.ms, c.rises, c.scary] for c in _reef.CROSSERS],
    "species": [[k.name, list(k.right), list(k.colors) if k is not fishes.CLASSIC else None] for k in fishes.SPECIES],
    "mirror": list(_reef.mirror(["/\\()<>[]{}`'ab"])),
}
print(json.dumps(out, ensure_ascii=False, sort_keys=True))
