# SPDX-License-Identifier: GPL-3.0-or-later
"""Frames on a scrolling vim: the view moves, the frame does not."""

from __future__ import annotations

import pytest
from conftest import wait_pane
from ptyrc import Pilot, PtyrcError
from tests_support import until


def test_pane_follows_scrolling(vim: Pilot) -> None:
    pane = wait_pane(vim)
    assert pane.kind == "pane" and pane.usable and pane.state == "stable"
    assert pane.origin == (0, 0)
    assert pane.text()[0] == "line 001 of the file"
    assert vim.frame("active").id == pane.id
    assert vim.frame("largest").id == pane.id
    assert vim.frame(pane.id).rect == pane.rect
    assert vim.frame("raw").id == 0

    # A sprite fixed in the frame, on line 010; a HUD fixed in the view.
    word = pane.find("line 010")[0]
    assert (word.x, word.y) == (0, 9) and word.screen == pane.to_screen(0, 9)
    mark = pane.draw(["<<"], at=(word.x + 21, word.y), space="frame", fg="red")
    hud = pane.draw(["HUD"], at=(-1, 0), bg="blue")
    assert until(lambda: mark.info()["screen"]) == [21, 9]
    hud_at = hud.info()["screen"]
    assert hud_at == [pane.rect[0] + pane.size[0] - 3, pane.rect[1]]

    # Scroll five lines: the origin follows, the mark moves with its line.
    vim.keys(*["ctrl+e"] * 5)
    vim.wait_for("line 006")
    until(lambda: pane.refresh().origin == (0, 5))
    assert pane.to_screen(0, 9) == (pane.rect[0], pane.rect[1] + 4)
    assert pane.to_frame(*pane.to_screen(3, 9)) == (3, 9)
    assert pane.to_screen(0, 0) is None
    assert until(lambda: mark.info()["screen"] == [21, 4])
    assert hud.info()["screen"] == hud_at
    assert vim.frame_at(0, pane.rect[1]) == (pane.id, (0, 5))

    # Scrolled out of the view: clipped away (its place is above the
    # screen), not lost.
    vim.keys(*["ctrl+e"] * 10)
    until(lambda: pane.refresh().origin == (0, 15))
    assert until(lambda: mark.info()["screen"] == [21, -6])
    assert "<<" not in "".join(vim.dump()["host"])

    # What went by is remembered, and searchable.
    known = pane.known(0, 12)
    assert known[0] == "line 001 of the file" and known[9].startswith("line 010")
    assert pane.find("line 003") == []
    off = pane.find("line 003", known=True)
    assert [(m.y, m.screen) for m in off] == [(2, None)]
    assert pane.extent[0] == 0 and pane.extent[1] >= 15

    # Back to the top: same rows, the mark returns to its line.
    vim.keys("g", "g")
    until(lambda: pane.refresh().origin == (0, 0))
    assert until(lambda: mark.info()["screen"] == [21, 9])


def test_frame_list_and_heat(vim: Pilot) -> None:
    pane = wait_pane(vim)
    kinds = {f.kind for f in vim.frames()}
    assert "raw" in kinds and "pane" in kinds
    raw = vim.frame("raw")
    assert raw.rect == (0, 0, 60, 20) and raw.size == (60, 20)
    heat = pane.heat()
    assert len(heat) == pane.size[1]
    assert all(len(row) == pane.size[0] and row.isdigit() for row in heat)
    # Text is occupied, the right of the pane is free.
    assert int(heat[2][2]) < int(heat[2][-2])
    assert repr(pane).startswith(f"Frame({pane.id}, pane")


def test_idle(vim: Pilot) -> None:
    pane = wait_pane(vim)
    # Just opened: nothing has been static for 10 s yet.
    assert pane.check_idle() < 0.8 and not pane.is_idle
    assert set("".join(pane.heat_classes())) <= set("ht.p")
    # Left alone, the pane becomes idle; redrawing it as it is changes
    # nothing.
    until(lambda: pane.is_idle, timeout=15)
    vim.keys("ctrl+l")
    idle = pane.idle()
    assert idle["idle"] and 0.8 <= idle["score"] <= 1.0
    assert 0.0 < idle["free"] < idle["score"]
    assert pane.check_idle() == idle["score"] or pane.is_idle
    classes = pane.heat_classes()
    assert classes[2][2] == "t" and classes[2][-2] == "."
    assert vim.frame("idle").id == pane.id
    assert pane.refresh().info["is_idle"] is True
    # Scrolling by three lines: only the digits that differ are hot.
    vim.keys(*["ctrl+e"] * 3)
    until(lambda: pane.check_idle() < idle["score"])
    classes = pane.heat_classes()
    assert classes[2][7] == "h" and classes[2][2] == "t"


def test_wait_idle(vim: Pilot) -> None:
    pane = wait_pane(vim)
    # Just opened: not idle yet, and a short wait says so.
    with pytest.raises(TimeoutError):
        vim.wait_idle(pane.id, timeout=0.3)
    # One request, answered when the pane is idle; a name is resolved by
    # the driver.
    vim.subscribe("idle")
    assert vim.wait_idle("idle", timeout=15).id == pane.id
    assert pane.wait_idle(timeout=1).info["is_idle"] is True
    # The event came on its own, once per frame.
    seen = [d for _, d in vim.events(timeout=0.5) if d["frame"] == pane.id]
    assert [d["idle"] for d in seen] == [True] and seen[0]["score"] >= 0.8


def test_frame_errors_carry_the_hint(vim: Pilot) -> None:
    with pytest.raises(PtyrcError) as err:
        vim.frame("activ")
    assert err.value.code == "no-frame"
    assert err.value.hint is not None and "did you mean active?" in err.value.hint
    pane = wait_pane(vim)
    pane.info["id"] = 99
    with pytest.raises(PtyrcError) as err:
        pane.refresh()
    assert err.value.code == "no-frame" and err.value.hint
