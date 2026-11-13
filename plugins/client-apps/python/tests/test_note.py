# SPDX-License-Identifier: GPL-3.0-or-later
"""`note`: the reduced markdown, every variant in its box, a file
read again when it changes."""

from __future__ import annotations

import os
import threading
import time
import unicodedata
from pathlib import Path

import pytest
from ptyrc import Pilot, clock
from ptyrc_apps import note

TEXT = """# Groceries

- [x] bread


- [ ] milk, the **whole** one, from the [shop](http://x) at the corner
* [X] eggs
- [ ] coffee
  - [ ] filters

## Later ##

1. call the `plumber`
> quoted
plain text

"""


class Skip:
    """The session clock, for the app alone: `end` jumps past the
    `seconds` it was given."""

    def __init__(self) -> None:
        self.by = 0.0

    def monotonic(self) -> float:
        return clock.monotonic() + self.by

    def end(self) -> None:
        self.by = 1e6


def text(rows: list[list[dict[str, object]]]) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


def made(source: str = TEXT, name: str = "TODO.md") -> note.Note:
    return note.Note(name, tuple(note.parse(source)))


def test_parse() -> None:
    assert [(i.kind, i.text, i.indent) for i in note.parse(TEXT)] == [
        ("head", "Groceries", 0),
        ("blank", "", 0),
        ("done", "bread", 0),
        ("blank", "", 0),
        ("open", "milk, the whole one, from the shop at the corner", 0),
        ("done", "eggs", 0),
        ("open", "coffee", 0),
        ("open", "filters", 1),
        ("blank", "", 0),
        ("head", "Later", 0),
        ("blank", "", 0),
        ("bullet", "call the plumber", 0),
        ("text", "quoted", 0),
        ("text", "plain text", 0),
    ]
    assert note.parse("") == [] and note.parse("\n\n") == []
    assert note.tally(made()) == (2, 5) and note.tally(made("words")) == (0, 0)


def test_wrap() -> None:
    assert note.wrap("one two three", 7) == ["one two", "three"]
    assert note.wrap("abcdefghij kl", 4) == ["abcd", "efgh", "ij", "kl"]
    assert note.wrap("", 5) == [""]


def test_one_line() -> None:
    n = made()
    assert text(note.render(n, 5, 1)) == ["2/5"]
    assert text(note.render(n, 18, 1)) == ["2/5 milk, the who…"]
    done = made("- [x] a\n- [x] b\n")
    assert text(note.render(done, 18, 1)) == ["2/2 all done"]
    assert text(note.render(made("# Title\nwords"), 18, 1)) == ["Title"]
    assert text(note.render(made(""), 18, 1)) == ["empty"]
    gone = note.Note("x.md", missing=True)
    assert text(note.render(gone, 18, 3)) == ["no x.md"]
    assert text(note.render(gone, 5, 1)) == ["—"]


def test_fits_or_not() -> None:
    n = made()
    full = text(note.render(n, 48, 16))
    assert full[0] == "Groceries" and full[2] == "☑ bread"
    assert full[4:6] == ["☐ milk, the whole one, from the shop at the", "  corner"]
    assert "  ☐ filters" in full and full[-3:] == [
        "• call the plumber",
        "quoted",
        "plain text",
    ]
    # Too long: without what is done.
    less = text(note.render(n, 48, 12))
    assert "☑ bread" not in less and "Later" in less and len(less) == 12
    # Shorter yet: the open boxes alone; then cut.
    assert text(note.render(n, 30, 6)) == [
        "☐ milk, the whole one, from",
        "  the shop at the corner",
        "☐ coffee",
        "  ☐ filters",
    ]
    assert text(note.render(n, 24, 3)) == [
        "☐ milk, the whole one,",
        "  from the shop at the",
        "  corner …",
    ]
    assert text(note.render(made("a\nb\nc\nd"), 10, 2)) == ["a", "b …"]


def test_looks() -> None:
    n = made()
    plain = note.render(n, 48, 16)
    assert plain[0] == [{"t": "Groceries", "bold": True}] and plain[2] == [
        {"t": "☑ bread"}
    ]
    assert note.render(n, 48, 16, note.Look(done=True))[2][0]["fg"] == "bright-black"
    hidden = text(note.render(n, 48, 16, note.Look(open=True)))
    assert not any("☑" in row for row in hidden) and "☐ coffee" in hidden
    counted = text(note.render(n, 20, 6, note.Look(count=True)))
    assert counted[0] == "━━━━━━╾───────── 2/5" and counted[1].startswith("☐ milk")
    assert note.title(n, note.Look(count=True)) == "TODO.md 2/5"
    assert note.title(n) == note.title(made("x"), note.Look(count=True)) == "TODO.md"
    paper = note.render(n, 30, 6, note.Look(postit=True, done=True))
    for row in paper:
        assert sum(len(r["t"]) for r in row) == 30
        assert all(r["bg"] == note.PAPER for r in row)
    assert (
        note.POSTIT["style"] == {"fg": note.PAPER} and len(note.POSTIT["corners"]) == 4
    )


@pytest.mark.parametrize(
    "look",
    [note.Look(), note.Look(count=True, done=True, postit=True), note.Look(open=True)],
)
def test_every_variant(look: note.Look) -> None:
    sources = (TEXT, "", "word " * 80, "- [x] a\n", "x" * 300, "- [ ] " + "y" * 90)
    for n in [*map(made, sources), note.Note("gone", missing=True)]:
        for w, h in [*note.SIZES, (1, 1), (3, 2), (7, 3), (120, 40)]:
            rows = note.render(n, w, h, look)
            assert 0 < len(rows) <= h, (w, h)
            for row in text(rows):
                assert len(row) <= w, (w, h, row)
                assert all(unicodedata.east_asian_width(c) != "W" for c in row)
        sizes = note.trimmed(n, look)
        assert [w for w, _ in sizes] == [w for w, _ in note.SIZES]
        assert all(1 <= h <= s[1] for (_, h), s in zip(sizes, note.SIZES, strict=True))
        assert len(note.named(sizes)) == len(note.SIZES)


def test_box() -> None:
    n = made()
    look = note.Look(postit=True, count=True)
    lines = note.box(
        note.title(n, look), note.render(n, 24, 3, look), 24, 3, look, "round", False
    )
    assert lines[0] == "█ TODO.md 2/5 ███████████◣" and lines[-1] == "▀" * 26
    assert all(len(line) == 26 and line[0] == line[-1] == "█" for line in lines[1:-1])
    lines = note.box(
        "TODO.md", note.render(n, 18, 1), 18, 1, note.Look(), "round", False
    )
    assert lines == [
        "╭" + "─" * 18 + "╮",
        "│2/5 milk, the who…│",
        "╰───────── TODO.md ╯",
    ]
    coloured = note.box("t", note.render(n, 18, 1, look), 18, 1, look, "round", True)
    assert f"\x1b[48;5;{note.PAPER}m" in coloured[1]


def test_options(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    args = note.options([])
    assert args.files == [Path("TODO.md")] and args.look == note.Look()
    assert (args.border, args.merged, args.print) == ("round", False, None)
    assert note.options(["-a"]).look == note.Look(count=True, done=True, postit=True)
    args = note.options(["a.md", "b.md", "-o", "--merged", "--print", "30x4"])
    assert args.files == [Path("a.md"), Path("b.md")] and args.look.open
    assert args.merged and args.print == (30, 4)
    with pytest.raises(SystemExit):
        note.options(["--cycle", "0"])
    capsys.readouterr()
    path = tmp_path / "TODO.md"
    path.write_text(TEXT)
    monkeypatch.chdir(tmp_path)
    assert note.main(["--print"]) == 0
    out = capsys.readouterr().out
    assert out.count("╭") == len(note.SIZES) and "─ TODO.md ╯" in out
    assert note.main(["--print", "20x2", "-p", "nope.md"]) == 0
    assert "█no nope.md" in capsys.readouterr().out
    assert path.read_text() == TEXT


def test_live(session: Pilot, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A widget a file; a file written is read again, and never
    written by the app."""
    monkeypatch.setattr(note, "clock", skip := Skip())
    one, two = tmp_path / "TODO.md", tmp_path / "ideas.md"
    one.write_text("- [ ] first thing\n- [x] second\n")
    two.write_text("an idea\n")
    seen: dict[str, object] = {}
    argv = ["-s", session.session, "-c", str(one), str(two)]
    thread = threading.Thread(
        target=lambda: seen.update(status=note.main(argv, seconds=600))
    )
    thread.start()

    def until(what: str, test: object) -> None:
        deadline = time.monotonic() + 8
        while not test():  # type: ignore[operator]
            assert time.monotonic() < deadline and thread.is_alive(), what
            time.sleep(0.03)

    def titles() -> set[object]:
        return {w["title"] for w in session.widgets() if w["state"] == "placed"}

    def shown() -> str:
        return "\n".join(session.dump()["host"])

    try:
        until("placed", lambda: titles() == {"TODO.md 1/2", "ideas.md"})
        until("drawn", lambda: "first thing" in shown() and "an idea" in shown())
        stamp = one.stat().st_mtime_ns
        one.write_text("- [x] first thing\n- [x] second\n- [ ] a third, new\n")
        os.utime(one, ns=(stamp + 10**9, stamp + 10**9))
        until(
            "read again",
            lambda: "TODO.md 2/3" in titles() and "a third, new" in shown(),
        )
        two.unlink()
        until("gone", lambda: "no ideas.md" in shown())
    finally:
        skip.end()
        thread.join()
    assert seen == {"status": 0}
    assert one.read_text() == "- [x] first thing\n- [x] second\n- [ ] a third, new\n"


def test_live_merged(session: Pilot, tmp_path: Path) -> None:
    one, two = tmp_path / "a.md", tmp_path / "b.md"
    one.write_text("- [ ] alpha\n")
    two.write_text("beta\n")
    seen: dict[str, object] = {}
    argv = [
        "-s",
        session.session,
        "--merged",
        "--cycle",
        "0.5",
        "-p",
        str(one),
        str(two),
    ]
    thread = threading.Thread(
        target=lambda: seen.update(status=note.main(argv, seconds=5))
    )
    thread.start()
    titles: set[object] = set()
    tags: set[object] = set()
    while thread.is_alive():
        widgets = session.widgets()
        titles.update(w["title"] for w in widgets)
        tags.update(w["tag"] for w in widgets)
        time.sleep(0.02)
    thread.join()
    assert seen == {"status": 0} and titles == {"a.md", "b.md"} and tags == {"note"}
