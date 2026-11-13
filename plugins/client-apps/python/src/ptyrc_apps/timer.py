# SPDX-License-Identifier: GPL-3.0-or-later
"""Timer: a countdown, a pomodoro, an hourglass, in one widget.

    ptyrc-app timer 25m
    ptyrc-app timer --pomodoro 25/5 --rounds 4 -a
    ptyrc-app timer --until 17:30 --glass
    ptyrc-app timer 10m --print --at 0.4 -a

What it shows:

- one tag, `timer`, a variant per face: `MM:SS`, the time and a bar,
  block digits (those of `clock`); the driver picks the face;
- `-g`: an hourglass. Its sand is counted: `sand(k, n)` is the glass
  after `n` grains fell, and a grain falls every `length / N` seconds:
  the last one lands as the time ends. A new phase turns it over;
- a face is a function of a `State`: `face(state, w, h, look)`. The
  state is that of a `Run` at a time of the session clock; `--speed`
  makes its seconds shorter;
- the end: `-r`, the border `alert`; `-b`, a burst of `confetti`;
  `--modal`, the widget shown again as a modal;
- `--print`: every face at a moment (`--at`), no driver.
"""

from __future__ import annotations

import argparse
import functools
import math
import re
import sys
from collections.abc import Callable, Sequence
from dataclasses import dataclass, replace
from datetime import datetime, timedelta

from ptyrc import Pilot, PtyrcError
from ptyrc import clock as session_clock

from . import _cli, _widgets, clock, confetti

__all__ = [
    "GLASSES",
    "Hover",
    "Look",
    "Phase",
    "Run",
    "State",
    "bar",
    "bulbs",
    "duration",
    "face",
    "glass",
    "ladder",
    "main",
    "plan",
    "pomodoro",
    "sand",
    "short",
    "spin",
    "stamp",
    "title",
    "until",
]

Size = tuple[int, int]
Run_ = dict[str, object]
Row = list[Run_]
Rows = list[Row]
Dot = tuple[int, int]

TURN = 1.2
"""Session seconds a glass takes to turn over."""
RING = 6.0
"""Session seconds the border rings at a change of phase."""
STREAM = 0.2
"""Session seconds between two drawings of a glass that runs."""


# -- the time ---------------------------------------------------------------


@dataclass(frozen=True)
class Phase:
    name: str  # `timer`, `work`, `break`
    length: float  # seconds


_UNITS = re.compile(
    r"(?:(\d+(?:\.\d+)?)h)?(?:(\d+(?:\.\d+)?)m)?(?:(\d+(?:\.\d+)?)s)?", re.ASCII
)


def duration(text: str) -> float:
    """Seconds of `25m`, `90s`, `1h30m`, `1.5h`, `1:30` (minutes and
    seconds), `1:30:00`, or `25` (minutes)."""
    text = text.strip().lower()
    seconds = -1.0
    if re.fullmatch(r"\d+(:\d\d){1,2}", text, re.ASCII):
        seconds = 0.0
        for part in text.split(":"):
            seconds = 60 * seconds + int(part)
    elif re.fullmatch(r"\d+(\.\d+)?", text, re.ASCII):
        seconds = 60 * float(text)
    elif text and (found := _UNITS.fullmatch(text)):
        h, m, s = (float(g or 0) for g in found.groups())
        seconds = 3600 * h + 60 * m + s
    if seconds <= 0:
        raise ValueError(f"not a duration: {text!r}")
    return seconds


def until(text: str, now: datetime) -> float:
    """Seconds from `now` to the next `HH:MM`."""
    try:
        at = datetime.strptime(text.strip(), "%H:%M")
    except ValueError:
        raise ValueError(f"not a time HH:MM: {text!r}") from None
    then = now.replace(hour=at.hour, minute=at.minute, second=0, microsecond=0)
    if then <= now:
        then += timedelta(days=1)
    return (then - now).total_seconds()


def pomodoro(text: str) -> tuple[float, float]:
    """`25/5`: the work and the break, as `duration`."""
    work, sep, rest = text.partition("/")
    if not sep:
        raise ValueError(f"not WORK/BREAK: {text!r}")
    return duration(work), duration(rest)


def plan(
    length: float | None = None,
    work: float | None = None,
    rest: float = 0.0,
    rounds: int = 4,
) -> list[Phase]:
    """The phases: one `timer`, or `rounds` of `work` with a `break`
    between two; `rounds` 0: one work and one break, to loop."""
    if work is None:
        return [Phase("timer", length or 0.0)]
    pair = [Phase("work", work), Phase("break", rest)]
    return pair * (rounds - 1) + pair[:1] if rounds else pair


@dataclass(frozen=True)
class State:
    """A run at a moment."""

    name: str = "timer"
    length: float = 1.0
    left: float = 1.0  # seconds of the phase
    index: int = 0  # phases begun before this one
    round: int = 1  # work phases begun
    rounds: int = 0  # 0: one phase, or no end
    paused: bool = False
    done: bool = False
    over: float = 0.0  # seconds since the end
    beat: float = 0.0  # session seconds: what moves by itself
    turn: float | None = None  # the glass turning over, 0 to 1

    @property
    def share(self) -> float:
        """The share of the phase that is gone."""
        return min(1.0, max(0.0, 1.0 - self.left / self.length))


class Run:
    """Phases run on a clock given: `state(now)`. `speed`: seconds of
    the timer a second of the clock. `loop`: no end."""

    def __init__(
        self,
        phases: Sequence[Phase],
        now: float = 0.0,
        speed: float = 1.0,
        loop: bool = False,
    ) -> None:
        self.phases = list(phases)
        self.speed = speed
        self.loop = loop
        self.elapsed = 0.0
        self.paused = False
        self._at = now

    @property
    def total(self) -> float:
        return sum(p.length for p in self.phases)

    def _advance(self, now: float) -> None:
        if not self.paused:
            self.elapsed += max(0.0, now - self._at) * self.speed
        self._at = now

    def toggle(self, now: float) -> bool:
        """Pause, or go on; whether it is paused now."""
        self._advance(now)
        self.paused = not self.paused
        return self.paused

    def skip(self, now: float, seconds: float) -> None:
        """Move `seconds` of the timer ahead."""
        self._advance(now)
        self.elapsed += seconds

    def state(self, now: float) -> State:
        self._advance(now)
        works = sum(p.name == "work" for p in self.phases)
        rounds = 0 if self.loop or works < 2 else works
        elapsed, lap = self.elapsed, 0
        if self.loop:
            lap, elapsed = divmod(elapsed, self.total)
        if elapsed >= self.total:
            last = self.phases[-1]
            return State(
                last.name,
                last.length,
                0.0,
                len(self.phases) - 1,
                max(1, works),
                rounds,
                done=True,
                over=elapsed - self.total,
                beat=now,
            )
        start = 0.0
        for index, phase in enumerate(self.phases):
            if elapsed < start + phase.length:
                break
            start += phase.length
        begun = int(lap) * len(self.phases) + index
        since = (elapsed - start) / self.speed
        return State(
            phase.name,
            phase.length,
            start + phase.length - elapsed,
            begun,
            int(lap) * works + sum(p.name == "work" for p in self.phases[: index + 1]),
            rounds,
            paused=self.paused,
            beat=now,
            turn=since / TURN if begun and since < TURN else None,
        )


def stamp(left: float) -> str:
    """`MM:SS` under 100 minutes, else `H:MM:SS`; a second begun counts."""
    seconds = math.ceil(max(0.0, left) - 1e-9)
    if seconds < 6000:
        return f"{seconds // 60:02}:{seconds % 60:02}"
    return f"{seconds // 3600}:{seconds // 60 % 60:02}:{seconds % 60:02}"


def short(left: float) -> str:
    """Three cells or so: `45s`, `24m`, `2h`."""
    seconds = math.ceil(max(0.0, left) - 1e-9)
    if seconds < 60:
        return f"{seconds}s"
    if seconds < 3600:
        return f"{math.ceil(seconds / 60)}m"
    return f"{math.ceil(seconds / 360) / 10:g}h"


# -- the look ---------------------------------------------------------------


@dataclass(frozen=True)
class Look:
    """What the options ask of a face."""

    glass: bool = False
    colour: bool = False
    name: str = "timer"


class Hover:
    """The pointer over the widget: `at`, a cell of its inner box, or
    None. `on_move` and `on_leave` are the handlers of a hover."""

    def __init__(self) -> None:
        self.at: tuple[int, int] | None = None

    def on_move(self, x: int, y: int) -> bool:
        """False: the app draws by itself."""
        self.at = (x, y)
        return False

    def on_leave(self) -> bool:
        self.at = None
        return False


SAND, FALL, WOOD, GLASS = 221, 229, 137, 110
"""Colours of the 256, with `-c`: the sand, a grain in the air, the
plates, the glass."""


def tone(state: State, look: Look) -> object:
    """The colour of the digits and of the bar: none without `-c`;
    else by the share left, the break in cyan."""
    if not look.colour:
        return None
    if state.done:
        return "bright-red"
    if state.name == "break":
        return "bright-cyan"
    left = 1.0 - state.share
    if left > 0.5:
        return "bright-green"
    if left > 0.2:
        return "bright-yellow"
    return 208 if left > 0.1 else "bright-red"


def _run(text: str, fg: object = None, **style: object) -> Run_:
    out: Run_ = {"t": text, **{k: v for k, v in style.items() if v}}
    if fg is not None:
        out["fg"] = fg
    return out


def bar(left: float, w: int, fg: object = None) -> Row:
    """A bar `w` cells long that empties from the right: the share
    `left`, half a cell exact."""
    halves = round(min(1.0, max(0.0, left)) * 2 * w)
    full = "━" * (halves // 2) + "╸" * (halves % 2)
    out = [_run(full, fg, bold=True)] if full else []
    if len(full) < w:
        out.append(_run("─" * (w - len(full)), dim=True))
    return out


def label(state: State) -> Row:
    """`work 2/4`, `paused`, `done +0:42`."""
    if state.done:
        over = [_run(" +" + stamp(state.over).removeprefix("0"), dim=True)]
        return [_run("done", bold=True), *(over if state.over >= 1 else [])]
    out = [_run(state.name)]
    if state.name == "timer":
        out = [_run("of " + stamp(state.length), dim=True)]
    elif state.rounds:
        out.append(_run(f" {state.round}/{state.rounds}", dim=True))
    else:
        out.append(_run(f" {state.round}", dim=True))
    if state.paused:
        out = [_run("paused", bold=True), _run(" "), *out]
    return out


def _blink(state: State) -> bool:
    """The dim half of a blink: the end, and a pause."""
    return (state.done or state.paused) and int(2 * state.beat) % 2 == 1


def _stamp(state: State, look: Look) -> Row:
    return [_run(stamp(state.left), tone(state, look), bold=True, dim=_blink(state))]


def line(state: State, w: int, h: int, look: Look) -> Rows:
    """`MM:SS`; in 5 cells an hour and more is `1h24m`."""
    text = stamp(state.left)
    if len(text) > w:
        minutes = math.ceil(state.left / 60)
        text = f"{minutes // 60}h{minutes % 60:02}m"
    row = [_run(text, tone(state, look), bold=True, dim=_blink(state))]
    return clock._fit([[row]], w, h)


def lined(state: State, w: int, h: int, look: Look) -> Rows:
    """The time and a bar, one line."""
    text = _stamp(state, look)
    room = w - clock._width(text) - 1
    return [[*text, _run(" "), *bar(1 - state.share, room, tone(state, look))]]


def plain(state: State, w: int, h: int, look: Look) -> Rows:
    """The time and what runs, over a bar."""
    text, what = _stamp(state, look), label(state)
    gap = w - clock._width(text) - clock._width(what)
    return [[*text, _run(" " * gap), *what], bar(1 - state.share, w, tone(state, look))]


def _digits(state: State, font: Sequence[Sequence[str]], look: Look) -> Rows:
    """Block digits: `MM:SS`, or `H:MM` from 100 minutes."""
    text = stamp(state.left)
    if len(text) > 5:
        minutes = math.ceil(state.left / 60)
        text = f"{minutes // 60:02}:{minutes % 60:02}"
    fg = tone(state, look) or "white"
    colon = clock.DIM if int(state.beat) % 2 and not state.paused else fg
    colours = [fg, fg, colon, fg, fg]
    if _blink(state):
        colours = [clock.DIM] * 5
    return clock._blocks(text, font, colours)  # type: ignore[arg-type]


def _under(state: State, w: int, look: Look) -> Rows:
    """A bar and the label, under block digits."""
    what = label(state)
    pad = max(0, (w - clock._width(what)) // 2)
    return [bar(1 - state.share, w, tone(state, look)), [_run(" " * pad), *what]]


def blocks(
    state: State,
    w: int,
    h: int,
    look: Look,
    *,
    font: Sequence[Sequence[str]],
    more: bool,
) -> Rows:
    """Block digits; `more`: a bar and the label under them."""
    digits = _digits(state, font, look)
    width = max(map(clock._width, digits))
    groups = [digits, [[]], _under(state, width, look)] if more else [digits]
    return clock._fit(groups, w, h)


# -- the hourglass ----------------------------------------------------------

# Braille: the bit of each dot of a cell, by row and column.
_BRAILLE = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))

SLOPE = 0.6
"""Dot rows a dot across: the dip of the sand that leaves, the side of
the heap."""


def _grain(x: int, y: int) -> float:
    """A fixed share of a dot row, by dot: the surface is not a line."""
    return ((x * 73856093) ^ (y * 19349663)) % 89 / 89 * 0.9


@functools.cache
def bulbs(k: int) -> tuple[tuple[Dot, ...], tuple[Dot, ...]]:
    """The dots of a glass of `k` rows a bulb, `2k + 1` cells by
    `2k + 2`: those of the upper bulb in the order they leave, those of
    the lower one in the order they are taken. A cell is 2 dots by 4;
    the plates are the first and last rows of cells, a wall takes a
    cell."""
    cx = 2 * k + 0.5

    def dots(row: int, first: int, last: int) -> list[Dot]:
        xs = range(2 * first, 2 * last + 2)
        return [(x, y) for y in range(4 * row, 4 * row + 4) for x in xs]

    upper = [d for i in range(k) for d in dots(1 + i, i + 1, 2 * k - 1 - i)]
    lower = [d for j in range(k) for d in dots(k + 1 + j, k - j, k + j)]
    if k > 2:  # the sand does not touch the plate
        upper = [d for d in upper if d[1] > 4]
    floor = 4 * (2 * k + 1) - 1
    upper.sort(key=lambda d: d[1] + SLOPE * abs(d[0] - cx) + _grain(*d))
    lower.sort(key=lambda d: floor - d[1] + SLOPE * abs(d[0] - cx) + _grain(*d))
    return tuple(upper), tuple(lower)


def grains(k: int) -> int:
    """The grains of a glass."""
    return len(bulbs(k)[0])


def fallen(k: int, share: float) -> int:
    """The grains fallen when `share` of the time is gone: one every
    `1 / grains(k)`, the last as it ends."""
    n = grains(k)
    return n if share >= 1.0 else min(n - 1, int(n * max(0.0, share)))


def sand(k: int, n: int) -> tuple[frozenset[Dot], frozenset[Dot]]:
    """The sand of a glass after `n` grains fell: the dots of the upper
    bulb, those of the lower."""
    upper, lower = bulbs(k)
    return frozenset(upper[n:]), frozenset(lower[:n])


def _stream(k: int, lower: frozenset[Dot], beat: float) -> set[Dot]:
    """Grains in the air: one dot in three down the middle, from the
    neck to the heap, a dot lower every tenth of a second."""
    x, top, floor = 2 * k, 4 * (k + 1), 4 * (2 * k + 1)
    end = next((y for y in range(top, floor) if (x, y) in lower), floor)
    return {(x, y) for y in range(top, end) if (y - int(10 * beat)) % 3 == 0}


def _cells(
    w: int, h: int, dots: dict[Dot, int], glyphs: dict[Dot, tuple[str, int]]
) -> list[list[tuple[str, int]]]:
    """A grid of `(glyph, rank)`: braille from `dots` (a dot: its
    rank; a cell has that of its highest dot), `glyphs` over it."""
    bits: dict[Dot, list[int]] = {}
    for (x, y), rank in dots.items():
        if 0 <= x < 2 * w and 0 <= y < 4 * h:
            cell = bits.setdefault((x // 2, y // 4), [0, 0])
            cell[0] |= _BRAILLE[y % 4][x % 2]
            cell[1] = max(cell[1], rank)
    grid = []
    for y in range(h):
        row = []
        for x in range(w):
            if (x, y) in glyphs:
                row.append(glyphs[x, y])
            elif (x, y) in bits:
                row.append((chr(0x2800 + bits[x, y][0]), bits[x, y][1]))
            else:
                row.append((" ", 0))
        grid.append(row)
    return grid


# Ranks of a cell of a glass, and their styles: plain, with `-c`.
_NONE, _WALL, _PLATE, _SAND, _FALL = range(5)
_PLAIN: tuple[Run_, ...] = (
    {},
    {"dim": True},
    {},
    {"fg": "yellow"},
    {"fg": "bright-yellow"},
)
_FANCY: tuple[Run_, ...] = (
    {},
    {"fg": GLASS},
    {"fg": WOOD},
    {"fg": SAND},
    {"fg": FALL},
)


def _rows(grid: list[list[tuple[str, int]]], look: Look, last: bool = False) -> Rows:
    """Runs of cells of one rank. `last`: the sand is nearly out, red
    with `-c`."""
    styles = list(_FANCY if look.colour else _PLAIN)
    if last and look.colour:
        styles[_SAND] = {"fg": 209}
    rows: Rows = []
    for cells in grid:
        row: Row = []
        rank = -1
        for glyph, r in cells:
            if r == rank:
                row[-1]["t"] = str(row[-1]["t"]) + glyph
            else:
                row.append({"t": glyph, **styles[r]})
                rank = r
        rows.append(row)
    return rows


def glass(k: int, state: State, look: Look) -> Rows:
    """An hourglass of `k` rows a bulb, `2k + 1` cells by `2k + 2`: the
    sand of the state, a stream while it runs; turning while
    `state.turn`."""
    if state.turn is not None:
        return spin(k, state.turn, look)
    w, h = 2 * k + 1, 2 * k + 2
    n = fallen(k, state.share)
    upper, lower = sand(k, n)
    dots = dict.fromkeys(upper | lower, _SAND)
    if 0 < n < grains(k) and not state.paused:
        dots |= dict.fromkeys(_stream(k, lower, state.beat), _FALL)
    glyphs = {(x, 0): ("▄", _PLATE) for x in range(w)}
    glyphs |= {(x, h - 1): ("▀", _PLATE) for x in range(w)}
    for i in range(k):
        glyphs[i, 1 + i] = glyphs[k + 1 + i, k + 1 + i] = ("╲", _WALL)
        glyphs[2 * k - i, 1 + i] = glyphs[k - 1 - i, k + 1 + i] = ("╱", _WALL)
    last = state.share > 0.9 and not state.done
    return _rows(_cells(w, h, dots, glyphs), look, last)


def _near(p: tuple[float, float], a: Dot, b: Dot, reach: float) -> bool:
    """`p` within `reach` of the segment from `a` to `b`."""
    (px, py), (ax, ay), (bx, by) = p, a, b
    dx, dy = bx - ax, by - ay
    t = ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)
    t = min(1.0, max(0.0, t))
    return math.hypot(px - ax - t * dx, py - ay - t * dy) <= reach


def spin(k: int, turn: float, look: Look) -> Rows:
    """The glass turning over, all in braille: half a turn as `turn`
    goes from 0 to 1, the sand of the end in its bulb; made smaller
    while it does not fit the box."""
    w, h = 2 * k + 1, 2 * k + 2
    turn = min(1.0, max(0.0, turn))
    angle = math.pi * turn * turn * (3 - 2 * turn)
    cos, sin = math.cos(angle), math.sin(angle)
    hw, hh = float(w), 2.0 * h
    scale = min(
        1.0,
        hw / (abs(hw * cos) + abs(hh * sin)),
        hh / (abs(hw * sin) + abs(hh * cos)),
    )
    cx, cy = hw - 0.5, hh - 0.5
    heap = sand(k, grains(k))[1]
    right, low, mid = 2 * w - 1, 4 * h - 4, 4 * (k + 1)
    walls = (
        ((0, 3), (right, 3)),
        ((0, low), (right, low)),
        ((0, 4), (2 * k - 1, mid)),
        ((2 * k - 1, mid), (0, low - 1)),
        ((right, 4), (2 * k + 2, mid)),
        ((2 * k + 2, mid), (right, low - 1)),
    )
    dots: dict[Dot, int] = {}
    for y in range(4 * h):
        for x in range(2 * w):
            dx, dy = (x - cx) / scale, (y - cy) / scale
            u, v = cx + dx * cos + dy * sin, cy - dx * sin + dy * cos
            if (round(u), round(v)) in heap:
                dots[x, y] = _SAND
            elif any(_near((u, v), a, b, 0.5 / scale) for a, b in walls[:2]):
                dots[x, y] = _PLATE
            elif any(_near((u, v), a, b, 0.5 / scale) for a, b in walls[2:]):
                dots[x, y] = _WALL
    return _rows(_cells(w, h, dots, {}), look)


def _over(rows: Rows, w: int, y: int, text: Row) -> Rows:
    """`text` centred on row `y` of `rows`, a blank on each side, over
    what is there."""
    width = clock._width(text) + 2
    if width > w or not 0 <= y < len(rows):
        return rows
    start = (w - width) // 2
    cells: Row = []
    for run in rows[y]:
        cells += [{**run, "t": c} for c in str(run["t"])]
    cells += [{"t": " "}] * (w - len(cells))
    middle: Row = [{"t": " "}, *text, {"t": " "}]
    return [
        *rows[:y],
        [*cells[:start], *middle, *cells[start + width :]],
        *rows[y + 1 :],
    ]


def alone(
    state: State, w: int, h: int, look: Look, hover: bool = False, *, k: int
) -> Rows:
    """The glass alone: the time is the title of the widget. Under the
    pointer, the time in digits over the neck."""
    rows = glass(k, state, look)
    if hover:
        text = stamp(state.left) if w >= 7 else short(state.left)
        rows = _over(rows, w, k, [_run(text, tone(state, look), bold=True)])
    return rows


def beside(
    state: State,
    w: int,
    h: int,
    look: Look,
    hover: bool = False,
    *,
    k: int,
    font: Sequence[Sequence[str]],
) -> Rows:
    """The glass, and block digits, a bar and the label next to it."""
    digits = _digits(state, font, look)
    width = max(map(clock._width, digits))
    right = [*digits, [], *_under(state, width, look)]
    left = glass(k, state, look)
    above = (len(left) - len(right)) // 2
    right = [[] for _ in range(above)] + right
    right += [[] for _ in range(len(left) - len(right))]
    return [[*a, _run("   "), *b] if b else a for a, b in zip(left, right, strict=True)]


Face = Callable[..., Rows]

GLASSES = {(2 * k + 1, 2 * k + 2): k for k in range(2, 7)}
"""The sizes of a glass alone, and its rows a bulb."""


def ladder(look: Look) -> dict[Size, Face]:
    """The faces of a look, by size, smallest first."""
    part = functools.partial
    faces: dict[Size, Face] = {(5, 1): line, (12, 1): lined, (20, 2): plain}
    if not look.glass:
        faces |= {
            (17, 3): part(blocks, font=clock.SMALL, more=False),
            (17, 6): part(blocks, font=clock.SMALL, more=True),
            (32, 5): part(blocks, font=clock.LARGE, more=False),
            (32, 8): part(blocks, font=clock.LARGE, more=True),
        }
        return faces
    faces |= {size: part(alone, k=k) for size, k in GLASSES.items()}
    faces |= {
        (27, 8): part(beside, k=3, font=clock.SMALL),
        (29, 10): part(beside, k=4, font=clock.SMALL),
        (46, 12): part(beside, k=5, font=clock.LARGE),
    }
    return dict(sorted(faces.items(), key=lambda item: (item[0][0] * item[0][1], item)))


def face(state: State, w: int, h: int, look: Look, hover: bool = False) -> Rows:
    """The face of size `w` by `h` of a look. `hover`: the pointer is
    on the widget."""
    draw = ladder(look)[w, h]
    if (w, h) in GLASSES or w > 20 and look.glass:
        return draw(state, w, h, look, hover)
    return draw(state, w, h, look)


def moves(size: Size | None, state: State, look: Look) -> bool:
    """Whether the face of `size` moves by itself: a stream, a turn."""
    wide = size is not None and (size in GLASSES or size[0] > 20)
    return look.glass and wide and not state.done and not state.paused


def title(state: State, size: Size | None, look: Look) -> str:
    """The title of the widget: its name; the time left on a glass
    alone, the only place for it."""
    if not look.glass or size not in GLASSES:
        return look.name if size is None or len(look.name) <= size[0] - 2 else ""
    if state.done:
        return "done"
    return stamp(state.left) if size[0] >= 9 else short(state.left)


# -- print ------------------------------------------------------------------


def boxed(rows: Rows, w: int, h: int, name: str, colour: bool) -> list[str]:
    """`rows` in a plain box of inner size `w` by `h`, titled."""
    edge = (lambda t: f"\x1b[2m{t}\x1b[0m") if colour else (lambda t: t)
    paint = clock.ansi if colour else (lambda run: str(run["t"]))
    room = w - 2
    name = name if len(name) <= room else name[: room - 1] + "…"
    name = f" {name} " if room >= 3 and name else ""
    out = [edge("┌" + "─" * w + "┐")]
    for row in rows + [[] for _ in range(h - len(rows))]:
        body = "".join(paint(run) for run in row) + " " * (w - clock._width(row))
        out.append(edge("│") + body + edge("│"))
    out.append(edge("└" + "─" * (w - len(name))) + name + edge("┘"))
    return out


def show(args: argparse.Namespace, phases: Sequence[Phase], look: Look) -> int:
    """`--print`: the faces at `--at` of the run, or the one that fits
    the size given; ANSI colours on a tty."""
    run = Run(phases, loop=args.rounds == 0 and args.pomodoro is not None)
    run.skip(0.0, args.at * run.total)
    state = replace(run.state(0.0), beat=args.at * run.total)
    faces = ladder(look)
    sizes = list(faces)
    if args.print:
        w, h = args.print
        fit = [s for s in sizes if s[0] <= w and s[1] <= h]
        if not fit:
            print(f"ptyrc-app timer: no face fits {w}x{h}", file=sys.stderr)
            return 1
        sizes = [max(fit, key=lambda s: s[0] * s[1])]
    colour = sys.stdout.isatty()
    for w, h in sizes:
        rows = face(state, w, h, look, args.hover)
        print(f"{w}x{h}")
        print("\n".join(boxed(rows, w, h, title(state, (w, h), look), colour)))
    return 0


# -- live -------------------------------------------------------------------


def _origin(pilot: Pilot, frame: int | str, rect: Sequence[int]) -> tuple[int, int]:
    """The centre of a rectangle of the screen, as a cell of the view
    of `frame`."""
    fx, fy = _cli.pick(pilot, frame).info["rect"][:2]
    return rect[0] + rect[2] // 2 - fx, rect[1] + rect[3] // 2 - fy


def live(
    pilot: Pilot,
    args: argparse.Namespace,
    phases: Sequence[Phase],
    look: Look,
    seconds: float | None,
) -> None:
    now = session_clock.monotonic
    loop = args.rounds == 0 and args.pomodoro is not None
    run = Run(phases, now(), args.speed, loop)
    run.skip(now(), args.at * run.total)
    faces = ladder(look)
    hover = Hover()

    def render(w: int, h: int) -> Rows:
        return face(run.state(now()), w, h, look, hover.at is not None)

    deck = _widgets.Deck(pilot, args)
    widget = deck.add("timer", list(faces), render, title=look.name)
    if look.glass:
        # While the pointer is there: a mouse at rest sends nothing.
        deck.hover_timeout = None
        deck.on_hover(widget, hover.on_move, hover.on_leave)
    if args.click:
        pilot.subscribe("mouse")

    def shown(border: str | None = None, modal: bool = False) -> None:
        """Show the widget again: in `border`, or as it was declared."""
        if border is None and not modal:
            deck.resize(widget, list(faces))
            return
        options = {**deck.shared, "title": look.name, "modal": modal}
        options["border"] = border or options["border"]
        widget.info = pilot.widget("timer", list(faces), content=render, **options).info

    def burst(size: int) -> None:
        rect = widget.rect
        if rect is None:
            return
        frame = deck.shared.get("frame") or "raw"
        try:
            confetti.burst(
                pilot,
                frame=frame,
                size=size,
                origin=_origin(pilot, frame, rect),
                seed=args.seed,
            )
        except PtyrcError as exc:
            if exc.code != "not-usable":
                raise

    end = None if seconds is None else now() + seconds
    sent: tuple[object, ...] = ()
    before = run.state(now())
    ring: float | None = None  # when the border stops ringing
    leave: float | None = None  # when the app ends, `--hold`
    while True:
        at = now()
        if end is not None and at >= end or leave is not None and at >= leave:
            return
        state = run.state(at)
        if state.done and not before.done:
            if args.ring or args.modal:
                shown("alert" if args.ring else None, args.modal)
                ring, sent = None, ()
            if args.burst:
                burst(96)
            if args.hold is not None:
                leave = at + args.hold
        elif state.index != before.index:
            if args.ring:
                shown("alert")
                ring, sent = at + RING, ()
            if args.burst:
                burst(32)
        if ring is not None and at >= ring:
            shown()
            ring, sent = None, ()
        before = state
        size = widget.size
        if size is not None:
            rows = face(state, *size, look, hover.at is not None)
            name = title(state, size, look)
            if (size, rows, name) != sent:
                widget.set(rows, title=name if (size, name) != sent[::2] else None)
                sent = (size, rows, name)
        # The next drawing: when a second of the timer ends; sooner
        # while a glass runs.
        wait = (state.left % 1.0 or 1.0) / run.speed + 0.01
        if state.done or state.paused:
            wait = 0.5
        elif state.turn is not None and moves(size, state, look):
            wait = 0.08
        elif moves(size, state, look):
            wait = min(wait, STREAM)
        due = min(d for d in (at + max(0.05, wait), end, ring, leave) if d is not None)
        event = pilot.event(timeout=max(0.0, due - now()))
        while event is not None:
            topic, data = event
            if topic == "exit":
                return
            if topic == "move":
                deck.move(data["frame"])
                if ring is not None or state.done and (args.ring or args.modal):
                    shown("alert" if args.ring else None, state.done and args.modal)
                sent = ()
            elif topic == "widgets":
                place = (widget.variant, widget.rect)
                if deck.take(data) and place != (widget.variant, widget.rect):
                    deck.leave()
            elif topic == "hover":
                deck.hover(data)
            elif topic == "mouse" and args.click:
                mine = data.get("sprite") == widget.info.get("sprite")
                if mine and data.get("action") == "press":
                    run.toggle(now())
            event = pilot.event(timeout=0)


# -- options ----------------------------------------------------------------


def _typed(parse: Callable[[str], object]) -> Callable[[str], object]:
    def typed(text: str) -> object:
        try:
            return parse(text)
        except ValueError as exc:
            raise argparse.ArgumentTypeError(str(exc)) from None

    typed.__name__ = getattr(parse, "__name__", "value")
    return typed


def _size(text: str) -> Size:
    w, sep, h = text.partition("x")
    if not (sep and w.isdigit() and h.isdigit() and int(w) > 0 and int(h) > 0):
        raise ValueError(f"not a size WxH: {text!r}")
    return int(w), int(h)


def _share(text: str) -> float:
    value = float(text)
    if not 0.0 <= value <= 1.0:
        raise ValueError(f"not a share, 0 to 1: {text!r}")
    return value


def _positive(text: str) -> float:
    value = float(text)
    if not 0.0 < value < float("inf"):
        raise ValueError(f"not a positive number: {text!r}")
    return value


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("timer", "A countdown, a pomodoro, an hourglass.")
    _widgets.options(out, border="round")
    out.add_argument(
        "duration",
        nargs="?",
        type=_typed(duration),
        metavar="DURATION",
        help="25m, 90s, 1h30m, 1:30, 25 (minutes) [default: 25m]",
    )
    out.add_argument(
        "--until", metavar="HH:MM", help="count down to that time of the day"
    )
    out.add_argument(
        "--pomodoro",
        type=_typed(pomodoro),
        metavar="WORK/BREAK",
        help="work and break in turn, as 25/5",
    )
    out.add_argument(
        "--rounds",
        type=int,
        default=4,
        metavar="N",
        help="work phases of a pomodoro; 0: no end [default: 4]",
    )
    out.add_argument("-g", "--glass", action="store_true", help="an hourglass")
    out.add_argument(
        "-c", "--colour", action="store_true", help="colours, by the time left"
    )
    out.add_argument(
        "-r",
        "--ring",
        action="store_true",
        help="the border chases at the end, and a while at a new phase",
    )
    out.add_argument(
        "-b", "--burst", action="store_true", help="confetti at the end of a phase"
    )
    out.add_argument("-a", "--all", action="store_true", help="-g -c -r -b")
    out.add_argument(
        "--modal",
        action="store_true",
        help="at the end, a modal: over the text if it must",
    )
    out.add_argument(
        "--click",
        action="store_true",
        help="a click of the host mouse on the widget pauses; takes the mouse",
    )
    out.add_argument(
        "--hold",
        type=_typed(_positive),
        metavar="SECONDS",
        help="leave that long after the end [default: stay]",
    )
    out.add_argument("--name", default="timer", help="the title of the widget")
    out.add_argument(
        "--speed",
        type=_typed(_positive),
        default=1.0,
        metavar="N",
        help="seconds of the timer a second [default: 1]",
    )
    out.add_argument(
        "--at",
        type=_typed(_share),
        default=0.0,
        metavar="SHARE",
        help="start with that share of the time gone, 0 to 1",
    )
    out.add_argument("--seed", type=int, help="seed of the confetti")
    out.add_argument(
        "--print",
        nargs="?",
        type=_typed(_size),
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the faces once, or the one that fits the size",
    )
    out.add_argument("--hover", action="store_true", help=argparse.SUPPRESS)
    args = _cli.parse(out, argv)
    if sum(x is not None for x in (args.duration, args.until, args.pomodoro)) > 1:
        out.error("one of DURATION, --until, --pomodoro")
    if args.rounds < 0:
        out.error("--rounds: 0 or more")
    if args.until is not None:
        try:
            args.duration = until(args.until, datetime.now())
        except ValueError as exc:
            out.error(str(exc))
    if args.all:
        args.glass = args.colour = args.ring = args.burst = True
    return args


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    look = Look(args.glass, args.colour, args.name)
    if args.pomodoro is not None:
        phases = plan(None, *args.pomodoro, rounds=args.rounds)
    else:
        phases = plan(args.duration or duration("25m"))
    if args.print is not None:
        return show(args, phases, look)
    return _cli.run(lambda pilot, a: live(pilot, a, phases, look, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
