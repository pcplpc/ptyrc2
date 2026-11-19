# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixtures: a headless `sh` session the apps draw on; one on a host
that takes a mouse."""

from __future__ import annotations

import fcntl
import os
import pty
import shutil
import struct
import subprocess
import tempfile
import termios
import threading
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

# Session time runs faster than real time (`ptyrc.clock`); read at import.
os.environ.setdefault("PTYRC_CLOCK_RATE", "5")

from ptyrc import Pilot, PtyrcError, clock

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
    # The drivers read `permissions.toml` there, not the user's.
    config = tempfile.mkdtemp(prefix="ptyrc-c")
    names = ("XDG_RUNTIME_DIR", "PTYRC_BIN", "XDG_CONFIG_HOME")
    saved = {k: os.environ.get(k) for k in names}
    os.environ["XDG_CONFIG_HOME"] = config
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
    shutil.rmtree(config, ignore_errors=True)


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


class Host:
    """A terminal that is not there: `ptyrc run` on a pty, what it
    writes kept, what a mouse would send typed in."""

    def __init__(self, name: str) -> None:
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        argv = [os.environ["PTYRC_BIN"], "run", "--name", name, "--"]
        argv += ["env", "PS1=$ ", "sh"]
        self.driver = subprocess.Popen(
            argv, stdin=slave, stdout=slave, stderr=slave, start_new_session=True
        )
        os.close(slave)
        self.seen = b""
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self) -> None:
        try:
            while data := os.read(self.master, 65536):
                self.seen += data
        except OSError:
            pass

    def move(self, x: int, y: int) -> None:
        """A move with no button down on a screen cell, SGR form."""
        os.write(self.master, f"\x1b[<35;{x + 1};{y + 1}M".encode())

    def press(self, x: int, y: int, down: bool = True) -> None:
        """The left button pressed on a screen cell, or released there."""
        os.write(self.master, f"\x1b[<0;{x + 1};{y + 1}{'M' if down else 'm'}".encode())

    def wait_seen(self, needle: bytes, start: int = 0) -> None:
        deadline = time.monotonic() + 5
        while needle not in self.seen[start:]:
            assert time.monotonic() < deadline, (needle, self.seen[-200:])
            time.sleep(0.01)

    def close(self) -> None:
        self.driver.kill()
        self.driver.wait()
        os.close(self.master)


@pytest.fixture
def hosted() -> Iterator[tuple[Host, Pilot]]:
    name = f"hover-{os.getpid()}-{time.monotonic_ns() % 10**9}"
    host = Host(name)
    try:
        deadline = time.monotonic() + 5
        while True:
            try:
                pilot = Pilot(name)
                break
            except (PtyrcError, OSError):
                assert time.monotonic() < deadline, "no session"
                time.sleep(0.02)
        with pilot:
            pilot.wait_for("$ ")
            while not pilot.frame("raw").usable:
                assert time.monotonic() < deadline, "no usable frame"
                time.sleep(0.02)
            yield host, pilot
    finally:
        host.close()
