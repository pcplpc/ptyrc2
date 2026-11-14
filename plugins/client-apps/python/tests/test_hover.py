# SPDX-License-Identifier: GPL-3.0-or-later
"""`Deck.on_hover`: the host mouse over a widget."""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

from ptyrc import Pilot, clock
from ptyrc_apps import _widgets

if TYPE_CHECKING:
    from conftest import Host


class Seen:
    """Handlers that note what they are told."""

    def __init__(self) -> None:
        self.calls: list[object] = []

    def move(self, x: int, y: int) -> None:
        self.calls.append((x, y))

    def leave(self) -> None:
        self.calls.append("leave")


def settle(deck: _widgets.Deck, *widgets) -> None:
    """Run the deck until the driver placed `widgets`."""
    deadline = time.monotonic() + 5
    while not all(w.placed for w in widgets):
        assert time.monotonic() < deadline, [w.info for w in widgets]
        deck.run(seconds=0.5)
    deck.run(seconds=0.5)


def inner(widget) -> tuple[int, int]:
    """The screen cell of the first cell of the inner box."""
    rect, size = widget.rect, widget.size
    return (
        rect[0] + (rect[2] - size[0]) // 2,
        rect[1] + (rect[3] - size[1]) // 2,
    )


def test_no_handler_asks_nothing(hosted: tuple[Host, Pilot]) -> None:
    host, pilot = hosted
    deck = _widgets.Deck(pilot)
    deck.add("still", [(6, 2)], lambda w, h: ["." * w] * h)
    deck.run(seconds=0.5)
    assert b"\x1b[?1003h" not in host.seen
    assert b"\x1b[?1002h" not in host.seen


def test_the_host_mouse_over_a_widget(hosted: tuple[Host, Pilot]) -> None:
    host, pilot = hosted
    deck = _widgets.Deck(pilot)
    seen, drawn = Seen(), []

    def render(w: int, h: int) -> list[str]:
        drawn.append((w, h))
        return ["." * w] * h

    widget = deck.add("hovered", [(6, 2)], render, border="round")
    other = deck.add("other", [(4, 1)], lambda w, h: ["o" * w], border="round")
    deck.on_hover(widget, seen.move, seen.leave)
    host.wait_seen(b"\x1b[?1003h\x1b[?1006h")
    settle(deck, widget, other)
    x, y = inner(widget)
    assert (x, y) == (widget.rect[0] + 1, widget.rect[1] + 1)

    # Inner cells; a cell of the border is out of the box.
    drawn.clear()
    host.move(x, y)
    host.move(x + 5, y + 1)
    host.move(x - 1, y)
    host.move(x + 2, y)
    deck.run(seconds=1.0)
    assert seen.calls == [(0, 0), (5, 1), "leave", (2, 0)]
    # Drawn again after each.
    assert drawn.count((6, 2)) >= 4

    # No move for `hover_timeout`: it is over, once.
    deck.run(seconds=deck.hover_timeout + 1.0)
    assert seen.calls[4:] == ["leave"]

    # Onto another widget, with no handler; then off every sprite.
    ox, oy = inner(other)
    host.move(x, y)
    host.move(ox, oy)
    host.move(x + 1, y)
    free = next(
        (cx, cy)
        for cy in range(24)
        for cx in range(80)
        if not any(
            r[0] <= cx < r[0] + r[2] and r[1] <= cy < r[1] + r[3]
            for r in (widget.rect, other.rect)
        )
    )
    host.move(*free)
    deck.run(seconds=1.0)
    assert seen.calls[5:] == [(0, 0), "leave", (1, 0), "leave"]

    # The app asked for no move: it was sent none.
    assert "[<35" not in "\n".join(pilot.text())

    # The deck gone, the host has the app's modes again.
    mark = len(host.seen)
    pilot.close()
    host.wait_seen(b"\x1b[?1003l", mark)


def test_hover_data_without_a_host(session: Pilot) -> None:
    """The handlers, given the data of events: no pty needed."""
    deck = _widgets.Deck(session)
    seen = Seen()
    quiet = Seen()
    drawn: list[str] = []

    def still(x: int, y: int) -> bool:
        quiet.move(x, y)
        return False

    a = deck.add("a", [(6, 2)], lambda w, h: drawn.append("a") or ["a" * w] * h)
    b = deck.add("b", [(4, 1)], lambda w, h: drawn.append("b") or ["b" * w])
    deck.on_hover(a, seen.move, seen.leave)
    deck.on_hover(b, still)
    settle(deck, a, b)

    def at(widget, dx: int, dy: int) -> dict[str, object]:
        x, y = inner(widget)
        return {"sprite": widget.info["sprite"], "x": x + dx, "y": y + dy}

    drawn.clear()
    deck.hover(at(a, 3, 1))
    assert (seen.calls, drawn) == ([(3, 1)], ["a"])
    # Another widget of the deck: the first is left; False: not drawn.
    deck.hover(at(b, 0, 0))
    assert seen.calls == [(3, 1), "leave"]
    assert quiet.calls == [(0, 0)] and drawn == ["a", "a"]
    # No sprite: a leave, none to call for `b`; nothing twice.
    deck.hover({"sprite": None, "x": 0, "y": 0})
    deck.hover({"sprite": None, "x": 0, "y": 0})
    deck.leave()
    assert seen.calls == [(3, 1), "leave"] and quiet.calls == [(0, 0)]
    # A sprite that is no widget of the deck.
    deck.hover(at(a, 0, 0))
    deck.hover({"sprite": 10**6, "x": 0, "y": 0})
    assert seen.calls[2:] == [(0, 0), "leave"]

    # The timeout runs on the session clock, in `run`.
    deck.hover(at(a, 1, 1))
    begin = clock.monotonic()
    deck.run(seconds=deck.hover_timeout + 1.0)
    assert seen.calls[4:] == [(1, 1), "leave"]
    assert clock.monotonic() - begin >= deck.hover_timeout
    # Never, when None.
    deck.hover_timeout = None
    deck.hover(at(a, 1, 1))
    deck.run(seconds=2.0)
    assert seen.calls[6:] == [(1, 1)]
