# SPDX-License-Identifier: GPL-3.0-or-later
"""Brick: a brick breaker that plays alone, around the words.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app brick
    ptyrc-app brick -r -b            # rules and bonuses
    ptyrc-app brick -a               # the arcade

What it shows:

- The screen read as a map: glyphs are walls, the ball bounces off
  them, off the sides of the view and off the bricks. The top and the
  bottom of the view are open.
- A racket that moves in two dimensions and is hit on either side: it
  foresees where the ball leaves the view and gets in its way, above or
  below it, on a spot from which the ball goes to a brick.
- Terrain in frame cells: bricks hang under the lines of text, at times
  stand on them. They are remembered: scroll away and back, they are as
  they were. A view short of bricks gets a new area.

The game never ends. A ball that leaves the view sets the score to 0.

More, each on its own flag; `-a` for all (see `_bricks`):

- `-r`: rules. Bricks that take three hits; loose ones that fall when
  touched and kill the racket under them; bumpers that send the ball
  away faster, anywhere; bombs that take their neighbours with them.
- `-b`: bonuses. A broken brick may drop a capsule; the racket that
  touches it gets a wide racket, a twin, three balls for one, or 5 s
  of fire: the ball goes through the bricks. Two are bad, and no
  racket goes for them: a small racket, a faster ball.
- `-m`: two rackets, two scores. One keeps the top, the other the
  bottom; a brick is worth points to the last that hit the ball.
- `-e`: particles, a trail, a best score.

Rules count in steps of the game, not in seconds.
"""

from __future__ import annotations

import argparse
import random
from collections import deque
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from math import floor
from typing import Any

from ptyrc import Frame, Pilot

from . import _bricks, _cli
from .snake import walls_of

__all__ = [
    "BONUSES",
    "BW",
    "KINDS",
    "LOW",
    "RACKET",
    "SMALL",
    "WIDE",
    "Ball",
    "Brick",
    "Capsule",
    "Faller",
    "Game",
    "Racket",
    "fly",
    "main",
]

Cell = tuple[int, int]
Box = tuple[int, int, int, int]
Solid = Callable[[Cell], bool]
State = tuple[float, float, float, float]  # x, y, vx, vy
STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))

BW = 4  # cells of a brick
SMALL, RACKET, WIDE = 3, 5, 7  # cells of a racket
VY = 0.5  # rows a step
VX = 0.9  # columns a step, at most
MOVES = 1.5  # cells a racket moves in a step: 3 in 2
LOW = 14  # bricks in the view under which an area is spawned
SPAWN_EVERY = 40  # steps between two areas, at least
SERVE = 12  # steps before a new ball
FORESEE = 90  # steps a racket looks ahead
SPARE = 6  # moves a racket likes to have in hand
AIM = 50  # steps of a shot it weighs
QUIET = 240  # steps without a brick: the ball is nudged; thrice: served again
POINTS = 10  # a brick
COLORS = 7  # of the bricks: indices in a palette
#: Kinds of bricks with `-r`: lives and share.
KINDS = {
    "plain": (1, 0.66),
    "hard": (3, 0.14),
    "loose": (1, 0.08),
    "bumper": (2, 0.05),
    "bomb": (1, 0.07),
}
BONUSES = ("wide", "small", "twin", "multi", "fast", "fire")
BAD = ("small", "fast")  # no racket goes for these
DROP = 1 / 7  # share of the broken bricks that drop a capsule
BALLS = 5  # at once
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

    @property
    def cells(self) -> list[Cell]:
        return [(self.x + i, self.y) for i in range(BW)]


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


class Game:
    """Bricks, balls, rackets and the rules. No drawing here.

    `rules`: kinds of bricks; `bonuses`: capsules; `multiplayer`: two
    teams; `second`: steps in a second. Each step leaves in `events`
    what happened, a cell second: `("break", cell, brick, team)`,
    `("hit", cell, brick)`, `("bump", cell)`, `("boom", cell)`,
    `("crash", cell)` (a faller on the ground), `("kill", cell, racket)`,
    `("lost", cell, team)` (a ball out, the team's score at 0),
    `("serve", cell)`, `("catch", cell, kind, team)`, `("area", bricks)`.
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
        self.spawn_due = 0
        self.lure: list[float] = []  # columns of the bricks in view
        self.serve_due = SERVE
        self.server = rng.randrange(self.teams)

    # -- the terrain -------------------------------------------------------

    def bricks(self, box: Box) -> list[Brick]:
        """Those of the rows of `box`."""
        return [b for y in range(box[1], box[3]) for b in self.rows.get(y, ())]

    def _number(self) -> int:
        self.serial += 1
        return self.serial

    def _lay(self, brick: Brick) -> None:
        self.rows.setdefault(brick.y, []).append(brick)
        for cell in brick.cells:
            self.solid[cell] = brick

    def _lift(self, brick: Brick) -> bool:
        """Take a brick off the terrain; False when it was not on it."""
        if self.solid.get((brick.x, brick.y)) is not brick:
            return False
        self.rows[brick.y].remove(brick)
        for cell in brick.cells:
            del self.solid[cell]
        return True

    def _area(self, box: Box, walls: set[Cell]) -> list[Brick]:
        """Lay a new area: a block of bricks on free cells of the view.
        Most hang under a line of text, some stand on one, the others
        float; two rows free of text on an open side."""
        rng = self.rng
        x0, y0, x1, y1 = box
        keep = {c for r in self.rackets if not r.dead for c in r.cells}
        keep |= {(x, y + dy) for x, y in list(keep) for dy in (-2, -1, 1, 2)}
        keep |= {b.at for b in self.balls}
        found: dict[str, list[tuple[int, int, int, int, bool]]] = {}
        for _ in range(120):
            cols, rows = rng.randint(2, 8), rng.randint(1, 4)
            if x1 - x0 < cols * BW + 2 or y1 - y0 < rows + 2:
                continue
            x = rng.randrange(x0 + 1, x1 - cols * BW)
            y = rng.randrange(y0 + 1, y1 - rows)
            span = range(x, x + cols * BW)
            if any(
                c in walls or c in self.solid or c in keep
                for yy in range(y, y + rows)
                for c in ((xx, yy) for xx in span)
            ):
                continue

            # Room for a ball and a racket: two rows free of text.
            above, below = (
                not any((xx, yy) in walls for yy in side for xx in span)
                for side in (range(y - 2, y), range(y + rows, y + rows + 2))
            )
            if any((xx, y - 1) in walls for xx in span):
                where, open_ = "under", below
            elif any((xx, y + rows) in walls for xx in span):
                where, open_ = "over", above
            else:
                where, open_ = "free", above or below
            if not open_:
                continue
            found.setdefault(where, []).append((x, y, cols, rows, where == "over"))
        if not found:
            return []
        dice = rng.random()
        order = (
            ("under", "over", "free")
            if dice < 0.65
            else ("over", "under", "free")
            if dice < 0.85
            else ("free", "under", "over")
        )
        place = rng.choice(next(found[w] for w in order if w in found))
        x, y, cols, rows, up = place
        shape = rng.choice(("full", "full", "checker", "frame", "slope"))
        tint = rng.randrange(COLORS)
        out = []
        for j in range(rows):
            d = rows - 1 - j if up else j  # rows from the text: 0 is whole
            for i in range(cols):
                if d and (
                    (shape == "checker" and (i + d) % 2)
                    or (shape == "frame" and 0 < i < cols - 1 and d < rows - 1)
                    or (shape == "slope" and not d <= i < cols - d)
                ):
                    continue
                kind, lives = "plain", 1
                if self.rules:
                    kind = rng.choices(
                        list(KINDS), [share for _, share in KINDS.values()]
                    )[0]
                    lives = KINDS[kind][0]
                brick = Brick(
                    self._number(), x + i * BW, y + j, kind, lives, (tint + j) % COLORS
                )
                self._lay(brick)
                out.append(brick)
        self.events.append(("area", out))
        return out

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
                inside(c) and c not in walls and c not in self.solid
                for c in racket.cells
            ):
                racket.dead = 1
        if len(self.bricks(box)) < LOW and self.steps >= self.spawn_due:
            # The first view is filled at once.
            for _ in range(3 if self.spawn_due == 0 else 1):
                self._area(box, walls)
            self.spawn_due = self.steps + SPAWN_EVERY
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

    def _blocked(self, but: Racket | None = None) -> set[Cell]:
        """Cells a racket cannot move to: the other rackets, and two
        cells each side of a ball, not to push it with an end."""
        out = {(b.at[0] + dx, b.at[1]) for b in self.balls for dx in range(-2, 3)}
        for other in self.rackets:
            if other is not but and not other.dead:
                out.update(other.cells)
        return out

    def _room(self, box: Box, walls: set[Cell], w: int, taken: set[Cell]) -> set[Cell]:
        """Left cells of the places where a racket of `w` cells fits."""
        x0, y0, x1, y1 = box
        out = set()
        solid = self.solid
        for y in range(y0, y1):
            run = 0
            for x in range(x0, x1):
                c = (x, y)
                run = 0 if c in walls or c in solid or c in taken else run + 1
                if run >= w:
                    out.add((x - w + 1, y))
        return out

    def _wake(self, box: Box, walls: set[Cell]) -> None:
        """Timers of the rackets; one away comes back near its home: the
        middle of the view, or the team's half of it."""
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
                    racket.span = 0 if self._size(racket, RACKET, box, walls) else 1
            if not racket.dead:
                continue
            racket.dead -= 1
            if racket.dead:
                continue
            racket.w, racket.span = RACKET, 0
            room = self._room(box, walls, racket.w, self._blocked(racket))
            share = (2 * racket.team + 1) / (2 * self.teams)
            hx = (x0 + x1 - racket.w) // 2
            hy = y0 + int((y1 - y0) * share)
            if racket.left is not None:  # a twin: off its team's racket
                hy += (y1 - y0) // 4 * (1 if racket.n % 2 else -1)
            if not room:
                racket.dead = 4
                continue
            racket.x, racket.y = min(
                room, key=lambda c: (4 * abs(c[1] - hy) + abs(c[0] - hx), c)
            )

    def _serve(self, box: Box, walls: set[Cell]) -> None:
        """A ball when there is none: from a side of a racket, of the
        team that lost the last one first; from the nearest free cell
        when the racket is between two lines."""
        if self.balls:
            self.serve_due = self.steps + SERVE
            return
        rackets = [r for r in self.rackets if not r.dead]
        if self.steps < self.serve_due or not rackets:
            return
        rng = self.rng
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
            for dy in rng.sample((-1, 1), 2):
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
        vx = rng.choice((-1, 1)) * rng.uniform(0.3, VX)
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

    def _moves(self, ball: Ball, step: int) -> int:
        """Moves of `ball` in step `step`: a fast one, three in two."""
        return 2 if ball.fast and step % 2 else 1

    def _roll(self, ball: Ball, box: Box, walls: set[Cell]) -> None:
        y0, y1 = box[1], box[3]
        terrain = self._terrain(box, walls, bool(ball.fire))
        rackets = {c: r for r in self.rackets if not r.dead for c in r.cells}
        for _ in range(self._moves(ball, self.steps)):
            ball.x, ball.y, ball.vx, ball.vy, hits = fly(
                ball.x, ball.y, ball.vx, ball.vy, lambda c: terrain(c) or c in rackets
            )
            for cell, flat in hits:
                racket = rackets.get(cell)
                if racket is not None:
                    ball.owner = racket.team
                    if flat:  # the angle is the racket's
                        half = racket.w / 2
                        off = (ball.x - racket.x - half) / half
                        ball.vx = max(-VX, min(VX, off * VX))
                elif cell in self.solid:
                    self._strike(self.solid[cell], ball, cell)
            if ball.fire and ball.at in self.solid:
                self._smash(self.solid[ball.at], ball.owner, ball.at)
            if not y0 <= ball.at[1] < y1:
                self._out(ball, box)
                return
        ball.fire, ball.fast = max(0, ball.fire - 1), max(0, ball.fast - 1)
        ball.quiet += 1
        if ball.quiet % QUIET == 0:
            # Going round in circles: another angle; in a pocket: gone.
            if ball.quiet >= 3 * QUIET:
                self.balls.remove(ball)
            else:
                sign = self.rng.choice((-1, 1))
                ball.vx = sign * self.rng.uniform(0.3, VX)

    def _out(self, ball: Ball, box: Box) -> None:
        """Out by the top or the bottom: the score of who keeps that
        side is lost."""
        self.balls.remove(ball)
        top = ball.at[1] < box[1]
        team = 0 if self.teams == 1 or top else 1
        self.score[team] = 0
        self.server = team
        at = (ball.at[0], box[1] if top else box[3] - 1)
        self.events.append(("lost", at, team))

    def _strike(self, brick: Brick, ball: Ball, cell: Cell) -> None:
        """A ball bounced off `brick`."""
        ball.quiet = 0
        if brick.kind == "loose":
            self._lift(brick)
            self.fallers.append(Faller(brick.n, brick.x, brick.y, brick.color))
            self.score[ball.owner] += POINTS
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
            self._smash(brick, ball.owner, cell)

    def _smash(self, brick: Brick, team: int, cell: Cell) -> None:
        """Break `brick`, and what its blast reaches when it is a bomb."""
        queue = deque([(brick, cell)])
        while queue:
            brick, cell = queue.popleft()
            if not self._lift(brick):
                continue
            self.score[team] += POINTS
            self.events.append(("break", cell, brick, team))
            if brick.kind == "bomb":
                at = (brick.x + BW // 2, brick.y)
                self.events.append(("boom", at))
                near = {
                    self.solid[c]
                    for y in (brick.y - 1, brick.y, brick.y + 1)
                    for x in range(brick.x - BW, brick.x + 2 * BW)
                    if (c := (x, y)) in self.solid
                }
                queue.extend(
                    (b, (b.x + BW // 2, b.y)) for b in sorted(near, key=lambda b: b.n)
                )
            elif self.bonuses and self.rng.random() < DROP:
                kind = self.rng.choice(BONUSES)
                self.capsules.append(Capsule(self._number(), *cell, kind))

    # -- the rackets -------------------------------------------------------

    def _foresee(
        self, ball: Ball, box: Box, solid: Solid, steps: int = FORESEE
    ) -> tuple[list[tuple[int, State]], int, int | None, bool]:
        """Where `ball` goes with no racket in its way: its state after
        each move with the step it is made in, counted from this one;
        the index of its last bounce; the side it leaves by (-1 top, 1
        bottom) or None; whether it met a brick."""
        y0, y1 = box[1], box[3]
        state: State = (ball.x, ball.y, ball.vx, ball.vy)
        path = [(0, state)]
        last, fast, met = 0, ball.fast, False
        bricks = self.solid
        for k in range(1, steps + 1):
            for _ in range(2 if fast and (self.steps + k - 1) % 2 else 1):
                *state, hits = fly(*state, solid)
                path.append((k, state))
                if hits:
                    last = len(path) - 1
                    met = met or any(c in bricks for c, _ in hits)
                if not y0 <= floor(state[1]) < y1:
                    return path, last, (-1 if state[1] < y0 else 1), met
            fast = max(0, fast - 1)
        return path, last, None, met

    def _shot(
        self,
        ball: Ball,
        racket: Racket,
        place: Cell,
        before: State,
        after: State,
        box: Box,
        terrain: Solid,
    ) -> float:
        """Worth of `place` to hit `ball` from, `before` and `after`
        being its states around the move that enters the row."""
        lx, row = place
        half = racket.w / 2
        vx = max(-VX, min(VX, (after[0] - lx - half) / half * VX))
        ghost = Ball(0, after[0], before[1], vx, -before[3])
        ghost.fast, ghost.fire = ball.fast, ball.fire

        def solid(c: Cell) -> bool:
            return terrain(c) or (c[1] == row and lx <= c[0] < lx + racket.w)

        path, _, side, met = self._foresee(ghost, box, solid, AIM)
        if met:  # a brick: the sooner the better
            return 1000 - len(path)
        if side is None:
            return 0
        if self.teams > 1 and (side > 0) != (racket.team > 0):
            return 500 - len(path)  # out by the other's side
        # Out with no brick met: towards the column of the nearest one.
        x = path[-1][1][0]
        return -1000 - min((abs(x - at) for at in self.lure), default=0)

    def _steer(self, box: Box, walls: set[Cell]) -> None:
        """Each racket chooses a place and moves towards it."""
        alive = [r for r in self.rackets if not r.dead]
        if not alive:
            return
        self.lure = [b.x + BW / 2 for b in self.bricks(box)]
        terrain = self._terrain(box, walls)
        burning = self._terrain(box, walls, True)
        threats = []
        for ball in self.balls:
            hard = burning if ball.fire else terrain
            path, last, side, _ = self._foresee(ball, box, hard)
            if side is not None:
                threats.append((path[-1][0], ball.n, ball, path, last, side, hard))
        threats.sort(key=lambda t: t[:2])
        reach: dict[Racket, tuple[dict[Cell, int], dict[Cell, Cell], set[Cell]]] = {}

        def ways(r: Racket) -> tuple[dict[Cell, int], dict[Cell, Cell], set[Cell]]:
            """Moves from `r` to each place it fits, the way back, the
            places."""
            if r not in reach:
                room = self._room(box, walls, r.w, self._blocked(r))
                dist, back = {(r.x, r.y): 0}, {}
                queue = deque(dist)
                while queue:
                    here = queue.popleft()
                    for dx, dy in STEPS:
                        there = (here[0] + dx, here[1] + dy)
                        if there in room and there not in dist:
                            dist[there] = dist[here] + 1
                            back[there] = here
                            queue.append(there)
                reach[r] = (dist, back, room)
            return reach[r]

        goals: dict[Racket, Cell] = {}
        for _, _, ball, path, last, side, hard in threats:
            team = 0 if self.teams == 1 or side < 0 else 1
            free = [r for r in alive if r.team == team and r not in goals]
            plans = []
            for r in free:
                plan = self._intercept(r, ball, path, last, ways(r)[0], box, hard)
                if plan is not None:
                    plans.append((plan, r.n, r))
            if plans:
                (_, goal), _, r = min(plans, key=lambda p: p[:2])
                goals[r] = goal
        danger = {
            (x, y)
            for f in self.fallers
            for x in range(f.x, f.x + BW)
            for y in range(f.y, f.y + WARN)
        }
        for r in alive:
            if r in goals:
                continue
            if danger.intersection(r.cells):
                dist = ways(r)[0]
                safe = (
                    c
                    for c in dist
                    if not any((c[0] + i, c[1]) in danger for i in range(r.w))
                )
                goal = next(safe, None)  # in the order found: the nearest
                if goal is not None:
                    goals[r] = goal
            elif self.capsules:
                dist = ways(r)[0]
                spots = [
                    (dist[c], c)
                    for cap in self.capsules
                    if cap.kind not in BAD
                    for y in range(cap.y, cap.y + 4)
                    for x in range(cap.x - r.w + 1, cap.x + 1)
                    if (c := (x, y)) in dist
                ]
                if spots:
                    goals[r] = min(spots)[1]
        moves = int(self.steps * MOVES) - int((self.steps - 1) * MOVES)
        for r, goal in goals.items():
            dist, back, _ = ways(r)
            way = [goal]
            while way[-1] in back:
                way.append(back[way[-1]])
            taken = self._blocked(r)
            for x, y in way[-2::-1][:moves]:
                if any((x + i, y) in taken for i in range(r.w)):
                    break
                r.x, r.y = x, y

    def _intercept(
        self,
        r: Racket,
        ball: Ball,
        path: list[tuple[int, State]],
        last: int,
        dist: dict[Cell, int],
        box: Box,
        terrain: Solid,
    ) -> tuple[int, Cell] | None:
        """Where `r` gets in the way of `ball` before it leaves: on a
        row the ball enters, under it or over it: the latest row `r`
        reaches in time, one of the last leg with `SPARE` moves to
        spare if any; else the row where it is the least late. Of the
        places on that row, the best shot. Returns `(lateness, place)`."""
        # Rows entered, but for the last move: that one is out of the view.
        entries = [
            i
            for i in range(len(path) - 2)
            if floor(path[i + 1][1][1]) != floor(path[i][1][1])
        ]
        found = []  # (rank, entry, places in time)
        late = []  # (moves short, entry, place)
        for i in entries:
            k, (x, y, _, _) = path[i + 1]
            spots = [(lx, floor(y)) for lx in range(floor(x) - r.w + 1, floor(x) + 1)]
            spots = [c for c in spots if c in dist]
            if not spots:
                continue
            due = int(MOVES * k) - 1
            timely = [c for c in spots if dist[c] <= due]
            if timely:
                # The last leg first, with moves to spare; the latest row.
                spare = max(due - dist[c] for c in timely)
                rank = 2 if i < last else spare < SPARE
                found.append(((rank, -i), i, timely))
            else:
                c = min(spots, key=lambda c: (dist[c], c))
                late.append((dist[c] - due, -i, c))
        if not found:
            return min(late)[::2] if late else None
        _, i, spots = min(found)
        before, (k, after) = path[i][1], path[i + 1]
        goal = min(
            spots,
            key=lambda c: (
                dist[c] / 100 - self._shot(ball, r, c, before, after, box, terrain),
                c,
            ),
        )
        return (dist[goal] - int(MOVES * k), goal)

    # -- rules and bonuses -------------------------------------------------

    def _fall(self, box: Box, walls: set[Cell]) -> None:
        """Loose bricks fall a row every other step: onto the ground,
        out of the view, or on a racket."""
        if self.steps % 2:
            return
        rackets = {c: r for r in self.rackets if not r.dead for c in r.cells}
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
        rackets = {c: r for r in self.rackets if not r.dead for c in r.cells}
        for cap in list(self.capsules):
            at = (cap.x, cap.y)
            if at in rackets:
                self.capsules.remove(cap)
                self._award(cap, rackets[at], box, walls)
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

    def _size(self, r: Racket, w: int, box: Box, walls: set[Cell]) -> bool:
        """Make `r` `w` cells wide, about its middle; False when a wider
        one does not fit."""
        mid = r.x + (r.w - w) // 2
        if w > r.w:
            room = self._room(box, walls, w, self._blocked(r))
            fits = [x for x in range(r.x - w + r.w, r.x + 1) if (x, r.y) in room]
            if not fits:
                return False
            mid = min(fits, key=lambda x: (abs(x - mid), x))
        r.x, r.w = mid, w
        return True

    def _award(self, cap: Capsule, racket: Racket, box: Box, walls: set[Cell]) -> None:
        team = racket.team
        self.score[team] += POINTS
        self.events.append(("catch", (cap.x, cap.y), cap.kind, team))
        if cap.kind in ("wide", "small"):
            for r in [r for r in self.rackets if r.team == team and not r.dead]:
                w = WIDE if cap.kind == "wide" else SMALL
                if self._size(r, w, box, walls):
                    r.span = LAST[cap.kind]
        elif cap.kind == "twin":
            twins = [r for r in self.rackets if r.team == team and r.left is not None]
            if twins:
                twins[0].left = LAST["twin"]
            else:
                self.rackets.append(Racket(self._number(), team, left=LAST["twin"]))
        elif cap.kind == "multi":
            for ball in list(self.balls):
                for turn in (-0.45, 0.45):
                    if len(self.balls) >= BALLS:
                        break
                    vx = max(-VX, min(VX, ball.vx + turn))
                    twin = Ball(self._number(), ball.x, ball.y, vx, ball.vy, ball.owner)
                    twin.fire, twin.fast = ball.fire, ball.fast
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
    for _ in _cli.ticks(1.0 / args.speed, seconds):
        if not sight.look():
            continue
        box = view(frame)
        if sight.changed:
            walls = walls_of(sight.rows, box[0], box[1])
        game.step(box, walls)
        with sight.drawing(), pilot.batch():
            stage.show(game, box)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "brick", "A brick breaker that plays alone around the words.", frame="largest"
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
        help="capsules: wide, small, twin, three balls, fast, fire",
    )
    out.add_argument(
        "-m", "--multiplayer", action="store_true", help="two rackets, two scores"
    )
    out.add_argument(
        "-e", "--effects", action="store_true", help="particles, a trail, a best score"
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
