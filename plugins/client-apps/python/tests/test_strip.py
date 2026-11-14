# SPDX-License-Identifier: GPL-3.0-or-later
"""`_strip`: profiles from a seed, layers, the sky over them."""

from __future__ import annotations

import itertools
from dataclasses import replace
from datetime import datetime

import pytest
from ptyrc_apps import _sky, _strip
from ptyrc_apps._canvas import Half
from ptyrc_apps._strip import Layer, Pointer, Strip

NOON = _sky.at(datetime(2026, 6, 21, 12))
NIGHT = _sky.at(datetime(2026, 6, 21, 0))
EVENING = _sky.at(datetime(2026, 6, 21, 21, 30))


def test_noise() -> None:
    draws = [_strip.noise(3, n) for n in range(2000)]
    assert all(0 <= d < 1 for d in draws)
    assert 0.45 < sum(draws) / len(draws) < 0.55
    assert _strip.noise(3, 5, 7) == _strip.noise(3, 5, 7)
    assert (
        len({_strip.noise(s, a, b) for s in (1, 2) for a in (1, 2) for b in (1, 2)})
        == 8
    )
    # Smooth: it meets the draws on the integers, and moves little between.
    assert _strip.wave(9, 4.0) == _strip.noise(9, 4)
    steps = [_strip.wave(9, x / 50) for x in range(500)]
    assert max(abs(a - b) for a, b in itertools.pairwise(steps)) < 0.05
    assert _strip.wave(9, 13.3, period=5) == pytest.approx(
        _strip.wave(9, 3.3, period=5)
    )


@pytest.mark.parametrize("kind", _strip.KINDS)
def test_profiles(kind: str) -> None:
    strip = Strip(11, kind, 12.0)
    heights = [strip.height(x / 2) for x in range(-400, 400)]
    assert all(0 <= high <= 1 for high in heights)
    assert heights == [Strip(11, kind, 12.0).height(x / 2) for x in range(-400, 400)]
    if kind == "flat":
        assert set(heights) == {0.0}
    else:
        assert max(heights) - min(heights) > 0.15
        assert heights != [
            Strip(12, kind, 12.0).height(x / 2) for x in range(-400, 400)
        ]
    again = Strip(11, kind, 12.0, period=4)  # every 48 pixels
    assert [again.height(x) for x in range(48)] == pytest.approx(
        [again.height(x + 48) for x in range(48)]
    )


def test_strip_kinds() -> None:
    with pytest.raises(ValueError, match="unknown strip"):
        Strip(1, "swamp")
    city = Strip(5, "city", 6)
    for x in range(200):
        n, left, wide, tall = city.block(x)
        assert left <= x < left + 6 and 2 <= wide <= 6
        assert city.block(left) == (n, left, wide, tall)
        assert city.height(x) == (tall if x - left < wide else 0.0)


def test_standing() -> None:
    layer = Layer(
        Strip(3), 0.0, 10.0, 4.0, things=("tree", "house"), gap=6, density=0.7
    )
    things = layer.standing(0, 300)
    assert things == layer.standing(0, 300)
    assert [t.x for t in things] == sorted(t.x for t in things)
    assert {t.kind for t in things} == {"tree", "house"}
    assert all(0 <= t.size < 1 and 0 <= t.tint < 1 for t in things)
    assert 0.5 * 50 < len(things) < 0.9 * 54
    # A thing is where it is, whatever the stretch looked at.
    part = layer.standing(100, 160)
    assert [t for t in things if 100 <= t.x < 160] == [
        t for t in part if 100 <= t.x < 160
    ]
    assert Layer(Strip(3), 0.0, 10.0, 4.0).standing(0, 300) == []


def test_rate() -> None:
    assert Layer(Strip(1), 0.0, 0, 0).rate() == 1
    assert Layer(Strip(1), 1.0, 0, 0).rate() == 0
    assert Layer(Strip(1), 0.5, 0, 0).rate() == 0.25
    assert Layer(Strip(1), 0.5, 0, 0, speed=2.0).rate() == 2.0


def columns(half: Half) -> list[tuple]:
    return [tuple(row[x] for row in half.px) for x in range(half.w)]


@pytest.mark.parametrize("depth", [0.0, 0.5])
def test_paint_scrolls(depth: float) -> None:
    """Scrolled, a layer shows the same land further on."""
    layer = Layer(Strip(8, "hills", 9), depth, 14.0, 9.0, things=("tree", "pine"))
    layer.gap, layer.size = 7, 4.0
    wide, narrow = Half(60, 10), Half(30, 10)
    _strip.paint(wide, layer, NOON, 0.0, "summer")
    step = 16 / layer.rate()
    _strip.paint(narrow, layer, NOON, step, "summer")
    # Away from the edges, where a thing may be cut by the other canvas.
    assert columns(narrow)[:24] == columns(wide)[16:40]
    ground = NOON.shade(layer.ground("summer"), depth)
    assert all(row[x] == ground for row in wide.px[15:] for x in range(60))
    assert any(wide.px[y][x] != (0, 0, 0) for y in range(5, 14) for x in range(60))
    assert all(c == (0, 0, 0) for c in wide.px[0])


def test_paint_seasons() -> None:
    layer = Layer(Strip(8, "hills", 9), 0.0, 14.0, 6.0, things=("tree",), gap=5)
    seen = {}
    for season in _sky.SEASONS:
        half = Half(40, 10)
        _strip.paint(half, layer, NOON, 0.0, season)
        seen[season] = columns(half)
    assert len({tuple(v) for v in seen.values()}) == 4
    white = NOON.shade(_strip.GROUND["winter"])
    assert seen["winter"][0][-1] == white and min(white) > 200


def test_houses() -> None:
    layer = Layer(Strip(2, "plain"), 0.0, 16.0, 2.0, things=("house",), gap=10)
    layer.density, layer.size = 1.0, 6.0
    half = Half(60, 10)
    marks = _strip.paint(half, layer, NIGHT, 0.0, "summer")
    windows = [m for m in marks if m.kind == "window"]
    chimneys = [m for m in marks if m.kind == "chimney"]
    assert len(windows) >= 5 and {m.n for m in chimneys} <= {m.n for m in windows}
    dark = [half.get(m.x, m.y) for m in windows if 0 <= m.x < 60]
    _strip.lights(half, NIGHT, marks, 4, None)
    lit = [half.get(m.x, m.y) for m in windows if 0 <= m.x < 60]
    assert _strip.LAMP in lit and _strip.LAMP not in dark
    # By day none is lit, but the one under the pointer.
    day = Half(60, 10)
    marks = _strip.paint(day, layer, NOON, 0.0, "summer")
    _strip.lights(day, NOON, marks, 4, None)
    assert all(day.get(m.x, m.y) != _strip.LAMP for m in marks)
    pointer = Pointer(lambda: 0.0)
    here = next(m for m in marks if m.kind == "window" and 3 <= m.x < 57)
    pointer.move(here.x + 1, here.y // 2)
    _strip.lights(day, NOON, marks, 4, pointer)
    assert day.get(here.x, here.y) == _strip.LAMP
    assert sum(day.get(m.x, m.y) == _strip.LAMP for m in marks) == 1
    # A window painted over since is hidden: no lamp shows through.
    night = Half(60, 10)
    marks = _strip.paint(night, layer, NIGHT, 0.0, "summer")
    windows = [m for m in marks if m.kind == "window" and 0 <= m.x < 60]
    night.set(windows[0].x, windows[0].y, (0, 90, 0))
    _strip.lights(night, NIGHT, marks, 4, None)
    pointer.move(windows[0].x, windows[0].y // 2)
    _strip.lights(night, NIGHT, marks, 4, pointer)
    assert night.get(windows[0].x, windows[0].y) == (0, 90, 0)


def test_city() -> None:
    layer = Layer(Strip(6, "city", 5), 0.2, 20.0, 14.0, (100, 104, 120))
    night, day = Half(60, 10), Half(60, 10)
    _strip.paint(night, layer, EVENING)
    _strip.paint(day, layer, NOON)
    warm = [c for row in night.px for c in row if c[0] > 200 and c[0] > c[2]]
    assert len(warm) > 10
    assert not [c for row in day.px for c in row if c[0] > 200 and c[0] > c[2] + 40]


def test_heavens() -> None:
    half = Half(40, 12)
    clear = _strip.heavens(half, NOON, 14)
    assert clear == half.px and clear is not half.px
    assert half.px[0][2] == NOON.colour(2.5 / 40, 0.5 / 14)
    assert half.px[20] == half.px[23]  # under the horizon: as at it
    bright = [(x, y) for y in range(6) for x in range(40) if min(half.px[y][x]) > 190]
    assert len(bright) >= 8 and all(abs(x - 19.5) <= 2 for x, _ in bright)  # the sun
    _strip.stars(half, clear, NOON, 5)
    assert not half.over  # none by day
    dark = Half(40, 12)
    clear = _strip.heavens(dark, NIGHT, 14)
    dark.rect(0, 0, 10, 24, (1, 2, 3))  # something before the sky
    _strip.stars(dark, clear, NIGHT, 5, None, 14)
    assert len(dark.over) > 8
    assert all(cx >= 10 and cy < 7 for cx, cy in dark.over)
    still = dict(dark.over)
    dark.over.clear()
    _strip.stars(dark, clear, NIGHT, 5, None, 14)
    assert dark.over == still
    looks = set()
    for beat in range(8):  # they twinkle
        dark.over.clear()
        _strip.stars(dark, clear, NIGHT, 5, beat * 0.7, 14)
        looks.add(tuple(sorted(dark.over.items())))
    assert len(looks) > 4


def test_moon_phases() -> None:
    def lit(when: datetime) -> list[tuple[int, int]]:
        sky = _sky.at(when)
        half = Half(40, 12)
        before = [
            [sky.colour((x + 0.5) / 40, min(1, (y + 0.5) / 20)) for x in range(40)]
            for y in range(24)
        ]
        _strip.heavens(half, sky, 20)
        return [
            (x, y)
            for y in range(24)
            for x in range(40)
            if half.px[y][x] != before[y][x]
        ]

    full = datetime(2026, 11, 24, 23, 30)
    assert _sky.at(full).lit > 0.97
    disc = lit(full)
    assert len(disc) >= 12
    quarter = lit(datetime(2026, 11, 17, 19, 0))  # the first: its right half
    assert 0 < len(quarter) < len(disc)
    xs = [x for x, _ in quarter]
    assert max(xs) - min(xs) < max(x for x, _ in disc) - min(x for x, _ in disc)


def test_weather() -> None:
    half = Half(30, 8)
    _strip.heavens(half, NOON, 10)
    before = [list(row) for row in half.px]
    _strip.clouds(half, NOON, 3, 10, 0.6, 0.0)
    changed = [
        (x, y) for y in range(16) for x in range(30) if half.px[y][x] != before[y][x]
    ]
    assert changed and all(y < 10 for _, y in changed)
    moved = Half(30, 8)
    _strip.heavens(moved, NOON, 10)
    _strip.clouds(moved, NOON, 3, 10, 0.6, 9.0)
    assert moved.px != half.px
    # Clouds of whole cells: the two pixels of a cell are one cloud or none.
    cells = Half(30, 8, (40, 60, 90))
    _strip.clouds(cells, NOON, 3, 10, 0.6, 0.0, 2)
    assert any(c != (40, 60, 90) for row in cells.px for c in row)
    assert all(cells.px[2 * y] == cells.px[2 * y + 1] for y in range(8))
    for fall in (_strip.rain, _strip.snow):
        looks = []
        for t in (0.0, 0.3):
            sheet = Half(30, 8, (40, 60, 90))
            fall(sheet, NOON, 4, t)
            assert sheet.over and all(
                0x2800 < ord(g[0]) < 0x2900 for g in sheet.over.values()
            )
            looks.append(dict(sheet.over))
        assert looks[0] != looks[1]
    misty = Half(30, 8, (0, 90, 0))
    _strip.fog(misty, NOON, 10)
    assert all(min(c) > 40 for row in misty.px for c in row)
    assert sum(misty.px[10][0]) > sum(misty.px[0][0])  # thickest at the horizon


def test_water() -> None:
    half = Half(30, 8)
    _strip.heavens(half, EVENING, 8)
    _strip.water(half, EVENING, 8, 3, 0.0)

    # A mirror: the colours of the horizon next to it, the zenith's at the bottom.
    def far(a: tuple, b: tuple) -> int:
        return sum(abs(p - q) for p, q in zip(a, b, strict=True))

    assert far(half.px[8][15], half.px[7][15]) < far(half.px[15][15], half.px[7][15])
    noon = Half(30, 8)
    _strip.heavens(noon, NOON, 8)
    _strip.water(noon, NOON, 8, 3, 0.0)
    assert all(c[2] > c[0] for row in noon.px[9:] for c in row if min(c) < 200)  # blue


def test_pointer() -> None:
    now = [10.0]
    pointer = Pointer(lambda: now[0], keep=4.0)
    assert pointer.at is None and pointer.ease(10.0) == 0 and pointer.trail == []
    pointer.leave()  # not there: nothing
    assert pointer.ease(99.0) == 0
    pointer.move(3, 1)
    assert pointer.at == (3, 1) and pointer.since == 10.0
    assert pointer.ease(10.0) == 0 and pointer.ease(11) == 1
    assert pointer.ease(10.3) == pytest.approx(0.5)
    now[0] = 11.0
    pointer.move(3, 1)  # still there: one place, its time renewed
    pointer.move(4, 1)
    assert pointer.since == 10.0 and pointer.trail == [(3, 1, 11.0), (4, 1, 11.0)]
    now[0] = 13.0
    pointer.leave()
    assert pointer.at is None and pointer.since == 13.0
    assert pointer.ease(13.0) == 1 and pointer.ease(15) == 0
    assert pointer.ease(13.6) == pytest.approx(0.5)
    now[0] = 16.0
    pointer.move(0, 0)  # the old places are forgotten
    assert pointer.trail == [(0, 0, 16.0)] and pointer.since == 16.0


def test_ground() -> None:
    """`fields`, `strata`, `streak`: a ground of more than one colour,
    that scrolls with its layer."""
    crops = {s: ((200, 180, 90), (90, 160, 40), (140, 100, 70)) for s in _sky.SEASONS}

    def tones(layer: Layer, shift: float = 0.0, sky: _sky.Sky = NOON) -> list[list]:
        half = Half(60, 10)
        _strip.paint(half, layer, sky, shift, "summer")
        return [row[:] for row in half.px[8:]]

    flat = Layer(Strip(2, "flat"), 0.0, 4.0, 0.0)
    assert len({c for row in tones(flat) for c in row}) == 1
    for knob in (
        {"fields": _strip.Fields(crops, 9.0, 2, 1.0, 0.5)},
        {"strata": ((200, 110, 70), (150, 80, 60), (230, 150, 100))},
        {"streak": 0.5},
    ):
        layer = Layer(Strip(2, "flat"), 0.0, 4.0, 0.0, **knob)
        rows = tones(layer)
        assert len({c for row in rows for c in row}) >= 2, knob
        moved = tones(layer, 7.0)
        assert [row[7:] for row in rows] == [row[:-7] for row in moved]
    # A field: bands under the ground line, borders that lean.
    fields = _strip.Fields(crops, 10.0, 2, 1.0)
    assert [fields.at(0, 3.0, d)[0] for d in range(9)] == [0, 0, 1, 1, 1, 2, 2, 2, 2]
    assert fields.at(0, 3.0, 0)[1] == fields.at(0, 2.0, 1)[1] + 0
    assert sum(fields.at(0, x, 0)[2] for x in range(100)) == 10
    # In the dark the crops are nearly one.
    layer = Layer(Strip(2, "flat"), 0.0, 4.0, 0.0, fields=fields)
    by_day = {c for row in tones(layer) for c in row}
    by_night = {c for row in tones(layer, 0.0, NIGHT) for c in row}
    spread = [max(c[1] for c in s) - min(c[1] for c in s) for s in (by_day, by_night)]
    assert spread[0] > 4 * max(1, spread[1])


@pytest.mark.parametrize(
    "kind",
    ["tree", "pine", "poplar", "house", "barn", "chalet", "church", "lighthouse"]
    + ["sheep", "cow", "bush", "rock", "hay", "cactus", "palm", "pylon", "hut"],
)
def test_things(kind: str) -> None:
    for size in (2.0, 5.0, 20.0):
        layer = Layer(Strip(2, "flat"), 0.0, 40.0, 0.0, things=(kind,), gap=24)
        layer.density, layer.size = 1.0, size
        empty, half = Half(80, 22), Half(80, 22)
        _strip.paint(empty, replace(layer, things=()), NOON, 0.0, "summer")
        marks = _strip.paint(half, layer, NOON, 0.0, "summer")
        assert half.px != empty.px and not half.over
        lit = {m.kind for m in marks}
        assert ("window" in lit) == (
            kind in ("house", "chalet", "church", "lighthouse")
        )
        assert ("chimney" in lit) == (kind in ("house", "chalet") and size > 2)
        for m in marks:
            if m.kind == "window" and 0 <= m.x < 80:
                assert half.get(m.x, m.y) == m.pane


def test_pale_at_night() -> None:
    """Snow shows at night; what is dark stays dark."""
    snow = Layer(Strip(2, "flat"), 0.0, 4.0, 0.0, colour=_strip.SNOW)
    soil = Layer(Strip(2, "flat"), 0.0, 4.0, 0.0, colour=(90, 160, 40))
    out = []
    for layer in (snow, soil):
        half = Half(4, 4)
        _strip.paint(half, layer, NIGHT, 0.0, "summer")
        out.append(half.px[7][0])
    assert sum(out[0]) > 2.5 * sum(out[1]) and out[0][2] > out[0][0]
    dark = NIGHT.shade((90, 160, 40))
    assert all(abs(a - b) <= 4 for a, b in zip(out[1], dark, strict=True))
