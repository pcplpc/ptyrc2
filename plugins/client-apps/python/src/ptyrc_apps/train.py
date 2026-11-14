# SPDX-License-Identifier: GPL-3.0-or-later
"""Train: the land going by the window of a train car.

    ptyrc-app train
    ptyrc-app train -a --prefer top
    ptyrc-app train -lp --line alps --at 08:00 --season winter
    ptyrc-app train -a --print 48x11 --at 21:40
    ptyrc-app train -a -r

What it shows:

- one widget, a variant a size, from 10 by 3 to 48 by 11; the border
  is the frame of the glass, rounded, a rail under it;
- layers of `_strip`, each at its depth: the far ones barely move, the
  near ones run; `scene(w, h, when, t)` is pure: `when` the local time
  (the light of `_sky`), `t` the seconds of the trip, of which
  `dist(trip, t)` makes pixels of the nearest plane;
- `FPS` frames a second, the whole content: what a parallax costs
  from a client. The trip goes a frame a draw and the nearest plane
  a pixel a frame; each layer scrolls evenly (`_even`): a pixel every
  so many frames;
- far to near, nothing shows through what is nearer: the sky, the
  layers of `_line`, the poles on the verge and their wires, what
  brushes the glass, what the trip meets;
- bare: the sky, two ranges of hills, the verge. `-l`: the land: far
  ridges, trees, houses lit at night, what brushes the glass; `-p`:
  poles and the wire between them, sagging; `-e`: what a trip meets, one a
  `CYCLE`: a tunnel, a cutting up to its mouth, then a wall of
  bricks and its lamps; a truss over a river; a train the other way;
  a level crossing, its boom down; a station where it stops; `-y`:
  the season of the date; `-w`: the weather of the day, rain along
  the glass; `-a`: all, and the line of the day (`--line`: plain,
  coast, alps, desert, city);
- the pointer on the glass (`_strip.Pointer`): a breath of fog where
  it goes, gone in a few seconds;
- `-r`: what is not given is drawn at random, another at each run
  (`window.drawn`): the line, the season, the time, the weather,
  where along the way; `--seed`: the same draws;
- `--print [WxH]`: no driver, the variants on stdout.
"""

from __future__ import annotations

import argparse
import datetime
import math
import random
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from functools import cache, lru_cache
from typing import Any

from ptyrc import Pilot, clock

from . import _cli, _sky, _widgets
from ._canvas import NEAR, RGB, Braille, Half, Rows, ansi, mix
from ._line import LINES, land
from ._strip import (
    LAMP,
    SEA,
    WHITE,
    Layer,
    Mark,
    Pointer,
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
    wave,
)
from .window import Era, drawn, times

__all__ = [
    "BORDER",
    "CYCLE",
    "EVENTS",
    "LINES",
    "SIZES",
    "Trip",
    "boxed",
    "breath",
    "dist",
    "event",
    "main",
    "scene",
]

Size = tuple[int, int]

#: The inner sizes of the variants.
SIZES: tuple[Size, ...] = ((10, 3), (16, 4), (24, 6), (34, 8), (48, 11))
EVENTS = ("tunnel", "station", "train", "bridge", "crossing")
#: The frame of the glass: rounded above, a rail under.
BORDER: dict[str, Any] = {
    "corners": "╭╮╘╛",
    "top": "─",
    "bottom": "═",
    "left": "│",
    "right": "│",
    "style": {"fg": 250},
}

FPS = 24  # frames a second
PACE = 24.0  # pixels a second, the nearest plane: one a frame
CYCLE = 70.0  # seconds from an event to the next
WHEN = 24.0  # seconds into a cycle, its event
BRAKE, STAND = 9.0, 10.0  # seconds: to stop and to leave; standing
SPAN = {"tunnel": 8.0, "bridge": 6.0, "crossing": 1.5}  # seconds along each
CAR, CARS = 48, 9  # pixels a car of a train the other way; its cars
RUSH = 6  # pixels a frame it goes by
LAMPS = 48  # pixels of wall from a lamp of a tunnel to the next
PORTAL = 3  # pixels of stone at a mouth
POLES = 30  # pixels from a pole to the next
KEEP = 6.0  # seconds a breath stays on the glass

DARK: RGB = (22, 20, 24)
BRICK: RGB = (62, 50, 48)
BAKED: RGB = (88, 58, 50)
MORTAR: RGB = (40, 36, 38)
ROCK: RGB = (118, 110, 108)
GLOW: RGB = (255, 184, 24)  # of a lamp on a wall: amber in the 256 too
IRON: RGB = (160, 40, 36)
STONE: RGB = (150, 150, 158)
LIVERY: tuple[RGB, ...] = ((204, 44, 44), (40, 96, 170), (236, 190, 60), (60, 140, 90))


@dataclass(frozen=True)
class Trip:
    """What a trip shows, the time apart."""

    seed: int = 0
    line: str = "plain"
    lat: float = 47.0
    land: bool = False
    poles: bool = False
    events: bool = False
    seasons: bool = False
    weather: str | None = None  # None: clear, no cloud; `day`: the day's
    deep: bool = False


# --- the way ---------------------------------------------------------


def event(trip: Trip, k: int) -> str | None:
    """The event of the cycle `k`, one of `EVENTS`, or none."""
    if not trip.events or noise(trip.seed + 500, k) > 0.86:
        return None
    return EVENTS[int(noise(trip.seed + 501, k) * len(EVENTS))]


def _within(u: float) -> float:
    """Seconds at full pace worth the `u` first seconds of a cycle
    with a station in it."""
    a, b, c = WHEN, WHEN + BRAKE, WHEN + BRAKE + STAND
    if u <= a:
        return u
    if u <= b:
        return a + (u - a) - (u - a) ** 2 / (2 * BRAKE)
    if u <= c:
        return a + BRAKE / 2
    if u <= c + BRAKE:
        return a + BRAKE / 2 + (u - c) ** 2 / (2 * BRAKE)
    return u - BRAKE - STAND


@cache
def _before(trip: Trip, k: int) -> float:
    """Seconds at full pace worth the cycles before `k`."""
    if k <= 0:
        return 0.0
    full = _within(CYCLE) if event(trip, k - 1) == "station" else CYCLE
    return _before(trip, k - 1) + full


def dist(trip: Trip, t: float) -> float:
    """Pixels of the nearest plane gone by after `t` seconds: `PACE`
    a second, less what a station takes."""
    t = max(0.0, t)
    k = int(t // CYCLE)
    for i in range(0, k, 256):  # the memo, without a deep recursion
        _before(trip, i)
    u = t - k * CYCLE
    return PACE * (
        _before(trip, k) + (_within(u) if event(trip, k) == "station" else u)
    )


# --- what goes by ----------------------------------------------------


Strung = list[tuple[int, int, int, RGB, RGB]]


def _hung(rows: int) -> tuple[int, int, float]:
    """The pixel row of the cross arms; the dot row of the wire at a
    pole, how far it sags, in dots."""
    top = max(1, round(rows * 0.1))
    return top, 2 * top, max(2.0, rows * 0.24)


def _cells(px: list[list[RGB]], h: int) -> None:
    """The cells a wire may cross, as one colour where their two
    pixels are near one: a glyph shows its cell as one."""
    _, high, sag = _hung(2 * h)
    for cy in range(high // 4, min(h, int((high + sag) // 4) + 1)):
        up, low = px[2 * cy], px[2 * cy + 1]
        for cx, (a, b) in enumerate(zip(up, low, strict=True)):
            if sum(abs(i - j) for i, j in zip(a, b, strict=True)) <= NEAR:
                up[cx] = low[cx] = mix(a, b, 0.5)


def _wires(
    half: Half,
    sky: _sky.Sky,
    shift: float,
    verge: Layer,
    bare: Sequence[tuple[float, float]] = (),
) -> Strung:
    """Poles on the verge, at its pace, and a wire that sags from
    one to the next. No pole within the spans of `bare`, in pixels of
    the way, and no wire to a pole that is not.

    The poles are painted. The wire is returned, for `_string` to
    draw once what is nearer is painted: the cells it crosses, each
    with its braille bits and its two pixels as they are now."""
    w, rows = half.w, 2 * half.h
    shift = math.floor(shift + 1e-6)
    top, high, sag = _hung(rows)

    def stands(i: int) -> bool:
        return not any(a - 2 <= i * POLES < b + 2 for a, b in bare)

    dots = Braille(w, half.h)
    for dx in range(2 * w):
        i, u = divmod((dx / 2 + shift) / POLES, 1)
        if not (stands(int(i)) and stands(int(i) + 1)):
            continue
        dots.set(dx, round(high + sag * (1 - (2 * u - 1) ** 2)))
    out: Strung = [
        (cx, cy, dots.bits[cy][cx], half.px[2 * cy][cx], half.px[2 * cy + 1][cx])
        for cx, cy in dots.cells()
    ]
    wood = sky.shade((92, 66, 44))
    for i in range(shift // POLES, (shift + w) // POLES + 1):
        if not stands(i):
            continue
        x = i * POLES - shift
        foot = min(rows, round(verge.top(i * POLES)) + 2)
        for y in range(top - 1, foot):
            half.set(x, y, wood)
        half.set(x - 1, top, wood)
        half.set(x + 1, top, wood)
    return out


def _string(half: Half, sky: _sky.Sky, wires: Strung) -> None:
    """Draw the wires where the cell is as it was: not behind a thing.

    A glyph has one background: both pixels of the cell take their
    mean, so the wire is whole and what is drawn is what is shown, in
    any palette."""
    ink = mix(sky.shade((60, 50, 44)), (0, 0, 0), 0.3)
    for cx, cy, bits, up, low in wires:
        if (half.px[2 * cy][cx], half.px[2 * cy + 1][cx]) != (up, low):
            continue
        under = mix(up, low, 0.5)
        half.px[2 * cy][cx] = half.px[2 * cy + 1][cx] = under
        half.over.setdefault((cx, cy), (chr(0x2800 + bits), ink, under))


def _whole(x: float) -> int:
    return math.floor(x + 1e-6)


def _ease(x: float) -> float:
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def approach(rows: int) -> int:
    """Pixels of cutting before the mouth of a tunnel, and after."""
    return 2 * rows + 6


def _cutting(
    half: Half, sky: _sky.Sky, x: int, near: int, at: int, seed: int, ground: RGB
) -> None:
    """The column `x` of the hillside a tunnel goes into, `near`
    pixels from its mouth; `at`: its pixel along the way. Of the
    colour `ground`, it turns to rock as it rises."""
    rows = 2 * half.h
    f = _ease(1 - near / approach(rows))
    bump = (wave(seed + 710, at / 5) - 0.5) * 3 * (1 - f)
    top = round(rows - 1 - (rows + 2) * f + bump)
    body = mix(mix(ground, sky.shade(ROCK), 0.75 * f), (0, 0, 0), 0.1)
    for y in range(max(0, top), rows):
        c = body
        if y == top:
            c = mix(body, WHITE, 0.14)
        elif noise(seed + 711, at, y) < 0.1:
            c = mix(body, (0, 0, 0), 0.16)
        half.px[y][x] = c


def _tunnel(
    half: Half,
    sky: _sky.Sky,
    x0: float,
    x1: float,
    shift: float,
    seed: int,
    ground: RGB,
) -> tuple[int, int]:
    """A tunnel, its mouths at the pixels `x0` and `x1`: a cutting
    rises to a portal of stone; inside, a wall of bricks goes by at
    twice the pace, lamps on it, a cable, a dashed line. The columns
    under the vault."""
    w, rows = half.w, 2 * half.h
    a, b, s = _whole(x0), _whole(x1), _whole(shift)
    reach = approach(rows)
    stone, joint = sky.shade(STONE), sky.shade(mix(STONE, (40, 40, 48), 0.45))
    for x in range(max(0, a - reach), min(w, b + reach)):
        if a - PORTAL <= x < b + PORTAL:
            continue
        _cutting(
            half,
            sky,
            x,
            a - PORTAL - x if x < a else x - b - PORTAL,
            x + s,
            seed,
            ground,
        )
    for x in (*range(a - PORTAL, a), *range(b, b + PORTAL)):
        if not 0 <= x < w:
            continue
        inner = x in (a - 1, b)
        for y in range(rows):
            dark = inner or y % 3 == 2 or (x + s + y // 3) % 2 == 0 and y % 3 == 0
            half.px[y][x] = joint if dark else stone
    lamp_y = max(1, round(rows * 0.22))
    cable, line = round(rows * 0.44), round(rows * 0.78)
    ledge = max(line + 1, round(rows * 0.88))
    glow = max(3.0, rows * 0.38)
    day = sky.shade((176, 178, 184))
    lo, hi = max(0, a), min(w, b)
    bricks: dict[tuple[int, int], RGB] = {}
    for x in range(lo, hi):
        at = x + 2 * s  # along the wall
        aside = (at + LAMPS // 2) % LAMPS - LAMPS // 2  # from the nearest lamp
        mouth = max(0.0, 1 - min(x - a, b - 1 - x) / 10) * sky.light
        for y in range(rows):
            course = y // 3
            run = at + 4 * (course % 2)
            if y >= ledge:
                c = (40, 38, 40) if y == ledge else DARK
            elif y == cable:
                c = (108, 108, 112) if at % 16 == 0 else (18, 18, 22)
            elif y == line and at // 6 % 2 == 0:
                c = (176, 170, 140)
            elif y == 0:
                c = (26, 24, 26)
            elif y % 3 == 2 or run % 8 == 0:
                c = MORTAR
            else:
                brick = (run // 8, course)
                if brick not in bricks:
                    bricks[brick] = mix(BRICK, BAKED, noise(seed + 700, *brick))
                c = bricks[brick]
            if -glow < aside < glow and y < ledge:
                near = math.hypot(aside, y - lamp_y)
                if near < glow:
                    c = mix(c, GLOW, 0.74 * (1 - near / glow) ** 1.5)
            if mouth:  # the day comes in, grey
                grey = sum(c) // 3
                c = mix(mix(c, (grey, grey, grey), mouth), day, 0.4 * mouth)
            half.px[y][x] = c
        if aside in (0, 1):
            half.set(x, lamp_y, (255, 238, 186))
            half.set(x, lamp_y - 1, (70, 66, 62))
    return lo, hi


def _bridge(
    half: Half,
    sky: _sky.Sky,
    x0: float,
    x1: float,
    shift: float,
    hz: int,
    seed: int,
    t: float,
) -> None:
    """A truss over a river, from the pixel `x0` to `x1`: the near
    ground gives way to water between two banks, the far land beyond;
    the iron in front."""
    w, rows = half.w, 2 * half.h
    a, b, s = _whole(x0), _whole(x1), _whole(shift)
    iron, dark = sky.shade(IRON), sky.shade(mix(IRON, (26, 18, 22), 0.55))
    top, low = max(1, round(rows * 0.1)), max(3, round(rows * 0.87))
    bay = max(6, low - top)
    line = min(low - 1, max(hz + 1, round(rows * 0.62)))  # of the water
    bank = max(4, rows // 2)
    deep, reeds = sky.shade(SEA), sky.shade((40, 110, 50))
    earth, pier = sky.shade((132, 112, 84)), sky.shade(STONE)
    beat = int(t * 2)
    for x in range(max(0, a), min(w, b)):
        along, back = x - a, b - 1 - x
        edge = line + max(0, round((rows - line) * (1 - min(along, back) / bank)))
        for y in range(line, rows):
            if y < edge:
                continue
            if y == edge:
                c = reeds if edge == line else earth
            else:
                v = (y - line) / max(1, rows - line)
                murk = (0.3 + 0.15 * v) * (0.35 + 0.65 * sky.light)
                c = mix(sky.colour((x + 0.5) / w, 1 - 0.85 * v), deep, murk)
                ripple = noise(seed + 9, (x + s) // 3 + beat * 7, y) < 0.09
                if ripple and sky.light > 0.3:  # dim, the 256 turn it purple
                    c = mix(c, WHITE, 0.1 + 0.25 * sky.light)
            half.px[y][x] = c
        post = along % bay == 0 or along < 2 or back < 2
        slope = along % (2 * bay)
        slope = slope if slope < bay else 2 * bay - slope
        brace = top + round(slope * (low - top) / bay)
        for y in range(top - 1, rows):
            if y <= top or y == low:
                c = iron
            elif y > low:
                c = pier if min(along, back) < 3 else dark
                if c is dark and along % 4 == 0:
                    c = mix(dark, iron, 0.4)
            elif post:
                c = iron
            elif y == brace:
                c = dark
            else:
                continue
            half.px[y][x] = c


def _other(half: Half, sky: _sky.Sky, head: int, n: int) -> None:
    """A train the other way, its head at the pixel `head`: `CARS`
    cars of `CAR` pixels, a nose, a lamp at each end."""
    w, rows = half.w, 2 * half.h
    coat = LIVERY[n % len(LIVERY)]
    body, roof = sky.shade(coat), sky.shade(mix(coat, (40, 40, 48), 0.6))
    line, door = sky.shade(mix(coat, WHITE, 0.7)), sky.shade(mix(coat, (0, 0, 0), 0.25))
    under = sky.shade((44, 42, 48))
    pane = mix(LAMP, sky.shade((150, 196, 226)), sky.light)
    top = round(rows * 0.2)
    lintel, sill = round(rows * 0.34), round(rows * 0.6)
    skirt = max(sill + 2, round(rows * 0.88))
    long = CAR * CARS
    for x in range(max(0, head), min(w, head + long)):
        along = x - head
        i = along % CAR
        nose = max(0, 5 - along) + max(0, along - long + 6)  # the ends slope
        for y in range(top + nose, rows):
            if i >= CAR - 2 and along < long - 2:  # between two cars
                if y < top + 2:
                    continue
                c = under
            elif y >= skirt:
                c = under
            elif y == top + nose:
                c = roof
            elif y == sill + 1:
                c = line
            elif lintel <= y < sill and 9 <= i < CAR - 11 and (i - 9) % 14 < 12:
                c = pane
            elif lintel <= y < sill and (3 <= i < 6 or CAR - 8 <= i < CAR - 5):
                c = pane if y < lintel + 2 else door
            elif y > lintel and (2 <= i < 7 or CAR - 9 <= i < CAR - 4):
                c = door
            else:
                c = body
            half.px[y][x] = c
    for at, lamp in ((head, (255, 246, 206)), (head + long - 1, (236, 44, 40))):
        half.set(at, sill + 2, lamp)
        half.set(at, sill + 3, lamp)


def _crossing(
    half: Half, sky: _sky.Sky, x0: float, x1: float, n: int, t: float
) -> None:
    """A road over the track, from the pixel `x0` to `x1`: a boom
    down, two red lamps that blink in turn, a car that waits."""
    rows = 2 * half.h
    if rows < 12:
        return
    a, b = _whole(x0), _whole(x1)
    mid = (a + b) // 2
    reach = (b - a) // 2 - 4  # half the road, by the glass
    far, boom = round(rows * 0.66), round(rows * 0.8)
    tar, paint = sky.shade((84, 84, 92)), sky.shade((226, 226, 214))
    for y in range(far, rows):
        wide = 3 + round((reach - 3) * (y - far) / max(1, rows - 1 - far))
        for x in range(mid - wide, mid + wide + 1):
            half.set(x, y, paint if x == mid and y % 2 else tar)
    night = sky.light < 0.4
    coat = sky.shade(LIVERY[(n + 1) % 3])  # not the green of a field
    half.rect(mid - 3, far - 1, 5, 2, coat)  # the car, its nose to us
    half.rect(mid - 2, far - 2, 3, 1, sky.shade((50, 70, 96)))
    for x in (mid - 3, mid + 1):
        half.set(x, far, LAMP if night else paint)
    post, red = sky.shade((120, 120, 128)), (226, 44, 40)
    for y in range(boom - 2, boom + 2):
        half.set(mid - reach, y, post)
    for x in range(mid - reach + 1, mid + reach):
        half.set(x, boom - 1, red if (x - mid) // 2 % 2 else (244, 244, 240))
    mast = mid + reach + 1
    for y in range(far - 4, boom + 2):
        half.set(mast, y, post)
    beat = int(t * 3) % 2
    half.set(mast - 1, far - 4, red if beat else (70, 24, 24))
    half.set(mast + 1, far - 4, (70, 24, 24) if beat else red)
    half.set(mast, far - 5, paint)


def _station(half: Half, sky: _sky.Sky, at: float, trip: Trip, k: int) -> None:
    """The platform of a stop; `at`: the pixel of its start. A canopy
    on posts, a lamp a bay, benches, people, the name on its board;
    at night the lamps light the stone under them."""
    w, rows = half.w, 2 * half.h
    long = w + 150
    floor, roof = round(rows * 0.76), max(1, round(rows * 0.26))
    dark = sky.light < 0.4
    start = math.ceil(at)

    def lit(c: RGB) -> RGB:  # its lamps light it at night
        return (
            mix(sky.shade(c), mix(c, (120, 96, 40), 0.35), 0.6)
            if dark
            else sky.shade(c)
        )

    stone, edge = lit(STONE), lit((232, 208, 96))
    beam, tiles = lit((128, 62, 52)), lit((160, 84, 66))
    post, wood = lit((96, 96, 104)), lit((66, 110, 84))
    for x in range(max(0, start), min(w, start + long)):
        i = x - start
        under = 24 <= i < long - 24  # the canopy
        bay = (i - 24) % 22
        pool = dark and under and abs(bay - 11) <= 4  # of a lamp
        for y in range(floor, rows):
            c = edge if y == floor else stone
            half.px[y][x] = mix(c, (214, 170, 100), 0.5) if pool else c
        if not under:
            continue
        half.set(x, roof, beam)
        half.set(x, roof - 1, tiles)
        if bay == 0:
            for y in range(roof + 1, floor):
                half.set(x, y, post)
        elif bay == 11:
            half.set(x, roof + 1, LAMP if dark else post)
        elif bay in (10, 12) and dark:
            half.set(x, roof + 1, LAMP, 0.35)
        if 15 <= bay < 19 and rows >= 16:  # a bench
            half.set(x, floor - 1, wood)
            half.set(x, floor - 2, wood, 0.7)
            continue
        who = noise(trip.seed + 600 + k, (i - 24) // 5)
        if (i - 24) % 5 == 2 and who < 0.3 and bay not in (0, 1, 21):
            coat = LIVERY[int(who * 40) % len(LIVERY)]
            half.set(x, floor - 1, lit(mix(coat, (30, 30, 40), 0.4)))
            half.set(x, floor - 2, lit(coat))
            half.set(x, floor - 3, lit((226, 190, 160)))
    sign = start + long // 2 - 4  # the name, on its board
    half.rect(sign, roof + 2, 8, 3, lit((40, 60, 130)))
    for dx in (1, 2, 3, 5, 6):
        half.set(sign + dx, roof + 3, lit((244, 244, 236)))


def breath(half: Half, pointer: Pointer | None, t: float) -> None:
    """Fog on the glass where the pointer has been, fading."""
    if pointer is None:
        return
    for x, y, at in pointer.trail:
        left = 1 - (t - at) / pointer.keep
        if left <= 0:
            continue
        cx, cy = x + 0.5, 2 * y + 1.0
        r = 2.6
        for py in range(int(cy - r - 1), int(cy + r + 2)):
            for px in range(int(cx - r - 1), int(cx + r + 2)):
                d = math.hypot(px + 0.5 - cx, py + 0.5 - cy) / r
                if d <= 1:
                    half.set(px, py, (226, 232, 238), 0.7 * left * (1 - d * d * 0.6))


@lru_cache(maxsize=32)
def _sky_of(w: int, h: int, hz: int, sky: _sky.Sky) -> tuple[tuple[RGB, ...], ...]:
    half = Half(w, h)
    heavens(half, sky, hz)
    return tuple(tuple(row) for row in half.px)


def _even(rate: float) -> float:
    """The nearest rate that scrolls evenly: a pixel every so many
    frames, or so many pixels a frame."""
    return float(round(rate)) if rate >= 0.75 else 1 / round(1 / max(rate, 0.004))


def scene(
    w: int,
    h: int,
    when: datetime.datetime,
    t: float = 0.0,
    trip: Trip | None = None,
    pointer: Pointer | None = None,
) -> Rows:
    """The glass of `w` by `h` cells at the local time `when`, `t`
    seconds into the trip, the pointer at `pointer`.

    Far to near: the sky, the layers beyond the track, the poles and
    their wires, what brushes the glass, what the trip meets; then
    what is on the glass."""
    trip = trip or Trip()
    seed = trip.seed
    when = when.replace(second=0, microsecond=0)  # the sky, a minute at a time
    sky = _sky.at(when, trip.lat)
    season = sky.season if trip.seasons else "summer"
    kind = trip.weather
    if kind == "day":
        kind = _sky.weather(when.toordinal(), seed, season)
    if kind == "snow":
        season = "winter"
    cover = {"clouds": 0.25, "rain": 0.85, "fog": 0.55, "snow": 0.75}.get(
        kind or "", 0.0
    )
    sky = _sky.at(when, trip.lat, cover)

    half = Half(w, h)
    rows = 2 * h
    hz, back, front, sea = land(trip.line, seed, rows, trip.land)
    for layer in (*back, *front):
        layer.speed = _even(layer.rate())
    verge = back[-1]
    shift = dist(trip, t)
    clear = [list(row) for row in _sky_of(w, h, hz, sky)]
    half.px = [list(row) for row in clear]
    if kind is not None:
        amount = {"clear": 0.14, "clouds": 0.6}.get(kind, 0.3)
        clouds(half, sky, seed + 7, hz, amount, -shift * 0.012, 2)
        if cover < 0.5:
            clear = [list(row) for row in half.px]

    # What the trip meets, in pixels of the way: `(what, cycle, x0, x1)`.
    k = int(t // CYCLE)
    met = []
    for n in (k - 1, k):  # an event may end in the next cycle
        what = event(trip, n) if n >= 0 else None
        start = n * CYCLE + WHEN
        if what == "station":
            x0 = dist(trip, start + BRAKE) - 75
            met.append((what, n, x0, x0 + w + 150))
        elif what == "train":  # as long as it takes to go by
            end = start + (w + CAR * CARS) / (RUSH * FPS)
            met.append((what, n, dist(trip, start) + w, dist(trip, end)))
        elif what == "tunnel":  # and the cuttings it goes into, and out of
            x0 = dist(trip, start) + w
            met.append((what, n, x0, x0 + PACE * SPAN[what] + 2 * approach(rows)))
        elif what is not None:
            x0 = dist(trip, start) + w
            met.append((what, n, x0, x0 + PACE * SPAN[what]))
    # No pole at a station or on a bridge; nothing by the glass where
    # something is met.
    poles = [(x0, x1) for what, _, x0, x1 in met if what not in ("train", "tunnel")]
    for layer in front:
        reach, rate = layer.size + 3, layer.rate()
        layer.bare = [
            (rate * (x0 - w) - reach, rate * x1 + w + reach) for _, _, x0, x1 in met
        ]

    pad = approach(rows)
    if any(
        what == "tunnel" and x0 + pad <= shift and x1 - pad >= shift + w
        for what, _, x0, x1 in met
    ):
        back, front = [], []  # under a vault: no land to paint
    marks: list[Mark] = []
    for i, layer in enumerate(back):
        if sea and i == 1:
            water(half, sky, hz, seed, t)
        marks += paint(half, layer, sky, shift, season)
    if trip.land:
        lights(half, sky, marks, seed, None)
    if kind == "fog":
        fog(half, sky, hz)
    wires: Strung = []
    if trip.poles and rows >= 8 and back:
        _cells(half.px, h)
        _cells(clear, h)
        wires = _wires(half, sky, shift, back[-1], poles)
    for layer in front:
        paint(half, layer, sky, shift, season)

    dry = None  # the columns under a vault
    for what, n, x0, x1 in met:
        a, b = x0 - shift, x1 - shift
        if what == "train":  # it goes by at its own pace, `RUSH` a frame
            head = w - RUSH * _whole((t - n * CYCLE - WHEN) * FPS)
            if -CAR * CARS < head < w:
                _other(half, sky, head, n)
        elif a >= w or b <= 0:
            continue
        elif what == "station":
            _station(half, sky, a, trip, n)
        elif what == "bridge":
            _bridge(half, sky, a, b, shift, hz, seed, t)
        elif what == "crossing":
            _crossing(half, sky, a, b, n, t)
        elif what == "tunnel":
            grass = sky.shade(verge.ground(season))
            dry = _tunnel(half, sky, a + pad, b - pad, shift, seed, grass)

    stars(half, clear, sky, seed + 3, None, hz)  # not hidden by a wire
    _string(half, sky, wires)
    glass, was = set(half.over), [list(row) for row in half.px]
    if kind == "rain":
        rain(half, sky, seed, t, 1.5, 0.35)
    elif kind == "snow":
        snow(half, sky, seed, t, 0.45)
    if dry is not None:  # no rain in a tunnel
        for cell in set(half.over) - glass:
            if dry[0] <= cell[0] < dry[1]:
                del half.over[cell]
        for y, row in enumerate(was):
            half.px[y][dry[0] : dry[1]] = row[dry[0] : dry[1]]
    breath(half, pointer, t)
    return half.rows(trip.deep)


# --- the app ---------------------------------------------------------


def boxed(rows: Rows, w: int) -> list[str]:
    """`rows` in the frame of the glass, with SGR escapes."""

    def edge(text: str) -> str:
        return f"\x1b[38;5;{BORDER['style']['fg']}m{text}\x1b[0m"

    c = BORDER["corners"]
    out = [edge(c[0] + BORDER["top"] * w + c[1])]
    out += [edge(BORDER["left"]) + line + edge(BORDER["right"]) for line in ansi(rows)]
    return [*out, edge(c[2] + BORDER["bottom"] * w + c[3])]


def _trip(args: argparse.Namespace, start: datetime.datetime) -> Trip:
    seed = start.toordinal() if args.seed is None else args.seed
    line = args.line
    if line in (None, "random"):
        drawn = LINES[int(noise(seed + 4321) * len(LINES))]
        line = drawn if args.all or line == "random" else "plain"
    return Trip(
        seed=seed,
        line=line,
        lat=args.lat,
        land=args.all or args.land,
        poles=args.all or args.poles,
        events=args.all or args.events,
        seasons=args.all or args.seasons,
        weather=args.weather or ("day" if args.all or args.sky else None),
        deep=args.rgb,
    )


def _era(args: argparse.Namespace) -> Era:
    return Era(datetime.datetime.now(), args.speed, args.at, args.season, args.days)


def show(args: argparse.Namespace) -> int:
    """`--print`: the variants in their frame on stdout, no driver;
    `--after`: that far into the trip."""
    era = _era(args)
    trip = _trip(args, era.start)
    t = args.after
    for w, h in SIZES if args.print == (0, 0) else (args.print,):
        sys.stdout.write("\n".join(boxed(scene(w, h, era.when(t), t, trip), w)) + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    era = _era(args)
    trip = _trip(args, era.start)
    start = clock.monotonic() - args.after
    pointer = Pointer(lambda: clock.monotonic() - start, KEEP)
    frame = math.floor(args.after * FPS)

    def tick() -> None:
        # The trip goes a frame a draw, so that the land scrolls
        # evenly; more than a frame off the clock, it catches up.
        nonlocal frame
        due = (clock.monotonic() - start) * FPS
        frame = frame + 1 if abs(due - frame - 1) < 1.5 else math.floor(due)

    def render(w: int, h: int) -> Rows:
        t = frame / FPS
        return scene(w, h, era.when(t), t, trip, pointer)

    deck = _widgets.Deck(pilot, args)
    border = BORDER if args.border == "car" else args.border
    widget = deck.add("train", list(SIZES), render, border=border, title="")
    # The next frame shows the breath: no draw of its own.
    deck.on_hover(widget, lambda x, y: pointer.move(x, y) or False, pointer.leave)
    deck.run(1 / FPS, seconds, tick, align=True)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("train", "The land going by the window of a train car.")
    _widgets.options(out, border="car")
    out.add_argument(
        "-l",
        "--land",
        action="store_true",
        help="far ridges, trees, houses, what brushes the glass",
    )
    out.add_argument(
        "-p",
        "--poles",
        action="store_true",
        help="poles and the sagging wire between them",
    )
    out.add_argument(
        "-e",
        "--events",
        action="store_true",
        help="a tunnel, a bridge, a train the other way, a crossing, a station",
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
        "-a", "--all", action="store_true", help="-l -p -e -y -w, the line of the day"
    )
    out.add_argument(
        "--line", choices=(*LINES, "random"), help="where it runs [default: plain]"
    )
    out.add_argument(
        "--weather", choices=_sky.WEATHERS, help="this weather, not the day's"
    )
    out.add_argument(
        "--after",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="start that far into the trip [default: 0]",
    )
    times(out)
    args = _cli.parse(out, argv)
    if args.speed <= 0:
        out.error("--speed: more than 0")
    drawn(args, "line", LINES)
    if args.random and not args.after:  # somewhere along the way, an event soon
        rng = random.Random(args.seed)
        args.after = rng.randrange(40) * CYCLE + rng.uniform(0.0, WHEN - 6.0)
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
