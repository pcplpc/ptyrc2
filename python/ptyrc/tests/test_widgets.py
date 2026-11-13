# SPDX-License-Identifier: GPL-3.0-or-later
"""Widgets: declared, placed by the driver, set, moved, hidden."""

from __future__ import annotations

import pytest
from ptyrc import Pilot, PtyrcError, widget_content
from tests_support import until


def test_widget_content() -> None:
    assert widget_content("a\nb", fg="red") == {
        "rows": ["a", "b"],
        "style": {"fg": "red"},
    }
    run = {"t": "a", "fg": "red"}
    assert widget_content([[run], "b"]) == {"cells": [[run], ["b"]]}
    assert widget_content([run]) == {"cells": [[run]]}


def test_placed_set_hidden(shell: Pilot) -> None:
    assert "plain" in shell.borders() and "painting" in shell.borders()
    note = shell.widget("note", [(4, 1), (12, 3)], content=lambda w, h: ["?" * w] * h)
    until(lambda: note.refresh().placed)
    assert note.variant == "12x3" and note.size == (12, 3)
    x, y, w, h = note.rect
    assert (w, h) == (14, 5)
    edge = shell.shown()[y + h - 1]
    assert edge[x] == "└" and edge[x + w - 8 : x + w] == "─ note ┘"
    assert shell.shown()[y + 1][x : x + w] == "│" + "?" * 12 + "│"

    note.set(["hello"], title="memo", fg="green")
    assert shell.shown()[y + h - 1][x + w - 8 : x + w] == "─ memo ┘"
    assert shell.shown()[y + 1][x + 1 : x + 6] == "hello"
    note.set(["small"], variant="4x1")  # kept for when it is shown
    assert [w["id"] for w in shell.widgets()] == [note.id]

    note.hide()
    note.hide()
    assert shell.widgets() == [] and not note.refresh().placed
    assert shell.sprites() == []


def test_events_and_smaller(spawn) -> None:  # type: ignore[no-untyped-def]
    pilot = spawn(["env", "PS1=$ ", "sh"], size=(40, 10))
    pilot.wait_for("$ ")
    until(lambda: pilot.frame("raw").usable)
    pilot.subscribe("widgets")
    note = pilot.widget("note", {"big": (20, 5), "small": (6, 1)}, border="round")
    until(lambda: note.refresh().placed)
    assert note.variant == "big"
    # Text where it stands: the driver puts the small one elsewhere.
    pilot.send("clear; for i in 1 2 3 4 5 6 7; do echo $i $i $i $i $i $i; done\n")
    seen = []
    for topic, data in pilot.events(timeout=5):
        if topic == "widgets" and note.update(data):
            seen.append(note.variant)
            if note.variant == "small":
                break
    assert seen[-1] == "small" and note.rect is not None
    x, y, _, _ = note.rect
    assert pilot.shown()[y][x] == "╭"


def test_modal_over_the_text(spawn) -> None:  # type: ignore[no-untyped-def]
    fill = "for i in $(seq 9); do printf '%039d\\n' 0; done; printf '%039d' 0; sleep 60"
    pilot = spawn(["sh", "-c", fill], size=(40, 10))
    pilot.wait_for("0" * 39)
    until(lambda: pilot.frame("raw").usable)
    sizes = [(10, 2), (30, 4), (60, 12)]
    menu = pilot.widget("menu", sizes, modal=True, border="alert", fill=1.0)
    until(lambda: menu.refresh().placed)
    assert menu.over and menu.info["modal"] and menu.variant == "30x4"
    assert menu.rect == (4, 2, 32, 6)
    menu.set(["> start an app"])
    shown = pilot.shown()
    assert shown[0] == "0" * 39
    assert shown[1] == "00" + " " * 36 + "0"  # the gap, blanked
    assert shown[2] == "00  ┏" + "━" * 30 + "┓  0"
    assert shown[3] == "00  ┃> start an app" + " " * 16 + "┃  0"
    assert shown[7] == "00  ┗" + "━" * 24 + " menu ┛  0"
    sprite = next(s for s in pilot.sprites() if s["id"] == menu.info["sprite"])
    assert sprite["layer"] == 100 and sprite["anchor"]["at"] == [2, 1]
    menu.hide()
    assert pilot.shown()[3] == "0" * 39


def test_errors(shell: Pilot) -> None:
    with pytest.raises(PtyrcError) as err:
        shell.call("widget.show", tag="nope")
    assert err.value.code == "bad-params"
    with pytest.raises(PtyrcError) as err:
        shell.call("widget.set", id=999, title="x")
    assert err.value.code == "no-sprite"
    with pytest.raises(PtyrcError) as err:
        shell.widget("note", [(4, 1)], border="nope")
    assert err.value.code == "bad-params" and "plain" in (err.value.hint or "")
