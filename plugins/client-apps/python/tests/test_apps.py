# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app runs against a headless driver and cleans up after itself."""

from __future__ import annotations

import re
import threading
import time
from collections.abc import Callable
from datetime import datetime

import pytest
from ptyrc import Pilot
from ptyrc_apps import APPS, clock, confetti
from ptyrc_apps.__main__ import main


def shown(pilot: Pilot) -> str:
    return "\n".join(pilot.dump()["host"])


def watch(pilot: Pilot, run: Callable[[], int], seen: Callable[[str], bool]) -> None:
    """Run an app in a thread; `seen` must hold on the host at some point,
    and the host must be clean once the app has returned."""
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(run()))
    thread.start()
    ok = False
    while thread.is_alive():
        ok = ok or seen(shown(pilot))
        time.sleep(0.02)
    thread.join()
    assert status == [0]
    assert ok, "the app never showed"
    deadline = time.monotonic() + 3
    while pilot.sprites() and time.monotonic() < deadline:
        time.sleep(0.02)
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()


def test_confetti(session: Pilot) -> None:
    args = ["-s", session.session, "--duration", "0.6", "--bursts", "2", "--sparkle"]
    watch(
        session,
        lambda: confetti.main([*args, "--seed", "3"]),
        lambda host: sum(host.count(c) for c in "❋✦*·") >= 10,
    )


def test_clock(session: Pilot) -> None:
    def seen(host: str) -> bool:
        top = host.split("\n")[0]
        return bool(re.search(r" \d\d:\d\d:\d\d$", top)) and len(top) == 79

    watch(
        session, lambda: clock.main(["-s", session.session, "--seconds", "1.2"]), seen
    )


def test_clock_big_bottom_left(session: Pilot) -> None:
    def seen(host: str) -> bool:
        bottom = host.split("\n")[-3:]
        return any(c in "".join(bottom) for c in "█▀▄") and bottom[-1][:1] in "█▀▄"

    args = [
        "-s",
        session.session,
        "--seconds",
        "1.2",
        "--big",
        "--prefer",
        "bottom-left",
    ]
    watch(session, lambda: clock.main(args), seen)


def test_clock_face() -> None:
    row = clock.face(datetime(2026, 11, 12, 9, 5, 7))[0]
    assert "".join(run["t"] for run in row) == " 09:05:07 "


def test_dispatch(session: Pilot, capsys: pytest.CaptureFixture[str]) -> None:
    assert main([]) == 2
    out = capsys.readouterr().out
    assert all(name in out for name in APPS)
    assert main(["nope"]) == 2
    assert main(["confetti", "-s", session.session, "--duration", "0.1"]) == 0
    assert main(["confetti", "-s", "no-such-session"]) == 1
    assert "ptyrc-app: gone" in capsys.readouterr().err
