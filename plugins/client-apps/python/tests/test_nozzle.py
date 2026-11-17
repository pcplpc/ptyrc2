# SPDX-License-Identifier: GPL-3.0-or-later
"""`_cli.Nozzle`: the host mouse told to an effect (`sand --hover`)."""

from __future__ import annotations

import threading
import time
from typing import TYPE_CHECKING, Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import _cli, sand

if TYPE_CHECKING:
    from conftest import Host


def grains(row: str) -> list[int]:
    """The columns of `row` that hold sand."""
    return [x for x, c in enumerate(row) if "⠁" <= c <= "⣿"]


class Wire:
    """What a `Nozzle` asks of a pilot."""

    def __init__(self) -> None:
        self.calls: list[tuple[Any, ...]] = []

    def subscribe(self, *topics: str) -> None:
        self.calls.append(("subscribe", *topics))

    def draw(self, rows: list[str], **kw: Any) -> Tape:
        self.calls.append(("draw", len(rows[0]), len(rows), kw))
        return Tape(self, 9)


class Tape:
    """A sprite that notes what it is told."""

    def __init__(self, wire: Wire, id: int) -> None:
        self.wire, self.id = wire, id

    def update(self, **patch: Any) -> None:
        self.wire.calls.append(("update", self.id, patch))

    def set(self, rows: list[str]) -> None:
        self.wire.calls.append(("set", self.id, len(rows[0]), len(rows)))

    def tune(self, **params: Any) -> None:
        self.wire.calls.append(("tune", self.id, params))


class Pane:
    def __init__(self, id: int, size: tuple[int, int]) -> None:
        self.id, self.size = id, size


def test_nozzle() -> None:
    wire = Wire()
    nozzle = _cli.Nozzle(wire, Pane(3, (40, 10)), Tape(wire, 5))  # type: ignore[arg-type]
    where = {"frame": 3, "at": (0, 0), "space": "view", "z": -1}
    assert wire.calls == [("subscribe", "hover"), ("draw", 40, 10, where)]
    wire.calls.clear()
    # On its sprite: the cell, once; on another, or on none: no pointer.
    nozzle.hover({"sprite": 9, "at": [4, 2]})
    nozzle.hover({"sprite": 9, "at": [4, 2]})
    nozzle.hover({"sprite": 7, "at": [1, 1]})
    nozzle.hover({"sprite": None, "at": None})
    nozzle.hover({"sprite": 9, "at": [5, 2]})
    assert wire.calls == [
        ("tune", 5, {"hover": [4, 2]}),
        ("tune", 5, {"hover": None}),
        ("tune", 5, {"hover": [5, 2]}),
    ]
    wire.calls.clear()
    # The same frame: nothing; another size, another frame: its sprite
    # is as large, and there.
    nozzle.place(Pane(3, (40, 10)))  # type: ignore[arg-type]
    nozzle.place(Pane(3, (30, 8)))  # type: ignore[arg-type]
    nozzle.place(Pane(4, (30, 8)))  # type: ignore[arg-type]
    assert wire.calls == [
        ("set", 9, 30, 8),
        ("update", 9, {"frame": 4}),
        ("set", 9, 30, 8),
    ]


@pytest.mark.realtime
def test_sand_hover(hosted: tuple[Host, Pilot]) -> None:
    """The host mouse takes the sand it is over, and what pours there."""
    host, pilot = hosted
    args = ["-s", pilot.session, "-f", "raw", "--now", "--rate", "400", "--hover"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(sand.main(args, seconds=14)))
    thread.start()

    def floor() -> str:
        return pilot.dump()["host"][23]

    try:
        host.wait_seen(b"\x1b[?1003h")
        deadline = time.monotonic() + 8
        while len(pile := grains(floor())) < 7:
            assert time.monotonic() < deadline, floor()
            time.sleep(0.1)
        x = pile[len(pile) // 2]
        host.move(x, 23)
        deadline = time.monotonic() + 3
        while set(grains(floor())) & {x - 1, x, x + 1}:
            assert time.monotonic() < deadline, floor()
            time.sleep(0.05)
        # The rest of the pile stays, less what slides in.
        assert grains(floor())
    finally:
        thread.join()
    assert status == [0]
