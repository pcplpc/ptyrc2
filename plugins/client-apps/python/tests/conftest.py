# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixtures: a headless `sh` session the apps draw on."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

# Session time runs faster than real time (`ptyrc.clock`); read at import.
os.environ.setdefault("PTYRC_CLOCK_RATE", "5")

from ptyrc import Pilot, clock

ROOT = Path(__file__).resolve().parents[4]


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers", "realtime: session time is real time (rate 1) in this test"
    )


@pytest.fixture(autouse=True)
def _realtime(request: pytest.FixtureRequest) -> Iterator[None]:
    """With `@pytest.mark.realtime`: rate 1 for the pilot and the drivers
    the test starts."""
    if request.node.get_closest_marker("realtime") is None:
        yield
        return
    saved = clock.rate()
    with pytest.MonkeyPatch.context() as mp:
        mp.setenv("PTYRC_CLOCK_RATE", "1")
        clock.set_rate(1)
        try:
            yield
        finally:
            clock.set_rate(saved)


@pytest.fixture(scope="session", autouse=True)
def _driver_env() -> Iterator[None]:
    binary = ROOT / "target" / "debug" / "ptyrc"
    if not binary.is_file():
        subprocess.run(
            ["cargo", "build", "-q", "-p", "ptyrc-cli"], cwd=ROOT, check=True
        )
    runtime = tempfile.mkdtemp(prefix="ptyrc-a")
    saved = {k: os.environ.get(k) for k in ("XDG_RUNTIME_DIR", "PTYRC_BIN")}
    os.environ.update(XDG_RUNTIME_DIR=runtime, PTYRC_BIN=str(binary))
    for name in ("PTYRC_SESSION", "PTYRC_SOCKET"):
        os.environ.pop(name, None)
    yield
    for key, value in saved.items():
        if value is None:
            os.environ.pop(key, None)
        else:
            os.environ[key] = value
    shutil.rmtree(runtime, ignore_errors=True)


@pytest.fixture
def session() -> Iterator[Pilot]:
    """An observer on a fresh session; the app under test connects to it by
    name, as a user's would."""
    with Pilot.spawn(["env", "PS1=$ ", "sh"], size=(80, 24)) as pilot:
        pilot.wait_for("$ ")
        deadline = time.monotonic() + 3
        while not pilot.frame("raw").usable and time.monotonic() < deadline:
            time.sleep(0.02)
        yield pilot
