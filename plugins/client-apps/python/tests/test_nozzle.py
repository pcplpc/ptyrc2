# SPDX-License-Identifier: GPL-3.0-or-later
"""`_cli.Nozzle`: the host mouse told to an effect (`sand --hover`,
`sand --bomb`)."""

from __future__ import annotations

import threading
import time
from typing import TYPE_CHECKING, Any

import pytest
from ptyrc import Pilot, Sprite, clock
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


def test_bomb(monkeypatch: pytest.MonkeyPatch) -> None:
    now = [10.0]
    monkeypatch.setattr(clock, "monotonic", lambda: now[0])
    wire = Wire()
    nozzle = _cli.Nozzle(wire, Pane(3, (40, 10)), Tape(wire, 5), False, True)  # type: ignore[arg-type]
    assert wire.calls[0] == ("subscribe", "mouse")
    both = _cli.Nozzle(wire, Pane(3, (40, 10)), Tape(wire, 5), bomb=True)  # type: ignore[arg-type]
    assert ("subscribe", "hover", "mouse") in wire.calls and both.pad is not None
    wire.calls.clear()

    def report(
        action: str,
        at: list[int] | None,
        sprite: int = 9,
        xy: tuple[int, int] | None = None,
    ) -> None:
        # The sprite is at (10, 5) of the screen.
        x, y = xy or (at[0] + 10, at[1] + 5) if xy or at else (None, None)
        data = {"sprite": sprite, "action": action, "at": at, "x": x, "y": y}
        nozzle.mouse(data)

    # A press tells nothing; its release, the cell pressed and how long.
    report("press", [4, 2])
    now[0] += 0.25
    assert wire.calls == []
    report("release", [4, 2])
    assert wire.calls == [("tune", 5, {"bomb": [4, 2, 250]})]
    wire.calls.clear()
    # Released on another cell: the cells from the press to it; off the
    # sprite, by the screen cell; one that came back: no aim.
    report("press", [4, 2])
    report("drag", [6, 2])
    report("release", [1, 3])
    report("press", [4, 2])
    report("drag", [6, 2])
    report("release", None, xy=(60, 2))
    report("press", [4, 2])
    report("drag", [6, 2])
    report("release", [4, 2])
    # No screen cell at the release: the last one told.
    report("press", [4, 2])
    report("drag", [6, 1])
    nozzle.mouse({"sprite": 9, "action": "release", "at": None})
    assert wire.calls == [
        ("tune", 5, {"bomb": [4, 2, 0, -3, 1]}),
        ("tune", 5, {"bomb": [4, 2, 0, 46, -5]}),
        ("tune", 5, {"bomb": [4, 2, 0]}),
        ("tune", 5, {"bomb": [4, 2, 0, 2, -1]}),
    ]
    wire.calls.clear()
    # A release alone, a wheel, a click on another sprite: nothing.
    report("release", [4, 2])
    report("wheel", [4, 2])
    report("press", [1, 1], sprite=7)
    report("release", [1, 1], sprite=7)
    assert wire.calls == []


def test_all(monkeypatch: pytest.MonkeyPatch) -> None:
    """`-a`: `--cursor`, `--bomb` and `--now`."""
    seen: list[tuple[bool, ...]] = []
    monkeypatch.setattr(
        sand._cli,
        "run",
        lambda app, args: (
            seen.append((args.cursor, args.hover, args.bomb, args.now)) or 0
        ),
    )
    for argv in (["-a"], ["--all", "--hover"], [], ["--bomb"], ["--hover"]):
        assert sand.main(argv) == 0
    assert seen == [
        (True, False, True, True),
        (True, True, True, True),
        (False, False, False, False),
        (False, False, True, False),
        (False, True, False, False),
    ]


@pytest.mark.realtime
def test_sand_bomb(hosted: tuple[Host, Pilot]) -> None:
    """A click on the pile throws its sand about; it lands."""
    host, pilot = hosted
    args = ["-s", pilot.session, "-f", "raw", "--now", "--rate", "400", "--bomb"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(sand.main(args, seconds=16)))
    thread.start()

    def sand_at() -> set[tuple[int, int]]:
        rows = pilot.dump()["host"]
        return {(x, y) for y, row in enumerate(rows) for x in grains(row)}

    try:
        host.wait_seen(b"\x1b[?1002h")
        deadline = time.monotonic() + 8
        while len(pile := grains(pilot.dump()["host"][23])) < 12:
            assert time.monotonic() < deadline, pile
            time.sleep(0.1)
        (effect,) = (s for s in pilot.sprites() if s["kind"] == "effect:sand")
        Sprite(pilot, effect["id"]).tune(rate=0)
        time.sleep(1.5)  # the stream lands
        top = min(y for _, y in sand_at())
        pile = grains(pilot.dump()["host"][23])
        x = pile[len(pile) // 2]
        # Nothing until the release: no sand over the pile.
        host.press(x, 23)
        time.sleep(0.6)
        assert min(y for _, y in sand_at()) >= top
        host.press(x, 23, down=False)
        # Sand in the air, over the pile; then on the floor, wider.
        deadline = time.monotonic() + 3
        while min((y for _, y in sand_at()), default=top) >= top - 2:
            assert time.monotonic() < deadline, pilot.dump()["host"]
            time.sleep(0.02)
        deadline = time.monotonic() + 6
        while len(grains(pilot.dump()["host"][23])) <= len(pile) + 4:
            assert time.monotonic() < deadline, pilot.dump()["host"]
            time.sleep(0.1)
    finally:
        thread.join()
    assert status == [0]


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
