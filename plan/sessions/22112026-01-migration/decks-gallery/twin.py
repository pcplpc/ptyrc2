# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of gallery/twin.js: each picture shipped
(its header, its sizes; for each size and frame, the FNV-1a hash of the
rows of runs as JSON, for each look of LOOKS), the ship list's order,
and the wall of a seed. Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/decks-gallery/twin.py > ../../store-apps/gallery/twin.js"""
import json

from ptyrc_apps import gallery


def fnv(text):
    """Over the UTF-16 code units, as JS strings are."""
    h = 0x811C9DC5
    data = text.encode("utf-16-le")
    for i in range(0, len(data), 2):
        h = ((h ^ (data[i] | data[i + 1] << 8)) * 0x01000193) & 0xFFFFFFFF
    return h


def dump(rows):
    return json.dumps(rows, ensure_ascii=False, separators=(",", ":"))


def looks(w, h):
    return [None, (0, 0), (w - 1, h - 1), (w // 2, 0), (0, h // 2), (w // 2, h - 1)]


pictures = gallery.collection(candidates=True)
out = []
for p in pictures:
    hold = p.hold or (1,) * p.frames
    sizes = []
    for (w, h) in p.sizes:
        frames = []
        for nth in range(p.frames):
            t = (sum(hold[:nth]) + 0.5) * p.period
            frames.append([t, p.frame_at(t), [fnv(dump(p.render(w, h, t, look))) for look in looks(w, h)]])
        sizes.append([w, h, frames])
    out.append({"stem": p.stem, "name": p.name, "author": p.author, "kind": p.kind, "period": p.period,
                "hold": list(p.hold), "eyes": p.eyes, "sizes": sizes})
first = pictures[0]
w, h = next(iter(first.sizes))
whole = {"stem": first.stem, "size": [w, h], "rows": first.render(w, h)}

ship = [p.stem for p in gallery.collection()]
animals = [p.stem for p in gallery.collection(["animal"])]
hall = gallery.Hall(pictures[:6], count=3, every=60, seed=1)
wall = {"stems": [p.stem for p in pictures[:6]], "first": [s.picture.stem for s in hall.slots], "steps": []}
for t in range(20, 400, 20):
    wall["steps"].append([t, [[hall.slots.index(s), s.picture.stem] for s in hall.at(t)]])
one = gallery.Hall(gallery.collection(), seed=7)
order = [one.slots[0].picture.stem]
for t in range(600, 600 * 60, 600):
    order += [s.picture.stem for s in one.at(t)]

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's looks (plan/sessions/22112026-01-migration/decks-gallery/twin.py):")
print("// a picture: its sizes, a size its frames `[t, frame, hashes]`, a hash a look of `looks`.")
print("export const PICTURES = [")
for item in out:
    print(" ", json.dumps(item, ensure_ascii=False, separators=(",", ":")) + ",")
print("];")
print(f"export const WHOLE = {json.dumps(whole, ensure_ascii=False, separators=(',', ':'))};")
print(f"export const SHIP = {json.dumps(ship)};")
print(f"export const ANIMALS = {json.dumps(animals)};")
print(f"export const WALL = {json.dumps(wall)};")
print(f"export const ORDER = {json.dumps(order)};")
