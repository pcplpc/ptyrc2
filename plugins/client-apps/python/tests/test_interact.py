# SPDX-License-Identifier: GPL-3.0-or-later
"""`Deck.interact`: the keys of a focused widget. The driver's side is
not needed: the events are given."""

from __future__ import annotations

import base64
import contextlib
from typing import Any

from ptyrc import PtyrcError
from ptyrc.widgets import Widgets
from ptyrc_apps import _widgets


class Script(Widgets):
    """What a deck needs of a pilot: the calls are kept, the events
    are those queued. One widget an id, from 1."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self.queue: list[tuple[str, Any]] = []
        self.topics: list[str] = []
        self.infos: dict[str, dict[str, Any]] = {}

    def call(self, method: str, **params: Any) -> Any:
        return self._request(method, params)

    def _request(self, method: str, params: dict[str, Any], then: Any = None) -> Any:
        self.calls.append((method, params))
        result: dict[str, Any] = {}
        if method == "widget.show":
            info = self.infos.setdefault(
                params["tag"],
                {
                    "id": len(self.infos) + 1,
                    "state": "placed",
                    "variant": "6x2",
                    "size": [6, 2],
                    "asks": None,
                    "focused": False,
                },
            )
            info["frame"] = params.get("frame")
            result = {"widget": dict(info)}
        elif method == "widget.ask":
            info = next(i for i in self.infos.values() if i["id"] == params["id"])
            info["asks"] = "quiet" if params.get("quiet") else "loud"
            result = {"widget": dict(info)}
        if then is not None:
            then(result)
        return result

    def subscribe(self, *topics: str, snapshot: bool = False) -> None:
        if "move" in topics:
            raise PtyrcError("bad-params", "no such topic: move")
        self.topics += topics

    def event(self, timeout: float | None = None) -> tuple[str, Any] | None:
        return self.queue.pop(0) if self.queue else ("exit", {"status": 0})

    def batch(self) -> Any:
        return contextlib.nullcontext()

    def named(self, method: str) -> list[dict[str, Any]]:
        return [params for name, params in self.calls if name == method]


def key(name: str | None, raw: bytes = b"", widget: int = 1) -> tuple[str, Any]:
    data = {"key": name, "bytes": base64.b64encode(raw).decode(), "widget": widget}
    return ("input", data)


def test_keys() -> None:
    keys = _widgets.Keys()

    def feed(name: str | None, raw: bytes = b"") -> Any:
        return keys.feed(key(name, raw)[1])

    assert feed("a", b"a") == ("key", "a")
    assert feed(None, b"\x1b[1;2A") == ("key", "shift+up")
    assert feed(None, b"\x1b[1;2B") == ("key", "shift+down")
    assert feed(None, b"\x1b[<35;1;1M") is None
    # A paste: what is between the marks, whole, enter a new line.
    assert feed(None, b"\x1b[200~") is None
    assert feed("a", b"a") is None and feed("enter", b"\r") is None
    assert feed(None, "é".encode()) is None
    assert feed(None, b"\x1b[201~") == ("paste", "a\né")
    # Cut by a focus lost: dropped.
    assert feed(None, b"\x1b[200~") is None and feed("x", b"x") is None
    keys.reset()
    assert feed("y", b"y") == ("key", "y")


def test_interact() -> None:
    pilot = Script()
    deck = _widgets.Deck(pilot)  # type: ignore[arg-type]
    seen: list[str] = []
    drawn: list[bool] = []

    def render(w: int, h: int) -> list[str]:
        drawn.append(widget.focused)
        return ["".join(seen)[-w:]] * h

    widget = deck.add("pad", [(6, 2)], lambda w, h: [""] * h)
    deck.widgets[0] = (widget, render)
    still = deck.add("still", [(6, 2)], lambda w, h: [""] * h)
    deck.interact(widget, lambda name: seen.append(name) or name != "z")
    assert pilot.named("widget.ask") == [{"id": 1, "quiet": True}]
    assert widget.asks == "quiet" and not widget.focused
    sets = lambda: len(pilot.named("widget.set"))

    # The key comes before the event of the focus: focused all the same.
    assert deck.handle(key("a", b"a"))
    assert seen == ["a"] and widget.focused and drawn == [True]
    # A handler that says False: not drawn again.
    n = sets()
    deck.handle(key("z", b"z"))
    assert seen == ["a", "z"] and sets() == n
    # Another widget's, one that does not interact, a key without a name.
    deck.handle(key("b", b"b", widget=2))
    deck.handle(key("b", b"b", widget=9))
    deck.handle(key(None, b"\x1b[<0;1;1M"))
    assert seen == ["a", "z"] and sets() == n and not still.focused
    deck.handle(key(None, b"\x1b[1;2A"))
    assert seen[-1] == "shift+up"
    # The focus lost: drawn again, not focused; told twice, drawn once.
    n = sets()
    deck.handle(("input", {"widget": 1, "released": "escape"}))
    assert not widget.focused and drawn[-1] is False and sets() == n + 1
    deck.handle(("widgets", {**widget.info, "focused": False}))
    assert sets() == n + 1
    # The focus given, by the event of the widget alone.
    deck.handle(("widgets", {**widget.info, "focused": True}))
    assert widget.focused and drawn[-1] is True and sets() == n + 2
    assert not deck.handle(("exit", {})) and not deck.handle(None)


def test_interact_paste_and_run() -> None:
    pilot = Script()
    deck = _widgets.Deck(pilot)  # type: ignore[arg-type]
    seen: list[str] = []
    widget = deck.add("pad", [(6, 2)], lambda w, h: [""] * h)
    deck.interact(widget, seen.append, quiet=False, escape="ctrl+g", paste=seen.append)
    assert pilot.named("widget.ask") == [{"id": 1, "escape": "ctrl+g"}]
    assert widget.asks == "loud"
    pilot.queue += [
        key("a", b"a"),
        key(None, b"\x1b[200~"),
        key("b", b"b"),
        key("enter", b"\r"),
        key("c", b"c"),
        key(None, b"\x1b[201~"),
        # A paste cut by the focus lost.
        key(None, b"\x1b[200~"),
        key("d", b"d"),
        ("widgets", {**widget.info, "focused": False}),
        key("e", b"e"),
    ]
    deck.run(period=3600.0)
    assert seen == ["a", "b\nc", "e"]


def test_interact_survives_a_move() -> None:
    pilot = Script()
    deck = _widgets.Deck(pilot)  # type: ignore[arg-type]
    seen: list[str] = []
    widget = deck.add("pad", [(6, 2)], lambda w, h: [""] * h)
    deck.interact(widget, seen.append)
    deck.move(3)
    # Shown again: the same widget, the ask kept by the driver.
    assert widget.info["frame"] == 3 and widget.asks == "quiet"
    assert len(pilot.named("widget.ask")) == 1
    deck.handle(key("a", b"a"))
    assert seen == ["a"]


def test_interact_closed() -> None:
    pilot = Script()
    deck = _widgets.Deck(pilot)  # type: ignore[arg-type]
    seen: list[str] = []
    closed: list[str] = []
    one = deck.add("one", [(6, 2)], lambda w, h: [""] * h)
    two = deck.add("two", [(6, 2)], lambda w, h: [""] * h)
    deck.interact(one, seen.append, close="f10", closed=lambda: closed.append("one"))
    deck.interact(two, seen.append, close=False)
    assert pilot.named("widget.ask") == [
        {"id": 1, "quiet": True, "close": "f10"},
        {"id": 2, "quiet": True, "close": False},
    ]
    deck.on_hover(one, lambda x, y: None)
    deck.handle(key("a", b"a"))
    # Closed by its key: out of the deck, its handlers too; not drawn.
    n = len(pilot.named("widget.set"))
    deck.handle(("input", {"widget": 1, "released": "close"}))
    assert closed == ["one"] and [w for w, _ in deck.widgets] == [two]
    assert not one.focused and one.asks is None
    assert len(pilot.named("widget.set")) == n
    deck.handle(key("b", b"b"))
    deck.handle(("widgets", {**one.info, "state": "gone"}))
    deck.draw()
    assert seen == ["a"] and all(p["id"] == 2 for p in pilot.named("widget.set")[n:])
    # The last one closed: `run` ends.
    pilot.queue += [
        key("c", b"c", widget=2),
        ("input", {"widget": 2, "released": "close"}),
        key("d", b"d", widget=2),
    ]
    deck.run(period=3600.0)
    assert seen == ["a", "c"] and deck.widgets == [] and len(pilot.queue) == 1
