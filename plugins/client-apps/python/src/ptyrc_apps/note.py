# SPDX-License-Identifier: GPL-3.0-or-later
"""Note: a file as a sticky note; with `-i`, one to write on.

    ptyrc-app note                     # TODO.md of the working directory
    ptyrc-app note TODO.md ideas.md    # a widget a file
    ptyrc-app note -a --prefer top-right
    ptyrc-app note a.md b.md --merged  # one widget, the files in turn
    ptyrc-app note -i                  # ctrl+t twice: tick, add, edit
    ptyrc-app note --print [WxH]       # no driver: every variant

Markdown reduced to what a note needs (`parse`): `- [ ]` and `- [x]`
are boxes, `-` and `*` bullets, `#` a heading; emphasis and quote
marks go.
The variants, from `3/7` in five cells to the note wrapped: when the
note does not fit, without what is done; then the open boxes alone;
then cut, with `…`.

`-c`: the boxes ticked over all of them, in the title and as a bar;
`-x`: what is done dimmed; `-o`: what is done left out; `-p`: a
yellow ground and the border `POSTIT`; `-a`: `-c -x -p`.

`-i`: the notes are quiet interactive widgets (`Deck.interact`).
Focused, a note is a list, an item a row, with a bar (`Desk`): `space`
or `enter` ticks a box, `a` adds one under the bar, `e` edits the
words, `d` then `y` deletes, `shift+↑↓` (`K`, `J`) moves the item;
ctrl+x closes the note, the app ends with the last.
Each change is one of a line of the file (`change`), written at once
by a rename (`edit`); the other lines are kept as they are. A file
that changed since it was read is read again, the key is lost. With
`--merged` the turn stands still while the note is focused.

A file is read again when its mtime changes, looked at each second.
`render(note, w, h, look, desk)` is pure: rows of styled runs.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from ptyrc import Pilot, clock, ui

from . import _cli, _widgets
from .monitor import BORDERS, DIM, _seconds, _size, bar, clip, sgr, width

__all__ = [
    "PAPER",
    "PLAIN",
    "POSTIT",
    "SIZES",
    "Desk",
    "Item",
    "Look",
    "Note",
    "box",
    "change",
    "edit",
    "listed",
    "live",
    "main",
    "named",
    "options",
    "parse",
    "read",
    "render",
    "tally",
    "title",
    "trimmed",
    "wrap",
]

type Run = dict[str, Any]
type Row = list[Run]
type Rows = list[Row]

SIZES = ((5, 1), (18, 1), (24, 3), (30, 6), (40, 10), (48, 16))
#: The ground of a post-it, its ink, its faded ink.
PAPER, INK, FADED = 221, 235, 136
#: A border of blocks in the colour of the paper, the top right corner
#: folded, the title on the top edge.
POSTIT: dict[str, Any] = {
    "corners": "█◣▀▀",
    "top": "█",
    "bottom": "▀",
    "left": "█",
    "right": "█",
    "style": {"fg": PAPER},
    "title": {
        "side": "top",
        "align": "left",
        "style": {"fg": INK, "bg": PAPER, "bold": True},
    },
}
BOX, TICKED, BULLET = "☐", "☑", "•"


@dataclass(frozen=True)
class Item:
    """A line of a note. `kind`: `head`, `open`, `done`, `bullet`,
    `text` or `blank`; `indent`: its depth in a list. `line`: its
    line in the file, from 0; `raw`: its words as written there."""

    kind: str
    text: str = ""
    indent: int = 0
    line: int = field(default=-1, compare=False)
    raw: str = field(default="", compare=False)


@dataclass(frozen=True)
class Note:
    name: str
    items: tuple[Item, ...] = ()
    missing: bool = False


@dataclass(frozen=True)
class Look:
    """`count`: the bar of the boxes; `done`: what is done dimmed;
    `open`: what is done left out; `postit`: the yellow ground."""

    count: bool = False
    done: bool = False
    open: bool = False
    postit: bool = False


#: The bare look.
PLAIN = Look()

_BOX = re.compile(r"^(\s*)(?:[-*+]|\d+[.)])\s+\[([ xX])\]\s*(.*)$")
_BULLET = re.compile(r"^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$")
_HEAD = re.compile(r"^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$")
_MARKS = re.compile(r"(\*\*|__|~~|`)")
_LINK = re.compile(r"\[([^\]]+)\]\([^)]*\)")


def _plain(text: str) -> str:
    """`text` without emphasis marks; a link is its words."""
    return _MARKS.sub("", _LINK.sub(r"\1", text)).strip()


def parse(text: str) -> list[Item]:
    """The lines of a note; runs of blank lines are one, none at the
    ends."""
    out: list[Item] = []
    for n, line in enumerate(text.expandtabs(4).splitlines()):
        if not line.strip():
            if out and out[-1].kind != "blank":
                out.append(Item("blank", line=n))
        elif m := _BOX.match(line):
            kind = "open" if m[2] == " " else "done"
            out.append(Item(kind, _plain(m[3]), len(m[1]) // 2, n, m[3]))
        elif m := _BULLET.match(line):
            out.append(Item("bullet", _plain(m[2]), len(m[1]) // 2, n, m[2]))
        elif m := _HEAD.match(line):
            out.append(Item("head", _plain(m[1]), 0, n, m[1]))
        else:
            words = line.lstrip().removeprefix(">")
            out.append(Item("text", _plain(words), 0, n, words.strip()))
    while out and out[-1].kind == "blank":
        out.pop()
    return out


def read(path: Path) -> Note:
    """The note of `path`; `missing` when it cannot be read."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return Note(path.name, missing=True)
    return Note(path.name, tuple(parse(text)))


def tally(note: Note) -> tuple[int, int]:
    """Boxes ticked, boxes."""
    done = sum(i.kind == "done" for i in note.items)
    return done, done + sum(i.kind == "open" for i in note.items)


def title(note: Note, look: Look = PLAIN) -> str:
    done, boxes = tally(note)
    return f"{note.name} {done}/{boxes}" if look.count and boxes else note.name


def wrap(text: str, w: int) -> list[str]:
    """`text` in lines of `w` cells, cut at spaces; a longer word is
    cut."""
    lines = [""]
    for word in text.split():
        while len(word) > w > 0:
            if lines[-1]:
                lines.append("")
            lines[-1], word = word[:w], word[w:]
            lines.append("")
        if lines[-1] and len(lines[-1]) + 1 + len(word) > w:
            lines.append("")
        lines[-1] += (" " if lines[-1] else "") + word
    return [line for line in lines if line] or [""]


def _style(kind: str, look: Look) -> dict[str, Any]:
    faded = {"fg": FADED} if look.postit else {"fg": DIM}
    base: dict[str, Any] = {"fg": INK, "bg": PAPER} if look.postit else {}
    if kind == "head":
        return base | {"bold": True}
    if kind == "done" and look.done:
        return base | faded
    if kind == "faded":
        return base | faded
    return base


def _lines(items: Sequence[Item], w: int, look: Look) -> Rows:
    """The items laid out: a mark, the text wrapped under itself."""
    rows: Rows = []
    for item in items:
        if item.kind == "blank":
            rows.append([])
            continue
        mark = {"open": BOX, "done": TICKED, "bullet": BULLET}.get(item.kind, "")
        lead = "  " * min(item.indent, 2) + (mark + " " if mark else "")
        room = max(4, w - len(lead))
        style = _style(item.kind, look)
        for n, line in enumerate(wrap(item.text, room)):
            rows.append([{"t": (lead if n == 0 else " " * len(lead)) + line} | style])
    return rows


def _ground(rows: Rows, w: int, look: Look) -> Rows:
    """Each row cut to `w`; on the paper, filled to `w`."""
    out = []
    for row in rows:
        row = [r | {"t": clip(r["t"], w)} for r in row[:1]] + row[1:]
        if look.postit:
            row = [*row, {"t": " " * (w - width(row)), "bg": PAPER}]
        out.append(row)
    return out


def listed(note: Note, look: Look = PLAIN) -> list[Item]:
    """The items a focused note lists: no blank, and with `look.open`
    nothing done."""
    out = [i for i in note.items if i.kind != "blank"]
    return [i for i in out if not (look.open and i.kind == "done")]


#: A key of a move, and its step among the items.
_MOVES = {"shift+up": -1, "K": -1, "shift+down": 1, "J": 1}


class Desk:
    """The keys of a focused note. Pure: `key` answers the change to
    make to the file, `change` makes it.

    `sel`: the item under the bar, among `listed`. `mode`: None,
    `add`, `edit` (the line typed is `editor`) or `delete` (asked:
    `y` does it, any other key keeps)."""

    def __init__(self) -> None:
        self.sel = 0
        self.mode: str | None = None
        self.editor = ui.Editor()
        self._list = ui.Listing()
        self._view = ui.Listing()

    def rest(self) -> bool:
        """Leave what was under way (the focus was lost); whether
        something was."""
        mode, self.mode = self.mode, None
        return mode is not None

    def key(self, items: Sequence[Item], name: str) -> tuple[Any, ...] | None:
        """Take a key; `items`: those listed. None, or a change:
        `("tick", line)`, `("add", line, words)` (under that line; -1:
        at the end), `("words", line, words)`, `("delete", line)`,
        `("swap", line, line)`."""
        self.sel = max(0, min(len(items) - 1, self.sel))
        item = items[self.sel] if items else None
        if self.mode in ("add", "edit"):
            mode, words = self.mode, self.editor.text.strip()
            if name == "ctrl+c":
                self.mode = None
            elif name == "enter":
                self.mode = None
                if words and mode == "add":
                    self.sel += 1 if item else 0
                    return ("add", item.line if item else -1, words)
                if words and item and words != item.raw:
                    return ("words", item.line, words)
            else:
                self.editor.key(name)
            return None
        if self.mode == "delete":
            self.mode = None
            return ("delete", item.line) if name == "y" and item else None
        self._list.count, self._list.sel = len(items), self.sel
        if self._list.key(name):
            self.sel = self._list.sel
        elif name == "a":
            self.mode, self.editor = "add", ui.Editor()
        elif item is None:
            pass
        elif name in ("space", "enter") and item.kind in ("open", "done"):
            return ("tick", item.line)
        elif name == "e":
            self.mode, self.editor = "edit", ui.Editor(item.raw)
        elif name == "d":
            self.mode = "delete"
        elif name in _MOVES and 0 <= self.sel + _MOVES[name] < len(items):
            self.sel += _MOVES[name]
            return ("swap", item.line, items[self.sel].line)
        return None

    def paste(self, text: str) -> None:
        if self.mode in ("add", "edit"):
            self.editor.insert(" ".join(text.split("\n")).strip())

    def keys(self) -> list[tuple[str, int]]:
        """The foot: the keys of the mode, the least useful ranked
        highest."""
        if self.mode in ("add", "edit"):
            return [("enter ok", 0), ("^C cancel", 1)]
        if self.mode == "delete":
            return [("y delete", 0), ("any key keeps", 1)]
        return [
            ("space tick", 1),
            ("a add", 2),
            ("e edit", 3),
            ("d delete", 4),
            ("⇧↑↓ move", 5),
            ("^X close", 6),
            ("esc leave", 0),
        ]


def _desk(note: Note, w: int, h: int, look: Look, desk: Desk) -> Rows:
    """A focused note: an item a row, cut; the bar; under it the line
    being added; a foot of keys from 3 rows."""
    base = _style("text", look)
    faded = _style("faded", look)
    items = listed(note, look)
    desk.sel = max(0, min(len(items) - 1, desk.sel))
    # What each row is: an item, or None for the line added.
    entries: list[Item | None] = list(items)
    at = desk.sel
    if desk.mode == "add":
        at = desk.sel + 1 if items else 0
        entries.insert(at, None)
    feet = 1 if h >= 3 else 0
    view = desk._view
    view.count, view.sel = len(entries), at
    rows: Rows = []
    for n in view.window(h - feet):
        entry = entries[n]
        on = n == at
        near = entry or (items[desk.sel] if items else Item("open"))
        kind = "open" if entry is None else entry.kind
        mark = {"open": BOX, "done": TICKED, "bullet": BULLET}.get(kind, "")
        lead = "  " * min(near.indent, 2) + (mark + " " if mark else "")
        style = _style(kind, look)
        if on and desk.mode in ("add", "edit"):
            room = max(1, w - len(lead))
            row = [{"t": lead} | style, *desk.editor.render(room, True, **base)]
        else:
            text = ui.cut(lead + (entry.text if entry else ""), w)
            row = [{"t": text + " " * (w - ui.cells(text))} | style]
            if on:
                row = [row[0] | {"inverse": True}]
                if desk.mode == "delete":
                    row = [row[0] | {"bold": True}]
        rows.append(row)
    if not entries:
        rows.append([{"t": ui.cut("empty", w)} | faded])
    rows += [[] for _ in range(h - feet - len(rows))]
    if feet:
        rows.append([{"t": ui.foot(desk.keys(), w)} | faded])
    out = []
    for row in rows:
        row = [base | r if look.postit else r for r in ui.fit(row, w)]
        out.append(row)
    return out


def render(
    note: Note, w: int, h: int, look: Look = PLAIN, desk: Desk | None = None
) -> Rows:
    """`desk`: the note is focused, shown as the list the keys act
    on."""
    if desk is not None:
        return _desk(note, w, h, look, desk)
    base = _style("text", look)
    faded = _style("faded", look)
    if note.missing:
        text = next((t for t in (f"no {note.name}", "no file", "—") if len(t) <= w), "")
        return _ground([[{"t": text} | faded]], w, look)
    items = [i for i in note.items if not (look.open and i.kind == "done")]
    done, boxes = tally(note)
    if not items:
        text = "all done" if boxes else "empty"
        return _ground([[{"t": clip(text, w)} | faded]], w, look)
    if h == 1:
        first = next((i for i in items if i.kind == "open"), None)
        if not boxes:
            first = next(i for i in items if i.kind != "blank")
            return _ground([[{"t": clip(first.text, w)} | base]], w, look)
        row: Row = [{"t": f"{done}/{boxes}"} | base | {"bold": True}]
        text = first.text if first else "all done"
        if w - width(row) - 1 >= 6:
            style = base if first else faded
            row.append({"t": " " + clip(text, w - width(row) - 1)} | style)
        return _ground([row], w, look)
    top: Rows = []
    if look.count and boxes and h >= 3:
        n = max(3, w - len(f" {done}/{boxes}"))
        fg, rest = (INK, FADED) if look.postit else ("green", DIM)
        full = bar(done / boxes, n, fg)
        cells = [r | base | {"fg": fg if r["fg"] == fg else rest} for r in full]
        cells.append({"t": " " * (n - width(full))} | base)
        top = [[*cells, {"t": f" {done}/{boxes}"} | base | {"bold": True}]]
    room = h - len(top)
    rows = _lines(items, w, look)
    # Too long: without what is done; then what is left to do, alone.
    for keep in (("head", "open", "bullet", "text", "blank"), ("open",)):
        if len(rows) <= room or not boxes:
            break
        still: list[Item] = []
        for item in items:
            if item.kind in keep and not (
                item.kind == "blank" and (not still or still[-1].kind == "blank")
            ):
                still.append(item)
        while still and still[-1].kind in ("blank", "head"):
            still.pop()
        if any(i.kind == "open" for i in still):
            rows = _lines(still, w, look)
    if len(rows) > room:
        rows = rows[:room]
        last = rows[-1] or [{"t": ""} | base]
        text = last[0]["t"]
        rows[-1] = [last[0] | {"t": clip(text, w - 2).rstrip("…") + " …"}]
    return _ground([*top, *rows], w, look)


def trimmed(
    note: Note, look: Look = PLAIN, foot: bool = False
) -> list[tuple[int, int]]:
    """`SIZES` cut to the rows the note fills: one a size, two may be
    equal. `foot`: a row more from two rows on, for the keys of a
    focused note."""
    out = []
    for w, h in SIZES:
        rows = max(1, min(h, len(render(note, w, h, look))))
        out.append((w, rows + (1 if foot and h > 1 else 0)))
    return out


def named(sizes: Sequence[tuple[int, int]]) -> dict[str, tuple[int, int]]:
    """Variants by rank: as many at each declaration, two of one size
    allowed. A tag declared again with fewer, a variant past them
    still placed, is more than the driver takes."""
    return {f"v{n}": size for n, size in enumerate(sizes)}


# --print


def box(
    name: str, rows: Rows, w: int, h: int, look: Look, border: str, colour: bool
) -> list[str]:
    """`rows` as text in the border the driver would draw."""
    reset = "\x1b[0m" if colour else ""

    def ink(run: Run) -> str:
        if not colour:
            return run["t"]
        bg = f"\x1b[48;5;{run['bg']}m" if "bg" in run else ""
        return sgr(run) + bg + run["t"] + reset

    def body(row: Row) -> str:
        return "".join(ink(r) for r in row) + " " * (w - width(row))

    blank: Row = [{"t": " " * w, "bg": PAPER}] if look.postit else []
    lines = [body(row) for row in rows] + [body(blank)] * (h - len(rows))
    if look.postit:
        edge = {"fg": PAPER}
        label = f" {clip(name, w - 2)} " if w >= 5 else ""
        top = ink({"t": "█"} | edge) + ink({"t": label} | POSTIT["title"]["style"])
        top += ink({"t": "█" * (w - len(label)) + "◣"} | edge)
        side = ink({"t": "█"} | edge)
        return [
            top,
            *(side + line + side for line in lines),
            ink({"t": "▀" * (w + 2)} | edge),
        ]
    if border == "none":
        return lines
    tl, tr, bl, br, edge_, side_ = BORDERS.get(border, BORDERS["plain"])
    dim = (lambda t: ink({"t": t, "fg": DIM})) if colour else (lambda t: t)
    label = f" {clip(name, w - 2)} " if w >= 5 else ""
    return [
        dim(tl + edge_ * w + tr),
        *(dim(side_) + line + dim(side_) for line in lines),
        dim(bl + edge_ * (w - len(label))) + label + dim(br),
    ]


def show(args: argparse.Namespace) -> int:
    """`--print`: each note once, in its sizes cut to it, or in the
    size given."""
    colour = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    look: Look = args.look
    for path in args.files:
        note = read(path)
        for w, h in [args.print] if args.print else dict.fromkeys(trimmed(note, look)):
            rows = render(note, w, h, look)
            print(f"{note.name} {w}x{h}")
            print(
                "\n".join(box(title(note, look), rows, w, h, look, args.border, colour))
            )
            print()
    return 0


# Written


def _parts(line: str) -> tuple[str, str]:
    """A line of the file: what is written, its end."""
    body = line.rstrip("\r\n")
    return body, line[len(body) :]


def _lead(body: str) -> tuple[str, str]:
    """A line as what stands before its words, and them."""
    for pattern, group in ((_BOX, 3), (_BULLET, 2), (_HEAD, 1)):
        if m := pattern.match(body):
            return body[: m.start(group)], m[group]
    words = body.lstrip()
    if words.startswith(">"):
        words = words[1:].lstrip()
    return body[: len(body) - len(words)], words


def change(text: str, what: Sequence[Any]) -> str:
    """`text` after a change of `Desk.key`. Lines are those of
    `str.splitlines`; each keeps its end, its indent, its mark. A line
    added takes the indent and the list mark of the line it is under,
    as an open box."""
    lines = text.splitlines(keepends=True)
    op, at = what[0], what[1]
    body, end = _parts(lines[at]) if 0 <= at < len(lines) else ("", "\n")
    if op == "tick":
        m = _BOX.match(body)
        if m:
            mark = "x" if m[2] == " " else " "
            lines[at] = body[: m.start(2)] + mark + body[m.end(2) :] + end
    elif op == "words":
        lines[at] = _lead(body)[0] + what[2] + end
    elif op == "delete":
        del lines[at : at + 1]
    elif op == "swap":
        other, other_end = _parts(lines[what[2]])
        lines[at], lines[what[2]] = other + end, body + other_end
    elif op == "add":
        m = _BOX.match(body) or _BULLET.match(body)
        lead = body[: body.index(" ", len(m[1])) + 1] if m else "- "
        if not 0 <= at < len(lines):
            at = len(lines) - 1
            if lines and not _parts(lines[at])[1]:
                lines[at] += "\n"
        elif not end:
            lines[at], end = body + "\n", "\n"
        lines.insert(at + 1, f"{lead}[ ] {what[2]}{end or chr(10)}")
    return "".join(lines)


def edit(path: Path, note: Note, what: Sequence[Any]) -> bool:
    """Make a change of `Desk.key` in the file, by a rename. False,
    and nothing written: the file is not as `note` was read (the lines
    named differ), or cannot be written."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        text = ""
    except OSError:
        return False
    was = {i.line: i for i in note.items}
    now = {i.line: i for i in parse(text)}
    named = [n for n in what[1:] if isinstance(n, int) and n >= 0]
    if any(n not in now or now[n] != was.get(n) for n in named):
        return False
    try:
        fd, temp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as out:
            out.write(change(text, what))
        if path.exists():
            os.chmod(temp, path.stat().st_mode & 0o7777)
        os.replace(temp, path)
    except OSError:
        return False
    return True


# Live


def _mtime(path: Path) -> int:
    try:
        return path.stat().st_mtime_ns
    except OSError:
        return 0


def live(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    """A widget a file, or one for all with `args.merged`; a file is
    read again when it changed."""
    look: Look = args.look
    files: list[Path] = args.files
    notes = [read(path) for path in files]
    stamps = [_mtime(path) for path in files]
    deck = _widgets.Deck(pilot, args)
    extra = {"border": POSTIT} if look.postit else {}
    turn, since = 0, clock.monotonic()
    hands: bool = getattr(args, "interactive", False)
    widgets: list[_widgets.Widget] = []
    #: The notes closed by their key (ctrl+x): out of the deck.
    gone: set[int] = set()
    desks: list[Desk] = []

    def which(n: int) -> int:
        return turn % len(files) if args.merged else n

    def draw(n: int) -> _widgets.Render:
        def rows(w: int, h: int) -> Rows:
            desk = desks[n] if n < len(desks) and widgets[n].focused else None
            return render(notes[which(n)], w, h, look, desk)

        return rows

    def sizes(n: int) -> list[tuple[int, int]]:
        if not args.merged:
            return trimmed(notes[n], look, hands)
        every = [trimmed(note, look, hands) for note in notes]
        return [
            (
                max(s[min(r, len(s) - 1)][0] for s in every),
                max(s[min(r, len(s) - 1)][1] for s in every),
            )
            for r in range(max(map(len, every)))
        ]

    tags = (
        ["note"]
        if args.merged
        else ["note" if n == 0 else f"note-{n + 1}" for n in range(len(files))]
    )
    declared = [sizes(n) for n in range(len(tags))]
    widgets += [
        deck.add(
            tag,
            named(declared[n]),
            draw(n),
            title=title(notes[which(n)], look),
            **extra,
        )
        for n, tag in enumerate(tags)
    ]

    def again(m: int) -> None:
        """The file `m` as it is now; its widgets as it asks."""
        stamps[m], notes[m] = _mtime(files[m]), read(files[m])
        sync()

    def key(n: int) -> _widgets.Key:
        def take(name: str) -> None:
            m = which(n)
            what = desks[n].key(listed(notes[m], look), name)
            if what is not None:
                edit(files[m], notes[m], what)
                again(m)

        return take

    if hands:
        desks += [Desk() for _ in widgets]
        for n, widget in enumerate(widgets):
            deck.interact(
                widget, key(n), paste=desks[n].paste, closed=lambda n=n: gone.add(n)
            )

    def sync() -> None:
        for n, widget in enumerate(widgets):
            if n in gone:
                continue
            if (new := sizes(n)) != declared[n]:
                declared[n] = new
                deck.resize(widget, named(new))
            widget.set(title=title(notes[which(n)], look))

    end = None if seconds is None else clock.monotonic() + seconds
    due = clock.monotonic() + 1.0
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        if now >= due:
            due = now + 1.0
            changed = args.merged and len(files) > 1 and now - since >= args.cycle
            if changed and any(w.focused for w in widgets):
                # A focused note stays: the keys are for the one shown.
                changed, since = False, now
            if changed:
                turn, since = turn + 1, now
            for n, path in enumerate(files):
                if (stamp := _mtime(path)) != stamps[n]:
                    stamps[n], notes[n], changed = stamp, read(path), True
            if changed:
                sync()
                deck.draw()
        wait = due - now if end is None else min(due, end) - now
        event = pilot.event(timeout=max(0.0, wait))
        if event is None:
            continue
        if event[0] == "exit":
            return
        was = [w.focused for w in widgets]
        deck.handle(event)
        if hands and not deck.widgets:
            return
        for n, desk in enumerate(desks):
            # The focus lost: what was typed is dropped.
            if was[n] and not widgets[n].focused and desk.rest():
                deck.draw(widgets[n])


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("note", "A file as a sticky note; -i: one to write on.")
    out.add_argument(
        "files",
        nargs="*",
        type=Path,
        default=[Path("TODO.md")],
        metavar="FILE",
        help="the notes, a widget each [default: TODO.md]",
    )
    out.add_argument(
        "-c", "--count", action="store_true", help="the boxes ticked: title and bar"
    )
    out.add_argument("-x", "--done", action="store_true", help="what is done dimmed")
    out.add_argument("-o", "--open", action="store_true", help="what is done left out")
    out.add_argument(
        "-p", "--postit", action="store_true", help="a yellow ground, a folded corner"
    )
    out.add_argument("-a", "--all", action="store_true", help="-c -x -p")
    out.add_argument(
        "-i",
        "--interactive",
        action="store_true",
        help="ctrl+t twice: tick, add, edit, delete, move",
    )
    out.add_argument(
        "--merged", action="store_true", help="one widget, the notes in turn"
    )
    out.add_argument(
        "--cycle",
        type=_seconds,
        default=10.0,
        metavar="SECONDS",
        help="turn of --merged [default: 10]",
    )
    _widgets.options(out, border="round")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the variants once, or the size given",
    )
    args = _cli.parse(out, argv)
    args.look = Look(
        count=args.all or args.count,
        done=args.all or args.done,
        open=args.open,
        postit=args.all or args.postit,
    )
    return args


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: live(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
