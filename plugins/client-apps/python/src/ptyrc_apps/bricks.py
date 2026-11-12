# SPDX-License-Identifier: GPL-3.0-or-later
"""Bricks: a brick breaker that plays alone, around the words.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app bricks
    ptyrc-app bricks -r -b           # rules and bonuses
    ptyrc-app bricks -a              # the arcade

What it shows:

- The screen read as a map: glyphs are walls, the ball bounces off
  them, off the sides of the view and off the bricks. The top and the
  bottom of the view are open.
- A racket that moves in two dimensions and is hit on either side. No
  one plays: it foresees the ball, gets in its way before it leaves
  the view, and of the places it reaches in time takes the one whose
  shot breaks the most bricks. With no ball to stop it hits one on
  purpose when that breaks more bricks than not.
- Text does not stop a racket: sprites are drawn behind the text, a
  racket under it is hidden, and the ball meets the text, not the
  racket. A racket keeps off the text when it has the choice.
- Terrain in frame cells: groups of 2 or 3 bricks, laid one at a time
  all over the view, far from each other, where a ball gets: between
  the lines, after the short ones, in the blanks. A group goes 10 to
  20 s after it is laid, dim for its last second.

The game never ends. The last ball out sets the score to 0. 30 s with
no brick broken: the game starts again, on a new map, the rackets
elsewhere; the scores stay.

More, each on its own flag; `-a` for all (see `_bricks`):

- `-r`: rules. Bricks that take three hits; loose ones that fall when
  touched and kill the racket under them; bumpers that send the ball
  away faster, anywhere; bombs that take their neighbours with them.
- `-b`: bonuses. Two broken bricks in five drop a capsule; the racket
  that touches it gets, three times in five, two balls for each one;
  else a wide racket, a twin, or 5 s of fire: the ball goes through
  the bricks. Two are bad, and no racket goes for them: a small
  racket, a faster ball. A spare ball out costs nothing.
- `-m`: two rackets, two scores. One keeps the top, the other the
  bottom; a brick is worth points to the last that hit the ball.
  Bricks come first; the other's loss only tells two equal shots
  apart. Face to face, one of the two keeps away for 2 to 3 s.
- `-e`: particles, a trail, combos, a best score.

Rules count in steps of the game, not in seconds; `second` is the
steps in a second.
"""

from __future__ import annotations

import argparse
import random
from collections import deque
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from itertools import pairwise
from math import floor
from typing import Any

from ptyrc import Frame, Pilot

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
PACE = 12  # eighths of a move a ball makes in a step
RALLY = 4  # bricks in a row that speed it up, an eighth each
KICK = 6  # eighths more for a fast one
MOVES = 2  # cells a racket moves in a step
GROUPS = 24  # groups in the view, at most; one a `PLOT` cells, 3 at least
PLOT = 160
FILL = 12  # seconds to lay as many
AGE = (10, 20)  # seconds a group lasts
FADE = 1  # second it is dim before it goes
CLEAR = 60  # moves of a ball no group is laid in the way of
TRIES = 60  # places drawn for a group
PICK = 10  # of which the farthest from the others is kept
ROOM = 40  # free cells a group must be next to
#: Shapes of a group: left cells of its bricks.
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
BALLS = 5  # at once
SPLIT = (0.3, 0.8)  # columns a move between a ball and the one that leaves it
DEAD = 60  # steps a killed racket is away
WARN = 6  # rows under a falling brick a racket keeps out of
LAST = {"wide": 320, "small": 240, "twin": 400, "fast": 160, "kick": 24}  # steps


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
    """Bricks laid together: they go together at step `due`."""

    bricks: list[Brick]
    at: tuple[float, float]  # its middle
    due: int
    dim: bool = False  # about to go


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
    teams; `second`: steps in a second. Same `rng`, same game: no clock.
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
    ) -> None:
        self.rng = rng
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
        self.spawn_due = 0
        self.last = 0  # step of the last brick broken, or of the start
        self.home = (0.5, 0.5)  # of a racket, in shares of its part of the view
        self.lure: list[tuple[float, int]] = []  # the bricks in view
        self.serve_due = SERVE
        self.server = rng.randrange(self.teams)
        self.version = 0  # of the terrain
        self.fits: dict[int, Any] = {}  # `_free`, by width
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

    def _group(self, box: Box, walls: set[Cell]) -> list[Brick]:
        """Lay a group of 2 or 3 bricks on free cells of the view: of
        `PICK` places a ball can get to (`_open`), drawn at random, the
        farthest from the other groups, a row counting for two columns.
        Not on a racket or the two rows about it, nor where a ball is
        about to go."""
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

        others = [g.at for g in self.groups]
        best = None
        found = 0
        for _ in range(TRIES):
            shape = rng.choice(SHAPES)
            x, y = rng.randrange(x0, x1), rng.randrange(y0 + 1, y1 - 1)
            cells = [(x + dx + i, y + dy) for dx, dy in shape for i in range(BW)]
            if not all(free(c) and c not in keep for c in cells):
                continue
            if not self._open(cells, free):
                continue
            mx = sum(c[0] for c in cells) / len(cells) + 0.5
            my = sum(c[1] for c in cells) / len(cells) + 0.5
            far = min(
                ((mx - gx) ** 2 + (2 * (my - gy)) ** 2 for gx, gy in others),
                default=0.0,
            )
            if best is None or far > best[0]:
                best = (far, shape, x, y, (mx, my))
            found += 1
            if found >= PICK:
                break
        if best is None:
            return []
        _, shape, x, y, at = best
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
        due = self.steps + round(rng.uniform(*AGE) * self.second)
        for brick in out:
            brick.due = due
        self.groups.append(Group(out, at, due))
        self.events.append(("group", out))
        return out

    @staticmethod
    def _open(cells: list[Cell], free: Solid) -> bool:
        """Whether a ball can get to `cells`: two free cells or more
        next to them, that lead to `ROOM` free cells."""
        group = set(cells)
        seen = {
            n
            for x, y in cells
            for dx, dy in STEPS
            if (n := (x + dx, y + dy)) not in group and free(n)
        }
        if len(seen) < 2:
            return False
        queue = deque(sorted(seen))
        while queue and len(seen) < ROOM:
            x, y = queue.popleft()
            for dx, dy in STEPS:
                n = (x + dx, y + dy)
                if n not in seen and n not in group and free(n):
                    seen.add(n)
                    queue.append(n)
        return len(seen) >= ROOM

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
        """A group at a time, `FILL` seconds to the most the view
        takes; four times faster under three groups."""
        if self.steps < self.spawn_due:
            return
        x0, y0, x1, y1 = box
        most = max(3, min(GROUPS, (x1 - x0) * (y1 - y0) // PLOT))
        here = sum(x0 <= g.at[0] < x1 and y0 <= g.at[1] < y1 for g in self.groups)
        if here >= most:
            return
        self._group(box, walls)
        gap = max(2, round(FILL * self.second / most))
        self.spawn_due = self.steps + (gap if here >= 2 else max(1, gap // 4))

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
        out = set()
        solid = self.solid
        for y in range(y0, y1):
            run = 0
            for x in range(x0, x1):
                c = (x, y)
                run = 0 if c in solid or c in taken else run + 1
                if run >= w:
                    out.add((x - w + 1, y))
        return out

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
            if racket.left is not None:  # a twin: off its team's racket
                hy += (y1 - y0) // 4 * (1 if racket.n % 2 else -1)
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
            # Towards the far side: time to get in its way.
            far = 1 if 2 * racket.y < y0 + y1 else -1
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
        the view, bricks unless on `fire`."""
        x0, y0, x1, y1 = box
        solid = self.solid

        def hard(c: Cell) -> bool:
            if not y0 <= c[1] < y1:
                return False
            return c in walls or not x0 <= c[0] < x1 or (not fire and c in solid)

        return hard

    def _roll(self, ball: Ball, box: Box, walls: set[Cell]) -> None:
        y0, y1 = box[1], box[3]
        terrain = self._terrain(box, walls, bool(ball.fire))
        rackets = self._cells(walls)
        # The pace is counted in eighths: what is left is owed.
        ball.phase += PACE + min(ball.combo, RALLY) + (KICK if ball.fast else 0)
        moves, ball.phase = ball.phase >> 3, ball.phase & 7
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
        one = self.rng.choice(hits[-2:])
        one.away = self.steps + round(self.rng.uniform(*AWAY) * self.second)
        one.plan = None
        rally.clear()
        self.events.append(("retreat", (one.x + one.w // 2, one.y), one))

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
        k = 0

        def there(brick: Brick) -> bool:
            return dents.get(brick, 1) > 0 and not 0 < brick.due <= now + k

        def hard(c: Cell) -> bool:
            cx, cy = c
            if not y0 <= cy < y1:
                return False
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
            moves, phase = phase >> 3, phase & 7
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
        return gain - (RISK if far + SPARE <= MOVES * k else LOSS)

    def _free(self, box: Box, w: int) -> set[Cell]:
        """`_room` with nothing taken; kept while the terrain is."""
        kept = self.fits.get(w)
        key = (box, self.version)
        if kept is None or kept[0] != key:
            kept = self.fits[w] = (key, self._room(box, w, set()))
        return kept[1]

    def _reach(self, r: Racket, room: set[Cell], depth: int) -> dict[Cell, int]:
        """Moves from `r` to each place it fits, `depth` at most, the
        nearest first."""
        bad = {(x - i, y) for x, y in self._blocked(r) for i in range(r.w)}
        fits = room - bad
        edge = {(r.x, r.y)}
        dist = dict.fromkeys(edge, 0)
        for moves in range(1, depth + 1):
            around: set[Cell] = set()
            for x, y in edge:
                around.update(((x, y + 1), (x, y - 1), (x + 1, y), (x - 1, y)))
            around = {c for c in around & fits if c not in dist}
            if not around:
                break
            dist.update(dict.fromkeys(around, moves))
            edge = around
        return dist

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
        a ball is never foreseen in one."""
        alive = [r for r in self.rackets if not r.dead]
        if not alive:
            return
        self.lure = [(b.x + BW / 2, b.y) for b in self.bricks(box)]
        flights = {b.n: self._foresee(b, box, walls) for b in self.balls}
        reach: dict[Racket, tuple[int, dict[Cell, int]]] = {}

        def ways(r: Racket, depth: int) -> dict[Cell, int]:
            if r not in reach or reach[r][0] < depth:
                reach[r] = (depth, self._reach(r, self._free(box, r.w), depth))
            return reach[r][1]

        def way(r: Racket, goal: Cell, depth: int) -> list[Cell]:
            """From `r` to `goal`, less where it is: along the rows
            first, to be on the row of a ball before the ball."""
            dist = ways(r, depth)
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
                (x + i, y) in taken for x, y in plan.way[:MOVES] for i in range(r.w)
            ):
                # A ball or a racket in the way: another way, if in time.
                depth = MOVES * (plan.step - self.steps)
                if plan.place in ways(r, depth):
                    plan.way = way(r, plan.place, depth)
                else:
                    r.plan = None
        goals: dict[Racket, list[Cell]] = {}
        slack: dict[Racket, int] = {}  # moves in hand of one with a ball to stop
        minded = set()  # balls a racket goes for
        threats = [f for f in flights.values() if f.side is not None]
        threats.sort(key=lambda f: (f.path[-1][0], f.ball.n))
        # One search a racket: as far as the last of them.
        depth = int(MOVES * threats[-1].path[-1][0]) + SPARE if threats else 0
        for flight in threats:
            team = 0 if self.teams == 1 or flight.side < 0 else 1
            free = [r for r in alive if r.team == team and r not in goals]
            keeper = [r for r in free if r.plan and r.plan.ball == flight.ball.n]
            if keeper:
                r, plan = keeper[0], keeper[0].plan
                goals[r] = plan.way
                slack[r] = MOVES * (plan.step - self.steps) - len(plan.way)
                minded.add(flight.ball.n)
                continue
            plans = []
            for r in free:
                found = self._intercept(r, flight, ways(r, depth), box, walls)
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
        for r in alive:
            away = r.away > self.steps
            if r in goals:
                # Time in hand to stop its ball: a capsule on the way.
                spare = slack[r] - SPARE
                if away or not good or spare < 2 or danger.intersection(r.cells):
                    continue
                dist = ways(r, spare // 2)
                spots = [
                    (dist[c], c)
                    for cap in good
                    for y in range(cap.y, cap.y + 4)
                    for x in range(cap.x - r.w + 1, cap.x + 1)
                    if (c := (x, y)) in dist and 2 * dist[c] <= spare
                ]
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
                dist = ways(r, FETCH)
                spots = [
                    (dist[c], c)
                    for cap in good
                    for y in range(cap.y, cap.y + 4)
                    for x in range(cap.x - r.w + 1, cap.x + 1)
                    if (c := (x, y)) in dist
                ]
                if spots:
                    goals[r], r.plan = way(r, min(spots)[1], FETCH), None
                    continue
            if r.plan is not None and r.plan.ball in minded:
                r.plan = None
            if r.plan is None and self.steps >= r.think and not away:
                r.think = self.steps + THINK
                depth = int(MOVES * CHASE)
                rest = [f for f in flights.values() if f.ball.n not in minded]
                found = self._chase(r, rest, ways(r, depth), box, walls)
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
            for x, y in path[:MOVES]:
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
            or len(plan.way) > MOVES * k
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
        of a rank, the one worth the most (`_shot`). None in time: the
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
            due = MOVES * k - 1
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
        for _, i, spots in [f for f in found[:3] if f[0] == found[0][0]]:
            for c in spots:
                worth = path[-i][5] + self._shot(r, flight, -i, c, box, walls)
                worth -= HIDE * self._hidden(c, r.w, walls)
                key = (dist[c] / 100 - worth, c)
                if best is None or key < best[0]:
                    best = (key, -i, c)
        assert best is not None
        _, i, goal = best
        return dist[goal] - MOVES * path[i + 1][0], goal, i

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
        for flight in sorted(flights, key=near)[:2]:
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
                    if (c := (lx, floor(y))) in dist and dist[c] < MOVES * k - 1
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
            mid = (y0 + y1) / 2
            if self.teams > 1:
                r.guard = 1 if r.team else -1
            elif (max(rows) - mid if r.guard > 0 else mid - min(rows)) < -2:
                r.guard = -r.guard
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
            twins = [r for r in self.rackets if r.team == team and r.left is not None]
            if twins:
                twins[0].left = LAST["twin"]
            else:
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
    )
    frame = _cli.pick(pilot, args.frame)
    # Its own dice: the particles do not change the game of a seed.
    stage = _bricks.Stage(pilot, frame, random.Random(args.seed), args.effects)
    # The walls are read when the driver says the text changed or the
    # view moved (topic `text`), not at each step.
    sight = _cli.Sight(pilot, frame)
    walls: set[Cell] = set()
    # A tick in two is a step; the other puts in place the balls that
    # were shown half way (`Stage.glide`).
    step = False
    for _ in _cli.ticks(0.5 / args.speed, seconds):
        if not sight.look():
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
    args = _cli.parse(out, argv)
    if args.all:
        args.rules = args.bonuses = args.multiplayer = args.effects = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
