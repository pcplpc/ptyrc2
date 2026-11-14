# SPDX-License-Identifier: GPL-3.0-or-later
"""Landscapes from a seed, in layers, on a `_canvas.Half`.

Pure: a seed, a place along the strip and a `_sky.Sky` give the pixels.

- `Strip(seed, kind)`: an endless profile; `height(x)` in `0..1` for
  any `x`, in pixels of its own layer. Kinds: `hills`, `mountains`,
  `plain`, `dunes`, `city`, `mesas`, `flat`. `period`: the profile repeats.
- `Layer`: a strip at a `depth`, 0 the nearest to 1 the farthest: its
  ground line (`base`, the pixel row of height 0), its relief (`amp`,
  pixels), its colour a season, and the things that stand on it
  (`things`: kinds drawn one a `gap`, with `density`; none within
  the spans of `bare`, in pixels of the layer). A layer scrolls
  at `speed`, `(1 - depth)²` by default: `shift` pixels of the nearest
  plane are `shift * speed` of this one.
  The ground is one colour, or more: `fields`, a patchwork of crops
  in bands that follow the relief; `strata`, level beds of rock;
  `streak`, the share of it in short dashes, darker, lighter: what
  shows that a near plane runs.
  Things: `tree`, `pine`, `poplar`, `bush`, `palm`, `cactus`, `rock`,
  `hay`, `sheep`, `cow`, `pylon`, `hut`; with a window to light:
  `house`, `chalet`, `church`, `lighthouse`; `barn`.
- `heavens(half, sky, horizon)`: the sky down to the pixel row
  `horizon`, the sun and the moon; it returns the pixels as painted.
  `clouds(...)` next, then `paint(half, layer, sky, shift)` far to
  near, then `stars(half, clear, sky, seed, t)`: stars only in the
  cells still as `heavens` left them.
- `paint` returns `Mark`s, in pixels: the `window` and the `chimney`
  of each house, for the scene to light (`lights`) and to smoke. A
  window keeps its `pane`: painted over since, it is hidden, and
  stays dark.
- `water(half, sky, horizon, seed, t)`: a sea under the horizon, the
  sky mirrored. `fog`, `rain`, `snow`: the weather over what is
  painted; rain and snow are braille, and move with `t`.
- `Pointer`: the host pointer over a scene, for the hovers:
  `move(x, y)` and `leave()` are the handlers; `at`, `since`, `trail`.

Colours of the land are given for full daylight; `Sky.shade` lights
and hazes them. At night what is pale keeps a glow (`NIGHT`): snow,
sand, a wall.
"""

from __future__ import annotations

import math
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from typing import NamedTuple

from ._canvas import RGB, Braille, Half, mix
from ._sky import Sky

__all__ = [
    "KINDS",
    "Fields",
    "Layer",
    "Mark",
    "Pointer",
    "Strip",
    "Thing",
    "clouds",
    "fog",
    "heavens",
    "lights",
    "noise",
    "paint",
    "rain",
    "snow",
    "stars",
    "water",
    "wave",
]

KINDS = ("hills", "mountains", "plain", "dunes", "city", "mesas", "flat")
ALT_TOP = 76.0  # altitude, degrees, drawn at the top of the sky

WHITE: RGB = (255, 255, 255)
SNOW: RGB = (232, 236, 242)
TRUNK: RGB = (125, 85, 30)
LAMP: RGB = (255, 214, 120)
SEA: RGB = (0, 92, 140)
DROP: RGB = (150, 172, 208)
MIST: RGB = (206, 210, 214)
NIGHT: RGB = (98, 104, 146)  # what is pale, at night
#: The ground of a season, in daylight.
GROUND: dict[str, RGB] = {
    "spring": (135, 190, 70),
    "summer": (90, 160, 40),
    "autumn": (150, 156, 60),
    "winter": (226, 231, 238),
}
WALLS: tuple[RGB, ...] = ((234, 224, 200), (216, 196, 168), (204, 150, 128))
ROOFS: tuple[RGB, ...] = ((170, 72, 54), (124, 84, 70), (96, 96, 110))


def _mix32(n: int) -> int:
    n &= 0xFFFFFFFF
    n ^= n >> 16
    n = n * 0x7FEB352D & 0xFFFFFFFF
    n ^= n >> 15
    n = n * 0x846CA68B & 0xFFFFFFFF
    return n ^ n >> 16


def noise(seed: int, a: int = 0, b: int = 0) -> float:
    """A fixed draw in `[0, 1)` for `(seed, a, b)`."""
    return _mix32(_mix32(_mix32(seed) ^ a * 0x9E3779B1) ^ b * 0x85EBCA6B) / 2**32


def wave(seed: int, x: float, period: int | None = None) -> float:
    """Smooth noise in `0..1`: a draw an integer of `x`, eased between."""
    i = math.floor(x)
    t = x - i
    t = t * t * (3 - 2 * t)
    a, b = (i, i + 1) if period is None else (i % period, (i + 1) % period)
    return noise(seed, a) * (1 - t) + noise(seed, b) * t


class Strip:
    """An endless profile from a seed. `scale`: pixels a feature."""

    def __init__(
        self,
        seed: int,
        kind: str = "hills",
        scale: float = 16.0,
        period: int | None = None,
    ) -> None:
        if kind not in KINDS:
            raise ValueError(f"unknown strip: {kind}")
        self.seed, self.kind, self.scale = seed, kind, max(1.0, scale)
        #: In features; the profile repeats every `period * scale` pixels.
        self.period = period

    def block(self, x: float) -> tuple[int, int, int, float]:
        """`city`, `mesas`: the block at `x`: its number, left edge, width
        and height (`0..1`; 0: a gap)."""
        cell = max(3, round(self.scale))
        i = math.floor(x / cell)
        n = i if self.period is None else i % self.period
        wide = max(2, cell - int(noise(self.seed, n, 1) * 3))
        tall = noise(self.seed, n, 2)
        tall = 0.0 if tall < 0.12 else 0.25 + 0.75 * tall**1.7
        return n, i * cell, wide, tall

    def height(self, x: float) -> float:
        """The profile at `x`, in `0..1`."""
        s, p = self.seed, self.period
        u = x / self.scale
        if self.kind == "flat":
            return 0.0
        if self.kind in ("city", "mesas"):
            _, left, wide, tall = self.block(x)
            return tall if x - left < wide else 0.0
        if self.kind == "mountains":
            ridge = 1 - abs(2 * wave(s, u, p) - 1)
            fine = 1 - abs(2 * wave(s + 1, u * 3, p and p * 3) - 1)
            return min(1.0, 0.78 * ridge**1.35 + 0.22 * fine * ridge)
        if self.kind == "dunes":
            return wave(s, u, p) ** 2 * 0.8 + 0.1 * wave(s + 1, u * 2, p and p * 2)
        base = 0.68 * wave(s, u, p) + 0.32 * wave(s + 1, u * 2, p and p * 2)
        return base * (0.3 if self.kind == "plain" else 1.0)


class Thing(NamedTuple):
    x: float  # in pixels of the layer
    kind: str
    size: float  # 0..1
    tint: float  # 0..1
    n: int  # its number along the strip


class Mark(NamedTuple):
    kind: str  # `window`, `chimney`
    x: int  # on the canvas, in pixels
    y: int
    n: int  # the number of its thing
    pane: RGB | None = None  # `window`: its colour as painted


@dataclass(frozen=True)
class Fields:
    """A patchwork over the ground of a layer: `crops`, the colours a
    season; a field is `wide` pixels; the bands are `deep` pixel rows
    under the ground line, then a row more each; a border leans
    `lean` pixels a row; `hedge`: the share of fields with one."""

    crops: dict[str, Sequence[RGB]]
    wide: float = 16.0
    deep: int = 2
    lean: float = 0.8
    hedge: float = 0.0

    def at(self, seed: int, x: float, d: int) -> tuple[int, int, bool]:
        """The field `d` rows under the ground line at `x`: its band,
        its number, and whether this pixel is its border."""
        band, tall, low = 0, self.deep, self.deep
        while d >= low:
            band, tall = band + 1, tall + 1
            low += tall
        u = (x + self.lean * d) / self.wide + noise(seed + 41, band) * 9
        n = math.floor(u)
        return band, n, (u - n) * self.wide < 1.0


@dataclass
class Layer:
    strip: Strip
    depth: float
    base: float
    amp: float
    colour: RGB | dict[str, RGB] = field(default_factory=lambda: dict(GROUND))
    things: Sequence[str] = ()
    gap: int = 8
    density: float = 0.6
    size: float = 5.0  # pixels: the height of a tall thing
    snow: float | None = None  # heights over it are snowed
    speed: float | None = None
    bare: Sequence[tuple[float, float]] = ()  # spans without a thing
    fields: Fields | None = None
    strata: Sequence[RGB] = ()  # beds of rock, two pixel rows each
    streak: float = 0.0

    def rate(self) -> float:
        return (1 - self.depth) ** 2 if self.speed is None else self.speed

    def top(self, x: float) -> float:
        """The pixel row of the ground at `x` of the layer."""
        return self.base - self.amp * self.strip.height(x)

    def ground(self, season: str) -> RGB:
        c = self.colour
        return c[season] if isinstance(c, dict) else c

    def standing(self, x0: float, x1: float) -> list[Thing]:
        """The things between `x0` and `x1`, left to right."""
        if not self.things:
            return []
        out = []
        seed = self.strip.seed + 77
        period = self.strip.period
        for i in range(math.floor(x0 / self.gap) - 1, math.floor(x1 / self.gap) + 2):
            n = i if period is None else i % max(1, round(period * self.strip.scale))
            if noise(seed, n, 0) >= self.density:
                continue
            kind = self.things[int(noise(seed, n, 1) * len(self.things))]
            x = i * self.gap + noise(seed, n, 2) * max(1, self.gap - 2)
            if any(a <= x < b for a, b in self.bare):
                continue
            out.append(Thing(x, kind, noise(seed, n, 3), noise(seed, n, 4), n))
        return out


class Pointer:
    """The host pointer over a scene, in cells of its inner box.

    `move` and `leave` are the handlers of a hover. `at`: the cell, or
    None; `since`: when it came in, or went; `trail`: where it has
    been, `(x, y, time)`, the last `keep` seconds. `now`: the clock.
    """

    def __init__(self, now: Callable[[], float], keep: float = 6.0) -> None:
        self.now, self.keep = now, keep
        self.at: tuple[int, int] | None = None
        self.since = -math.inf
        self.trail: list[tuple[int, int, float]] = []

    def move(self, x: int, y: int) -> None:
        t = self.now()
        if self.at is None:
            self.since = t
        self.at = (x, y)
        self.trail = [p for p in self.trail if t - p[2] < self.keep]
        if not self.trail or self.trail[-1][:2] != (x, y):
            self.trail.append((x, y, t))
        else:
            self.trail[-1] = (x, y, t)

    def leave(self) -> None:
        if self.at is not None:
            self.at, self.since = None, self.now()

    def ease(self, t: float, come: float = 0.6, go: float = 1.2) -> float:
        """0 away to 1 here, eased: `come` seconds in, `go` out."""
        if self.since == -math.inf:
            return 0.0
        x = (t - self.since) / (come if self.at is not None else go)
        x = min(1.0, max(0.0, x))
        x = x * x * (3 - 2 * x)
        return x if self.at is not None else 1 - x


# --- the sky ---------------------------------------------------------


def heavens(half: Half, sky: Sky, horizon: int) -> list[list[RGB]]:
    """Paint the sky down to the pixel row `horizon` (and under it, as
    at the horizon), the sun and the moon. The pixels as painted."""
    w = half.w
    horizon = max(1, horizon)
    for y in range(2 * half.h):
        row = half.px[y]
        for x in range(w):
            row[x] = sky.colour((x + 0.5) / w, min(1.0, (y + 0.5) / horizon))
    r = max(1.0, min(w / 20, horizon / 7))
    if sky.sun > -6:
        cx = math.floor(sky.sun_x * w) + (0.5 if round(2 * r) % 2 else 0.0)
        cy = math.floor(horizon * (1 - sky.sun / ALT_TOP)) + cx % 1
        low = 1 - min(1.0, max(0.0, sky.sun / 20))
        disc = mix((255, 246, 200), (255, 150, 70), low)
        seen = 1 - 0.8 * sky.cover
        half.disc(cx, cy, r * 2.2, disc, 0.3 * seen * low)
        half.disc(cx, cy, r, disc, seen)
    if sky.moon > -4 and sky.lit > 0.03:
        cx = math.floor(sky.moon_x * w) + 0.5
        cy = math.floor(horizon * (1 - sky.moon / ALT_TOP)) + 0.5
        _moon(half, sky, cx, cy, max(1.5, math.floor(r * 1.25) + 0.5))
    return [list(row) for row in half.px]


def _moon(half: Half, sky: Sky, cx: float, cy: float, r: float) -> None:
    """The lit side only; the terminator is half an ellipse."""
    seen = (0.45 + 0.55 * (1 - sky.light)) * (1 - 0.85 * sky.cover)
    bright = mix((244, 242, 220), (255, 255, 255), sky.light)
    k = math.cos(2 * math.pi * sky.phase)  # 1 new, -1 full
    waxing = sky.phase < 0.5
    for y in range(int(cy - r - 1), int(cy + r + 2)):
        for x in range(int(cx - r - 1), int(cx + r + 2)):
            dx, dy = x + 0.5 - cx, y + 0.5 - cy
            if dx * dx + dy * dy > r * r:
                continue
            edge = math.sqrt(max(0.0, r * r - dy * dy)) * k
            if (dx if waxing else -dx) >= edge - 0.25:
                half.set(x, y, bright, seen)


def clouds(
    half: Half,
    sky: Sky,
    seed: int,
    horizon: int,
    amount: float = 0.4,
    drift: float = 0.0,
) -> None:
    """Clouds over the sky, `amount` in `0..1`; `drift`: pixels gone
    by, to the right."""
    w = half.w
    count = max(1, round(amount * (2 + w / 7)))
    lap = w + 24
    tone = sky.shade(mix(WHITE, (150, 154, 166), 0.75 * sky.cover))
    for i in range(count):
        far = noise(seed, i, 2)  # 0 high and near to 1 low and far
        long = max(4, round(w / 5 * (0.6 + 0.8 * noise(seed, i, 1)) * (1 - 0.4 * far)))
        left = int((noise(seed, i, 0) * lap + drift * (1.2 - 0.7 * far)) % lap) - 12
        base = round(horizon * (0.14 + 0.5 * far))
        body = mix(tone, sky.colour(0.5, 0.9), 0.3 * far)
        under = mix(body, (70, 76, 96), 0.2 + 0.3 * sky.cover)
        tiers = 1 if long < 6 else 2 if long < 11 else 3
        for tier in range(tiers):  # a flat base, shorter rows over it
            a = round(tier * long * (0.1 + 0.2 * noise(seed, i, 5 + tier)))
            b = round(tier * long * (0.1 + 0.2 * noise(seed, i, 8 + tier)))
            tint = under if tier == 0 and tiers > 1 else body
            half.rect(left + a, base - tier, long - a - b, 1, tint, 0.94)


def stars(
    half: Half,
    clear: Sequence[Sequence[RGB]],
    sky: Sky,
    seed: int,
    t: float | None = None,
    horizon: int | None = None,
) -> None:
    """Stars in the cells still as `clear`, the pixels `heavens` gave;
    with `t`, they twinkle. None by day."""
    seen = sky.stars
    if seen <= 0.05:
        return
    rows = half.h if horizon is None else min(half.h, horizon // 2)
    for cy in range(rows):
        for cx in range(half.w):
            draw = noise(seed, cx * 7919 + 13, cy)
            if draw >= 0.11 or (cx, cy) in half.over:
                continue
            if any(half.px[2 * cy + k][cx] != clear[2 * cy + k][cx] for k in (0, 1)):
                continue
            rank = noise(seed, cx, cy * 104729 + 7)
            bright = (0.45 + 0.55 * rank) * seen
            if t is not None:
                beat = int(t / 0.7 + rank * 9)
                bright *= 0.55 + 0.45 * noise(seed, cx * 131 + cy, beat)
            if bright < 0.2:
                continue
            glyph = "✦" if rank > 0.93 and bright > 0.7 else "·"
            if rank < 0.3:
                glyph = "⠁⠂⠄⠈⠐⠠"[int(rank * 20) % 6]
            tint = ((200, 215, 255), (255, 244, 214), WHITE)[int(rank * 97) % 3]
            under = mix(half.px[2 * cy][cx], half.px[2 * cy + 1][cx], 0.5)
            half.put(cx, cy, glyph, mix(under, tint, min(1.0, bright)))


# --- the land --------------------------------------------------------


def _crown(season: str, tint: float) -> RGB | None:
    """The leaves of a tree; None: bare."""
    if season == "winter":
        return None
    if season == "spring":
        if tint < 0.3:
            return mix((244, 176, 204), (252, 234, 240), tint / 0.3)
        return mix((104, 170, 74), (140, 190, 90), tint)
    if season == "autumn":
        if tint > 0.72:
            return (224, 184, 62)
        return mix((214, 126, 44), (172, 56, 44), tint / 0.72)
    return mix((0, 125, 20), (44, 142, 10), tint)


def _tree(half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit) -> None:
    trunk = lit(TRUNK)
    leaves = _crown(lit.season, thing.tint)
    if leaves is None:  # bare: a trunk, two boughs, a little snow
        for j in range(1, s):
            half.set(x, y - j, trunk)
        if s >= 3:
            half.set(x - 1, y - s + 1, trunk)
            half.set(x + 1, y - s, trunk)
            half.set(x - 1, y - s, lit(SNOW), 0.8)
        return
    stem = max(1, s // 3)
    for j in range(1, stem + 1):
        half.set(x, y - j, trunk)
    r = max(0.8, (s - stem) / 2)
    cx, cy = x + 0.5, y - stem - r
    body = lit(leaves)
    half.disc(cx, cy, r, body)
    if r >= 1.4:
        half.disc(
            cx + r * 0.35, cy + r * 0.4, r * 0.55, lit(mix(leaves, (0, 30, 20), 0.3))
        )
        half.disc(cx - r * 0.3, cy - r * 0.35, r * 0.45, lit(mix(leaves, WHITE, 0.22)))


def _pine(half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit) -> None:
    base = mix((0, 84, 30), (0, 112, 40), thing.tint)
    green, dark = lit(base), lit(mix(base, (0, 24, 20), 0.35))
    snow = lit(SNOW)
    half.set(x, y - 1, lit(TRUNK))
    if s >= 9:
        half.set(x, y - 2, lit(TRUNK))
    tier = s if s < 9 else max(3, s // 4)  # boughs in tiers, when tall
    for j in range(s - 1):  # from the top
        wide = (j + 1) // 2 if s < 9 else round(j * 0.3 + j % tier * 0.5)
        white = lit.season == "winter" and (j < 1 or (j % 2 == 1 and thing.tint < 0.6))
        for dx in range(-wide, wide + 1):
            edge = white and (j < 1 or abs(dx) == wide)
            c = dark if dx > 0 else green
            half.set(x + dx, y - s + j, snow if edge else c)


def _poplar(half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit) -> None:
    half.set(x, y - 1, lit(TRUNK))
    leaves = _crown(lit.season, 0.4 + 0.5 * thing.tint)
    if leaves is None:
        for j in range(2, s + 1):
            half.set(x, y - j, lit(TRUNK))
        return
    for j in range(2, s + 2):
        half.set(x, y - j, lit(leaves))
        if 2 < j < s + 1 and s >= 4:
            half.set(x + 1, y - j, lit(mix(leaves, (0, 30, 20), 0.3)))


def _house(half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit) -> list[Mark]:
    wide = max(3, s)
    tall = max(2, round(s * 0.5))
    wall = WALLS[int(thing.tint * 3) % 3]
    roof = ROOFS[int(thing.tint * 7) % 3]
    if thing.kind == "barn":
        wide, wall, roof = wide + 1, (158, 60, 48), (84, 70, 70)
    elif thing.kind == "chalet":
        wall, roof = (132, 92, 60), (92, 70, 60)
    wall, roof = lit(wall), lit(SNOW if lit.season == "winter" else roof)
    if thing.kind == "church":  # a tower at one end, a spire
        tx = x - 2 if thing.tint < 0.5 else x + wide
        half.rect(tx, y - tall - 3, 2, tall + 3, wall)
        half.rect(tx, y - tall - 4, 2, 1, roof)
        half.set(tx + (thing.tint < 0.5), y - tall - 5, roof)
    half.rect(x, y - tall, wide, tall, wall)
    rows = max(1, round(s * 0.34))
    for j in range(rows):  # the eaves first
        inset = j if rows > 1 else 0
        half.rect(x - 1 + inset, y - tall - 1 - j, wide + 2 - 2 * inset, 1, roof)
    marks: list[Mark] = []
    if thing.kind == "barn":  # a wide door, no window
        half.rect(x + wide // 2 - 1, y - min(2, tall), 2, min(2, tall), roof)
        return marks
    wx, wy = x + (1 if thing.tint < 0.5 else wide - 2), y - max(1, tall - 1)
    pane = lit(mix((70, 90, 120), wall, 0.2))
    half.set(wx, wy, pane)
    marks.append(Mark("window", wx, wy, thing.n, pane))
    if wide >= 4:
        px = x + (wide - 2 if thing.tint < 0.5 else 1)
        half.set(px, y - 1, lit(mix(TRUNK, (40, 30, 30), 0.4)))  # the door
        if thing.kind in ("house", "chalet"):
            top = y - tall - rows - 1
            half.set(px, top, lit(mix(ROOFS[1], (40, 40, 40), 0.3)))
            marks.append(Mark("chimney", px, top, thing.n))
    return marks


def _small(half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit) -> None:
    kind = thing.kind
    if kind == "sheep":
        if lit.season in ("spring", "summer"):
            half.set(x, y - 1, lit((244, 242, 234)))
            half.set(x + 1, y - 1, lit((244, 242, 234)))
            half.set(x + (2 if thing.tint < 0.5 else -1), y - 1, lit((60, 54, 52)))
    elif kind == "bush":
        green = _crown(lit.season, thing.tint) or mix(TRUNK, SNOW, 0.5)
        half.rect(x, y - 1, 2 + (thing.size > 0.5), 1, lit(green))
        if thing.size > 0.7:
            half.set(x + 1, y - 2, lit(green))
    elif kind == "rock":
        half.rect(x, y - 1, 2, 1, lit((128, 126, 130)))
        if thing.size > 0.5:
            half.set(x, y - 2, lit((150, 148, 150)))
    elif kind == "hay":
        if lit.season in ("summer", "autumn"):
            half.rect(x, y - 1, 3, 1, lit((218, 184, 84)))
            half.set(x + 1, y - 2, lit((232, 200, 100)))
    elif kind == "cactus":
        green = lit((62, 122, 74))
        for j in range(1, s + 1):
            half.set(x, y - j, green)
        if s >= 3:
            half.set(x + (1 if thing.tint < 0.5 else -1), y - s + 1, green)
    elif kind == "palm":
        for j in range(1, s):
            half.set(x + (1 if j > s // 2 else 0), y - j, lit((140, 110, 76)))
        green, dark = lit((0, 135, 60)), lit((0, 104, 56))
        cx = x + 1
        half.rect(cx - 2, y - s, 5, 1, green)
        half.rect(cx - 1, y - s - 1, 3, 1, green)
        half.set(cx - 2, y - s + 1, dark)
        half.set(cx + 2, y - s + 1, dark)
        half.set(cx + 1, y - s, dark)
    elif kind == "cow":
        if lit.season != "winter":
            coat = lit((250, 246, 240) if thing.tint < 0.5 else (124, 78, 50))
            half.rect(x, y - 2, 3, 1, coat)
            half.set(x, y - 1, coat)
            half.set(x + 2, y - 1, coat)
            half.set(x + (3 if thing.size < 0.5 else -1), y - 2, lit((60, 50, 46)))
    elif kind == "pylon":
        steel = lit((150, 154, 164))
        for j in range(1, s + 2):
            half.set(x, y - j, steel)
        half.rect(x - 1, y - s, 3, 1, steel)
        if s >= 4:
            half.rect(x - 1, y - s + 2, 3, 1, steel)
    elif kind == "hut":  # of a beach, striped
        coat = ((70, 130, 190), (220, 90, 80), (240, 200, 90))[int(thing.tint * 3) % 3]
        half.rect(x, y - 2, 2, 2, lit(coat))
        half.rect(x, y - 3, 2, 1, lit((244, 240, 230)))


def _lighthouse(
    half: Half, x: int, y: int, s: int, thing: Thing, lit: _Lit
) -> list[Mark]:
    tall = s + 2
    for j in range(1, tall + 1):
        band = lit((206, 62, 54) if j // 2 % 2 else (244, 240, 232))
        half.set(x, y - j, band)
        if j <= tall - 2:
            half.set(x + 1, y - j, band)
    pane = lit((60, 70, 90))
    half.set(x, y - tall, pane)
    half.set(x, y - tall - 1, lit((70, 64, 70)))
    return [Mark("window", x, y - tall, thing.n, pane)]


class _Lit:
    """`Sky.shade` at one depth, and the season."""

    def __init__(self, sky: Sky, depth: float, season: str) -> None:
        self.sky, self.depth, self.season = sky, depth, season
        self.memo: dict[RGB, RGB] = {}

    def __call__(self, c: RGB) -> RGB:
        if c not in self.memo:
            lit = self.sky.shade(c, self.depth)
            # What is pale still shows at night: snow, sand, a wall.
            pale = (min(c) / 255) ** 2 * (1 - self.sky.light)
            self.memo[c] = mix(lit, NIGHT, 0.75 * pale) if pale else lit
        return self.memo[c]


def _city(half: Half, layer: Layer, sky: Sky, shift: float, lit: _Lit) -> None:
    """Windows on the buildings of a `city` strip."""
    strip = layer.strip
    night = 1 - sky.light
    late = 0.35 if sky.hour < 5.5 or sky.hour >= 23.5 else 1.0
    for x in range(half.w):
        n, left, wide, tall = strip.block(x + shift)
        dx = math.floor(x + shift) - left
        if not tall or dx >= wide or dx % 2 == 0 and wide > 2:
            continue
        top = round(layer.base - layer.amp * tall)
        for y in range(top + 1, round(layer.base), 2):
            draw = noise(strip.seed + 5, n * 64 + dx, y - top)
            if night > 0.4:
                if draw < 0.42 * late:
                    warm = mix(LAMP, (255, 240, 200), noise(strip.seed, n, y))
                    half.set(x, y, mix(warm, sky.colour(0.5, 1), 0.5 * layer.depth))
            elif draw < 0.5:
                half.set(x, y, lit(mix((120, 150, 180), (60, 70, 90), draw)))


def _soil(
    layer: Layer, lit: _Lit, season: str, x: int, y: int, d: int, ground: RGB
) -> RGB:
    """The ground of `layer` at the pixel `x` of it, on the row `y`,
    `d` rows under its line."""
    seed = layer.strip.seed
    c = ground
    if layer.strata:
        bed = (round(layer.base) - y) // 2
        c = lit(layer.strata[int(noise(seed + 43, bed) * len(layer.strata))])
    if layer.fields is not None:
        band, n, border = layer.fields.at(seed, x, d)
        crops = layer.fields.crops[season]
        raw = crops[int(noise(seed + 42, n, band) * len(crops))]
        if border and noise(seed + 44, n, band) < layer.fields.hedge:
            raw = mix(raw, (30, 60, 30), 0.45) if season != "winter" else TRUNK
        # In a low light the crops are one: the palette has few darks.
        c = lit(mix(raw, layer.ground(season), 0.8 * (1 - lit.sky.light)))
    if layer.streak:
        run = math.floor(x / 4 + noise(seed + 45, y) * 16)
        draw = noise(seed + 46, run, y)
        if draw < layer.streak:
            c = mix(c, (0, 0, 0) if draw < layer.streak / 2 else WHITE, 0.14)
    return c


def paint(
    half: Half,
    layer: Layer,
    sky: Sky,
    shift: float = 0.0,
    season: str | None = None,
) -> list[Mark]:
    """Paint `layer`, its ground then what stands on it; `shift`:
    pixels the nearest plane has scrolled, to the left."""
    season = season or sky.season
    lit = _Lit(sky, layer.depth, season)
    shift = math.floor(shift * layer.rate() + 1e-6)
    w, rows = half.w, 2 * half.h
    ground = lit(layer.ground(season))
    rim = lit(mix(layer.ground(season), WHITE, 0.16))
    snow = lit(SNOW)
    blocky = layer.strip.kind == "city"
    plain = not (layer.fields or layer.strata or layer.streak)
    for x in range(w):
        high = layer.strip.height(x + shift)
        top = round(layer.base - layer.amp * high)
        white = 0
        if layer.snow is not None and high > layer.snow:
            jag = 0.5 + noise(layer.strip.seed, math.floor(x + shift), 9)
            white = round((high - layer.snow) * layer.amp * jag) + 1
        for y in range(max(0, top), rows):
            if y - top < white:
                c = snow
            elif y == top and not blocky and high > 0:
                c = rim
            elif plain:
                c = ground
            else:
                c = _soil(layer, lit, season, x + shift, y, y - top, ground)
            half.px[y][x] = c
    if blocky:
        _city(half, layer, sky, shift, lit)
    marks: list[Mark] = []
    reach = round(layer.size) + 3
    for thing in layer.standing(shift - reach, shift + w + reach):
        x = math.floor(thing.x) - shift
        y = round(layer.top(thing.x))
        s = max(2, round(layer.size * (0.6 + 0.4 * thing.size)))
        if thing.kind == "tree":
            _tree(half, x, y, s, thing, lit)
        elif thing.kind == "pine":
            _pine(half, x, y, s + 1, thing, lit)
        elif thing.kind == "poplar":
            _poplar(half, x, y, s + 1, thing, lit)
        elif thing.kind in ("house", "barn", "chalet", "church"):
            y = round(max(layer.top(thing.x), layer.top(thing.x + s)))
            marks += _house(half, x, y, s, thing, lit)
        elif thing.kind == "lighthouse":
            marks += _lighthouse(half, x, y, s, thing, lit)
        else:
            _small(half, x, y, s, thing, lit)
    return marks


# --- water, weather, lamps -------------------------------------------


def water(half: Half, sky: Sky, hz: int, seed: int, t: float) -> None:
    w, rows = half.w, 2 * half.h
    deep = sky.shade(SEA)
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


def fog(half: Half, sky: Sky, hz: int) -> None:
    grey = sky.shade(MIST)
    rows = 2 * half.h
    for y in range(rows):
        d = (y - hz) / rows
        thick = 0.82 - (1.5 * d if d > 0 else -0.9 * d)
        for x in range(half.w):
            half.set(x, y, grey, max(0.25, thick))


def rain(
    half: Half,
    sky: Sky,
    seed: int,
    t: float,
    lean: float = 0.0,
    rough: float = 0.0,
) -> None:
    """Drops over the scene; `lean`: dots aside a dot fallen; `rough`:
    how much a drop tints a pixel where a cell has two colours
    (`Half.ink`), 0: no drop there."""
    dots = Braille(half.w, half.h)
    tall = 4 * half.h
    for col in range(2 * half.w):
        if noise(seed + 21, col) > 0.42:
            continue
        fall = 9 + 6 * noise(seed + 22, col)
        y = int(t * fall + noise(seed + 23, col) * 97) % (tall + 5)
        for k in range(4 if lean else 2):  # a streak, leaning
            aside = int((y - k) * lean) if lean else (y - k) // 5 % 2
            dots.set((col - aside) % (2 * half.w), y - k)
    half.ink(dots, sky.shade(DROP), 0.75, rough)


def snow(half: Half, sky: Sky, seed: int, t: float, rough: float = 0.0) -> None:
    """Flakes over the scene; `rough`: as for `rain`."""
    dots = Braille(half.w, half.h)
    wide, tall = 2 * half.w, 4 * half.h
    for i in range(max(3, half.w * half.h // 7)):
        fall = 1.6 + 1.8 * noise(seed + 31, i)
        y = int(t * fall + noise(seed + 32, i) * tall) % tall
        sway = math.sin(t * 0.7 + i) * 1.6
        dots.set(int(noise(seed + 33, i) * wide + sway + t * 0.4) % wide, y)
    half.ink(dots, mix(WHITE, sky.shade(WHITE), 0.5), rough=rough)


def lights(
    half: Half,
    sky: Sky,
    marks: Sequence[Mark],
    seed: int,
    pointer: Pointer | None,
) -> None:
    """The windows of the houses: lit at night, fewer late; lit under
    the pointer at any time. Not those painted over since."""
    late = sky.hour >= 23.25 or sky.hour < 5.5
    for mark in marks:
        if mark.kind != "window":
            continue
        if mark.pane is not None and half.get(mark.x, mark.y) != mark.pane:
            continue
        home = noise(seed + 95, mark.n)
        lit = sky.light < 0.4 and home < (0.25 if late else 0.85)
        if pointer is not None and pointer.at is not None:
            px, py = pointer.at
            lit = lit or (abs(px - mark.x) <= 3 and abs(py - mark.y // 2) <= 2)
        if lit:
            half.set(mark.x, mark.y, LAMP)
