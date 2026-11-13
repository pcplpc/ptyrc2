# SPDX-License-Identifier: GPL-3.0-or-later
"""The demo: which app comes when."""

from __future__ import annotations

import itertools

import pytest
from ptyrc import Pilot
from ptyrc_apps import APPS, confetti, demo
from ptyrc_apps.__main__ import main


def test_rounds() -> None:
    shown = sorted(name for name in APPS if demo.VARIANTS.get(name, ((),)))
    assert "marks" not in shown and "snake" in shown
    for seed in range(20):
        picks = [demo.pick(seed, nth) for nth in range(6 * len(shown))]
        names = [name for name, *_ in picks]
        # Each round shows every app once; none comes twice in a row.
        for start in range(0, len(names), len(shown)):
            assert sorted(names[start : start + len(shown)]) == shown
        assert all(a != b for a, b in itertools.pairwise(names))
        # The same seed, the same order.
        assert picks == [demo.pick(seed, nth) for nth in range(len(picks))]
    assert demo.order(1, 0) != demo.order(2, 0)


def test_versions() -> None:
    seen = {tuple(demo.pick(seed, nth)) for seed in range(8) for nth in range(40)}
    assert {("snake",), ("snake", "-a"), ("flower", "-a"), ("clock",)} <= seen
    assert ("flower",) not in seen and ("clock", "-a") not in seen
    assert set(demo.VARIANTS) <= set(APPS)


def test_main(
    session: Pilot,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    assert main(["demo"]) == 2
    assert main(["demo", "1", "-1"]) == 2
    assert "usage: ptyrc-app demo SEED NTH" in capsys.readouterr().err
    monkeypatch.setenv("PTYRC_SESSION", session.session)
    monkeypatch.setattr(demo, "pick", lambda seed, nth: ["confetti", "--one-shot"])
    monkeypatch.setattr(confetti.time, "sleep", lambda _: None)
    assert main(["demo", "3", "0"]) == 0
    assert session.sprites()
