# SPDX-License-Identifier: GPL-3.0-or-later
"""The wall: what `brick` draws.

- A brick, a racket, a ball, a capsule, a falling brick: one sprite
  each, in frame coordinates. Those that blink are animations played by
  the driver; the client moves them.
- The score: one sprite in view coordinates, top right.
- Particles (`_arcade.spark`): each one request, played and removed by
  the driver.

`Stage` keeps the sprites in step with a game. Bricks are remembered by
the game wherever they are; their sprites exist within a view's height
of the view.

Layers: all behind the text. `z`, from the back: bricks, capsules and
falling bricks, rackets, balls, particles, the score.
"""

from __future__ import annotations

import random
from typing import Any

from ptyrc import Frame, Pilot, Sprite

from ._arcade import Board, burst, spark

__all__ = [
    "LABEL",
    "PALETTE",
    "TEAM",
    "Stage",
    "ball",
    "board",
    "brick",
    "capsule",
    "faller",
    "label",
    "racket",
]

PALETTE = (196, 208, 226, 46, 51, 33, 201)  # bricks, by row
STEEL = {3: ("⣿⣿⣿⣿", 255), 2: ("⢾⣻⣟⡷", 250), 1: ("⠪⡢⢔⠕", 245)}  # a hard brick, by lives
TEAM = (51, 214)  # two rackets; one alone is white
FLAME = (226, 208, 196)
LABEL = {
    "wide": ("W", 46),
    "twin": ("T", 51),
    "multi": ("M", 201),
    "small": ("N", 244),
    "fast": ("S", 226),
    "fire": ("F", 196),
}

Color = int | str
Cell = tuple[int, int]
Box = tuple[int, int, int, int]


def _loop(frames: list[tuple[str, Color]], ms: int, **style: Any) -> dict[str, Any]:
    return {
        "frames": [
            {"cells": [[{"t": t, "fg": c, **style}]], "ms": ms} for t, c in frames
        ],
        "loop": True,
    }


def brick(kind: str, lives: int, color: int) -> dict[str, Any]:
    """A brick of 4 cells in braille, as arguments of `Pilot.draw` less
    the place: round ends, so that two of a colour show as two."""
    if kind == "hard":
        glyphs, ink = STEEL[max(1, min(3, lives))]
        return {"cells": [[{"t": glyphs, "fg": ink, "bold": True}]]}
    if kind == "loose":
        return {"cells": [[{"t": "⠲⠶⠶⠖", "fg": PALETTE[color]}]]}
    if kind == "bumper":
        inks = (201, 213) if lives > 1 else (213, 225)
        return _loop(list(zip(("⢎⡱⢎⡱", "⡱⢎⡱⢎"), inks, strict=True)), 300, bold=True)
    if kind == "bomb":
        return _loop([("⢾⣏⣹⡷", 196), ("⢾⣹⣏⡷", 226)], 250, bold=True)
    return {"cells": [[{"t": "⢾⣿⣿⡷", "fg": PALETTE[color]}]]}


def faller(color: int) -> dict[str, Any]:
    """A loose brick on its way down: it shakes."""
    return _loop([("⠲⠶⠶⠖", 196), ("⠴⠶⠶⠦", PALETTE[color])], 90, bold=True)


def racket(width: int, team: int | None = None) -> list[list[dict[str, Any]]]:
    """A racket: the same on both sides. `team`: None when alone."""
    ink = "bright-white" if team is None else TEAM[team]
    return [[{"t": "[" + "=" * (width - 2) + "]", "fg": ink, "bold": True}]]


def ball(team: int | None = None, fire: bool = False) -> dict[str, Any]:
    """A ball, of its owner's colour; on fire, it flickers."""
    if fire:
        return _loop([("o", c) for c in FLAME], 70, bold=True)
    ink = "bright-white" if team is None else TEAM[team]
    return {"cells": [[{"t": "o", "fg": ink, "bold": True}]]}


def capsule(kind: str) -> dict[str, Any]:
    """A capsule: its letter, blinking."""
    letter, ink = LABEL[kind]
    return {
        "frames": [
            {"cells": [[{"t": letter, "fg": 16, "bg": ink, "bold": True}]], "ms": 220},
            {"cells": [[{"t": letter, "fg": ink, "bold": True}]], "ms": 220},
        ],
        "loop": True,
    }


def label(text: str, ink: Color) -> dict[str, Any]:
    """A word that rises and goes."""
    return {
        "frames": [
            {"cells": [[{"t": text, "fg": ink, "bold": True}]], "ms": 500},
            {"cells": [[{"t": text, "fg": ink, "dim": True}]], "ms": 250},
        ],
        "loop": False,
        "motion": {"vx": 0.0, "vy": -3.0, "ax": 0.0, "ay": 0.0, "bounce": False},
        "ttl": 1.0,
    }


def board(
    score: list[int], best: list[int] | None = None
) -> list[list[dict[str, Any]]]:
    """The score; two: the keeper of the top, then of the bottom. With
    `best`, the highest so far."""
    if len(score) == 1:
        runs = [{"t": f"◆ {score[0]}", "fg": "bright-white", "bold": True}]
    else:
        runs = [
            {"t": f"▲ {score[0]}", "fg": TEAM[0], "bold": True},
            {"t": "  "},
            {"t": f"▼ {score[1]}", "fg": TEAM[1], "bold": True},
        ]
    if best is not None:
        runs.append({"t": "  best " + " ".join(map(str, best)), "dim": True})
    return [runs]


class Stage:
    """The sprites of a `brick.Game`; particles and more when
    `effects`."""

    def __init__(
        self, pilot: Pilot, frame: Frame, rng: random.Random, effects: bool
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.effects = effects
        self.bricks: dict[int, tuple[Sprite, int]] = {}  # by serial: sprite, lives
        self.moving: dict[int, tuple[Sprite, Any]] = {}  # sprite, what is shown
        self.board = Board(pilot, frame)

    def draw(self, at: Cell, **kw: Any) -> Sprite:
        return self.pilot.draw(frame=self.frame.id, at=at, space="frame", **kw)

    def sparks(self, at: Cell, n: int, colors: Any, **kw: Any) -> None:
        for one in burst(n, self.rng, colors, **kw):
            self.draw(at, **{**one, "z": 4})

    def show(self, game: Any, box: Box) -> None:
        """One step: inside a batch."""
        x0, y0, x1, y1 = box
        alone = game.teams == 1
        reach = y1 - y0
        near = {b.n: b for b in game.bricks((x0, y0 - reach, x1, y1 + reach))}
        for n in [n for n in self.bricks if n not in near]:
            self.bricks.pop(n)[0].remove()
        for n, it in near.items():
            if n not in self.bricks:
                look = brick(it.kind, it.lives, it.color)
                self.bricks[n] = (self.draw((it.x, it.y), z=0, **look), it.lives)
            elif self.bricks[n][1] != it.lives:
                self.bricks[n][0].set(**brick(it.kind, it.lives, it.color))
                self.bricks[n] = (self.bricks[n][0], it.lives)

        things: dict[int, tuple[Cell, Any, int, dict[str, Any]]] = {}
        for it in game.fallers:
            things[it.n] = ((it.x, it.y), "faller", 1, faller(it.color))
        for it in game.capsules:
            things[it.n] = ((it.x, it.y), it.kind, 1, capsule(it.kind))
        for it in game.rackets:
            if not it.dead:
                team = None if alone else it.team
                look = {"cells": racket(it.w, team)}
                things[it.n] = ((it.x, it.y), it.w, 2, look)
        for it in game.balls:
            team, hot = None if alone else it.owner, bool(it.fire)
            things[it.n] = (it.at, (team, hot), 3, ball(team, hot))
        for n in [n for n in self.moving if n not in things]:
            self.moving.pop(n)[0].remove()
        for n, (at, key, z, look) in things.items():
            if n not in self.moving:
                self.moving[n] = (self.draw(at, z=z, **look), (at, key))
                continue
            sprite, (was, shown) = self.moving[n]
            if shown != key:
                sprite.set(**look)
            if was != at:
                sprite.move(*at, space="frame")
            self.moving[n] = (sprite, (at, key))

        self.board.show(board(game.score, game.best if self.effects else None))

        for it in game.balls:
            if it.fire:  # flames behind a ball on fire
                one = spark(self.rng, FLAME, "*+.", (0.5, 3.0), (0.2, 0.5), -4.0)
                self.draw(it.at, **{**one, "z": 4})
            elif self.effects:
                self.draw(it.at, rows=["·"], dim=True, ttl=0.25, z=1)
        for event in game.events:
            self.play(*event)

    def play(self, what: str, at: Any = None, *data: Any) -> None:
        """An event of the game, as particles. A blast and a kill always
        show; the others when `effects`."""
        if what == "boom":
            self.sparks(at, 22, (231, *FLAME), speed=(6.0, 22.0))
        elif what == "kill":
            self.sparks(at, 18, ("bright-white", 196), glyphs="=-.", gravity=14.0)
        if not self.effects:
            return
        if what == "break":
            it = data[0]
            ink = PALETTE[it.color] if it.kind in ("plain", "loose") else 250
            self.sparks(at, 5, (ink,), glyphs="⠶⠒.", speed=(2.0, 9.0), life=(0.3, 0.6))
        elif what == "hit":
            self.sparks(at, 2, (252,), glyphs="+.", speed=(2.0, 6.0), life=(0.2, 0.4))
        elif what == "bump":
            self.sparks(at, 6, (201, 213), glyphs="≈~.", gravity=0.0)
        elif what == "crash":
            self.sparks(at, 6, (244, 196), glyphs="⠶⠒.", speed=(2.0, 8.0))
        elif what == "lost":
            self.sparks(at, 12, (196, 160), glyphs="o*.", speed=(4.0, 12.0))
            self.draw(at, **{**label("0", 196), "z": 4})
        elif what == "serve":
            self.sparks(at, 5, (231,), glyphs="+'.", speed=(2.0, 6.0), life=(0.2, 0.4))
        elif what == "catch":
            letter, ink = LABEL[data[0]]
            self.sparks(at, 8, (ink,), glyphs=letter + "*.", gravity=0.0)
            self.draw(at, **{**label(data[0], ink), "z": 4})
        elif what == "area":
            for it in at:
                one = spark(self.rng, (231,), "+'", (0.0, 1.0), (0.2, 0.5), 0.0)
                self.draw((it.x + self.rng.randrange(4), it.y), **{**one, "z": 4})
