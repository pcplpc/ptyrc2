# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixtures: a headless driver per test, sockets in a private directory."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterator
from pathlib import Path

import pytest
from ptyrc import Pilot

# `tests_support` is imported by the test modules.
sys.path.insert(0, str(Path(__file__).parent))

from tests_support import until

ROOT = Path(__file__).resolve().parents[3]
VIM = "/usr/bin/vim"


@pytest.fixture(scope="session", autouse=True)
def _driver_env() -> Iterator[None]:
    """Build the binary if needed; keep the sockets out of the user's
    runtime directory (and short: a socket path is at most 107 bytes)."""
    binary = ROOT / "target" / "debug" / "ptyrc"
    if not binary.is_file():
        subprocess.run(
            ["cargo", "build", "-q", "-p", "ptyrc-cli"], cwd=ROOT, check=True
        )
    runtime = tempfile.mkdtemp(prefix="ptyrc-t")
    saved = {k: os.environ.get(k) for k in ("XDG_RUNTIME_DIR", "PTYRC_BIN")}
    os.environ["XDG_RUNTIME_DIR"] = runtime
    os.environ["PTYRC_BIN"] = str(binary)
    for name in ("PTYRC_SESSION", "PTYRC_SOCKET"):
        os.environ.pop(name, None)
    yield
    for key, value in saved.items():
        if value is None:
            os.environ.pop(key, None)
        else:
            os.environ[key] = value
    shutil.rmtree(runtime, ignore_errors=True)


Spawn = Callable[..., Pilot]


@pytest.fixture
def spawn() -> Iterator[Spawn]:
    """`spawn(argv, size=(80, 24))`: a pilot on a headless driver, closed
    after the test."""
    pilots: list[Pilot] = []

    def make(argv: list[str], **kw: object) -> Pilot:
        pilot = Pilot.spawn(argv, env={"TERM": "xterm-256color"}, **kw)
        pilots.append(pilot)
        return pilot

    yield make
    for pilot in pilots:
        pilot.close()


@pytest.fixture
def shell(spawn: Spawn) -> Pilot:
    """A bare `sh` with a `$ ` prompt."""
    pilot = spawn(["env", "PS1=$ ", "sh"])
    pilot.wait_for("$ ")
    # The raw frame takes a moment to settle after the first screen.
    until(lambda: pilot.frame("raw").usable)
    return pilot


@pytest.fixture
def long_file(tmp_path: Path) -> Path:
    path = tmp_path / "long.txt"
    path.write_text("".join(f"line {i:03d} of the file\n" for i in range(1, 201)))
    return path


@pytest.fixture
def vim(spawn: Spawn, long_file: Path) -> Pilot:
    """vim on a 200-line file, its pane stable."""
    if not Path(VIM).exists():
        pytest.skip("vim not installed")
    pilot = spawn([VIM, "-u", "NONE", "-N", str(long_file)], size=(60, 20))
    pilot.wait_for("line 001")
    wait_pane(pilot)
    return pilot


def wait_pane(pilot: Pilot):
    """The first usable pane."""
    return until(pilot.panes, timeout=6.0)[0]
