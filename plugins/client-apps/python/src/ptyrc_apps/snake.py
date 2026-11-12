# SPDX-License-Identifier: GPL-3.0-or-later
"""Snake: plays alone, around the words of a scrolling buffer.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app snake
    ptyrc-app snake --speed 20 -f active
    ptyrc-app snake -a               # the arcade

What it shows:

- Reading the screen as a map: every glyph of the pane is a wall. The
  snake finds its way between the words to the apple (a breadth-first
  search over the free cells of the view).
- The topic `text` (`_cli.Sight`): the driver tells when the text of
  the pane changed or scrolled; the map is read again then, and only
  then. Between two changes a move costs one batch of drawing.
- Frame coordinates: the snake and the apple belong to the text, not to
  the window. Scroll and they move with it; when the snake is scrolled
  out of sight it starts again where you look.
- One sprite for the whole snake, redrawn in place each step with
  `Sprite.update`: anchor and content change together, in one repaint.

More, each on its own flag; `-a` for all (see `_arcade`):

- `-e`: an apple bursts into particles when eaten and goes down the
  body as a lump; one apple in six is golden and worth more; a lost
  tail crumbles. The body is shaded from head to tail.
- `-x`: foes. One goes for the middle of the body at half the snake's
  speed, bites it off there and is gone. The snake goes around them.
- `-l`: lasers. A beam is announced along a free row or column, from
  wall to wall, then fires: what is in it burns, the snake from the
  first segment hit, foes too. The snake keeps out of an announced
  beam when it can.

Rules count in steps of the game, not in seconds.
"""

from __future__ import annotations

import argparse
import random
import unicodedata
from collections import deque
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Frame, Pilot, Sprite

from . import _arcade, _cli

__all__ = [
    "BEAMS",
    "BEAM_EVERY",
    "FIRE",
    "FOES",
    "FOE_EVERY",
    "FUSE",
    "GOLDEN",
    "Beam",
    "Foe",
    "Game",
    "main",
    "route",
    "walls_of",
]

Cell = tuple[int, int]
Box = tuple[int, int, int, int]
STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))
GOLDEN = 1 / 6  # share of golden apples
FOES = 2  # at once
FOE_EVERY = (60, 140)  # steps between two foes
BEAMS = 2  # at once
BEAM_EVERY = (80, 200)  # steps between two beams
FUSE, FIRE = 14, 5  # steps a beam is announced, then fires


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


@dataclass(eq=False)
class Foe:
    n: int  # serial
    at: Cell


@dataclass(eq=False)
class Beam:
    """`cells` in a line, sorted: along a row if `across`, else down a
    column. Announced for `fuse` steps, then firing for `left`."""

    n: int  # serial
    cells: tuple[Cell, ...]
    across: bool
    fuse: int = FUSE
    left: int = FIRE

    @property
    def firing(self) -> bool:
        return self.fuse <= 0


class Game:
    """The snake, the apple and the rules. No drawing here.

    `golden`: golden apples; `enemies`: foes; `lasers`: beams. Each
    step leaves in `events` what happened: `("eat", cell, golden)`,
    `("lost", cells)` (a tail gone), `("pop", cell)` (a foe gone),
    `("zap", beam)` (a beam fires)."""

    def __init__(
        self,
        rng: random.Random,
        *,
        golden: bool = False,
        enemies: bool = False,
        lasers: bool = False,
    ) -> None:
        self.rng = rng
        self.body: deque[Cell] = deque()  # head first
        self.apple: Cell | None = None
        self.grow = 0
        self.eaten = 0
        self.fancy, self.enemies, self.lasers = golden, enemies, lasers
        self.golden = False  # the apple in play is
        self.lumps: list[int] = []  # apples on their way down the body
        self.foes: list[Foe] = []
        self.beams: list[Beam] = []
        self.events: list[tuple[Any, ...]] = []
        self.steps = 0
        self.serial = 0
        self.foe_due = rng.randint(20, 50) if enemies else 0
        self.beam_due = rng.randint(20, 50) if lasers else 0

    def free_cell(self, box: Box, walls: set[Cell]) -> Cell | None:
        x0, y0, x1, y1 = box
        for _ in range(200):
            cell = (self.rng.randrange(x0, x1), self.rng.randrange(y0, y1))
            if cell not in walls and cell not in self.body:
                return cell
        return None

    def step(self, box: Box, walls: set[Cell]) -> None:
        """One move inside `box` (x0, y0, x1, y1: the view, frame cells)."""
        x0, y0, x1, y1 = box

        def inside(c: Cell) -> bool:
            return x0 <= c[0] < x1 and y0 <= c[1] < y1

        self.events = []
        self.steps += 1
        # Scrolled out of sight, or text written on it: gone.
        self.foes = [f for f in self.foes if inside(f.at) and f.at not in walls]
        self.beams = [
            b for b in self.beams if all(inside(c) and c not in walls for c in b.cells)
        ]
        self._crawl(box, walls, inside)
        if self.enemies:
            self._hunt(box, walls, inside)
        if self.lasers:
            self._burn(box, walls, inside)
        self.lumps = [i + 2 for i in self.lumps if i + 2 < len(self.body)]

    def _plant(self, box: Box, taken: set[Cell]) -> None:
        self.apple = self.free_cell(box, taken)
        self.golden = self.fancy and self.rng.random() < GOLDEN

    def _cut(self, i: int) -> None:
        """Lose the body from segment `i` on."""
        cells = list(self.body)
        if cells[i:]:
            self.events.append(("lost", cells[i:]))
        self.body = deque(cells[:i])

    def _crawl(
        self, box: Box, walls: set[Cell], inside: Callable[[Cell], bool]
    ) -> None:
        """The snake's move."""
        foes = {f.at for f in self.foes}
        warned = {c for b in self.beams for c in b.cells}
        firing = {c for b in self.beams if b.firing for c in b.cells}
        taken = walls | foes | warned if foes or warned else walls

        def clear(c: Cell) -> bool:
            return inside(c) and c not in walls and c not in self.body and c not in foes

        def free(c: Cell) -> bool:
            return clear(c) and c not in warned

        def bold(c: Cell) -> bool:
            return clear(c) and c not in firing

        if not self.body or not inside(self.body[0]) or self.body[0] in walls:
            # Start, or scrolled out of sight, or text written on its head.
            start = self.free_cell(box, taken)
            self.body = deque([start] if start else [])
            self.grow = 3
        if self.apple is None or not inside(self.apple) or self.apple in walls:
            self._plant(box, taken)
        if not self.body:
            return
        head = self.body[0]
        around = [(head[0] + dx, head[1] + dy) for dx, dy in STEPS]
        to = None
        # Through an announced beam only when there is no other way.
        for ok in (free, bold) if warned else (free,):
            to = route(head, self.apple, ok) if self.apple else None
            if to is None:
                # No way to the apple: any free neighbour, to stay alive.
                near = [c for c in around if ok(c)]
                to = self.rng.choice(near) if near else None
            if to is not None:
                break
        if to is None or to in firing:
            # Cornered: shed the tail and try again from the head.
            self._cut(1)
            self.grow = 3
            return
        self.body.appendleft(to)
        if to == self.apple:
            self.events.append(("eat", to, self.golden))
            self.grow += 5 if self.golden else 2
            self.apple, self.eaten = None, self.eaten + 1
            self.lumps.append(-2)
        if self.grow:
            self.grow -= 1
        else:
            self.body.pop()

    def _hunt(self, box: Box, walls: set[Cell], inside: Callable[[Cell], bool]) -> None:
        """A foe comes; foes move every other step; one that reaches the
        body bites it off there and is gone."""
        rng = self.rng
        if len(self.foes) < FOES and self.steps >= self.foe_due:
            self.foe_due = self.steps + rng.randint(*FOE_EVERY)
            taken = walls | {f.at for f in self.foes} | {self.apple}
            at = self.free_cell(box, taken)
            head = self.body[0] if self.body else None
            if at is not None and (
                head is None or abs(at[0] - head[0]) + abs(at[1] - head[1]) >= 8
            ):
                self.serial += 1
                self.foes.append(Foe(self.serial, at))
        if self.steps % 2:
            return
        for foe in list(self.foes):
            taken = {f.at for f in self.foes if f is not foe} | {self.apple}
            taken.update(self.body)

            def free(c: Cell, taken: set = taken) -> bool:
                return inside(c) and c not in walls and c not in taken

            goal = self.body[len(self.body) // 2] if len(self.body) >= 4 else None
            to = route(foe.at, goal, free) if goal else None
            if to is None:
                x, y = foe.at
                near = [c for c in ((x + dx, y + dy) for dx, dy in STEPS) if free(c)]
                to = rng.choice(near) if near else foe.at
            if to == goal:
                self._cut(self.body.index(to))
                self.foes.remove(foe)
                self.events.append(("pop", to))
            else:
                foe.at = to

    def _burn(self, box: Box, walls: set[Cell], inside: Callable[[Cell], bool]) -> None:
        """Beams age, fire and burn; a new one is aimed."""
        for beam in self.beams:
            if beam.fuse > 0:
                beam.fuse -= 1
                if beam.fuse == 0:
                    self.events.append(("zap", beam))
            else:
                beam.left -= 1
        self.beams = [b for b in self.beams if b.left > 0]
        hot = {c for b in self.beams if b.firing for c in b.cells}
        hit = [i for i, c in enumerate(self.body) if c in hot]
        if hit:
            self._cut(hit[0])
        for foe in [f for f in self.foes if f.at in hot]:
            self.foes.remove(foe)
            self.events.append(("pop", foe.at))
        if len(self.beams) < BEAMS and self.steps >= self.beam_due:
            self.beam_due = self.steps + self.rng.randint(*BEAM_EVERY)
            beam = self._aim(box, walls, inside)
            if beam is not None:
                self.beams.append(beam)

    def _aim(
        self, box: Box, walls: set[Cell], inside: Callable[[Cell], bool]
    ) -> Beam | None:
        """A beam through a segment of the snake, or through a free
        cell: from wall to wall, 5 cells or more."""
        rng = self.rng
        if self.body and rng.random() < 0.5:
            seed = rng.choice(tuple(self.body))
        else:
            seed = self.free_cell(box, walls)
        if seed is None:
            return None
        for across in rng.sample((True, False), 2):
            dx, dy = (1, 0) if across else (0, 1)
            cells = [seed]
            for sign in (1, -1):
                c = (seed[0] + sign * dx, seed[1] + sign * dy)
                while inside(c) and c not in walls:
                    cells.append(c)
                    c = (c[0] + sign * dx, c[1] + sign * dy)
            if len(cells) >= 5:
                self.serial += 1
                return Beam(self.serial, tuple(sorted(cells)), across)
        return None

    def picture(
        self, fancy: bool = False
    ) -> tuple[Cell, list[list[dict[str, object]]]]:
        """The snake as a sprite: its top-left cell and rows of runs.
        `fancy`: shaded from head to tail, a lump where an apple is."""
        xs = [c[0] for c in self.body]
        ys = [c[1] for c in self.body]
        left, top = min(xs), min(ys)
        rows: list[list[dict[str, object]]] = [
            [{"t": " "} for _ in range(max(xs) - left + 1)]
            for _ in range(max(ys) - top + 1)
        ]
        shades, n = _arcade.BODY, len(self.body)
        for i, (x, y) in enumerate(self.body):
            if i == 0:
                run = {"t": "@", "fg": "bright-green", "bold": True}
            elif not fancy:
                run = {"t": "o", "fg": "green"}
            elif i in self.lumps or i - 1 in self.lumps:
                run = {"t": "O", "fg": _arcade.LUMP, "bold": True}
            else:
                run = {"t": "o", "fg": shades[i * len(shades) // n]}
            rows[y - top][x - left] = run
        return (left, top), rows


def view(frame: Frame) -> Box:
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    game = Game(
        random.Random(args.seed),
        golden=args.effects,
        enemies=args.enemies,
        lasers=args.lasers,
    )
    frame = _cli.pick(pilot, args.frame)
    # Its own dice: the particles do not change the game of a seed.
    stage = _arcade.Stage(pilot, frame, random.Random(args.seed), args.effects)
    snake: Sprite | None = None
    apple: Sprite | None = None
    shown_apple = None
    # The map is read when the driver says the text changed or the view
    # moved (topic `text`), not at each move.
    sight = _cli.Sight(pilot, frame)
    walls: set[Cell] = set()
    for _ in _cli.ticks(1.0 / args.speed, seconds):
        if not sight.look():
            continue
        box = view(frame)
        if sight.changed:
            walls = walls_of(sight.rows, box[0], box[1])
        game.step(box, walls)
        with sight.drawing(), pilot.batch():
            stage.show(game)
            if not game.body:
                # A full screen: wait for room. Or burnt: back next step.
                if snake is not None:
                    snake.remove()
                    snake = None
                continue
            at, cells = game.picture(args.effects)
            anchor = {"at": list(at), "space": "frame"}
            if snake is None:
                snake = pilot.draw(cells=cells, frame=frame.id, at=at, space="frame")
            else:
                snake.update(anchor=anchor, content={"cells": cells})
            now = (game.apple, game.golden)
            if now != shown_apple and game.apple is not None:
                look = _arcade.apple(game.golden) if args.effects else {}
                if apple is None:
                    apple = pilot.draw(
                        None if look else ["●"],
                        frame=frame.id,
                        at=game.apple,
                        space="frame",
                        **(look or {"fg": "bright-red"}),
                    )
                else:
                    apple.move(*game.apple, space="frame")
                    if look and shown_apple[1] != game.golden:
                        apple.set(**look)
                shown_apple = now


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "snake", "A snake that plays alone between the words.", frame="largest"
    )
    out.add_argument(
        "--speed", type=float, default=12.0, help="moves per second [default: 12]"
    )
    out.add_argument("--seed", type=int, help="same seed, same game")
    out.add_argument(
        "-e", "--effects", action="store_true", help="particles, golden apples, shades"
    )
    out.add_argument(
        "-x", "--enemies", action="store_true", help="foes that bite the tail off"
    )
    out.add_argument(
        "-l", "--lasers", action="store_true", help="beams that burn what they cross"
    )
    out.add_argument("-a", "--all", action="store_true", help="-e -x -l")
    args = _cli.parse(out, argv)
    if args.all:
        args.effects = args.enemies = args.lasers = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
