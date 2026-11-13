# SPDX-License-Identifier: GPL-3.0-or-later
"""Client apps for ptyrc: small programs that draw on a wrapped terminal.

Run one with `ptyrc-app <name>` or `python -m ptyrc_apps.<name>`.
"""

from __future__ import annotations

__all__ = ["APPS"]

#: Name of each app and what it does; the module is `ptyrc_apps.<name>`.
APPS = {
    "confetti": "adds confetti to your terminal",
    "clock": "one clock in a quiet corner",
    "fishes": "fishes hides behind the text",
    "cat": "one cat comes to visit you",
    "rain": "quiet rain behind the text",
    "snow": "snow on the lines of text",
    "sand": "sand falling in an idle pane",
    "snake": "snake game that plays alone",
    "bricks": "brick breaker game that plays alone",
    "flower": 'add flowers next to the word "flower"',
    "marks": "highlights a selected pattern",
}
