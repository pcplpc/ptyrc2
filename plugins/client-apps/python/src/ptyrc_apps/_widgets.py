# SPDX-License-Identifier: GPL-3.0-or-later
"""What the apps made of widgets share: a deck of them, kept fresh."""

from __future__ import annotations

import argparse
import time
from collections.abc import Callable, Mapping, Sequence
from typing import Any

from ptyrc import Pilot, PtyrcError, Widget, clock

from . import _cli

__all__ = ["Deck", "options"]

Size = tuple[int, int]
#: What of a widget's info a hover stands on.
_PLACE = ("sprite", "variant", "rect")
#: `render(w, h)`: the rows of a variant, strings or rows of runs. The
#: size is the key: one variant a size in a tag.
Render = Callable[[int, int], Any]
#: `move(x, y)`: the host mouse over the cell `(x, y)` of the inner box.
#: Returning False: nothing changed, the widget is not drawn again.
Move = Callable[[int, int], Any]
Leave = Callable[[], Any]


def options(
    out: argparse.ArgumentParser, border: str = "plain", prefer: str = "any"
) -> None:
    """The options common to the apps made of widgets."""
    out.add_argument(
        "--border",
        default=border,
        help=f"border preset: plain, round, heavy, double, ascii, none, "
        f"painting... [default: {border}]",
    )
    out.add_argument(
        "--prefer",
        default=prefer,
        help="top-left, top, top-right, left, center, right, bottom-left, "
        f"bottom, bottom-right, any [default: {prefer}]",
    )
    out.add_argument("--front", action="store_true", help="over the app's text")


class Deck:
    """The widgets of an app. Each has a `render(w, h)`; the driver
    chooses the variant and the place, the deck draws what is shown.

        deck = Deck(pilot, args)
        deck.add("clock", [(8, 1), (5, 1)], lambda w, h: [now()[:w]])
        deck.run(period=1.0)

    Driven by the topic `widgets`: a variant just put is drawn at once,
    the others once a period, in one batch. A hidden widget is not
    rendered. On the topic `move` its tags are shown again on the frame
    named (`move`). `on_hover`: the host mouse over a widget.
    """

    #: Session seconds without a move after which a hover is over: the
    #: host tells no move of a mouse that left its window. None: never.
    hover_timeout: float | None = 1.5

    def __init__(self, pilot: Pilot, args: argparse.Namespace | None = None) -> None:
        self.pilot = pilot
        self.shared: dict[str, Any] = {}
        if args is not None:
            self.shared = {
                "border": getattr(args, "border", None),
                "prefer": getattr(args, "prefer", None),
                "frame": getattr(args, "frame", None),
                "layer": "front" if getattr(args, "front", False) else None,
            }
        self.widgets: list[tuple[Widget, Render]] = []
        self._show: dict[int, dict[str, Any]] = {}
        self._sizes: dict[int, Sequence[Size] | Mapping[str, Size]] = {}
        self._hover: dict[int, tuple[Move, Leave | None]] = {}
        self._over: Widget | None = None
        self._over_until: float | None = None
        pilot.subscribe("widgets", "exit")
        _cli.movable(pilot)

    def add(
        self,
        tag: str,
        sizes: Sequence[Size] | Mapping[str, Size],
        render: Render,
        **show: Any,
    ) -> Widget:
        """Declare and show `tag`; `show` as for `Pilot.widget`, over
        the options of the deck. Every variant is rendered once: the
        one the driver puts next is never blank."""
        show = {**self.shared, **show}
        widget = self.pilot.widget(tag, sizes, content=render, **show)
        self.widgets.append((widget, render))
        self._show[id(widget)] = show
        self._sizes[id(widget)] = sizes
        return widget

    def resize(
        self, widget: Widget, sizes: Sequence[Size] | Mapping[str, Size]
    ) -> None:
        """Other sizes for a widget of the deck: its tag is declared
        again, every variant rendered; the driver lays it out again."""
        render = next(r for w, r in self.widgets if w is widget)
        show = self._show[id(widget)]
        self._sizes[id(widget)] = sizes
        widget.info = self.pilot.widget(widget.tag, sizes, content=render, **show).info

    def move(self, frame: int | str) -> None:
        """Show every tag again on `frame`: the driver lays them out
        there. What they show is rendered again."""
        self.shared["frame"] = frame
        for widget, _ in self.widgets:
            self._show[id(widget)]["frame"] = frame
            self.resize(widget, self._sizes[id(widget)])

    def draw(self, only: Widget | None = None) -> None:
        """Render the variant shown of each widget, or of `only`."""
        with self.pilot.batch():
            for widget, render in self.widgets:
                size = widget.size
                if size is not None and only in (None, widget):
                    widget.set(render(*size))

    def on_hover(self, widget: Widget, move: Move, leave: Leave | None = None) -> None:
        """Follow the host mouse over `widget`, a widget of the deck.

        `move(x, y)`: the mouse, no button down, came on the cell
        `(x, y)` of the inner box, or went from one to another; a cell
        of the border is not in the box. `leave()`, once after the last
        `move`: the mouse went out of the box (on the border, another
        sprite or none), `hover_timeout` passed without a move, or the
        widget was moved, changed or hidden. After either the widget is
        drawn again, unless the handler returned False.

        The first handler subscribes to `hover`: the driver then asks
        the host for the moves. A driver without the topic refuses:
        the handlers are never called."""
        if not self._hover:
            try:
                self.pilot.subscribe("hover")
            except PtyrcError as err:
                if err.code != "bad-params":
                    raise
        self._hover[id(widget)] = (move, leave)

    def hover(self, data: dict[str, Any]) -> None:
        """Give the data of a `hover` event to the handlers."""
        sprite = data.get("sprite")
        found = None
        for widget, _ in self.widgets:
            if id(widget) not in self._hover or sprite is None:
                continue
            rect, size = widget.rect, widget.size
            if widget.info.get("sprite") != sprite or rect is None or size is None:
                continue
            # The border is as thick on every side.
            x = data["x"] - rect[0] - (rect[2] - size[0]) // 2
            y = data["y"] - rect[1] - (rect[3] - size[1]) // 2
            if 0 <= x < size[0] and 0 <= y < size[1]:
                found = (widget, x, y)
        if found is None or found[0] is not self._over:
            self.leave()
        if found is None:
            return
        widget, x, y = found
        self._over = widget
        if self.hover_timeout is not None:
            self._over_until = clock.monotonic() + self.hover_timeout
        if self._hover[id(widget)][0](x, y) is not False:
            self.draw(widget)

    def leave(self) -> None:
        """End the hover under way, if any."""
        widget, self._over, self._over_until = self._over, None, None
        if widget is None:
            return
        leave = self._hover[id(widget)][1]
        if leave is not None and leave() is not False:
            self.draw(widget)

    def take(self, data: dict[str, Any]) -> Widget | None:
        """Give the data of a `widgets` event to the widget it tells
        of; that widget, if it is of the deck."""
        return next((w for w, _ in self.widgets if w.update(data)), None)

    def run(
        self,
        period: float = 1.0,
        seconds: float | None = None,
        tick: Callable[[], None] | None = None,
        align: bool = False,
    ) -> None:
        """Keep the deck fresh until the session ends, or for `seconds`.
        `tick`: the app's work before each periodic draw (read its
        data). `align`: draw on the multiples of `period` of the wall
        clock, as a clock does."""
        end = None if seconds is None else clock.monotonic() + seconds
        due = clock.monotonic()
        while True:
            now = clock.monotonic()
            if end is not None and now >= end:
                return
            if now >= due:
                if tick is not None:
                    tick()
                self.draw()
                now = clock.monotonic()
                # The wall clock is real time: one of its seconds is `rate()` here.
                wall = (period - time.time() % period) * clock.rate()
                due = now + (wall if align else period)
            if self._over_until is not None and now >= self._over_until:
                self.leave()
            wake = min(t for t in (due, end, self._over_until) if t is not None)
            event = self.pilot.event(timeout=max(0.0, wake - now))
            if event is None:
                continue
            if event[0] == "hover":
                self.hover(event[1])
                continue
            if event[0] == "exit":
                return
            if event[0] == "move":
                self.move(event[1]["frame"])
                continue
            if event[0] != "widgets":
                continue
            shown = {id(w): w.variant for w, _ in self.widgets}
            over = {} if self._over is None else self._over.info
            widget = self.take(event[1])
            # The widget under the mouse moved, changed or went.
            if widget is self._over is not None and any(
                over.get(k) != widget.info.get(k) for k in _PLACE
            ):
                self.leave()
            # Moved only: what it shows stands.
            if widget is not None and widget.variant != shown[id(widget)]:
                self.draw(widget)
