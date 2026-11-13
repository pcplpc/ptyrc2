# SPDX-License-Identifier: GPL-3.0-or-later
"""Gallery: framed pictures, hung one after the other.

    ptyrc-app gallery
    ptyrc-app gallery -a --every 2m --kind animal,silly
    ptyrc-app gallery --file mine.pic --print

What it shows:

- `Pilot.widget`: a tag a picture hung, the sizes of the picture its
  variants, the `painting` border, the name on the plate; a new
  picture declares the tag again (`Deck.resize`);
- a picture is a text file (`parse`): no code to contribute one;
- `Picture.render(w, h, t, look)`: pure; the time and the pointer are
  arguments.

A picture file:

    # a comment, in the header only
    name: Pear and fly
    author: agent
    kind: still
    period: 2
    hold: 8 1
    eyes: ◐●◑
    ink: y=220 g=70 k=240/233 s=/52 w=255!
    == 14x6
    rows of glyphs
    ::
    rows of ink
    ++
    rows of glyphs, the next frame
    == 8x4
    ...

- `name`; `author` (`anonymous`); `kind`, one of `KINDS`;
- `period`: seconds a frame (2); `hold`: periods each frame stays (1);
- `ink`: a key to `fg`, `fg/bg` or `/bg`, colours of the 256; `!`: bold;
- `== WxH`: a size, the inner box of a variant; then its frames, apart
  by `++`. A frame is at most H rows of at most W glyphs, each one cell
  wide; then, after `::`, as many rows of ink keys, a space or `.` for
  none. Without `::` a frame takes the ink of the one before. Every
  size has the same number of frames;
- `eyes`: three glyphs, looking left, ahead, right; or five: up and
  down too. A cell inked `@` is an eye: it turns to the pointer.
"""

from __future__ import annotations

import argparse
import random
import re
import sys
import unicodedata
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass, field
from pathlib import Path

from ptyrc import Pilot, clock

from . import _cli, _widgets

__all__ = [
    "HERE",
    "KINDS",
    "SHIP",
    "Hall",
    "Picture",
    "PictureError",
    "Slot",
    "collection",
    "framed",
    "load",
    "main",
    "parse",
    "shipped",
]

Size = tuple[int, int]
Run = dict[str, object]
Rows = list[list[Run]]
#: `(fg, bg, bold)`.
Ink = tuple[int | None, int | None, bool]

KINDS = ("portrait", "landscape", "still", "animal", "abstract", "silly")
#: The collection shipped.
HERE = Path(__file__).parent / "pictures"
#: The ship list: file stems, one a line, best first. Absent: all hang.
SHIP = HERE / "SHIPPED"

_SIZE = re.compile(r"== (\d+)x(\d+)")
_PLAIN: Ink = (None, None, False)
_GOLD = 178


class PictureError(ValueError):
    """A picture file that does not parse: `source:line: what`."""


@dataclass(frozen=True)
class Frame:
    rows: tuple[str, ...]
    ink: tuple[str, ...]


@dataclass(frozen=True)
class Picture:
    name: str
    author: str = "anonymous"
    kind: str = "abstract"
    period: float = 2.0
    hold: tuple[int, ...] = ()
    eyes: str = ""
    inks: dict[str, Ink] = field(default_factory=dict)
    sizes: dict[Size, tuple[Frame, ...]] = field(default_factory=dict)
    source: str = ""

    @property
    def frames(self) -> int:
        return len(next(iter(self.sizes.values())))

    @property
    def stem(self) -> str:
        return Path(self.source).stem

    def frame_at(self, t: float) -> int:
        """The frame shown at `t` seconds."""
        if self.frames == 1:
            return 0
        hold = self.hold or (1,) * self.frames
        beat = int(max(0.0, t) / self.period) % sum(hold)
        for nth, beats in enumerate(hold):
            if beat < beats:
                return nth
            beat -= beats
        return 0

    def gaze(
        self, w: int, h: int, look: Size | None, t: float = 0.0
    ) -> dict[Size, str]:
        """The glyph of each eye of size `w` by `h`, `look` the cell of
        the pointer, in the frame shown at `t`. A cell is twice as high
        as wide."""
        out: dict[Size, str] = {}
        if not self.eyes:
            return out
        left, ahead, right = self.eyes[:3]
        up, down = self.eyes[3:] or (ahead, ahead)
        for y, row in enumerate(self.sizes[w, h][self.frame_at(t)].ink):
            for x, key in enumerate(row):
                if key != "@":
                    continue
                glyph = ahead
                if look is not None:
                    dx, dy = look[0] - x, look[1] - y
                    if len(self.eyes) == 5 and dy and abs(dy) * 2 >= abs(dx):
                        glyph = up if dy < 0 else down
                    elif dx:
                        glyph = left if dx < 0 else right
                out[x, y] = glyph
        return out

    def render(self, w: int, h: int, t: float = 0.0, look: Size | None = None) -> Rows:
        """The rows of size `w` by `h` at `t`. `look`: the eyes turn to
        that cell; none: the glyphs as drawn."""
        if (w, h) not in self.sizes:
            raise ValueError(f"{self.name}: no size {w}x{h}")
        frame = self.sizes[w, h][self.frame_at(t)]
        eyes = {} if look is None else self.gaze(w, h, look, t)
        out: Rows = []
        for y, (glyphs, keys) in enumerate(zip(frame.rows, frame.ink, strict=True)):
            row: list[Run] = []
            last: Ink | None = None
            for x, (glyph, key) in enumerate(zip(glyphs, keys, strict=True)):
                ink = self.inks.get(key, _PLAIN)
                glyph = eyes.get((x, y), glyph)
                if ink == last:
                    row[-1]["t"] = f"{row[-1]['t']}{glyph}"
                    continue
                last = ink
                run: Run = {"t": glyph}
                fg, bg, bold = ink
                if fg is not None:
                    run["fg"] = fg
                if bg is not None:
                    run["bg"] = bg
                if bold:
                    run["bold"] = True
                row.append(run)
            out.append(row)
        return out

    def label(self, author: bool = False) -> str:
        """What the plate says."""
        return f"{self.name} · {self.author}" if author else self.name


def _colour(text: str, where: str) -> int | None:
    if not text:
        return None
    if not text.isdigit() or int(text) > 255:
        raise PictureError(f"{where}: colour {text!r}: not one of the 256")
    return int(text)


def _inks(value: str, where: str) -> dict[str, Ink]:
    out: dict[str, Ink] = {}
    for item in value.split():
        key, sep, spec = item.partition("=")
        if not sep or len(key) != 1 or key == ".":
            raise PictureError(f"{where}: ink {item!r}: want K=FG, K=FG/BG or K=/BG")
        bold = spec.endswith("!")
        fg, _, bg = spec.rstrip("!").partition("/")
        out[key] = (_colour(fg, where), _colour(bg, where), bold)
    return out


def _wide(glyph: str) -> bool:
    return (
        unicodedata.east_asian_width(glyph) in "WF"
        or unicodedata.category(glyph)[0] in "MC"
    )


def _block(
    lines: list[tuple[int, str]], w: int, h: int, source: str, what: str
) -> tuple[str, ...]:
    while lines and not lines[-1][1]:
        lines.pop()
    if len(lines) > h:
        raise PictureError(
            f"{source}:{lines[h][0]}: {len(lines)} rows of {what} in a {w}x{h}"
        )
    for n, row in lines:
        if len(row) > w:
            raise PictureError(
                f"{source}:{n}: a row of {what} of {len(row)} cells in a {w}x{h}"
            )
        bad = next((g for g in row if _wide(g)), None) if what == "glyphs" else None
        if bad is not None:
            raise PictureError(f"{source}:{n}: {bad!r} is not one cell wide")
    rows = [row.ljust(w) for _, row in lines]
    return tuple(rows + [" " * w] * (h - len(rows)))


def _frame(
    glyphs: list[tuple[int, str]],
    ink: list[tuple[int, str]] | None,
    size: Size,
    source: str,
    inks: dict[str, Ink],
    eyes: str,
    before: Sequence[Frame],
) -> Frame:
    w, h = size
    rows = _block(glyphs, w, h, source, "glyphs")
    if ink is None:
        return Frame(rows, before[-1].ink if before else (" " * w,) * h)
    keys = _block(ink, w, h, source, "ink")
    for n, row in ink:
        bad = next((k for k in row if k not in inks and k not in " .@"), "")
        if bad or ("@" in row and not eyes):
            what = f"ink {bad!r} not in `ink:`" if bad else "`@`: no `eyes:`"
            raise PictureError(f"{source}:{n}: {what}")
    return Frame(rows, keys)


def parse(text: str, source: str = "<picture>") -> Picture:
    """A picture from the text of its file; `PictureError` names the
    line."""
    lines = [line.rstrip() for line in text.split("\n")]
    head: dict[str, str] = {}
    at = 0
    while at < len(lines) and not _SIZE.fullmatch(lines[at]):
        line, at = lines[at], at + 1
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition(":")
        if not sep or key not in ("name", "author", "kind", "period", "hold") + (
            "eyes",
            "ink",
        ):
            raise PictureError(f"{source}:{at}: not a header line: {line!r}")
        value = value.strip()
        head[key] = f"{head.get(key, '')} {value}" if key == "ink" else value
        if key in ("period", "hold"):
            try:
                good = all(float(v) > 0 for v in value.split()) and bool(value.split())
            except ValueError:
                good = False
            if not good:
                raise PictureError(f"{source}:{at}: {key}: want positive numbers")
    where = f"{source}: header"
    if not head.get("name"):
        raise PictureError(f"{where}: no name")
    kind = head.get("kind", "abstract")
    if kind not in KINDS:
        raise PictureError(f"{where}: kind {kind!r}: one of {', '.join(KINDS)}")
    eyes = head.get("eyes", "").replace(" ", "")
    if len(eyes) not in (0, 3, 5) or any(_wide(g) for g in eyes):
        raise PictureError(f"{where}: eyes: want 3 or 5 glyphs")
    inks = _inks(head.get("ink", ""), where)
    try:
        hold = tuple(int(v) for v in head.get("hold", "").split())
    except ValueError:
        raise PictureError(f"{where}: hold: want whole numbers") from None

    sizes: dict[Size, tuple[Frame, ...]] = {}
    while at < len(lines):
        match = _SIZE.fullmatch(lines[at])
        assert match is not None
        w, h = int(match[1]), int(match[2])
        opened, at = at + 1, at + 1
        if w < 1 or h < 1 or (w, h) in sizes:
            raise PictureError(f"{source}:{opened}: size {w}x{h}: null or given twice")
        frames: list[Frame] = []
        glyphs: list[tuple[int, str]] = []
        ink: list[tuple[int, str]] | None = None

        while at < len(lines) and not _SIZE.fullmatch(lines[at]):
            line, at = lines[at], at + 1
            if line == "++":
                frames.append(_frame(glyphs, ink, (w, h), source, inks, eyes, frames))
                glyphs, ink = [], None
            elif line == "::":
                if ink is not None:
                    raise PictureError(f"{source}:{at}: `::` twice in a frame")
                ink = []
            else:
                (glyphs if ink is None else ink).append((at, line))
        frames.append(_frame(glyphs, ink, (w, h), source, inks, eyes, frames))
        sizes[w, h] = tuple(frames)
    if not sizes:
        raise PictureError(f"{source}: no size: want a line `== WxH`")
    counts = {len(frames) for frames in sizes.values()}
    if len(counts) != 1:
        raise PictureError(
            f"{source}: sizes with {sorted(counts)} frames: want one count"
        )
    if hold and len(hold) != counts.pop():
        raise PictureError(f"{where}: hold: one number a frame")
    return Picture(
        name=head["name"],
        author=head.get("author") or "anonymous",
        kind=kind,
        period=float(head.get("period", 2.0)),
        hold=hold,
        eyes=eyes,
        inks=inks,
        sizes=sizes,
        source=source,
    )


def load(path: Path | str) -> Picture:
    """The picture of a file."""
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise PictureError(f"{path}: {exc}") from exc
    return parse(text, str(path))


def shipped(ship: Path | None = None) -> list[str] | None:
    """The stems of the ship list (`SHIP`), best first; None without a
    list."""
    try:
        lines = (ship or SHIP).read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    return [s for s in (line.strip() for line in lines) if s and s[0] != "#"]


def collection(
    kinds: Iterable[str] = (),
    files: Iterable[Path | str] = (),
    dirs: Iterable[Path | str] = (),
    candidates: bool = False,
) -> list[Picture]:
    """The pictures to hang: those of `files` and `dirs` (`*.pic`), or
    the collection shipped: the ship list when there is one and
    `candidates` is not set, else every file. Of `kinds` only, if any."""
    files, dirs = list(files), list(dirs)
    paths = [Path(f) for f in files]
    for name in dirs:
        if not Path(name).is_dir():
            raise PictureError(f"{name}: not a directory")
        paths += sorted(Path(name).glob("*.pic"))
    if not files and not dirs:
        paths = sorted(HERE.glob("*.pic"))
        ship = None if candidates else shipped()
        if ship is not None:
            order = {stem: nth for nth, stem in enumerate(ship)}
            paths = sorted(
                (p for p in paths if p.stem in order), key=lambda p: order[p.stem]
            )
    wanted = set(kinds)
    return [p for p in map(load, paths) if not wanted or p.kind in wanted]


@dataclass
class Slot:
    """A nail of the wall and the picture on it. `on_move`, `on_leave`:
    the pointer over its inner box (`Deck.on_hover`)."""

    picture: Picture
    since: float = 0.0
    until: float = 0.0
    now: float = 0.0
    moving: bool = False
    author: bool = False
    hover: bool = False
    look: Size | None = None
    size: Size | None = None
    retitle: Callable[[], None] | None = None

    @property
    def label(self) -> str:
        return self.picture.label(self.author or (self.hover and self.look is not None))

    def _t(self) -> float:
        return self.now - self.since if self.moving else 0.0

    def render(self, w: int, h: int) -> Rows:
        return self.picture.render(w, h, self._t(), self.look if self.hover else None)

    def _seen(self) -> tuple[object, ...]:
        if self.size is None or self.size not in self.picture.sizes:
            return (self.label,)
        return (self.label, self.picture.gaze(*self.size, self.look, self._t()))

    def _point(self, look: Size | None) -> bool:
        """Whether what is seen changed; the plate is then set again."""
        before = self._seen()
        self.look = look
        if not self.hover or self._seen() == before:
            return False
        if self.retitle is not None:
            self.retitle()
        return True

    def on_move(self, x: int, y: int) -> bool:
        return self._point((x, y))

    def on_leave(self) -> bool:
        return self._point(None)


class Hall:
    """Which picture hangs where, and until when. Time is an argument."""

    def __init__(
        self,
        pictures: Sequence[Picture],
        count: int = 1,
        every: float = 600.0,
        seed: int | None = None,
        moving: bool = False,
        author: bool = False,
        hover: bool = False,
    ) -> None:
        if not pictures:
            raise ValueError("no picture to hang")
        self.pictures = list(pictures)
        self.every = every
        self._rng = random.Random(seed)
        self._pile: list[Picture] = []
        self.slots: list[Slot] = []
        count = max(1, min(count, len(self.pictures)))
        for nth in range(count):
            slot = Slot(self._draw(), moving=moving, author=author, hover=hover)
            # Staggered: the wall never changes all at once.
            slot.until = every * (nth + 1) / count
            self.slots.append(slot)

    def _draw(self) -> Picture:
        hung = [slot.picture for slot in self.slots]
        if not self._pile:
            self._pile = self.pictures[:]
            self._rng.shuffle(self._pile)
        for nth, picture in enumerate(self._pile):
            if picture not in hung or len(self.pictures) <= len(hung):
                return self._pile.pop(nth)
        self._pile = []
        return self._draw()

    @property
    def due(self) -> float:
        """When the next picture is hung."""
        return min(slot.until for slot in self.slots)

    def period(self) -> float | None:
        """The shortest period of the pictures that move; None: all
        still."""
        periods = [
            slot.picture.period
            for slot in self.slots
            if slot.moving and slot.picture.frames > 1
        ]
        return min(periods, default=None)

    def next(self, slot: Slot, t: float) -> None:
        """Another picture on `slot`, now."""
        slot.picture = self._draw()
        slot.since, slot.until, slot.look = t, t + self.every, None

    def at(self, t: float) -> list[Slot]:
        """Move to `t`; the slots that took a new picture."""
        new = []
        for slot in self.slots:
            slot.now = t
            if t >= slot.until and len(self.pictures) > len(self.slots):
                self.next(slot, t)
                new.append(slot)
        return new


def _sgr(run: Run) -> str:
    codes = ["1"] if run.get("bold") else []
    if "fg" in run:
        codes.append(f"38;5;{run['fg']}")
    if "bg" in run:
        codes.append(f"48;5;{run['bg']}")
    text = str(run["t"])
    return f"\x1b[{';'.join(codes)}m{text}\x1b[0m" if codes else text


def framed(rows: Rows, w: int, title: str = "", colour: bool = False) -> list[str]:
    """`rows` in the `painting` frame, the plate on the bottom edge."""
    edge = (lambda t: f"\x1b[38;5;{_GOLD}m{t}\x1b[0m") if colour else (lambda t: t)
    paint = _sgr if colour else (lambda run: str(run["t"]))
    plate = f" {title} " if title and len(title) + 3 <= w else ""
    out = [edge("╔" + "".join("═╤"[i % 2] for i in range(w)) + "╗")]
    for y, row in enumerate(rows):
        out.append(edge("║╟"[y % 2]) + "".join(map(paint, row)) + edge("║╢"[y % 2]))
    rail = "".join("═╧"[i % 2] for i in range(w - len(plate)))
    out.append(edge("╚" + rail) + plate + edge("╝"))
    return out


def show(pictures: Sequence[Picture], args: argparse.Namespace) -> None:
    """Print each picture in its frame: every size, or the largest that
    fits `--print WxH`; with `-m`, every frame. ANSI colours on a tty."""
    colour = sys.stdout.isatty() or args.colour
    for picture in pictures:
        sizes = sorted(picture.sizes, key=lambda s: (s[0] * s[1], s))
        if args.print is not True:
            fit = [s for s in sizes if s[0] <= args.print[0] and s[1] <= args.print[1]]
            sizes = fit[-1:]
        hold = picture.hold or (1,) * picture.frames
        starts = [sum(hold[:n]) * picture.period for n in range(picture.frames)]
        for w, h in sizes:
            shots = [
                framed(picture.render(w, h, t), w, picture.label(args.label), colour)
                for t in (starts if args.move else starts[:1])
            ]
            print(f"{picture.stem or picture.name}  {picture.kind}  {w}x{h}")
            for parts in zip(*shots, strict=True):
                print("  ".join(parts))


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    hall = Hall(
        args.pictures,
        count=args.count,
        every=args.every,
        seed=args.seed,
        moving=args.move,
        author=args.label,
        hover=args.hover,
    )
    deck = _widgets.Deck(pilot, args)
    start = clock.monotonic()

    def hang(slot: Slot, widget: object = None) -> None:
        sizes = list(slot.picture.sizes)
        if widget is None:
            tag = "gallery" if not hung else f"gallery-{len(hung) + 1}"
            hung.append(deck.add(tag, sizes, slot.render, title=slot.label))
        else:
            deck.resize(widget, sizes)  # type: ignore[arg-type]
            widget.set(title=slot.label)  # type: ignore[attr-defined]

    def wire(slot: Slot, widget: object) -> None:
        def move(x: int, y: int) -> bool:
            slot.size = widget.size  # type: ignore[attr-defined]
            return slot.on_move(x, y)

        slot.retitle = lambda: widget.set(title=slot.label)  # type: ignore[attr-defined]
        deck.on_hover(widget, move, slot.on_leave)  # type: ignore[arg-type]

    hung: list[object] = []
    for slot in hall.slots:
        hang(slot)
        if args.hover:
            wire(slot, hung[-1])

    def tick() -> None:
        for slot, widget in zip(hall.slots, hung, strict=True):
            slot.size = widget.size  # type: ignore[attr-defined]
        for slot in hall.at((clock.monotonic() - start) * args.speed):
            hang(slot, hung[hall.slots.index(slot)])

    end = None if seconds is None else start + seconds
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        span = max(0.05, start + hall.due / args.speed - now)
        if end is not None:
            span = min(span, end - now)
        period = hall.period()
        step = span if period is None else min(span, max(0.2, period / args.speed))
        deck.run(period=step, seconds=span, tick=tick)
        if clock.monotonic() - now < span - 0.02:
            return  # the session ended
        tick()


def _span(text: str) -> float:
    match = re.fullmatch(r"(\d+(?:\.\d+)?)([smh]?)", text)
    if match is None or float(match[1]) <= 0:
        raise argparse.ArgumentTypeError(
            f"not a duration: {text} (as 90, 30s, 10m, 1h)"
        )
    return float(match[1]) * {"": 1, "s": 1, "m": 60, "h": 3600}[match[2]]


def _box(text: str) -> Size:
    match = re.fullmatch(r"(\d+)x(\d+)", text)
    if match is None:
        raise argparse.ArgumentTypeError(f"not a size: {text} (as 24x8)")
    return int(match[1]), int(match[2])


def _kinds(text: str) -> list[str]:
    names = [name for name in text.split(",") if name]
    for name in names:
        if name not in KINDS:
            raise argparse.ArgumentTypeError(f"{name}: not one of {', '.join(KINDS)}")
    return names


def _positive(text: str) -> float:
    try:
        value = float(text)
    except ValueError:
        value = 0.0
    if value <= 0:
        raise argparse.ArgumentTypeError(f"not a positive number: {text}")
    return value


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("gallery", "Framed pictures, hung one after the other.")
    _widgets.options(out, border="painting")
    out.add_argument(
        "--every",
        type=_span,
        default=600.0,
        metavar="TIME",
        help="a picture hangs this long: 90, 30s, 10m, 1h [default: 10m]",
    )
    out.add_argument(
        "--kind",
        type=_kinds,
        default=[],
        metavar="LIST",
        help=f"kinds to hang, comma separated: {', '.join(KINDS)} [default: all]",
    )
    out.add_argument(
        "--file", action="append", default=[], help="a picture file; may be repeated"
    )
    out.add_argument(
        "--dir",
        action="append",
        default=[],
        help="a directory of *.pic; may be repeated",
    )
    out.add_argument("-m", "--move", action="store_true", help="the pictures move")
    out.add_argument(
        "-l", "--label", action="store_true", help="the plate names the author too"
    )
    out.add_argument(
        "-o",
        "--hover",
        action="store_true",
        help="under the pointer: eyes follow it, the plate names the author",
    )
    out.add_argument("-w", "--wall", action="store_true", help="three pictures at once")
    out.add_argument("-a", "--all", action="store_true", help="-m -l -o -w")
    out.add_argument(
        "--count", type=int, metavar="N", help="pictures at once [default: 1; -w: 3]"
    )
    out.add_argument("--seed", type=int, help="the order of the pictures")
    out.add_argument(
        "--speed",
        type=_positive,
        default=1.0,
        metavar="N",
        help="time N times as fast [default: 1]",
    )
    out.add_argument(
        "--candidates",
        action="store_true",
        help="every picture of the collection, not the ship list only",
    )
    out.add_argument(
        "--print",
        nargs="?",
        const=True,
        type=_box,
        metavar="WxH",
        help="print the pictures, no driver; WxH: the largest size that fits",
    )
    out.add_argument("--colour", action="store_true", help="--print: ANSI colours")
    args = _cli.parse(out, argv)
    if args.all:
        args.move = args.label = args.hover = args.wall = True
    if args.count is None:
        args.count = 3 if args.wall else 1
    try:
        args.pictures = collection(args.kind, args.file, args.dir, args.candidates)
    except PictureError as exc:
        out.error(str(exc))
    if not args.pictures:
        out.error("no picture to hang")
    if args.print:
        show(args.pictures, args)
        return 0
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
