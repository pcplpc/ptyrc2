# SPDX-License-Identifier: GPL-3.0-or-later
"""Window: a house window on a landscape, by day and by night.

    ptyrc-app window
    ptyrc-app window -a --prefer top-right
    ptyrc-app window -ly --place mountain --at 21:30 --season winter
    ptyrc-app window -a --print 40x12 --at 07:10

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
- the pointer over the window (`_strip.Pointer`): the curtains move
  aside, the cat turns its head, a house near it lights up;
- `--print [WxH]`: no driver, the variants on stdout.
"""

from __future__ import annotations

import argparse
import datetime
import math
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Pilot, clock

from . import _cli, _sky, _widgets
from ._canvas import RGB, Braille, Half, Rows, ansi, mix
from ._strip import (
    GROUND,
    LAMP,
    WHITE,
    Layer,
    Mark,
    Pointer,
    Strip,
    clouds,
    heavens,
    noise,
    paint,
    stars,
)

__all__ = [
    "BORDER",
    "PLACES",
    "SIZES",
    "Era",
    "Look",
    "boxed",
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
FABRICS: tuple[RGB, ...] = ((178, 62, 66), (222, 208, 180), (70, 110, 140))
COATS: tuple[RGB, ...] = ((26, 24, 28), (206, 126, 56), (120, 118, 122))
SLATE: RGB = (150, 156, 176)
DEEP: RGB = (0, 92, 140)  # the sea
RAIN: RGB = (150, 172, 208)
FOG: RGB = (206, 210, 214)

PERIOD = 0.25  # seconds a frame, when something moves
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
    seconds of the app, `speed` seconds of the scene a second."""

    def __init__(
        self,
        start: datetime.datetime,
        speed: float = 1.0,
        at: tuple[int, int] | None = None,
        season: str | None = None,
    ) -> None:
        if at is not None:
            start = start.replace(hour=at[0], minute=at[1], second=0)
        if season is not None:
            month, day = MIDDLE[season]
            start = start.replace(month=month, day=day)
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
    near.colour = (104, 110, 136)
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


def _water(half: Half, sky: _sky.Sky, hz: int, seed: int, t: float) -> None:
    w, rows = half.w, 2 * half.h
    deep = sky.shade(DEEP)
    for y in range(hz, rows):
        v = (y - hz + 0.5) / max(1, rows - hz)
        for x in range(w):
            # A mirror: the horizon far off, the zenith at one's feet.
            murk = (0.3 + 0.15 * v) * (0.35 + 0.65 * sky.light)
            c = mix(sky.colour((x + 0.5) / w, 1 - 0.85 * v), deep, murk)
            if sky.light > 0.3 and noise(seed + 9, x + int(t * 1.3) + y * 3, y) < 0.04:
                c = mix(c, WHITE, 0.3 * sky.light)
            half.px[y][x] = c
    for body, across, shine in (
        (sky.sun, sky.sun_x, (255, 226, 160)),
        (sky.moon, sky.moon_x, (236, 236, 220)),
    ):
        if not -2 < body < 50 or (shine[2] == 220 and sky.moonlight < 0.2):
            continue
        for y in range(hz, rows):
            reach = 0.5 + (y - hz) * 0.22
            for x in range(int(across * w - reach - 1), int(across * w + reach + 2)):
                near = abs(x + 0.5 - across * w) <= reach
                if near and noise(seed + 4, x * 31 + y, int(t * 2.2)) < 0.45:
                    half.set(x, y, shine, 0.7 * (1 - 0.7 * sky.cover))


def _fog(half: Half, sky: _sky.Sky, hz: int) -> None:
    grey = sky.shade(FOG)
    rows = 2 * half.h
    for y in range(rows):
        d = (y - hz) / rows
        thick = 0.82 - (1.5 * d if d > 0 else -0.9 * d)
        for x in range(half.w):
            half.set(x, y, grey, max(0.25, thick))


def _rain(half: Half, sky: _sky.Sky, seed: int, t: float) -> None:
    dots = Braille(half.w, half.h)
    tall = 4 * half.h
    for col in range(2 * half.w):
        if noise(seed + 21, col) > 0.42:
            continue
        fall = 9 + 6 * noise(seed + 22, col)
        y = int(t * fall + noise(seed + 23, col) * 97) % (tall + 5)
        for k in range(2):
            dots.set(col - (y - k) // 5 % 2, y - k)
    for (cx, cy), _ in list(_cells(dots)):
        if (cx, cy) not in half.over:
            under = mix(half.px[2 * cy][cx], half.px[2 * cy + 1][cx], 0.5)
            half.put(cx, cy, dots.glyph(cx, cy), mix(under, sky.shade(RAIN), 0.75))


def _snow(half: Half, sky: _sky.Sky, seed: int, t: float) -> None:
    dots = Braille(half.w, half.h)
    wide, tall = 2 * half.w, 4 * half.h
    for i in range(max(3, half.w * half.h // 7)):
        fall = 1.6 + 1.8 * noise(seed + 31, i)
        y = int(t * fall + noise(seed + 32, i) * tall) % tall
        sway = math.sin(t * 0.7 + i) * 1.6
        dots.set(int(noise(seed + 33, i) * wide + sway + t * 0.4) % wide, y)
    flake = mix(WHITE, sky.shade(WHITE), 0.5)
    for (cx, cy), _ in list(_cells(dots)):
        if (cx, cy) not in half.over:
            half.put(cx, cy, dots.glyph(cx, cy), flake)


def _cells(dots: Braille) -> list[tuple[tuple[int, int], int]]:
    return [
        ((cx, cy), bits)
        for cy, row in enumerate(dots.bits)
        for cx, bits in enumerate(row)
        if bits
    ]


WINGS = ("⠑⠊", "⠒⠒", "⠊⠑", "⠒⠒")


def _birds(half: Half, sky: _sky.Sky, seed: int, t: float, hz: int) -> None:
    slot, cross = 45.0, 32.0
    k = int(t // slot)
    p = (t - k * slot) / cross
    if p > 1 or noise(seed + 41, k) > 0.8 or sky.light < 0.45:
        return
    w = half.w
    count = 1 if w < 12 else 1 + int(noise(seed + 42, k) * 4)
    back = noise(seed + 43, k) < 0.5
    top = max(1, hz // 2 - 2)
    row = int(noise(seed + 44, k) * top)
    for i in range(count):
        x = round(-3 + p * (w + 6 + 3 * count)) - 3 * i
        x = w - 2 - x if back else x
        y = row + (i + 1) // 2 * (1 if i % 2 else -1) + round(math.sin(t * 0.5 + i))
        if not (0 <= y < half.h and 0 <= x < w - 1):
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
    for (cx, cy), _ in _cells(dots):
        if (cx, cy) not in half.over:
            half.put(cx, cy, dots.glyph(cx, cy), (255, 250, 226))


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


def _lights(
    half: Half,
    sky: _sky.Sky,
    marks: Sequence[Mark],
    seed: int,
    pointer: Pointer | None,
) -> None:
    """The windows of the houses: lit at night, fewer late; lit under
    the pointer at any time."""
    late = sky.hour >= 23.25 or sky.hour < 5.5
    for mark in marks:
        if mark.kind != "window":
            continue
        home = noise(seed + 95, mark.n)
        lit = sky.light < 0.4 and home < (0.25 if late else 0.85)
        if pointer is not None and pointer.at is not None:
            px, py = pointer.at
            lit = lit or (abs(px - mark.x) <= 3 and abs(py - mark.y // 2) <= 2)
        if lit:
            half.set(mark.x, mark.y, LAMP)


def _mullions(half: Half, sky: _sky.Sky) -> None:
    w, h = half.w, half.h
    if w < 18:
        return
    ink = mix((70, 60, 60), FRAME, sky.light)
    bars = [w // 2] if w < 34 else [w // 3, w - 1 - w // 3]
    rail = 2 * (h // 3) + 1 if h >= 8 else None
    for x in bars:
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


def _curtains(half: Half, sky: _sky.Sky, seed: int, ease: float) -> None:
    w, h = half.w, half.h
    cloth = FABRICS[int(noise(seed + 101) * len(FABRICS))]
    room = 0.4 + 0.6 * sky.light
    wide = curtain(w, ease) if w >= 12 else 0  # none on a porthole
    for i in range(wide):
        # Folds: a light cell, a dark one; denser when drawn aside.
        fold = (0.0, 0.2, 0.36)[(i * (2 if ease > 0.5 else 1) + (i == wide - 1)) % 3]
        c = mix(mix(cloth, (20, 14, 24), fold), (0, 0, 0), 1 - room)
        for y in range(h):
            for x in (i, w - 1 - i):
                half.put(x, y, " ", bg=c)
                half.px[2 * y][x] = half.px[2 * y + 1][x] = c


CAT = (" # # ", " ### ", " ### ", "#####", "#####")


def _cat(
    half: Half,
    sky: _sky.Sky,
    seed: int,
    t: float,
    pointer: Pointer | None,
    side: int,
) -> None:
    """A cat on the sill, seen from behind; it looks at the pointer."""
    w, rows = half.w, 2 * half.h
    if w < 18 or half.h < 6:
        return
    coat = COATS[int(noise(seed + 111) * len(COATS))]
    coat = mix(coat, (0, 0, 0), 0.5 * (1 - sky.light))
    left = side + 1 + int(noise(seed + 112) * (w - 2 * side - 9))
    top = rows - len(CAT)
    for dy, line in enumerate(CAT):
        for dx, char in enumerate(line):
            if char == "#":
                half.set(left + dx, top + dy, coat)
    flick = int(t) % 23 < 2 and int(t * 3) % 2
    half.set(left + 5, rows - 1, coat)
    half.set(left + 6, rows - (2 if flick else 1), coat)
    if not flick:
        half.set(left + 7, rows - 2, coat)
    if pointer is not None and pointer.at is not None:
        eyes = (200, 230, 90)
        turn = (pointer.at[0] > left + 2) - (pointer.at[0] < left + 2)
        if turn:
            half.set(left + 2 + turn, top + 1, eyes)
        else:
            half.set(left + 1, top + 1, eyes)
            half.set(left + 3, top + 1, eyes)


def sized(w: int, h: int) -> Size:
    """The pixels of a box: `w` by `2h`."""
    return w, 2 * h


def scene(
    w: int,
    h: int,
    when: datetime.datetime,
    t: float = 0.0,
    look: Look | None = None,
    pointer: Pointer | None = None,
) -> Rows:
    """The window of `w` by `h` cells at the local time `when`, `t`
    seconds after the start, the pointer at `pointer`."""
    look = look or Look()
    seed = look.seed
    sky = _sky.at(when, look.lat)
    season = sky.season if look.seasons else "summer"
    kind = look.weather
    if kind == "day":
        kind = _sky.weather(when.toordinal(), seed, season)
    if kind == "snow":
        season = "winter"
    cover = {"clouds": 0.25, "rain": 0.85, "fog": 0.55, "snow": 0.75}.get(
        kind or "", 0.0
    )
    sky = _sky.at(when, look.lat, cover)

    half = Half(w, h)
    rows = 2 * h
    hz, layers = _LAND[look.place](look, w, rows)
    clear = heavens(half, sky, hz)
    if kind is not None:
        amount = {"clear": 0.14, "clouds": 0.6, "fog": 0.3}.get(kind, 0.8)
        clouds(half, sky, seed + 7, hz, amount, t * 0.25)
        if cover < 0.5:  # stars and birds show between the clouds
            clear = [list(row) for row in half.px]
    marks: list[Mark] = []
    for layer in layers:
        if look.place == "sea" and layer.depth == 0.0:
            _water(half, sky, hz, seed, t)
        marks += paint(half, layer, sky, season=season)
    if look.place == "sea" and layers[-1].depth != 0.0:
        _water(half, sky, hz, seed, t)
    if look.land:
        _lights(half, sky, marks, seed, pointer)
    if look.passing:
        _smoke(half, sky, marks, seed, t)
        _balloon(half, sky, seed, t, hz)
        _plane(half, sky, seed, t, hz)
        if look.place == "sea":
            _boat(half, sky, seed, t, hz)
    if kind == "fog":
        _fog(half, sky, hz)
    stars(half, clear, sky, seed + 3, t if look.passing else None, hz)
    if look.passing and kind not in ("rain", "fog", "snow"):
        _birds(half, sky, seed, t, hz)
        _meteor(half, sky, seed, t, hz)
    if kind == "rain":
        _rain(half, sky, seed, t)
    elif kind == "snow":
        _snow(half, sky, seed, t)
    _mullions(half, sky)
    if look.cosy:
        ease = pointer.ease(t) if pointer is not None else 0.0
        _cat(half, sky, seed, t, pointer, curtain(w, ease))
        _curtains(half, sky, seed, ease)
    return half.rows(look.deep)


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
    return Era(datetime.datetime.now(), args.speed, args.at, args.season)


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
    pointer = Pointer(clock.monotonic)
    start = clock.monotonic()

    def render(w: int, h: int) -> Rows:
        t = clock.monotonic() - start
        return scene(w, h, era.when(t), t, look, pointer)

    deck = _widgets.Deck(pilot, args)
    border = BORDER if args.border == "window" else args.border
    widget = deck.add("window", list(SIZES), render, border=border, title="")
    deck.on_hover(widget, pointer.move, pointer.leave)
    # A minute of the scene a frame at least, when nothing moves.
    still = min(STILL, max(PERIOD, 60.0 / max(1.0, args.speed)))
    deck.run(PERIOD if look.moving else still, seconds)


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


def times(out: argparse.ArgumentParser) -> None:
    """The options of the time of a scene, and `--seed`, `--rgb`,
    `--print`."""
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
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
