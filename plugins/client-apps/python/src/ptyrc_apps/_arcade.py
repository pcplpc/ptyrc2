# SPDX-License-Identifier: GPL-3.0-or-later
"""The arcade: what `snake` draws with `-e`, `-x` and `-l`.

- Particles (`spark`, `burst`, `debris`): each one request, a finite
  animation with a `motion`, played and removed by the driver.
- The apple, a foe: a looped animation, moved by the client.
- A beam: one sprite, its frames replaced when it fires.

`Stage` keeps the sprites of the foes and the beams of a game in step
with it, and turns its events into particles.

Layers: all behind the text. `z`, from the back: beams, the snake and
the apple, foes, particles.
"""

from __future__ import annotations

import math
import random
from collections.abc import Sequence
from typing import Any

from ptyrc import Frame, Pilot, Sprite

__all__ = [
    "BODY",
    "Board",
    "Stage",
    "apple",
    "beam",
    "burst",
    "debris",
    "foe",
    "spark",
    "tally",
]

BODY = (118, 82, 46, 40, 34, 28, 22)  # head to tail
LUMP = 226  # an apple on its way down
RED = (196, 202, 208, 160)
GOLD = (226, 220, 214, 231)
FOE = (201, 165, 213)
HOT = (231, 196, 203)
DEBRIS = 40  # particles of a lost tail, at most

Color = int | str
Cell = tuple[int, int]


def spark(
    rng: random.Random,
    colors: Sequence[Color],
    glyphs: str = "*+'.",
    speed: tuple[float, float] = (4.0, 14.0),
    life: tuple[float, float] = (0.4, 0.9),
    gravity: float = 8.0,
) -> dict[str, Any]:
    """A particle, as arguments of `Pilot.draw` less the place: it
    leaves in a random direction at `speed` (columns per second; a row
    counts double), goes through `glyphs` in `life` seconds and falls."""
    angle, v = rng.uniform(0, math.tau), rng.uniform(*speed)
    seconds = rng.uniform(*life)
    color = rng.choice(colors)
    last = len(glyphs) - 1
    frames = []
    for i, glyph in enumerate(glyphs):
        run: dict[str, Any] = {"t": glyph, "fg": color}
        if i == 0:
            run["bold"] = True
        if i == last and last:
            run["dim"] = True
        frames.append(
            {"cells": [[run]], "ms": max(40, int(seconds * 1000 / len(glyphs)))}
        )
    return {
        "frames": frames,
        "loop": False,
        "motion": {
            "vx": v * math.cos(angle),
            "vy": v * math.sin(angle) / 2,
            "ax": 0.0,
            "ay": gravity,
            "bounce": False,
        },
        "ttl": seconds + 0.5,
        "layer": "back",
        "z": 3,
    }


def burst(
    n: int, rng: random.Random, colors: Sequence[Color], **kw: Any
) -> list[dict[str, Any]]:
    """`n` particles from one cell."""
    return [spark(rng, colors, **kw) for _ in range(n)]


def debris(rng: random.Random) -> dict[str, Any]:
    """A segment of a lost tail: it crumbles and falls."""
    return spark(rng, BODY[2:], "o:.", (0.5, 4.0), (0.5, 1.1), 14.0)


def apple(golden: bool = False) -> dict[str, Any]:
    """The apple, pulsing; a golden one glitters."""
    colors, ms = (GOLD, 120) if golden else (RED[:1] + RED[3:], 350)
    return {
        "frames": [
            {"cells": [[{"t": "●", "fg": c, "bold": i == 0}]], "ms": ms}
            for i, c in enumerate(colors)
        ],
        "loop": True,
    }


def foe() -> dict[str, Any]:
    """A foe: a blade that spins."""
    return {
        "frames": [
            {"cells": [[{"t": t, "fg": c, "bold": True}]], "ms": 110}
            for t, c in zip("x+x+x+", FOE * 2, strict=True)
        ],
        "loop": True,
        "layer": "back",
        "z": 2,
    }


def beam(length: int, across: bool, firing: bool) -> dict[str, Any]:
    """A beam of `length` cells, along a row if `across`, else down a
    column: dashes that blink, then a line that flickers."""
    if firing:
        glyph, inks, ms = "━" if across else "┃", HOT, 60
    else:
        glyph, inks, ms = "╌" if across else "╎", (88, 160), 180

    def cells(color: Color) -> list[list[dict[str, Any]]]:
        run = (
            {"t": glyph, "fg": color, "bold": True}
            if firing
            else {"t": glyph, "fg": color}
        )
        if across:
            return [[{**run, "t": glyph * length}]]
        return [[run] for _ in range(length)]

    return {"frames": [{"cells": cells(c), "ms": ms} for c in inks], "loop": True}


def tally(length: int, best: int) -> list[list[dict[str, Any]]]:
    """The score: the length of the snake, and the longest it was."""
    return [
        [
            {"t": f"◆ {length}", "fg": "bright-green", "bold": True},
            {"t": f"  best {best}", "dim": True},
        ]
    ]


class Board:
    """A score: one sprite, top right in view cells, over the game.
    Its blanks are opaque: what passes under does not show through.
    Sent when it changes."""

    def __init__(self, pilot: Pilot, frame: Frame) -> None:
        self.pilot = pilot
        self.frame = frame
        self.sprite: Sprite | None = None
        self.shown: list[list[dict[str, Any]]] | None = None

    def show(self, cells: list[list[dict[str, Any]]]) -> None:
        """`cells`: one row of runs. Inside a batch."""
        if cells == self.shown:
            return
        at = [-sum(len(run["t"]) for run in cells[0]) - 1, 0]
        if self.sprite is None:
            self.sprite = self.pilot.draw(
                cells=cells, frame=self.frame.id, at=at, space="view", z=5, opaque=True
            )
        else:
            self.sprite.update(
                anchor={"at": at, "space": "view"},
                content={"cells": cells, "opaque": True},
            )
        self.shown = cells


class Stage:
    """The sprites of a game's foes and beams; when `effects`, its
    particles and its score. `game`: a `snake.Game`."""

    def __init__(
        self, pilot: Pilot, frame: Frame, rng: random.Random, effects: bool
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.effects = effects
        self.foes: dict[int, tuple[Sprite, Cell]] = {}
        self.beams: dict[int, tuple[Sprite, bool]] = {}
        self.board = Board(pilot, frame)
        self.best = 0

    def draw(self, at: Cell, **kw: Any) -> Sprite:
        return self.pilot.draw(frame=self.frame.id, at=at, space="frame", **kw)

    def show(self, game: Any) -> None:
        """One step: inside a batch."""
        alive = {f.n: f for f in game.foes}
        for n in [n for n in self.foes if n not in alive]:
            self.foes.pop(n)[0].remove()
        for n, it in alive.items():
            if n not in self.foes:
                self.foes[n] = (self.draw(it.at, **foe()), it.at)
            elif self.foes[n][1] != it.at:
                self.foes[n][0].move(*it.at, space="frame")
                self.foes[n] = (self.foes[n][0], it.at)
        lit = {b.n: b for b in game.beams}
        for n in [n for n in self.beams if n not in lit]:
            self.beams.pop(n)[0].remove()
        for n, it in lit.items():
            look = beam(len(it.cells), it.across, it.firing)
            if n not in self.beams:
                sprite = self.draw(it.cells[0], layer="back", z=-1, **look)
                self.beams[n] = (sprite, it.firing)
            elif self.beams[n][1] != it.firing:
                self.beams[n][0].set(**look)
                self.beams[n] = (self.beams[n][0], it.firing)
        if self.effects:
            for event in game.events:
                self.play(*event)
            self.best = max(self.best, len(game.body))
            self.board.show(tally(len(game.body), self.best))

    def play(self, what: str, *data: Any) -> None:
        """An event of the game, as particles."""
        rng = self.rng
        if what == "eat":
            at, golden = data
            sparks = (
                burst(26, rng, GOLD, speed=(6.0, 20.0))
                if golden
                else burst(10, rng, RED)
            )
            for one in sparks:
                self.draw(at, **one)
        elif what == "lost":
            (cells,) = data
            for at in rng.sample(cells, min(len(cells), DEBRIS)):
                self.draw(at, **debris(rng))
        elif what == "pop":
            (at,) = data
            for one in burst(12, rng, FOE, glyphs="x+'."):
                self.draw(at, **one)
        elif what == "zap":
            (it,) = data
            for at in (it.cells[0], it.cells[-1]):
                for one in burst(4, rng, HOT, speed=(3.0, 9.0), life=(0.2, 0.5)):
                    self.draw(at, **one)
