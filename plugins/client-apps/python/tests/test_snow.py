# SPDX-License-Identifier: GPL-3.0-or-later
"""`snow`: the effect on a session; the wind tuned in place."""

from __future__ import annotations

import random
import threading
import time
from typing import TYPE_CHECKING

import pytest
from ptyrc import Pilot, PtyrcError, Sprite
from ptyrc import clock as session_clock
from ptyrc_apps import snow

if TYPE_CHECKING:
    from conftest import Host

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


def test_max_depth(capsys: pytest.CaptureFixture[str]) -> None:
    class Stub:
        def effect(self, name: str, params: dict[str, object], **where: object) -> str:
            self.sent = (name, params, where)
            return "sprite"

    class Pane:
        id = 7

    stub = Stub()
    assert snow.snow(stub, Pane(), 14.0, -1.0, True) == "sprite"  # type: ignore[arg-type]
    assert stub.sent[1]["max_depth"] == snow.DEEP == 3
    snow.snow(stub, Pane(), 14.0, -1.0, True, 0)  # type: ignore[arg-type]
    assert stub.sent[0] == "snow" and stub.sent[1]["max_depth"] == 0
    with pytest.raises(SystemExit):
        snow.main(["--cover", "--max-depth", "-1"])
    assert "--max-depth must not be negative" in capsys.readouterr().err


# The drift is shaped by the step of the effect: real time keeps it fine.
@pytest.mark.realtime
def test_snow(session: Pilot) -> None:
    session.send("clear; printf '\\n\\n\\n\\n          %040d\\n' 0\n")
    session.wait_for("0" * 40, frame="raw")
    time.sleep(0.3)
    args = ["-s", session.session, "--cover", "--rate", "900", "--wind", "0"]
    args += ["--max-depth", "2"]
    status: list[int] = []
    # Long enough to see the melt: the line is cold 10 s after it is
    # written, the air clear 3 s later, the snow melted in 2 more.
    thread = threading.Thread(target=lambda: status.append(snow.main(args, seconds=20)))
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
        # (Column 0 is over the prompt: its stack may reach this row.)
        assert all(c not in DRIFT for c in host[3][1:10]), host[3]
        # The effect is tuned in place: the snow stays; a bad value is
        # refused by the effect, with its words.
        # (Once the line is cold: written lately, it is hot, and its snow
        # lasts only as long as more falls.)
        deadline = time.monotonic() + 8
        while "h" in session.frame("raw").heat_classes()[4]:
            assert time.monotonic() < deadline
            time.sleep(0.1)
        (sprite,) = session.sprites()
        flakes = Sprite(session, sprite["id"])
        flakes.tune(rate=0)
        assert lying(session)[3] >= 20
        with pytest.raises(PtyrcError, match="wind is out of range"):
            flakes.tune(wind=99)
        # The last flakes land: the snow of a line that is still stays.
        deadline = time.monotonic() + 6
        while "".join(session.dump()["host"][:2]).strip():
            assert time.monotonic() < deadline
            time.sleep(0.1)
        assert lying(session)[3] >= 20
        # The line changes, every cell of it (one written again the same
        # is not hot): its snow melts; the floor keeps its own. Seen
        # while the app runs: its end takes all the snow.
        session.send("clear; printf '\\n\\n\\n\\n          %040d\\n' 0 | tr 0 1\n")
        session.wait_for("1" * 40, frame="raw")
        deadline = time.monotonic() + 5
        while (rows := lying(session))[3] and time.monotonic() < deadline:
            time.sleep(0.1)
        assert thread.is_alive()
        assert rows[3] == 0 and rows[23] >= 40, rows
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
        seen = looks = 0
        while session_clock.monotonic() < deadline:
            assert sum(lying(session)) == 0
            host = "".join(session.dump()["host"])
            seen += any(c in host for c in "*+")
            looks += 1
            session_clock.sleep(0.1)
        # Most looks, however few a loaded machine leaves time for.
        assert seen > looks // 2
    finally:
        thread.join()


def test_all(monkeypatch: pytest.MonkeyPatch) -> None:
    """`-a`: `--cover`, `--hover`, and no limit unless one is given."""
    seen: list[tuple[bool, bool, int]] = []
    monkeypatch.setattr(
        snow._cli,
        "run",
        lambda app, args: seen.append((args.cover, args.hover, args.max_depth)) or 0,
    )
    for argv in (["-a"], ["--all", "--max-depth", "2"], [], ["--hover"]):
        assert snow.main(argv) == 0
    assert seen == [
        (True, True, 0),
        (True, True, 2),
        (False, False, snow.DEEP),
        (False, True, snow.DEEP),
    ]


@pytest.mark.realtime
def test_hover(hosted: tuple[Host, Pilot]) -> None:
    """The host mouse takes the snow it is over; it lies again after."""
    host, pilot = hosted
    args = ["-s", pilot.session, "-a", "-f", "raw", "--rate", "900", "--wind", "0"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(snow.main(args, seconds=20)))
    thread.start()

    def floor() -> str:
        return pilot.dump()["host"][23]

    try:
        host.wait_seen(b"\x1b[?1003h")
        deadline = time.monotonic() + 10
        while floor()[30:50] != "█" * 20:
            assert time.monotonic() < deadline, floor()
            time.sleep(0.1)
        # Three cells of the row; a flake may be on its way through.
        host.move(40, 23)
        deadline = time.monotonic() + 3
        while any(c in DRIFT for c in floor()[39:42]):
            assert time.monotonic() < deadline, floor()
            time.sleep(0.05)
        # Beside it the snow stays, less what slides in.
        assert all(c in DRIFT for c in floor()[36:39] + floor()[42:45]), floor()
        # The pointer rests: after a while it snows there again.
        deadline = time.monotonic() + 9
        while floor()[39:42] != "███":
            assert time.monotonic() < deadline, floor()
            time.sleep(0.1)
    finally:
        thread.join()
    assert status == [0]
