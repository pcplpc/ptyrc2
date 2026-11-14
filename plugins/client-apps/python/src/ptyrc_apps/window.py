# SPDX-License-Identifier: GPL-3.0-or-later
"""Window: a house window on a landscape, by day and by night.

    ptyrc-app window
    ptyrc-app window -a --prefer top-right
    ptyrc-app window -ly --place mountain --at 21:30 --season winter
    ptyrc-app window -a --print 40x12 --at 07:10
    ptyrc-app window -a -r

What it shows:

- one widget, a variant a size, from a porthole of 7 by 3 to a bay of
  40 by 12: the same landscape, cut wider; the border is the frame of
  the window, a sill under it; the mullions are content;
- a scene that is a function of the time: `scene(w, h, when, t)` is
  pure; `when` is the local time of the place (the light, the sun and
  the moon of `_sky`, the season), `t` the seconds the app has run
  (what moves). `--at`, `--season`: where the time starts; `--speed`:
  how fast it goes, 1440: a day a minute;
- pixels of half a cell (`_canvas.Half`), the land of `_strip` from
  a seed: one a day unless `--seed`;
- bare: the sky, the sun, the moon in its phase, stars, a hill and a
  meadow. `-l`: the land: ridges, trees, houses lit at night; `-y`:
  the season of the date: blossom, green, leaves turned, snow; `-w`:
  the weather of the day: clouds, rain, fog, snow; `-p`: what passes:
  birds, smoke, a balloon, a meteor, a plane, the stars twinkle; `-c`:
  curtains and a cat on the sill; `-a`: all, and the place of the day;
- indoors, the mullions, the curtains and the cat: colours that stand
  out from the view behind them and from each other, lit by a lamp at
  night; the cat at rest between the mullions;
- the cat (`_mouser`): alone, it sits, looks aside, grooms, stretches,
  watches the birds, sleeps at night; its tail sways;
- the pointer over the window (`_strip.Pointer`): the curtains move
  aside, a house near it lights up, the cat hunts it all along the
  sill: it runs, crouches, pounces, bats what it caught, looks for
  what it lost, walks home; 12 frames a second meanwhile;
- `-r`: what is not given is drawn at random, another at each run:
  the place, the season and its day, the time (`MOMENTS`: more often
  a low sun or an evening than a noon), the weather; `--seed`: the
  same draws;
- `--print [WxH]`: no driver, the variants on stdout.
"""

from __future__ import annotations

import argparse
import datetime
import math
import random
import sys
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from functools import cache, lru_cache
from typing import Any

from ptyrc import Pilot, clock

from . import _cli, _mouser, _sky, _widgets
from ._canvas import RGB, Braille, Half, Rows, ansi, mix, rgb, xterm
from ._strip import (
    GROUND,
    LAMP,
    WHITE,
    Layer,
    Mark,
    Pointer,
    Strip,
    clouds,
    fog,
    heavens,
    lights,
    noise,
    paint,
    rain,
    snow,
    stars,
    water,
)

__all__ = [
    "BORDER",
    "PLACES",
    "SIZES",
    "Era",
    "Look",
    "boxed",
    "drawn",
    "main",
    "scene",
    "sized",
]

Size = tuple[int, int]

#: The inner sizes of the variants.
SIZES: tuple[Size, ...] = ((7, 3), (12, 4), (18, 6), (26, 8), (40, 12))
PLACES = ("countryside", "sea", "mountain", "city")
#: The frame: thin jambs, a sill.
BORDER: dict[str, Any] = {
    "corners": "┌┐╘╛",
    "top": "─",
    "bottom": "═",
    "left": "│",
    "right": "│",
    "style": {"fg": 180},
}
MIDDLE = {"spring": (4, 20), "summer": (7, 20), "autumn": (10, 20), "winter": (1, 20)}

FRAME: RGB = (196, 156, 116)
#: The paints of the mullions: `FRAME` unless it melts into the view.
FRAMES: tuple[RGB, ...] = (
    FRAME,
    (238, 236, 228),
    (84, 54, 38),
    (46, 88, 62),
    (54, 76, 116),
    (132, 50, 44),
)
FABRICS: tuple[RGB, ...] = (
    (178, 62, 66),
    (222, 208, 180),
    (70, 110, 140),
    (214, 162, 48),
    (58, 116, 78),
    (118, 58, 108),
    (190, 94, 58),
    (40, 50, 96),
    (36, 126, 128),
    (240, 236, 228),
)
COATS: tuple[RGB, ...] = (
    (26, 24, 28),
    (206, 126, 56),
    (120, 118, 122),
    (238, 234, 226),
    (218, 196, 160),
    (104, 72, 48),
    (76, 78, 92),
)
#: The lamp of the room: what lights the inside at night; the edge of
#: the cat towards it.
ROOM: RGB = (255, 228, 192)
RIM: RGB = (250, 230, 200)
#: The least contrast of a thing indoors with what is behind it, and
#: with the other things indoors (`_apart`).
FLOOR = 36.0
SLATE: RGB = (128, 130, 140)

PERIOD = 0.25  # seconds a frame, when something moves
BRISK = 1 / 12  # when the cat is up to something
STILL = 20.0  # seconds a frame, else


@dataclass(frozen=True)
class Look:
    """What a window shows, the time apart."""

    seed: int = 0
    place: str = "countryside"
    lat: float = 47.0
    land: bool = False
    seasons: bool = False
    weather: str | None = None  # None: clear, no cloud; `day`: the day's
    passing: bool = False
    cosy: bool = False
    deep: bool = False  # colours as `#rrggbb`

    @property
    def moving(self) -> bool:
        return self.passing or self.cosy or self.weather is not None


class Era:
    """The time of the scene: `when(t)`, the local time after `t`
    seconds of the app, `speed` seconds of the scene a second. It
    starts at `start`: at the time `at`, in the middle of `season`,
    `days` later."""

    def __init__(
        self,
        start: datetime.datetime,
        speed: float = 1.0,
        at: tuple[int, int] | None = None,
        season: str | None = None,
        days: int = 0,
    ) -> None:
        if at is not None:
            start = start.replace(hour=at[0], minute=at[1], second=0)
        if season is not None:
            month, day = MIDDLE[season]
            start = start.replace(month=month, day=day)
        start += datetime.timedelta(days=days)  # of the season: its middle is the 20th
        self.start, self.speed = start.replace(tzinfo=None, microsecond=0), speed

    def when(self, t: float) -> datetime.datetime:
        return self.start + datetime.timedelta(seconds=t * self.speed)


# --- the land of each place ------------------------------------------


def _tall(look: Look, rows: int, share: float, lo: int, hi: int) -> float:
    return float(min(hi, max(lo, round(rows * share))))


def _countryside(look: Look, w: int, rows: int) -> tuple[int, list[Layer]]:
    s = look.seed
    hz = round(rows * 0.6)
    far = Layer(Strip(s + 1, "hills", w / 4 + 4), 0.5, hz + rows * 0.12, rows * 0.34)
    near = Layer(Strip(s + 2, "hills", w / 2.5 + 5), 0.0, rows * 0.96, rows * 0.26)
    if not look.land or rows < 8:
        return hz, [far, near] if rows > 6 else [far]
    ridge = Layer(Strip(s, "mountains", w / 2 + 5), 0.82, hz + rows * 0.02, rows * 0.24)
    far.things = ("pine", "tree", "tree", "bush")
    far.gap, far.density, far.size = 4, 0.55, _tall(look, rows, 0.14, 2, 4)
    near.things = ("tree", "tree", "house", "tree", "pine", "sheep", "bush", "hay")
    near.size = _tall(look, rows, 0.32, 3, 9)
    near.gap, near.density = round(near.size) + 1, 0.8
    return hz, [ridge, far, near]


def _sea(look: Look, w: int, rows: int) -> tuple[int, list[Layer]]:
    s = look.seed
    hz = round(rows * 0.56)
    amp = rows * 0.34
    isles = Layer(Strip(s + 1, "hills", w / 2 + 5), 0.6, hz + amp * 0.56, amp)
    isles.colour = (60, 130, 70)
    if not look.land or rows < 8:
        return hz, [isles]
    isles.things, isles.gap, isles.size = ("pine", "bush"), 3, 2.0
    shore = Layer(Strip(s + 2, "dunes", w / 1.2 + 8), 0.0, rows * 1.02, rows * 0.34)
    shore.colour = {**GROUND, "summer": (206, 190, 140), "spring": (206, 190, 140)}
    shore.colour["autumn"] = (196, 178, 132)
    shore.things = ("palm", "rock", "bush", "house", "rock")
    shore.size = _tall(look, rows, 0.3, 3, 7)
    shore.gap, shore.density = round(shore.size) + 4, 0.6
    return hz, [isles, shore]


def _mountain(look: Look, w: int, rows: int) -> tuple[int, list[Layer]]:
    s = look.seed
    hz = round(rows * 0.7)
    rock = (112, 112, 124)
    peaks = Layer(Strip(s, "mountains", w / 3.5 + 4), 0.7, hz + 1, rows * 0.62, rock)
    peaks.snow = 0.42
    if rows <= 6:
        return hz, [peaks]
    near = Layer(Strip(s + 2, "hills", w / 1.6 + 6), 0.0, rows * 0.95, rows * 0.2)
    if not look.land or rows < 8:
        return hz, [peaks, near]
    mid = Layer(Strip(s + 1, "mountains", w / 2.2 + 4), 0.42, hz + 2, rows * 0.36)
    mid.colour, mid.snow = {**GROUND, "summer": (78, 112, 84)}, 0.86
    mid.things, mid.gap, mid.density, mid.size = ("pine",), 3, 0.5, 2.0
    near.things = ("pine", "pine", "house", "pine", "rock", "tree")
    near.size = _tall(look, rows, 0.3, 3, 8)
    near.gap, near.density = round(near.size * 0.6) + 2, 0.75
    return hz, [peaks, mid, near]


def _city(look: Look, w: int, rows: int) -> tuple[int, list[Layer]]:
    s = look.seed
    hz = round(rows * 0.8)
    far = Layer(Strip(s, "city", 4), 0.55, rows, rows * 0.5, SLATE)
    if rows <= 6:
        return hz, [far]
    near = Layer(Strip(s + 1, "city", 5 if w < 26 else 6), 0.12, rows, rows * 0.62)
    near.colour = (92, 94, 104)
    if not look.land or rows < 8:
        return hz, [far, near]
    hills = Layer(
        Strip(s + 2, "hills", w / 2 + 6), 0.85, hz, rows * 0.24, (96, 110, 128)
    )
    return hz, [hills, far, near]


_LAND = {
    "countryside": _countryside,
    "sea": _sea,
    "mountain": _mountain,
    "city": _city,
}


# --- what the scene is made of ---------------------------------------


WINGS = ("⠑⠊", "⠒⠒", "⠊⠑", "⠒⠒")


def _flock(w: int, light: float, seed: int, t: float, hz: int) -> list[tuple[int, int]]:
    """The cells of the birds that cross the sky at `t`."""
    slot, cross = 45.0, 32.0
    k = int(t // slot)
    p = (t - k * slot) / cross
    if p > 1 or noise(seed + 41, k) > 0.8 or light < 0.45:
        return []
    count = 1 if w < 12 else 1 + int(noise(seed + 42, k) * 4)
    back = noise(seed + 43, k) < 0.5
    top = max(1, hz // 2 - 2)
    row = int(noise(seed + 44, k) * top)
    out = []
    for i in range(count):
        x = round(-3 + p * (w + 6 + 3 * count)) - 3 * i
        x = w - 2 - x if back else x
        y = row + (i + 1) // 2 * (1 if i % 2 else -1) + round(math.sin(t * 0.5 + i))
        out.append((x, y))
    return out


def _birds(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    for i, (x, y) in enumerate(_flock(half.w, sky.light, seed, t, hz)):
        if not (0 <= y < half.h and 0 <= x < half.w - 1):
            continue
        under = half.px[2 * y][x]
        ink = mix(under, (24, 26, 36), 0.8)
        half.put(x, y, WINGS[int(t * 3 + i * 1.7) % 4], ink)


def _meteor(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    slot, burn = 38.0, 0.9
    k = int(t // slot)
    p = (t - k * slot - 3) / burn
    if not 0 <= p <= 1 or noise(seed + 51, k) > 0.55 or sky.stars < 0.5:
        return
    dots = Braille(half.w, half.h)
    wide, tall = 2 * half.w, 2 * hz
    x0 = (0.15 + 0.7 * noise(seed + 52, k)) * wide
    y0 = 0.05 * tall
    lean = -1 if noise(seed + 53, k) < 0.5 else 1
    run = 0.55 * min(wide, 2 * tall)
    head, tail = p * run, max(0.0, p * run - 0.3 * run)
    dots.line(x0 + lean * tail, y0 + tail * 0.5, x0 + lean * head, y0 + head * 0.5)
    half.ink(dots, (255, 250, 226))


def _smoke(
    half: Half, sky: _sky.Sky, marks: Sequence[Mark], seed: int, t: float
) -> None:
    grey = sky.shade((214, 214, 220))
    for mark in marks:
        if mark.kind != "chimney" or noise(seed + 61, mark.n) > 0.75:
            continue
        for i in range(4):
            age = (t * 0.7 + i) % 4
            x = mark.x + round(age * 0.6 + math.sin(t * 0.9 + i * 2.1) * 0.7)
            half.set(x, mark.y - 1 - round(age * 1.2), grey, 0.62 * (1 - age / 4.4))


def _balloon(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    slot, cross = 260.0, 220.0
    k = int((t + 250) // slot)
    p = (t + 250 - k * slot) / cross
    if p > 1 or half.w < 18 or sky.light < 0.6 or noise(seed + 71, k) > 0.7:
        return
    x = round(-3 + p * (half.w + 6))
    x = half.w - x if noise(seed + 72, k) < 0.5 else x
    y = round(hz * (0.22 + 0.2 * noise(seed + 73, k)) + math.sin(t * 0.21) * 1.2)
    side, band = ((224, 62, 62), (250, 206, 76)) if k % 2 else ((66, 120, 210), WHITE)
    for dy, wide in ((-3, 1), (-2, 3), (-1, 3), (0, 1)):
        for dx in range(-(wide // 2), wide // 2 + 1):
            half.set(x + dx, y + dy, sky.shade(side if dx else band))
    half.set(x, y + 2, sky.shade((110, 80, 54)))


def _plane(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    slot, cross = 80.0, 34.0
    k = int((t + 20) // slot)
    p = (t + 20 - k * slot) / cross
    if p > 1 or half.w < 12 or sky.light > 0.25 or noise(seed + 81, k) > 0.6:
        return
    x = round(p * (half.w + 2)) - 1
    x = half.w - 1 - x if noise(seed + 82, k) < 0.5 else x
    y = round(hz * (0.1 + 0.3 * noise(seed + 83, k)) - p * 2)
    half.set(x, y, (255, 70, 60) if int(t * 2) % 2 else (190, 196, 220))


def _boat(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    slot, cross = 150.0, 130.0
    k = int((t + 40) // slot)
    p = (t + 40 - k * slot) / cross
    if p > 1 or half.w < 12 or noise(seed + 91, k) > 0.8:
        return
    x = round(-3 + p * (half.w + 6))
    x = half.w - x if noise(seed + 92, k) < 0.5 else x
    y = hz + 1 + int(noise(seed + 93, k) * 2)
    hull, sail = sky.shade((120, 70, 50)), sky.shade((246, 244, 236))
    half.rect(x - 1, y, 3, 1, hull)
    half.set(x, y - 1, sail)
    half.set(x, y - 2, sail)
    half.set(x + 1, y - 1, sail)
    if sky.light < 0.3:
        half.set(x, y - 3, LAMP)


# --- indoors ---------------------------------------------------------


def _lit(c: RGB, light: float) -> RGB:
    """`c` indoors: as it is by day, under the lamp of the room at night."""
    lamp = (c[0] * ROOM[0] // 400, c[1] * ROOM[1] // 400, c[2] * ROOM[2] // 400)
    return mix(lamp, c, light)


@cache
def _lab(n: int) -> tuple[float, float, float]:
    """CIE L*a*b* of the colour `n` of the 256."""

    def lin(v: int) -> float:
        u = v / 255
        return u / 12.92 if u <= 0.04045 else ((u + 0.055) / 1.055) ** 2.4

    def f(u: float) -> float:
        return u ** (1 / 3) if u > 0.008856 else 7.787 * u + 16 / 116

    r, g, b = (lin(v) for v in rgb(n))
    x = f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047)
    y = f(0.2126 * r + 0.7152 * g + 0.0722 * b)
    z = f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883)
    return 116 * y - 16, 500 * (x - y), 200 * (y - z)


def _apart(a: RGB, b: RGB) -> float:
    """Contrast of two colours as the 256 show them: CIE76 with the
    lightness weighed 1.5."""
    return _gap(xterm(a), xterm(b))


@cache
def _gap(m: int, n: int) -> float:
    (l0, a0, b0), (l1, a1, b1) = _lab(m), _lab(n)
    return math.sqrt((1.5 * (l0 - l1)) ** 2 + (a0 - a1) ** 2 + (b0 - b1) ** 2)


#: What is behind a thing indoors: the light and the pixels, by day and
#: by night.
Behind = tuple[tuple[float, tuple[RGB, ...]], ...]


def _rim(c: RGB, light: float) -> RGB:
    """The edge of the cat in `c`: lit by the lamp at night, the more so
    as the coat is dark."""
    return mix(_lit(c, light), RIM, 0.5 * (1 - light) * (1 - max(c) / 255))


def _against(c: RGB, behind: Behind, rim: bool = False) -> float:
    """Contrast of `c` indoors with the pixels behind it: the mean of the
    lowest third of the contrasts per pixel, the worse of day and night.
    `rim`: the better of `c` and its rim."""
    worst = math.inf
    for light, pixels in behind:
        best = 0.0
        for ink in (_lit(c, light), _rim(c, light))[: 1 + rim]:
            seen = sorted(_apart(ink, p) for p in pixels)
            low = seen[: max(1, len(seen) // 3)]
            best = max(best, sum(low) / len(low))
        worst = min(worst, best)
    return worst


def _between(a: RGB, b: RGB) -> float:
    """Contrast of two things indoors, the worse of day and night."""
    return min(_apart(_lit(a, light), _lit(b, light)) for light in (0.0, 1.0))


def _pick(
    among: Sequence[RGB],
    behind: Behind,
    others: Sequence[tuple[RGB, float]],
    order: list[int],
    rim: bool = False,
) -> tuple[RGB, float]:
    """The first of `among` in `order` with `FLOOR` of contrast with what
    is behind it, and with each of `others` its share of `FLOOR`; else
    the one of most contrast. With its contrast."""

    def fit(c: RGB) -> float:
        apart = (_between(c, o) / k for o, k in others)
        return min([_against(c, behind, rim), *apart])

    return _first((among[i], fit(among[i])) for i in order)


def _first(fits: Iterable[tuple[Any, float]]) -> Any:
    """The first of `fits` of `FLOOR` at least; else the best."""
    best: Any = None
    for it in fits:
        if it[1] >= FLOOR:
            return it
        if best is None or it[1] > best[1]:
            best = it
    return best


@dataclass(frozen=True)
class Room:
    """The things indoors: the paint of the mullions, the cloth of the
    curtains, the coat of the cat and where it sits (`_perch`)."""

    frame: RGB
    cloth: RGB
    coat: RGB
    perch: tuple[int, int] | None


def _bars(w: int, h: int) -> tuple[list[int], int | None]:
    """The columns of the mullions, and the pixel row of the rail."""
    if w < 18:
        return [], None
    cols = [w // 2] if w < 34 else [w // 3, w - 1 - w // 3]
    return cols, 2 * (h // 3) + 1 if h >= 8 else None


def _mullions(half: Half, sky: _sky.Sky, paint: RGB) -> None:
    w, h = half.w, half.h
    cols, rail = _bars(w, h)
    ink = _lit(paint, sky.light)
    for x in cols:
        for y in range(2 * h):
            half.px[y][x] = ink
            half.over.pop((x, y // 2), None)
    if rail is not None:
        for x in range(w):
            half.px[rail][x] = ink
            half.over.pop((x, rail // 2), None)


def curtain(w: int, ease: float) -> int:
    """Cells of curtain on each side: drawn aside when `ease` is 1."""
    full = max(1, w // 8)
    return max(1, round(full - ease * (full - max(1, full // 3))))


def _curtains(half: Half, sky: _sky.Sky, cloth: RGB, ease: float) -> None:
    w, h = half.w, half.h
    cloth = _lit(cloth, sky.light)
    wide = curtain(w, ease) if w >= 12 else 0  # none on a porthole
    for i in range(wide):
        # Folds: a light cell, a dark one; denser when drawn aside.
        fold = (0.0, 0.2, 0.36)[(i * (2 if ease > 0.5 else 1) + (i == wide - 1)) % 3]
        c = mix(cloth, (20, 14, 24), fold)
        for y in range(h):
            for x in (i, w - 1 - i):
                half.put(x, y, " ", bg=c)
                half.px[2 * y][x] = half.px[2 * y + 1][x] = c


CAT = _mouser.SIT.rows


def _tail(left: int, way: int) -> list[int]:
    """The columns of the tail of a cat at `left`, to the right if `way`
    is 1, to the left if -1."""
    return [left + 2 + way * k for k in (3, 4, 5)]


def _perch(w: int, h: int, seed: int) -> tuple[int, int] | None:
    """Where the cat sits: its left column and the way of its tail; None
    if no cat. The body clear of the mullions and of the drawn
    curtains, the tail clear of the mullions; a gap to a mullion and a
    tail in the open preferred. Among those, by `seed`."""
    if w < 18 or h < 6:
        return None
    cols, _ = _bars(w, h)
    side = curtain(w, 0.0)
    fits: list[tuple[int, int, int, int]] = []
    for left in range(side, w - side - 4):
        if any(left <= x <= left + 4 for x in cols):
            continue
        for way in (1, -1):
            tail = _tail(left, way)
            if any(x in cols or not 0 <= x < w for x in tail):
                continue
            hidden = sum(x < side or x >= w - side for x in tail)
            near = any(left - 1 <= x <= left + 5 for x in cols)
            fits.append((hidden, near, left, way))
    if not fits:  # not at the sizes there are
        return side + 1, 1
    best = min(fits)[:2]
    fits = [f for f in fits if f[:2] == best]
    _, _, left, way = fits[int(noise(seed + 112) * len(fits))]
    return left, way


@cache
def _sill(w: int, h: int, seed: int, perch: tuple[int, int]) -> _mouser.Sill:
    """The sill of the cat at `perch`: it has the run of the window,
    between the curtains drawn aside."""
    cols, _ = _bars(w, h)
    home = perch[0] + 2
    lo, hi, left, right = (
        f(home, x)
        for side in (curtain(w, 1.0), curtain(w, 0.0))
        for f, x in ((min, side + 2), (max, w - 3 - side))
    )
    return _mouser.Sill(
        seed, w, 2 * h, home, perch[1], tuple(cols), lo, hi, left, right
    )


def _cat(
    half: Half,
    sky: _sky.Sky,
    t: float,
    pointer: Pointer | None,
    room: Room,
    cat: _mouser.Cat | None = None,
    seed: int = 0,
    prey: float | None = None,
) -> None:
    """A cat on the sill (`_mouser`): alone, it lives its life; it hunts
    the pointer all along the sill. Indoors, before the mullions: a
    cell it covers loses the glyph from outside. At rest it is on no
    mullion. `cat`: the one that hunts, else one from the trail of the
    pointer; `prey`: the column of what flies outside."""
    w, rows = half.w, 2 * half.h
    if room.perch is None:
        return
    sill = _sill(w, half.h, seed, room.perch)
    pose = (cat or _mouser.Cat()).at(t, pointer, sill, sky.light, prey)
    fur = [(x, y) for x, y in _mouser.cells(pose, rows) if 0 <= x < w and 0 <= y < rows]
    if pose.shape.back:  # the tail goes behind a mullion
        fur = [(x, y) for x, y in fur if x not in sill.cols or abs(x - pose.x) < 3]
    # The top of each column: the rim the lamp lights at night.
    edge: dict[int, int] = {}
    for x, y in fur:
        edge[x] = min(y, edge.get(x, rows))
    coat, rim = _lit(room.coat, sky.light), _rim(room.coat, sky.light)
    for x, y in fur:
        half.set(x, y, rim if edge[x] == y else coat)
        half.over.pop((x, y // 2), None)


def sized(w: int, h: int) -> Size:
    """The pixels of a box: `w` by `2h`."""
    return w, 2 * h


#: The share of the sky the clouds cover, by weather.
COVER = {"clouds": 0.25, "rain": 0.85, "fog": 0.55, "snow": 0.75}


def _outside(
    w: int,
    h: int,
    look: Look,
    sky: _sky.Sky,
    season: str,
    kind: str | None,
    t: float,
    pointer: Pointer | None,
    live: bool = True,
) -> Half:
    """The view, no window. Not `live`: the land, the clouds, the fog;
    nothing that falls, passes or twinkles."""
    seed = look.seed
    half = Half(w, h)
    rows = 2 * h
    hz, layers = _LAND[look.place](look, w, rows)
    clear = heavens(half, sky, hz)
    if kind is not None:
        amount = {"clear": 0.14, "clouds": 0.6}.get(kind, 0.3)
        clouds(half, sky, seed + 7, hz, amount, t * 0.25)
        if sky.cover < 0.5:  # stars and birds show between the clouds
            clear = [list(row) for row in half.px]
    marks: list[Mark] = []
    for layer in layers:
        if look.place == "sea" and layer.depth == 0.0:
            water(half, sky, hz, seed, t)
        marks += paint(half, layer, sky, season=season)
    if look.place == "sea" and layers[-1].depth != 0.0:
        water(half, sky, hz, seed, t)
    if look.land:
        lights(half, sky, marks, seed, pointer)
    if live and look.passing:
        _smoke(half, sky, marks, seed, t)
        _balloon(half, sky, seed, t, hz)
        _plane(half, sky, seed, t, hz)
        if look.place == "sea":
            _boat(half, sky, seed, t, hz)
    if kind == "fog":
        fog(half, sky, hz)
    if not live:
        return half
    stars(half, clear, sky, seed + 3, t if look.passing else None, hz)
    if look.passing and kind not in ("rain", "fog", "snow"):
        _birds(half, sky, seed, t, hz)
        _meteor(half, sky, seed, t, hz)
    if kind == "rain":
        rain(half, sky, seed, t)
    elif kind == "snow":
        snow(half, sky, seed, t)
    return half


@lru_cache(maxsize=64)
def _views(
    look: Look, w: int, h: int, season: str, kind: str | None
) -> tuple[tuple[float, Half], ...]:
    """The view at noon and at midnight in the middle of `season`, still:
    what the things indoors are chosen against."""
    month = MIDDLE[season][0]
    if look.lat < 0:
        month = (month + 5) % 12 + 1
    out = []
    for hour in (12, 0):
        when = datetime.datetime(2001, month, 20, hour, 30)
        sky = _sky.at(when, look.lat, COVER.get(kind or "", 0.0))
        out.append(
            (sky.light, _outside(w, h, look, sky, season, kind, 0.0, None, False))
        )
    return tuple(out)


def _behind(
    views: tuple[tuple[float, Half], ...], cells: Sequence[tuple[int, int]]
) -> Behind:
    """The pixels `cells` of each of `views`."""
    return tuple(
        (light, tuple(half.px[y][x] for x, y in cells)) for light, half in views
    )


@lru_cache(maxsize=256)
def _room(look: Look, w: int, h: int, season: str, kind: str | None) -> Room:
    """The things indoors, chosen by contrast with the view behind them
    and with each other; among those, by the seed. A function of the
    look, the size, the season and the weather: no colour changes as
    the time runs."""
    seed, rows = look.seed, 2 * h
    views = _views(look, w, h, season, kind)

    def order(among: Sequence[RGB], salt: int) -> list[int]:
        return sorted(range(len(among)), key=lambda i: noise(seed + salt, i))

    cols, rail = _bars(w, h)
    cells = [(x, y) for x in cols for y in range(rows)]
    cells += [(x, rail) for x in range(w)] if rail is not None else []
    bars = _behind(views, cells)

    def framed(coat: RGB | None) -> tuple[RGB, float]:
        if not cols:
            return FRAMES[0], math.inf
        near = [(coat, 1.0)] if coat is not None else []
        return _pick(FRAMES, bars, near, list(range(len(FRAMES))))

    # The cat first, with a paint of the mullions that suits it.
    perch = _perch(w, h, seed) if look.cosy else None
    coat: RGB | None = None
    frame, _ = framed(None)
    if perch is not None:
        left, way = perch
        xs = {
            *range(left - 1, left + 6),
            *(x + d for x in _tail(left, way) for d in (-1, 0, 1)),
        }
        cells = [(x, y) for x in xs if 0 <= x < w for y in range(rows - 6, rows)]
        behind = _behind(views, cells)

        def both(i: int) -> tuple[tuple[RGB, RGB], float]:
            fit = _against(COATS[i], behind, rim=True)
            paint, apart = framed(COATS[i])
            return (COATS[i], paint), min(fit, apart)

        (coat, frame), _ = _first(both(i) for i in order(COATS, 111))
    near = [(frame, 1.0)] if cols else []
    near += [(coat, 0.6)] if coat is not None else []
    side = curtain(w, 0.0)
    cells = [(x, y) for x in range(min(side + 1, w)) for y in range(rows)]
    cells += [(w - 1 - x, y) for x, y in cells]
    cloth, _ = _pick(FABRICS, _behind(views, cells), near, order(FABRICS, 101))
    return Room(frame, cloth, coat or COATS[0], perch)


def scene(
    w: int,
    h: int,
    when: datetime.datetime,
    t: float = 0.0,
    look: Look | None = None,
    pointer: Pointer | None = None,
    cat: _mouser.Cat | None = None,
) -> Rows:
    """The window of `w` by `h` cells at the local time `when`, `t`
    seconds after the start, the pointer at `pointer`. `cat`: the cat
    that hunts it, stepped from a call to the next; else the cat is a
    function of `t` and of the trail of the pointer."""
    return _painted(w, h, when, t, look, pointer, cat).rows((look or Look()).deep)


def _painted(
    w: int,
    h: int,
    when: datetime.datetime,
    t: float,
    look: Look | None,
    pointer: Pointer | None,
    cat: _mouser.Cat | None,
) -> Half:
    look = look or Look()
    sky = _sky.at(when, look.lat)
    season = sky.season if look.seasons else "summer"
    kind = look.weather
    if kind == "day":
        kind = _sky.weather(when.toordinal(), look.seed, season)
    if kind == "snow":
        season = "winter"
    sky = _sky.at(when, look.lat, COVER.get(kind or "", 0.0))
    half = _outside(w, h, look, sky, season, kind, t, pointer)
    room = _room(look, w, h, season, kind)
    _mullions(half, sky, room.frame)
    if look.cosy:
        ease = pointer.ease(t) if pointer is not None else 0.0
        prey = None
        if look.passing and kind not in ("rain", "fog", "snow"):
            hz = _LAND[look.place](look, w, 2 * h)[0]
            flock = [x for x, _ in _flock(w, sky.light, look.seed, t, hz) if 0 <= x < w]
            prey = sum(flock) / len(flock) if flock else None
        _cat(half, sky, t, pointer, room, cat, look.seed, prey)
        _curtains(half, sky, room.cloth, ease)
    return half


# --- the app ---------------------------------------------------------


def boxed(rows: Rows, w: int) -> list[str]:
    """`rows` in the frame of the window, with SGR escapes."""

    def edge(text: str) -> str:
        return f"\x1b[38;5;{BORDER['style']['fg']}m{text}\x1b[0m"

    c = BORDER["corners"]
    out = [edge(c[0] + BORDER["top"] * w + c[1])]
    out += [edge(BORDER["left"]) + line + edge(BORDER["right"]) for line in ansi(rows)]
    return [*out, edge(c[2] + BORDER["bottom"] * w + c[3])]


def _look(args: argparse.Namespace, start: datetime.datetime) -> Look:
    seed = start.toordinal() if args.seed is None else args.seed
    place = args.place
    if place in (None, "random"):
        drawn = PLACES[int(noise(seed + 1234) * len(PLACES))]
        place = drawn if args.all or place == "random" else "countryside"
    weather = args.weather or ("day" if args.all or args.sky else None)
    return Look(
        seed=seed,
        place=place,
        lat=args.lat,
        land=args.all or args.land,
        seasons=args.all or args.seasons,
        weather=weather,
        passing=args.all or args.passing,
        cosy=args.all or args.cosy,
        deep=args.rgb,
    )


def _era(args: argparse.Namespace) -> Era:
    return Era(datetime.datetime.now(), args.speed, args.at, args.season, args.days)


def show(args: argparse.Namespace) -> int:
    """`--print`: the variants in their frame on stdout, no driver."""
    era = _era(args)
    look = _look(args, era.start)
    for w, h in SIZES if args.print == (0, 0) else (args.print,):
        sys.stdout.write("\n".join(boxed(scene(w, h, era.start, 0.0, look), w)) + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    era = _era(args)
    look = _look(args, era.start)
    start = clock.monotonic()
    pointer = Pointer(lambda: clock.monotonic() - start)
    cat = _mouser.Cat()

    def render(w: int, h: int) -> Rows:
        t = clock.monotonic() - start
        return scene(w, h, era.when(t), t, look, pointer, cat)

    deck = _widgets.Deck(pilot, args)
    border = BORDER if args.border == "window" else args.border
    widget = deck.add("window", list(SIZES), render, border=border, title="")
    deck.on_hover(widget, pointer.move, pointer.leave)
    # A minute of the scene a frame at least, when nothing moves.
    still = min(STILL, max(PERIOD, 60.0 / max(1.0, args.speed)))
    period = PERIOD if look.moving else still
    deck.run(lambda: BRISK if cat.busy else period, seconds)


def _size(text: str) -> Size:
    try:
        w, h = (int(n) for n in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"not WxH: {text}") from None
    if not (1 <= w <= 200 and 1 <= h <= 60):
        raise argparse.ArgumentTypeError(f"not within 200x60: {text}")
    return w, h


def _clock(text: str) -> tuple[int, int]:
    try:
        hour, minute = (int(n) for n in text.split(":"))
        datetime.time(hour, minute)
    except ValueError:
        raise argparse.ArgumentTypeError(f"not HH:MM: {text}") from None
    return hour, minute


#: `-r`: the moments of a day, and how often each is drawn.
MOMENTS = {
    "dawn": 2,
    "morning": 2,
    "noon": 2,
    "afternoon": 2,
    "golden": 3,
    "dusk": 3,
    "evening": 3,
    "night": 1,
}
#: `-r`: how often a weather is drawn; no snow but in winter.
SKIES = {"clear": 34, "clouds": 28, "rain": 14, "fog": 10, "snow": 14}


def _sun(day: datetime.datetime, lat: float) -> tuple[float, float]:
    """The hours the sun rises and sets at, to a few minutes."""
    up = [m / 12 for m in range(288) if _sky.at(_hour(day, m / 12), lat).sun > 0]
    return (up[0], up[-1]) if up else (12.0, 12.0)


def _hour(day: datetime.datetime, hour: float) -> datetime.datetime:
    minutes = round(hour % 24 * 60) % 1440
    return day.replace(hour=minutes // 60, minute=minutes % 60, second=0)


def moment(name: str, rise: float, fall: float, draw: float) -> float:
    """The hour of the moment `name` of `MOMENTS` of a day whose sun
    rises at `rise` and sets at `fall`; `draw`, `0..1`: where in it."""
    noon = (rise + fall) / 2
    a, b = {
        "dawn": (rise - 0.4, rise + 0.3),
        "morning": (rise + 0.8, noon - 1.5),
        "noon": (noon - 1.5, noon + 1.5),
        "afternoon": (noon + 1.5, fall - 1.2),
        "golden": (fall - 1.0, fall - 0.2),
        "dusk": (fall - 0.1, fall + 0.6),
        "evening": (fall + 0.8, max(fall + 1.0, 23.0)),
        "night": (fall + 4.0, rise + 23.0),
    }[name]
    return (a + max(0.0, b - a) * draw) % 24


def drawn(args: argparse.Namespace, key: str, among: Sequence[str]) -> None:
    """`-r`: draw what `args` leave open: the seed, `key` among
    `among`, the season and a day of it, the time, the weather. The
    season shows. A seed given: the same draws."""
    args.days = 0
    if not args.random:
        return
    rng = random.Random(args.seed)
    if args.seed is None:
        args.seed = rng.randrange(1 << 30)
    if getattr(args, key) in (None, "random"):
        setattr(args, key, rng.choice(among))
    if args.season is None:
        args.season = rng.choice(_sky.SEASONS)
        args.days = rng.randrange(-28, 29)  # another moon
    args.seasons = True
    if args.weather is None:
        skies = {
            k: n for k, n in SKIES.items() if k != "snow" or args.season == "winter"
        }
        args.weather = rng.choices(list(skies), list(skies.values()))[0]
    if args.at is None:
        day = Era(datetime.datetime.now(), 1.0, None, args.season, args.days).start
        name = rng.choices(list(MOMENTS), list(MOMENTS.values()))[0]
        hour = moment(name, *_sun(day, args.lat), rng.random())
        args.at = (int(hour), int(hour % 1 * 60))


def times(out: argparse.ArgumentParser) -> None:
    """The options of the time of a scene, and `-r`, `--seed`,
    `--rgb`, `--print`."""
    out.add_argument(
        "-r",
        "--random",
        action="store_true",
        help="at random, what is not given: the land, the season, the time, "
        "the weather; another at each run",
    )
    out.add_argument(
        "--at",
        type=_clock,
        metavar="HH:MM",
        help="the time it starts at [default: now]",
    )
    out.add_argument("--season", choices=_sky.SEASONS, help="the season it starts in")
    out.add_argument(
        "--speed",
        type=float,
        default=1.0,
        metavar="N",
        help="seconds of the scene a second; 1440: a day a minute [default: 1]",
    )
    out.add_argument(
        "--lat", type=float, default=47.0, help="latitude, degrees [default: 47]"
    )
    out.add_argument("--seed", type=int, help="same seed, same land [default: the day]")
    out.add_argument(
        "--rgb", action="store_true", help="colours of 24 bits, not of the 256"
    )
    out.add_argument(
        "--print",
        nargs="?",
        const=(0, 0),
        type=_size,
        metavar="WxH",
        help="print the scene, no driver; every variant, or one size",
    )


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("window", "A house window on a landscape, by day and by night.")
    _widgets.options(out, border="window")
    out.add_argument(
        "-l", "--land", action="store_true", help="ridges, trees, houses lit at night"
    )
    out.add_argument(
        "-y", "--seasons", action="store_true", help="the season of the date"
    )
    out.add_argument(
        "-w",
        "--sky",
        action="store_true",
        help="the weather of the day: clouds, rain, fog, snow",
    )
    out.add_argument(
        "-p",
        "--passing",
        action="store_true",
        help="birds, smoke, a balloon, a meteor; the stars twinkle",
    )
    out.add_argument(
        "-c", "--cosy", action="store_true", help="curtains, a cat on the sill"
    )
    out.add_argument(
        "-a", "--all", action="store_true", help="-l -y -w -p -c, the place of the day"
    )
    out.add_argument(
        "--place",
        choices=(*PLACES, "random"),
        help="what is outside [default: countryside]",
    )
    out.add_argument(
        "--weather", choices=_sky.WEATHERS, help="this weather, not the day's"
    )
    times(out)
    args = _cli.parse(out, argv)
    if args.speed <= 0:
        out.error("--speed: more than 0")
    drawn(args, "place", PLACES)
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
