# SPDX-License-Identifier: GPL-3.0-or-later
"""The app table: `app.*`, the topics `apps` and `move`."""

from __future__ import annotations

import os
import sys

import pytest
from ptyrc import Pilot, PtyrcError
from tests_support import until

#: A client app: one sprite, then it waits; `move` when asked.
APP = """
import sys
from ptyrc import Pilot
pilot = Pilot()
pilot.draw(["HELLO"], at=(2, 2), space="view", layer="front")
if "--moves" in sys.argv:
    pilot.subscribe("move")
for topic, data in pilot.events(timeout=None):
    if topic == "move":
        pilot.draw(["MOVED %d" % data["frame"]], at=(2, 4), space="view")
"""


def by_id(pilot: Pilot, app: int) -> dict:
    return next((a for a in pilot.apps() if a["id"] == app), {})


def test_app_lifecycle(shell: Pilot) -> None:
    argv = [sys.executable, "-c", APP]
    shell.subscribe("apps")
    me = next(a for a in shell.apps() if a["me"])
    assert me["kind"] == "client" and me["pid"] == os.getpid()

    app = shell.app_start(argv, name="hello")
    assert app["kind"] == "child" and app["name"] == "hello" and app["runs"] == 1
    assert app["state"] == "running" and not app["me"]
    until(lambda: "HELLO" in "\n".join(shell.shown()))
    info = by_id(shell, app["id"])
    assert info["sprites"] == 1 and info["frames"] == [0] and not info["moves"]

    assert shell.app_pause(app["id"])["state"] == "paused"
    until(lambda: "HELLO" not in "\n".join(shell.shown()))
    assert shell.app_resume(app["id"])["state"] == "running"
    until(lambda: "HELLO" in "\n".join(shell.shown()))

    # Not subscribed to `move`: it is restarted to move.
    with pytest.raises(PtyrcError) as err:
        shell.app_move(app["id"], "raw")
    assert err.value.code == "not-usable" and "move --restart" in err.value.hint
    shell.app_restart(app["id"], argv=[*argv, "--moves"])
    until(lambda: by_id(shell, app["id"])["moves"])
    assert by_id(shell, app["id"])["runs"] == 2
    assert shell.app_move(app["id"], "raw") == 0
    until(lambda: "MOVED 0" in "\n".join(shell.shown()))

    with pytest.raises(PtyrcError) as err:
        shell.app_pause(me["id"])
    assert err.value.code == "not-usable"
    with pytest.raises(PtyrcError) as err:
        shell.app_stop(99)
    assert err.value.code == "no-sprite" and "hello" in err.value.hint

    shell.app_stop(app["id"])
    until(lambda: by_id(shell, app["id"])["state"] == "exited")
    assert by_id(shell, app["id"])["status"] == 128 + 15
    shell.app_forget(app["id"])
    assert not by_id(shell, app["id"])
    events = shell.events(timeout=0.3)
    states = [
        d["state"] for topic, d in events if topic == "apps" and d["id"] == app["id"]
    ]
    assert states[-1] == "gone" and "paused" in states and "exited" in states
