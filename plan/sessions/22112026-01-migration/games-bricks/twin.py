# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's games as digests: the fixtures of bricks/twin.js.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/games-bricks/twin.py
> ../../store-apps/bricks/twin.js"""

import json
import random
from math import floor

from ptyrc_apps import bricks, snake

F = 2**30


def q(v):
    return str(floor(v * F))


def state(g):
    """The game after a step, as text: floats as floor(v * 2**30)."""
    balls = ";".join(
        f"{b.n}:{q(b.x)},{q(b.y)},{q(b.vx)},{q(b.vy)},{b.owner},{b.fire},{b.fast},{b.combo},{b.phase}"
        for b in g.balls
    )
    rackets = ";".join(f"{r.n}:{r.x},{r.y},{r.w},{r.dead},{r.span}" for r in g.rackets)
    laid = ";".join(
        f"{b.n}:{b.x},{b.y},{b.kind},{b.lives},{b.color},{b.due}" for b in g.bricks((-9999, -9999, 9999, 9999))
    )
    caps = ";".join(f"{c.n}:{c.x},{c.y},{c.kind}" for c in g.capsules)
    falls = ";".join(f"{f.n}:{f.x},{f.y}" for f in g.fallers)
    events = ";".join(e[0] for e in g.events)
    score = ",".join(map(str, g.score))
    return "|".join([balls, rackets, laid, caps, falls, events, score])


def fnv(h, text):
    for b in text.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


PAGE = ["", "  the words of a page", "", "      and more words here, a line", "", "", "  short", "", "", "", "", ""]
WIDE = PAGE + [""] * 8 + ["   a last line, far below   "] + [""] * 3
GAMES = [
    # rows, x0, y0, box, seed, flags, steps, presses {step: key}
    (PAGE, 0, 0, (0, 0, 60, 12), 3, {}, 1200, {}),
    (PAGE, 10, 100, (10, 100, 70, 112), 5, {"rules": True, "bonuses": True}, 1600, {}),
    (WIDE, 0, 0, (0, 0, 80, 24), 7, {"multiplayer": True}, 1200, {}),
    (WIDE, 0, 0, (0, 0, 80, 24), 11, {"rules": True, "bonuses": True, "multiplayer": True}, 1600, {}),
    (PAGE, 0, 0, (0, 0, 60, 12), 2, {"player": 0}, 400, {30: "left", 60: "up", 90: "right", 91: "right", 150: "down", 200: "left"}),
    (WIDE, 0, 0, (0, 0, 80, 24), 4, {"multiplayer": True, "bonuses": True, "player": 1}, 800, {40: "left", 100: "up", 160: "right"}),
]
games = []
for rows, x0, y0, box, seed, flags, steps, presses in GAMES:
    walls = snake.walls_of(rows, x0, y0)
    g = bricks.Game(random.Random(seed), **flags)
    h, marks = 2166136261, []
    for i in range(steps):
        if i in presses:
            g.press(presses[i])
        g.step(box, walls)
        h = fnv(h, state(g))
        if (i + 1) % 100 == 0 or i + 1 == steps:
            marks.append(h)
    games.append([rows, x0, y0, list(box), seed, flags, steps, {str(k): v for k, v in presses.items()}, marks, state(g)])

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's games: written by")
print("// plan/sessions/22112026-01-migration/games-bricks/twin.py.")
print()
print(f"export const GAMES = {json.dumps(games, ensure_ascii=False)};")
