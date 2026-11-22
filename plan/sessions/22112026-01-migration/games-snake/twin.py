# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's games as digests: the fixtures of snake/twin.tests.js.
Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/games-snake/twin.py
> ../../store-apps/snake/twin.tests.js"""

import json
import random

from ptyrc_apps import _arcade, snake

B, M = 2**24, 2**25


def key(c):
    return (c[0] + B) * M + (c[1] + B)


def state(g):
    """The game after a step, as text."""
    body = ";".join(f"{x},{y}" for x, y in g.body)
    apple = "-" if g.apple is None else f"{g.apple[0]},{g.apple[1]}"
    foes = ";".join(f"{f.n}:{f.at[0]},{f.at[1]}" for f in g.foes)
    beams = ";".join(
        f"{b.n}:{b.cells[0][0]},{b.cells[0][1]}:{len(b.cells)}:{int(b.across)}:{b.fuse}:{b.left}"
        for b in g.beams
    )
    events = ";".join(e[0] for e in g.events)
    lumps = ",".join(map(str, g.lumps))
    return "|".join(
        [body, apple, str(int(g.golden)), str(g.grow), str(g.eaten), foes, beams,
         events, lumps, str(int(g.ghost))]
    )


def fnv(h, text):
    for b in text.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


GAME = ["", "  wall of words  ", "", "   another   one", ""]
RULES = ["", "  wall of words  ", "", "   another   one", "", "", ""]
ROOM = [
    "          #         ",
    "########  #  ###### ",
    "       #  #  #    # ",
    "       #  #  #### # ",
    "       #  #       # ",
    "########  ######### ",
    "                    ",
]
GAMES = [
    # rows, x0, y0, box, seed, flags, steps
    (GAME, 10, 100, (10, 100, 30, 105), 3, {}, 400),
    (RULES, 10, 100, (10, 100, 40, 107), 5, {"golden": True, "enemies": True, "lasers": True}, 3000),
    (RULES, 10, 100, (10, 100, 40, 107), 9, {"lasers": True}, 300),
    (ROOM, 0, 0, (0, 0, 20, 7), 1, {}, 300),
    (ROOM, 0, 0, (0, 0, 20, 7), 11, {"golden": True, "enemies": True}, 600),
    ([], 0, 0, (0, 0, 24, 8), 4, {"golden": True, "enemies": True, "lasers": True}, 800),
]
games = []
for rows, x0, y0, box, seed, flags, steps in GAMES:
    walls = snake.walls_of(rows, x0, y0)
    g = snake.Game(random.Random(seed), **flags)
    h, marks = 2166136261, []
    for i in range(steps):
        g.step(box, walls)
        h = fnv(h, state(g))
        if (i + 1) % 100 == 0 or i + 1 == steps:
            marks.append(h)
    games.append([rows, x0, y0, list(box), seed, flags, steps, marks, state(g)])

rooms = []
walls = snake.walls_of(ROOM)
for start, need in [((0, 6), 10), ((0, 6), 30), (None, 10), ((0, 2), 10), ((0, 1), 10), (None, 0), ((9, 5), 1), ((19, 3), 60)]:
    got = snake.room((0, 0, 20, 7), walls, start, need)
    rooms.append([start and list(start), need, [list(c) for c in got]])

draws = []
for n in (0, 1, 3, 2**32 + 7, 999999999):
    r = random.Random(n)
    draws.append([
        n,
        [r.randrange(10, 30) for _ in range(5)],
        [r.randint(20, 50) for _ in range(3)],
        r.choice(list(range(7))),
        r.sample((True, False), 2),
        r.sample(list(range(100)), 40),
        r.sample(list(range(12)), 5),
        r.choices(["a", "b", "c"], [5, 1, 3])[0],
        r.uniform(4.0, 14.0),
        r.random(),
    ])

r = random.Random(2)
sparks = [_arcade.spark(r, (1, 2)), _arcade.debris(r), *_arcade.burst(2, r, (7,), glyphs="x+'.", speed=(3.0, 9.0))]

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's games, rooms and draws: written by")
print("// plan/sessions/22112026-01-migration/games-snake/twin.py.\n")
print("export const GAMES = " + json.dumps(games) + ";\n")
print("export const ROOMS = " + json.dumps(rooms) + ";\n")
print("export const DRAWS = " + json.dumps(draws) + ";\n")
print("export const SPARKS = " + json.dumps(sparks, ensure_ascii=False) + ";")
