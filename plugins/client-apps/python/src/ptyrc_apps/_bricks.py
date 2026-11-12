# SPDX-License-Identifier: GPL-3.0-or-later
"""The wall: what `bricks` draws.

- A brick, a racket, a ball, a capsule, a falling brick: one sprite
  each, in frame coordinates. Those that blink are animations played by
  the driver; the client moves them.
- A brick is 3 cells: two brackets and a sign. `⟦=⟧` plain; `⟦≡⟧`,
  `⟦=⟧`, `⟦-⟧` hard, a bar a life, in steel; `⟨=⟩` loose; `⟦o⟧`
  bumper, pulsing; `⟦*⟧` bomb, blinking. One whose group is about to
  go is dim; gone, it leaves `⟦·⟧` then `·`, dim (`fade`).
- A racket: `(━━━)`; hit by a ball, `(═══)` for `FLASH` steps.
- A ball: half a cell high, `▀` or `▄` by the half of the row it is
  in: at half a row a step it moves at each step. One that moved more
  than half a row or a column in a step is shown half way first, then
  in place by `Stage.glide`, between two steps.
- The score: one sprite in view coordinates, top right.
- Marks (`pop`, `fade`, `lock`, `splash`, `again`): each one request,
  a finite animation.
- Particles (`_arcade.spark`, `shards`): each one request, played and
  removed by the driver.

`Stage` keeps the sprites in step with a game. Bricks are remembered by
the game wherever they are; their sprites exist within a view's height
of the view.

Layers: all behind the text. `z`, from the back: shards, bricks, the
trail, capsules and falling bricks, rackets, balls, marks and
particles, the score.
"""

from __future__ import annotations

import random
from math import floor
from typing import Any

from ptyrc import Frame, Pilot, Sprite

from ._arcade import Board, burst, spark

__all__ = [
    "FLASH",
    "LABEL",
    "PALETTE",
    "TEAM",
    "Stage",
    "again",
    "ball",
    "board",
    "brick",
    "capsule",
    "fade",
    "faller",
    "label",
    "lock",
    "pop",
    "racket",
    "shards",
    "splash",
    "trail",
]

W = 3  # cells of a brick
PALETTE = (196, 208, 226, 46, 51, 33, 201)  # bricks, by row
STEEL = {3: ("≡", 255), 2: ("=", 250), 1: ("-", 245)}  # a hard brick, by lives
TEAM = (51, 214)  # two rackets; one alone is white
FLAME = (226, 208, 196)
FLASH = 2  # steps a racket shows a hit
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


def _once(frames: list[tuple[str, Color, bool]], ms: int) -> dict[str, Any]:
    """A finite animation: text, colour, dim."""
    return {
        "frames": [
            {"cells": [[{"t": t, "fg": c, "dim" if dim else "bold": True}]], "ms": ms}
            for t, c, dim in frames
        ],
        "loop": False,
        "ttl": len(frames) * ms / 1000 + 0.5,
    }


def _ink(kind: str, color: int) -> Color:
    return PALETTE[color] if kind in ("plain", "loose") else 250


def brick(kind: str, lives: int, color: int, dim: bool = False) -> dict[str, Any]:
    """A brick of 3 cells, as arguments of `Pilot.draw` less the
    place. `dim`: its group is about to go."""
    look = {"dim": True} if dim else {}
    bold = look or {"bold": True}
    if kind == "hard":
        sign, ink = STEEL[max(1, min(3, lives))]
        return {"cells": [[{"t": f"⟦{sign}⟧", "fg": ink, **bold}]]}
    if kind == "loose":
        return {"cells": [[{"t": "⟨=⟩", "fg": PALETTE[color], **look}]]}
    if kind == "bumper":
        inks = (201, 213) if lives > 1 else (213, 225)
        return _loop(list(zip(("⟦o⟧", "⟦O⟧"), inks, strict=True)), 300, **bold)
    if kind == "bomb":
        return _loop([("⟦*⟧", 196), ("⟦+⟧", 226)], 250, **bold)
    return {"cells": [[{"t": "⟦=⟧", "fg": PALETTE[color], **look}]]}


def faller(color: int) -> dict[str, Any]:
    """A loose brick on its way down: it shakes."""
    return _loop([("⟨=⟩", 196), ("⟨~⟩", PALETTE[color])], 90, bold=True)


def racket(
    width: int, team: int | None = None, hit: bool = False
) -> list[list[dict[str, Any]]]:
    """A racket: the same on both sides. `team`: None when alone.
    `hit`: a ball just left it."""
    if hit:
        bar, ink = "═", 226 if team is None else 231
    else:
        bar, ink = "━", "bright-white" if team is None else TEAM[team]
    return [[{"t": "(" + bar * (width - 2) + ")", "fg": ink, "bold": True}]]


def ball(
    team: int | None = None, fire: bool = False, low: bool = False, step: int = 0
) -> dict[str, Any]:
    """A ball, of its owner's colour, in the upper half of its cell or
    the `low` one; on fire, a colour of the flame a `step`."""
    ink = "bright-white" if team is None else TEAM[team]
    if fire:
        ink = FLAME[step % len(FLAME)]
    return {"cells": [[{"t": "▄" if low else "▀", "fg": ink, "bold": True}]]}


def trail(team: int | None = None, low: bool = False) -> dict[str, Any]:
    """What a ball leaves where it was: it fades."""
    glyph = "▄" if low else "▀"
    ink = 250 if team is None else TEAM[team]
    return _once([(glyph, ink, True), (glyph, 240, True), (glyph, 236, True)], 60)


def pop(kind: str, color: int) -> dict[str, Any]:
    """A brick that breaks, in its place."""
    left, right = "⟨⟩" if kind == "loose" else "⟦⟧"
    ink = _ink(kind, color)
    return _once([(f"{left}·{right}", ink, True), (" · ", ink, True)], 70)


def fade(kind: str, color: int) -> dict[str, Any]:
    """A brick that goes with its group, in its place: dim, then a
    dot."""
    left, right = "⟨⟩" if kind == "loose" else "⟦⟧"
    ink = _ink(kind, color)
    return _once([(f"{left}·{right}", ink, True), (" · ", 240, True)], 160)


def again() -> dict[str, Any]:
    """The game starts again: three dots, dim. 5 cells."""
    return _once(
        [("· · ·", 244, True), (" · · ", 240, True), ("  ·  ", 236, True)], 220
    )


def shards(rng: random.Random, kind: str, color: int) -> list[dict[str, Any]]:
    """The pieces of a broken brick, one a cell: the brackets fly
    apart, the sign up; all fall. Drawn behind the bricks."""
    left, right = ("⟨<.", "⟩>.") if kind == "loose" else ("⟦[.", "⟧].")
    out = []
    for glyphs, way in ((left, -1.0), ("=-.", 0.0), (right, 1.0)):
        one = spark(rng, (_ink(kind, color),), glyphs, life=(0.35, 0.6))
        one["motion"] = {
            "vx": way * rng.uniform(4.0, 10.0) + rng.uniform(-1.5, 1.5),
            "vy": rng.uniform(-6.0, -2.0),
            "ax": 0.0,
            "ay": 16.0,
            "bounce": False,
        }
        out.append(one)
    return out


def lock() -> dict[str, Any]:
    """A serve: brackets close on the ball. 5 cells, the ball in the
    middle."""
    return _once([("(   )", 231, False), (" ( ) ", 231, False)], 90)


def splash() -> dict[str, Any]:
    """A ball out: a mark that widens on the edge. 5 cells."""
    return _once(
        [("  x  ", 196, False), (" - - ", 196, False), ("-   -", 160, True)], 90
    )


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


def label(text: str, ink: Color, seconds: float = 0.5) -> dict[str, Any]:
    """A word that rises and goes: bold for `seconds`, then dim half
    as long."""
    ms = int(seconds * 1000)
    return {
        "frames": [
            {"cells": [[{"t": text, "fg": ink, "bold": True}]], "ms": ms},
            {"cells": [[{"t": text, "fg": ink, "dim": True}]], "ms": ms // 2},
        ],
        "loop": False,
        "motion": {"vx": 0.0, "vy": -3.0, "ax": 0.0, "ay": 0.0, "bounce": False},
        "ttl": 1.5 * seconds + 0.25,
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
    """The sprites of a `bricks.Game`; particles and more when
    `effects`."""

    def __init__(
        self, pilot: Pilot, frame: Frame, rng: random.Random, effects: bool
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.effects = effects
        self.bricks: dict[int, tuple[Sprite, Any]] = {}  # by serial: sprite, key
        self.dim: set[int] = set()  # serials of the bricks about to go
        self.moving: dict[int, tuple[Sprite, Any]] = {}  # sprite, what is shown
        self.board = Board(pilot, frame)
        self.steps = 0
        self.hit: dict[int, int] = {}  # by racket: the step its flash ends
        self.was: dict[int, tuple[float, float]] = {}  # by ball: where it was
        self.late: dict[int, tuple[Cell, Any, dict[str, Any]]] = {}  # for `glide`

    def draw(self, at: Cell, **kw: Any) -> Sprite:
        return self.pilot.draw(frame=self.frame.id, at=at, space="frame", **kw)

    def sparks(self, at: Cell, n: int, colors: Any, **kw: Any) -> None:
        for one in burst(n, self.rng, colors, **kw):
            self.draw(at, **{**one, "z": 4})

    def _put(self, n: int, at: Cell, key: Any, z: int, look: dict[str, Any]) -> None:
        if n not in self.moving:
            self.moving[n] = (self.draw(at, z=z, **look), (at, key))
            return
        sprite, (was, shown) = self.moving[n]
        if shown != key:
            sprite.set(**look)
        if was != at:
            sprite.move(*at, space="frame")
        self.moving[n] = (sprite, (at, key))

    def _ball(self, it: Any, x: float, y: float, alone: bool) -> tuple[Any, ...]:
        """Place, key and look of ball `it` at `x`, `y`."""
        team, low = None if alone else it.owner, y - floor(y) >= 0.5
        hot = 1 + self.steps % len(FLAME) if it.fire else 0
        look = ball(team, bool(hot), low, self.steps)
        return (floor(x), floor(y)), (team, hot, low), look

    def show(self, game: Any, box: Box) -> None:
        """One step: inside a batch."""
        self.steps += 1
        self.late = {}
        for event in game.events:
            self.play(*event)
        x0, y0, x1, y1 = box
        alone = game.teams == 1
        reach = y1 - y0
        near = {b.n: b for b in game.bricks((x0, y0 - reach, x1, y1 + reach))}
        for n in [n for n in self.bricks if n not in near]:
            self.bricks.pop(n)[0].remove()
        self.dim.intersection_update(near)
        for n, it in near.items():
            key = (it.lives, n in self.dim)
            if n not in self.bricks:
                look = brick(it.kind, it.lives, it.color, key[1])
                self.bricks[n] = (self.draw((it.x, it.y), z=0, **look), key)
            elif self.bricks[n][1] != key:
                self.bricks[n][0].set(**brick(it.kind, it.lives, it.color, key[1]))
                self.bricks[n] = (self.bricks[n][0], key)

        things: dict[int, tuple[Cell, Any, int, dict[str, Any]]] = {}
        for it in game.fallers:
            things[it.n] = ((it.x, it.y), "faller", 1, faller(it.color))
        for it in game.capsules:
            things[it.n] = ((it.x, it.y), it.kind, 1, capsule(it.kind))
        self.hit = {n: end for n, end in self.hit.items() if end > self.steps}
        for it in game.rackets:
            if not it.dead:
                team, hit = None if alone else it.team, it.n in self.hit
                look = {"cells": racket(it.w, team, hit)}
                things[it.n] = ((it.x, it.y), (it.w, hit), 2, look)
        was, self.was = self.was, {b.n: (b.x, b.y) for b in game.balls}
        for it in game.balls:
            x, y = it.x, it.y
            px, py = was.get(it.n, (x, y))
            if abs(y - py) > 0.55 or abs(x - px) > 1.05:
                # More than its own size in a step: half way first.
                self.late[it.n] = self._ball(it, x, y, alone)
                x, y = (x + px) / 2, (y + py) / 2
            at, key, look = self._ball(it, x, y, alone)
            things[it.n] = (at, key, 3, look)
            if it.fire:  # flames behind a ball on fire
                one = spark(self.rng, FLAME, "*+.", (0.5, 3.0), (0.2, 0.5), -4.0)
                self.draw(at, **{**one, "z": 4})
            elif self.effects:
                self.draw(at, z=1, **trail(key[0], key[2]))
        for n in [n for n in self.moving if n not in things]:
            self.moving.pop(n)[0].remove()
        for n, (at, key, z, look) in things.items():
            self._put(n, at, key, z, look)

        self.board.show(board(game.score, game.best if self.effects else None))

    def glide(self) -> None:
        """Between two steps, inside a batch: the balls shown half way
        go to their place."""
        for n, (at, key, look) in self.late.items():
            if n in self.moving:
                self._put(n, at, key, 3, look)
        self.late = {}

    def play(self, what: str, at: Any = None, *data: Any) -> None:
        """An event of the game: marks, and particles when `effects`. A
        blast and a kill always show. One not known is ignored."""
        if what == "fade":
            self.dim.update(it.n for it in at)
        elif what == "gone":
            for it in at:
                self.draw((it.x, it.y), **{**fade(it.kind, it.color), "z": 0})
        elif what == "reset":
            self.draw((at[0] - 2, at[1]), **{**again(), "z": 4})
        if what == "boom":
            self.sparks(at, 22, (231, *FLAME), speed=(6.0, 22.0))
        elif what == "kill":
            self.sparks(at, 18, ("bright-white", 196), glyphs="━=-.", gravity=14.0)
        elif what == "break":
            it = data[0]
            self.draw((it.x, it.y), **{**pop(it.kind, it.color), "z": 4})
            if self.effects:
                for i, one in enumerate(shards(self.rng, it.kind, it.color)):
                    self.draw((it.x + i, it.y), **{**one, "z": -1})
        elif what == "bounce":
            if data and hasattr(data[0], "n"):
                self.hit[data[0].n] = self.steps + FLASH
        elif what == "serve":
            self.draw((at[0] - 2, at[1]), **{**lock(), "z": 4})
        elif what in ("lost", "out"):
            self.draw((at[0] - 2, at[1]), **{**splash(), "z": 4})
        if not self.effects:
            return
        if what == "hit":
            self.sparks(at, 2, (252,), glyphs="+.", speed=(2.0, 6.0), life=(0.2, 0.4))
        elif what == "bounce":
            self.sparks(at, 3, (231,), glyphs="'.", speed=(3.0, 8.0), life=(0.15, 0.3))
        elif what == "combo":
            n = data[0]
            ink = 252 if n < 4 else 226 if n < 7 else 208
            self.draw((at[0], at[1] - 1), **{**label(f"x{n}", ink, 0.3), "z": 4})
        elif what == "bump":
            self.sparks(at, 6, (201, 213), glyphs="≈~.", gravity=0.0)
        elif what == "crash":
            for i, one in enumerate(shards(self.rng, "loose", 0)):
                self.draw((at[0] - 1 + i, at[1]), **{**one, "z": -1})
        elif what == "lost":
            self.sparks(at, 10, (196, 160), glyphs="▄*.", speed=(4.0, 12.0))
            self.draw(at, **{**label("0", 196), "z": 4})
        elif what == "serve":
            self.sparks(at, 4, (231,), glyphs="+'.", speed=(2.0, 6.0), life=(0.2, 0.4))
        elif what == "catch":
            letter, ink = LABEL[data[0]]
            self.sparks(at, 8, (ink,), glyphs=letter + "*.", gravity=0.0)
            self.draw(at, **{**label(data[0], ink), "z": 4})
        elif what == "group":
            for it in at:
                one = spark(self.rng, (231,), "+'", (0.0, 1.0), (0.2, 0.5), 0.0)
                self.draw((it.x + self.rng.randrange(W), it.y), **{**one, "z": 4})
