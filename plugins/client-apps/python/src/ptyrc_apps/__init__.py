# SPDX-License-Identifier: GPL-3.0-or-later
"""Client apps for ptyrc: small programs that draw on a wrapped terminal.

Run one with `ptyrc-app <name>` or `python -m ptyrc_apps.<name>`.
"""

from __future__ import annotations

__all__ = ["APPS"]

#: Name of each app and what it does; the module is `ptyrc_apps.<name>`.
APPS = {
    "clock": "a floating clock in a quiet corner",
    "confetti": "a burst of confetti",
}
