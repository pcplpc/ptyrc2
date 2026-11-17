# SPDX-License-Identifier: GPL-3.0-or-later
"""Interactive widgets: questions asked in a widget of the session.

    from ptyrc import Pilot, ui

    with Pilot() as pilot:
        answer = ui.ask(pilot, ui.Choice("Deploy?", ["yes", "no", "later"]))

Three parts. A **model** is pure: keys in, rows of styled runs out, for
a given size; no clock, no I/O (`Choice`, `Line`, `Text`, `Form`; yours
on `Model`). The **runner** (`Dialog`, `ask`) shows a model in a widget
that asks for the keyboard and feeds it the keys once the user gave the
focus (ctrl+t twice). The front is the widget.

Keys are named as the driver names them: a printable character is
itself, then `space`, `enter`, `tab`, `shift+tab`, `backspace`, `up`,
`ctrl+a`, `alt+enter`... `ctrl+j` is the line feed. The escape key of
the widget never comes: the driver takes it and drops the focus.

Pieces for other models: `Editor` (a line and its cursor), `Listing` (a
bar on a list that scrolls), `foot`, `fit`, `cut`, `wrap`, `cells`; `Keys` (the `input` events of a
focused widget as keys and pastes).
"""

from __future__ import annotations

import base64
import re
import unicodedata
from collections.abc import Sequence
from typing import TYPE_CHECKING, Any

from . import clock
from ._wire import PtyrcError

if TYPE_CHECKING:
    from .pilot import Pilot
    from .widgets import Widget

__all__ = [
    "CANCEL",
    "DONE",
    "Choice",
    "Dialog",
    "Editor",
    "Form",
    "Keys",
    "Line",
    "Listing",
    "Model",
    "Text",
    "ask",
    "cells",
    "cut",
    "fit",
    "foot",
    "run",
    "wrap",
]

type Run = dict[str, Any]
type Row = list[Run]
type Rows = list[Row]
type Size = tuple[int, int]
#: A key of the foot and its rank: the highest is dropped first, 0 never.
type Key = tuple[str, int]

#: What `Model.key` answers when the model is over.
DONE, CANCEL = "done", "cancel"


# -- cells ------------------------------------------------------------------


def _wide(ch: str) -> int:
    if unicodedata.combining(ch) or unicodedata.category(ch) in ("Mn", "Me", "Cf"):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def cells(text: str) -> int:
    """Width of `text`, in cells."""
    return sum(map(_wide, text))


def _head(text: str, n: int) -> str:
    """The longest start of `text` in `n` cells."""
    used = 0
    for i, ch in enumerate(text):
        used += _wide(ch)
        if used > n:
            return text[:i]
    return text


def cut(text: str, n: int) -> str:
    """`text` in `n` cells at most; cut with `…`."""
    if cells(text) <= n:
        return text
    return _head(text, n - 1) + "…" if n > 0 else ""


def run(text: str, **style: Any) -> Run:
    """A run of a row: `fg`, `bg`, `bold`, `dim`, `inverse`."""
    return {"t": text, **{k: v for k, v in style.items() if v}}


def fit(row: Row | str, w: int) -> Row:
    """`row` in exactly `w` cells: cut, then padded with blanks. A wide
    glyph cut by the edge is a blank."""
    if isinstance(row, str):
        row = [run(row)]
    out: Row = []
    room = max(w, 0)
    for piece in row:
        text = _head(piece["t"], room)
        if text:
            out.append({**piece, "t": text})
            room -= cells(text)
        if text != piece["t"]:
            break
    if room or not out:
        out.append({"t": " " * room})
    return out


def wrap(text: str, w: int, n: int) -> list[str]:
    """`text` on `n` lines of `w` cells at most, cut at blanks; what is
    left over ends the last line with `…`."""
    lines: list[str] = []
    words = text.split()
    while words and len(lines) < n:
        line = words.pop(0)
        while words and cells(line) + 1 + cells(words[0]) <= w:
            line += " " + words.pop(0)
        lines.append(line)
    if words and lines:
        lines[-1] += " " + " ".join(words)
    return [cut(line, w) for line in lines]


def foot(keys: Sequence[Key], w: int) -> str:
    """The keys of a foot line in `w` cells: those of the highest rank
    go first, the last of them first; rank 0 stays, cut."""
    kept = list(keys)
    while True:
        line = "  ".join(k for k, _ in kept)
        worst = max((rank for _, rank in kept), default=0)
        if cells(line) <= w or worst == 0:
            return cut(line, w)
        at = max(i for i, (_, rank) in enumerate(kept) if rank == worst)
        del kept[at]


# -- keys -------------------------------------------------------------------

_MODIFIED = re.compile(rb"\x1b\[(\d+);([2-8])([A-DFH~])")
_LETTERS = {
    b"A": "up",
    b"B": "down",
    b"C": "right",
    b"D": "left",
    b"H": "home",
    b"F": "end",
}
_TILDES = {2: "insert", 3: "delete", 5: "pageup", 6: "pagedown"}


class Keys:
    """The `input` events of a focused widget, as keys and pastes.

    `feed(data)`, `data` that of an event: `("key", name)`, `("paste",
    text)` (its lines parted by `\\n`), or None: nothing yet, or a key
    without a name. A paste is an event `{"paste": text}` (a focused
    widget: one a read, as it comes), or the keys between the marks
    `START` and `END` (under `input.grab`), then given whole. `reset()`: the focus was lost, a paste under way is dropped.
    Pure: one instance a widget."""

    START, END = b"\x1b[200~", b"\x1b[201~"

    def __init__(self) -> None:
        self._paste: bytearray | None = None

    @staticmethod
    def name(raw: bytes) -> str | None:
        """The name of a key the driver does not name: an arrow,
        `home`, `end`, `insert`, `delete` or a page key held with
        modifiers (`CSI 1;m A`, `CSI n;m ~`): `shift+up`, `ctrl+left`,
        `ctrl+alt+shift+end`. None for any other bytes."""
        m = _MODIFIED.fullmatch(raw)
        if m is None:
            return None
        code, mods, last = int(m[1]), int(m[2]) - 1, m[3]
        key = _TILDES.get(code) if last == b"~" else _LETTERS[last]
        if key is None or (last != b"~" and code != 1):
            return None
        held = [n for bit, n in ((4, "ctrl"), (2, "alt"), (1, "shift")) if mods & bit]
        return "+".join([*held, key])

    def reset(self) -> None:
        self._paste = None

    def feed(self, data: dict[str, Any]) -> tuple[str, str] | None:
        if isinstance(data.get("paste"), str):
            text = data["paste"]
            return ("paste", text.replace("\r\n", "\n").replace("\r", "\n"))
        raw = base64.b64decode(data.get("bytes") or "")
        if self._paste is not None:
            if raw != self.END:
                self._paste += raw
                return None
            text = self._paste.decode("utf-8", "replace")
            self._paste = None
            return ("paste", text.replace("\r\n", "\n").replace("\r", "\n"))
        if raw == self.START:
            self._paste = bytearray()
            return None
        name = data.get("key") or self.name(raw)
        return None if name is None else ("key", name)


# -- pieces -----------------------------------------------------------------


class Editor:
    """A line of text and its cursor.

    `key`: `left`, `right`, `home`, `end`, `backspace`, `delete`,
    `ctrl+a`, `ctrl+e`, `ctrl+b`, `ctrl+f`, `ctrl+d`, `ctrl+h`, `ctrl+u`
    (to the start), `ctrl+k` (to the end), `ctrl+w` (the word before);
    a character is typed. False: not a key of it."""

    def __init__(self, text: str = "") -> None:
        self.text = text
        self.cursor = len(text)
        self._off = 0

    def insert(self, text: str) -> None:
        """Type `text` at the cursor; a control character is a blank."""
        text = "".join(" " if unicodedata.category(c) == "Cc" else c for c in text)
        self.text = self.text[: self.cursor] + text + self.text[self.cursor :]
        self.cursor += len(text)

    def key(self, name: str) -> bool:
        text, at = self.text, self.cursor
        if name in ("left", "ctrl+b"):
            self.cursor = max(0, at - 1)
        elif name in ("right", "ctrl+f"):
            self.cursor = min(len(text), at + 1)
        elif name in ("home", "ctrl+a"):
            self.cursor = 0
        elif name in ("end", "ctrl+e"):
            self.cursor = len(text)
        elif name in ("backspace", "ctrl+h"):
            self.text, self.cursor = text[: max(0, at - 1)] + text[at:], max(0, at - 1)
        elif name in ("delete", "ctrl+d"):
            self.text = text[:at] + text[at + 1 :]
        elif name == "ctrl+u":
            self.text, self.cursor = text[at:], 0
        elif name == "ctrl+k":
            self.text = text[:at]
        elif name == "ctrl+w":
            start = len(text[:at].rstrip())
            start = text.rfind(" ", 0, start) + 1
            self.text, self.cursor = text[:start] + text[at:], start
        elif name == "space":
            self.insert(" ")
        elif len(name) == 1 and name.isprintable():
            self.insert(name)
        else:
            return False
        return True

    def render(self, w: int, focused: bool, placeholder: str = "", **style: Any) -> Row:
        """The line in `w` cells. Focused: the cursor is an inverse
        cell, the line scrolls sideways to keep it in; else the start
        of the line, cut, or `placeholder`, dim, when it is empty."""
        if not focused:
            if not self.text:
                return fit([run(placeholder, dim=True)], w)
            return fit([run(cut(self.text, w), **style)], w)
        text, at = self.text, self.cursor
        under = text[at] if at < len(text) else " "
        self._off = min(self._off, at)
        while self._off < at and cells(text[self._off : at]) + _wide(under) > w:
            self._off += 1
        row = [
            run(text[self._off : at], **style),
            run(under, **{**style, "inverse": True}),
            run(text[at + 1 :], **style),
        ]
        if not text:
            row.append(run(placeholder, dim=True))
        return fit(row, w)


class Listing:
    """A bar on a list of `count` rows, of which a window shows.

    `key`: `up`, `down`, `tab`, `home`, `end`, `pageup`, `pagedown`,
    and with `letters` `j`, `k`. False: not a key of it."""

    def __init__(self, count: int = 0, sel: int = 0) -> None:
        self.count = count
        self.sel = sel
        self.top = 0
        self._page = 1

    def key(self, name: str, letters: bool = True) -> bool:
        if letters and name in ("j", "k"):
            name = {"j": "down", "k": "up"}[name]
        step = {
            "up": -1,
            "down": 1,
            "tab": 1,
            "pageup": -self._page,
            "pagedown": self._page,
            "home": -self.count,
            "end": self.count,
        }.get(name)
        if step is None:
            return False
        self.sel = max(0, min(self.count - 1, self.sel + step))
        return True

    def window(self, h: int) -> range:
        """The rows shown in `h` rows: the bar is among them."""
        h = max(h, 0)
        self._page = max(h, 1)
        self.sel = max(0, min(self.count - 1, self.sel))
        top = min(self.top, self.sel, max(0, self.count - h))
        self.top = max(top, self.sel - h + 1, 0)
        return range(self.top, min(self.count, self.top + h))

    def mark(self, h: int) -> str:
        """`3/12` when the list does not fit `h` rows."""
        return f"{self.sel + 1}/{self.count}" if self.count > max(h, 0) else ""

    def arrow(self, i: int, h: int) -> str:
        """`↑`, `↓` on the first and last rows shown, when rows hide
        beyond them."""
        shown = self.window(h)
        if not shown:
            return ""
        if i == shown[0] and i > 0:
            return "↑"
        if i == shown[-1] and i < self.count - 1:
            return "↓"
        return ""


# -- models -----------------------------------------------------------------


class Model:
    """A screen: the question, the rows, a foot line of keys.

    A subclass gives `body(w, h, focused)` (the rows under the
    question), `key(name)`, `natural()` and, as needed, `paste`,
    `keys`, `hint`. `render` is exact at every size: `h` rows of `w`
    cells. One row: the body when focused, else the question. Two: no
    foot. `result` is the answer once `key` said `DONE`."""

    question: str = ""
    result: Any = None
    #: Shown dim at the right of the question (`2/3` of a form).
    mark: str = ""
    #: The key that gives the keyboard back, for the foot; None: none.
    escape: str | None = "esc"
    #: The foot of a widget not focused.
    hint: Sequence[Key] = (("ctrl+t ctrl+t", 0), ("to answer", 1))
    #: Keys added to the foot (a form's way back).
    more: Sequence[Key] = ()

    def key(self, name: str) -> str | None:
        """Take a key. None: go on; `DONE`: validated; `CANCEL`:
        dismissed."""
        return CANCEL if name == "ctrl+c" else None

    def paste(self, text: str) -> None:
        """Take a pasted text, its lines parted by `\\n`."""

    def body(self, w: int, h: int, focused: bool) -> Rows:
        """`h` rows at most, under the question."""
        return []

    def natural(self) -> Size:
        """The size the body is at ease in."""
        return (24, 1)

    def keys(self) -> list[Key]:
        """The foot of the focused widget."""
        out: list[Key] = [("enter ok", 1), *self.more, ("^C dismiss", 3)]
        if self.escape:
            out.append((f"{self.escape} later", 0))
        return out

    def note(self, h: int) -> str:
        """Beside `mark`: where the bar is in a list too long."""
        return ""

    def sizes(self) -> list[Size]:
        """Inner sizes for the variants of the widget, the best first."""
        w, h = self.natural()
        w = max(w, cells(self.question) + 1, 24)
        w, h = min(w, 78), min(h, 20)
        out = [(w, h + 2), (max(w * 2 // 3, 16), min(h, 4) + 2), (min(w, 24), 3)]
        out.append((min(w, 18), 1))
        return list(dict.fromkeys(out))

    def render(self, w: int, h: int, focused: bool) -> Rows:
        w, h = max(w, 1), max(h, 1)
        feet = 1 if h >= 3 else 0
        head: list[str] = []
        if self.question and (h >= 2 or not focused):
            head = wrap(self.question, w, max(1, (h - feet) // 3)) or [""]
        room = h - len(head) - feet
        rows: Rows = [[run(line, bold=True)] for line in head]
        note = self.note(room) if room > 0 else ""
        mark = " ".join(m for m in (note, self.mark) if m)
        if rows and mark and cells(mark) + 2 <= w:
            first = cut(head[0], w - cells(mark) - 1)
            gap = w - cells(first) - cells(mark)
            rows[0] = [run(first, bold=True), run(" " * gap), run(mark, dim=True)]
        rows += self.body(w, room, focused)[: max(room, 0)]
        rows += [[] for _ in range(h - feet - len(rows))]
        if feet:
            rows.append([run(foot(self.keys() if focused else self.hint, w), dim=True)])
        return [fit(row, w) for row in rows[:h]]


class Line(Model):
    """One line typed. `result`: the text."""

    def __init__(
        self, question: str = "", *, default: str = "", placeholder: str = ""
    ) -> None:
        self.question = question
        self.placeholder = placeholder
        self.editor = Editor(default)

    def key(self, name: str) -> str | None:
        if name == "enter":
            self.result = self.editor.text
            return DONE
        if self.editor.key(name):
            return None
        return super().key(name)

    def paste(self, text: str) -> None:
        self.editor.insert(text.rstrip("\n"))

    def body(self, w: int, h: int, focused: bool) -> Rows:
        return [self.editor.render(w, focused, self.placeholder)]

    def natural(self) -> Size:
        return (max(32, cells(self.editor.text) + 2), 1)


class Text(Model):
    """Several lines typed. `enter` validates; `alt+enter` or `ctrl+j`
    is a new line; a paste keeps its lines. `result`: the text, its
    lines parted by `\\n`."""

    def __init__(self, question: str = "", *, default: str = "") -> None:
        self.question = question
        self.lines = [Editor(line) for line in default.split("\n")]
        self.row = len(self.lines) - 1
        self._top = 0

    @property
    def text(self) -> str:
        return "\n".join(line.text for line in self.lines)

    def _go(self, row: int, cursor: int) -> None:
        self.row = row
        self.lines[row].cursor = min(cursor, len(self.lines[row].text))

    def _newline(self) -> None:
        line = self.lines[self.row]
        rest = Editor(line.text[line.cursor :])
        rest.cursor = 0
        line.text = line.text[: line.cursor]
        self.lines.insert(self.row + 1, rest)
        self.row += 1

    def _join(self, row: int) -> None:
        """The line `row + 1` at the end of `row`, the cursor between."""
        line, after = self.lines[row], self.lines.pop(row + 1)
        line.cursor = len(line.text)
        line.text += after.text
        self.row = row

    def key(self, name: str) -> str | None:
        line, last = self.lines[self.row], len(self.lines) - 1
        if name == "enter":
            self.result = self.text
            return DONE
        if name in ("alt+enter", "ctrl+j"):
            self._newline()
        elif name == "up" and self.row > 0:
            self._go(self.row - 1, line.cursor)
        elif name == "down" and self.row < last:
            self._go(self.row + 1, line.cursor)
        elif name in ("up", "down"):
            pass
        elif name in ("backspace", "ctrl+h") and line.cursor == 0:
            if self.row > 0:
                self._join(self.row - 1)
        elif name in ("delete", "ctrl+d") and line.cursor == len(line.text):
            if self.row < last:
                self._join(self.row)
        elif name == "left" and line.cursor == 0 and self.row > 0:
            self._go(self.row - 1, len(self.lines[self.row - 1].text))
        elif name == "right" and line.cursor == len(line.text) and self.row < last:
            self._go(self.row + 1, 0)
        elif not line.key(name):
            return super().key(name)
        return None

    def paste(self, text: str) -> None:
        first, *rest = text.split("\n")
        self.lines[self.row].insert(first)
        for piece in rest:
            self._newline()
            self.lines[self.row].insert(piece)

    def body(self, w: int, h: int, focused: bool) -> Rows:
        h = max(h, 1)
        self._top = max(min(self._top, self.row), self.row - h + 1, 0)
        shown = range(self._top, min(len(self.lines), self._top + h))
        return [self.lines[i].render(w, focused and i == self.row) for i in shown]

    def note(self, h: int) -> str:
        n = len(self.lines)
        return f"{self.row + 1}/{n}" if n > max(h, 1) else ""

    def keys(self) -> list[Key]:
        return [("enter ok", 1), ("alt+enter new line", 2), *super().keys()[1:]]

    def natural(self) -> Size:
        width = max(cells(line.text) for line in self.lines) + 2
        return (max(40, width), max(4, len(self.lines)))


class Choice(Model):
    """One of `options`, or with `multi` some of them.

    Arrows, `j` `k`, `tab`, page keys move the bar; a digit goes to
    that row; `space` ticks with `multi`; `enter` validates. `other`:
    a last row typed in place (there letters and digits are typed).
    `result`: the option (the text typed for `other`), or with `multi`
    the list of those ticked, the text of `other` last when not
    empty."""

    def __init__(
        self,
        question: str = "",
        options: Sequence[str] = (),
        *,
        multi: bool = False,
        other: bool = False,
    ) -> None:
        self.question = question
        self.options = list(options)
        self.multi = multi
        self.other = Editor() if other else None
        self.ticked: set[int] = set()
        self.listing = Listing(len(self.options) + (1 if other else 0))

    @property
    def _typing(self) -> bool:
        return self.other is not None and self.listing.sel == len(self.options)

    def key(self, name: str) -> str | None:
        sel = self.listing.sel
        if name == "enter":
            typed = self.other.text if self.other is not None else ""
            if self.multi:
                picked = [self.options[i] for i in sorted(self.ticked)]
                self.result = picked + ([typed] if typed else [])
            elif self._typing:
                if not typed:
                    return None
                self.result = typed
            elif self.options:
                self.result = self.options[sel]
            else:
                return None
            return DONE
        if self._typing and self.other is not None:
            keep = name in ("home", "end")
            if (not keep and self.listing.key(name, False)) or self.other.key(name):
                return None
            return super().key(name)
        if self.listing.key(name):
            return None
        if name.isdigit() and 1 <= int(name) <= self.listing.count:
            self.listing.sel = int(name) - 1
        elif name == "space" and self.multi and sel < len(self.options):
            self.ticked ^= {sel}
        else:
            return super().key(name)
        return None

    def paste(self, text: str) -> None:
        if self._typing and self.other is not None:
            self.other.insert(text.rstrip("\n"))

    def body(self, w: int, h: int, focused: bool) -> Rows:
        rows: Rows = []
        for i in self.listing.window(h):
            on = focused and i == self.listing.sel
            digit = str(i + 1) if i < 9 else " "
            row = [run(digit + " ", dim=not on, inverse=on)]
            room = w - 2
            if self.multi and i < len(self.options):
                row.append(run("[x] " if i in self.ticked else "[ ] ", inverse=on))
                room -= 4
            arrow = self.listing.arrow(i, h)
            room -= 2 if arrow else 0
            if i < len(self.options):
                row.append(run(cut(self.options[i], room), inverse=on))
                row = fit(row, w - (2 if arrow else 0))
                if on:
                    row[-1] = {**row[-1], "inverse": True}
            elif self.other is not None:
                row += self.other.render(room, on, "other…")
            if arrow:
                row += [run(" "), run(arrow, dim=True)]
            rows.append(row)
        return rows

    def note(self, h: int) -> str:
        return self.listing.mark(h)

    def keys(self) -> list[Key]:
        out: list[Key] = [("↑↓ move", 2)]
        if self.multi:
            out.append(("space tick", 2))
        return out + super().keys()

    def natural(self) -> Size:
        width = max((cells(o) for o in self.options), default=0)
        width += 6 if self.multi else 2
        return (max(width, 24 if self.other is not None else 0), self.listing.count)


class Form(Model):
    """Models one after another; `2/3` beside the question. `enter`
    validates a step and goes to the next; `shift+tab` goes back.
    `result`: the list of theirs."""

    def __init__(self, steps: Sequence[Model]) -> None:
        if not steps:
            raise ValueError("a form has a step at least")
        self.steps = list(steps)
        self.at = 0

    @property
    def step(self) -> Model:
        step = self.steps[self.at]
        step.mark = f"{self.at + 1}/{len(self.steps)}" if len(self.steps) > 1 else ""
        step.more = [("shift+tab back", 2)] if self.at else []
        step.escape = self.escape
        step.hint = self.hint
        return step

    @property
    def question(self) -> str:  # type: ignore[override]
        return self.step.question

    def key(self, name: str) -> str | None:
        if name == "shift+tab":
            self.at = max(0, self.at - 1)
            return None
        flow = self.step.key(name)
        if flow != DONE:
            return flow
        if self.at + 1 < len(self.steps):
            self.at += 1
            return None
        self.result = [step.result for step in self.steps]
        return DONE

    def paste(self, text: str) -> None:
        self.step.paste(text)

    def natural(self) -> Size:
        sizes = [step.natural() for step in self.steps]
        width = max(max(w, cells(s.question)) for (w, _), s in zip(sizes, self.steps))
        return (width + 4, max(h for _, h in sizes))

    def render(self, w: int, h: int, focused: bool) -> Rows:
        return self.step.render(w, h, focused)


# -- runner -----------------------------------------------------------------


class Dialog:
    """A model shown in a widget that asks for the keyboard.

        dialog = Dialog(pilot, Choice("Deploy?", ["yes", "no"]), tag="deploy")
        while not dialog.done:
            dialog.take(pilot.event())
        dialog.result                     # None: dismissed

    It declares the tag (`sizes`: inner sizes, else `model.sizes()`),
    shows it (`title`, and `show` as for `Pilot.widget`; `fill` 1 when
    not given) and asks (`quiet`, `escape` as for `Widget.ask`). It
    subscribes to `widgets`. `take` gives it each event of the pilot:
    those of its widget are its own. The model over, the widget is
    released and hidden."""

    def __init__(
        self,
        pilot: Pilot,
        model: Model,
        *,
        tag: str = "ask",
        sizes: Sequence[Size] | None = None,
        title: str | None = None,
        quiet: bool = False,
        escape: str | bool | None = None,
        **show: Any,
    ) -> None:
        self.pilot = pilot
        self.model = model
        self.done = False
        self.result: Any = None
        if escape is not None:
            model.escape = escape or None
        self._keys = Keys()
        self._drawn: tuple[Any, ...] | None = None
        show.setdefault("fill", 1.0)
        pilot.subscribe("widgets")
        self.widget: Widget = pilot.widget(
            tag,
            list(sizes or model.sizes()),
            title=title,
            content=lambda w, h: model.render(w, h, False),
            **show,
        )
        self.widget.ask(quiet=quiet, escape=escape)

    def draw(self) -> None:
        """Set the variant shown, when what it shows changed."""
        size = self.widget.size
        if size is None or self.done:
            return
        rows = self.model.render(*size, self.widget.focused)
        if self._drawn != (self.widget.variant, rows):
            self.widget.set(rows)
            self._drawn = (self.widget.variant, rows)

    def take(self, event: tuple[str, Any] | None) -> bool:
        """Give an event of the pilot, `(topic, data)`. False: it is
        not the dialog's."""
        if event is None or self.done:
            return False
        topic, data = event
        if topic == "widgets":
            if not self.widget.update(data):
                return False
            if data.get("state") == "gone":
                self.done = True
            elif not self.widget.focused:
                self._keys.reset()
        elif topic == "input" and data.get("widget") == self.widget.id:
            if "released" in data:
                self._keys.reset()
                self.widget.info = {**self.widget.info, "focused": False}
            else:
                self.widget.info = {**self.widget.info, "focused": True}
                match self._keys.feed(data):
                    case ("paste", text):
                        self.model.paste(text)
                    case ("key", name):
                        self.finish(self.model.key(name))
        else:
            return False
        self.draw()
        return True

    def finish(self, flow: str | None) -> None:
        """What a `Model.key` answered: `DONE`, `CANCEL` end the
        dialog."""
        if flow not in (DONE, CANCEL):
            return
        self.result = self.model.result if flow == DONE else None
        self.close()

    def close(self) -> None:
        """Release and hide the widget; fine if the session is gone."""
        if self.done and self.widget.info.get("state") == "gone":
            return
        self.done = True
        try:
            self.widget.release()
            self.widget.hide()
        except PtyrcError as exc:
            if exc.code != "gone":
                raise


def ask(
    pilot: Pilot, model: Model, *, timeout: float | None = None, **options: Any
) -> Any:
    """Show `model` (`options` as for `Dialog`) and wait for the user:
    `model.result`; None when dismissed, after `timeout` seconds
    (session time), or when the session ends."""
    pilot.subscribe("exit")
    dialog = Dialog(pilot, model, **options)
    end = None if timeout is None else clock.monotonic() + timeout
    try:
        dialog.draw()
        while not dialog.done:
            left = None if end is None else end - clock.monotonic()
            if left is not None and left <= 0:
                break
            event = pilot.event(timeout=left)
            if event is not None and event[0] == "exit":
                break
            dialog.take(event)
    except PtyrcError as exc:
        if exc.code != "gone":
            raise
        return None
    finally:
        if not dialog.done:
            dialog.close()
    return dialog.result
