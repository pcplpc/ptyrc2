# SPDX-License-Identifier: GPL-3.0-or-later
"""Session time: real time times `$PTYRC_CLOCK_RATE` (default 1).

A driver reads the variable when it starts, so that a pilot and its
driver agree. Tests set it above 1 to make waits shorter. Programs in the
pty keep real time.
"""

from __future__ import annotations

import os
import time

__all__ = ["monotonic", "rate", "real", "set_rate", "sleep"]


def _parse(text: str | None) -> float:
    try:
        value = float(text or "1")
    except ValueError:
        return 1.0
    return value if 0.0 < value < float("inf") else 1.0


_rate = _parse(os.environ.get("PTYRC_CLOCK_RATE"))
# Session time `_at` at real time `_real`.
_real = _at = time.monotonic()


def rate() -> float:
    """Session seconds per real second."""
    return _rate


def set_rate(value: float) -> None:
    """Change the rate from now on; session time stays continuous. A
    driver started later needs `$PTYRC_CLOCK_RATE` to match."""
    global _rate, _real, _at
    now = time.monotonic()
    _at, _real, _rate = monotonic(), now, _parse(str(value))


def monotonic() -> float:
    """Session seconds, monotonic."""
    return _at + (time.monotonic() - _real) * _rate


def real(seconds: float) -> float:
    """The real seconds that `seconds` of session time take."""
    return seconds / _rate


def sleep(seconds: float) -> None:
    """Sleep `seconds` of session time."""
    time.sleep(max(0.0, seconds) / _rate)
