# SPDX-License-Identifier: GPL-3.0-or-later
"""The topic `move`: an app changes frame while it runs. The driver's
side is not needed: the events are given."""

from __future__ import annotations

import argparse
import contextlib
from typing import Any

import pytest
from ptyrc import Frame, PtyrcError, Widget
from ptyrc_apps import _cli, _widgets


class Fake:
    """What `follow` and `Deck` ask of a pilot; the events it is fed."""

    def __init__(self, events: list[tuple[str, Any]], old: bool = False) -> None:
        self.events = events
        self.old = old
        self.topics: list[str] = []
        self.shown: list[dict[str, Any]] = []

    def subscribe(self, *topics: str, snapshot: bool = False) -> None:
        if self.old and "move" in topics:
            raise PtyrcError("bad-params", "no such topic: move")
        self.topics += topics

    def event(self, timeout: float | None = None) -> tuple[str, Any] | None:
        return self.events.pop(0) if self.events else ("exit", {"status": 0})

    def frame(self, sel: int | str) -> Frame:
        if sel == 9:
            raise PtyrcError("no-frame", "no such frame: 9")
        return Frame(self, {"id": sel, "kind": "pane", "state": "stable"})  # type: ignore[arg-type]

    def widget(self, tag: str, sizes: Any, **show: Any) -> Widget:
        self.shown.append({"tag": tag, "sizes": sizes, **show})
        return Widget(self, tag, {"id": len(self.shown), "frame": show.get("frame")})  # type: ignore[arg-type]

    def batch(self) -> Any:
        # No variant is shown: nothing is sent in it.
        return contextlib.nullcontext()


class Sprite:
    def __init__(self) -> None:
        self.frames: list[int] = []

    def update(self, **patch: Any) -> None:
        self.frames.append(patch["frame"])


def test_follow_moves_its_sprite() -> None:
    pilot = Fake(
        [("move", {"frame": 4}), ("move", {"frame": 9}), ("move", {"frame": 2})]
    )
    sprite = Sprite()
    frame = Frame(pilot, {"id": 1, "kind": "pane", "state": "stable"})  # type: ignore[arg-type]
    _cli.follow(pilot, frame, sprite)  # type: ignore[arg-type]
    # A frame that is gone since: the sprite stays.
    assert sprite.frames == [4, 2]
    assert pilot.topics == ["frames", "exit", "move"]


def test_an_older_driver_has_no_such_topic() -> None:
    pilot = Fake([], old=True)
    assert not _cli.movable(pilot)  # type: ignore[arg-type]
    frame = Frame(pilot, {"id": 1, "kind": "pane", "state": "stable"})  # type: ignore[arg-type]
    _cli.follow(pilot, frame, Sprite())  # type: ignore[arg-type]
    assert pilot.topics == ["frames", "exit"]

    class Refused(Fake):
        def subscribe(self, *topics: str, snapshot: bool = False) -> None:
            raise PtyrcError("gone", "closed")

    with pytest.raises(PtyrcError):
        _cli.movable(Refused([]))  # type: ignore[arg-type]


def test_deck_shows_its_tags_on_the_frame() -> None:
    pilot = Fake([("move", {"frame": 3})])
    args = argparse.Namespace(border="round", prefer="top", frame="raw", front=False)
    deck = _widgets.Deck(pilot, args)  # type: ignore[arg-type]
    assert pilot.topics == ["widgets", "exit", "move"]

    def render(w: int, h: int) -> list[str]:
        return ["x" * w] * h

    clock = deck.add("clock", [(5, 1), (8, 1)], render)
    load = deck.add("load", {"small": (9, 2)}, render, prefer="bottom")
    deck.resize(load, {"small": (9, 3)})
    del pilot.shown[:]
    deck.run(period=3600.0, tick=lambda: None)
    # Shown again, as they were but for the frame; the sizes last given.
    assert [(s["tag"], s["frame"], s["prefer"]) for s in pilot.shown] == [
        ("clock", 3, "top"),
        ("load", 3, "bottom"),
    ]
    assert pilot.shown[1]["sizes"] == {"small": (9, 3)}
    assert pilot.shown[0]["content"] is render
    assert clock.info["frame"] == 3 and deck.shared["frame"] == 3
