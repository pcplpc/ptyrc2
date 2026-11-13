# SPDX-License-Identifier: GPL-3.0-or-later
"""`gallery`: the picture format, the collection, the wall."""

from __future__ import annotations

import threading
from pathlib import Path

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import _catalog, gallery

PEAR = """\
# a comment
name: Pear
author: someone
kind: still
period: 1
hold: 3 1
eyes: <o>
ink: g=70 y=220/52 w=255! s=/17
== 5x3
 ,
(o)
 "
::
 g
y@y
 w
++
 ,
(-)
== 3x1
(o)
::
y@y
++
(-)
"""


def text(rows: gallery.Rows) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


def test_parse() -> None:
    pear = gallery.parse(PEAR, "pear.pic")
    assert (pear.name, pear.author, pear.kind) == ("Pear", "someone", "still")
    assert (pear.period, pear.hold, pear.eyes) == (1.0, (3, 1), "<o>")
    assert pear.inks["y"] == (220, 52, False)
    assert pear.inks["w"] == (255, None, True)
    assert pear.inks["s"] == (None, 17, False)
    assert list(pear.sizes) == [(5, 3), (3, 1)]
    assert pear.frames == 2 and pear.stem == "pear"
    assert text(pear.render(5, 3)) == [" ,   ", "(o)  ", ' "   ']
    # No `::`: the ink of the frame before.
    assert pear.sizes[5, 3][1].ink == pear.sizes[5, 3][0].ink
    assert pear.label() == "Pear" and pear.label(True) == "Pear · someone"


def test_render_runs() -> None:
    pear = gallery.parse(PEAR)
    rows = pear.render(5, 3)
    assert rows[1][:3] == [
        {"t": "(", "fg": 220, "bg": 52},
        {"t": "o"},
        {"t": ")", "fg": 220, "bg": 52},
    ]
    assert rows[2][1] == {"t": '"', "fg": 255, "bold": True}
    assert rows == pear.render(5, 3)  # pure
    with pytest.raises(ValueError, match="no size 4x4"):
        pear.render(4, 4)


def test_frames_and_hold() -> None:
    pear = gallery.parse(PEAR)
    assert [pear.frame_at(t) for t in (0, 1, 2.9, 3, 3.9, 4, 7)] == [
        0,
        0,
        0,
        1,
        1,
        0,
        1,
    ]
    assert text(pear.render(3, 1, 3.5)) == ["(-)"]
    still = gallery.parse("name: x\n== 1x1\n#\n")
    assert still.frame_at(99) == 0 and still.kind == "abstract"
    assert still.author == "anonymous"


def test_eyes() -> None:
    pear = gallery.parse(PEAR)
    assert text(pear.render(3, 1, look=(0, 0))) == ["(<)"]
    assert text(pear.render(3, 1, look=(1, 0))) == ["(o)"]
    assert text(pear.render(3, 1, look=(2, 0))) == ["(>)"]
    # The frame shown has no eye: nothing turns.
    owl = gallery.load(gallery.HERE / "owl.pic")
    assert "◐" in text(owl.render(11, 7, 0, look=(0, 2)))[2]
    assert text(owl.render(11, 7, 3.7, look=(0, 2)))[2] == "  █━ ▼ ━█  "
    five = gallery.parse("name: x\neyes: <o>^v\n== 3x3\n\n o\n::\n\n @\n")
    assert five.gaze(3, 3, (1, 0)) == {(1, 1): "^"}
    assert five.gaze(3, 3, (1, 2)) == {(1, 1): "v"}
    assert five.gaze(3, 3, (0, 1)) == {(1, 1): "<"}
    assert five.gaze(3, 3, None) == {(1, 1): "o"}


@pytest.mark.parametrize(
    ("source", "what"),
    [
        ("== 1x1\nx\n", "no name"),
        ("name: x\n", "no size"),
        ("name: x\nkind: cubist\n== 1x1\n", "kind 'cubist'"),
        ("name: x\ncolour: 3\n== 1x1\n", "p:2: not a header line"),
        ("name: x\nperiod: soon\n== 1x1\n", "p:2: period"),
        ("name: x\nink: a=300\n== 1x1\n", "colour '300'"),
        ("name: x\nink: ab=3\n== 1x1\n", "ink 'ab=3'"),
        ("name: x\n== 2x1\nabc\n", "p:3: a row of glyphs of 3 cells in a 2x1"),
        ("name: x\n== 2x1\na\nb\n", "p:4: 2 rows of glyphs in a 2x1"),
        ("name: x\n== 2x1\n中\n", "is not one cell wide"),
        ("name: x\n== 2x1\na\n::\nq\n", "p:5: ink 'q' not in `ink:`"),
        ("name: x\n== 2x1\na\n::\n@\n", "p:5: `@`: no `eyes:`"),
        ("name: x\n== 2x1\na\n::\n\n::\n", "`::` twice"),
        ("name: x\n== 1x1\na\n== 1x1\nb\n", "given twice"),
        ("name: x\n== 1x1\na\n++\nb\n== 2x1\na\n", "want one count"),
        ("name: x\nhold: 1 2\n== 1x1\na\n", "hold: one number a frame"),
        ("name: x\neyes: ab\n== 1x1\na\n", "eyes: want 3 or 5"),
    ],
)
def test_errors(source: str, what: str) -> None:
    with pytest.raises(gallery.PictureError, match="p") as caught:
        gallery.parse(source, "p")
    assert what in str(caught.value)


def test_load_errors(tmp_path: Path) -> None:
    with pytest.raises(gallery.PictureError, match="nowhere.pic"):
        gallery.load(tmp_path / "nowhere.pic")
    with pytest.raises(gallery.PictureError, match="not a directory"):
        gallery.collection(dirs=[tmp_path / "nowhere"])


PICTURES = sorted(gallery.HERE.glob("*.pic"))


def test_collection_shipped() -> None:
    pictures = gallery.collection(candidates=True)
    assert len(pictures) == len(PICTURES) >= 50
    assert {p.kind for p in pictures} == set(gallery.KINDS)
    assert len({p.name for p in pictures}) == len(pictures)
    assert any(p.frames > 1 for p in pictures)
    assert any(len(p.sizes) > 1 for p in pictures)
    assert any(p.eyes for p in pictures)
    animals = gallery.collection(["animal"], candidates=True)
    assert animals and {p.kind for p in animals} == {"animal"}


@pytest.mark.parametrize("path", PICTURES, ids=lambda p: p.stem)
def test_picture(path: Path) -> None:
    """Every picture shipped parses, fills its sizes, inks what it draws."""
    assert path.read_text(encoding="utf-8").startswith("# SPDX-License-Identifier:")
    picture = gallery.load(path)
    assert picture.frames <= 6 and picture.period >= 0.2
    for (w, h), frames in picture.sizes.items():
        assert 3 <= w <= 40 and 1 <= h <= 14
        for nth, frame in enumerate(frames):
            t = (
                sum((picture.hold or (1,) * picture.frames)[:nth]) + 0.5
            ) * picture.period
            assert picture.frame_at(t) == nth
            rows = picture.render(w, h, t, look=(0, 0))
            assert len(rows) == h
            assert all(len(row) == w for row in text(rows))
            for glyphs, keys in zip(frame.rows, frame.ink, strict=True):
                for glyph, key in zip(glyphs, keys, strict=True):
                    bg = picture.inks.get(key, (None, None, False))[1]
                    assert glyph == " " or key not in " .", (nth, glyphs)
                    assert glyph != " " or key in " ." or bg is not None, (nth, keys)


def test_ship_list(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    assert gallery.shipped(tmp_path / "none") is None
    ship = tmp_path / "SHIPPED"
    ship.write_text("# best first\nowl\n\npear-and-fly\nnot-there\n")
    assert gallery.shipped(ship) == ["owl", "pear-and-fly", "not-there"]
    monkeypatch.setattr(gallery, "SHIP", ship)
    assert [p.stem for p in gallery.collection()] == ["owl", "pear-and-fly"]
    assert len(gallery.collection(candidates=True)) == len(PICTURES)
    # A file or a directory named: the ship list is not asked.
    (tmp_path / "pear.pic").write_text(PEAR)
    assert [p.name for p in gallery.collection(dirs=[tmp_path])] == ["Pear"]
    assert [p.name for p in gallery.collection(files=[tmp_path / "pear.pic"])] == [
        "Pear"
    ]
    assert gallery.collection(["silly"], dirs=[tmp_path]) == []


def test_hall() -> None:
    pictures = gallery.collection(candidates=True)[:6]
    hall = gallery.Hall(pictures, count=3, every=60, seed=1)
    again = gallery.Hall(pictures, count=3, every=60, seed=1)
    assert [s.picture for s in hall.slots] == [s.picture for s in again.slots]
    assert [s.until for s in hall.slots] == [20, 40, 60]  # staggered
    assert hall.due == 20 and hall.at(19) == []
    seen = []
    for t in range(20, 400, 20):
        for slot in hall.at(t):
            seen.append(slot.picture)
            assert slot.since == t and slot.until == t + 60
        hung = [s.picture for s in hall.slots]
        assert len(set(map(id, hung))) == 3  # never twice the same at once
    assert {id(p) for p in seen} == {id(p) for p in pictures}
    # Fewer pictures than nails: fewer nails; one picture stays.
    alone = gallery.Hall(pictures[:1], count=3, every=1)
    assert len(alone.slots) == 1 and alone.at(5) == []
    with pytest.raises(ValueError, match="no picture"):
        gallery.Hall([])


def test_hall_period() -> None:
    pear = gallery.parse(PEAR)
    assert gallery.Hall([pear]).period() is None
    hall = gallery.Hall([pear], moving=True)
    assert hall.period() == 1.0
    slot = hall.slots[0]
    hall.at(3.5)
    assert text(slot.render(3, 1)) == ["(-)"]
    assert text(gallery.Hall([pear]).slots[0].render(3, 1)) == ["(o)"]


def test_slot_hover() -> None:
    pear = gallery.parse(PEAR)
    slot = gallery.Slot(pear, hover=True, size=(3, 1))
    titles: list[str] = []
    slot.retitle = lambda: titles.append(slot.label)
    assert slot.label == "Pear"
    assert slot.on_move(0, 0) is True
    assert text(slot.render(3, 1)) == ["(<)"] and titles == ["Pear · someone"]
    assert slot.on_move(0, 0) is False  # nothing new to see
    assert slot.on_move(2, 0) is True and text(slot.render(3, 1)) == ["(>)"]
    assert slot.on_leave() is True
    assert text(slot.render(3, 1)) == ["(o)"] and titles[-1] == "Pear"
    assert slot.on_leave() is False
    # Without `--hover` the pointer changes nothing.
    deaf = gallery.Slot(pear, size=(3, 1))
    assert deaf.on_move(0, 0) is False and text(deaf.render(3, 1)) == ["(o)"]
    assert gallery.Slot(pear, author=True).label == "Pear · someone"


def test_framed() -> None:
    pear = gallery.parse(PEAR)
    assert gallery.framed(pear.render(5, 3), 5) == [
        "╔═╤═╤═╗",
        "║ ,   ║",
        "╟(o)  ╢",
        '║ "   ║',
        "╚═╧═╧═╝",
    ]
    out = gallery.framed(pear.render(5, 3), 5, "P", colour=True)
    assert "\x1b[38;5;220;48;5;52m(" in out[2] and " P " in out[-1]


def test_print(capsys: pytest.CaptureFixture[str], tmp_path: Path) -> None:
    assert gallery.main(["--print", "--candidates"]) == 0
    out = capsys.readouterr().out
    assert "owl  animal  11x7" in out and "\x1b" not in out
    (tmp_path / "pear.pic").write_text(PEAR)
    argv = ["--file", str(tmp_path / "pear.pic"), "--colour", "-a"]
    assert gallery.main(["--print", "4x2", *argv]) == 0
    out = capsys.readouterr().out
    assert "pear  still  3x1" in out and "5x3" not in out
    assert out.count("╔") == 2 and "\x1b[" in out  # -m: both frames
    (tmp_path / "bad.pic").write_text("name: x\n")
    with pytest.raises(SystemExit):
        gallery.main(["--print", "--file", str(tmp_path / "bad.pic")])
    assert "no size" in capsys.readouterr().err
    with pytest.raises(SystemExit):
        gallery.main(["--print", "--kind", "cubist"])


def test_catalog() -> None:
    actions = _catalog.parser_of("gallery")._actions
    flags = {flag for action in actions for flag in action.option_strings}
    assert {"-a", "-m", "-l", "-o", "-w", "--every", "--kind", "--print"} <= flags


def test_gallery_live(session: Pilot, tmp_path: Path) -> None:
    """One picture hung, in its frame, named; the next takes its place."""
    for name in ("One", "Two"):
        (tmp_path / f"{name}.pic").write_text(
            f"name: {name}\nink: a=220\n== 24x3\n\n {name.upper()}\n::\n\n aaa\n"
        )
    argv = ["-s", session.session, "--dir", str(tmp_path), "--every", "1", "-a"]
    status: list[int] = []
    run = threading.Thread(
        target=lambda: status.append(gallery.main(argv, seconds=4.0))
    )
    run.start()
    seen: set[str] = set()
    plates: set[str] = set()
    while run.is_alive():
        host = "\n".join(session.dump()["host"])
        seen |= {name for name in ("ONE", "TWO") if name in host}
        plates |= {p for p in ("One · anonymous", "Two · anonymous") if p in host}
        if "ONE" in host or "TWO" in host:
            assert "╔" in host
        session_clock.sleep(0.02)
    run.join()
    assert status == [0]
    assert seen == {"ONE", "TWO"} and plates
    assert session.widgets() == []
