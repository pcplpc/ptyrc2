# SPDX-License-Identifier: GPL-3.0-or-later
"""What the apps made of widgets share: a deck of them, kept fresh."""

from __future__ import annotations

import argparse
import time
from collections.abc import Callable, Mapping, Sequence
from typing import Any

from ptyrc import Pilot, Widget

__all__ = ["Deck", "options"]

Size = tuple[int, int]
#: `render(w, h)`: the rows of a variant, strings or rows of runs. The
#: size is the key: one variant a size in a tag.
Render = Callable[[int, int], Any]


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
    rendered.
    """

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
        pilot.subscribe("widgets", "exit")

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
        return widget

    def resize(
        self, widget: Widget, sizes: Sequence[Size] | Mapping[str, Size]
    ) -> None:
        """Other sizes for a widget of the deck: its tag is declared
        again, every variant rendered; the driver lays it out again."""
        render = next(r for w, r in self.widgets if w is widget)
        show = self._show[id(widget)]
        widget.info = self.pilot.widget(widget.tag, sizes, content=render, **show).info

    def draw(self, only: Widget | None = None) -> None:
        """Render the variant shown of each widget, or of `only`."""
        with self.pilot.batch():
            for widget, render in self.widgets:
                size = widget.size
                if size is not None and only in (None, widget):
                    widget.set(render(*size))

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
        end = None if seconds is None else time.monotonic() + seconds
        due = time.monotonic()
        while True:
            now = time.monotonic()
            if end is not None and now >= end:
                return
            if now >= due:
                if tick is not None:
                    tick()
                self.draw()
                now = time.monotonic()
                due = now + (period - time.time() % period if align else period)
            wait = due - now if end is None else min(due, end) - now
            event = self.pilot.event(timeout=max(0.0, wait))
            if event is None:
                continue
            if event[0] == "exit":
                return
            if event[0] != "widgets":
                continue
            shown = {id(w): w.variant for w, _ in self.widgets}
            widget = self.take(event[1])
            # Moved only: what it shows stands.
            if widget is not None and widget.variant != shown[id(widget)]:
                self.draw(widget)
