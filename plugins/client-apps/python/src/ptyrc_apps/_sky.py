# SPDX-License-Identifier: GPL-3.0-or-later
"""The sky of a place at a time: light, colours, sun, moon, season.

Pure: the time is an argument. `at(when, lat)` gives a `Sky`:

- `sun`, `moon`: altitudes in degrees over the horizon; `sun_x`,
  `moon_x`: where across a view to the south (to the north under the
  equator), 0 the left edge, 1 the right; out of `0..1`: out of view.
- `phase`: of the moon, 0 new, 0.5 full; `lit`: its lit share.
- `light`: 0 at night to 1 by day; `stars`: how well they show, `0..1`.
- `season`: `spring`, `summer`, `autumn`, `winter`, by the day and the
  hemisphere.
- `colour(u, v)`: the sky at `u` across (`0..1`) and `v` down, 0 the
  zenith, 1 the horizon. `shade(c, depth)`: a colour of the land under
  this light, `depth` 0 near to 1 far, hazed towards the horizon.
- `cover`, `0..1`: clouds; they grey the sky and the light.

`weather(day, seed, season)`: the weather of a day, the same all day.
Colours are `(r, g, b)`; `_canvas.xterm` names them in the 256.

A sketch, not an almanac: the sun within a few degrees, the moon a
circle the sun lights, no equation of time, no longitude.
"""

from __future__ import annotations

import datetime
import math
from dataclasses import dataclass

from ._canvas import RGB, mix

__all__ = [
    "SEASONS",
    "SYNODIC",
    "WEATHERS",
    "Sky",
    "at",
    "phase_of",
    "season_of",
    "weather",
]

SEASONS = ("spring", "summer", "autumn", "winter")
WEATHERS = ("clear", "clouds", "rain", "fog", "snow")
SYNODIC = 29.530588853  # days from a new moon to the next
NEW_MOON = datetime.datetime(2000, 1, 6, 18, 14)
TILT = 23.44  # of the Earth, degrees
SPAN = 110.0  # hour angle, degrees, at the edges of the view

# By altitude of the sun: (degrees, zenith, horizon). The horizon of
# the low sun is `DAWN` or `DUSK`.
_KEYS: tuple[tuple[float, RGB, RGB], ...] = (
    (-18.0, (0, 0, 50), (0, 6, 112)),
    (-9.0, (0, 0, 95), (80, 20, 130)),
    (-4.0, (10, 10, 135), (215, 95, 95)),
    (0.0, (70, 90, 190), (255, 140, 90)),
    (5.0, (80, 135, 215), (255, 205, 140)),
    (14.0, (70, 140, 230), (175, 215, 245)),
    (40.0, (60, 130, 225), (150, 205, 245)),
)
DAWN: RGB = (250, 150, 150)
DUSK: RGB = (255, 130, 60)
GLOW: RGB = (255, 200, 110)
# The light on the land, by altitude of the sun.
_LIGHT: tuple[tuple[float, RGB], ...] = (
    (-18.0, (42, 52, 104)),
    (-6.0, (70, 80, 130)),
    (0.0, (115, 125, 140)),
    (6.0, (225, 205, 185)),
    (16.0, (255, 255, 255)),
)


def _step(lo: float, hi: float, x: float) -> float:
    """0 under `lo`, 1 over `hi`, smooth between."""
    t = min(1.0, max(0.0, (x - lo) / (hi - lo)))
    return t * t * (3 - 2 * t)


def _keyed(keys: tuple[tuple[float, ...], ...], x: float) -> tuple[int, float]:
    """The key under `x` and the share of the way to the next."""
    if x <= keys[0][0]:
        return 0, 0.0
    for i in range(len(keys) - 1):
        if x < keys[i + 1][0]:
            return i, (x - keys[i][0]) / (keys[i + 1][0] - keys[i][0])
    return len(keys) - 2, 1.0


def phase_of(when: datetime.datetime) -> float:
    """The phase of the moon: 0 new, 0.5 full."""
    if when.tzinfo is not None:
        when = when.astimezone(datetime.UTC).replace(tzinfo=None)
    return (when - NEW_MOON).total_seconds() / 86400 / SYNODIC % 1.0


def season_of(day: int, lat: float = 47.0) -> str:
    """The season of the day `day` of the year (1: 1 January)."""
    if lat < 0:
        day = (day + 182) % 365
    for name, first in (("winter", 355), ("autumn", 266), ("summer", 172)):
        if day >= first:
            return name
    return "spring" if day >= 80 else "winter"


def weather(day: int, seed: int = 0, season: str = "summer") -> str:
    """The weather of the day `day` (an ordinal), one of `WEATHERS`."""
    n = (day * 0x9E3779B1 ^ seed * 0x85EBCA6B) & 0xFFFFFFFF
    n ^= n >> 15
    n = n * 0x2C1B3C6D & 0xFFFFFFFF
    n ^= n >> 12
    draw = n % 100
    if draw < 40:
        return "clear"
    if draw < 68:
        return "clouds"
    if draw < 84:
        return "snow" if season == "winter" else "rain"
    if draw < 94:
        return "fog"
    return "snow" if season in ("winter", "autumn") and draw < 97 else "rain"


def _altitude(lat: float, dec: float, angle: float) -> float:
    """Degrees over the horizon, at the hour angle `angle`."""
    la, de, ha = math.radians(lat), math.radians(dec), math.radians(angle)
    s = math.sin(la) * math.sin(de) + math.cos(la) * math.cos(de) * math.cos(ha)
    return math.degrees(math.asin(min(1.0, max(-1.0, s))))


def _across(angle: float, lat: float) -> float:
    x = 0.5 + angle / (2 * SPAN)
    return x if lat >= 0 else 1 - x


@dataclass(frozen=True)
class Sky:
    hour: float  # of the day, 0..24, local solar
    day: int  # of the year, from 1
    lat: float
    sun: float
    sun_x: float
    moon: float
    moon_x: float
    phase: float
    cover: float = 0.0

    @property
    def season(self) -> str:
        return season_of(self.day, self.lat)

    @property
    def lit(self) -> float:
        """Lit share of the moon's disc."""
        return (1 - math.cos(2 * math.pi * self.phase)) / 2

    @property
    def light(self) -> float:
        return _step(-6.0, 8.0, self.sun)

    @property
    def stars(self) -> float:
        dark = 1 - _step(-13.0, -3.0, self.sun)
        return dark * (1 - self.cover) * (1 - 0.35 * self.moonlight)

    @property
    def moonlight(self) -> float:
        """0..1: the moon up, by its lit share, at night."""
        return self.lit * _step(-2.0, 12.0, self.moon) * (1 - self.light)

    def _clear(self, u: float, v: float) -> RGB:
        i, t = _keyed(_KEYS, self.sun)
        low = DAWN if self.hour < 12 else DUSK
        ends = []
        for alt, zenith, horizon in (_KEYS[i], _KEYS[i + 1]):
            # The colour of the low sun shows towards it only.
            if -6.0 < alt < 3.0:
                warm = math.exp(-(((u - self.sun_x) / 0.55) ** 2))
                horizon = mix(horizon, low, 0.25 + 0.5 * warm)
            ends.append((zenith, horizon))
        zenith = mix(ends[0][0], ends[1][0], t)
        horizon = mix(ends[0][1], ends[1][1], t)
        c = mix(zenith, horizon, min(1.0, max(0.0, v)) ** 1.6)
        if -8.0 < self.sun < 30.0:
            near = math.exp(-(((u - self.sun_x) / 0.22) ** 2))
            height = _step(-8.0, 1.0, self.sun) * (1 - _step(8.0, 30.0, self.sun))
            c = mix(c, GLOW, 0.45 * near * height * max(0.0, v) ** 2)
        if self.moonlight:
            c = mix(c, (70, 90, 160), 0.22 * self.moonlight)
        return c

    def colour(self, u: float, v: float) -> RGB:
        """The sky at `u` across and `v` down to the horizon."""
        c = self._clear(u, v)
        if self.cover:
            grey = mix((14, 16, 30), (168, 174, 186), self.light)
            grey = mix(grey, c, 0.25)
            lower = mix(grey, (255, 255, 255), 0.12 * v * self.light)
            c = mix(c, lower, self.cover)
        return c

    @property
    def ambient(self) -> RGB:
        """The light on the land: white at noon."""
        i, t = _keyed(_LIGHT, self.sun)
        c = mix(_LIGHT[i][1], _LIGHT[i + 1][1], t)
        if self.moonlight:
            c = mix(c, (110, 125, 175), 0.5 * self.moonlight)
        if self.cover:
            dull = sum(c) // 3
            c = mix(c, (int(dull * 0.8),) * 3, 0.7 * self.cover)  # type: ignore[arg-type]
        return c

    def shade(self, c: RGB, depth: float = 0.0) -> RGB:
        """`c` under this light, hazed by `depth`."""
        a = self.ambient
        lit = (c[0] * a[0] // 255, c[1] * a[1] // 255, c[2] * a[2] // 255)
        return mix(lit, self.colour(0.5, 0.9), 0.5 * depth)


def at(when: datetime.datetime, lat: float = 47.0, cover: float = 0.0) -> Sky:
    """The sky at the local time `when`, at the latitude `lat`."""
    hour = when.hour + when.minute / 60 + when.second / 3600
    day = when.timetuple().tm_yday
    phase = phase_of(when)
    lat = min(66.0, max(-66.0, lat))

    def lean(d: float) -> float:
        return -TILT * math.cos(2 * math.pi * (d + 10) / 365)

    angle = 15 * (hour - 12)
    # The moon trails the sun by its phase; full, it is where the sun
    # will be in half a year.
    trail = (angle - 360 * phase + 180) % 360 - 180
    return Sky(
        hour=hour,
        day=day,
        lat=lat,
        sun=_altitude(lat, lean(day), angle),
        sun_x=_across(angle, lat),
        moon=_altitude(lat, lean(day + 365 * phase), trail),
        moon_x=_across(trail, lat),
        phase=phase,
        cover=min(1.0, max(0.0, cover)),
    )
