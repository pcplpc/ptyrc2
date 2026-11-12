#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The arrows move a sprite; the app does not see them.

    ptyrc run -- vim FILE          in one terminal
    scripts/grab-sprite.py         in another (uv run, or PYTHONPATH)

`input.grab` takes the four arrows from the keyboard of the session
before the app. Each press comes as an `input` event and moves a sprite
in the pane that holds the cursor, in frame cells: scrolled, it goes
with the text. Every other key is the app's. ctrl+] gives the arrows
back and ends.
"""

from __future__ import annotations

import sys

from ptyrc import Pilot

STEPS = {"up": (0, -1), "down": (0, 1), "left": (-2, 0), "right": (2, 0)}
SHIP = ["<*>"]


def main(session: str | None = None) -> None:
    with Pilot(session) as pilot:
        frame = pilot.frame("active")
        (ox, oy), (w, h) = frame.origin, frame.size
        x, y = ox + w // 2, oy + h // 2
        ship = frame.draw(
            SHIP, at=(x, y), space="frame", layer="front", fg="yellow", bold=True
        )
        pilot.grab(*STEPS)
        for _, data in pilot.events("exit"):
            if not data or "released" in data or "status" in data:
                break
            dx, dy = STEPS.get(data.get("key"), (0, 0))
            frame.refresh()
            (ox, oy), (w, h) = frame.origin, frame.size
            # Inside the view, wherever it scrolled to.
            x = min(max(x + dx, ox), ox + w - len(SHIP[0]))
            y = min(max(y + dy, oy), oy + h - 1)
            ship.move(x, y, space="frame")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else None)
