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
        events = []
        # The session ends: the last event, then a closed connection.
        with pytest.raises(PtyrcError) as err:
            events.extend(pilot.events("exit", timeout=5))
        assert err.value.code == "gone"
        assert events[-1] == ("exit", {"status": 3})
        with pytest.raises(PtyrcError):
            pilot.event(timeout=1)
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


def test_the_socket_is_this_users_alone() -> None:
    with Pilot.spawn(["cat"], name="life-m"):
        assert (runtime_dir() / "life-m.sock").stat().st_mode & 0o777 == 0o600
        assert runtime_dir().stat().st_mode & 0o777 == 0o700


def test_an_open_socket_directory_is_refused(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("XDG_RUNTIME_DIR", str(tmp_path))
    assert live_sessions() == []  # no directory: no session
    (tmp_path / "ptyrc").mkdir(mode=0o755)
    for call in (live_sessions, lambda: socket_path("x"), Pilot):
        with pytest.raises(PtyrcError) as err:
            call()
        assert err.value.code == "gone"
        assert "open to other users" in err.value.message
    # A symlink to a good directory is no directory.
    (tmp_path / "ptyrc").rmdir()
    (tmp_path / "real").mkdir(mode=0o700)
    (tmp_path / "ptyrc").symlink_to(tmp_path / "real")
    with pytest.raises(PtyrcError, match="not a directory of ours"):
        live_sessions()
