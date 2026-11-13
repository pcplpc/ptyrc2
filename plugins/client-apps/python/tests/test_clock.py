# SPDX-License-Identifier: GPL-3.0-or-later
"""The faces of `clock`, without a driver."""

from __future__ import annotations

import unicodedata
from datetime import UTC, datetime
from zoneinfo import ZoneInfo

import pytest
from ptyrc_apps import clock

TIMES = (
    datetime(2026, 11, 13, 0, 0, 0),
    datetime(2026, 11, 13, 9, 5, 7),
    datetime(2026, 12, 31, 23, 59, 59),
)
MODES = [(s, b) for s in (True, False) for b in (False, True)]


def plain(rows: clock.Rows, w: int) -> list[str]:
    return ["".join(str(run["t"]) for run in row).ljust(w) for row in rows]


@pytest.mark.parametrize(("seconds", "big"), MODES)
@pytest.mark.parametrize("now", TIMES)
def test_bounds(now: datetime, seconds: bool, big: bool) -> None:
    for (w, h), draw in clock.ladder(seconds, big).items():
        rows = draw(now, w, h)
        assert 0 < len(rows) <= h, (w, h)
        for text in plain(rows, w):
            assert len(text) == w, (w, h, text)
            assert all(unicodedata.east_asian_width(c) not in "WF" for c in text)
            assert all(unicodedata.category(c)[0] != "C" for c in text)
        # Nothing cut: in a larger box, the same rows.
        large = [t.strip() for t in plain(draw(now, w + 20, h + 4), w + 20)]
        assert [t for t in large if t] == [
            t.strip() for t in plain(rows, w) if t.strip()
        ]


@pytest.mark.parametrize("now", TIMES)
def test_digits(now: datetime) -> None:
    hm, s = f"{now:%H:%M}", f"{now:%S}"
    for seconds in (True, False):
        faces = clock.ladder(seconds)
        texts = {size: plain(draw(now, *size), size[0]) for size, draw in faces.items()}
        assert texts[5, 1] == [hm]
        assert texts[10, 2][0].strip() == (f"{hm}:{s}" if seconds else hm)
        assert texts[10, 2][1] == f"{now:%a} {now.day} {now:%b}".center(10)
        for size in (17, 3), (17, 5):
            assert clock.decode(texts[size][:3], clock.SMALL) == hm
        for size in (32, 5), (32, 7):
            assert clock.decode(texts[size][:5], clock.LARGE) == hm
        grand = texts[max(faces)]
        assert clock.decode(grand[:5], clock.LARGE) == (hm + "?" if seconds else hm)
        assert f"{now:%A} {now.day} {now:%B} {now.year}" in grand[6]
        assert f"week {now.isocalendar().week}" in grand[7]
        if seconds:
            assert texts[8, 1] == [f"{hm}:{s}"]
            assert clock.decode([r[-7:] for r in grand[2:5]], clock.SMALL) == s


def test_fonts() -> None:
    for font, (w, h) in (clock.SMALL, (3, 3)), (clock.LARGE, (6, 5)):
        digits = font[:10]
        assert len({tuple(g) for g in font}) == 11
        assert all(len(g) == h and {len(r) for r in g} == {w} for g in digits)
        rows = [" ".join(g[y] for g in digits) for y in range(h)]
        assert clock.decode(rows, font) == "0123456789"


def test_blink() -> None:
    def colon(now: datetime, seconds: bool) -> object:
        rows = clock.face(now, 17, 3, seconds=seconds)
        return next(r["fg"] for r in rows[0] if r["t"] == clock.SMALL[10][0])

    even, odd = datetime(2026, 1, 1, 9, 5, 6), datetime(2026, 1, 1, 9, 5, 7)
    assert colon(even, True) != colon(odd, True)
    assert colon(even, False) == colon(odd, False)


def test_dial() -> None:
    """At 3:00, the hour hand points right, the minute hand up; the
    second marks the rim."""
    rows = clock.face(datetime(2026, 1, 1, 3, 0, 30), 17, 8)
    at = {
        colour: [
            (x, y)
            for y, row in enumerate(rows)
            for x, run in enumerate(row)
            if run.get("fg") == colour
        ]
        for colour in (clock.HOURS, clock.MINUTES, clock.SECONDS)
    }
    assert all(len(row) == 17 for row in rows)
    assert at[clock.HOURS] and all(x >= 8 and y in (3, 4) for x, y in at[clock.HOURS])
    assert at[clock.MINUTES] and all(x == 8 and y < 4 for x, y in at[clock.MINUTES])
    assert at[clock.SECONDS] == [(8, 7)]
    quiet = clock.face(datetime(2026, 1, 1, 3, 0, 30), 17, 8, seconds=False)
    assert not any(r.get("fg") == clock.SECONDS for row in quiet for r in row)


def test_ladders() -> None:
    assert clock.SIZES == list(clock.ladder())
    assert clock.SIZES == [
        (5, 1),
        (8, 1),
        (10, 2),
        (17, 3),
        (17, 5),
        (17, 8),
        (32, 5),
        (32, 7),
        (41, 8),
    ]
    assert list(clock.ladder(False)) == [
        (5, 1),
        (10, 2),
        (17, 3),
        (17, 5),
        (17, 8),
        (32, 5),
        (32, 7),
        (32, 8),
    ]
    assert list(clock.ladder(True, True)) == [
        (17, 3),
        (17, 5),
        (32, 5),
        (32, 7),
        (41, 8),
    ]
    assert list(clock.ladder(False, True)) == [
        (17, 3),
        (17, 5),
        (32, 5),
        (32, 7),
        (32, 8),
    ]
    for seconds in (True, False):
        sizes = list(clock.ladder(seconds))
        assert sizes == sorted(sizes, key=lambda s: s[0] * s[1])


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert clock.main(["--print"]) == 0
    out = capsys.readouterr().out
    assert out.count("┌") == len(clock.SIZES) and "\x1b" not in out
    assert out.count(" clock ") == len([s for s in clock.SIZES if s[0] >= 9])
    assert clock.main(["--print", "--big", "--no-seconds"]) == 0
    assert capsys.readouterr().out.count("┌") == 5


@pytest.mark.parametrize(
    ("argv", "tz"),
    [(["--tz", "Asia/Tokyo"], ZoneInfo("Asia/Tokyo")), (["--utc"], None)],
)
def test_zone(
    argv: list[str], tz: ZoneInfo | None, capsys: pytest.CaptureFixture[str]
) -> None:
    before = datetime.now(tz or UTC)
    assert clock.main(["--print", *argv]) == 0
    after = datetime.now(tz or UTC)
    first = capsys.readouterr().out.split("\n")[2]
    assert first in (f"│{before:%H:%M}│", f"│{after:%H:%M}│")


def test_bad_zone(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exc:
        clock.main(["--print", "--tz", "Nowhere/Atlantis"])
    assert exc.value.code == 2
    assert "unknown time zone" in capsys.readouterr().err
