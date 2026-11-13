# SPDX-License-Identifier: GPL-3.0-or-later
"""Train: the land going by the window of a train car.

    ptyrc-app train
    ptyrc-app train -a --prefer top
    ptyrc-app train -lp --line alps --at 08:00 --season winter
    ptyrc-app train -a --print 48x11 --at 21:40

What it shows:

- one widget, a variant a size, from 10 by 3 to 48 by 11; the border
  is the frame of the glass, rounded, a rail under it;
- layers of `_strip`, each at its depth: the far ones barely move, the
  near ones run; `scene(w, h, when, t)` is pure: `when` the local time
  (the light of `_sky`), `t` the seconds of the trip, of which
  `dist(trip, t)` makes pixels of the nearest plane;
- a frame every `PERIOD`, the whole content: what a parallax costs
  from a client;
- bare: the sky, two ranges of hills, the verge. `-l`: the land: far
  ridges, trees, houses lit at night, what brushes the glass; `-p`:
  poles and their wires, sagging; `-e`: what a trip is made of: a
  tunnel and the car in the glass, a bridge, a train the other way, a
  station where it stops; `-y`: the season of the date; `-w`: the
  weather of the day, rain along the glass; `-a`: all, and the line
  of the day (`--line`: plain, coast, alps, desert, city);
- the pointer on the glass (`_strip.Pointer`): a breath of fog where
  it goes, gone in a few seconds;
- `--print [WxH]`: no driver, the variants on stdout.
"""

from __future__ import annotations

import argparse
import datetime
import math
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from functools import cache, lru_cache
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
from .window import Era, times

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
LINES = ("plain", "coast", "alps", "desert", "city")
EVENTS = ("tunnel", "station", "train", "bridge")
#: The frame of the glass: rounded above, a rail under.
BORDER: dict[str, Any] = {
    "corners": "╭╮╘╛",
    "top": "─",
    "bottom": "═",
    "left": "│",
    "right": "│",
    "style": {"fg": 250},
}

PERIOD = 0.1  # seconds a frame
PACE = 26.0  # pixels a second, the nearest plane
CYCLE = 70.0  # seconds from an event to the next
WHEN = 24.0  # seconds into a cycle, its event
BRAKE, STAND = 9.0, 10.0  # seconds: to stop and to leave; standing
SPAN = {"tunnel": 8.0, "train": 3.2, "bridge": 5.0}  # seconds an event lasts
POLES = 30  # pixels from a pole to the next
KEEP = 6.0  # seconds a breath stays on the glass

DARK: RGB = (10, 10, 14)
IRON: RGB = (120, 56, 44)
STONE: RGB = (150, 150, 158)
LIVERY: tuple[RGB, ...] = ((196, 52, 48), (40, 96, 170), (236, 190, 60), (60, 140, 90))


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


# --- the land of each line -------------------------------------------


def _verge(trip: Trip, rows: int, colour: Any = None) -> Layer:
    verge = Layer(Strip(trip.seed + 9, "plain", 24), 0.0, rows * 0.99, rows * 0.22)
    if colour is not None:
        verge.colour = colour
    return verge


def _brush(trip: Trip, rows: int, things: Sequence[str]) -> Layer:
    """What stands by the track: large, quick, seldom."""
    near = Layer(Strip(trip.seed + 8, "flat"), 0.0, rows + 1.0, 0.0, speed=1.7)
    near.things, near.size = things, rows * 0.95
    near.gap, near.density = round(rows * 1.4), 0.3
    return near


def _plain(trip: Trip, rows: int) -> tuple[int, list[Layer]]:
    s, hz = trip.seed, round(rows * 0.58)
    far = Layer(Strip(s + 1, "hills", 26), 0.55, hz + rows * 0.08, rows * 0.32)
    fields = Layer(Strip(s + 2, "hills", 44), 0.25, rows * 0.84, rows * 0.2)
    out = [far, fields, _verge(trip, rows)]
    if trip.land and rows >= 8:
        ridge = Layer(Strip(s, "mountains", 30), 0.9, hz + 1, rows * 0.3)
        far.things, far.gap, far.density, far.size = (
            ("pine", "tree", "bush"),
            4,
            0.5,
            2.0,
        )
        fields.things = (
            "tree",
            "house",
            "tree",
            "pine",
            "bush",
            "hay",
            "tree",
            "sheep",
        )
        fields.size = float(min(6, max(3, round(rows * 0.24))))
        fields.gap, fields.density = round(fields.size) + 2, 0.7
        out = [ridge, *out, _brush(trip, rows, ("tree", "tree", "pine"))]
    return hz, out


def _coast(trip: Trip, rows: int) -> tuple[int, list[Layer]]:
    s, hz = trip.seed, round(rows * 0.52)
    amp = rows * 0.3
    isles = Layer(Strip(s + 1, "hills", 30), 0.86, hz + amp * 0.5, amp, (60, 130, 70))
    sand = {**GROUND, "spring": (214, 196, 140), "summer": (214, 196, 140)}
    sand["autumn"] = (200, 182, 134)
    shore = Layer(Strip(s + 2, "dunes", 40), 0.3, rows * 0.98, rows * 0.3, sand)
    out = [isles, shore, _verge(trip, rows)]
    if trip.land and rows >= 8:
        shore.things = ("palm", "rock", "house", "bush", "palm")
        shore.size = float(min(6, max(3, round(rows * 0.26))))
        shore.gap, shore.density = round(shore.size) + 3, 0.6
        out.append(_brush(trip, rows, ("palm", "tree")))
    return hz, out


def _alps(trip: Trip, rows: int) -> tuple[int, list[Layer]]:
    s, hz = trip.seed, round(rows * 0.66)
    rock = (112, 112, 126)
    peaks = Layer(Strip(s, "mountains", 22), 0.86, hz + 1, rows * 0.6, rock, snow=0.42)
    woods = Layer(Strip(s + 2, "hills", 30), 0.3, rows * 0.88, rows * 0.26)
    out = [peaks, woods, _verge(trip, rows)]
    if trip.land and rows >= 8:
        mid = Layer(Strip(s + 1, "mountains", 34), 0.6, hz + 2, rows * 0.4)
        mid.colour, mid.snow = {**GROUND, "summer": (70, 120, 80)}, 0.8
        woods.things, woods.gap, woods.density = (
            ("pine", "pine", "house", "pine"),
            3,
            0.7,
        )
        woods.size = float(min(6, max(3, round(rows * 0.24))))
        out = [peaks, mid, woods, out[-1], _brush(trip, rows, ("pine",))]
    return hz, out


def _desert(trip: Trip, rows: int) -> tuple[int, list[Layer]]:
    s, hz = trip.seed, round(rows * 0.6)
    mesas = Layer(Strip(s, "mesas", 13), 0.84, hz + 1, rows * 0.3, (204, 116, 76))
    dunes = Layer(
        Strip(s + 1, "dunes", 34), 0.4, rows * 0.86, rows * 0.3, (232, 190, 124)
    )
    out = [mesas, dunes, _verge(trip, rows, (214, 170, 110))]
    if trip.land and rows >= 8:
        dunes.things, dunes.gap, dunes.density = (
            ("cactus", "rock", "cactus", "bush"),
            6,
            0.5,
        )
        dunes.size = float(min(5, max(3, round(rows * 0.2))))
        out.append(_brush(trip, rows, ("cactus", "rock")))
    return hz, out


def _town(trip: Trip, rows: int) -> tuple[int, list[Layer]]:
    s, hz = trip.seed, round(rows * 0.72)
    far = Layer(Strip(s, "city", 5), 0.72, rows, rows * 0.5, (128, 130, 140))
    near = Layer(Strip(s + 1, "city", 8), 0.34, rows, rows * 0.66, (92, 94, 104))
    out = [far, near, _verge(trip, rows, (92, 92, 100))]
    if trip.land and rows >= 8:
        hills = Layer(Strip(s + 2, "hills", 40), 0.92, hz, rows * 0.24, (96, 110, 128))
        out = [hills, *out, _brush(trip, rows, ("tree",))]
    return hz, out


_LAND = {
    "plain": _plain,
    "coast": _coast,
    "alps": _alps,
    "desert": _desert,
    "city": _town,
}


# --- what goes by ----------------------------------------------------


def _wires(
    half: Half, sky: _sky.Sky, shift: float, clear: tuple[float, float] | None = None
) -> None:
    """Poles and two wires that sag from one to the next; no pole
    from the pixel `clear[0]` to `clear[1]`."""
    w, rows = half.w, 2 * half.h
    top = max(1, round(rows * 0.1))
    first = math.floor(shift / POLES) * POLES - shift
    dots = Braille(w, half.h)
    high, sag = 2 * top, max(2.0, rows * 0.24)  # in dots
    for dx in range(2 * w):
        u = (dx / 2 - first) % POLES / POLES
        drop = sag * (1 - (2 * u - 1) ** 2)
        for wire in (0, 3):
            dots.set(dx, round(high + wire + drop))
    half.ink(dots, mix(sky.shade((60, 50, 44)), (0, 0, 0), 0.3))
    wood = sky.shade((92, 66, 44))
    x = first
    while x < w + POLES:
        px = round(x)
        if clear is not None and clear[0] - 2 <= px < clear[1] + 2:
            x += POLES
            continue
        for y in range(top - 1, rows):
            half.set(px, y, wood)
        half.set(px - 1, top, wood)
        half.set(px + 1, top, wood)
        for cy in range(half.h):
            half.over.pop((px, cy), None)
        x += POLES


def _tunnel(half: Half, sky: _sky.Sky, x0: float, x1: float, shift: float) -> None:
    """Dark from the pixel `x0` to `x1`: lamps go by, and the car
    shows in the glass."""
    w, rows = half.w, 2 * half.h
    a, b = max(0, math.ceil(x0)), min(w, math.ceil(x1))
    for x in range(a, b):
        for y in range(rows):
            half.px[y][x] = DARK
        for cy in range(half.h):
            half.over.pop((x, cy), None)
    for edge in (
        math.ceil(x0) - 1,
        math.ceil(x0) - 2,
        math.ceil(x1),
        math.ceil(x1) + 1,
    ):
        for y in range(rows):  # the stone of the mouth
            half.set(edge, y, sky.shade(STONE))
    glass = mix(DARK, (255, 220, 160), 0.16)
    for x in range(a, b):
        half.set(x, 0, glass)  # the lamps of the car
        seat = (x + 2) % 11
        if seat < 6:
            for y in range(round(rows * 0.62) + (seat in (0, 5)), rows):
                half.set(x, y, mix(DARK, (70, 80, 130), 0.3))
        if seat in (2, 3) and x // 11 % 3 == 1:  # someone
            half.set(x, round(rows * 0.62) - 1, mix(DARK, (200, 170, 150), 0.25))
            half.set(x, round(rows * 0.62) - 2, mix(DARK, (200, 170, 150), 0.25))
    lamp = round(rows * 0.3)
    step = 46
    x = math.floor(shift * 1.3 / step) * step - shift * 1.3
    while x < w:
        for tail in range(4):
            if a <= round(x) + tail < b:
                half.set(round(x) + tail, lamp, (255, 196, 110), 1 - tail * 0.27)
        x += step


def _bridge(half: Half, sky: _sky.Sky, x0: float, x1: float, shift: float) -> None:
    """A truss from the pixel `x0` to `x1`."""
    w, rows = half.w, 2 * half.h
    iron = sky.shade(IRON)
    top, low = max(1, round(rows * 0.12)), round(rows * 0.82)
    bay = max(6, low - top)
    for x in range(max(0, math.ceil(x0)), min(w, math.ceil(x1))):
        along = math.floor(x + shift) % (2 * bay)
        half.set(x, top, iron)
        half.set(x, top - 1, iron)
        for y in range(low, rows):
            half.set(x, y, iron if y == low else sky.shade((66, 46, 42)))
        if along % bay == 0:
            for y in range(top, low):
                half.set(x, y, iron)
        slope = along if along < bay else 2 * bay - along
        half.set(x, top + round(slope * (low - top) / bay), iron)
        for cy in range(half.h):
            if cy * 2 >= low or cy * 2 <= top or along % bay == 0:
                half.over.pop((x, cy), None)


def _other(
    half: Half, sky: _sky.Sky, x0: float, x1: float, slide: float, n: int
) -> None:
    """A train the other way, from the pixel `x0` to `x1`."""
    w, rows = half.w, 2 * half.h
    coat = LIVERY[n % len(LIVERY)]
    body, roof = sky.shade(coat), sky.shade(mix(coat, (40, 40, 48), 0.6))
    line = sky.shade(mix(coat, WHITE, 0.7))
    top = round(rows * 0.22)
    sill, head = round(rows * 0.62), round(rows * 0.36)
    pane = mix(LAMP, sky.shade((150, 196, 226)), sky.light)
    for x in range(max(0, math.ceil(x0)), min(w, math.ceil(x1))):
        along = math.floor(x + slide) % 34
        for y in range(top, rows):
            c = roof if y == top else body
            if y == sill + 1:
                c = line
            if head <= y < sill and 2 <= along % 8 < 7 and along < 32:
                c = pane
            if along >= 32:  # between two cars
                c = roof if y > top + 1 else half.px[y][x]
            half.px[y][x] = c
        for cy in range(top // 2, half.h):
            half.over.pop((x, cy), None)


def _station(half: Half, sky: _sky.Sky, at: float, trip: Trip, k: int) -> None:
    """The platform of a stop; `at`: the pixel of its start."""
    w, rows = half.w, 2 * half.h
    long = w + 150
    floor, roof = round(rows * 0.76), max(1, round(rows * 0.26))
    dark = sky.light < 0.4

    def lit(c: RGB) -> RGB:  # its lamps light it at night
        return (
            mix(sky.shade(c), mix(c, (120, 96, 40), 0.35), 0.6)
            if dark
            else sky.shade(c)
        )

    stone, edge = lit(STONE), lit((232, 208, 96))
    beam, post = lit((128, 62, 52)), lit((96, 96, 104))
    for x in range(max(0, math.ceil(at)), min(w, math.ceil(at + long))):
        i = x - math.ceil(at)
        for y in range(floor, rows):
            half.px[y][x] = edge if y == floor else stone
        for cy in range(half.h):
            half.over.pop((x, cy), None)
        if not 24 <= i < long - 24:
            continue
        half.set(x, roof, beam)
        half.set(x, roof - 1, lit((160, 84, 66)))
        bay = (i - 24) % 22
        if bay == 0:
            for y in range(roof + 1, floor):
                half.set(x, y, post)
        elif bay == 11 and dark:
            half.set(x, roof + 1, LAMP)
        elif bay == 11:
            half.set(x, roof + 1, post)
        who = noise(trip.seed + 600 + k, (i - 24) // 5)
        if (i - 24) % 5 == 2 and who < 0.3 and bay not in (0, 1, 21):
            coat = LIVERY[int(who * 40) % len(LIVERY)]
            half.set(x, floor - 1, lit(mix(coat, (30, 30, 40), 0.4)))
            half.set(x, floor - 2, lit(coat))
            half.set(x, floor - 3, lit((226, 190, 160)))
    sign = math.ceil(at) + long // 2 - 4  # the name, on its board
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


def scene(
    w: int,
    h: int,
    when: datetime.datetime,
    t: float = 0.0,
    trip: Trip | None = None,
    pointer: Pointer | None = None,
) -> Rows:
    """The glass of `w` by `h` cells at the local time `when`, `t`
    seconds into the trip, the pointer at `pointer`."""
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
    hz, layers = _LAND[trip.line](trip, rows)
    shift = dist(trip, t)
    clear = [list(row) for row in _sky_of(w, h, hz, sky)]
    half.px = [list(row) for row in clear]
    if kind is not None:
        amount = {"clear": 0.14, "clouds": 0.6}.get(kind, 0.3)
        clouds(half, sky, seed + 7, hz, amount, -shift * 0.012)
        if cover < 0.5:
            clear = [list(row) for row in half.px]
    k = int(t // CYCLE)
    halt = None  # the platform in sight, in pixels
    for n in (k - 1, k):
        if n >= 0 and event(trip, n) == "station":
            at = dist(trip, n * CYCLE + WHEN + BRAKE) - 75 - shift
            if at < w and at + w + 150 > 0:
                halt = (at, at + w + 150)
    marks: list[Mark] = []
    for i, layer in enumerate(layers):
        if trip.line == "coast" and i == 1:
            water(half, sky, hz, seed, t)
        marks += paint(half, layer, sky, shift, season)
        if trip.poles and rows >= 8 and layer.depth == 0.0 and layer.speed is None:
            _wires(half, sky, shift * 0.9, halt)
    if trip.land:
        lights(half, sky, marks, seed, None)
    if kind == "fog":
        fog(half, sky, hz)
    stars(half, clear, sky, seed + 3, None, hz)

    if halt is not None:
        _station(half, sky, halt[0], trip, k)
    for n in (k - 1, k):  # an event may end in the next cycle
        what = event(trip, n) if n >= 0 else None
        start = n * CYCLE + WHEN
        if what == "train":
            p, q = (t - start) / 0.3, (t - start - SPAN[what]) / 0.3
            if p > 0 and q < 1:
                _other(
                    half, sky, w * (1 - p), w * (1 - q) if q > 0 else w, shift * 4, n
                )
        elif what in ("tunnel", "bridge"):
            x0 = dist(trip, start) + w - shift
            x1 = x0 + PACE * SPAN[what]
            if x0 < w and x1 > 0:
                (_tunnel if what == "tunnel" else _bridge)(half, sky, x0, x1, shift)
    if kind == "rain":
        rain(half, sky, seed, t, 1.5)
    elif kind == "snow":
        snow(half, sky, seed, t)
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
    return Era(datetime.datetime.now(), args.speed, args.at, args.season)


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

    def render(w: int, h: int) -> Rows:
        t = clock.monotonic() - start
        return scene(w, h, era.when(t), t, trip, pointer)

    deck = _widgets.Deck(pilot, args)
    border = BORDER if args.border == "car" else args.border
    widget = deck.add("train", list(SIZES), render, border=border, title="")
    # The next frame shows the breath: no draw of its own.
    deck.on_hover(widget, lambda x, y: pointer.move(x, y) or False, pointer.leave)
    deck.run(PERIOD, seconds)


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
        "-p", "--poles", action="store_true", help="poles and their sagging wires"
    )
    out.add_argument(
        "-e",
        "--events",
        action="store_true",
        help="a tunnel, a bridge, a train the other way, a station",
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
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
