# SPDX-License-Identifier: GPL-3.0-or-later
"""ptyrc: the pty remote controller, pilot side.

from ptyrc import Pilot

with Pilot() as pilot:
    print("\n".join(pilot.text()))
    pilot.draw(["hello"], auto="top-right", fg="yellow")
"""

from __future__ import annotations

from . import clock
from ._wire import PtyrcError, live_sessions, runtime_dir, socket_path
from .pilot import Cursor, Frame, Match, Pilot, Sprite, find_binary, sprite_spec
from .tools import Locator, Tape
from .widgets import Widget, widget_content

__all__ = [
    "Cursor",
    "Frame",
    "Locator",
    "Match",
    "Pilot",
    "PtyrcError",
    "Sprite",
    "Tape",
    "Widget",
    "clock",
    "find_binary",
    "live_sessions",
    "runtime_dir",
    "socket_path",
    "sprite_spec",
    "widget_content",
]
