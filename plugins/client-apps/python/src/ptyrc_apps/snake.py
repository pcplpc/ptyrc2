# SPDX-License-Identifier: GPL-3.0-or-later
"""Snake: plays alone, around the words of a scrolling buffer.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app snake
    ptyrc-app snake --speed 20 -f active

What it shows:

- Reading the screen as a map: every glyph of the pane is a wall. The
  snake finds its way between the words to the apple (a breadth-first
  search over the free cells of the view).
- Frame coordinates: the snake and the apple belong to the text, not to
  the window. Scroll and they move with it; when the snake is scrolled
  out of sight it starts again where you look.
- One sprite for the whole snake, redrawn in place each step with
  `Sprite.update`: anchor and content change together, in one repaint.
"""

from __future__ import annotations

import argparse
import random
import unicodedata
from collections import deque
from collections.abc import Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = ["Game", "main", "route", "walls_of"]

Cell = tuple[int, int]
STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))


def walls_of(rows: Sequence[str], x0: int = 0, y0: int = 0) -> set[Cell]:
    """Cells holding a glyph, `rows` being the text from `(x0, y0)`. A
    wide glyph takes two columns."""
    out = set()
    for y, row in enumerate(rows, y0):
        x = x0
        for ch in row:
            wide = unicodedata.east_asian_width(ch) in "WF"
            if not ch.isspace():
                out.add((x, y))
                if wide:
                    out.add((x + 1, y))
            x += 2 if wide else 1
    return out


def route(start: Cell, goal: Cell, free) -> Cell | None:
    """First step of a shortest path from `start` to `goal` over cells
    where `free(cell)` holds; None when there is none."""
    first: dict[Cell, Cell] = {}
    queue = deque([start])
    seen = {start}
    while queue:
        here = queue.popleft()
        if here == goal:
            return first.get(here)
        for dx, dy in STEPS:
            there = (here[0] + dx, here[1] + dy)
            if there not in seen and (there == goal or free(there)):
                seen.add(there)
                first[there] = first.get(here, there)
                queue.append(there)
    return None


class Game:
    """The snake, the apple and the rules. No drawing here."""

    def __init__(self, rng: random.Random) -> None:
        self.rng = rng
        self.body: deque[Cell] = deque()  # head first
        self.apple: Cell | None = None
        self.grow = 0
        self.eaten = 0

    def free_cell(self, box: tuple[int, int, int, int], walls: set[Cell]):
        x0, y0, x1, y1 = box
        for _ in range(200):
            cell = (self.rng.randrange(x0, x1), self.rng.randrange(y0, y1))
            if cell not in walls and cell not in self.body:
                return cell
        return None

    def step(self, box: tuple[int, int, int, int], walls: set[Cell]) -> None:
        """One move inside `box` (x0, y0, x1, y1: the view, frame cells)."""
        x0, y0, x1, y1 = box

        def inside(c: Cell) -> bool:
            return x0 <= c[0] < x1 and y0 <= c[1] < y1

        def free(c: Cell) -> bool:
            return inside(c) and c not in walls and c not in self.body

        if not self.body or not inside(self.body[0]) or self.body[0] in walls:
            # Start, or scrolled out of sight, or text written on its head.
            start = self.free_cell(box, walls)
            self.body = deque([start] if start else [])
            self.grow = 3
        if self.apple is None or not inside(self.apple) or self.apple in walls:
            self.apple = self.free_cell(box, walls)
        if not self.body:
            return
        head = self.body[0]
        to = route(head, self.apple, free) if self.apple else None
        if to is None:
            # No way to the apple: any free neighbour, to stay alive.
            around = [(head[0] + dx, head[1] + dy) for dx, dy in STEPS]
            around = [c for c in around if free(c)]
            to = self.rng.choice(around) if around else None
        if to is None:
            # Cornered: shed the tail and try again from the head.
            self.body = deque([head])
            self.grow = 3
            return
        self.body.appendleft(to)
        if to == self.apple:
            self.apple, self.grow, self.eaten = None, self.grow + 2, self.eaten + 1
        if self.grow:
            self.grow -= 1
        else:
            self.body.pop()

    def picture(self) -> tuple[Cell, list[list[dict[str, object]]]]:
        """The snake as a sprite: its top-left cell and rows of runs."""
        xs = [c[0] for c in self.body]
        ys = [c[1] for c in self.body]
        left, top = min(xs), min(ys)
        rows = [[" "] * (max(xs) - left + 1) for _ in range(max(ys) - top + 1)]
        for i, (x, y) in enumerate(self.body):
            rows[y - top][x - left] = "@" if i == 0 else "o"
        style = {"@": {"fg": "bright-green", "bold": True}, "o": {"fg": "green"}}
        return (left, top), [
            [{"t": ch, **style.get(ch, {})} for ch in row] for row in rows
        ]


def view(frame: Frame) -> tuple[int, int, int, int]:
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h)


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    game = Game(random.Random(args.seed))
    frame = _cli.pick(pilot, args.frame)
    snake: Sprite | None = None
    apple: Sprite | None = None
    shown_apple = None
    for _ in _cli.ticks(1.0 / args.speed, args.seconds):
        if not _cli.on_screen(frame):
            continue
        box = view(frame)
        with _cli.in_sight():
            game.step(box, walls_of(frame.text(), box[0], box[1]))
        if not game.body:
            continue  # a full screen: wait for room
        at, cells = game.picture()
        anchor = {"at": list(at), "space": "frame"}
        with _cli.in_sight(), pilot.batch():
            if snake is None:
                snake = pilot.draw(cells=cells, frame=frame.id, at=at, space="frame")
            else:
                snake.update(anchor=anchor, content={"cells": cells})
            if game.apple != shown_apple and game.apple is not None:
                if apple is None:
                    apple = pilot.draw(
                        ["●"],
                        frame=frame.id,
                        at=game.apple,
                        space="frame",
                        fg="bright-red",
                    )
                else:
                    apple.move(*game.apple, space="frame")
                shown_apple = game.apple


def main(argv: Sequence[str] | None = None) -> int:
    out = _cli.parser(
        "snake", "A snake that plays alone between the words.", frame="largest"
    )
    out.add_argument(
        "--speed", type=float, default=12.0, help="moves per second [default: 12]"
    )
    out.add_argument("--seed", type=int, help="same seed, same game")
    out.add_argument("--seconds", type=float, help="stop after this long")
    args = _cli.parse(out, argv)
    if args.speed <= 0:
        out.error("--speed must be positive")
    return _cli.run(app, args)


if __name__ == "__main__":
    raise SystemExit(main())
