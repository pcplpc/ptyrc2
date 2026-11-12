# SPDX-License-Identifier: GPL-3.0-or-later
"""Flower: flowers planted next to a word.

    ptyrc-app flower                       # one flower, next to "flower"
    ptyrc-app flower --word TODO --count 3
    ptyrc-app flower --count 0             # every match gets one
    ptyrc-app flower -a                    # next to words of the view

The client reads the view twice a second: the matches of `--word`
(`Frame.find`) and the heat classes (`Frame.heat_classes`). A match
without a flower gets one, while there are fewer than `--count`. It is
planted at the nearest spot (`place`) where its root is 3 to 10 columns
from the word and 1 to 3 rows above or below it, and where each of its
cells is cold: free and still. The match is redrawn in bold over the
text (`blend="tint"`). At most one flower is planted a scan.

A flower has a random size (`SHAPES`) and random colours (`Colors`).
It grows in three steps sent by the client, then sways, an animation
played by the driver. It wilts when its match leaves the view or text
covers it; with matches to spare, after 20 to 40 s (`LIFE`), and
another blooms elsewhere. Flowers and bold words hold frame positions:
they scroll with the text.

`-a`: the words are a pool of 3 to 5 (`POOL`) drawn from the view
(`pool`): words of 4 letters or more, matched whole. A word leaves the
pool when it leaves the view or no flower fits next to it; another is
drawn. `--count` is the size of the pool unless given. No bold.
"""

from __future__ import annotations

import argparse
import math
import random
import re
import time
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field

from ptyrc import Frame, Match, Pilot, Sprite

from . import _cli

__all__ = [
    "GAP",
    "LIFE",
    "POOL",
    "RISE",
    "SHAPES",
    "Colors",
    "Shape",
    "main",
    "paint",
    "place",
    "pool",
]

GAP = (3, 10)  # columns from the word to the root
RISE = (1, 3)  # rows from the word to the root
LIFE = (20.0, 40.0)  # seconds, while matches are left without a flower
SCAN = 0.5  # seconds between two reads of the view
STEP = 0.35  # seconds between two growth steps
HEARTS = "o@*"
POOL = (3, 5)  # words, with `-a`
WORD = re.compile(r"[^\W\d_]{4,}")

Rows = tuple[str, ...]


@dataclass(frozen=True)
class Shape:
    """A flower in a box of `w` x `h` cells, its root at the bottom of
    column `root`. `grow`: the steps before it opens; `sway`: leaning
    left, upright, right; `head`: rows of the flower head, the rest is
    stem. Spaces are transparent."""

    grow: tuple[Rows, ...]
    sway: tuple[Rows, Rows, Rows]
    head: int

    @property
    def w(self) -> int:
        return len(self.sway[1][0])

    @property
    def h(self) -> int:
        return len(self.sway[1])

    @property
    def root(self) -> int:
        return self.sway[1][-1].index("|")

    @property
    def mask(self) -> frozenset[tuple[int, int]]:
        """Cells with a glyph in some step, from the top left."""
        return frozenset(
            (x, y)
            for rows in (*self.grow, *self.sway)
            for y, row in enumerate(rows)
            for x, ch in enumerate(row)
            if ch != " "
        )


def _shape(
    head: int, grow: Iterable[Sequence[str]], sway: Iterable[Sequence[str]]
) -> Shape:
    """Pad to a common box: rows to the right, steps from the top."""
    grow, sway = [list(r) for r in grow], [list(r) for r in sway]
    w = max(len(row) for rows in grow + sway for row in rows)
    h = max(len(rows) for rows in grow + sway)

    def box(rows: list[str]) -> Rows:
        return tuple(r.ljust(w) for r in [""] * (h - len(rows)) + rows)

    left, up, right = map(box, sway)
    return Shape(tuple(map(box, grow)), (left, up, right), head)


SHAPES = {
    "small": _shape(
        1,
        [["  .  "], ["  o  ", "  |  "]],
        [
            ["(@)  ", "  \\  ", "  |  "],
            [" (@) ", "  |  ", "  |  "],
            ["  (@)", "  /  ", "  |  "],
        ],
    ),
    "daisy": _shape(
        3,
        [["   .   "], ["   o   ", "  \\|   ", "   |   "]],
        [
            [" \\|/   ", " -*-   ", " /|\\   ", "   \\   ", "  \\|   "],
            ["  \\|/  ", "  -*-  ", "  /|\\  ", "   |   ", "  \\|   "],
            ["   \\|/ ", "   -*- ", "   /|\\ ", "   /   ", "  \\|   "],
        ],
    ),
    "large": _shape(
        5,
        [
            ["   .   ", "   |   "],
            ["  ,-,  ", " ( o ) ", "  `-'  ", "  \\|/  ", "   |   "],
        ],
        [
            [
                ",-.-,  ",
                "( \\ / )",
                " ) o ( ",
                "( / \\ )",
                " `-'-' ",
                "  \\|/  ",
                "   |   ",
            ],
            [
                " ,-.-, ",
                "( \\ / )",
                " ) o ( ",
                "( / \\ )",
                " `-'-' ",
                "  \\|/  ",
                "   |   ",
            ],
            [
                "  ,-.-,",
                "( \\ / )",
                " ) o ( ",
                "( / \\ )",
                " `-'-' ",
                "  \\|/  ",
                "   |   ",
            ],
        ],
    ),
}

# 256-colour indices.
PETALS = (196, 197, 198, 201, 205, 207, 213, 219, 129, 135, 141, 99, 208, 214, 39)
CENTRES = (220, 226, 214, 229, 231)
STEMS = (28, 34, 40, 70, 76)


@dataclass(frozen=True)
class Colors:
    """Two petal colours, alternating, a heart and a stem."""

    petals: int | str
    alt: int | str
    heart: int | str
    stem: int | str

    @classmethod
    def random(cls, rng: random.Random) -> Colors:
        a, b = rng.sample(PETALS, 2)
        return cls(
            a, b if rng.random() < 0.4 else a, rng.choice(CENTRES), rng.choice(STEMS)
        )


def paint(
    rows: Sequence[str], colors: Colors, head: int, dim: bool = False
) -> list[list[dict[str, object]]]:
    """Rows of text to rows of styled runs, one run per glyph. The first
    `head` rows are the head: hearts bold, petals in two colours."""
    out = []
    for y, row in enumerate(rows):
        runs: list[dict[str, object]] = []
        for x, ch in enumerate(row):
            if ch == " ":
                run: dict[str, object] = {"t": " "}  # transparent
            elif y >= head:
                run = {"t": ch, "fg": colors.stem}
            elif ch in HEARTS:
                run = {"t": ch, "fg": colors.heart, "bold": True}
            else:
                run = {"t": ch, "fg": colors.petals if (x + y) % 2 else colors.alt}
            if dim and ch != " ":
                run["dim"] = True
            runs.append(run)
        out.append(runs)
    return out


def place(
    classes: Sequence[str],
    word: tuple[int, int, int],
    shape: Shape,
    taken: set[tuple[int, int]],
    rng: random.Random,
) -> tuple[int, int] | None:
    """Top left of the box, in view cells, of the flower planted next to
    `word` (x, y, length, view cells): its root `GAP` columns from an end
    of the word and `RISE` rows off its row, each cell of its `mask` in
    the view, cold (`.` in `classes`) and not `taken`, its neighbours
    left and right cold or out of the view: never in a gap between two
    words. The nearest such spot, rows counting double, ties at random;
    None if there is none."""
    wx, wy, n = word
    h = len(classes)

    def cold(x: int, y: int) -> bool:
        return not 0 <= x < len(classes[y]) or classes[y][x] == "."

    best: list[tuple[float, float, int, int]] = []
    for gap in range(GAP[0], GAP[1] + 1):
        for rise in range(RISE[0], RISE[1] + 1):
            for rx in (wx - gap, wx + n - 1 + gap):
                for ry in (wy - rise, wy + rise):
                    x0, y0 = rx - shape.root, ry - shape.h + 1
                    cells = [(x0 + dx, y0 + dy) for dx, dy in shape.mask]
                    if all(
                        0 <= y < h
                        and 0 <= x < len(classes[y])
                        and classes[y][x] == "."
                        and (x, y) not in taken
                        and cold(x - 1, y)
                        and cold(x + 1, y)
                        for x, y in cells
                    ):
                        best.append((math.hypot(gap, 2 * rise), rng.random(), x0, y0))
    if not best:
        return None
    _, _, x0, y0 = min(best)
    return x0, y0


@dataclass
class Flower:
    """A flower and its bold word, in frame cells."""

    key: tuple[int, int, str]  # the match: x, y, text
    at: tuple[int, int]  # top left of the box
    shape: Shape
    colors: Colors
    sprite: Sprite
    bold: Sprite | None
    born: float
    life: float
    step: int = 0  # growth steps shown
    next: float = 0.0  # time of the next step
    dies: float | None = None  # time it is gone, once wilting
    cells: set[tuple[int, int]] = field(default_factory=set)


def word_view(m: Match, origin: tuple[int, int]) -> tuple[int, int, int]:
    return m.x - origin[0], m.y - origin[1], m.len


def plant(
    pilot: Pilot,
    frame: Frame,
    m: Match,
    shape: Shape,
    at: tuple[int, int],
    layer: str,
    bold: bool,
    now: float,
    rng: random.Random,
) -> Flower:
    """Draw its first step, and the bold word if `bold`; `at` in frame
    cells."""
    colors = Colors.random(rng)
    return Flower(
        key=(m.x, m.y, m.text),
        at=at,
        shape=shape,
        colors=colors,
        sprite=pilot.draw(
            cells=paint(shape.grow[0], colors, 0),
            frame=frame.id,
            at=at,
            space="frame",
            layer=layer,
        ),
        bold=pilot.draw(
            [m.text],
            frame=frame.id,
            at=(m.x, m.y),
            space="frame",
            blend="tint",
            bold=True,
        )
        if bold
        else None,
        born=now,
        life=rng.uniform(*LIFE),
        next=now + STEP,
        cells={(at[0] + dx, at[1] + dy) for dx, dy in shape.mask},
    )


def grow(flower: Flower, now: float, rng: random.Random) -> None:
    """The next growth step when due; the last one starts the sway."""
    if flower.dies is not None or flower.next > now:
        return
    shape, colors = flower.shape, flower.colors
    flower.step += 1
    flower.next = now + STEP
    if flower.step < len(shape.grow):
        flower.sprite.set(cells=paint(shape.grow[flower.step], colors, 0))
        return
    flower.next = math.inf
    # left, upright, right, upright, from a random phase, at its own pace.
    order = [shape.sway[i] for i in (0, 1, 2, 1)]
    k = rng.randrange(4)
    ms = rng.randint(450, 800)
    sway = [
        {"cells": paint(r, colors, shape.head), "ms": ms} for r in order[k:] + order[:k]
    ]
    flower.sprite.set(frames=sway, loop=True)


def wilt(flower: Flower, now: float) -> None:
    """Close and fade; the driver removes it after the last frame."""
    if flower.dies is not None:
        return
    shape, colors = flower.shape, flower.colors
    steps = [shape.sway[1], *reversed(shape.grow)]
    frames = [
        {
            "cells": paint(
                r, colors, shape.head if r is shape.sway[1] else 0, dim=True
            ),
            "ms": 250,
        }
        for r in steps
    ]
    flower.sprite.set(frames=frames, loop=False)
    if flower.bold is not None:
        flower.bold.remove()
    flower.dies = now + 0.25 * len(frames)


def covered(flower: Flower, rows: Sequence[str], origin: tuple[int, int]) -> bool:
    """Text shows in one of its cells."""
    ox, oy = origin
    for x, y in flower.cells:
        vx, vy = x - ox, y - oy
        if (
            0 <= vy < len(rows)
            and 0 <= vx < len(rows[vy])
            and not rows[vy][vx].isspace()
        ):
            return True
    return False


def pool(
    rows: Sequence[str], words: Sequence[str], size: int, rng: random.Random
) -> list[str]:
    """`words` still in `rows`, the view's text, then words drawn at
    random from it up to `size`. Lower case, 4 letters or more."""
    seen = sorted({w.lower() for row in rows for w in WORD.findall(row)})
    out = [w for w in words if w in seen]
    rest = [w for w in seen if w not in out]
    return out + rng.sample(rest, min(len(rest), max(0, size - len(out))))


@dataclass
class Garden:
    """What the app keeps between two scans."""

    flowers: list[Flower] = field(default_factory=list)
    words: list[str] = field(default_factory=list)  # with `-a`


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    # Planted on cold cells, a flower is seen on either layer; `--front`
    # keeps it whole when text comes over it before it wilts.
    layer = "front" if args.front else "back"
    size = rng.randint(*POOL)
    if args.count is None:
        args.count = size if args.anywhere else 1
    frame = _cli.pick(pilot, args.frame)
    garden = Garden()
    scanned = -math.inf
    for _ in _cli.ticks(0.05, seconds):
        now = time.monotonic()
        with _cli.in_sight(), pilot.batch():
            garden.flowers = [
                f for f in garden.flowers if f.dies is None or f.dies > now
            ]
            for flower in garden.flowers:
                grow(flower, now, rng)
        if now - scanned < SCAN or not _cli.on_screen(frame):
            continue
        scanned = now
        with _cli.in_sight():
            scan(pilot, frame, garden, args, size, layer, now, rng)


def scan(
    pilot: Pilot,
    frame: Frame,
    garden: Garden,
    args: argparse.Namespace,
    size: int,
    layer: str,
    now: float,
    rng: random.Random,
) -> None:
    """Wilt the flowers that lost their place, plant one if wanted."""
    origin = frame.origin
    rows = frame.text()
    if args.anywhere:
        garden.words = pool(rows, garden.words, size, rng)
        if not garden.words:
            return
        words = "|".join(map(re.escape, garden.words))
        found = frame.find(rf"\b(?:{words})\b", regex=True, ignore_case=True)
    else:
        found = frame.find(args.word, ignore_case=True)
    matches = [m for m in found if m.screen]
    by_key = {(m.x, m.y, m.text): m for m in matches}
    living = [f for f in garden.flowers if f.dies is None]
    spare = len(matches) > len(living)
    with pilot.batch():
        for f in living:
            old = spare and now - f.born > f.life and f.step >= len(f.shape.grow)
            if f.key not in by_key or covered(f, rows, origin) or old:
                wilt(f, now)
    living = [f for f in garden.flowers if f.dies is None]
    want = len(matches) if args.count == 0 else args.count
    free = [m for k, m in by_key.items() if k not in {f.key for f in living}]
    if len(living) >= want or not free:
        return
    classes = frame.heat_classes()
    ox, oy = origin
    taken = {(x - ox, y - oy) for f in garden.flowers for x, y in f.cells}
    for m in matches:  # the words, and a cell around them
        x, y = m.x - ox, m.y - oy
        taken.update((x + i, y + j) for i in range(-1, m.len + 1) for j in (-1, 0, 1))
    # Words with the fewest flowers first, then at random.
    per_word: dict[str, int] = {}
    for f in living:
        per_word[f.key[2].lower()] = per_word.get(f.key[2].lower(), 0) + 1
    rng.shuffle(free)
    free.sort(key=lambda m: per_word.get(m.text.lower(), 0))
    failed: set[str] = set()
    for m in free:
        # A random size; smaller ones where it does not fit.
        first = rng.choice(list(SHAPES.values()))
        for shape in (first, *sorted(SHAPES.values(), key=lambda s: s.h)):
            at = place(classes, word_view(m, origin), shape, taken, rng)
            if at is not None:
                at = (at[0] + ox, at[1] + oy)
                with pilot.batch():
                    flower = plant(
                        pilot, frame, m, shape, at, layer, not args.anywhere, now, rng
                    )
                garden.flowers.append(flower)
                return
        failed.add(m.text.lower())
    # No room next to any of its free matches, none flowered: drawn anew.
    garden.words = [w for w in garden.words if w not in failed or w in per_word]


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until the session ends."""
    out = _cli.parser("flower", "Flowers next to a word.", frame="largest")
    out.add_argument("--word", default="flower", help="the word [default: flower]")
    out.add_argument(
        "-a",
        "--anywhere",
        action="store_true",
        help="words drawn from the screen, 3 to 5, in place of --word; no bold",
    )
    out.add_argument(
        "--count",
        type=int,
        metavar="N",
        help="flowers, 0: one per match [default: 1; with -a, one per word]",
    )
    out.add_argument("--front", action="store_true", help="over the text")
    out.add_argument("--seed", type=int, help="same seed, same garden")
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
