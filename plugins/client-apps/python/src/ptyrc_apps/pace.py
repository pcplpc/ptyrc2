# SPDX-License-Identifier: GPL-3.0-or-later
"""Pace: keys a minute, and the words typed most.

    ptyrc-app pace
    ptyrc-app pace -a --save ~/.cache/pace.json
    ptyrc-app pace --print -a

What it shows:

- topic `input`: the bursts of the host keyboard. `keys(data)` counts
  them; the bytes are dropped at once. A `Meter` keeps counts by slot
  of time: the pace now, the best minute, half an hour of history;
- the words are read from the screen, never from the keys: what is not
  echoed, a password, is not read. Topics `cursor` and `text`: after a
  key, the row of the cursor is read; a `Reader` follows the word that
  grows under the cursor, a cell a key, and gives it when a separator
  or a new line ends it. Text a program writes is not typed: no key,
  or more than a cell at once;
- `wordy`: what is left out: hashes, keys, base64, what does not look
  like a word; `Board`: the counts, `--save` keeps them;
- a face is a function of a `View`: `face(view, w, h, look)`;
- `--print`: the faces on a made-up session (`--seed`), no driver.
"""

from __future__ import annotations

import argparse
import base64
import itertools
import json
import os
import random
import re
import sys
import time
import unicodedata
from collections import deque
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from pathlib import Path

from ptyrc import Pilot, PtyrcError
from ptyrc import clock as session_clock

from . import _cli, _widgets
from ._canvas import Braille

__all__ = [
    "STOP",
    "Board",
    "Hover",
    "Look",
    "Meter",
    "Reader",
    "View",
    "ago",
    "face",
    "graph",
    "keys",
    "ladder",
    "main",
    "wordy",
]

Size = tuple[int, int]
Run = dict[str, object]
Row = list[Run]
Rows = list[Row]

SLOT = 10.0
"""Seconds of a slot of the history."""
SPAN = 1800.0
"""Seconds of history kept."""
PASTE = 12
"""Keys in one burst from which it is a paste: not counted."""

# -- keys -------------------------------------------------------------------


def keys(data: bytes) -> int:
    """The keys of a burst of the host keyboard: a character, a control,
    an escape sequence (an arrow, a function key) are one each. A
    bracketed paste, a mouse report and a burst of `PASTE` keys or more
    are none."""
    if b"\x1b[200~" in data or b"\x1b[<" in data or b"\x1b[M" in data:
        return 0
    n = i = 0
    while i < len(data):
        byte = data[i]
        i += 1
        if byte == 0x1B and i < len(data) and data[i] in b"[O":
            i += 1
            while i < len(data) and not 0x40 <= data[i] <= 0x7E:
                i += 1
            i += 1
        elif byte == 0x1B and i < len(data):
            i += 1  # alt and a key
        elif 0x80 <= byte < 0xC0:
            continue  # inside a character
        n += 1
    return n if n < PASTE else 0


class Meter:
    """Keys counted by time: `add(now, n)`. Nothing else is kept."""

    def __init__(self, window: float = 15.0) -> None:
        self.window = window
        self.best = 0  # the highest pace
        self.total = 0
        self._recent: deque[tuple[float, int]] = deque()  # the last minute
        self._slots: dict[int, int] = {}  # slot: keys

    def add(self, now: float, n: int) -> None:
        if n <= 0:
            return
        self.total += n
        self._recent.append((now, n))
        slot = int(now // SLOT)
        self._slots[slot] = self._slots.get(slot, 0) + n
        for old in [s for s in self._slots if s < slot - SPAN / SLOT]:
            del self._slots[old]
        self.best = max(self.best, self.rate(now))

    def minute(self, now: float) -> int:
        """Keys of the last minute."""
        while self._recent and self._recent[0][0] <= now - 60.0:
            self._recent.popleft()
        return sum(n for _, n in self._recent)

    def rate(self, now: float) -> int:
        """Keys a minute, over the last `window` seconds."""
        self.minute(now)
        got = sum(n for at, n in self._recent if at > now - self.window)
        return round(got * 60.0 / self.window)

    def slots(self, now: float) -> tuple[int, ...]:
        """Keys of each slot of the last `SPAN`, oldest first."""
        last = int(now // SLOT)
        count = int(SPAN / SLOT)
        return tuple(self._slots.get(s, 0) for s in range(last - count + 1, last + 1))


# -- words ------------------------------------------------------------------


def _letter(char: str) -> bool:
    return char.isalpha() or char in "'’"


def _tail(text: str) -> str:
    """The letters `text` ends with."""
    n = len(text)
    while n and _letter(text[n - 1]):
        n -= 1
    return text[n:]


def _token(text: str) -> str:
    """What `text` ends with, back to a blank."""
    return text.rsplit(None, 1)[-1] if text.strip() and not text[-1:].isspace() else ""


class Reader:
    """The words typed, from what the row of the cursor shows.

    `see(row, text, x, keyed)`: the cursor is on `row`, at the cell `x`
    of `text`; `keyed`: the keys that came since the last call. It
    gives the `(word, token)` that ended: a word that grew a cell a
    key, at least half of it, with a separator typed after it, or the
    cursor gone to a new line. `token`: what stands around it, for
    `wordy`. More cells than keys: a completion, a paste, a program.
    """

    def __init__(self) -> None:
        self.row: int | None = None
        self.left = ""  # the row up to the cursor
        self.typed = 0  # letters of the word under way that were typed

    def _word(self) -> tuple[str, str] | None:
        word = _tail(self.left).strip("'’")
        typed, self.typed = self.typed, 0
        if not word or 2 * typed < len(word):
            return None
        return word, _token(self.left)

    def see(self, row: int, text: str, x: int, keyed: int) -> list[tuple[str, str]]:
        left = text[:x].ljust(x)  # a row is told without its last blanks
        out: list[tuple[str, str] | None] = []
        same = row == self.row
        if same and left == self.left:
            return []
        if same and keyed and left.startswith(self.left):
            more = left[len(self.left) :]
            if len(more) > keyed:
                if not all(map(_letter, more)):
                    self.typed = 0
                more = ""
            for char in more:
                if _letter(char):
                    self.typed = self.typed + 1 if _tail(self.left) else 1
                else:
                    out.append(self._word())
                self.left += char
        elif same and self.left.startswith(left):  # rubbed out
            self.typed = min(self.typed, len(_tail(left)))
        elif keyed:
            out.append(self._word())
        else:
            self.typed = 0
        self.row, self.left = row, left
        return [word for word in out if word is not None]


#: Words too common to be a favourite, 4 letters and more.
STOP = frozenset(
    """
    about after again also another back because been before being both
    came come could does done down each even from have having here into
    just know like made make many more most much must need only other
    over same should some such take than that their them then there
    these they this those through time upon very want well were what
    when where which while will with would your yours
    alors avec avoir aussi avant bien cette comme dans donc elle elles
    encore entre être fait faire leur leurs mais même nous pour plus
    quand quel quelle sans sont sous tous tout toute très vous
    """.split()  # noqa: SIM905
)

_VOWELS = frozenset("aeiouy")
#: The 195 pairs of letters English and French words are most made of.
_PAIRS = frozenset(
    """
    th he in er an re on at en nd ti es or te of ed is it al ar st
    to nt ng se ha as ou io le ve co me de hi ri ro ic ne ea ra ce
    li ch ll be ma si om ur ca el ta la ns di fo ho pe ec pr no ct
    us ac ot il tr ly nc et ut ss so rs un lo wa ge ie wh ee wi em
    ad ol rt po we na ul ni ts mo ow pa im mi ai sh ir su id os iv
    ia am fi ci vi pl ig tu ev ld ry mp fe bl ab gh ty op wo sa ay
    ex ke fr oo av ag if ap gr od bo sp rd do uc bu ei ov by rm ep
    tt oc fa ef cu rn sc gi da yo cr cl du ga qu ue ff ba ey ls va
    um pp ua up lu go ht ru ug ds lt pi rc rr eg au ck br bi pt pu
    ui ib ki eb ob mm
    """.split()  # noqa: SIM905
)


_HUMPS = re.compile(r"[A-Z]+[a-z]*|[a-z]+")


def _bare(word: str) -> str:
    """Lowered, the accents off."""
    flat = unicodedata.normalize("NFD", word.lower())
    return "".join(c for c in flat if not unicodedata.combining(c))


def wordy(word: str, token: str = "") -> bool:
    """Whether `word` looks like a word of a language, and not like a
    piece of a hash, of a key, of base64, of a setting. `token`: what it
    was cut from, when known: digits next to it tell.

    Left out: over 24 letters; no vowel; six consonants or four of one
    letter in a row; seven letters and more, all of `a` to `f`; mixed
    case with a hump under three letters, two for the first and the
    last (`Word`, `WORD`, `camelCase`, `FrameId` pass); from 8 letters, vowels under an eighth or over four fifths,
    or under half of the pairs of letters among `_PAIRS`; a word in
    a run of 8 letters and digits and more with two numbers in it;
    a token of 12 letters and digits and more where the kind (lower,
    upper, digit) changes 3 times in 10 or more."""
    bare = "".join(c for c in _bare(word) if c.isalpha())
    n = len(bare)
    if not 1 <= n <= 24 or not bare.isascii():
        return 1 <= n <= 24 and not bare.isascii()  # another script: kept
    vowels = sum(c in _VOWELS for c in bare)
    if not vowels:
        return False
    run = same = 1
    for a, b in itertools.pairwise(bare):
        same = same + 1 if a == b else 1
        run = run + 1 if a not in _VOWELS and b not in _VOWELS else 1
        if same >= 4 or run >= 6:
            return False
    if n >= 7 and set(bare) <= set("abcdef"):
        return False
    # Case: `word`, `WORD`, `Word`, or humps of three letters and more.
    if not (word.islower() or word.isupper() or word[1:].islower()):
        humps = _HUMPS.findall(word)
        ends = min(len(humps[0]), len(humps[-1])) < 2
        if ends or any(len(h) < 3 for h in humps[1:-1]):
            return False
    if n >= 8:
        if not 0.125 <= vowels / n <= 0.8:
            return False
        pairs = [a + b for a, b in itertools.pairwise(bare)]
        if 2 * sum(p in _PAIRS for p in pairs) < len(pairs):
            return False
    # The token: digits next to the word, or kinds of characters that
    # change all the time.
    runs = "".join(c if c.isalnum() else " " for c in token).split()
    near = next((t for t in runs if word in t), "")
    if len(near) >= 8 and len(re.findall(r"\d+", near)) >= 2:
        return False
    kinds = [c.isdigit() + 2 * c.isupper() for c in token if c.isalnum()]
    turns = sum(a != b for a, b in itertools.pairwise(kinds))
    return not (len(kinds) >= 12 and turns >= 0.3 * len(kinds))


class Board:
    """The count of each word, and when it was last typed (seconds of
    the epoch)."""

    def __init__(self, least: int = 4, stop: Iterable[str] = STOP) -> None:
        self.least = least
        self.stop = frozenset(_bare(w) for w in stop)
        self.words: dict[str, tuple[int, float]] = {}
        self.dirty = False

    def add(self, word: str, token: str = "", at: float = 0.0) -> bool:
        """Count `word`, unless it is short, common or not a word."""
        low = word.lower()
        if len(low) < self.least or _bare(low) in self.stop or not wordy(word, token):
            return False
        self.words[low] = (self.words.get(low, (0, 0.0))[0] + 1, at)
        self.dirty = True
        return True

    def top(self, n: int) -> list[tuple[str, int, float]]:
        """The `n` words typed most: `(word, count, last)`; of two as
        many times, the last typed first."""
        order = sorted(self.words.items(), key=lambda i: (-i[1][0], -i[1][1], i[0]))
        return [(word, count, last) for word, (count, last) in order[:n]]

    def load(self, path: Path) -> None:
        """Add the counts of a file of `save`; one that is not there, or
        not such a file, adds nothing."""
        try:
            words = json.loads(path.read_text(encoding="utf-8"))["words"]
            for word, (count, last) in words.items():
                mine = self.words.get(word, (0, 0.0))
                self.words[word] = (mine[0] + int(count), max(mine[1], float(last)))
        except (OSError, ValueError, KeyError, TypeError, AttributeError):
            pass

    def save(self, path: Path) -> None:
        """Write the counts: a new file, moved in place; mode 600."""
        data = {"words": {w: list(v) for w, v in sorted(self.words.items())}}
        path.parent.mkdir(parents=True, exist_ok=True)
        temp = path.with_name(path.name + ".new")
        with open(
            os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w"
        ) as f:
            json.dump(data, f, ensure_ascii=False)
        temp.replace(path)
        self.dirty = False


# -- the faces --------------------------------------------------------------


@dataclass(frozen=True)
class Look:
    """What the options ask of a face."""

    graph: bool = False
    best: bool = False
    words: bool = False
    bars: bool = False
    colour: bool = False
    top: int = 10


@dataclass(frozen=True)
class View:
    """What a face shows, at a moment."""

    rate: int = 0  # keys a minute, now
    best: int = 0  # the highest pace
    slots: tuple[int, ...] = ()  # keys of each slot, oldest first
    top: tuple[tuple[str, int, float], ...] = ()  # word, count, seconds ago


class Hover:
    """The pointer over the widget: `at`, a cell of its inner box, or
    None. `on_move` and `on_leave` are the handlers of a hover."""

    def __init__(self) -> None:
        self.at: tuple[int, int] | None = None

    def on_move(self, x: int, y: int) -> bool:
        """False: the app draws by itself."""
        self.at = (x, y)
        return False

    def on_leave(self) -> bool:
        self.at = None
        return False


#: Colours of the 256, with `-c`: a rank of the board each.
RANKS = (220, 214, 208, 209, 204, 170, 141, 105, 75, 80)
#: The rows of the graph, the lowest first.
GRAPH = (37, 44, 87)


def _run(text: str, fg: object = None, **style: object) -> Run:
    out: Run = {"t": text, **{k: v for k, v in style.items() if v}}
    if fg is not None:
        out["fg"] = fg
    return out


def _width(row: Row) -> int:
    return sum(len(str(run["t"])) for run in row)


def ago(seconds: float) -> str:
    """`now`, `40s ago`, `3m ago`, `2h ago`, `5d ago`."""
    if seconds < 5:
        return "now"
    for unit, size in (("d", 86400), ("h", 3600), ("m", 60)):
        if seconds >= size:
            return f"{int(seconds // size)}{unit} ago"
    return f"{int(seconds)}s ago"


def graph(slots: Sequence[int], w: int, h: int, look: Look) -> Rows:
    """The history in braille, `w` cells by `h`: two columns a cell,
    the newest on the right, each the mean pace of its share of the
    time; full height is the highest. A column is one dot at least: the
    floor."""
    columns = 2 * w
    values = []
    for i in range(columns):
        a, b = i * len(slots) // columns, (i + 1) * len(slots) // columns
        part = slots[a : max(b, a + 1)]
        values.append(sum(part) / len(part) if part else 0.0)
    high = max(values, default=0.0) or 1.0
    canvas = Braille(w, h)
    for x, value in enumerate(values):
        for y in range(1 + round(value / high * (4 * h - 1))):
            canvas.set(x, 4 * h - 1 - y)
    rows: Rows = []
    for y, text in enumerate(canvas.rows()):
        fg = GRAPH[min(h - 1 - y, len(GRAPH) - 1)] if look.colour else None
        rows.append([_run(text.ljust(w), fg, dim=not look.colour)])
    return rows


def _head(view: View, w: int, look: Look) -> Row:
    """`312 k/min ▲ 345  62 wpm`, as much as fits."""
    top = view.best and view.rate >= 0.95 * view.best
    fg = None
    if look.colour:
        fg = "bright-yellow" if top else "bright-cyan" if view.rate else None
    row = [_run(str(view.rate), fg, bold=bool(view.rate), dim=not view.rate)]
    if w >= _width(row) + 6:
        row.append(_run(" k/min", dim=True))
    best = [_run(" ▲ ", "bright-yellow" if look.colour else None), _run(str(view.best))]
    if look.best and w >= _width(row) + _width(best):
        row += best
    wpm = [_run(f"{round(view.rate / 5)} wpm", dim=True)]
    if w >= _width(row) + _width(wpm) + 2:
        row += [_run(" " * (w - _width(row) - _width(wpm))), *wpm]
    return row


def _clip(text: str, n: int) -> str:
    return text if len(text) <= n else text[: max(0, n - 1)] + "…"


_EIGHTHS = " ▏▎▍▌▋▊▉█"


def _bar(share: float, n: int) -> str:
    """A bar of `n` cells, an eighth of a cell exact; never empty."""
    eighths = max(1, round(share * 8 * n))
    return ("█" * (eighths // 8) + _EIGHTHS[eighths % 8].strip()).ljust(n)


def _words(view: View, w: int, h: int, look: Look, over: int | None) -> Rows:
    """The board, `h` rows: rank, word, a bar with `-r`, count. The row
    `over`, under the pointer: the word, its count, when it was last
    typed."""
    top = view.top[:h]
    if not top:
        return [[_run(_clip("no word yet", w), dim=True)]]
    ranks = len(str(len(top)))
    counts = max(len(str(count)) for _, count, _ in top)
    room = w - ranks - counts - 2
    name = min(room, max(len(word) for word, _, _ in top))
    if look.bars:
        name = min(name, max(4, room - 5, room * 3 // 5))
    bar = room - name - 1 if look.bars else 0
    rows: Rows = []
    for i, (word, count, last) in enumerate(top):
        fg = RANKS[min(i, len(RANKS) - 1)] if look.colour else None
        rank = _run(f"{i + 1:>{ranks}} ", dim=True)
        if i == over:
            tip = f"×{count} · {ago(last)}"
            text = _clip(word, max(1, w - ranks - 2 - len(tip)))
            gap = " " * max(1, w - ranks - 1 - len(text) - len(tip))
            rows.append([rank, _run(text, fg, bold=True), _run(gap + tip)])
            continue
        row = [rank, _run(_clip(word, name).ljust(name), fg, bold=i == 0)]
        if bar >= 2:
            row.append(
                _run(" " + _bar(count / top[0][1], bar), fg, dim=not look.colour)
            )
            row.append(_run(f" {count:>{counts}}"))
        else:
            row.append(_run(f"{count:>{w - ranks - 1 - name}}"))
        rows.append(row)
    return rows


def _graph_rows(h: int, look: Look) -> int:
    """Rows of the graph in a face `h` high."""
    if not look.graph or h < 2:
        return 0
    return 1 if h < 3 or look.words and h < 7 else 2


def _cut(row: Row, w: int) -> Row:
    """`row`, what exceeds `w` cells cut."""
    out: Row = []
    for run in row:
        text = str(run["t"])[: w - _width(out)]
        if text:
            out.append({**run, "t": text})
    return out


def face(
    view: View, w: int, h: int, look: Look, hover: tuple[int, int] | None = None
) -> Rows:
    """`faces`, cut to the box."""
    return [_cut(row, w) for row in faces(view, w, h, look, hover)[:h]]


def faces(
    view: View, w: int, h: int, look: Look, hover: tuple[int, int] | None = None
) -> Rows:
    """A face of `w` by `h`: the pace; under it the graph (`-g`), then
    the board (`-w`), as many words as rows are left. One row with
    `-w`, from 20 cells: the pace and the word typed most. `hover`:
    the cell under the pointer."""
    if h == 1:
        row = _head(view, w, look)
        if look.words and view.top and w >= 20:
            row = _head(view, min(w, 9), Look(colour=look.colour))
            fg = RANKS[0] if look.colour else None
            word = _clip(view.top[0][0], w - _width(row) - 3)
            row += [_run(" ♥ ", fg), _run(word, bold=True)]
        return [row]
    rows = [_head(view, w, look)]
    g = _graph_rows(h, look)
    if g:
        rows += graph(view.slots, w, g, look)
    if look.words and h > len(rows):
        first = len(rows)
        over = None if hover is None else hover[1] - first
        rows += _words(view, w, h - first, look, over)
    return rows


def ladder(look: Look) -> list[Size]:
    """The sizes of the faces of a look, smallest first."""
    sizes = [(3, 1), (9, 1)]
    if look.best:
        sizes.append((16, 1))
    if look.words:
        sizes.append((24, 1))
    if look.graph:
        sizes += [(12, 2), (20, 3)]
    if look.words:
        head = 3 if look.graph else 1
        wide = 6 if look.bars else 0
        for n, w in ((3, 20), (5, 22), (10, 26)):
            if n <= look.top or n == 3:
                sizes.append((w + wide, head + min(n, look.top)))
        if look.top not in (3, 5, 10):
            sizes.append((26 + wide, head + look.top))
    return sorted(set(sizes), key=lambda s: (s[0] * s[1], s))


# -- print ------------------------------------------------------------------

_SAMPLE = (  # noqa: SIM905
    "widget hourglass border driver sprite frame braille quiet render "
    "variant ladder favourite window garden sand train lantern meadow "
    "pilot cursor"
).split()


def sample(seed: int | None, top: int) -> View:
    """A made-up session."""
    rng = random.Random(seed)
    pace, slots, left = 0.0, [], 0
    for _ in range(int(SPAN / SLOT)):
        if left <= 0:  # a spell of typing, or of quiet
            pace = rng.choice((0.0, 0.0, 25.0, 45.0, 60.0))
            left = rng.randint(4, 18)
        left -= 1
        slots.append(int(max(0.0, pace + rng.uniform(-12, 12))) if pace else 0)
    words = rng.sample(_SAMPLE, k=min(top, len(_SAMPLE)))
    count, board = rng.randint(30, 60), []
    for word in words:
        board.append((word, count, rng.choice((2, 40, 200, 4000, 90000))))
        count = max(1, int(count * rng.uniform(0.6, 0.95)))
    rate = round(sum(slots[-2:]) * 60 / (2 * SLOT))
    return View(rate, max(rate, 6 * max(slots)), tuple(slots), tuple(board))


def boxed(rows: Rows, w: int, h: int, colour: bool) -> list[str]:
    """`rows` in a plain box of inner size `w` by `h`, titled `pace`."""
    from .clock import ansi

    edge = (lambda t: f"\x1b[2m{t}\x1b[0m") if colour else (lambda t: t)
    paint = ansi if colour else (lambda run: str(run["t"]))
    name = " pace " if w >= 8 else ""
    out = [edge("┌" + "─" * w + "┐")]
    for row in rows + [[] for _ in range(h - len(rows))]:
        body = "".join(paint(run) for run in row) + " " * (w - _width(row))
        out.append(edge("│") + body + edge("│"))
    out.append(edge("└" + "─" * (w - len(name))) + name + edge("┘"))
    return out


def show(args: argparse.Namespace, look: Look) -> int:
    """`--print`: the faces, or one of the size given, on `sample`."""
    view = sample(args.seed, look.top)
    colour = sys.stdout.isatty()
    hover = (0, args.hover) if args.hover is not None else None
    for w, h in [args.print] if args.print else ladder(look):
        print(f"{w}x{h}")
        print("\n".join(boxed(face(view, w, h, look, hover), w, h, colour)))
    return 0


# -- live -------------------------------------------------------------------


def live(
    pilot: Pilot, args: argparse.Namespace, look: Look, seconds: float | None
) -> None:
    def now() -> float:
        return session_clock.monotonic() * args.speed

    meter = Meter(args.window)
    board = Board(args.min, STOP | frozenset(args.stop))
    if args.save is not None:
        board.load(args.save)
    reader = Reader()
    hover = Hover()

    def view() -> View:
        at, wall = now(), time.time()
        top = tuple((w, c, max(0.0, wall - last)) for w, c, last in board.top(look.top))
        return View(meter.rate(at), meter.best, meter.slots(at), top)

    def render(w: int, h: int) -> Rows:
        return face(view(), w, h, look, hover.at)

    deck = _widgets.Deck(pilot, args)
    widget = deck.add("pace", ladder(look), render)
    pilot.subscribe("input", "cursor", "text")
    if look.words:
        # While the pointer is there: a mouse at rest sends nothing.
        deck.hover_timeout = None
        deck.on_hover(widget, hover.on_move, hover.on_leave)

    def read() -> None:
        """The row of the cursor, given to the reader."""
        try:
            cursor = pilot.cursor()
            if not cursor.visible:
                return
            cols = pilot.size[0]
            text = pilot.text(rect=[0, cursor.y, cols, 1])[0]
        except (PtyrcError, IndexError) as exc:
            if isinstance(exc, PtyrcError) and exc.code == "gone":
                raise
            return
        for word, token in reader.see(cursor.y, text, cursor.x, keyed[0]):
            board.add(word, token, at=time.time())
        keyed[0] = 0

    keyed = [0]
    end = None if seconds is None else session_clock.monotonic() + seconds
    sent: tuple[object, ...] = ()
    saved = session_clock.monotonic()
    try:
        while True:
            at = session_clock.monotonic()
            if end is not None and at >= end:
                return
            size = widget.size
            if size is not None:
                rows = render(*size)
                if (size, rows) != sent:
                    widget.set(rows)
                    sent = (size, rows)
            if args.save is not None and board.dirty and at >= saved + 30:
                board.save(args.save)
                saved = at
            # The pace falls by itself: a look a second while it is up.
            wait = 1.0 if meter.minute(now()) else 30.0
            due = at + wait if end is None else min(at + wait, end)
            event = pilot.event(timeout=max(0.0, due - session_clock.monotonic()))
            moved = False
            while event is not None:
                topic, data = event
                if topic == "exit":
                    return
                if topic == "input" and "bytes" in data:
                    got = keys(base64.b64decode(data["bytes"]))
                    meter.add(now(), got)
                    keyed[0] += got
                elif topic in ("cursor", "text"):
                    moved = True
                elif topic == "move":
                    deck.move(data["frame"])
                    sent = ()
                elif topic == "widgets":
                    place = (widget.variant, widget.rect)
                    if deck.take(data) and place != (widget.variant, widget.rect):
                        deck.leave()
                elif topic == "hover":
                    deck.hover(data)
                event = pilot.event(timeout=0)
            if moved and look.words:
                read()
    finally:
        if args.save is not None and board.dirty:
            board.save(args.save)


# -- options ----------------------------------------------------------------


def _size(text: str) -> Size:
    w, sep, h = text.partition("x")
    if not (sep and w.isdigit() and h.isdigit() and int(w) > 0 and int(h) > 0):
        raise argparse.ArgumentTypeError(f"not a size WxH: {text!r}")
    return int(w), int(h)


def _positive(text: str) -> float:
    try:
        value = float(text)
    except ValueError:
        value = 0.0
    if not 0.0 < value < float("inf"):
        raise argparse.ArgumentTypeError(f"not a positive number: {text!r}")
    return value


def _count(text: str) -> int:
    if not (text.isdigit() and 1 <= int(text) <= 40):
        raise argparse.ArgumentTypeError(f"not a count, 1 to 40: {text!r}")
    return int(text)


def _stop(text: str) -> list[str]:
    try:
        return Path(text).read_text(encoding="utf-8").split()
    except (OSError, ValueError) as exc:
        raise argparse.ArgumentTypeError(f"cannot read {text}: {exc}") from None


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("pace", "Keys a minute, and the words typed most.")
    _widgets.options(out, border="round")
    out.add_argument(
        "-g", "--graph", action="store_true", help="the last half hour, in braille"
    )
    out.add_argument(
        "-b", "--best", action="store_true", help="the highest pace of the session"
    )
    out.add_argument(
        "-w",
        "--words",
        action="store_true",
        help="the words typed most, read from the screen",
    )
    out.add_argument("-r", "--bars", action="store_true", help="a bar a word; with -w")
    out.add_argument("-c", "--colour", action="store_true", help="colours")
    out.add_argument("-a", "--all", action="store_true", help="-g -b -w -r -c")
    out.add_argument(
        "--top", type=_count, default=10, metavar="N", help="words shown [default: 10]"
    )
    out.add_argument(
        "--min",
        type=_count,
        default=4,
        metavar="N",
        help="letters of the shortest word counted [default: 4]",
    )
    out.add_argument(
        "--stop",
        type=_stop,
        default=[],
        metavar="FILE",
        help="more words never counted, blanks between them",
    )
    out.add_argument(
        "--save",
        type=Path,
        metavar="FILE",
        help="keep the counts of the words there, across sessions",
    )
    out.add_argument(
        "--window",
        type=_positive,
        default=15.0,
        metavar="SECONDS",
        help="the pace is that of the last SECONDS [default: 15]",
    )
    out.add_argument(
        "--speed",
        type=_positive,
        default=1.0,
        metavar="N",
        help="seconds of the pace a second [default: 1]",
    )
    out.add_argument("--seed", type=int, help="seed of the session of --print")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the faces once, or one of the size given",
    )
    out.add_argument("--hover", type=int, help=argparse.SUPPRESS)
    args = _cli.parse(out, argv)
    if args.all:
        args.graph = args.best = args.words = args.bars = args.colour = True
    args.words |= args.bars
    return args


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    look = Look(args.graph, args.best, args.words, args.bars, args.colour, args.top)
    if args.print is not None:
        return show(args, look)
    return _cli.run(lambda pilot, a: live(pilot, a, look, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
