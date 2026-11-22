# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's pure parts as fixtures of cat/src/twin.txt: a line an item,
`NAME VALUE`; a look as the FNV-1a 64 hash of its text (`text`). Run in
plugins/client-apps/python:
uv run python PATH/twin.py > ../../store-apps/cat/src/twin.txt
With `show NAME`: the text of that look."""
import contextlib
import math
import random
import struct
import sys
from types import SimpleNamespace

from ptyrc_apps import cat
from ptyrc_apps.cat import COATS, POSES, Skin


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


SKINS = (cat.PLAIN, Skin("|\\_/|", "•●", "w"), Skin("/^_^\\", "*O", "x"), Skin("/\\-/\\", "uO", "v"))

SCREENS = (
    (["", "", "", "hello world, this is a line", "", "", "", "  short", "", "",
      "a long line of text with words in it and more"], (0, 0, 60, 14)),
    (["", "", "", "#### ##### ###", "", "", "", "############", "", "   ###########"],
     (3, 2, 40, 12)),
    ([], (0, 0, 30, 6)),
    (["x" * 30, "", "", "", "   " + "y" * 20], (0, 0, 30, 8)),
)


def looks():
    for p in POSES:
        n = len(POSES[p])
        for right in (True, False):
            for step in (None, 0, 1, n + 2):
                for c in (0, 4):
                    yield f"look.{p}.{int(right)}.{step}.{c}.0", cat.look(p, right, step, COATS[c])
        for k, skin in enumerate(SKINS[1:], 1):
            yield f"look.{p}.1.None.{k}.{k}", cat.look(p, True, None, COATS[k], skin)
            yield f"look.{p}.0.1.{k + 4}.{k}", cat.look(p, False, 1, COATS[k + 4], skin)
    for left in range(5):
        yield f"plate.{left}", cat.plate_look(left)
    for right in (True, False):
        for step in (None, 0, 1, 7):
            yield f"mouse.{int(right)}.{step}", cat.mouse_look(right, step)
        for left in range(4):
            for flop in (False, True):
                yield f"fish.{left}.{int(right)}.{int(flop)}", cat.fish_look(left, right, flop)
    for plays in (0, 2):
        for dx in (0, 1):
            for dy in range(4):
                for turn in range(5):
                    yield (f"ball.{plays}.{dx}.{dy}.{turn}",
                           cat.ball_look(203, plays, dx, dy, turn))


def bits(v):
    return struct.unpack("<Q", struct.pack("<d", float(v)))[0]


YARN_SCREENS = (
    (SCREENS[0][0], (0, 0, 60, 14), 10, 4, 3),
    (["", "", "", "", "     a stair of text", "", "",
      "                 and lower down", "", "", "", "                  at the very end"],
     (0, 0, 50, 13), 21, 5, 11),
)


def yarns():
    """For each screen and seed: a ball batted whenever it rests, a
    line a step of `TICK`: its state, its thread, its picture."""
    for i, (rows, view, x, g, seed) in enumerate(YARN_SCREENS):
        width, height = view[2] - view[0], view[3] - view[1]
        rows = cat.pad([*rows, *[""] * height][:height], width)
        home = SimpleNamespace(rng=random.Random(seed), stale=False, view=view,
                               rows=rows, walls=[cat.mass(r) for r in rows])
        home.solid = lambda cx, cy, home=home: cat.Home.solid(home, cx, cy)
        puts = []
        home.together = contextlib.nullcontext
        home.put = lambda sprite, x, y, look, z=0: puts.append(
            f"{x},{y},{z},{fnv(text(look))}") or 1
        yarn = cat.Yarn(home, x, g, None, math.inf)
        yield f"yarn.{i}.new", f"{yarn.color} {yarn.plays}"
        now, way = 0.0, 1
        for k in range(400):
            if not yarn.moving and not yarn.loose and yarn.plays > 0 and k % 40 == 0:
                yarn.knock(way, now)
                way = -way
            if now >= yarn.due:
                yarn.step(now)
            if k == 250:
                home.walls[g - 1] = [False] * width
                yarn.wake(now)
            st = [bits(v) for v in (yarn.px, yarn.py, yarn.vx, yarn.vy, yarn.rolled)]
            st += [yarn.way, int(yarn.moving), yarn.plays, len(yarn.points), yarn.x, yarn.g]
            pts = fnv(repr(yarn.points))
            strand = yarn.strand()
            wire = fnv(repr(strand)) if strand else "-"
            drawn = ";".join(puts) or "-"
            puts.clear()
            yield (f"yarn.{i}.{k}",
                   " ".join(map(str, st)) + f" {pts} {wire} {int(home.stale)} {drawn}")
            now += cat.TICK / 2 if k % 3 else cat.TICK * 1.7
        fade = yarn.fade()
        yield f"yarn.{i}.fade", fnv(text(fade)) if fade else "-"


def main():
    if sys.argv[1:2] == ["show"]:
        for name, look in looks():
            if name == sys.argv[2]:
                print(text(look))
        return
    for name, look in looks():
        print(name, fnv(text(look)))
    print("wave", ",".join(str(cat.wave(x)) for x in range(-8, 40)))
    for i, (rows, box) in enumerate(SCREENS):
        spots = cat.ledges(rows, box)
        print(f"ledges.{i}", ",".join(f"{s.g}:{s.x0}:{s.x1}" for s in spots) or "-")
    for i, row in enumerate(("", "ab  cd   ef    gh", " x x  x   x    ", "#  #")):
        print(f"mass.{i}", "".join("#" if b else "." for b in cat.mass(row)) or "-")
    rng = random.Random(7)
    for i in range(40):
        a = (rng.randint(0, 70), rng.randint(3, 30))
        b = (rng.randint(0, 70), rng.randint(3, 30))
        print(f"arc.{a[0]}.{a[1]}.{b[0]}.{b[1]}",
              ",".join(f"{x}:{y}" for x, y in cat.arc(a, b)))
    for i in range(40):
        xs = [rng.randint(0, 40) for _ in range(3)]
        print(f"set.{xs[0]}.{xs[1]}.{xs[2]}", ",".join(str(x) for x in {xs[0], xs[1], xs[2]}))
    for i in range(30):
        x, g = rng.randint(0, 40), rng.randint(3, 20)
        c = (rng.randint(-2, 60), rng.randint(-2, 25))
        print(f"near.{x}.{g}.{c[0]}.{c[1]}", int(cat.near(x, g, c)))
    for name, value in yarns():
        print(name, value)


main()
