# SPDX-License-Identifier: GPL-3.0-or-later
"""Client apps for ptyrc: small programs that draw on a wrapped terminal.

Run one with `ptyrc-app <name>` or `python -m ptyrc_apps.<name>`.
"""

from __future__ import annotations

__all__ = ["APPS"]

#: Name of each app and what it does; the module is `ptyrc_apps.<name>`.
APPS = {
    "cat": "a cat on the text; it sleeps when nothing moves",
    "clock": "a floating clock in a quiet corner",
    "confetti": "a burst of confetti; a party with --party",
    "fishes": "fishes behind the text, afraid of the cursor",
    "flower": "a flower next to a word",
    "marks": "a highlighter: the matches of patterns, in colours",
    "rain": "quiet rain behind the text",
    "sand": "sand falling in an idle pane",
    "snake": "a snake that plays alone between the words",
}
