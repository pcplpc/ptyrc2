# SPDX-License-Identifier: GPL-3.0-or-later
"""`snow`: the effect on a session; the wind tuned in place."""

from __future__ import annotations

import random
import threading
import time

import pytest
from ptyrc import Pilot, PtyrcError, Sprite
from ptyrc import clock as session_clock
from ptyrc_apps import snow

DRIFT = "▁▂▃▄▅▆▇█"


def lying(pilot: Pilot) -> list[int]:
    """Cells of snow on each row of the host."""
    return [sum(c in DRIFT for c in row) for row in pilot.dump()["host"]]


def test_gust() -> None:
    rng = random.Random(4)
    winds = [snow.gust(1.0, rng) for _ in range(200)]
    assert all(-1.0 <= w <= 2.0 and w != 0 for w in winds)
    assert sum(w > 0 for w in winds) > 130 and any(w < 0 for w in winds)


def test_gusts() -> None:
    """The beats of `_cli.follow`: a wait first, then a wind a beat."""

    winds: list[float] = []

    class Vane:
        def tune(self, **params: float) -> None:
            winds.append(params["wind"])

    beats = snow.gusts(Vane(), 1.0, random.Random(4))  # type: ignore[arg-type]
    lo, hi = snow.GUST
    assert lo <= next(beats) <= hi and winds == []
    assert lo <= next(beats) <= hi and len(winds) == 1


def test_snow(session: Pilot) -> None:
    session.send("clear; printf '\\n\\n\\n\\n          %040d\\n' 0\n")
    session.wait_for("0" * 40, frame="raw")
    time.sleep(0.3)
    args = ["-s", session.session, "--cover", "--rate", "900", "--wind", "0"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(snow.main(args, seconds=9)))
    thread.start()
    try:
        deadline = time.monotonic() + 8
        rows: list[int] = []
        while time.monotonic() < deadline:
            rows = lying(session)
            if rows[3] >= 20 and rows[23] >= 40:
                break
            time.sleep(0.1)
        # Over the line, a cell away from the prompt of its own row,
        # and on the floor; none in the air, none over the text.
        assert rows[3] >= 20 and rows[23] >= 40, rows
        # (Row 4 holds the line, and a flake or two over the prompt.)
        assert rows[4] <= 2 and rows[10] == 0, rows
        host = session.dump()["host"]
        assert "0" * 40 in host[4]
        assert all(c not in DRIFT for c in host[3][:10]), host[3]
        # The effect is tuned in place: the snow stays; a bad value is
        # refused by the effect, with its words.
        (sprite,) = session.sprites()
        flakes = Sprite(session, sprite["id"])
        flakes.tune(rate=0)
        assert lying(session)[3] >= 20
        with pytest.raises(PtyrcError, match="wind is out of range"):
            flakes.tune(wind=99)
        # The line changes: its snow melts; the floor keeps its own.
        session.send("clear; printf '\\n\\n\\n\\n          %040d\\n' 1\n")
        session.wait_for("0" * 39 + "1", frame="raw")
        deadline = time.monotonic() + 4
        while lying(session)[3] and time.monotonic() < deadline:
            time.sleep(0.1)
        assert lying(session)[3] == 0
    finally:
        thread.join()
    assert status == [0]
    deadline = time.monotonic() + 3
    while session.sprites() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert session.sprites() == []
    assert session.dump()["host"] == session.text()
    assert snow.main(["-s", session.session, "--rate", "-1"]) == 1


def test_snow_without_cover(session: Pilot) -> None:
    """By default the flakes only fall."""
    session.send("clear; printf '\\n\\n\\n\\n          %040d\\n' 0\n")
    session.wait_for("0" * 40, frame="raw")
    args = ["-s", session.session, "--rate", "900", "--wind", "0"]
    thread = threading.Thread(target=lambda: snow.main(args, seconds=5))
    thread.start()
    try:
        deadline = session_clock.monotonic() + 4
        seen = 0
        while session_clock.monotonic() < deadline:
            assert sum(lying(session)) == 0
            host = "".join(session.dump()["host"])
            seen += any(c in host for c in "*+")
            session_clock.sleep(0.1)
        assert seen > 10
    finally:
        thread.join()
