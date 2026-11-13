# SPDX-License-Identifier: GPL-3.0-or-later
"""Globe: a snow globe in one widget, from 11x5 to 30x13.

    ptyrc-app globe
    ptyrc-app globe -k -d           # shaken now and then; the snow lies
    ptyrc-app globe -a --scene fir
    ptyrc-app globe -a --print --at 12

What it shows:

- `scene(t, w, h, look, shaken)`: pure. A flake is a function of the
  time: where it falls, how it swirls after a shake. No state but the
  time of the last shake;
- a widget without a border: the dome is the content;
- `_widgets.Deck`: the driver picks the variant and the place.

Bare: a house between two trees under a dome, a little snow falling.
More, each on its own flag; `-a` for all:

- `-k`: a shake now and then (`--every`, 45 s): the snow swirls, then
  falls thick, then thins.
- `-d`: the snow lies: drifts on the ground, then the roof and the
  trees turn white; a shake lifts it all.
- `-l`: lights: the window flickers, the trees wear a garland, a
  glint slides along the glass.
- `-r`: the scene is drawn at random (`SCENES`), another at each
  shake; `--scene NAME` names it.

Hover (`Shaker`): the pointer over the globe shakes it.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from collections.abc import Callable, Sequence
from dataclasses import dataclass

from ptyrc import Pilot, clock

from . import _canvas, _cli, _widgets

__all__ = [
    "DOMES",
    "SCENES",
    "SIZES",
    "Dome",
    "Look",
    "Shaker",
    "auto",
    "main",
    "scene",
    "which",
]

Size = tuple[int, int]
Run = dict[str, object]
Rows = list[list[Run]]
Cell = dict[str, object]

PERIOD = 0.1  # seconds between two draws
EVERY = 45.0  # seconds between two shakes
SETTLE = 20.0  # seconds for the snow to lie, after a shake
SWIRL = 1.3  # seconds: the swirl dies by e
STORM = 6.0  # seconds: the thick snow thins by e
THIN = 0.4  # share of the flakes that fall when all is calm
GLASS, GLINT, SNOW, BASE = 110, 231, 255, 94
FLAKES = "··..*"
GARLAND = (196, 226, 46, 51, 201)
#: Ink of a scene: a letter of its mask to a colour and bold.
INK: dict[str, tuple[int, bool]] = {
    "h": (180, False),  # walls
    "R": (180, False),  # roof: white under the snow
    "w": (220, True),  # window
    "t": (71, False),  # tree
    "T": (71, False),  # top of a tree: white under the snow
    "s": (255, True),  # snowman
    "k": (244, False),  # hat, buttons
    "o": (208, True),  # bulbs
    "y": (226, True),  # star
    "c": (250, False),  # cross
}


def _art(text: str) -> tuple[str, ...]:
    rows = text.strip("\n").split("\n")
    w = max(len(row) for row in rows)
    return tuple(row.ljust(w) for row in rows)


@dataclass(frozen=True)
class Dome:
    """A globe: its `art`, glass over a base; `ground`: the row the
    scene stands on; `scale`: which art of a scene fits."""

    art: tuple[str, ...]
    ground: int
    scale: str

    @property
    def size(self) -> Size:
        return (len(self.art[0]), len(self.art))

    def span(self, y: int) -> tuple[int, int]:
        """First and last cell inside the glass on row `y`."""
        row = self.art[y]
        ink = [x for x, c in enumerate(row) if c != " "]
        return ink[0] + 1, ink[-1] - 1

    def free(self) -> set[tuple[int, int]]:
        """The cells a flake may be in."""
        out = set()
        for y in range(1, self.ground + 1):
            x0, x1 = self.span(y)
            out |= {(x, y) for x in range(x0, x1 + 1) if self.art[y][x] == " "}
        return out


DOMES = (
    Dome(
        _art(r"""
  .-'''-.
 /       \
|         |
 \_______/
 /_______\
"""),
        3,
        "s",
    ),
    Dome(
        _art(r"""
    .-'''''-.
  .'         '.
 /             \
|               |
 \             /
  '._________.'
  /___________\
 '-------------'
"""),
        4,
        "m",
    ),
    Dome(
        _art(r"""
    _.-'''''''-._
  .'             '.
 /                 \
|                   |
|                   |
|                   |
 \                 /
  '._____________.'
  /_______________\
 '-----------------'
"""),
        6,
        "l",
    ),
    Dome(
        _art(r"""
        _..-'''''''-.._
     .-'               '-.
   .'                     '.
  /                         \
 |                           |
 |                           |
 |                           |
 |                           |
  \                         /
   '.                     .'
     '-._______________.-'
     /___________________\
    '---------------------'
"""),
        9,
        "l",
    ),
)
SIZES: tuple[Size, ...] = tuple(dome.size for dome in DOMES)


def _pair(text: str) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """Art and its mask, written one over the other, a blank line
    between: a letter of `INK` under each glyph."""
    art, mask = text.strip("\n").split("\n\n")
    rows, inks = art.split("\n"), mask.split("\n")
    w = max(len(row) for row in rows + inks)
    return tuple(r.ljust(w) for r in rows), tuple(r.ljust(w) for r in inks)


#: Scenes: for each, the art and the mask at the three scales.
SCENES: dict[str, dict[str, tuple[tuple[str, ...], tuple[str, ...]]]] = {
    "house": {
        "s": _pair(r"""
 /\  ♣
/[]\ ♣

 RR  T
hwwh t
"""),
        "m": _pair(r"""
     /\
 ♣  /  \  ♣
♣♣♣/_[]_\♣♣♣

     RR
 T  R  R  T
ttthhwwhhttt
"""),
        "l": _pair(r"""
      /\
 ♣   /  \   ♣
♣♣♣ /_[]_\ ♣♣♣

      RR
 T   R  R   T
ttt hhwwhh ttt
"""),
    },
    "snowman": {
        "s": _pair(r"""
 (") ♣
( : )♣

 sks T
s k st
"""),
        "m": _pair(r"""
     ▄
 ♣  (")  ♣
♣♣-( : )-♣♣

     k
 T  sks  T
ttks k sktt
"""),
        "l": _pair(r"""
      ▄
 ♣   (")   ♣
♣♣♣-( : )-♣♣♣

      k
 T   sks   T
tttks k skttt
"""),
    },
    "church": {
        "s": _pair(r"""
 †   ♣
/[]\_♣

 c   T
RwwRht
"""),
        "m": _pair(r"""
   †
♣ /\___  ♣
♣♣|[]|_|♣♣♣

   c
T RRRRR  T
tthwwhhhttt
"""),
        "l": _pair(r"""
    †
   /\_____
♣ /  \_____\ ♣
♣♣|[]|_[]_|♣♣♣

    c
   RRRRRRR
T R  RRRRRRR T
tthwwhhwwhhttt
"""),
    },
    "fir": {
        "s": _pair(r"""
 /o\ ♣
/.o.\♣

 ToT T
ttottt
"""),
        "m": _pair(r"""
     *
 ♣  /o\  ♣
♣♣♣/.o.\♣♣♣

     y
 T  ToT  T
tttttottttt
"""),
        "l": _pair(r"""
      *
     /o\
 ♣  /.o.\  ♣
♣♣♣/o.o.o\♣♣♣

      y
     ToT
 T  ttott  T
ttttotototttt
"""),
    },
}


@dataclass(frozen=True)
class Look:
    """What is drawn, as the options name it."""

    shake: bool = False
    drift: bool = False
    lights: bool = False
    random: bool = False
    scene: str = "house"
    seed: int = 1
    every: float = EVERY


def _hash(seed: int, x: int, y: int = 0) -> float:
    n = (x * 374761393 + y * 668265263 + seed * 2246822519) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 0xFFFF


def auto(t: float, look: Look) -> float | None:
    """With `shake`: when the globe last shook by itself, at `t`; once
    an `every`, not on the beat. None: not yet, or never."""
    if not look.shake:
        return None
    nth = int(t // look.every)
    for k in (nth, nth - 1):
        at = (k + 0.5 + 0.4 * (_hash(look.seed, k) - 0.5)) * look.every
        if k >= 0 and at <= t:
            return at
    return None


def which(look: Look, shakes: int = 0) -> str:
    """The scene shown: the one named; with `random`, one drawn by the
    seed, another after each shake."""
    if not look.random:
        return look.scene
    names = sorted(SCENES)
    first = int(_hash(look.seed, 99) * len(names))
    return names[(first + shakes) % len(names)]


def _flakes(
    dome: Dome, t: float, seed: int, since: float | None
) -> set[tuple[int, int]]:
    """The cells of the flakes in the air at `t`; `since`: seconds
    since the last shake."""
    free = dome.free()
    top, low = 1, dome.ground
    rows = low - top + 1
    w, _ = dome.size
    swirl = 0.0 if since is None else math.exp(-since / SWIRL)
    storm = 0.0 if since is None else math.exp(-since / STORM)
    share = THIN + (1 - THIN) * storm
    out = set()
    count = max(3, round(len(free) / 4.5))
    for i in range(count):
        rng = random.Random(seed * 7919 + i)
        if i / count >= share:
            continue
        speed = rng.uniform(0.5, 1.3)  # rows a second
        lane, phase = rng.random(), rng.uniform(0, rows + 1.0)
        sway, pace, turn = rng.uniform(0.3, 1.0), rng.uniform(0.6, 1.6), rng.random()
        spin = rng.uniform(4.0, 9.0) * rng.choice((-1, 1))
        y = (phase + speed * t) % (rows + 1.0)  # past the ground: it rests
        x0, x1 = dome.span(top + min(int(y), rows - 1))
        x = x0 + lane * (x1 - x0) + sway * math.sin(pace * t + turn * math.tau)
        if swirl > 0.02 and since is not None:
            angle = turn * math.tau + spin * since
            x += swirl * 0.45 * w * math.cos(angle)
            y = (y - swirl * rows * (0.6 + 0.4 * math.sin(angle))) % (rows + 1.0)
            x0, x1 = dome.span(top + min(int(y), rows - 1))
            x = x0 + (x - x0) % (x1 - x0 + 1)
        cell = (round(x), top + int(y))
        if cell in free:
            out.add(cell)
    return out


def _dome(w: int, h: int) -> Dome:
    return next(d for d in DOMES if d.size == (w, h))


def scene(
    t: float,
    w: int,
    h: int,
    look: Look | None = None,
    shaken: float | None = None,
    shakes: int = 0,
) -> Rows:
    """The globe of `w` by `h` cells (one of `SIZES`) at `t` seconds.
    `shaken`: when a hand last shook it; `shakes`: how many times."""
    look = look or Look()
    dome = _dome(w, h)
    last = max((s for s in (auto(t, look), shaken) if s is not None), default=None)
    since = None if last is None or last > t else t - last
    if look.shake:
        shakes += int(t // look.every) + (auto(t, look) is not None) - 1
    grid: list[list[Cell]] = []
    for y, row in enumerate(dome.art):
        fg = GLASS if y < h - (1 if h < 6 else 2) else BASE
        grid.append([{"t": c, "fg": fg} if c != " " else {"t": " "} for c in row])
    # How much snow lies, 0 to 1.
    lying = 0.0
    if look.drift:
        lying = 1.0 if since is None else min(1.0, max(0.0, since - 1.0) / SETTLE)
    art, mask = SCENES[which(look, max(0, shakes))][dome.scale]
    x0, x1 = dome.span(dome.ground)
    left = x0 + (x1 - x0 + 1 - len(art[0])) // 2
    for dy, (row, inks) in enumerate(zip(art, mask, strict=True)):
        y = dome.ground - len(art) + 1 + dy
        for dx, (glyph, ink) in enumerate(zip(row, inks, strict=True)):
            if glyph == " ":
                continue
            x = left + dx
            fg, bold = INK[ink]
            if ink == "R" and lying > 0.4 or ink == "T" and lying > 0.7:
                fg, bold = SNOW, True
            if look.lights:
                if ink == "w":  # a fire inside
                    flick = _hash(look.seed, int(t * 5), x)
                    fg = 220 if flick < 0.6 else 214 if flick < 0.85 else 228
                elif ink in "to" and _hash(look.seed, x, y) < (
                    0.3 if ink == "t" else 1
                ):
                    step = int(t * 1.5 + 5 * _hash(look.seed, y, x))
                    fg, bold = GARLAND[(step + x) % len(GARLAND)], True
                    if glyph == "♣" and step % 3:
                        glyph = "•"
            grid[y][x] = {"t": glyph, "fg": fg, "bold": bold}
    for x in range(x0, x1 + 1):
        # Drifts: the ground fills, unevenly.
        cell = grid[dome.ground][x]
        level = lying * (0.45 + 0.55 * _hash(look.seed + 3, x))
        if cell["t"] == "_" and cell["fg"] == GLASS and level > 0.2:
            grid[dome.ground][x] = {"t": "_", "fg": SNOW, "bold": True}
        elif cell["t"] == " " and level > 0.2:
            glyph = "." if level < 0.45 else "▁" if level < 0.75 else "▂"
            grid[dome.ground][x] = {"t": glyph, "fg": SNOW}
    for i, (x, y) in enumerate(sorted(_flakes(dome, t, look.seed, since))):
        if grid[y][x]["t"] == " ":
            glyph = FLAKES[int(_hash(look.seed, x * 31 + i, y) * len(FLAKES))]
            grid[y][x] = {"t": glyph, "fg": SNOW, "bold": True}
    if look.lights:
        # A glint: it slides along the top of the glass, then waits.
        at = (t % 9.0) / 1.6 * w
        for y in range(min(3, dome.ground)):
            for x, cell in enumerate(grid[y]):
                if cell["t"] != " " and cell.get("fg") == GLASS and abs(x - at) <= 1.5:
                    grid[y][x] = {**cell, "fg": GLINT, "bold": True}
    return [_runs(row) for row in grid]


def _runs(cells: Sequence[Cell]) -> list[Run]:
    """Cells to runs: neighbours of one style joined."""
    out: list[Run] = []
    for cell in cells:
        if out and {**out[-1], "t": ""} == {**cell, "t": ""}:
            out[-1]["t"] = str(out[-1]["t"]) + str(cell["t"])
        else:
            out.append(dict(cell))
    return out


class Shaker:
    """The hover: a shake when the pointer comes over the globe, and
    one more each `again` seconds it keeps moving there. `now()`: the
    time of the scene.

        hand = Shaker(now)
        hand.on_move(x, y)
        scene(t, w, h, look, hand.at, hand.count)
    """

    def __init__(self, now: Callable[[], float], again: float = 3.0) -> None:
        self.now = now
        self.again = again
        self.at: float | None = None
        self.count = 0
        self.over = False

    def on_move(self, x: int, y: int) -> None:
        now = self.now()
        if not self.over or self.at is None or now - self.at >= self.again:
            self.at = now
            self.count += 1
        self.over = True

    def on_leave(self) -> None:
        self.over = False


def _look(args: argparse.Namespace) -> Look:
    return Look(
        shake=args.shake,
        drift=args.drift,
        lights=args.lights,
        random=args.random and args.scene is None,
        scene=args.scene or "house",
        seed=1 if args.seed is None else args.seed,
        every=args.every,
    )


def show(args: argparse.Namespace) -> int:
    """`--print`: the globes at `--at`, or one size; no driver."""
    look = _look(args)
    colour = sys.stdout.isatty()
    for w, h in SIZES if args.print == (0, 0) else (args.print,):
        rows = scene(args.at, w, h, look)
        if colour:
            print("\n".join(_canvas.ansi(rows)))
        else:
            print("\n".join("".join(str(run["t"]) for run in row) for row in rows))
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    look = _look(args)
    start = clock.monotonic()

    def now() -> float:
        return (clock.monotonic() - start) * args.speed

    hand = Shaker(now)

    def render(w: int, h: int) -> Rows:
        return scene(now(), w, h, look, hand.at, hand.count)

    deck = _widgets.Deck(pilot, args)
    widget = deck.add("globe", list(SIZES), render)
    # The next draw shows it: no draw of its own.
    deck.on_hover(
        widget,
        lambda x, y: hand.on_move(x, y) or False,
        lambda: hand.on_leave() or False,
    )
    deck.run(period=PERIOD, seconds=seconds)


def _size(text: str) -> Size:
    try:
        w, h = (int(n) for n in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"not WxH: {text}") from None
    if (w, h) not in SIZES:
        sizes = ", ".join(f"{w}x{h}" for w, h in SIZES)
        raise argparse.ArgumentTypeError(f"not a globe: {text}; one of {sizes}")
    return (w, h)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("globe", "A snow globe in a quiet room of the screen.")
    _widgets.options(out, border="none")
    out.add_argument("-k", "--shake", action="store_true", help="shaken now and then")
    out.add_argument("-d", "--drift", action="store_true", help="the snow lies")
    out.add_argument(
        "-l", "--lights", action="store_true", help="a lit window, a garland, a glint"
    )
    out.add_argument(
        "-r", "--random", action="store_true", help="a scene at random, one a shake"
    )
    out.add_argument("-a", "--all", action="store_true", help="-k -d -l -r")
    out.add_argument(
        "--scene", choices=sorted(SCENES), help="the scene [default: house]"
    )
    out.add_argument(
        "--every",
        type=float,
        default=EVERY,
        metavar="SECONDS",
        help=f"with -k: between two shakes [default: {EVERY:.0f}]",
    )
    out.add_argument(
        "--speed", type=float, default=1.0, metavar="N", help="N seconds a second"
    )
    out.add_argument("--seed", type=int, help="same seed, same snow")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(0, 0),
        metavar="WxH",
        help="no driver: print every globe, or the size given",
    )
    out.add_argument(
        "--at",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="with --print: that long after the start",
    )
    args = _cli.parse(out, argv)
    if args.every <= 0:
        out.error("--every: more than 0")
    if args.all:
        args.shake = args.drift = args.lights = args.random = True
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
