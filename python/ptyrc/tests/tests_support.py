# SPDX-License-Identifier: GPL-3.0-or-later
"""Helpers shared by the tests."""

from __future__ import annotations

import os
import time
from collections.abc import Callable
from typing import Any

__all__ = ["pid_alive", "until"]


def pid_alive(pid: int) -> bool:
    """Running and not a zombie."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    try:
        with open(f"/proc/{pid}/stat") as stat:
            return stat.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def until(check: Callable[[], Any], timeout: float = 4.0) -> Any:
    """Poll `check` until it returns something true; that value."""
    deadline = time.monotonic() + timeout
    while True:
        value = check()
        if value:
            return value
        if time.monotonic() > deadline:
            raise AssertionError("condition not met in time")
        time.sleep(0.03)
