# SPDX-License-Identifier: GPL-3.0-or-later
"""`globe`: the scene without a driver, the widget on one."""

from __future__ import annotations

import itertools
import threading
import unicodedata

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import globe

FLAGS = ("shake", "drift", "lights", "random")
ALL = globe.Look(**dict.fromkeys(FLAGS, True))
LOOKS = [globe.Look(), ALL, *(globe.Look(**{name: True}) for name in FLAGS)]


def plain(rows: globe.Rows) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


def snow(rows: globe.Rows) -> int:
    """Flakes in the air."""
    return sum(
        len(str(run["t"]))
        for row in rows
        for run in row
        if run.get("fg") == globe.SNOW and set(str(run["t"])) <= set(globe.FLAKES)
    )


def test_art() -> None:
    assert globe.SIZES == tuple(sorted(globe.SIZES))
    for dome in globe.DOMES:
        w, _ = dome.size
        assert all(len(row) == w for row in dome.art)
        free = dome.free()
        assert free and all(0 < x < w - 1 and 0 < y <= dome.ground for x, y in free)
        for art, _ in (s[dome.scale] for s in globe.SCENES.values()):
            # The scene stands inside the glass.
            x0, x1 = dome.span(dome.ground)
            assert len(art[0]) <= x1 - x0 + 1 and len(art) <= dome.ground
    for name, scales in globe.SCENES.items():
        assert set(scales) == {"s", "m", "l"}, name
        for art, mask in scales.values():
            for row, inks in zip(art, mask, strict=True):
                assert len(row) == len(inks)
                for glyph, key in zip(row, inks, strict=True):
                    assert glyph == " " or key in globe.INK, (name, row, inks)
                    assert unicodedata.east_asian_width(glyph) not in "WF"


@pytest.mark.parametrize("look", LOOKS)
def test_bounds(look: globe.Look) -> None:
    for (w, h), t in itertools.product(globe.SIZES, (0.0, 3.7, 30.0, 31.0, 500.0)):
        rows = globe.scene(t, w, h, look, shaken=29.5, shakes=1)
        assert [len(text) for text in plain(rows)] == [w] * h
        # The dome stands whatever falls in it.
        dome = next(d for d in globe.DOMES if d.size == (w, h))
        for y, (text, art) in enumerate(zip(plain(rows), dome.art, strict=True)):
            if y != dome.ground:
                assert all(a in (" ", c) for c, a in zip(text, art, strict=True))


def test_pure() -> None:
    for look in (globe.Look(), ALL):
        assert globe.scene(7.3, 21, 10, look) == globe.scene(7.3, 21, 10, look)
        assert any(
            globe.scene(7.3, 21, 10, look) != globe.scene(7.3 + dt, 21, 10, look)
            for dt in (0.5, 1.0, 1.5)
        )


def test_bare() -> None:
    for w, h in globe.SIZES:
        frames = [globe.scene(t / 2, w, h) for t in range(60)]
        counts = [snow(rows) for rows in frames]
        assert max(counts) > 0 and len(set(map(str, frames))) > 20  # it falls
        text = "\n".join(plain(frames[0]))
        assert "[]" in text and "♣" in text  # the house, a tree


def test_auto() -> None:
    assert globe.auto(1000.0, globe.Look()) is None
    look = globe.Look(shake=True, every=40.0)
    assert globe.auto(1.0, look) is None
    times = sorted({globe.auto(float(t), look) for t in range(10, 400)} - {None})
    assert len(times) == 10  # one an `every`
    assert all(40 * k < at < 40 * (k + 1) for k, at in enumerate(times))
    assert all(globe.auto(t, look) <= t for t in map(float, range(40, 400)))


def test_shake() -> None:
    def mean(since: float) -> float:
        return (
            sum(
                snow(globe.scene(100.0 + since + i / 4, 30, 13, shaken=100.0))
                for i in range(8)
            )
            / 8
        )

    calm = sum(snow(globe.scene(100.0 + i / 4, 30, 13)) for i in range(8)) / 8
    assert mean(0.2) > 1.5 * calm  # thick, then thin again
    assert mean(0.2) > mean(8.0) > mean(60.0) - 3
    assert abs(mean(60.0) - calm) < 4
    # The flakes stay under the glass while they swirl.
    dome = globe.DOMES[-1]
    for i in range(60):
        rows = globe.scene(100.0 + i / 10, 30, 13, shaken=100.0)
        for y, text in enumerate(plain(rows)):
            for x, glyph in enumerate(text):
                if glyph in "·*":
                    assert (x, y) in dome.free()


def test_drift() -> None:
    look = globe.Look(drift=True)

    def lying(since: float) -> tuple[int, int]:
        rows = globe.scene(50.0 + since, 21, 10, look, shaken=50.0)
        ground = sum(str(r["t"]).count(c) for r in rows[6] for c in "▁▂")
        white = sum(r.get("fg") == globe.SNOW for r in rows[4] + rows[5])
        return ground, white

    assert lying(0.5)[0] == 0  # lifted by the shake
    assert lying(14.0)[0] > 0
    assert lying(40.0)[0] >= lying(14.0)[0]
    roofs = [globe.scene(t, 21, 10, look, shaken=50.0)[4] for t in (51.0, 90.0)]
    assert [
        any(r.get("fg") == globe.SNOW and "/" in str(r["t"]) for r in row)
        for row in roofs
    ] == [False, True]
    # Never shaken: it lies.
    assert globe.scene(5.0, 21, 10, look) == globe.scene(5.0, 21, 10, look)
    assert any("▂" in text for text in plain(globe.scene(5.0, 21, 10, look)))


def test_lights() -> None:
    look = globe.Look(lights=True)
    colours = set()
    glint = set()
    for i in range(90):
        rows = globe.scene(i / 5, 21, 10, look)
        colours |= {r["fg"] for r in rows[5] + rows[6] if r.get("fg") in globe.GARLAND}
        glint |= {i for r in rows[0] if r.get("fg") == globe.GLINT}
    assert len(colours) >= 3
    assert 0 < len(glint) < 60  # it passes, then waits
    dark = globe.scene(1.0, 21, 10)
    assert all(
        r.get("fg") not in (*globe.GARLAND, globe.GLINT) for row in dark for r in row
    )


def test_scenes() -> None:
    assert globe.which(globe.Look(scene="fir"), shakes=3) == "fir"
    look = globe.Look(random=True)
    names = [globe.which(look, shakes=n) for n in range(len(globe.SCENES))]
    assert sorted(names) == sorted(globe.SCENES)  # another at each shake
    marks = {"house": "[]", "snowman": '(")', "church": "†", "fir": "/o\\"}
    for name, mark in marks.items():
        for w, h in globe.SIZES:
            text = "\n".join(plain(globe.scene(0.0, w, h, globe.Look(scene=name))))
            assert mark in text, (name, w, h)


def test_shaker() -> None:
    now = [10.0]
    hand = globe.Shaker(lambda: now[0], again=3.0)
    assert (hand.at, hand.count) == (None, 0)
    hand.on_move(2, 2)
    now[0] = 11.0
    hand.on_move(3, 2)  # still over it: the same shake
    assert (hand.at, hand.count) == (10.0, 1)
    now[0] = 13.5
    hand.on_move(3, 3)
    assert (hand.at, hand.count) == (13.5, 2)
    hand.on_leave()
    now[0] = 14.0
    hand.on_move(1, 1)  # back over it
    assert (hand.at, hand.count) == (14.0, 3)


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert globe.main(["--print"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == sum(h for _, h in globe.SIZES)
    assert globe.main(["-a", "--scene", "church", "--print", "21x10", "--at", "9"]) == 0
    out = capsys.readouterr().out
    assert "†" in out and len(out.splitlines()) == 10
    for argv in (["--print", "20x10"], ["--scene", "boat"], ["--every", "0"]):
        with pytest.raises(SystemExit):
            globe.main(argv)
        capsys.readouterr()


@pytest.mark.parametrize("argv", [[], ["-a", "--prefer", "top-right"]])
def test_globe(session: Pilot, argv: list[str]) -> None:
    seen: list[dict] = []
    host: list[str] = []
    run = ["-s", session.session, *argv]
    thread = threading.Thread(target=lambda: globe.main(run, seconds=3))
    thread.start()
    while thread.is_alive():
        seen += [w for w in session.widgets() if w["state"] == "placed"]
        host += session.dump()["host"]
        session_clock.sleep(0.05)
    thread.join()
    assert seen and {w["tag"] for w in seen} == {"globe"}
    assert all(tuple(w["size"]) in globe.SIZES for w in seen)
    # No border: the outer box is the dome.
    assert all(tuple(w["rect"][2:]) == tuple(w["size"]) for w in seen)
    assert any("'-----" in row for row in host)
    assert session.widgets() == []
