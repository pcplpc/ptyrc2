# SPDX-License-Identifier: GPL-3.0-or-later
"""Cat: a cat that lives on the text, and sleeps when nothing moves.

    ptyrc run vim notes.txt          # or less, a shell
    ptyrc-app cat
    ptyrc-app cat -f active --front

What it shows:

- Events instead of a clock: the app subscribes to `idle`, `cursor`,
  `screen` and `frames`, and sleeps on the socket between two things
  to do. A cat asleep sends nothing: its breath is a looped animation
  played by the driver.
- Topic `idle`: the driver says when the pane has been still for a
  while. The cat yawns, curls up and sleeps; it wakes when the pane
  is busy again.
- `space="frame"`: the cat stands on a line of text, not on a row of
  the window. Scroll and it goes with its line.
- `draw.list` (`Sprite.info`): `visible` tells that a scroll left the
  cat out of the view, `covered` that text was written over it. It
  jumps to another place then, as it does when the cursor comes near
  or when the line under its paws goes.
- The back layer: text written where the cat is stays readable.

A place to stand is a `ledge`: a row with text under a box of free
cells, three rows by eleven columns (`ledges`). The bottom of the
view is one when it is free.
"""

from __future__ import annotations

import argparse
import math
import random
import time
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Frame, Pilot, PtyrcError, Sprite

from . import _cli
from ._reef import mirror, pad, paint

__all__ = [
    "COVERED",
    "GROUND",
    "POSES",
    "Cat",
    "H",
    "Ledge",
    "W",
    "arc",
    "ledges",
    "look",
    "main",
    "near",
    "perch",
]

W, H = 11, 3  # the box of every pose
STEP = 0.13  # seconds a step of the walk; one column
HOP = 0.05  # seconds between two points of a jump
NEAR = (7, 2)  # columns, rows: the cursor that close is a fright
READ = 0.3  # seconds, at least, between two reads of the text
AWAY = (2.0, 4.0)  # seconds out of the view before it comes back
GROUND = 5  # cells of text, at least, under the cat
COVERED = 0.2  # share of the cat under text that makes it move
INK = {"": 215, "oO": 114, "^": 211, "zZ": 244, "~_.": 208}

# Facing right. Each frame: three rows of `W` cells, then milliseconds.
POSES: dict[str, tuple[tuple[tuple[str, str, str], int], ...]] = {
    "sit": (
        (("     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "), 2600),
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 140),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 900),
        (("     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "), 500),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 1400),
    ),
    "alert": ((("     /\\_/\\ ", "    ( O.O )", "    _> ^ < "), 400),),
    "groom": (
        (("     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "), 260),
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 260),
        (("     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "), 260),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 700),
    ),
    "yawn": (
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 300),
        (("     /\\_/\\ ", "    ( -o- )", "    ~> ^ < "), 300),
        (("     /\\_/\\ ", "    ( -O- )", "    _> ^ < "), 500),
        (("     /\\_/\\ ", "    ( -o- )", "    _> ^ < "), 300),
    ),
    "sleep": (
        (("           ", "  __/\\_/\\  ", " (__( -.- )"), 1100),
        (("    z      ", "  __/\\_/\\  ", " (__( -.- )"), 900),
        (("   Z       ", " ___/\\_/\\  ", "(___( -.- )"), 900),
        (("  z        ", " ___/\\_/\\  ", "(___( -.- )"), 700),
    ),
    "walk": (
        (("      /\\_/\\", " ~.___(o.o)", "   /|   |\\ "), 0),
        (("      /\\_/\\", " -.___(o.o)", "   |\\   /| "), 0),
    ),
    "leap": ((("      /\\_/\\", " ~-=--(O.O)", "  ''     ``"), 0),),
}


def look(pose: str, right: bool = True, step: int | None = None) -> dict[str, Any]:
    """A pose as content for `Sprite.set`: a looped animation, or its
    frame `step` alone (the walk: the client is the clock)."""

    def cells(rows: Sequence[str]) -> list[list[dict[str, Any]]]:
        return paint(rows if right else mirror(rows), INK, bold=True)

    frames = POSES[pose]
    if step is not None or len(frames) == 1:
        return {"cells": cells(frames[(step or 0) % len(frames)][0])}
    return {
        "frames": [{"cells": cells(rows), "ms": ms} for rows, ms in frames],
        "loop": True,
    }


@dataclass(frozen=True)
class Ledge:
    """Where the cat may stand: its left column from `x0` to `x1`,
    its paws on the row above `g`. Frame cells."""

    g: int
    x0: int
    x1: int

    def has(self, x: int) -> bool:
        return self.x0 <= x <= self.x1


def ledges(rows: Sequence[str], box: tuple[int, int, int, int]) -> list[Ledge]:
    """The ledges of a view. `rows`: its text; `box`: its frame cells
    (x0, y0, x1, y1), ends excluded. Under the cat, text on `GROUND`
    cells at least; around it, its box free and a free column on each
    side. The floor of the view counts, with nothing under it."""
    x0, y0, x1, y1 = box
    width, height = x1 - x0, y1 - y0
    rows = pad([*rows, *[""] * height][:height], width) + ["#" * width]
    ink = [[not ch.isspace() for ch in row] for row in rows]
    out = []
    for g in range(H, height + 1):
        free = [not any(ink[y][x] for y in range(g - H, g)) for x in range(width)]
        run: list[int] = []
        for x in range(width - W + 2):
            ok = (
                x <= width - W
                and all(free[max(0, x - 1) : x + W + 1])
                and sum(ink[g][x : x + W]) >= GROUND
            )
            if ok:
                run.append(x)
            elif run:
                out.append(Ledge(y0 + g, x0 + run[0], x0 + run[-1]))
                run = []
    return out


def perch(spots: Sequence[Ledge], x: int, g: int) -> Ledge | None:
    """The ledge the cat at `x` above row `g` stands on, if any."""
    return next((s for s in spots if s.g == g and s.has(x)), None)


def near(x: int, g: int, cursor: tuple[int, int] | None) -> bool:
    """The cursor is in the cat's box, or `NEAR` it."""
    if cursor is None:
        return False
    dx, dy = NEAR
    return x - dx <= cursor[0] < x + W + dx and g - H - dy <= cursor[1] < g + dy


def arc(a: tuple[int, int], b: tuple[int, int]) -> list[tuple[int, int]]:
    """The cells of a jump from `a` to `b`, one per `HOP`: a parabola,
    higher and longer for a long jump; `b` is the last."""
    far = math.hypot(b[0] - a[0], 2 * (b[1] - a[1]))
    n = max(4, min(16, round(4 + far / 6)))
    rise = min(4.0, 1.5 + far / 20)
    out = []
    for i in range(1, n + 1):
        t = i / n
        lift = 4 * rise * t * (1 - t)
        out.append(
            (round(a[0] + (b[0] - a[0]) * t), round(a[1] + (b[1] - a[1]) * t - lift))
        )
    return out


class Cat:
    """The cat: where it stands (`x`, left column; `g`, the row under
    its paws; frame cells), what it does (`act`) and until when."""

    def __init__(
        self, pilot: Pilot, frame: Frame, rng: random.Random, front: bool = False
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.layer = "front" if front else "back"
        self.sprite: Sprite | None = None
        self.x = self.g = 0
        self.right = True
        self.act = "away"
        self.due = 0.0  # when the act ends, or its next step
        self.goal = 0  # the walk: the column to reach
        self.path: list[tuple[int, int]] = []  # the jump: cells left
        self.landing: tuple[int, int] | None = None
        self.idle = False
        self.cursor: tuple[int, int] | None = None
        self.spots: list[Ledge] = []
        self.read = 0.0  # time of the last read of the text
        self.stale = True  # the text changed since

    # -- what the cat knows -------------------------------------------

    def box(self) -> tuple[int, int, int, int]:
        (ox, oy), (w, h) = self.frame.origin, self.frame.size
        return (ox, oy, ox + w, oy + h)

    def survey(self, now: float) -> bool:
        """Read the view and find its ledges; False while the frame
        cannot be drawn on."""
        if not _cli.on_screen(self.frame):
            return False
        with _cli.in_sight():
            self.spots = ledges(self.frame.text(), self.box())
            self.read, self.stale = now, False
            return True
        return False

    def pick(self, far: bool = False) -> tuple[int, Ledge] | None:
        """A place to go: a column of a ledge, away from the cursor;
        `far`: the farthest of a few from where it stands."""
        spots = [
            (x, s)
            for s in self.spots
            for x in {s.x0, s.x1, self.rng.randint(s.x0, s.x1)}
            if not near(x, s.g, self.cursor)
        ]
        if not spots:
            return None
        some = self.rng.sample(spots, min(6, len(spots)))
        if not far:
            return some[0]
        return max(some, key=lambda p: math.hypot(p[0] - self.x, 2 * (p[1].g - self.g)))

    # -- what the cat does --------------------------------------------

    def show(self, pose: str, step: int | None = None) -> None:
        """Pose and place, in one request: one repaint."""
        content = look(pose, self.right, step)
        at = (self.x, self.g - H)
        if self.sprite is None:
            self.sprite = self.pilot.draw(
                frame=self.frame.id, at=at, space="frame", layer=self.layer, **content
            )
        else:
            with self.pilot.batch():
                self.sprite.update(anchor={"at": list(at), "space": "frame"})
                self.sprite.set(**content)

    def rest(self, pose: str, now: float, seconds: float | None) -> None:
        """A pose held `seconds`; None: until something happens."""
        self.act, self.due = pose, math.inf if seconds is None else now + seconds
        self.show(pose)

    def walk(self, goal: int, now: float) -> None:
        self.act, self.goal, self.due = "walk", goal, now
        self.right = goal >= self.x

    def jump(self, to: tuple[int, int], now: float, start: tuple[int, int]) -> None:
        """Leap from `start` (left column, row under the paws) to `to`."""
        self.act, self.due, self.landing = "leap", now, to
        self.right = to[0] >= start[0]
        self.path = arc(start, to)

    def flee(self, now: float) -> None:
        """Somewhere else, at once; nowhere to go: stay."""
        if self.act in ("leap", "away"):
            return
        if self.stale or now - self.read > 2.0:
            self.survey(now)
        if (to := self.pick(far=True)) is not None:
            self.jump((to[0], to[1].g), now, (self.x, self.g))

    def arrive(self, now: float) -> None:
        """Into the view from above it, on a ledge."""
        if not self.survey(now) or (to := self.pick()) is None:
            self.due = now + 0.3
            return
        x, g = to[0], to[1].g
        self.jump((x, g), now, (x, g))
        top = max(self.box()[1] + H, g - 6)
        self.path = [(x, y) for y in range(top, g + 1)]  # straight down

    def leave(self, now: float) -> None:
        """Out of the view: wait, then come back in it."""
        self.act, self.due = "away", now + self.rng.uniform(*AWAY)

    def check(self, now: float) -> None:
        """After a change of the text or of the view: is the cat seen,
        and is it still standing on something?"""
        if self.act in ("leap", "away") or self.sprite is None:
            return
        info = self.sprite.info()
        if info is None:  # its rows were forgotten: the driver took it
            self.sprite = None
            return self.leave(now)
        if not info["visible"]:
            return self.leave(now)
        if not self.survey(now):
            return
        if info["covered"] > COVERED or perch(self.spots, self.x, self.g) is None:
            self.flee(now)

    def step(self, now: float) -> None:
        """The act is due: its next step, or what comes after."""
        rng = self.rng
        if self.act == "away":
            self.arrive(now)
        elif self.act == "leap":
            if self.path:
                self.x, self.g = self.path.pop(0)
                self.show("leap")
                self.due = now + HOP
            else:
                self.x, self.g = self.landing or (self.x, self.g)
                self.rest("alert", now, 0.9)
                self.stale = True  # a look around, soon
        elif self.act == "walk":
            if self.x == self.goal:
                return self.rest("sit", now, rng.uniform(2.0, 6.0))
            self.x += 1 if self.goal > self.x else -1
            self.show("walk", self.x)
            self.due = now + STEP
        elif self.act == "yawn":
            self.rest("sleep", now, None)
        elif self.idle:
            self.rest("yawn", now, 1.4)
        else:
            self.wander(now)

    def wander(self, now: float) -> None:
        """Awake with nothing to do: groom, walk the ledge, hop to
        another, or sit a while more."""
        rng, roll = self.rng, self.rng.random()
        here = perch(self.spots, self.x, self.g)
        if roll < 0.25 and self.act != "groom":
            self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif roll < 0.65 and here is not None and here.x1 > here.x0:
            goal = rng.randint(here.x0, here.x1)
            if abs(goal - self.x) < 3:
                goal = here.x0 if self.x - here.x0 > here.x1 - self.x else here.x1
            if not near(goal, here.g, self.cursor):
                return self.walk(goal, now)
            self.rest("sit", now, rng.uniform(2.0, 5.0))
        elif roll < 0.85 and (to := self.pick()) is not None:
            self.jump((to[0], to[1].g), now, (self.x, self.g))
        else:
            self.rest("sit", now, rng.uniform(2.0, 6.0))

    # -- what happens to the cat --------------------------------------

    def on(self, topic: str, data: Any, now: float) -> None:
        """An event of the driver."""
        if topic == "idle" and data["frame"] == self.frame.id:
            self.idle = bool(data["idle"])
            if not self.idle and self.act in ("sleep", "yawn"):
                self.rest("alert", now, 0.6)
            elif self.idle and self.act in ("sit", "groom"):
                self.due = min(self.due, now + self.rng.uniform(0.5, 2.0))
        elif topic == "cursor":
            at = (data["x"], data["y"]) if data.get("visible", True) else None
            self.cursor = self.frame.to_frame(*at) if at else None
            if near(self.x, self.g, self.cursor):
                self.flee(now)
        elif topic in ("screen", "frames"):
            self.stale = True


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    frame = _cli.pick(pilot, args.frame)
    cat = Cat(pilot, frame, rng, args.front)
    pilot.subscribe("idle", "cursor", "screen", "frames")
    # No event tells what already is: ask once.
    cat.idle = frame.is_idle
    at = pilot.cursor()
    cat.cursor = frame.to_frame(at.x, at.y) if at.visible else None
    start = time.monotonic()
    end = None if seconds is None else start + seconds
    lost = 0.0  # since when the frame is out of sight
    while pilot.connected:
        now = time.monotonic()
        if end is not None and now >= end:
            return
        try:
            with _cli.in_sight():
                if cat.stale and now - cat.read >= READ:
                    cat.stale = False
                    cat.check(now)
                if now >= cat.due:
                    cat.step(now)
        except PtyrcError as exc:
            if exc.code != "no-frame":
                raise
        # The frame may be replaced (a resize of a pane with little
        # text): after a while out of sight, look for it again.
        if frame.usable:
            lost = 0.0
        elif not lost:
            lost = now
        elif now - lost > 2.0:
            lost = 0.0
            new = _cli.pick(pilot, args.frame)
            if new.id != frame.id and cat.sprite is not None:
                cat.sprite.remove()
                cat.sprite = None
            cat.frame = frame = new
            cat.leave(now)
        # Sleep on the socket until the next thing to do, or an event.
        wake = min(cat.due, now + READ if cat.stale else math.inf)
        if not frame.usable:
            wake = min(wake, now + 0.5)
        if end is not None:
            wake = min(wake, end)
        timeout = None if wake == math.inf else max(0.001, wake - time.monotonic())
        for topic, data in pilot.events(timeout=timeout):
            cat.on(topic, data, time.monotonic())
            break


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "cat", "A cat on the text; it sleeps when nothing moves.", frame="largest"
    )
    out.add_argument("--front", action="store_true", help="in front of the text")
    out.add_argument("--seed", type=int, help="same seed, same cat")
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
