# SPDX-License-Identifier: GPL-3.0-or-later
"""Blocks: falling pieces in braille, played alone between the words.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app blocks
    ptyrc-app blocks --sideways      # from the right, lines are columns
    ptyrc-app blocks -a              # chains, particles, a score
    ptyrc-app blocks --cursor        # the nearest piece follows the cursor

What it shows:

- The screen read as a map: every glyph of the pane is a wall, and so
  is the blank between two words. A line is a run of free cells
  between two walls or sides of the view; full of blocks, it goes and
  what stands on it comes down.
- Braille as sub-cells: a block is a cell, `⣿`; a piece falls a dot at
  a time, four dots a row (two a column with `--sideways`).
- Frame coordinates: the heap belongs to the text and scrolls with it.
  Blocks scrolled out of sight, or written on, are gone.
- One sprite for the heap, one a piece, each redrawn in place with
  `Sprite.update`.

The computer plays: each piece looks for the best place it can reach
(a breadth-first search over its positions and turns) and goes there.
The game never ends: a heap that reaches the entry crumbles, and the
score is 0 again.

More, each on its own flag; `-a` for the first three (see `_blocks`):

- `--chains`: a broken block breaks the blocks of its colour that
  touch it, one step later, and so on from block to block.
- `--effects`: a broken block is a particle.
- `--score`: the score is shown; the higher, the faster.
- `--cursor`: the piece nearest to the cursor lands under it (beside
  it with `--sideways`); the computer turns it.

Rules count in steps of the game, not in seconds: a step is a dot.
"""

from __future__ import annotations

import argparse
import random
from collections import deque
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from typing import Any

from ptyrc import Frame, Pilot

from . import _blocks, _cli
from .snake import walls_of

__all__ = [
    "FAST",
    "GAP",
    "INK",
    "RAMP",
    "SHAPES",
    "Game",
    "Piece",
    "braille",
    "main",
    "turns",
]

Cell = tuple[int, int]
Box = tuple[int, int, int, int]
Color = int | str
Run = tuple[int, int, int]  # depth, first cell across, end
STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))
GAP = 3  # cells a piece falls before the next comes
RAMP = 150  # points that add the base speed once, with `--score`
FAST = 3.0  # times the base speed, at most
DOTS = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))  # [y][x]

INK: dict[str, Color] = {
    "I": "cyan",
    "O": "yellow",
    "T": "magenta",
    "S": "green",
    "Z": "red",
    "J": "blue",
    "L": 208,
}


def turns(cells: Iterable[Cell]) -> tuple[tuple[Cell, ...], ...]:
    """The distinct quarter turns of a piece, each centred in the
    square that holds them all."""
    cells = sorted(cells)
    out: list[tuple[Cell, ...]] = []
    seen: set[tuple[Cell, ...]] = set()
    for _ in range(4):
        w = max(a for a, _ in cells) + 1
        h = max(b for _, b in cells) + 1
        if tuple(cells) not in seen:
            seen.add(tuple(cells))
            n = max(w, h)
            ox, oy = (n - w) // 2, (n - h) // 2
            out.append(tuple((a + ox, b + oy) for a, b in cells))
        cells = sorted((h - 1 - b, a) for a, b in cells)
    return tuple(out)


SHAPES = {
    "I": turns([(0, 0), (1, 0), (2, 0), (3, 0)]),
    "O": turns([(0, 0), (1, 0), (0, 1), (1, 1)]),
    "T": turns([(0, 0), (1, 0), (2, 0), (1, 1)]),
    "S": turns([(1, 0), (2, 0), (0, 1), (1, 1)]),
    "Z": turns([(0, 0), (1, 0), (1, 1), (2, 1)]),
    "J": turns([(0, 0), (0, 1), (1, 1), (2, 1)]),
    "L": turns([(2, 0), (0, 1), (1, 1), (2, 1)]),
}


def braille(dots: Iterable[Cell]) -> tuple[Cell, list[str]]:
    """Dots, 2 x 4 a cell, as rows of braille: the top-left cell and
    the rows; a cell without a dot is a space."""
    cells: dict[Cell, int] = {}
    for x, y in dots:
        key = (x >> 1, y >> 2)
        cells[key] = cells.get(key, 0) | DOTS[y & 3][x & 1]
    left = min(x for x, _ in cells)
    top = min(y for _, y in cells)
    right = max(x for x, _ in cells)
    rows = [
        "".join(
            chr(0x2800 | cells[x, y]) if (x, y) in cells else " "
            for x in range(left, right + 1)
        )
        for y in range(top, max(y for _, y in cells) + 1)
    ]
    return (left, top), rows


@dataclass(eq=False)
class Piece:
    """A falling piece. `u` across, `v` along the fall, `r` its turn;
    `phase` dots into the next cell. `plan`: the moves left, `L`, `R`,
    `T` (a turn), `D` (a cell down); `target`: where it will rest.
    `goal`: where the cursor wants it, across."""

    n: int  # serial
    kind: str
    color: Color
    u: int
    v: int
    r: int = 0
    phase: int = 0
    plan: list[str] = field(default_factory=list)
    target: frozenset[Cell] = frozenset()
    stamp: int = -1  # of the world the plan was made for
    goal: int | None = None


class Game:
    """The pieces, the heap and the rules. No drawing here.

    Inside, cells are `(u, v)`: `v` grows along the fall. Upright,
    `(u, v) = (x, y)`; `sideways`, `(u, v) = (y, -x)`. What goes out
    is in frame cells: `blocks`, `picture`, `heap_picture`, `events`.

    Each step leaves in `events` what happened: `("break", cell,
    color, how)` with `how` `"line"` or `"chain"`; `("lost", cells,
    color)`, a piece that text or the heap took the place of; `("over",
    [(cell, color), ...])`, the heap gone.

    `rev` changes when the heap does."""

    def __init__(
        self,
        rng: random.Random,
        *,
        sideways: bool = False,
        chains: bool = False,
        count: int = 3,
    ) -> None:
        self.rng = rng
        self.sideways, self.chains, self.count = sideways, chains, count
        self.period = 2 if sideways else 4  # dots a cell, along the fall
        self.heap: dict[Cell, Color] = {}
        self.pieces: list[Piece] = []
        self.wave: dict[Cell, Color] = {}  # broken in the last step
        self.holes: set[Cell] = set()  # broken, kept free until the heap comes down
        self.events: list[tuple[Any, ...]] = []
        self.score = self.lines = self.steps = self.serial = 0
        self.rev = 0
        self.box: Box = (0, 0, 0, 0)
        self.walls: set[Cell] = set()
        self._text: set[Cell] | None = None  # the walls as given
        self._stamp = 0
        self._open: set[Cell] | None = None
        self._runs: dict[Cell, Run] = {}
        self._full: dict[Run, int] = {}
        self._bag: list[str] = []
        self._due = 0

    # Frame cells and back.

    def _in(self, c: Cell) -> Cell:
        return (c[1], -c[0]) if self.sideways else c

    def _out(self, c: Cell) -> Cell:
        return (-c[1], c[0]) if self.sideways else c

    @property
    def blocks(self) -> dict[Cell, Color]:
        """The heap: frame cell, colour."""
        return {self._out(c): color for c, color in self.heap.items()}

    def cells(self, piece: Piece) -> list[Cell]:
        """Frame cells of `piece`, less its phase."""
        return [self._out(c) for c in self._at(piece.kind, piece.u, piece.v, piece.r)]

    def picture(self, piece: Piece) -> tuple[Cell, list[list[dict[str, Any]]]]:
        """A piece as a sprite: its top-left cell and rows of runs."""
        dots = []
        for x, y in self.cells(piece):
            dx, dy = (-piece.phase, 0) if self.sideways else (0, piece.phase)
            dots += [
                (2 * x + i + dx, 4 * y + j + dy) for i in range(2) for j in range(4)
            ]
        at, rows = braille(dots)
        style = {"fg": piece.color}
        if piece.goal is not None:
            style["bold"] = True
        return at, [[{"t": row, **style}] for row in rows]

    def heap_picture(self) -> tuple[Cell, list[list[dict[str, Any]]]] | None:
        """The heap as a sprite; None when there is none."""
        blocks = self.blocks
        if not blocks:
            return None
        left, top = min(x for x, _ in blocks), min(y for _, y in blocks)
        right, bottom = max(x for x, _ in blocks), max(y for _, y in blocks)
        rows = []
        for y in range(top, bottom + 1):
            runs: list[dict[str, Any]] = []
            for x in range(left, right + 1):
                color = blocks.get((x, y))
                glyph = " " if color is None else "⣿"
                if runs and runs[-1].get("fg") == color:
                    runs[-1]["t"] += glyph
                elif color is None:
                    runs.append({"t": glyph})
                else:
                    runs.append({"t": glyph, "fg": color})
            rows.append(runs)
        return (left, top), rows

    # The world.

    def _touch(self) -> None:
        """The heap or the walls changed: plans are void."""
        self._stamp += 1
        self._open = None
        self._full = {}

    @property
    def open(self) -> set[Cell]:
        """Cells a piece may be in."""
        if self._open is None:
            u0, v0, u1, v1 = self.box
            cells = {(u, v) for v in range(v0, v1) for u in range(u0, u1)}
            cells -= self.walls
            cells -= self.holes
            cells.difference_update(self.heap)
            self._open = cells
        return self._open

    def _see(self, box: Box, walls: set[Cell]) -> None:
        if self.sideways:
            x0, y0, x1, y1 = box
            box = (y0, 1 - x1, y1, 1 - x0)
        if box == self.box and walls == self._text:
            return
        self._text = walls
        # The blank between two words is no place for a line: the words
        # of a sentence are one wall.
        walls = walls | {(x + 1, y) for x, y in walls if (x + 2, y) in walls}
        self.walls = {(y, -x) for x, y in walls} if self.sideways else walls
        self.box = box
        self._runs = {}
        u0, v0, u1, v1 = box

        def gone(c: Cell) -> bool:
            return not (u0 <= c[0] < u1 and v0 <= c[1] < v1) or c in self.walls

        drop = [c for c in self.heap if gone(c)]
        for c in drop:
            del self.heap[c]
        self.rev += bool(drop)
        self.holes = {c for c in self.holes if not gone(c)}
        self._touch()

    def _run(self, c: Cell) -> Run:
        """The line of a free cell: from wall to wall, across."""
        run = self._runs.get(c)
        if run is None:
            u0, _, u1, _ = self.box
            u, v = c
            a, b = u, u + 1
            while a > u0 and (a - 1, v) not in self.walls:
                a -= 1
            while b < u1 and (b, v) not in self.walls:
                b += 1
            run = (v, a, b)
            for i in range(a, b):
                self._runs[i, v] = run
        return run

    def _filled(self, run: Run) -> int:
        """Blocks in a line."""
        n = self._full.get(run)
        if n is None:
            v, a, b = run
            n = self._full[run] = sum((u, v) in self.heap for u in range(a, b))
        return n

    # The rules.

    def step(self, box: Box, walls: set[Cell], cursor: Cell | None = None) -> None:
        """One dot of fall inside `box` (x0, y0, x1, y1: the view, frame
        cells). `cursor`: a frame cell; the nearest piece heads for it."""
        self.events = []
        self.steps += 1
        self._see(box, walls)
        if self.wave:
            self._spread()
        while not self.wave and self.holes:
            self._settle()
            self._clear()
        if len(self.pieces) < self.count and self.steps >= self._due:
            self._enter()
        self._aim(cursor)
        for piece in list(self.pieces):
            self._advance(piece)

    def _break(self, c: Cell, how: str, points: int = 1) -> None:
        color = self.heap.pop(c)
        self.holes.add(c)
        self.score += points
        self.events.append(("break", self._out(c), color, how))

    def _clear(self) -> None:
        """Full lines go. Each block is worth the number of lines that
        go at once."""
        full = {run for run in map(self._run, self.heap)}
        full = sorted(r for r in full if self._filled(r) == r[2] - r[1])
        if not full:
            return
        for v, a, b in full:
            for u in range(a, b):
                if self.chains:
                    self.wave[u, v] = self.heap[u, v]
                self._break((u, v), "line", len(full))
        self.lines += len(full)
        self.rev += 1
        self._touch()

    def _spread(self) -> None:
        """A block broken in the last step breaks those of its colour
        next to it, a point each."""
        hit = {
            (u + du, v + dv): color
            for (u, v), color in self.wave.items()
            for du, dv in STEPS
            if self.heap.get((u + du, v + dv)) == color
        }
        for c in sorted(hit):
            self._break(c, "chain")
        self.wave = hit
        if hit:
            self.rev += 1
            self._touch()

    def _settle(self) -> None:
        """What stands on a broken block comes down one cell."""
        for u, v in sorted(self.holes, key=lambda c: c[1]):
            while (u, v - 1) in self.heap:
                self.heap[u, v] = self.heap.pop((u, v - 1))
                v -= 1
        self.holes = set()
        self.rev += 1
        self._touch()

    def _at(self, kind: str, u: int, v: int, r: int) -> list[Cell]:
        return [(u + a, v + b) for a, b in SHAPES[kind][r]]

    def _fits(self, kind: str, u: int, v: int, r: int) -> bool:
        free = self.open
        return all((u + a, v + b) in free for a, b in SHAPES[kind][r])

    def _enter(self) -> None:
        """A new piece, at the first depth where one fits between the
        walls. No room there for the heap: the game is over, and goes
        on."""
        if not self._bag:
            self._bag = list(SHAPES)
            self.rng.shuffle(self._bag)
        kind = self._bag[-1]
        u0, v0, u1, v1 = self.box
        walls = self.walls
        spots: list[tuple[int, int, int]] = []
        for depth in range(v0, v1):
            for r, shape in enumerate(SHAPES[kind]):
                v = depth - min(b for _, b in shape)
                if max(b for _, b in shape) + v >= v1:
                    continue
                spots += [
                    (u, v, r)
                    for u in range(u0 - 3, u1)
                    if all(
                        u0 <= u + a < u1 and (u + a, v + b) not in walls
                        for a, b in shape
                    )
                ]
            if spots:
                break
        self._due = self.steps + GAP * self.period
        if not spots:
            return
        clear = [s for s in spots if self._fits(kind, *s)]
        if not clear:
            if any(c in self.heap for s in spots for c in self._at(kind, *s)):
                self._over()
            return
        busy = {c for p in self.pieces for c in self._body(p)}
        apart = [s for s in clear if busy.isdisjoint(self._at(kind, *s))]
        u, v, r = self.rng.choice(apart or clear)
        self._bag.pop()
        self.serial += 1
        self.pieces.append(Piece(self.serial, kind, INK[kind], u, v, r))

    def _over(self) -> None:
        heap = sorted(self.heap.items())
        self.events.append(("over", [(self._out(c), color) for c, color in heap]))
        self.heap, self.wave, self.holes = {}, {}, set()
        self.score = 0
        self.rev += 1
        self._touch()

    def _aim(self, cursor: Cell | None) -> None:
        """The piece nearest to the cursor gets its place across."""
        near, goal = None, None
        if cursor is not None and self.pieces:
            cu, cv = self._in(cursor)
            goal = cu
            near = min(
                self.pieces,
                key=lambda p: min(
                    abs(u - cu) + abs(v - cv)
                    for u, v in self._at(p.kind, p.u, p.v, p.r)
                ),
            )
        for piece in self.pieces:
            want = goal if piece is near else None
            if piece.goal != want:
                piece.goal, piece.stamp = want, -1

    def _body(self, piece: Piece) -> list[Cell]:
        """Cells a piece holds: two rows of them between two cells."""
        cells = self._at(piece.kind, piece.u, piece.v, piece.r)
        if piece.phase:
            cells += self._at(piece.kind, piece.u, piece.v + 1, piece.r)
        return cells

    def _moved(self, piece: Piece, move: str) -> tuple[int, int, int]:
        u, v, r = piece.u, piece.v, piece.r
        if move == "D":
            return u, v + 1, r
        if move == "T":
            return u, v, (r + 1) % len(SHAPES[piece.kind])
        return u + (1 if move == "R" else -1), v, r

    def _advance(self, piece: Piece) -> None:
        """One step of a piece: a move across or a turn, and a dot of
        fall."""
        free = self.open
        body = self._body(piece)
        if not all(c in free for c in body):
            # Text written on it, the heap moved into it, or scrolled out.
            self.pieces.remove(piece)
            u0, v0, u1, v1 = self.box
            seen = [c for c in body if u0 <= c[0] < u1 and v0 <= c[1] < v1]
            if seen:
                self.events.append(("lost", [self._out(c) for c in seen], piece.color))
            self._touch()
            return
        kind = piece.kind
        if piece.phase:
            if piece.plan and piece.plan[0] != "D" and piece.stamp == self._stamp:
                u, v, r = self._moved(piece, piece.plan[0])
                if self._fits(kind, u, v, r) and self._fits(kind, u, v + 1, r):
                    piece.u, piece.r = u, r
                    piece.plan.pop(0)
            piece.phase += 1
            if piece.phase == self.period:
                piece.v, piece.phase = piece.v + 1, 0
            return
        if piece.stamp != self._stamp:
            self._plan(piece)
        plan = piece.plan
        if not plan:
            if not self._fits(kind, piece.u, piece.v + 1, piece.r):
                self._lock(piece)
            elif self.steps % self.period == 0:
                # Over the place of another, still on its way: look again.
                piece.stamp = -1
            return
        if plan[0] != "D":
            piece.u, piece.v, piece.r = self._moved(piece, plan.pop(0))
            if "D" in plan and plan[0] != "D":
                # Fall first when the moves across can be made lower.
                at = plan.index("D")
                lower = ["D", *plan[:at], *plan[at + 1 :]]
                if self._valid(piece, lower):
                    plan[:] = lower
        if plan and plan[0] == "D":
            plan.pop(0)
            piece.phase = 1

    def _valid(self, piece: Piece, plan: list[str]) -> bool:
        at = Piece(0, piece.kind, 0, piece.u, piece.v, piece.r)
        for move in plan:
            at.u, at.v, at.r = self._moved(at, move)
            if not self._fits(at.kind, at.u, at.v, at.r):
                return False
        return True

    def _lock(self, piece: Piece) -> None:
        for c in self._at(piece.kind, piece.u, piece.v, piece.r):
            self.heap[c] = piece.color
        self.pieces.remove(piece)
        self.rev += 1
        self._touch()
        self._clear()

    def _plan(self, piece: Piece) -> None:
        """Choose where `piece` rests and how it gets there. The places
        of the other pieces are taken."""
        shapes = SHAPES[piece.kind]
        taken = {c for p in self.pieces if p is not piece for c in p.target}
        free = self.open - taken if taken else self.open
        start = (piece.u, piece.v, piece.r)
        back: dict[tuple[int, int, int], tuple[Any, str] | None] = {start: None}
        bad: set[tuple[int, int, int]] = set()
        queue = deque([start])
        rests = []

        def reach(state: tuple[int, int, int], move: str, to: tuple) -> bool:
            if to in back:
                return True
            if to in bad:
                return False
            u, v, r = to
            if all((u + a, v + b) in free for a, b in shapes[r]):
                back[to] = (state, move)
                queue.append(to)
                return True
            bad.add(to)
            return False

        while queue:
            state = queue.popleft()
            u, v, r = state
            reach(state, "L", (u - 1, v, r))
            reach(state, "R", (u + 1, v, r))
            reach(state, "T", (u, v, (r + 1) % len(shapes)))
            if not reach(state, "D", (u, v + 1, r)):
                rests.append(state)

        best, best_key = start, None
        for u, v, r in rests:
            cells = [(u + a, v + b) for a, b in shapes[r]]
            key: tuple[float, float] = (0, self._worth(cells, piece.color))
            if piece.goal is not None:
                across = [c[0] for c in cells]
                off = max(min(across) - piece.goal, piece.goal - max(across), 0)
                key = (-off, key[1])
            if best_key is None or key > best_key:
                best, best_key = (u, v, r), key
        plan = []
        state = best
        while back[state] is not None:
            state, move = back[state]
            plan.append(move)
        piece.plan = plan[::-1]
        piece.target = frozenset((best[0] + a, best[1] + b) for a, b in shapes[best[2]])
        piece.stamp = self._stamp

    def _worth(self, cells: list[Cell], color: Color) -> float:
        """How good a place is: lines made and advanced, sides against
        something, no free cell under it, deep; with `chains`, against
        its colour."""
        free, heap = self.open, self.heap
        own = set(cells)
        per: dict[Run, int] = {}
        for c in cells:
            run = self._run(c)
            per[run] = per.get(run, 0) + 1
        cleared = lines = 0
        advance = 0.0
        for run, k in per.items():
            n, filled = run[2] - run[1], self._filled(run) + k
            if filled == n:
                cleared, lines = cleared + n, lines + 1
            else:
                advance += k * filled / n
        holes = contact = same = depth = 0
        for u, v in cells:
            depth += v
            below = (u, v + 1)
            if below not in own:
                if below in free:
                    holes += 2 if (u, v + 2) in free else 1
                else:
                    contact += 1
            for side in ((u - 1, v), (u + 1, v)):
                if side not in own and side not in free:
                    contact += 1
            if self.chains:
                same += sum(heap.get((u + du, v + dv)) == color for du, dv in STEPS)
        return (
            8.0 * cleared
            + 6.0 * lines
            + 5.0 * advance
            + 1.5 * contact
            - 7.0 * holes
            + 0.7 * depth
            + 1.0 * same
        )


def view(frame: Frame) -> Box:
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    game = Game(
        random.Random(args.seed),
        sideways=args.sideways,
        chains=args.chains,
        count=args.count,
    )
    frame = _cli.pick(pilot, args.frame)
    # Its own dice: the particles do not change the game of a seed.
    stage = _blocks.Stage(
        pilot,
        frame,
        random.Random(args.seed),
        effects=args.effects,
        score=args.score,
        sideways=args.sideways,
    )
    # The walls are read when the driver says the text changed or the
    # view moved (topic `text`); the cursor is told too (topic `cursor`).
    sight = _cli.Sight(pilot, frame, cursor=args.cursor)
    walls: set[Cell] = set()
    rate = args.speed * game.period  # steps per second
    top = FAST if args.score else 1.0
    due = 0.0
    for dt in _cli.ticks(1.0 / (rate * top), seconds):
        due += dt * rate * (min(top, 1.0 + game.score / RAMP) if args.score else 1.0)
        if due < 1.0:
            continue
        due = min(due - 1.0, 1.0)
        if not sight.look():
            continue
        box = view(frame)
        if sight.changed:
            walls = walls_of(sight.rows, box[0], box[1])
        game.step(box, walls, sight.cursor)
        with sight.drawing(), pilot.batch():
            stage.show(game)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "blocks",
        "Falling pieces in braille, played alone between the words.",
        frame="largest",
    )
    out.add_argument(
        "--speed", type=float, default=4.0, help="cells per second [default: 4]"
    )
    out.add_argument("--seed", type=int, help="same seed, same game")
    out.add_argument(
        "--count", type=int, default=3, help="pieces falling at once [default: 3]"
    )
    out.add_argument(
        "--sideways",
        action="store_true",
        help="pieces come from the right; lines are columns",
    )
    out.add_argument(
        "--cursor",
        action="store_true",
        help="the piece nearest to the cursor lands under it",
    )
    out.add_argument(
        "--chains",
        action="store_true",
        help="a broken block breaks those of its colour that touch it",
    )
    out.add_argument(
        "--effects", action="store_true", help="broken blocks are particles"
    )
    out.add_argument(
        "--score", action="store_true", help="show the score; the higher, the faster"
    )
    out.add_argument(
        "-a", "--all", action="store_true", help="--chains --effects --score"
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.chains = args.effects = args.score = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    if args.count < 1:
        out.error("--count must be at least 1")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
