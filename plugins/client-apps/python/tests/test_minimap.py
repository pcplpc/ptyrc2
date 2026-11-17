# SPDX-License-Identifier: GPL-3.0-or-later
"""`minimap`: the scale, the dots and the colours without a driver; one
run over a pager, one under a host mouse."""

from __future__ import annotations

import re
import threading
import time
from pathlib import Path
from typing import Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import minimap
from ptyrc_apps.marks import INKS
from ptyrc_apps.minimap import CODE, CURSOR, EDITED, GROUND, NOTE, VISITED, Doc, Look


def plain(rows: minimap.Rows) -> list[str]:
    return ["".join(run["t"] for run in row) for row in rows]


def cells(rows: minimap.Rows) -> list[list[tuple[int, int | None]]]:
    """`(fg, bg)` of each cell."""
    return [[(r["fg"], r.get("bg")) for r in row for _ in r["t"]] for row in rows]


@pytest.mark.parametrize(
    ("count", "width", "size", "want"),
    [
        (0, 80, (6, 8), (7, 1)),
        (32, 80, (6, 8), (7, 1)),  # a line a dot row
        (33, 80, (6, 8), (7, 2)),
        (1000, 80, (6, 24), (7, 11)),
        (10, 12, (6, 8), (1, 1)),
        (10, 400, (10, 8), (5, 1)),  # cut at WIDTH
    ],
)
def test_scale(
    count: int, width: int, size: tuple[int, int], want: tuple[int, int]
) -> None:
    assert minimap.scale(count, width, *size) == want


def test_inks() -> None:
    assert minimap.inks("", 2, 3) == 0
    assert minimap.inks("ab", 2, 3) == 0b1
    assert minimap.inks("    x", 2, 3) == 0b100
    assert minimap.inks("a b c d e f g", 2, 3) == 0b111111  # the rest is cut
    assert minimap.inks("\t x", 1, 2) == 0b0100


def test_face_dots() -> None:
    # A column a dot, a line a dot row: the text is the picture.
    doc = Doc(lines=["xx", "x", " x", "", "x"], width=4)
    assert plain(minimap.face(doc, 2, 2)) == ["⠫ ", "⠁ "]
    # Two lines a dot row: on when one of the two holds ink.
    doc = Doc(lines=["x", "", "", "", "x", "x", "", ""], width=4)
    assert plain(minimap.face(doc, 2, 1)) == ["⠅ "]
    # Every row is as wide as the map, whatever is known.
    for count in (0, 1, 7, 300):
        rows = plain(minimap.face(Doc(lines=["x" * 9] * count), 6, 8))
        assert len(rows) == 8 and {len(row) for row in rows} == {6}


def test_face_view() -> None:
    doc = Doc(y0=100, lines=["x"] * 32, top=108, view=8, width=4)
    grounds = [row[0][1] for row in cells(minimap.face(doc, 2, 8))]
    assert grounds == [None, None, GROUND, GROUND, None, None, None, None]
    inks = [row[0][0] for row in cells(minimap.face(doc, 2, 8))]
    assert inks == [CODE[0]] * 2 + [CODE[1]] * 2 + [CODE[0]] * 4
    # A view smaller than a cell still shows.
    doc.top, doc.view = 105, 1
    grounds = [row[0][1] for row in cells(minimap.face(doc, 2, 8))]
    assert grounds == [None, GROUND] + [None] * 6
    # One run a row where the colours are the same.
    assert all(len(row) == 1 for row in minimap.face(doc, 2, 8))


def test_face_cursor() -> None:
    doc = Doc(lines=["x"] * 32, top=8, view=8, width=4, cursor=9)
    grounds = [row[0][1] for row in cells(minimap.face(doc, 2, 8))]
    assert grounds == [None, None, GROUND, GROUND] + [None] * 4
    grounds = [row[0][1] for row in cells(minimap.face(doc, 2, 8, Look(cursor=True)))]
    assert grounds == [None, None, CURSOR, GROUND] + [None] * 4


def test_face_notes() -> None:
    doc = Doc(lines=["# a", "# b", "# c", "# d", "x = 1", "# e", "", ""], width=4)
    assert [r[0][0] for r in cells(minimap.face(doc, 2, 2))] == [CODE[0]] * 2
    look = Look(notes=True)
    # A cell that holds code is of the colour of code.
    assert [r[0][0] for r in cells(minimap.face(doc, 2, 2, look))] == [NOTE[0], CODE[0]]
    doc.view = 4
    assert [r[0][0] for r in cells(minimap.face(doc, 2, 2, look))] == [NOTE[1], CODE[0]]


def test_face_marks() -> None:
    doc = Doc(lines=["x"] * 31 + ["      ERROR"], top=0, view=4, width=12)
    doc.mark([re.compile("WARN"), re.compile("ERR")])
    assert doc.marks == {31: 1}
    rows = minimap.face(doc, 2, 2)  # four lines a dot row
    assert {fg for fg, _ in cells(rows)[1]} == {INKS[1][0]}
    assert {fg for fg, _ in cells(rows)[0]} == {CODE[1]}
    # The marked line is drawn alone: one in four, it still shows.
    assert plain(rows)[1] == "⡇⣀"
    doc.marks = {}
    assert plain(minimap.face(doc, 2, 2))[1] == "⡇ "


def test_face_heat() -> None:
    doc = Doc(lines=["x"] * 32, top=28, view=4, width=4)
    doc.edited = {0: 100.0, 4: 70.0}
    doc.visited = {4: 100.0, 8: 60.0, 12: 30.0}
    look = Look(heat=True, fade=60.0)
    assert {r[0][0] for r in cells(minimap.face(doc, 2, 8, Look(), 100.0))[:7]} == {
        CODE[0]
    }
    got = [r[0][0] for r in cells(minimap.face(doc, 2, 8, look, 100.0))]
    assert got == [
        EDITED[0],
        minimap.BOTH[1],  # edited 30 s ago, visited now
        VISITED[2],
        CODE[0],  # visited 70 s ago: cold
        CODE[0],
        CODE[0],
        CODE[0],
        CODE[1],  # the view
    ]
    doc.forget(100.0, 60.0)
    assert (doc.edited, doc.visited) == ({0: 100.0, 4: 70.0}, {4: 100.0, 8: 60.0})
    doc.visit(130.0)
    assert doc.visited[28] == doc.visited[31] == 130.0 and 32 not in doc.visited


def test_line_at() -> None:
    doc = Doc(y0=50, lines=["", "", "  a", "b"] + ["c"] * 60, width=4)
    assert minimap.line_at(doc, 2, 4, 0) == 52  # 16 lines a cell
    assert minimap.line_at(doc, 2, 4, 1) == 66
    doc.lines[16:32] = [""] * 16
    assert minimap.line_at(doc, 2, 4, 1) is None
    assert minimap.line_at(doc, 2, 4, 9) is None


def test_title() -> None:
    assert minimap.title(Doc(lines=[""] * 412)) == "412"
    assert minimap.title(Doc(lines=[""] * 3449)) == "3.4k"


def test_heights() -> None:
    assert minimap.heights(0) == [4]
    assert minimap.heights(16) == [4]
    assert minimap.heights(17) == [4, 5]
    assert minimap.heights(50) == [4, 6, 8, 12, 13]
    assert minimap.heights(115) == [4, 6, 8, 12, 16, 24, 29]
    assert minimap.heights(10**6) == list(minimap.HEIGHTS)


def test_framed() -> None:
    rows = minimap.face(Doc(lines=["x"] * 8, width=8), 4, 2)
    assert plain(minimap.framed(rows, 4, "8")) == [
        "╭────╮ ",
        "│⡇   │ ",
        "│⡇   │ ",
        "╰─ 8 ╯ ",
    ]
    left = plain(minimap.framed(rows, 4, "12345", left=True))
    assert left == [" ╭────╮", " │⡇   │", " │⡇   │", " ╰────╯"]  # no room: no name
    assert rows == minimap.face(Doc(lines=["x"] * 8, width=8), 4, 2)


def test_print(capsys: pytest.CaptureFixture[str], tmp_path: Path) -> None:
    path = tmp_path / "text"
    path.write_text("".join(f"{'word ' * (i % 7)}\n" for i in range(200)))
    assert minimap.main(["--print", "8x10", "--file", str(path), "-a"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert out[0] == "8x10" and len(out) == 13
    assert out[1] == "╭" + "─" * 8 + "╮ " and out[-1] == "╰─── 200 ╯ "
    assert all(len(row) == 11 and row[0] == row[-2] == "│" for row in out[2:-1])
    assert minimap.main(["--print"]) == 0
    assert capsys.readouterr().out.count("╭") == 4
    assert minimap.main(["--print", "--file", str(tmp_path / "none")]) == 1
    for bad in (["--mark", "("], ["--width", "1"], ["--print", "8"], ["--fade", "0"]):
        with pytest.raises(SystemExit):
            minimap.main(bad)
        capsys.readouterr()


def widget_of(pilot: Pilot) -> dict[str, Any]:
    deadline = time.monotonic() + 5
    while True:
        placed = [w for w in pilot.widgets() if w["state"] == "placed"]
        if placed:
            return placed[0]
        assert time.monotonic() < deadline, "no widget"
        time.sleep(0.02)


def band(pilot: Pilot, widget: dict[str, Any]) -> list[int]:
    """The rows of the map on the ground of the view, or of the cursor
    in it. The map is inside the app's border, a blank on its right."""
    x, y, w, h = widget["rect"]
    rows = pilot.call("screen.cells", rect=[x + 1, y + 1, w - 3, h - 2], sprites=True)
    return [
        i
        for i, row in enumerate(rows["rows"])
        if any(run.get("bg") in (GROUND, CURSOR) for run in row)
    ]


def state(pilot: Pilot) -> tuple[list[int], str, list[int]]:
    """Of the widget: its size, its last row, its band."""
    widget = widget_of(pilot)
    x, y, w, h = widget["rect"]
    foot = pilot.shown(rect=[x, y + h - 1, w, 1])[0]
    return widget["size"], foot, band(pilot, widget)


def wait_state(pilot: Pilot, *want: Any) -> None:
    deadline = time.monotonic() + 5
    while state(pilot) not in want:
        assert time.monotonic() < deadline, state(pilot)
        time.sleep(0.02)


def test_live(tmp_path: Path) -> None:
    """Over `less`: the map grows with what was shown, the widget with
    it; the band is where the view is, and goes back with it."""
    path = tmp_path / "text"
    path.write_text("".join(f"line {i} {'word ' * (i % 9)}\n" for i in range(400)))
    status: list[int] = []
    with Pilot.spawn(["less", str(path)], size=(80, 24)) as pilot:
        pilot.wait_for("line 22 ")
        deadline = time.monotonic() + 5
        while not pilot.frame("largest").usable:
            assert time.monotonic() < deadline, "no usable frame"
            time.sleep(0.02)
        argv = ["-s", pilot.session, "-a", "--mark", "line 3 "]
        thread = threading.Thread(
            target=lambda: status.append(minimap.main(argv, seconds=600))
        )
        thread.start()
        # A page: 6 rows are enough. The row of the prompt is a line
        # until it is known as chrome. A blank is kept at the edge
        # (`shown` strips it; the rectangle tells).
        rows = [0, 1, 2, 3, 4, 5]
        wait_state(
            pilot,
            ([15, 8], "╰──────── 23 ╯", rows),
            ([15, 8], "╰──────── 24 ╯", rows),
        )
        assert widget_of(pilot)["rect"][0] == 80 - 15
        for page in range(1, 5):
            pilot.keys("space")
            pilot.wait_for(f"line {23 * page + 20} ")
        # 115 lines want 29 rows; 16 fit the pane: two lines a dot row,
        # the view is the last 23.
        wait_state(pilot, ([15, 18], "╰─────── 115 ╯", [11, 12, 13, 14]))
        time.sleep(0.5)  # the last page is remembered once it has stood
        pilot.keys("g")
        pilot.wait_for("line 3 ")
        wait_state(pilot, ([15, 18], "╰─────── 115 ╯", [0, 1, 2]))
        # The mark: the first cell row, in the ink of the first pattern.
        x, y, _, _ = widget_of(pilot)["rect"]
        rows = pilot.call("screen.cells", rect=[x + 1, y + 1, 12, 1], sprites=True)
        assert {run["fg"] for run in rows["rows"][0]} == {INKS[0][0]}
    thread.join()
    assert status == [0]


def test_hover(hosted: tuple[Any, Pilot]) -> None:
    """The host mouse on the map: the line there is told beside it,
    and taken away when the mouse leaves."""
    host, pilot = hosted
    pilot.send("printf 'alpha %s\\n' one two three\n")
    pilot.wait_for("alpha three")
    argv = ["-s", pilot.session, "--prefer", "top-right"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(minimap.main(argv, seconds=60))
    )
    thread.start()
    try:
        x, y, _, _ = widget_of(pilot)["rect"]

        def tips() -> list[str]:
            return [line for line in pilot.shown() if "alpha" in line and "│" in line]

        deadline = time.monotonic() + 5
        while not tips():
            assert time.monotonic() < deadline, pilot.shown()
            host.move(x + 2, y + 1)
            time.sleep(0.05)
        host.move(x + 2, y)  # the border is not the map
        while tips():
            assert time.monotonic() < deadline, pilot.shown()
            time.sleep(0.05)
        while not tips():
            assert time.monotonic() < deadline, pilot.shown()
            host.move(x + 2, y + 1)
            time.sleep(0.05)
        assert " $ printf 'alpha %s\\n' one two three " in tips()[0]
        while tips():
            assert time.monotonic() < deadline, pilot.shown()
            host.move(2, 20)
            time.sleep(0.05)
    finally:
        thread.join()
    assert status == [0]
