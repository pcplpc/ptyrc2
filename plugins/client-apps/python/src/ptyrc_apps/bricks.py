# SPDX-License-Identifier: GPL-3.0-or-later
"""Bricks: a brick breaker that plays alone, around the words.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app bricks
    ptyrc-app bricks -r -b           # rules and bonuses
    ptyrc-app bricks -a              # the arcade
    ptyrc-app bricks --grab          # the arrows are yours

What it shows:

- The screen read as a map: glyphs are walls, the ball bounces off
  them, off the sides of the view and off the bricks. The bottom
  of the view is open; with two rackets (`-m`), the top too.
- A racket that moves in two dimensions and is hit on either side. No
  one plays: it foresees the ball, gets in its way before it leaves
  the view, and of the places it reaches in time takes the one whose
  shot breaks the most bricks. With no ball to stop it hits one on
  purpose when that breaks more bricks than not.
- Text does not stop a racket: sprites are drawn behind the text, a
  racket under it is hidden, and the ball meets the text, not the
  racket. A racket keeps off the text when it has the choice.
- Terrain in frame cells: groups of bricks, laid one at a time all
  over the view, far from each other, where a ball gets: between the
  lines, after the short ones, in the blanks. Near the top of the
  view when it has room there, near the middle with two rackets
  (`-m`); elsewhere when it has not. Large groups, a colour a row:
  medium walls of 2 rows by 3 to 5 bricks, a row or a pile where the
  place is tight; now and then a big wall of 3 or 4 rows by 6 to 8.
  Where no large one fits, groups of 2 or 3. A group goes 50 to 100 s
  after it is laid,
  two seconds later for each brick over three, dim for its last second.

The game never ends. The last ball out sets the score to 0; the racket
keeps the bottom, the top sends the ball back. 30 s with
no brick broken: the game starts again, on a new map, the rackets
elsewhere; the scores stay.

More, each on its own flag; `-a` for all (see `_bricks`):

- `-r`: rules. Bricks that take three hits; loose ones that fall when
  touched and kill the racket under them; bumpers that send the ball
  away faster, anywhere; bombs that take their neighbours with them.
- `-b`: bonuses. Two broken bricks in five drop a capsule; the racket
  that touches it gets, three times in five, two balls for each one
  (none is dropped with 4 balls or more in play);
  else a wide racket, a twin for a while, or 5 s of fire: the ball goes through
  the bricks. Two are bad, and no racket goes for them: a small
  racket, a faster ball. A spare ball out costs nothing.
- `-m`: two rackets, two scores. One keeps the top, the other the
  bottom; a brick is worth points to the last that hit the ball.
  Bricks come first; the other's loss only tells two equal shots
  apart. Face to face, one of the two keeps away for 2 to 3 s.
- `-e`: particles, a trail, combos, a best score.

`--grab`, not in `-a`: the four arrows of the host keyboard are taken
before the program and move the racket; with `-m`, the bottom one, the
other plays alone. A press moves it for `HELD` steps, `REACH_X` cells
a step along the row or one row a step: a key held down, repeated by
the terminal, keeps it going; the opposite arrow stops it. The racket
moves as the others do: stopped by bricks, rackets and the sides of
the view, not by text. Twins play alone; no racket of a player keeps
away from a duel. With a player the balls go half as fast; rackets,
capsules and the rest keep their pace. With `-m` the rackets that play
alone take the pace of the player: the mean of the cells a step its
racket moved over the last `PACED` seconds, within `PACES`. ctrl+t
gives the arrows back and ends.

Rules count in steps of the game, not in seconds; `second` is the
steps in a second.
"""

from __future__ import annotations

import argparse
import contextlib
import random
from collections import deque
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from itertools import pairwise
from math import floor
from typing import Any

from ptyrc import Frame, Pilot, PtyrcError

from . import _bricks, _cli
from .snake import walls_of

__all__ = [
    "BONUSES",
    "BW",
    "KINDS",
    "RACKET",
    "SMALL",
    "WIDE",
    "Ball",
    "Brick",
    "Capsule",
    "Faller",
    "Game",
    "Group",
    "Racket",
    "fly",
    "main",
]

Cell = tuple[int, int]
Box = tuple[int, int, int, int]
Solid = Callable[[Cell], bool]
State = tuple[float, float, float, float]  # x, y, vx, vy
STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))

BW = 3  # cells of a brick
SMALL, RACKET, WIDE = 3, 5, 7  # cells of a racket
VY = 0.5  # rows a move
VX = 1.0  # columns a move, at most
PACE = 12  # eighths of a move a ball makes in a step; sixteenths with a player
RALLY = 4  # bricks in a row that speed it up, an eighth each
KICK = 6  # eighths more for a fast one
MOVES = 2  # cells a racket moves in a step
REACH_X = 3  # cells a step the racket of a player moves along its row; one row
HELD = 2  # steps a press of an arrow moves it
PACED = 4  # seconds of the moves of a player the other rackets take their pace from
PACES = (0.5, 3.0)  # cells a step, at least and at most, they then move
ARROWS = {"left": (0, -1), "right": (0, 1), "up": (1, -1), "down": (1, 1)}  # axis, way
BRICKS = 240  # bricks in the view, at most; one a `PLOT` cells, 8 at least
PLOT = 48
FILL = 12  # seconds to lay as many
SLOW = 5  # times slower once the view has `KEPT` of them
KEPT = 0.55
AGE = (50, 100)  # seconds a group lasts
LONG = 2  # seconds more for each brick over three
BIG = ((3, 4), (6, 8))  # rows and columns of a big wall, in bricks
OFTEN = 0.2  # share of the large groups a big wall is tried for
#: Of a medium one, in the order tried: a wall, a block, a row, a pile.
MEDIUM = (((2, 2), (3, 5)), ((2, 2), (3, 3)), ((1, 1), (4, 6)), ((3, 4), (2, 2)))
STYLES = ("full", "full", "full", "checker", "frame", "slope")  # of a wall
FADE = 1  # second it is dim before it goes
CLEAR = 60  # moves of a ball no group is laid in the way of
TRIES = 60  # places drawn for a group
PICK = 10  # of which the farthest from the others is kept
BANDS = 3  # of the view, by rows: a group goes to the nearest to where it is liked
ROOM = 40  # free cells a group must be next to
#: Shapes of a small group: left cells of its bricks.
SHAPES = (
    ((0, 0), (BW, 0)),
    ((0, 0), (BW, 0)),
    ((0, 0), (BW, 0), (2 * BW, 0)),
    ((0, 0), (0, 1)),
    ((0, 0), (BW - 1, 1)),
    ((1, 0), (0, 1), (BW, 1)),
    ((0, 0), (BW, 0), (1, 1)),
)
STALE = 30  # seconds without a brick broken: the game starts again
SERVE = 6  # steps before a new ball
FORESEE = 100  # steps a racket looks ahead
SPARE = 6  # moves a racket likes to have in hand
AIM = 50  # steps of a shot it weighs
FETCH = 48  # moves it goes for a capsule
CHASE = 40  # steps ahead within which it goes for a ball that is safe
THINK = 4  # steps between two looks for such a shot
SHOTS = 60  # shots weighed in a step after which a racket looks at one row
EDGE = 0.5  # bricks a shot must win over no shot
DENT = 0.3  # of a brick: a hit that does not break it
RISK = 0.25  # of a brick: a shot that has to be saved again
LOSS = 20.0  # bricks: a shot that cannot be
GAP = 4  # rows off the ball a racket with nothing to do waits
REACH = 8  # moves it looks around for such a place
AHEAD = 60  # moves of the ball it looks at to wait
HIDE = 0.02  # of a brick: a cell of the racket under text, for a shot
DUEL = 4  # bounces, a team each in turn, that make a duel
SPAN = 2  # seconds they take at most
NEAR = 6  # rows between the two rackets at most
AWAY = (2, 3)  # seconds one of them keeps away
QUIET = 160  # steps without a brick: the ball is nudged; thrice, not alone: gone
POINTS = 10  # a brick
BONUS = 2  # more for each brick before it in a row, five at most
COLORS = 7  # of the bricks: indices in a palette
#: Kinds of bricks with `-r`: lives and share.
KINDS = {
    "plain": (1, 0.66),
    "hard": (3, 0.14),
    "loose": (1, 0.08),
    "bumper": (2, 0.05),
    "bomb": (1, 0.07),
}
#: Capsules with `-b`: shares.
BONUSES = {"multi": 12, "wide": 2, "twin": 2, "fire": 2, "small": 1, "fast": 1}
BAD = ("small", "fast")  # no racket goes for these
DROP = 0.4  # share of the broken bricks that drop a capsule
CROWD = 4  # balls from which no `multi` is dropped
BALLS = 5  # at once
SPLIT = (0.3, 0.8)  # columns a move between a ball and the one that leaves it
DEAD = 60  # steps a killed racket is away
WARN = 6  # rows under a falling brick a racket keeps out of
LAST = {"wide": 320, "small": 240, "twin": 200, "fast": 160, "kick": 24}  # steps


def fly(x: float, y: float, vx: float, vy: float, solid: Solid) -> tuple[Any, ...]:
    """One move of a ball: along x, then along y; an axis that would
    enter a solid cell is reversed instead. Returns `(x, y, vx, vy,
    hits)`, `hits` being `(cell, flat)`: `flat` when hit from above or
    below."""
    hits = []
    cx, cy = floor(x), floor(y)
    nx = x + vx
    if floor(nx) != cx and solid((floor(nx), cy)):
        hits.append(((floor(nx), cy), False))
        vx, nx = -vx, x
    ny = y + vy
    if floor(ny) != cy and solid((floor(nx), floor(ny))):
        hits.append(((floor(nx), floor(ny)), True))
        vy, ny = -vy, y
    return nx, ny, vx, vy, hits


@dataclass(eq=False)
class Brick:
    n: int  # serial
    x: int  # its left cell
    y: int
    kind: str = "plain"
    lives: int = 1
    color: int = 0
    due: int = 0  # step it goes with its group; 0: never

    @property
    def cells(self) -> list[Cell]:
        return [(self.x + i, self.y) for i in range(BW)]


@dataclass(eq=False)
class Group:
    """Bricks laid together: they go together at step `due`. Medium
    and big ones are large: 4 bricks or more."""

    bricks: list[Brick]
    at: tuple[float, float]  # its middle
    due: int
    size: int = 0  # 0 small, 1 medium, 2 big
    dim: bool = False  # about to go

    @property
    def large(self) -> bool:
        return self.size > 0


@dataclass(eq=False)
class Ball:
    n: int
    x: float
    y: float
    vx: float
    vy: float
    owner: int = 0  # team of the last racket hit
    fire: int = 0  # steps left
    fast: int = 0
    quiet: int = 0  # steps since a brick
    combo: int = 0  # bricks broken since a racket
    phase: int = 0  # eighths of a move owed

    @property
    def at(self) -> Cell:
        return (floor(self.x), floor(self.y))


@dataclass(eq=False)
class Racket:
    n: int
    team: int
    x: int = 0  # its left cell
    y: int = 0
    w: int = RACKET
    span: int = 0  # steps left to a wide or a small one
    dead: int = 1  # steps away; 0: in play
    left: int | None = None  # steps left to a twin
    plan: _Plan | None = None  # the shot it is going for
    think: int = 0  # step of its next look for a shot
    guard: int = 1  # side it waits on: -1 top, 1 bottom
    away: int = 0  # step until which it keeps out of a duel

    @property
    def cells(self) -> list[Cell]:
        return [(self.x + i, self.y) for i in range(self.w)]


@dataclass(eq=False)
class Faller:
    """A loose brick on its way down."""

    n: int
    x: int
    y: int
    color: int = 0

    cells = Brick.cells


@dataclass(eq=False)
class Capsule:
    n: int
    x: int
    y: int
    kind: str
    rest: int = 0  # steps spent on the ground


@dataclass(eq=False)
class _Plan:
    """A shot a racket goes for."""

    ball: int  # its serial
    place: Cell  # where the racket is hit
    step: int  # when
    x: float  # of the ball then
    w: int  # of the racket
    way: list[Cell]  # left to go


@dataclass(eq=False)
class _Flight:
    """A ball foreseen with no racket in its way: after each move, the
    step counted from this one, `x, y, vx, vy`, the gain so far, then
    `phase, fast, fire`; the bricks it changes, `(index, brick, lives)`;
    the index of its last bounce; the side it leaves by (-1 top, 1
    bottom) or None; the indices before a move that enters a row."""

    ball: Ball
    path: list[tuple[Any, ...]]
    marks: list[tuple[int, Brick, int]]
    last: int
    side: int | None
    entries: list[int] = field(default_factory=list)


class Game:
    """Bricks, balls, rackets and the rules. No drawing here.

    `rules`: kinds of bricks; `bonuses`: capsules; `multiplayer`: two
    teams; `second`: steps in a second; `player`: the team whose racket
    is not steered but moved by `press`; the balls then go half as fast. Same `rng`, same game: no clock.
    Each step leaves in `events` what happened, a cell or bricks second:

    - `("group", bricks)`: laid; `("fade", bricks)`: about to go;
      `("gone", bricks)`: gone, by age or by a reset.
    - `("break", cell, brick, team)`, `("hit", cell, brick)`,
      `("bump", cell)`, `("boom", cell)`.
    - `("combo", cell, n)`: a ball broke its `n`-th brick in a row,
      `n >= 2`, with no racket in between; one a strike.
    - `("crash", cell)`: a faller on the ground;
      `("kill", cell, racket)`.
    - `("serve", cell)`; `("bounce", cell, racket)`: a ball off a
      racket, the cell hit.
    - `("out", cell, team)`: a ball out, others in play;
      `("lost", cell, team)`: the last one, the team's score at 0.
    - `("catch", cell, kind, team)`.
    - `("retreat", cell, racket)`: it keeps away from a duel until step
      `racket.away`.
    - `("reset", cell)`: the game starts again, the middle of the view.
    """

    def __init__(
        self,
        rng: random.Random,
        *,
        rules: bool = False,
        bonuses: bool = False,
        multiplayer: bool = False,
        second: int = 16,
        player: int | None = None,
    ) -> None:
        self.rng = rng
        self.player = player  # team whose racket is moved by `press`
        self.gear = 3 if player is None else 4  # a move is 2 ** gear of `PACE`
        self.held = [(0, 0), (0, 0)]  # along x, along y: way, steps left
        self.pace = float(MOVES)  # cells a step a racket that plays alone moves
        self.reach = MOVES  # of which in this step
        self.credit = 0.0  # cells it has yet to move
        self.moved: deque[int] = deque(maxlen=PACED * second)  # player: cells a step
        self.rules, self.bonuses, self.teams = rules, bonuses, 1 + multiplayer
        self.second = second
        self.solid: dict[Cell, Brick] = {}  # every cell of every brick
        self.rows: dict[int, list[Brick]] = {}  # bricks of a row
        self.balls: list[Ball] = []
        self.rackets = [Racket(t + 1, t) for t in range(self.teams)]
        self.fallers: list[Faller] = []
        self.capsules: list[Capsule] = []
        self.score = [0] * self.teams
        self.best = [0] * self.teams
        self.events: list[tuple[Any, ...]] = []
        self.steps = 0
        self.serial = self.teams
        self.groups: list[Group] = []
        self.laid = [0, 0]  # bricks laid so far: in small groups, in large ones
        self.spawn_due = 0
        self.last = 0  # step of the last brick broken, or of the start
        self.home = (0.5, 0.5)  # of a racket, in shares of its part of the view
        self.lure: list[tuple[float, int]] = []  # the bricks in view
        self.serve_due = SERVE
        self.server = rng.randrange(self.teams)
        self.version = 0  # of the terrain
        self.shots = 0  # weighed in this step
        self.fits: dict[int, Any] = {}  # `_free`, by width
        self.spans: dict[int, tuple[Box, set[Cell]]] = {}  # of `_room`, by width
        self.rally: deque[tuple[int, Racket, int]] = deque(maxlen=DUEL)  # bounces

    # -- the terrain -------------------------------------------------------

    def bricks(self, box: Box) -> list[Brick]:
        """Those of the rows of `box`."""
        return [b for y in range(box[1], box[3]) for b in self.rows.get(y, ())]

    def _number(self) -> int:
        self.serial += 1
        return self.serial

    def _lay(self, brick: Brick) -> None:
        self.version += 1
        self.rows.setdefault(brick.y, []).append(brick)
        for cell in brick.cells:
            self.solid[cell] = brick

    def _lift(self, brick: Brick) -> bool:
        """Take a brick off the terrain; False when it was not on it."""
        if self.solid.get((brick.x, brick.y)) is not brick:
            return False
        self.version += 1
        self.rows[brick.y].remove(brick)
        for cell in brick.cells:
            del self.solid[cell]
        return True

    def _group(self, box: Box, walls: set[Cell], most: int = 32) -> list[Brick]:
        """Lay a group on free cells of the view: a medium one, the
        first of `MEDIUM` that fits; once in five a big wall is tried
        first, of `most` bricks at most. Where no large one fits, a
        small group of 2 or 3. Not on a racket or the two rows about
        it, nor where a ball is about to go."""
        rng = self.rng
        x0, y0, x1, y1 = box
        if x1 - x0 < 3 * BW or y1 - y0 < 4:
            return []
        solid = self.solid
        keep = {
            (x, y + dy)
            for r in self.rackets
            if not r.dead
            for x, y in r.cells
            for dy in range(-2, 3)
        }
        for ball in self.balls:
            for _, x, y, *_ in self._foresee(ball, box, walls).path[:CLEAR]:
                keep |= {(floor(x) + dx, floor(y)) for dx in (-1, 0, 1)}

        def free(c: Cell) -> bool:
            return (
                x0 <= c[0] < x1
                and y0 <= c[1] < y1
                and c not in walls
                and c not in solid
            )

        found, size = None, 0
        big = rng.random() < OFTEN
        for n, sizes in enumerate((BIG, *MEDIUM) if big else MEDIUM):
            (r0, r1), (c0, c1) = sizes
            rows = rng.randint(r0, min(r1, max(r0, y1 - y0 - 4)))
            cols = rng.randint(c0, max(c0, min(c1, most // rows)))
            style = rng.choice(STYLES) if rows > 2 and cols > 3 else "full"
            shape = self._wall(rows, cols, style, rng.random() < 0.5)
            if rows * c0 <= most:
                found = self._spot([shape], box, free, keep, True)
            if found is not None:
                size = 2 if big and not n else 1
                break
        if found is None:
            found = self._spot(SHAPES, box, free, keep, False)
        if found is None:
            return []
        shape, x, y, at = found
        tint = rng.randrange(COLORS)
        out = []
        for dx, dy in shape:
            kind, lives = "plain", 1
            if self.rules:
                kind = rng.choices(list(KINDS), [share for _, share in KINDS.values()])[
                    0
                ]
                lives = KINDS[kind][0]
            brick = Brick(
                self._number(), x + dx, y + dy, kind, lives, (tint + dy) % COLORS
            )
            self._lay(brick)
            out.append(brick)
        life = rng.uniform(*AGE) + LONG * max(0, len(out) - 3)
        due = self.steps + round(life * self.second)
        for brick in out:
            brick.due = due
        self.laid[size > 0] += len(out)
        self.groups.append(Group(out, at, due, size))
        self.events.append(("group", out))
        return out

    @staticmethod
    def _wall(rows: int, cols: int, style: str, up: bool) -> list[Cell]:
        """Left cells of the bricks of a wall of `rows` by `cols`: full,
        or but for its first row (the last when `up`) a checker, a
        frame or a slope."""
        out = []
        for j in range(rows):
            d = rows - 1 - j if up else j  # rows from the whole one
            for i in range(cols):
                if d and (
                    (style == "checker" and (i + d) % 2)
                    or (style == "frame" and 0 < i < cols - 1 and d < rows - 1)
                    or (style == "slope" and not d <= i < cols - d)
                ):
                    continue
                out.append((i * BW, j))
        return out

    def _spot(
        self,
        shapes: Sequence[Sequence[Cell]],
        box: Box,
        free: Solid,
        keep: set[Cell],
        large: bool,
    ) -> tuple[Sequence[Cell], int, int, tuple[float, float]] | None:
        """Where a group goes: of `TRIES` places drawn for a shape of
        `shapes`, of the first `PICK` that are free and that a ball can
        get to (`_open`): those of the band of rows nearest to the top
        of the view, to its middle with two teams, the view cut in
        `BANDS`; of these the farthest from the other groups, a row
        counting for two columns. A large one touches no brick and,
        of two rows or more, has as many free cells next to it as its
        longest row has cells."""
        rng = self.rng
        x0, y0, x1, y1 = box
        solid = self.solid
        others = [g.at for g in self.groups]
        best = None
        found = 0
        for _ in range(TRIES):
            shape = rng.choice(shapes)
            x, y = rng.randrange(x0, x1), rng.randrange(y0 + 1, y1 - 1)
            cells = [(x + dx + i, y + dy) for dx, dy in shape for i in range(BW)]
            if not all(free(c) and c not in keep for c in cells):
                continue
            need = 2
            if large:
                ay, by = cells[0][1], cells[-1][1]
                xs = [c[0] for c in cells]
                ax, bx = min(xs), max(xs)
                if any(
                    (cx, cy) in solid
                    for cy in range(ay - 1, by + 2)
                    for cx in range(ax - 2, bx + 3)
                ):
                    continue
                need = bx - ax + 1 if by > ay else 2
            if not self._open(cells, free, need):
                continue
            mx = sum(c[0] for c in cells) / len(cells) + 0.5
            my = sum(c[1] for c in cells) / len(cells) + 0.5
            far = min(
                ((mx - gx) ** 2 + (2 * (my - gy)) ** 2 for gx, gy in others),
                default=0.0,
            )
            if self.teams > 1:
                off = abs(2 * my - y0 - y1) / (y1 - y0)
            else:
                off = (my - y0) / (y1 - y0)
            rank = (-min(BANDS - 1, int(BANDS * off)), far)
            if best is None or rank > best[0]:
                best = (rank, shape, x, y, (mx, my))
            found += 1
            if found >= PICK:
                break
        return best[1:] if best else None

    @staticmethod
    def _open(cells: list[Cell], free: Solid, need: int = 2) -> bool:
        """Whether a ball can get to `cells`: `need` free cells or more
        next to them, that lead to `ROOM` free cells more."""
        group = set(cells)
        seen = {
            n
            for x, y in cells
            for dx, dy in STEPS
            if (n := (x + dx, y + dy)) not in group and free(n)
        }
        if len(seen) < need:
            return False
        room = need + ROOM
        queue = deque(sorted(seen))
        while queue and len(seen) < room:
            x, y = queue.popleft()
            for dx, dy in STEPS:
                n = (x + dx, y + dy)
                if n not in seen and n not in group and free(n):
                    seen.add(n)
                    queue.append(n)
        return len(seen) >= room

    def _age(self) -> None:
        """Groups go when due, dim `FADE` seconds before."""
        for group in list(self.groups):
            left = [b for b in group.bricks if self.solid.get((b.x, b.y)) is b]
            group.bricks = left
            if not left:
                self.groups.remove(group)
            elif self.steps >= group.due:
                for brick in left:
                    self._lift(brick)
                self.groups.remove(group)
                self.events.append(("gone", left))
            elif not group.dim and self.steps >= group.due - FADE * self.second:
                group.dim = True
                self.events.append(("fade", left))

    def _spawn(self, box: Box, walls: set[Cell]) -> None:
        """A group at a time, while the view has fewer bricks than it
        takes; `FILL` seconds to lay as many, a group of more bricks
        being followed by a longer wait; `SLOW` times slower once the
        view has `KEPT` of them."""
        if self.steps < self.spawn_due:
            return
        x0, y0, x1, y1 = box
        most = max(8, min(BRICKS, (x1 - x0) * (y1 - y0) // PLOT))
        here = len(self.bricks(box))
        if here >= most:
            return
        laid = len(self._group(box, walls, most)) or 2
        gap = laid * FILL * self.second / most
        slow = SLOW if here >= KEPT * most else 1
        self.spawn_due = self.steps + max(1, round(gap * slow))

    def _reset(self, box: Box) -> None:
        """Start again: no brick, ball, capsule, falling brick or twin;
        the rackets come back about a new place. The scores stay."""
        x0, y0, x1, y1 = box
        rng = self.rng
        gone = self.bricks(box)
        self.solid.clear()
        self.rows.clear()
        self.groups.clear()
        self.version += 1
        self.balls, self.capsules, self.fallers = [], [], []
        self.rackets = [r for r in self.rackets if r.left is None]
        for racket in self.rackets:
            racket.dead, racket.plan = 1, None
        self.home = (rng.uniform(0.1, 0.9), rng.uniform(0.2, 0.8))
        self.last = self.spawn_due = self.steps
        self.serve_due = self.steps + SERVE
        if gone:
            self.events.append(("gone", gone))
        self.events.append(("reset", ((x0 + x1) // 2, (y0 + y1) // 2)))

    # -- a step ------------------------------------------------------------

    def step(self, box: Box, walls: set[Cell]) -> None:
        """One step inside `box` (x0, y0, x1, y1: the view, frame cells)."""
        x0, y0, x1, y1 = box

        def inside(c: Cell) -> bool:
            return x0 <= c[0] < x1 and y0 <= c[1] < y1

        self.events = []
        self.steps += 1
        # Text written on a brick: gone. Scrolled out of sight: the
        # bricks stay, what moves is dropped.
        for brick in [b for b in self.bricks(box) if walls.intersection(b.cells)]:
            self._lift(brick)
        self.balls = [b for b in self.balls if inside(b.at) and b.at not in walls]
        self.fallers = [f for f in self.fallers if inside((f.x, f.y))]
        self.capsules = [
            c for c in self.capsules if inside((c.x, c.y)) and (c.x, c.y) not in walls
        ]
        for racket in self.rackets:
            if not racket.dead and not all(
                inside(c) and c not in self.solid for c in racket.cells
            ):
                racket.dead = 1
        self._age()
        if self.steps - self.last >= STALE * self.second:
            self._reset(box)
        self._spawn(box, walls)
        self._wake(box, walls)
        self._play(box)
        self._steer(box, walls)
        self._serve(box, walls)
        for ball in list(self.balls):
            self._roll(ball, box, walls)
        if self.rules:
            self._fall(box, walls)
        if self.bonuses:
            self._drift(box, walls)
        self.best = [max(pair) for pair in zip(self.best, self.score, strict=True)]

    def _cells(self, walls: set[Cell]) -> dict[Cell, Racket]:
        """The cells of the rackets something can touch: not under text."""
        return {
            c: r for r in self.rackets if not r.dead for c in r.cells if c not in walls
        }

    def _blocked(self, but: Racket | None = None) -> set[Cell]:
        """Cells a racket cannot move to: the other rackets, and two
        cells each side of a ball, not to push it with an end."""
        out = {(b.at[0] + dx, b.at[1]) for b in self.balls for dx in range(-2, 3)}
        for other in self.rackets:
            if other is not but and not other.dead:
                out.update(other.cells)
        return out

    def _room(self, box: Box, w: int, taken: set[Cell]) -> set[Cell]:
        """Left cells of the places where a racket of `w` cells fits:
        in the view, off the bricks and `taken`. Text is no obstacle."""
        x0, y0, x1, y1 = box
        kept = self.spans.get(w)
        if kept is None or kept[0] != box:
            # The places of the view, with nothing on it.
            cells = {(x, y) for y in range(y0, y1) for x in range(x0, x1 - w + 1)}
            kept = self.spans[w] = (box, cells)
        rows = self.rows
        shut = {
            (x - i, y)
            for y in range(y0, y1)
            for brick in rows.get(y, ())
            for x in range(brick.x, brick.x + BW)
            for i in range(w)
        }
        shut.update((x - i, y) for x, y in taken for i in range(w))
        return kept[1] - shut

    def _wake(self, box: Box, walls: set[Cell]) -> None:
        """Timers of the rackets; one away comes back near its home,
        `home` in the view or in the team's half of it, off the text if
        it can."""
        x0, y0, x1, y1 = box
        for racket in list(self.rackets):
            if racket.left is not None:
                racket.left -= 1
                if racket.left <= 0:
                    self.rackets.remove(racket)
                    continue
            if racket.span:
                racket.span -= 1
                if not racket.span and not racket.dead:
                    # A small one that has no room to grow stays small.
                    racket.span = 0 if self._size(racket, RACKET, box) else 1
            if not racket.dead:
                continue
            racket.dead -= 1
            if racket.dead:
                continue
            racket.w, racket.span, racket.plan = RACKET, 0, None
            room = self._room(box, racket.w, self._blocked(racket))
            fx, fy = self.home
            hx = x0 + int((x1 - x0 - racket.w) * fx)
            hy = y0 + int((y1 - y0) * (racket.team + fy) / self.teams)
            if self.teams == 1:  # the bottom is its side: in the lower half
                hy = y0 + int((y1 - y0) * (1 + fy) / 2)
            if racket.left is not None:  # a twin: off its team's racket
                up = self.teams == 1 or not racket.n % 2
                hy += (y1 - y0) // 4 * (-1 if up else 1)
            if not room:
                racket.dead = 4
                continue

            # About its home first: text counts there.
            near = [c for c in room if abs(c[0] - hx) <= 24 and abs(c[1] - hy) <= 6]
            racket.x, racket.y = min(
                near or room, key=lambda c: (self._far(c, (hx, hy), racket.w, walls), c)
            )

    @staticmethod
    def _far(c: Cell, home: Cell, w: int, walls: set[Cell]) -> int:
        """How far from `home` a racket at `c` is; a cell under text
        counts for twelve columns."""
        hidden = sum((c[0] + i, c[1]) in walls for i in range(w))
        return 4 * abs(c[1] - home[1]) + abs(c[0] - home[0]) + 12 * hidden

    def _serve(self, box: Box, walls: set[Cell]) -> None:
        """A ball when there is none: from a side of a racket, the one
        with the most rows ahead, of the team that lost the last one
        first; from the nearest free cell
        when the racket is between two lines or under one."""
        if self.balls:
            self.serve_due = self.steps + SERVE
            return
        rackets = [r for r in self.rackets if not r.dead]
        if self.steps < self.serve_due or not rackets:
            return
        x0, y0, x1, y1 = box
        taken = self._blocked()

        def free(c: Cell) -> bool:
            return (
                x0 <= c[0] < x1
                and y0 < c[1] < y1 - 1
                and not (c in walls or c in self.solid or c in taken)
            )

        rackets.sort(key=lambda r: (r.team != self.server, r.n))
        mid = rackets[0].w // 2
        for racket in rackets:
            # Towards the far side: time to get in its way. With one
            # team, up: the top sends it back.
            far = 1 if self.teams > 1 and 2 * racket.y < y0 + y1 else -1
            for dy in (far, -far):
                for dx in sorted(range(racket.w), key=lambda i: abs(i - mid)):
                    if free((racket.x + dx, racket.y + dy)):
                        self._ball(racket.x + dx, racket.y + dy, dy, racket.team)
                        return
        racket = rackets[0]
        cells = [(x, y) for y in range(y0, y1) for x in range(x0, x1) if free((x, y))]
        if cells:
            x, y = min(
                cells,
                key=lambda c: (
                    2 * abs(c[1] - racket.y) + abs(c[0] - racket.x - mid),
                    c,
                ),
            )
            self._ball(x, y, 1 if y > racket.y else -1, racket.team)

    def _ball(self, x: int, y: int, dy: int, team: int) -> None:
        rng = self.rng
        vx = rng.choice((-1, 1)) * rng.uniform(0.2, VX / 2)
        ball = Ball(self._number(), x + 0.5, y + 0.5, vx, dy * VY, team)
        self.balls.append(ball)
        self.events.append(("serve", ball.at))

    # -- the balls ---------------------------------------------------------

    def _terrain(self, box: Box, walls: set[Cell], fire: bool = False) -> Solid:
        """What a ball bounces off, rackets apart: text, the sides of
        the view, its top with one team, bricks unless on `fire`."""
        x0, y0, x1, y1 = box
        solid = self.solid

        shut = self.teams == 1

        def hard(c: Cell) -> bool:
            if not y0 <= c[1] < y1:
                return shut and c[1] < y0
            return c in walls or not x0 <= c[0] < x1 or (not fire and c in solid)

        return hard

    def _roll(self, ball: Ball, box: Box, walls: set[Cell]) -> None:
        y0, y1 = box[1], box[3]
        terrain = self._terrain(box, walls, bool(ball.fire))
        rackets = self._cells(walls)
        # The pace is counted in eighths, in sixteenths with a player:
        # what is left is owed.
        ball.phase += PACE + min(ball.combo, RALLY) + (KICK if ball.fast else 0)
        moves, ball.phase = ball.phase >> self.gear, ball.phase & ((1 << self.gear) - 1)
        for _ in range(moves):
            ball.x, ball.y, ball.vx, ball.vy, hits = fly(
                ball.x, ball.y, ball.vx, ball.vy, lambda c: terrain(c) or c in rackets
            )
            for cell, flat in hits:
                racket = rackets.get(cell)
                if racket is not None:
                    ball.owner, ball.combo = racket.team, 0
                    if flat:  # the angle is the racket's
                        half = racket.w / 2
                        off = (ball.x - racket.x - half) / half
                        ball.vx = max(-VX, min(VX, off * VX))
                    self.events.append(("bounce", cell, racket))
                    self._duel(racket, ball)
                elif cell in self.solid:
                    self._strike(self.solid[cell], ball, cell)
            if ball.fire and ball.at in self.solid:
                self._smash(self.solid[ball.at], ball, ball.at)
            if not y0 <= ball.at[1] < y1:
                self._out(ball, box)
                return
        ball.fire, ball.fast = max(0, ball.fire - 1), max(0, ball.fast - 1)
        ball.quiet += 1
        if ball.quiet % QUIET == 0:
            # Going round in circles: another angle; in a pocket: gone,
            # unless alone: the game starts again by itself.
            if ball.quiet >= 3 * QUIET and len(self.balls) > 1:
                self.balls.remove(ball)
            else:
                sign = self.rng.choice((-1, 1))
                ball.vx = sign * self.rng.uniform(0.3, VX)

    def _duel(self, racket: Racket, ball: Ball) -> None:
        """Two teams face to face: `DUEL` bounces of one ball, a team
        each in turn, within `SPAN` seconds, the last two rackets `NEAR`
        rows apart at most. One of the two, drawn, keeps away for a
        while."""
        if self.teams < 2:
            return
        rally = self.rally
        rally.append((self.steps, racket, ball.n))
        hits = [r for _, r, _ in rally]
        if (
            len(hits) < DUEL
            or self.steps - rally[0][0] > SPAN * self.second
            or any(a.team == b.team for a, b in pairwise(hits))
            or any(n != ball.n for _, _, n in rally)
            or abs(hits[-1].y - hits[-2].y) > NEAR
        ):
            return
        one = self.rng.choice([r for r in hits[-2:] if r is not self.mine] or hits[-1:])
        if one is self.mine:
            return
        one.away = self.steps + round(self.rng.uniform(*AWAY) * self.second)
        one.plan = None
        rally.clear()
        self.events.append(("retreat", (one.x + one.w // 2, one.y), one))

    # -- the player ----------------------------------------------------------

    @property
    def mine(self) -> Racket | None:
        """The racket of the player: the one of its team that is no twin."""
        if self.player is None:
            return None
        return next(
            (r for r in self.rackets if r.team == self.player and r.left is None), None
        )

    def press(self, key: str) -> None:
        """An arrow (`up`, `down`, `left`, `right`): the racket of the
        player goes that way for `HELD` steps; the opposite one stops
        it. Anything else is ignored."""
        if key not in ARROWS:
            return
        axis, way = ARROWS[key]
        was, left = self.held[axis]
        self.held[axis] = (0, 0) if left and was == -way else (way, HELD)

    def _play(self, box: Box) -> None:
        """Move the racket of the player as held: `REACH_X` cells along
        its row, then a row; a cell at a time, as far as it fits."""
        r = self.mine
        moves = []
        for axis, (way, left) in enumerate(self.held):
            if left:
                self.held[axis] = (way, left - 1)
                moves += [(way, 0)] * REACH_X if axis == 0 else [(0, way)]
        done = 0
        if r is not None and not r.dead and moves:
            room = self._free(box, r.w)
            bad = {(x - i, y) for x, y in self._blocked(r) for i in range(r.w)}
            for dx, dy in moves:
                there = (r.x + dx, r.y + dy)
                if there in room and there not in bad:
                    r.x, r.y = there
                    done += 1
        self._pace(done)

    def _pace(self, done: int) -> None:
        """Set the cells the rackets that play alone move in this step.
        `MOVES`; against a player, two teams: the mean of the cells a
        step its racket moved (`done`, in this one) over the last
        `PACED` seconds, within `PACES`; a share of a cell is kept for
        the next step."""
        if self.player is not None and self.teams > 1:
            self.moved.append(done)
            if len(self.moved) == self.moved.maxlen:
                low, high = PACES
                self.pace = min(high, max(low, sum(self.moved) / len(self.moved)))
        self.credit += self.pace
        self.reach = int(self.credit)
        self.credit -= self.reach

    def _out(self, ball: Ball, box: Box) -> None:
        """Out by the top or the bottom. The last one: the score of who
        keeps that side is lost."""
        self.balls.remove(ball)
        top = ball.at[1] < box[1]
        team = 0 if self.teams == 1 or top else 1
        at = (ball.at[0], box[1] if top else box[3] - 1)
        if self.balls:
            self.events.append(("out", at, team))
            return
        self.score[team] = 0
        self.server = team
        self.events.append(("lost", at, team))

    def _strike(self, brick: Brick, ball: Ball, cell: Cell) -> None:
        """A ball bounced off `brick`."""
        ball.quiet = 0
        if brick.kind == "loose":
            self._lift(brick)
            self.fallers.append(Faller(brick.n, brick.x, brick.y, brick.color))
            self._score(ball)
            if ball.combo >= 2:
                self.events.append(("combo", cell, ball.combo))
            return
        if brick.kind == "bumper":
            rng = self.rng
            ball.vx = rng.choice((-1, 1)) * rng.uniform(0.5, VX)
            ball.fast = max(ball.fast, LAST["kick"])
            self.events.append(("bump", cell))
        brick.lives -= 1
        if brick.lives > 0:
            self.score[ball.owner] += POINTS // 2
            self.events.append(("hit", cell, brick))
        else:
            self._smash(brick, ball, cell)

    def _score(self, ball: Ball) -> None:
        """A brick more in a row for `ball`: each is worth more."""
        ball.combo += 1
        self.last = self.steps
        self.score[ball.owner] += POINTS + BONUS * min(ball.combo - 1, 5)

    def _near(self, brick: Brick) -> set[Brick]:
        """What the blast of `brick` reaches."""
        return {
            self.solid[c]
            for y in (brick.y - 1, brick.y, brick.y + 1)
            for x in range(brick.x - BW, brick.x + 2 * BW)
            if (c := (x, y)) in self.solid
        }

    def _smash(self, brick: Brick, ball: Ball, cell: Cell) -> None:
        """`ball` breaks `brick`, and what its blast reaches when it is
        a bomb."""
        at, team = cell, ball.owner
        queue = deque([(brick, cell)])
        while queue:
            brick, cell = queue.popleft()
            if not self._lift(brick):
                continue
            self._score(ball)
            self.events.append(("break", cell, brick, team))
            if brick.kind == "bomb":
                self.events.append(("boom", (brick.x + BW // 2, brick.y)))
                near = sorted(self._near(brick), key=lambda b: b.n)
                queue.extend((b, (b.x + BW // 2, b.y)) for b in near)
            elif self.bonuses and self.rng.random() < DROP:
                kind = self.rng.choices(list(BONUSES), list(BONUSES.values()))[0]
                if kind != "multi" or len(self.balls) < CROWD:
                    self.capsules.append(Capsule(self._number(), *cell, kind))
        if ball.combo >= 2:
            self.events.append(("combo", at, ball.combo))

    # -- the rackets -------------------------------------------------------

    def _trace(
        self,
        state: tuple[Any, ...],
        box: Box,
        walls: set[Cell],
        steps: int,
        *,
        at: int = 0,
        dents: dict[Brick, int] | None = None,
        racket: tuple[int, int, int] = (0, -1, 0),
        path: list[tuple[Any, ...]] | None = None,
        marks: list[tuple[int, Brick, int]] | None = None,
    ) -> tuple[float, int | None, int, float, float, int]:
        """A ball flown `steps` steps from `state` (`x, y, vx, vy,
        phase, combo, fast, fire`) as `_roll` does, on a copy of the
        terrain: `dents` gives the lives left to the bricks it changed,
        0 when gone; a brick is there until its group goes. `racket`
        (`x, y, w`) stands in its way, where not
        under text; no other.
        The angle a bumper gives is not known: the bounce is kept.

        Returns the gain, the side the ball leaves by (-1 top, 1 bottom)
        or None, the step, `x`, `y` and the index of the last bounce.
        The gain: a brick broken at step `k` counts `1 - (at + k) / 200`,
        0.25 at least; a hit that does not break, `DENT` of it."""
        x0, y0, x1, y1 = box
        x, y, vx, vy, phase, combo, fast, fire = state
        solid = self.solid
        dents = {} if dents is None else dents
        lx, row, w = racket
        end = lx + w
        gain, last, index = 0.0, 0, 0
        now = self.steps + at - 1
        shut = self.teams == 1
        gear = self.gear
        k = 0

        def there(brick: Brick) -> bool:
            return dents.get(brick, 1) > 0 and not 0 < brick.due <= now + k

        def hard(c: Cell) -> bool:
            cx, cy = c
            if not y0 <= cy < y1:
                return shut and cy < y0
            if c in walls or not x0 <= cx < x1 or (cy == row and lx <= cx < end):
                return True
            if fire:
                return False
            brick = solid.get(c)
            return brick is not None and there(brick)

        def smash(brick: Brick, weight: float) -> None:
            nonlocal gain, combo
            queue = deque([brick])
            while queue:
                brick = queue.popleft()
                if not there(brick):
                    continue
                dents[brick] = 0
                gain += weight
                combo += 1
                if marks is not None:
                    marks.append((index, brick, 0))
                if brick.kind == "bomb":
                    queue.extend(self._near(brick))

        for k in range(1, steps + 1):
            phase += PACE + min(combo, RALLY) + (KICK if fast else 0)
            moves, phase = phase >> gear, phase & ((1 << gear) - 1)
            weight = max(0.25, 1 - (at + k) / 200)
            for _ in range(moves):
                x, y, vx, vy, hits = fly(x, y, vx, vy, hard)
                index += 1
                for cell, flat in hits:
                    last = index
                    if cell[1] == row and lx <= cell[0] < end and cell not in walls:
                        combo = 0
                        if flat:
                            vx = max(-VX, min(VX, (x - lx - w / 2) / (w / 2) * VX))
                        continue
                    brick = solid.get(cell)
                    if brick is None or not there(brick):
                        continue
                    left = dents.get(brick, brick.lives) - 1
                    if brick.kind == "loose":
                        left = 0
                    elif brick.kind == "bumper":
                        fast = max(fast, LAST["kick"])
                    if left > 0:
                        dents[brick] = left
                        gain += DENT * weight
                        if marks is not None:
                            marks.append((index, brick, left))
                    else:
                        smash(brick, weight)
                if fire:
                    brick = solid.get((floor(x), floor(y)))
                    if brick is not None and there(brick):
                        smash(brick, weight)
                if path is not None:
                    path.append((k, x, y, vx, vy, gain, phase, fast, fire))
                if not y0 <= floor(y) < y1:
                    return gain, (-1 if y < y0 else 1), k, x, y, last
            fast, fire = max(0, fast - 1), max(0, fire - 1)
        return gain, None, steps, x, y, last

    def _foresee(self, ball: Ball, box: Box, walls: set[Cell]) -> _Flight:
        path = [(0, ball.x, ball.y, ball.vx, ball.vy, 0.0, 0, 0, 0)]
        marks: list[tuple[int, Brick, int]] = []
        state = (ball.x, ball.y, ball.vx, ball.vy)
        state += (ball.phase, ball.combo, ball.fast, ball.fire)
        *_, side, _, _, _, last = self._trace(
            state, box, walls, FORESEE, path=path, marks=marks
        )
        # Rows entered, but for the last move: that one may be out of the view.
        entries = [
            i
            for i in range(len(path) - 2)
            if floor(path[i + 1][2]) != floor(path[i][2])
        ]
        return _Flight(ball, path, marks, last, side, entries)

    def _shot(
        self,
        r: Racket,
        flight: _Flight,
        i: int,
        place: Cell,
        box: Box,
        walls: set[Cell],
    ) -> float:
        """Worth, in bricks, of what follows when `r` at `place` is hit
        by the ball of `flight` on its move `i`: the gain of `AIM` steps,
        the bricks broken before being gone. Less `RISK` for a ball that
        then leaves by a side `r` keeps, `LOSS` when `r` cannot be there;
        a ball out by the other's side and the way to the nearest brick
        only tell equal shots apart."""
        self.shots += 1
        before, after = flight.path[i], flight.path[i + 1]
        lx, row = place
        half = r.w / 2
        vx = max(-VX, min(VX, (after[1] - lx - half) / half * VX))
        state = (after[1], before[2], vx, -before[4], *after[6:7], 0, *after[7:])
        dents = {brick: left for j, brick, left in flight.marks if j <= i}
        gain, side, k, x, y, _ = self._trace(
            state, box, walls, AIM, at=after[0], dents=dents, racket=(lx, row, r.w)
        )
        if side is None:
            if gain:
                return gain
            far = min(
                (abs(x - bx) + 2 * abs(y - by) for bx, by in self.lure), default=0
            )
            return -min(0.2, far / 1000)
        if self.teams > 1 and (side > 0) != (r.team > 0):
            return gain + 0.05
        edge = box[1] if side < 0 else box[3] - 1
        far = abs(x - lx - half) + abs(edge - row)
        return gain - (RISK if far + SPARE <= int(self.pace * k) else LOSS)

    def _free(self, box: Box, w: int) -> set[Cell]:
        """`_room` with nothing taken; kept while the terrain is."""
        kept = self.fits.get(w)
        key = (box, self.version)
        if kept is None or kept[0] != key:
            kept = self.fits[w] = (key, self._room(box, w, set()))
        return kept[1]

    def _reach(
        self,
        r: Racket,
        room: set[Cell],
        depth: int,
        goals: set[Cell] | None = None,
        first: bool = False,
    ) -> tuple[dict[Cell, int], bool]:
        """Moves from `r` to each place it fits, `depth` at most, the
        nearest first. With `goals` the search ends once it has them
        all, or the `first` of them, and those as near. Also returns
        whether it went to its end."""
        bad = {(x - i, y) for x, y in self._blocked(r) for i in range(r.w)}
        edge = {(r.x, r.y)}
        dist = dict.fromkeys(edge, 0)
        left = None
        if goals is not None:
            left = (goals & room) - bad - edge
            if not left or (first and goals & edge):
                return dist, False
        for moves in range(1, depth + 1):
            around: set[Cell] = set()
            for x, y in edge:
                around.update(((x, y + 1), (x, y - 1), (x + 1, y), (x - 1, y)))
            around = {c for c in around & room if c not in dist and c not in bad}
            if not around:
                break
            dist.update(dict.fromkeys(around, moves))
            edge = around
            if left is not None:
                found = left & around
                left -= found
                if not left or (first and found):
                    return dist, False
        return dist, True

    @staticmethod
    def _spots(r: Racket, flights: list[_Flight], steps: int) -> set[Cell]:
        """The places where `r` is hit by a ball of `flights` as it
        enters a row, within `steps` steps."""
        out = set()
        for flight in flights:
            path = flight.path
            for i in flight.entries:
                k, x, y = path[i + 1][:3]
                if k > steps:
                    break
                out.update(
                    (lx, floor(y)) for lx in range(floor(x) - r.w + 1, floor(x) + 1)
                )
        return out

    def _steer(self, box: Box, walls: set[Cell]) -> None:
        """Each racket chooses a place and moves towards it. In order:
        a ball about to leave by its side, and a good capsule on the
        way when it has the time; out of the way of a falling brick; a
        good capsule; a ball that is safe, when hitting it breaks more
        bricks than not; else it waits near a ball, off the text. A
        shot chosen is kept as long as the ball comes as foreseen. One
        that keeps away from a duel only stops the balls about to
        leave by its side, as late as it can, and waits in its den.
        Text is no obstacle; a hit is never planned on a cell of text:
        a ball is never foreseen in one. The racket of a player is left
        alone."""
        alive = [r for r in self.rackets if not r.dead and r is not self.mine]
        if not alive:
            return
        self.lure = [(b.x + BW / 2, b.y) for b in self.bricks(box)]
        self.shots = 0
        flights = {b.n: self._foresee(b, box, walls) for b in self.balls}
        # By racket: the last search, its depth, whether whole, what for.
        reach: dict[Racket, tuple[dict[Cell, int], int, bool, str]] = {}

        def ways(
            r: Racket,
            depth: int,
            goals: set[Cell] | None = None,
            first: bool = False,
            why: str = "",
        ) -> dict[Cell, int]:
            """`_reach`; one kept is used again when whole and as
            deep, or made for the same `why`."""
            kept = reach.get(r)
            if kept is None or not (
                (kept[2] and kept[1] >= depth) or (why and kept[3] == why)
            ):
                room = self._free(box, r.w)
                kept = (*self._reach(r, room, depth, goals, first), why)
                reach[r] = kept = (kept[0], depth, kept[1], why)
            return kept[0]

        def way(r: Racket, goal: Cell, depth: int) -> list[Cell]:
            """From `r` to `goal`, less where it is: along the rows
            first, to be on the row of a ball before the ball."""
            kept = reach.get(r)
            dist = kept[0] if kept and goal in kept[0] else ways(r, depth)
            out = []
            left = dist.get(goal, 0)
            x, y = goal
            while left:
                out.append((x, y))
                left -= 1
                x, y = next(
                    c
                    for c in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1))
                    if dist.get(c) == left
                )
            return out[::-1]

        for r in alive:
            plan = r.plan
            if plan is None:
                continue
            if not self._holds(r, flights, box, walls):
                r.plan = None
                continue
            taken = self._blocked(r)
            if any(
                (x + i, y) in taken
                for x, y in plan.way[: self.reach]
                for i in range(r.w)
            ):
                # A ball or a racket in the way: another way, if in time.
                depth = int(self.pace * (plan.step - self.steps))
                if plan.place in ways(r, depth, {plan.place}):
                    plan.way = way(r, plan.place, depth)
                else:
                    r.plan = None
        goals: dict[Racket, list[Cell]] = {}
        slack: dict[Racket, int] = {}  # moves in hand of one with a ball to stop
        minded = set()  # balls a racket goes for
        threats = [f for f in flights.values() if f.side is not None]
        threats.sort(key=lambda f: (f.path[-1][0], f.ball.n))
        # One search a racket: as far as the last of them, until it has
        # every place where it would be hit.
        aims: dict[Racket, set[Cell]] = {}
        depth = int(self.pace * threats[-1].path[-1][0]) + SPARE if threats else 0
        for flight in threats:
            team = 0 if self.teams == 1 or flight.side < 0 else 1
            free = [r for r in alive if r.team == team and r not in goals]
            keeper = [r for r in free if r.plan and r.plan.ball == flight.ball.n]
            if keeper:
                r, plan = keeper[0], keeper[0].plan
                goals[r] = plan.way
                slack[r] = int(self.pace * (plan.step - self.steps)) - len(plan.way)
                minded.add(flight.ball.n)
                continue
            plans = []
            for r in free:
                if r not in aims:
                    mine = [f for f in threats if (f.side > 0) == (r.team > 0)]
                    aims[r] = self._spots(
                        r, mine if self.teams > 1 else threats, FORESEE
                    )
                dist = ways(r, depth, aims[r], why="stop")
                found = self._intercept(r, flight, dist, box, walls)
                if found is not None:
                    plans.append((found[0], r.n, r, found))
            if not plans:
                continue
            _, _, r, (late, goal, i) = min(plans, key=lambda p: p[:2])
            goals[r], r.plan = way(r, goal, depth), None
            slack[r] = -late
            minded.add(flight.ball.n)
            if late < 0:
                k, x = flight.path[i + 1][:2]
                r.plan = _Plan(flight.ball.n, goal, self.steps + k, x, r.w, goals[r])
        danger = {
            (x, y)
            for f in self.fallers
            for x in range(f.x, f.x + BW)
            for y in range(f.y, f.y + WARN)
        }
        good = [cap for cap in self.capsules if cap.kind not in BAD]

        def treats(r: Racket) -> set[Cell]:
            """Where `r` takes a good capsule: on it, or under it."""
            return {
                (x, y)
                for cap in good
                for y in range(cap.y, cap.y + 4)
                for x in range(cap.x - r.w + 1, cap.x + 1)
            }

        for r in alive:
            away = r.away > self.steps
            if r in goals:
                # Time in hand to stop its ball: a capsule on the way.
                spare = slack[r] - SPARE
                if away or not good or spare < 2 or danger.intersection(r.cells):
                    continue
                dist = ways(r, spare // 2, treats(r), True)
                spots = [(dist[c], c) for c in treats(r) if c in dist]
                if spots:
                    goals[r], r.plan = way(r, min(spots)[1], spare // 2), None
                continue
            if danger.intersection(r.cells):
                safe = (
                    c
                    for c in ways(r, 4 * WARN)
                    if not any((c[0] + i, c[1]) in danger for i in range(r.w))
                )
                goal = next(safe, None)  # in the order found: the nearest
                if goal is not None:
                    goals[r], r.plan = way(r, goal, 4 * WARN), None
                    continue
            elif good and not away:
                dist = ways(r, FETCH, treats(r), True)
                spots = [(dist[c], c) for c in treats(r) if c in dist]
                if spots:
                    goals[r], r.plan = way(r, min(spots)[1], FETCH), None
                    continue
            if r.plan is not None and r.plan.ball in minded:
                r.plan = None
            if r.plan is None and self.steps >= r.think and not away:
                r.think = self.steps + THINK
                depth = int(self.pace * CHASE)
                rest = [f for f in flights.values() if f.ball.n not in minded]
                dist = ways(r, depth, self._spots(r, rest, CHASE))
                found = self._chase(r, rest, dist, box, walls)
                if found is not None:
                    flight, goal, i = found
                    k, x = flight.path[i + 1][:2]
                    path = way(r, goal, depth)
                    r.plan = _Plan(flight.ball.n, goal, self.steps + k, x, r.w, path)
            if r.plan is not None:
                goals[r] = r.plan.way
                minded.add(r.plan.ball)
        for r in alive:
            if r not in goals:
                rest = [f for f in flights.values() if f.ball.n not in minded]
                rest = rest or list(flights.values())
                if r.away > self.steps:
                    goal = self._den(r, ways(r, REACH), box, walls)
                else:
                    goal = self._wait(r, rest, ways(r, REACH), box, walls)
                if goal is not None:
                    goals[r] = way(r, goal, REACH)
        for r, path in goals.items():
            taken = self._blocked(r)
            done = 0
            for x, y in path[: self.reach]:
                if any((x + i, y) in taken for i in range(r.w)):
                    break
                r.x, r.y = x, y
                done += 1
            if r.plan is not None and r.plan.way is path:
                del path[:done]

    def _holds(
        self, r: Racket, flights: dict[int, _Flight], box: Box, walls: set[Cell]
    ) -> bool:
        """Whether the shot `r` goes for is still to come: the ball
        enters the row then and there, and `r` is on its way in time."""
        plan = r.plan
        flight = flights.get(plan.ball)
        k = plan.step - self.steps
        if flight is None or k < 1 or plan.w != r.w:
            return False
        room = self._free(box, r.w)
        here = plan.way[0] if plan.way else plan.place
        if (
            abs(here[0] - r.x) + abs(here[1] - r.y) != bool(plan.way)
            or len(plan.way) > int(self.pace * k)
            or not room.issuperset(plan.way)
        ):
            return False
        path = flight.path
        return any(
            path[i + 1][0] == k
            and path[i + 1][1] == plan.x
            and floor(path[i + 1][2]) == plan.place[1]
            for i in flight.entries
        )

    def _intercept(
        self,
        r: Racket,
        flight: _Flight,
        dist: dict[Cell, int],
        box: Box,
        walls: set[Cell],
    ) -> tuple[int, Cell, int] | None:
        """Where `r` gets in the way of a ball before it leaves: on a
        row the ball enters, under it or over it. The rows `r` reaches
        in time are ranked: those of the last leg with `SPARE` moves to
        spare first, the latest first; of the places on the best three
        of a rank, the one worth the most (`_shot`); on the best one
        only once `SHOTS` shots were weighed in the step. None in time: the
        row where it is the least late. Returns `(lateness, place,
        index of the move)`."""
        path = flight.path
        found = []  # (rank, entry, places in time)
        late = []  # (moves short, entry, place)
        for i in flight.entries:
            k, x, y = path[i + 1][:3]
            spots = [(lx, floor(y)) for lx in range(floor(x) - r.w + 1, floor(x) + 1)]
            spots = [c for c in spots if c in dist]
            if not spots:
                continue
            due = int(self.pace * k) - 1
            timely = [c for c in spots if dist[c] <= due]
            if timely:
                spare = max(due - dist[c] for c in timely)
                rank = 2 if i < flight.last else int(spare < SPARE)
                found.append((rank, -i, timely))  # the latest first
            else:
                c = min(spots, key=lambda c: (dist[c], c))
                late.append((dist[c] - due, -i, c))
        if not found:
            if not late:
                return None
            short, i, c = min(late)
            return short, c, -i
        found.sort()
        best = None
        rows = 3 if self.shots < SHOTS else 1
        for _, i, spots in [f for f in found[:rows] if f[0] == found[0][0]]:
            for c in spots:
                worth = path[-i][5] + self._shot(r, flight, -i, c, box, walls)
                worth -= HIDE * self._hidden(c, r.w, walls)
                key = (dist[c] / 100 - worth, c)
                if best is None or key < best[0]:
                    best = (key, -i, c)
        assert best is not None
        _, i, goal = best
        return dist[goal] - int(self.pace * path[i + 1][0]), goal, i

    def _chase(
        self,
        r: Racket,
        flights: list[_Flight],
        dist: dict[Cell, int],
        box: Box,
        walls: set[Cell],
    ) -> tuple[_Flight, Cell, int] | None:
        """A ball that is safe, hit on purpose: of the two nearest
        balls, of three rows `r` reaches in time within `CHASE` steps,
        the place whose shot wins the most over the ball left alone,
        `EDGE` at least. With two teams the bricks a ball of the other
        team breaks count half."""

        def near(f: _Flight) -> tuple[float, int]:
            return abs(f.ball.x - r.x) + 2 * abs(f.ball.y - r.y), f.ball.n

        best = None
        for flight in sorted(flights, key=near)[: 2 if self.shots < SHOTS else 0]:
            path = flight.path
            share = 1.0 if flight.ball.owner == r.team else 0.5
            rows = []
            for i in flight.entries:
                k, x, y = path[i + 1][:3]
                if k > CHASE:
                    break
                spots = [
                    c
                    for lx in range(floor(x) - r.w + 1, floor(x) + 1)
                    if (c := (lx, floor(y))) in dist
                    and dist[c] < int(self.pace * k) - 1
                ]
                if spots:
                    rows.append((i, spots))
            for n in sorted({0, len(rows) // 2, len(rows) - 1} if rows else ()):
                i, spots = rows[n]
                k = path[i + 1][0] + AIM
                j = next((j for j, p in enumerate(path) if p[0] > k), len(path)) - 1
                alone = share * (path[j][5] - path[i][5])
                for c in spots:
                    worth = self._shot(r, flight, i, c, box, walls) - alone
                    worth -= HIDE * self._hidden(c, r.w, walls)
                    key = (dist[c] / 100 - worth, c)
                    if worth >= EDGE and (best is None or key < best[0]):
                        best = (key, flight, c, i)
        return best[1:] if best else None

    def _wait(
        self,
        r: Racket,
        flights: list[_Flight],
        dist: dict[Cell, int],
        box: Box,
        walls: set[Cell],
    ) -> Cell | None:
        """Where `r` waits with nothing to do: under the column the
        nearest ball is about to be in, `GAP` rows past the rows it is
        about to cross, on the side `r` keeps; alone, on the side the
        ball stays in. With no ball, where it is. A cell under text
        counts for four columns. None: it stays."""
        y0, y1 = box[1], box[3]
        tx, ty = r.x, r.y
        if flights:

            def near(f: _Flight) -> tuple[float, int]:
                return abs(f.ball.x - r.x) + 2 * abs(f.ball.y - r.y), f.ball.n

            path = min(flights, key=near).path
            rows = [p[2] for p in path[:AHEAD]]
            r.guard = 1 if r.team or self.teams == 1 else -1
            tx = floor(path[min(len(path) - 1, 8)][1]) - r.w // 2
            edge = max(rows) if r.guard > 0 else min(rows)
            ty = min(max(floor(edge) + r.guard * GAP, y0 + 1), y1 - 2)
        return self._settle(r, (tx, ty), dist, walls)

    def _den(
        self, r: Racket, dist: dict[Cell, int], box: Box, walls: set[Cell]
    ) -> Cell | None:
        """Where `r` keeps away from a duel: two rows off the side it
        keeps, in the half of the columns the nearest racket of the
        other team is not in."""
        x0, y0, x1, y1 = box
        others = [o for o in self.rackets if not o.dead and o.team != r.team]
        tx = r.x
        if others:
            other = min(others, key=lambda o: (abs(o.x - r.x) + abs(o.y - r.y), o.n))
            left = 2 * other.x + other.w >= x0 + x1
            tx = x0 + (x1 - x0) * (1 if left else 3) // 4 - r.w // 2
        ty = min(y0 + 2, y1 - 1) if r.team == 0 else max(y1 - 3, y0)
        return self._settle(r, (tx, ty), dist, walls)

    def _settle(
        self, r: Racket, to: Cell, dist: dict[Cell, int], walls: set[Cell]
    ) -> Cell | None:
        """The place within `REACH` moves nearest to `to`, a row counting
        for two columns, a cell under text for four; None when `r` is as well
        where it is, within three columns."""

        def off(c: Cell) -> int:
            hidden = self._hidden(c, r.w, walls)
            return abs(c[0] - to[0]) + 2 * abs(c[1] - to[1]) + 4 * hidden

        near = [c for c, moves in dist.items() if moves <= REACH]
        goal = min(near, key=lambda c: (off(c), dist[c], c))
        return goal if off((r.x, r.y)) - off(goal) >= 3 else None

    @staticmethod
    def _hidden(c: Cell, w: int, walls: set[Cell]) -> int:
        """Cells of a racket of `w` cells at `c` that are under text."""
        if not walls:
            return 0
        x, y = c
        return len([i for i in range(x, x + w) if (i, y) in walls])

    # -- rules and bonuses -------------------------------------------------

    def _fall(self, box: Box, walls: set[Cell]) -> None:
        """Loose bricks fall a row every other step: onto the ground,
        out of the view, or on a racket."""
        if self.steps % 2:
            return
        rackets = self._cells(walls)
        for faller in list(self.fallers):
            under = [(x, faller.y + 1) for x, _ in faller.cells]
            hit = {rackets[c] for c in faller.cells + under if c in rackets}
            if hit:
                self.fallers.remove(faller)
                for racket in sorted(hit, key=lambda r: r.n):
                    self._kill(racket)
            elif any(c in walls or c in self.solid for c in under):
                self.fallers.remove(faller)
                self.events.append(("crash", (faller.x + BW // 2, faller.y)))
            else:
                faller.y += 1

    def _kill(self, racket: Racket) -> None:
        at = (racket.x + racket.w // 2, racket.y)
        self.events.append(("kill", at, racket))
        racket.dead, racket.span = DEAD, 0
        if racket.left is not None:
            self.rackets.remove(racket)

    def _drift(self, box: Box, walls: set[Cell]) -> None:
        """Capsules sink a row every third step and rest on the ground
        for a while; a racket on one takes it."""
        rackets = self._cells(walls)
        for cap in list(self.capsules):
            at = (cap.x, cap.y)
            if at in rackets:
                self.capsules.remove(cap)
                self._award(cap, rackets[at], box)
                continue
            if self.steps % 3:
                continue
            under = (cap.x, cap.y + 1)
            if under in walls or under in self.solid:
                cap.rest += 1
                if cap.rest > 2 * self.second:
                    self.capsules.remove(cap)
            else:
                cap.y += 1

    def _size(self, r: Racket, w: int, box: Box) -> bool:
        """Make `r` `w` cells wide, about its middle; False when a wider
        one does not fit."""
        mid = r.x + (r.w - w) // 2
        if w > r.w:
            room = self._room(box, w, self._blocked(r))
            fits = [x for x in range(r.x - w + r.w, r.x + 1) if (x, r.y) in room]
            if not fits:
                return False
            mid = min(fits, key=lambda x: (abs(x - mid), x))
        r.x, r.w = mid, w
        return True

    def _award(self, cap: Capsule, racket: Racket, box: Box) -> None:
        team = racket.team
        self.score[team] += POINTS
        self.events.append(("catch", (cap.x, cap.y), cap.kind, team))
        if cap.kind in ("wide", "small"):
            for r in [r for r in self.rackets if r.team == team and not r.dead]:
                w = WIDE if cap.kind == "wide" else SMALL
                if self._size(r, w, box):
                    r.span = LAST[cap.kind]
        elif cap.kind == "twin":
            # One twin a team; another capsule does not make it last.
            if not any(r.team == team and r.left is not None for r in self.rackets):
                self.rackets.append(Racket(self._number(), team, left=LAST["twin"]))
        elif cap.kind == "multi":
            for ball in list(self.balls):  # two for one, `BALLS` at most
                if len(self.balls) >= BALLS:
                    break
                turn = self.rng.uniform(*SPLIT)
                vx = ball.vx + (turn if ball.vx <= 0 else -turn)
                twin = Ball(self._number(), ball.x, ball.y, vx, ball.vy, ball.owner)
                twin.fire, twin.fast = ball.fire, ball.fast
                twin.combo, twin.phase = ball.combo, ball.phase
                self.balls.append(twin)
        elif cap.kind == "fast":
            for ball in self.balls:
                ball.fast = LAST["fast"]
        elif cap.kind == "fire":
            for ball in self.balls:
                ball.fire = 5 * self.second


def view(frame: Frame) -> Box:
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    game = Game(
        random.Random(args.seed),
        rules=args.rules,
        bonuses=args.bonuses,
        multiplayer=args.multiplayer,
        second=max(1, round(args.speed)),
        player=int(args.multiplayer) if args.grab else None,
    )
    frame = _cli.pick(pilot, args.frame)
    # Its own dice: the particles do not change the game of a seed.
    stage = _bricks.Stage(pilot, frame, random.Random(args.seed), args.effects)
    keys: list[dict[str, Any]] = []
    if args.grab:
        stage.hint = "←↑↓→  ctrl+t"
        # The keys come among the events `Sight.look` takes: set apart.
        events = pilot.event

        def event(timeout: float | None = None) -> tuple[str, Any] | None:
            while (one := events(timeout)) is not None:
                if one[0] != "input":
                    return one
                keys.append(one[1] or {})
            return None

        pilot.event = event  # type: ignore[method-assign]
        pilot.grab(*ARROWS)
    try:
        _run(pilot, args, game, frame, stage, keys, seconds)
    finally:
        if args.grab:
            with contextlib.suppress(PtyrcError, OSError):
                pilot.release()


def _run(
    pilot: Pilot,
    args: argparse.Namespace,
    game: Game,
    frame: Frame,
    stage: _bricks.Stage,
    keys: list[dict[str, Any]],
    seconds: float | None,
) -> None:
    # The walls are read when the driver says the text changed or the
    # view moved (topic `text`), not at each step.
    sight = _cli.Sight(pilot, frame)
    walls: set[Cell] = set()
    # A tick in two is a step; the other puts in place the balls that
    # were shown half way (`Stage.glide`).
    step = False
    for _ in _cli.ticks(0.5 / args.speed, seconds):
        seen = sight.look()
        for data in keys:
            if "released" in data:  # the escape key: the end
                return
            game.press(data.get("key") or "")
        keys.clear()
        if not seen:
            continue
        box = view(frame)
        if sight.changed:
            walls = walls_of(sight.rows, box[0], box[1])
        step = not step
        if step:
            game.step(box, walls)
            with sight.drawing(), pilot.batch():
                stage.show(game, box)
        elif stage.late:
            with sight.drawing(), pilot.batch():
                stage.glide()


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "bricks", "A brick breaker that plays alone around the words.", frame="largest"
    )
    out.add_argument(
        "--speed", type=float, default=16.0, help="steps per second [default: 16]"
    )
    out.add_argument("--seed", type=int, help="same seed, same game")
    out.add_argument(
        "-r",
        "--rules",
        action="store_true",
        help="hard, loose, bumper and bomb bricks",
    )
    out.add_argument(
        "-b",
        "--bonuses",
        action="store_true",
        help="capsules: two balls for one, mostly; wide, twin, fire, small, fast",
    )
    out.add_argument(
        "-m", "--multiplayer", action="store_true", help="two rackets, two scores"
    )
    out.add_argument(
        "-e",
        "--effects",
        action="store_true",
        help="particles, a trail, combos, a best score",
    )
    out.add_argument("-a", "--all", action="store_true", help="-r -b -m -e")
    out.add_argument(
        "--grab",
        action="store_true",
        help="the arrows of the keyboard move the racket (with -m, the bottom "
        "one), the balls go half as fast; ctrl+t gives them back and ends",
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.rules = args.bonuses = args.multiplayer = args.effects = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
