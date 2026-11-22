# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app has its store twin: `plugins/store-apps/NAME/`."""

from __future__ import annotations

from pathlib import Path

from ptyrc_apps import APPS

STORE = Path(__file__).resolve().parents[3] / "store-apps"


def test_each_app_has_a_store_tree() -> None:
    trees = {p.parent.name for p in STORE.glob("*/ptyrc-app.toml")}
    assert set(APPS) <= trees, sorted(set(APPS) - trees)
    # `fishes --tank` is an app of its own.
    assert trees - set(APPS) == {"tank"}
