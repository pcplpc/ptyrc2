#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""A sprite that is a button: it counts the clicks it gets.

    ptyrc run -- vim FILE          in one terminal
    scripts/click-sprite.py        in another (uv run, or PYTHONPATH)

While this client listens to `mouse`, the driver asks the host terminal
for mouse reports. A press on the sprite comes as a `mouse` event and
is not the app's; a click anywhere else goes to the app, if it asked
for the mouse. A right click on the button ends.
"""

from __future__ import annotations

import sys

from ptyrc import Pilot


def label(count: int) -> list[str]:
    return [f"[clicks:{count}]"]


def main(session: str | None = None) -> None:
    with Pilot(session) as pilot:
        cols, _ = pilot.size
        button = pilot.draw(
            label(0), at=(cols - 14, 1), layer="front", fg="yellow", bold=True
        )
        for count, click in enumerate(pilot.clicks(button), 1):
            if click["button"] == "right":
                break
            button.set(label(count), fg="yellow", bold=True)
        button.remove()


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else None)
