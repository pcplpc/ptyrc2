# SPDX-License-Identifier: GPL-3.0-or-later
"""Sessions: spawn, connect, close."""

from __future__ import annotations

import os
import time

import pytest

from ptyrc import Pilot, PtyrcError, live_sessions, runtime_dir, socket_path
from tests_support import pid_alive


def test_spawn_connect_close() -> None:
    pilot = Pilot.spawn(["cat"], name="life-a")
    driver = pilot.hello["driver_pid"]
    app = pilot.hello["pid"]
    assert "life-a" in live_sessions()
    assert socket_path("life-a") == runtime_dir() / "life-a.sock"

    # A second client, by name; the only session is also the default.
    with Pilot("life-a") as other:
        assert other.session == "life-a"
        other.send("shared\n")
    with Pilot() as default:
        default.wait_for("shared")

    pilot.close()
    assert not pilot.connected
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline and (pid_alive(driver) or pid_alive(app)):
        time.sleep(0.02)
    assert not pid_alive(driver) and not pid_alive(app)
    assert "life-a" not in live_sessions()
    assert not (runtime_dir() / "life-a.sock").exists()
    with pytest.raises(PtyrcError) as err:
        pilot.text()
    assert err.value.code == "gone"


def test_app_exit_ends_session() -> None:
    with Pilot.spawn(["sh", "-c", "sleep 0.2; exit 3"], name="life-b") as pilot:
        events = list(pilot.events("exit", timeout=5))
        assert events[-1] == ("exit", {"status": 3})
    assert "life-b" not in live_sessions()


def test_no_session() -> None:
    assert live_sessions() == []
    with pytest.raises(PtyrcError) as err:
        Pilot()
    assert err.value.code == "gone"
    with pytest.raises(PtyrcError):
        Pilot("missing")
    with pytest.raises(PtyrcError):
        Pilot.spawn(["/no/such/program"], timeout=3).close()


def test_environment_names_the_session() -> None:
    with Pilot.spawn(["sh"], name="life-c") as pilot:
        pilot.send("echo $PTYRC_SESSION\n")
        pilot.wait_for(r"^life-c *$", regex=True)
        os.environ["PTYRC_SESSION"] = "life-c"
        try:
            with Pilot() as by_env:
                assert by_env.session == "life-c"
        finally:
            del os.environ["PTYRC_SESSION"]
