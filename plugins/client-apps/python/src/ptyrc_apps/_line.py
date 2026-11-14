# SPDX-License-Identifier: GPL-3.0-or-later
"""The land along a railway line, in layers of `_strip`.

`land(line, seed, rows, rich)`: the `Land` of a line of `LINES` for a
glass of `rows` pixel rows; `rich`: ridges, trees, houses, what
brushes the glass.

- `back`: the layers beyond the track, far to near; the last is the
  verge, where the poles stand, at the pace of the nearest plane;
- `front`: the layers nearer than the poles;
- `hz`: the pixel row of the horizon; `sea`: water under it, painted
  after the first layer.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any, NamedTuple

from ._strip import GROUND, Layer, Strip

__all__ = ["LINES", "Land", "land"]

LINES = ("plain", "coast", "alps", "desert", "city")


class Land(NamedTuple):
    hz: int
    back: list[Layer]
    front: list[Layer]
    sea: bool = False


def _verge(seed: int, rows: int, colour: Any = None) -> Layer:
    """The ground by the track: the nearest plane, the last of `back`."""
    verge = Layer(Strip(seed + 9, "plain", 24), 0.0, rows * 0.99, rows * 0.22)
    if colour is not None:
        verge.colour = colour
    return verge


def _brush(seed: int, rows: int, things: Sequence[str]) -> Layer:
    """What stands by the track: large, quick, seldom."""
    near = Layer(Strip(seed + 8, "flat"), 0.0, rows + 1.0, 0.0, speed=2.0)
    near.things, near.size = things, rows * 0.95
    near.gap, near.density = round(rows * 1.4), 0.3
    return near


def _plain(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.58)
    far = Layer(Strip(s + 1, "hills", 26), 0.55, hz + rows * 0.08, rows * 0.32)
    fields = Layer(Strip(s + 2, "hills", 44), 0.25, rows * 0.84, rows * 0.2)
    out = [far, fields, _verge(seed, rows)]
    if rich:
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
        return Land(hz, [ridge, *out], [_brush(seed, rows, ("tree", "tree", "pine"))])
    return Land(hz, out, [])


def _coast(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.52)
    amp = rows * 0.3
    isles = Layer(Strip(s + 1, "hills", 30), 0.86, hz + amp * 0.5, amp, (60, 130, 70))
    sand = {**GROUND, "spring": (214, 196, 140), "summer": (214, 196, 140)}
    sand["autumn"] = (200, 182, 134)
    shore = Layer(Strip(s + 2, "dunes", 40), 0.3, rows * 0.98, rows * 0.3, sand)
    out = [isles, shore, _verge(seed, rows)]
    if rich:
        shore.things = ("palm", "rock", "house", "bush", "palm")
        shore.size = float(min(6, max(3, round(rows * 0.26))))
        shore.gap, shore.density = round(shore.size) + 3, 0.6
        return Land(hz, out, [_brush(seed, rows, ("palm", "tree"))], True)
    return Land(hz, out, [], True)


def _alps(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.66)
    rock = (112, 112, 126)
    peaks = Layer(Strip(s, "mountains", 22), 0.86, hz + 1, rows * 0.6, rock, snow=0.42)
    woods = Layer(Strip(s + 2, "hills", 30), 0.3, rows * 0.88, rows * 0.26)
    out = [peaks, woods, _verge(seed, rows)]
    if rich:
        mid = Layer(Strip(s + 1, "mountains", 34), 0.6, hz + 2, rows * 0.4)
        mid.colour, mid.snow = {**GROUND, "summer": (70, 120, 80)}, 0.8
        woods.things, woods.gap, woods.density = (
            ("pine", "pine", "house", "pine"),
            3,
            0.7,
        )
        woods.size = float(min(6, max(3, round(rows * 0.24))))
        return Land(hz, [peaks, mid, *out[1:]], [_brush(seed, rows, ("pine",))])
    return Land(hz, out, [])


def _desert(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.6)
    mesas = Layer(Strip(s, "mesas", 13), 0.84, hz + 1, rows * 0.3, (204, 116, 76))
    dunes = Layer(
        Strip(s + 1, "dunes", 34), 0.4, rows * 0.86, rows * 0.3, (232, 190, 124)
    )
    out = [mesas, dunes, _verge(seed, rows, (214, 170, 110))]
    if rich:
        dunes.things, dunes.gap, dunes.density = (
            ("cactus", "rock", "cactus", "bush"),
            6,
            0.5,
        )
        dunes.size = float(min(5, max(3, round(rows * 0.2))))
        return Land(hz, out, [_brush(seed, rows, ("cactus", "rock"))])
    return Land(hz, out, [])


def _town(seed: int, rows: int, rich: bool) -> Land:
    s, hz = seed, round(rows * 0.72)
    far = Layer(Strip(s, "city", 5), 0.72, rows, rows * 0.5, (128, 130, 140))
    near = Layer(Strip(s + 1, "city", 8), 0.34, rows, rows * 0.66, (92, 94, 104))
    out = [far, near, _verge(seed, rows, (92, 92, 100))]
    if rich:
        hills = Layer(Strip(s + 2, "hills", 40), 0.92, hz, rows * 0.24, (96, 110, 128))
        return Land(hz, [hills, *out], [_brush(seed, rows, ("tree",))])
    return Land(hz, out, [])


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
