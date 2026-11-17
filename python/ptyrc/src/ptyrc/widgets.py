# SPDX-License-Identifier: GPL-3.0-or-later
"""Widgets of the pilot: small windows the driver places and sizes."""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from typing import TYPE_CHECKING, Any

from ._wire import PtyrcError

if TYPE_CHECKING:
    from .pilot import Pilot

__all__ = ["Widget", "Widgets", "widget_content"]

Size = tuple[int, int]


def widget_content(rows: Any = None, **style: Any) -> dict[str, Any]:
    """The wire form of a content: `rows` is a string, a list of
    strings, or rows of runs `{"t": "ab", "fg": ...}`; `style` (`fg`,
    `bg`, `bold`...) applies to strings."""
    if isinstance(rows, str):
        rows = rows.split("\n")
    rows = list(rows or [])
    if any(not isinstance(row, str) for row in rows):
        return {
            "cells": [[row] if isinstance(row, dict) else list(row) for row in rows]
        }
    out: dict[str, Any] = {"rows": rows}
    if style:
        out["style"] = style
    return out


class Widget:
    """Handle of a widget: a tag shown by the driver, in the variant and
    at the place it chose.

    `info` is what the driver last told: the answer of a call, or the
    data of a `widgets` event given to `update`. Made inside `batch()`,
    it is filled when the batch is sent.
    """

    def __init__(
        self, pilot: Pilot, tag: str, info: dict[str, Any] | None = None
    ) -> None:
        self._pilot = pilot
        self.tag = tag
        self.info: dict[str, Any] = info or {}

    @property
    def id(self) -> int | None:
        return self.info.get("id")

    @property
    def variant(self) -> str | None:
        """Name of the variant shown; None while hidden."""
        return self.info.get("variant")

    @property
    def size(self) -> Size | None:
        """Inner size of the variant shown."""
        size = self.info.get("size")
        return None if size is None else (size[0], size[1])

    @property
    def rect(self) -> tuple[int, int, int, int] | None:
        """Outer rectangle, screen cells; None while hidden."""
        rect = self.info.get("rect")
        return None if rect is None else (rect[0], rect[1], rect[2], rect[3])

    @property
    def placed(self) -> bool:
        return self.info.get("state") == "placed"

    @property
    def over(self) -> bool:
        """A modal put over the text, for want of a room."""
        return bool(self.info.get("over"))

    @property
    def asks(self) -> str | None:
        """`loud` (a question), `quiet`, or None: it does not ask."""
        return self.info.get("asks")

    @property
    def focused(self) -> bool:
        """The keys of the host keyboard are its own."""
        return bool(self.info.get("focused"))

    def ask(
        self,
        quiet: bool = False,
        escape: str | bool | None = None,
        close: str | bool | None = None,
    ) -> None:
        """Ask for the keyboard: the user gives it (ctrl+t twice), the
        app never takes it. Focused, each key is an `input` event
        `{"key", "bytes", "widget": id}` of this connection; the focus
        lost, `{"widget": id, "released": reason}`.

        `quiet`: it waits for no answer: unmarked, after the loud ones.
        `escape`: the key that gives the keyboard back (`esc`); False:
        none, every key is the widget's. `close`: the key that hides it
        (`ctrl+x`), released `close`; False: none."""
        params: dict[str, Any] = {"id": self.id}
        if quiet:
            params["quiet"] = True
        if escape is not None:
            params["escape"] = escape
        if close is not None:
            params["close"] = close
        self._pilot._request(
            "widget.ask", params, lambda res: setattr(self, "info", res["widget"])
        )

    def release(self) -> None:
        """Ask no more; the focus goes with it. Fine if it is gone."""
        try:
            self._pilot.call("widget.release", id=self.id)
        except PtyrcError as exc:
            if exc.code not in ("no-sprite", "gone"):
                raise
        self.info = {**self.info, "asks": None, "focused": False}

    def update(self, info: dict[str, Any]) -> bool:
        """Take the data of a `widgets` event; False when it tells of
        another widget."""
        if info.get("id") != self.id:
            return False
        self.info = info
        return True

    def refresh(self) -> Widget:
        """Ask the driver (`widget.list`)."""
        found = [w for w in self._pilot.widgets() if w["id"] == self.id]
        if found:
            self.info = found[0]
        else:
            self.info = {**self.info, "state": "gone", "variant": None, "rect": None}
        return self

    def set(
        self,
        rows: Any = None,
        *,
        variant: str | None = None,
        title: str | None = None,
        **style: Any,
    ) -> None:
        """New content for a variant (the one shown when not named),
        and, or, a new title. The driver keeps the content of each
        variant: one set while not shown is there when it is."""
        params: dict[str, Any] = {"id": self.id}
        if variant is not None:
            params["variant"] = variant
        if rows is not None:
            params["content"] = widget_content(rows, **style)
        if title is not None:
            params["title"] = title
        self._pilot.call("widget.set", **params)

    def hide(self) -> None:
        """Take it off the screen; fine if it is already gone."""
        try:
            self._pilot.call("widget.hide", id=self.id)
        except PtyrcError as exc:
            if exc.code not in ("no-sprite", "gone"):
                raise

    def __repr__(self) -> str:
        return f"Widget({self.id}, {self.tag!r}, {self.variant!r})"


class Widgets:
    """Methods of `Pilot` for the widgets."""

    # Given by `Pilot`.
    call: Any
    _request: Any

    def widget(
        self,
        tag: str,
        sizes: Sequence[Size] | Mapping[str, Size],
        *,
        border: str | dict[str, Any] | None = None,
        title: str | None = None,
        frame: int | str | None = None,
        prefer: str | None = None,
        fill: float | None = None,
        margin: int | None = None,
        layer: int | str | None = None,
        modal: bool = False,
        content: Callable[[int, int], Any] | None = None,
    ) -> Widget:
        """Declare `tag` and show it. `sizes`: the inner sizes `(w, h)`
        of its variants, named `WxH`, or a dict of names to sizes. The
        driver shows one of them where there is room, about `fill`
        (0.5) of it, moves it when the app needs the place, and puts
        another size when the room changes: topic `widgets`. `margin`:
        cells kept from the edges of the view, 0 to 4 (1).

        `border`: a preset (`Pilot.borders()`) or its wire object;
        `title`: on the bottom edge, the tag when absent. `content(w, h)`
        gives the first rows of each variant; without it they are
        blank until `Widget.set`. `modal`: always shown, in front of
        the text (layer `hud`), over it when no room takes it
        (`Widget.over`); it stays where it is put."""
        named = (
            sizes.items() if isinstance(sizes, Mapping) else ((None, s) for s in sizes)
        )
        variants = []
        for name, (w, h) in named:
            variant: dict[str, Any] = {"tag": tag, "size": [w, h]}
            if name is not None:
                variant["name"] = name
            if content is not None:
                variant["content"] = widget_content(content(w, h))
            variants.append(variant)
        show = {"tag": tag, "border": border, "title": title, "frame": frame}
        show |= {"prefer": prefer, "fill": fill, "margin": margin, "layer": layer}
        show |= {"modal": modal or None}
        widget = Widget(self, tag)  # type: ignore[arg-type]
        self._request("widget.declare", {"widgets": variants})
        self._request(
            "widget.show",
            {k: v for k, v in show.items() if v is not None},
            lambda res: setattr(widget, "info", res["widget"]),
        )
        return widget

    def widgets(self) -> list[dict[str, Any]]:
        """Every widget of the session, as `widget.list` tells them."""
        out: list[dict[str, Any]] = []
        self._request("widget.list", {}, lambda res: out.extend(res["widgets"]))
        return out

    def focus(self, widget: Widget | int | None = None) -> dict[str, Any] | None:
        """Give the keyboard to a widget that asks, as the hotkey does:
        `widget`, or the next of the order. Its info; None past the
        last. For a key binding of the user, a test: an app does not
        take the focus for itself."""
        ident = widget.id if isinstance(widget, Widget) else widget
        params = {} if ident is None else {"id": ident}
        info = self.call("widget.focus", **params)["widget"]
        if info is not None and isinstance(widget, Widget):
            widget.info = info
        return info

    def blur(self) -> None:
        """Drop the focus: the keys are the app's again."""
        self.call("widget.blur")

    def borders(self) -> list[str]:
        """Names of the border presets."""
        return self.call("widget.borders")["borders"]
