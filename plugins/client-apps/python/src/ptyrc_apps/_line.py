# SPDX-License-Identifier: GPL-3.0-or-later
"""The land along a railway line, in layers of `_strip`.

`land(line, seed, rows, rich)`: the `Land` of a line of `LINES` for a
glass of `rows` pixel rows; `rich`: ridges, trees, houses, what
brushes the glass.

- `back`: the layers beyond the track, far to near; the last is the
  verge, where the poles stand, at the pace of the nearest plane;
- `front`: the layers nearer than the poles: what brushes the glass,
  then the ballast of the next track, twice as fast as the verge;
- `hz`: the pixel row of the horizon; `sea`: water under it, painted
  after the first layer.

The lines: `plain`: a patchwork of crops (`CROPS`, a season), farms,
a church, poplars; `coast`: isles, the sea, a shore with palms, huts,
a lighthouse; `alps`: peaks under snow, meadows, chalets; `desert`:
mesas and buttes in beds of rock, dunes; `city`: two ranks of
buildings, a park.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any, NamedTuple

from ._canvas import RGB
from ._strip import GROUND, Fields, Layer, Strip

__all__ = ["LINES", "Land", "land"]

LINES = ("plain", "coast", "alps", "desert", "city")


class Land(NamedTuple):
    hz: int
    back: list[Layer]
    front: list[Layer]
    sea: bool = False


#: The crops of a plain, a season.
CROPS: dict[str, tuple[RGB, ...]] = {
    "spring": ((135, 190, 70), (112, 172, 66), (226, 208, 70), (136, 102, 70)),
    "summer": ((90, 160, 40), (120, 174, 60), (216, 190, 96), (70, 138, 52)),
    "autumn": ((150, 156, 60), (134, 100, 68), (188, 166, 98), (156, 120, 74)),
    "winter": ((226, 231, 238), (212, 220, 232), (236, 238, 242)),
}
#: The meadows of a mountain.
MEADOWS: dict[str, tuple[RGB, ...]] = {
    "spring": ((120, 180, 76), (140, 192, 90), (104, 166, 70)),
    "summer": ((84, 150, 50), (100, 164, 58), (70, 136, 56)),
    "autumn": ((140, 150, 64), (160, 150, 70), (126, 136, 60)),
    "winter": ((226, 231, 238), (214, 222, 234)),
}
BALLAST: RGB = (122, 112, 106)


def _low(rows: int) -> int:
    """The pixel row of the ballast."""
    return rows - max(1, rows // 11)


def _verge(seed: int, rows: int, colour: Any = None) -> Layer:
    """The ground by the track: the nearest plane, the last of `back`."""
    verge = Layer(Strip(seed + 9, "plain", 24), 0.0, _low(rows) - 0.6, rows * 0.3)
    if colour is not None:
        verge.colour = colour
    verge.streak = 0.3
    return verge


def _bed(seed: int, rows: int) -> Layer:
    """The ballast of the next track, under the glass: it runs."""
    bed = Layer(Strip(seed + 10, "flat"), 0.0, _low(rows), 0.0, BALLAST)
    bed.speed, bed.streak = 2.0, 0.8
    return bed


def _brush(seed: int, rows: int, things: Sequence[str]) -> Layer:
    """What stands by the track: large, quick, seldom."""
    near = Layer(Strip(seed + 8, "flat"), 0.0, rows + 1.0, 0.0, speed=2.0)
    near.things, near.size = things, rows * 0.95
    near.gap, near.density = round(rows * 1.4), 0.3
    return near


def _front(seed: int, rows: int, things: Sequence[str]) -> list[Layer]:
    return [_brush(seed, rows, things), _bed(seed, rows)]


def _size(rows: int, share: float, lo: int, hi: int) -> float:
    return float(min(hi, max(lo, round(rows * share))))


def _plain(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.56)
    far = Layer(Strip(s + 1, "hills", 26), 0.55, hz + rows * 0.1, rows * 0.3)
    fields = Layer(Strip(s + 2, "hills", 44), 0.25, rows * 0.82, rows * 0.2)
    fields.fields = Fields(CROPS, 15.0, max(1, rows // 11), 0.9, 0.5)
    out = [far, fields, _verge(seed, rows)]
    if not rich:
        return Land(hz, out, [])
    ridge = Layer(Strip(s, "mountains", 34), 0.8, hz + 1, rows * 0.26)
    ridge.colour = {**GROUND, "spring": (84, 110, 150), "summer": (84, 110, 150)}
    ridge.colour["autumn"] = (104, 104, 140)
    far.things = ("pine", "tree", "bush", "tree", "poplar")
    far.gap, far.density, far.size = 4, 0.5, 2.0
    fields.things = (
        "tree",
        "house",
        "tree",
        "poplar",
        "bush",
        "hay",
        "tree",
        "cow",
        "barn",
        "sheep",
        "poplar",
        "church",
        "pine",
    )
    fields.size = _size(rows, 0.24, 3, 6)
    fields.gap, fields.density = round(fields.size) + 2, 0.7
    return Land(hz, [ridge, *out], _front(seed, rows, ("tree", "tree", "pine")))


def _coast(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.52)
    amp = rows * 0.3
    isles = Layer(Strip(s + 1, "hills", 30), 0.86, hz + amp * 0.5, amp, (60, 130, 70))
    sand = {**GROUND, "spring": (214, 196, 140), "summer": (214, 196, 140)}
    sand["autumn"] = (200, 182, 134)
    shore = Layer(Strip(s + 2, "dunes", 40), 0.3, rows * 0.84, rows * 0.26, sand)
    shore.streak = 0.3
    grass = {**GROUND, "spring": (150, 176, 96), "summer": (140, 164, 90)}
    out = [isles, shore, _verge(seed, rows, grass)]
    if not rich:
        return Land(hz, out, [], True)
    shore.things = ("palm", "rock", "house", "bush", "palm", "hut", "lighthouse", "hut")
    shore.size = _size(rows, 0.26, 3, 6)
    shore.gap, shore.density = round(shore.size) + 3, 0.6
    return Land(hz, out, _front(seed, rows, ("palm", "tree")), True)


def _alps(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.66)
    rock = (112, 112, 126)
    peaks = Layer(Strip(s, "mountains", 22), 0.86, hz + 1, rows * 0.6, rock, snow=0.42)
    woods = Layer(Strip(s + 2, "hills", 30), 0.3, rows * 0.86, rows * 0.26)
    woods.fields = Fields(MEADOWS, 22.0, max(1, rows // 11), 1.2)
    out = [peaks, woods, _verge(seed, rows)]
    if not rich:
        return Land(hz, out, [])
    mid = Layer(Strip(s + 1, "mountains", 34), 0.6, hz + 2, rows * 0.4)
    mid.colour, mid.snow = {**GROUND, "summer": (70, 120, 80)}, 0.8
    mid.colour["spring"] = (84, 132, 84)
    mid.things, mid.gap, mid.density, mid.size = ("pine",), 3, 0.45, 2.0
    woods.things = ("pine", "pine", "chalet", "pine", "cow", "pine", "church", "pine")
    woods.gap, woods.density = 3, 0.7
    woods.size = _size(rows, 0.24, 3, 6)
    return Land(hz, [peaks, mid, *out[1:]], _front(seed, rows, ("pine",)))


def _desert(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.6)
    mesas = Layer(Strip(s, "mesas", 13), 0.84, hz + 1, rows * 0.3, (204, 116, 76))
    mesas.strata = ((204, 116, 76), (160, 84, 64), (228, 150, 96), (204, 116, 76))
    dunes = Layer(
        Strip(s + 1, "dunes", 34), 0.4, rows * 0.84, rows * 0.3, (232, 190, 124)
    )
    dunes.streak = 0.35
    out = [mesas, dunes, _verge(seed, rows, (214, 170, 110))]
    if not rich:
        return Land(hz, out, [])
    buttes = Layer(Strip(s + 3, "mesas", 9), 0.5, hz + 2, rows * 0.2, (176, 84, 56))
    buttes.strata = ((176, 84, 56), (140, 64, 48), (196, 104, 64))
    dunes.things, dunes.gap, dunes.density = (
        ("cactus", "rock", "cactus", "bush", "cactus"),
        6,
        0.55,
    )
    dunes.size = _size(rows, 0.22, 3, 5)
    return Land(hz, [mesas, buttes, *out[1:]], _front(seed, rows, ("cactus", "rock")))


def _town(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.72)
    far = Layer(Strip(s, "city", 5), 0.72, rows, rows * 0.5, (128, 130, 140))
    near = Layer(Strip(s + 1, "city", 8), 0.34, rows, rows * 0.66, (92, 94, 104))
    out = [far, near, _verge(seed, rows, (92, 92, 100))]
    if not rich:
        return Land(hz, out, [])
    hills = Layer(Strip(s + 2, "hills", 40), 0.92, hz, rows * 0.24, (96, 110, 128))
    park = Layer(Strip(s + 4, "plain", 30), 0.12, rows * 0.95, rows * 0.1)
    park.things, park.gap, park.density = ("tree", "bush", "tree", "poplar"), 7, 0.5
    park.size = _size(rows, 0.2, 3, 5)
    return Land(hz, [hills, far, near, park, out[-1]], _front(seed, rows, ("tree",)))


_LAND = {
    "plain": _plain,
    "coast": _coast,
    "alps": _alps,
    "desert": _desert,
    "city": _town,
}


def land(line: str, seed: int, rows: int, rich: bool = False) -> Land:
    """The land of `line` for `rows` pixel rows."""
    return _LAND[line](seed, rows, rich and rows >= 8)
