# SPDX-License-Identifier: GPL-3.0-or-later
"""What `blocks` draws.

- The heap: one sprite, redrawn when it changes.
- A piece: one sprite, redrawn each step.
- The score: one sprite in the top-right corner of the view.
- A broken block (`crumb`): one request, a finite animation in braille
  with a `motion`, played and removed by the driver.

All behind the text. `z`, from the back: the heap, the pieces,
particles, the score.
"""

from __future__ import annotations

import random
from typing import Any

from ptyrc import Frame, Pilot, Sprite

from ._arcade import spark

__all__ = ["CRUMBS", "Stage", "board", "crumb"]

CRUMBS = 48  # particles a step, at most

Color = int | str
Cell = tuple[int, int]


def crumb(rng: random.Random, color: Color, sideways: bool = False) -> dict[str, Any]:
    """A broken block: it loses its dots and falls the way pieces do."""
    one = spark(rng, (color,), "⣿⠶⠒⠂", (2.0, 9.0), (0.4, 0.9), 10.0)
    if sideways:
        one["motion"].update(ax=-20.0, ay=0.0)
    return one


def board(score: int) -> list[list[dict[str, Any]]]:
    return [[{"t": f"⣿ {score}", "fg": "bright-white", "bold": True}]]


class Stage:
    """The sprites of a `blocks.Game`; particles when `effects`, the
    score when `score`."""

    def __init__(
        self,
        pilot: Pilot,
        frame: Frame,
        rng: random.Random,
        *,
        effects: bool = False,
        score: bool = False,
        sideways: bool = False,
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.effects, self.score, self.sideways = effects, score, sideways
        self.heap: Sprite | None = None
        self.rev: int | None = None
        self.pieces: dict[int, tuple[Sprite, Any]] = {}  # sprite, what is shown
        self.board: tuple[Sprite, int] | None = None

    def draw(self, at: Cell, **kw: Any) -> Sprite:
        return self.pilot.draw(frame=self.frame.id, at=at, space="frame", **kw)

    def _put(self, sprite: Sprite | None, look: Any, z: int) -> Sprite:
        at, cells = look
        if sprite is None:
            return self.draw(at, cells=cells, z=z)
        sprite.update(
            anchor={"at": list(at), "space": "frame"}, content={"cells": cells}
        )
        return sprite

    def show(self, game: Any) -> None:
        """One step: inside a batch."""
        if self.rev != game.rev:
            self.rev = game.rev
            look = game.heap_picture()
            if look is not None:
                self.heap = self._put(self.heap, look, 0)
            elif self.heap is not None:
                self.heap.remove()
                self.heap = None
        falling = {p.n: p for p in game.pieces}
        for n in [n for n in self.pieces if n not in falling]:
            self.pieces.pop(n)[0].remove()
        for n, piece in falling.items():
            look = game.picture(piece)
            sprite, shown = self.pieces.get(n, (None, None))
            if shown != look:
                self.pieces[n] = (self._put(sprite, look, 1), look)
        if self.score and (self.board is None or self.board[1] != game.score):
            cells = board(game.score)
            at = [-len(cells[0][0]["t"]) - 1, 0]
            if self.board is None:
                sprite = self.pilot.draw(
                    cells=cells, frame=self.frame.id, at=at, space="view", z=5
                )
            else:
                sprite = self.board[0]
                sprite.update(
                    anchor={"at": at, "space": "view"}, content={"cells": cells}
                )
            self.board = (sprite, game.score)
        if self.effects:
            self.play(game.events)

    def play(self, events: list[tuple[Any, ...]]) -> None:
        """The events of a step, as particles: `CRUMBS` at most."""
        broken: list[tuple[Cell, Color]] = []
        for what, *data in events:
            if what == "break":
                broken.append((data[0], data[1]))
            elif what == "lost":
                broken += [(at, data[1]) for at in data[0]]
            elif what == "over":
                broken += data[0]
        for at, color in self.rng.sample(broken, min(len(broken), CRUMBS)):
            self.draw(at, **crumb(self.rng, color, self.sideways))
