# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's `cat.Home` driven by a fake pilot, as the fixture
cat/src/world.txt: each scene, its screens and events, then a digest a
second of the calls made and of the state after each step. Run in
plugins/client-apps/python:
uv run python PATH/twin.py > ../../store-apps/cat/src/world.txt
With `show SCENE SECOND`: the lines of that second."""
import contextlib
import random
import struct
import sys

from ptyrc_apps import cat


def cells_text(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            s = r["t"]
            if "fg" in r:
                s += f":{r['fg']}"
            if r.get("bold"):
                s += "!b"
            if r.get("dim"):
                s += "!d"
            runs.append(s)
        out.append("|".join(runs))
    return "/".join(out)


def text(look):
    if "cells" in look:
        return "cells " + cells_text(look["cells"])
    frames = ";".join(f"{f['ms']}:{cells_text(f['cells'])}" for f in look["frames"])
    return f"frames loop={int(look['loop'])} {frames}"


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.encode():
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def bits(f):
    return struct.pack(">d", f).hex()


class Sprite:
    def __init__(self, pilot, n):
        self.pilot, self.id = pilot, n

    def update(self, anchor):
        self.pilot.log.append(f"at {self.id} {anchor['at'][0]} {anchor['at'][1]}")

    def set(self, **content):
        self.pilot.log.append(f"set {self.id} {fnv(text(content))}")

    def remove(self):
        self.pilot.log.append(f"rm {self.id}")

    def info(self):
        self.pilot.log.append(f"info {self.id}")
        return {"visible": True, "covered": self.pilot.cover}


class Pilot:
    def __init__(self):
        self.log, self.n, self.cover = [], 0, 0.0

    def draw(self, frame, at, space, layer, z, **content):
        self.n += 1
        if space == "view":
            self.log.append(f"mat {self.n} {len(content['cells'][0][0]['t'])} {len(content['cells'])}")
        else:
            self.log.append(f"draw {self.n} {at[0]} {at[1]} {z} {fnv(text(content))}")
        return Sprite(self, self.n)

    @contextlib.contextmanager
    def batch(self):
        self.log.append("[")
        yield
        self.log.append("]")


class Frame:
    id = 1
    origin = (0, 0)

    def __init__(self, w, h, rows):
        self.size, self.rows = (w, h), rows

    def shape(self, words):
        return list(self.rows)


cat._cli.on_screen = lambda frame: True
cat._cli.in_sight = contextlib.nullcontext


def screen(lines):
    return [" " * x + s for x, s in lines]


# Rows at (column, text): `#` ink, the words as shown.
A = [(0, "")] * 4 + [(2, "#" * 30)] + [(0, "")] * 5 + [(4, "#" * 50)] + [(0, "")] * 5
A += [(0, "#" * 20 + " " * 10 + "#" * 25)] + [(0, "")] * 3 + [(0, "#" * 70)]
B = [(0, "")] * 6 + [(10, "#" * 40)] + [(0, "")] * 6 + [(0, "#" * 60)]
B += [(0, "")] * 4 + [(30, "#" * 30)]
C = [(0, "")] * 3 + [(2, "## food ####")] + [(0, "")] * 5 + [(4, "#" * 30 + " fish " + "#" * 10)]
C += [(0, "")] * 5 + [(0, "#" * 20 + " toy " + "#" * 25)] + [(0, "")] * 3 + [(0, "#" * 70)]

# name, options, seed, size, screens, until, events (time, what...)
SCENES = [
    ("one", dict(count=1), 7, (80, 24), [A, B], 150.0, [
        (20.0, "cursor", 12, 3), (24.0, "cursor", None),
        (40.0, "idle", 1), (75.0, "idle", 0),
        (80.0, "screen", 1), (90.0, "cover", 0.5), (91.0, "cover", 0.0),
        (100.0, "cursor", 40, 5), (101.0, "cursor", 41, 6), (110.0, "cursor", None),
        (115.0, "idle", 1), (130.0, "screen", 0),
    ]),
    ("three", dict(count=3, varied=True), 11, (80, 24), [A, B], 150.0, [
        (30.0, "idle", 1), (60.0, "idle", 0), (70.0, "screen", 1),
        (85.0, "cursor", 20, 12), (88.0, "cursor", None), (100.0, "screen", 0),
    ]),
]
FEATURES = [
    ("food", dict(count=2, food=True), 3, (80, 24), [A, C], 200.0, [
        (60.0, "screen", 1), (120.0, "screen", 0), (150.0, "idle", 1),
    ]),
    ("mice", dict(count=2, mice=True), 5, (80, 24), [A, B], 200.0, [
        (90.0, "screen", 1), (140.0, "idle", 1), (170.0, "idle", 0),
    ]),
    ("fish", dict(count=3, fish=True), 9, (80, 24), [A, C], 200.0, [
        (100.0, "screen", 1), (160.0, "screen", 0),
    ]),
    ("toys", dict(count=2, toys=True), 13, (80, 24), [A, C], 200.0, [
        (100.0, "screen", 1), (160.0, "screen", 0),
    ]),
    ("hunt", dict(count=2, hunt=True), 17, (80, 24), [A], 120.0, [
        (5.0, "hover", 30, 8), (6.0, "hover", 33, 8), (7.0, "hover", 36, 9),
        (12.0, "hover", 20, 14), (20.0, "hover", 22, 14), (30.0, "hover", None),
        (40.0, "hover", 50, 2), (41.0, "hover", 45, 6), (60.0, "hover", 10, 18),
        (90.0, "hover", None),
    ]),
    ("all", dict(count=2, varied=True, food=True, mice=True, fish=True, toys=True, hunt=True),
     21, (80, 24), [A, C], 240.0, [
        (30.0, "hover", 30, 8), (31.0, "hover", 35, 9), (50.0, "hover", None),
        (80.0, "screen", 1), (120.0, "idle", 1), (150.0, "idle", 0), (200.0, "screen", 0),
    ]),
]


def tick(k):
    return k / 30 + ((k * 7919) % 13) / 1000


def run(scene):
    """Yield (second, line) of `scene`."""
    name, opts, seed, (w, h), screens, until, events = scene
    pilot = Pilot()
    frame = Frame(w, h, screen(screens[0]))
    home = cat.Home(pilot, frame, random.Random(seed), **opts)
    events = list(events)
    k = 0
    while (now := tick(k)) < until:
        k += 1
        if k % 50 in (1, 2, 3):
            continue
        while events and events[0][0] <= now:
            _, what, *rest = events.pop(0)
            pilot.log.append(f"ev {what}")
            if what == "idle":
                home.on("idle", {"frame": 1, "idle": bool(rest[0])}, now)
            elif what == "cursor" and rest[0] is None:
                home.on("cursor", {"visible": False}, now)
            elif what == "cursor":
                home.on("cursor", {"visible": True, "frame": 1, "pos": rest}, now)
            elif what == "screen":
                frame.rows = screen(screens[rest[0]])
                home.on("screen", {}, now)
            elif what == "cover":
                pilot.cover = rest[0]
            elif what == "hover" and rest[0] is None:
                home.on("hover", {"sprite": None}, now)
            elif what == "hover":
                home.on("hover", {"sprite": 1, "frame": 1, "pos": rest}, now)
        home.step(now)
        for i, c in enumerate(home.cats):
            pilot.log.append(f"cat {i} {c.act} {c.x} {c.g} {int(c.right)} {bits(c.due)}")
        pilot.log.append(f"due {bits(home.due(now))}")
        for line in pilot.log:
            yield int(now), line
        pilot.log.clear()


def header(scene):
    name, opts, seed, (w, h), screens, until, events = scene
    o = " ".join(f"{k}={int(v)}" for k, v in sorted(opts.items()))
    yield f"scene {name} seed={seed} w={w} h={h} until={until} {o}"
    for s in screens:
        yield "screen"
        for row in screen(s):
            yield f"| {row}"
    for t, what, *rest in events:
        yield f"ev {t} {what} {' '.join('-' if r is None else str(r) for r in rest)}"


def main():
    scenes = SCENES + FEATURES
    if sys.argv[1:2] == ["show"]:
        scene = next(s for s in scenes if s[0] == sys.argv[2])
        for sec, line in run(scene):
            if sec == int(sys.argv[3]):
                print(line)
        return
    for scene in scenes:
        for line in header(scene):
            print(line)
        h, sec = 0, 0
        for s, line in run(scene):
            if s != sec:
                print(f"sum {sec} {h:016x}")
                sec = s
            h = int(fnv(f"{h:016x}{line}"), 16)
        print(f"sum {sec} {h:016x}")
        print("end")


main()
