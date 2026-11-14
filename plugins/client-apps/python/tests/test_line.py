# SPDX-License-Identifier: GPL-3.0-or-later
"""`_line`: the land of each line, in layers."""

from __future__ import annotations

from datetime import datetime

import pytest
from ptyrc_apps import _line, _sky, _strip
from ptyrc_apps._canvas import Half

NOON = _sky.at(datetime(2026, 7, 20, 12, 30))
NIGHT = _sky.at(datetime(2026, 1, 20, 23, 30))


@pytest.mark.parametrize("line", _line.LINES)
@pytest.mark.parametrize("rows", [2, 6, 8, 12, 16, 22, 32])
def test_land(line: str, rows: int) -> None:
    bare = _line.land(line, 4, rows)
    rich = _line.land(line, 4, rows, True)
    assert bare.sea == rich.sea == (line == "coast")
    assert not bare.front and not any(layer.things for layer in bare.back)
    assert 0 < rich.hz <= rows
    # Far to near; the last of `back` is the verge, at the pace of the way.
    depths = [layer.depth for layer in rich.back]
    assert depths == sorted(depths, reverse=True)
    verge = rich.back[-1]
    assert verge.depth == 0.0 and verge.rate() == 1.0 and not verge.things
    assert all(0 <= verge.top(x) <= rows for x in range(0, 400, 7))
    if rows < 8:  # too small for more
        assert not rich.front
        return
    assert rich.front and all(layer.rate() > 1.0 for layer in rich.front)
    assert any(layer.things for layer in rich.back)
    assert any(layer.things for layer in rich.front)
    # The ballast: under the verge, below every thing by the glass.
    assert rich.front[-1].base > max(verge.top(x) for x in range(400))


@pytest.mark.parametrize("line", _line.LINES)
@pytest.mark.parametrize("season", _sky.SEASONS)
def test_painted(line: str, season: str) -> None:
    """Every kind of thing is drawn, in the canvas only, the same twice."""
    seen = []
    for sky in (NOON, NIGHT):
        frames = []
        for shift in (0.0, 311.0):
            half = Half(48, 11)
            _strip.heavens(half, sky, 12)
            land = _line.land(line, 7, 22, True)
            for layer in (*land.back, *land.front):
                _strip.paint(half, layer, sky, shift, season)
            assert not half.over
            frames.append([list(row) for row in half.px])
        assert frames[0] != frames[1]
        seen.append(frames)
    again = Half(48, 11)
    _strip.heavens(again, NOON, 12)
    land = _line.land(line, 7, 22, True)
    for layer in (*land.back, *land.front):
        _strip.paint(again, layer, NOON, 0.0, season)
    assert again.px == seen[0][0]


def test_lines_differ() -> None:
    looks = set()
    for line in _line.LINES:
        half = Half(48, 11)
        land = _line.land(line, 7, 22, True)
        for layer in (*land.back, *land.front):
            _strip.paint(half, layer, NOON, 40.0, "summer")
        looks.add(tuple(map(tuple, half.px)))
    assert len(looks) == len(_line.LINES)
    with pytest.raises(KeyError):
        _line.land("moon", 1, 22)


def test_bare() -> None:
    """Nothing by the glass within a span of `bare`."""
    land = _line.land("plain", 7, 22, True)
    brush = land.front[0]
    things = brush.standing(0, 4000)
    assert len(things) > 8
    a, b = things[2].x - 1, things[5].x + 1
    brush.bare = [(a, b)]
    left = brush.standing(0, 4000)
    assert len(left) == len(things) - 4 and not any(a <= t.x < b for t in left)
