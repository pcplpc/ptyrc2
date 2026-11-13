# SPDX-License-Identifier: GPL-3.0-or-later
"""Fire: one widget, from a candle (3x3) to a hearth (45x13).

    ptyrc-app fire
    ptyrc-app fire -b -l            # half blocks, logs and a mantel
    ptyrc-app fire -a --burn 600    # all; burnt down in ten minutes
    ptyrc-app fire -a --print 33x11 --at 120

What it shows:

- `scene(t, w, h, look)`: pure. The flame is a field of heat, a
  function of the place and the time: tongues that lean and lick,
  turbulence that rises. No state: any moment can be drawn, a burn
  played fast (`--speed`);
- `_widgets.Deck`: the driver picks the variant and the place, the
  deck draws the one shown ten times a second.

Bare: a flame of glyphs, ` .:^*x#`, red to yellow. More, each on its
own flag; `-a` for all:

- `-b`: half blocks: two rows of heat a cell, the whole ramp.
- `-l`: what burns: a candle on its dish, a brazier, logs in a hearth
  (border `hearth`).
- `-c`: the fire burns down to embers over `--burn` seconds (3600),
  then a log is put on.
- `-k`: sparks rise and die.
- `-g`: the light of the fire: a halo; with `-l`, the bricks behind
  the logs, lit near the flames, dark away from them.

Hover (`Poke`): the flames rise and sparks fly for a few seconds.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from collections.abc import Sequence
from dataclasses import dataclass

from ptyrc import Pilot, clock

from . import _cli, _widgets

__all__ = [
    "BORDERS",
    "GLYPHS",
    "RAMP",
    "SIZES",
    "Look",
    "Poke",
    "ansi",
    "boxed",
    "fuel",
    "heat",
    "main",
    "scene",
    "sparks",
]

Size = tuple[int, int]
Run = dict[str, object]
Rows = list[list[Run]]
Cell = dict[str, object]

SIZES: tuple[Size, ...] = ((3, 3), (5, 4), (9, 5), (15, 7), (23, 9), (33, 11), (45, 13))
PERIOD = 0.1  # seconds between two draws
GLYPHS = " .:^*x#"
#: Heat to a colour of the 256, cold to hot.
RAMP = (52, 88, 124, 160, 196, 202, 208, 214, 220, 226, 227, 228, 229, 230)
BURN = 3600.0  # seconds from a log to the next
FULL = 0.85  # the fuel of a fire that does not burn down
POKED = 4.0  # seconds a poke lasts
WAX, WICK, BRASS, IRON = 230, 238, 136, 240
BARK = (94, 58, 95)
LIT = (131, 95, 52, 237, 235)  # bricks, near the flames to far

BORDERS: dict[str, dict[str, object]] = {
    # A mantel over jambs of brick.
    "hearth": {
        "corners": "▛▜▙▟",
        "top": "▀",
        "bottom": "▄",
        "left": "▌",
        "right": "▐",
        "style": {"fg": 131},
        "title": {"style": {"fg": 180}},
    },
}


@dataclass(frozen=True)
class Look:
    """What is drawn, as the options name it."""

    blocks: bool = False
    logs: bool = False
    cycle: bool = False
    sparks: bool = False
    glow: bool = False
    seed: int = 1
    burn: float = BURN


def _hash(seed: int, x: int, y: int) -> float:
    n = (x * 374761393 + y * 668265263 + seed * 2246822519) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 0xFFFF


def _noise(seed: int, x: float, y: float) -> float:
    """Value noise in [0, 1], smooth in `x` and `y`."""
    xi, yi = math.floor(x), math.floor(y)
    fx, fy = x - xi, y - yi
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    top = _hash(seed, xi, yi) * (1 - fx) + _hash(seed, xi + 1, yi) * fx
    low = _hash(seed, xi, yi + 1) * (1 - fx) + _hash(seed, xi + 1, yi + 1) * fx
    return top * (1 - fy) + low * fy


def fuel(t: float, look: Look) -> float:
    """How much burns at `t`, 0 to 1. With `cycle`: a log catches, burns
    down to embers over `look.burn` seconds, and the next is put on."""
    if not look.cycle:
        return FULL
    p = (t / look.burn) % 1.0
    if p < 0.02:  # it catches
        return 0.3 + 0.7 * p / 0.02
    if p < 0.7:
        return 1.0 - 0.5 * (p - 0.02) / 0.68
    if p < 0.9:  # it dies
        return 0.5 - 0.42 * (p - 0.7) / 0.2
    return 0.08 - 0.04 * (p - 0.9) / 0.1  # embers


def _strength(t: float, look: Look, poke: float | None) -> float:
    f = fuel(t, look)
    if poke is not None and 0 <= poke < POKED:
        f = min(1.25, f + 0.45 * math.exp(-poke / 1.2))
    return f


def heat(
    x: float, y: float, t: float, w: int, top: float, f: float, seed: int
) -> float:
    """Heat, 0 to 1, at column `x`, `y` half rows over the bed of a fire
    `w` columns wide that may rise `top` half rows, of strength `f`."""
    half = w / 2
    cx = (w - 1) / 2
    if w <= 5:
        # One flame: a tear that sways from its foot.
        sway = (_noise(seed, 7.0, t * 1.1) - 0.5) * 1.6
        u = (x - cx - sway * (y / max(top, 1.0)) ** 1.5) / (half * 0.75)
        if abs(u) >= 1:
            return 0.0
        rise = top * (0.7 + 0.3 * f) * (0.9 + 0.2 * _noise(seed, 3.0, t * 4.0))
        v = y / (rise * (1 - abs(u) ** 1.6))
        return max(0.0, min(1.0, (1 - v) * 2.4)) if v < 1 else 0.0
    u = (x - cx) / (half * 0.94)
    if abs(u) >= 1:
        return 0.0
    # Tongues: the height wanders along the bed and in time.
    lick = _noise(seed, x * 0.42 + 50.0, t * 1.4)
    wide = _noise(seed + 3, x * 0.13, t * 0.5)
    rise = top * f * (1 - u * u) ** 0.7 * (0.25 + 0.8 * lick**1.3 + 0.4 * wide)
    rise = min(rise, top * 1.05)
    if rise <= 0.05:
        return 0.0
    v = y / rise
    if v >= 1:
        return 0.0
    lean = (_noise(seed + 5, y * 0.1, t * 0.7) - 0.5) * 2.5
    churn = _noise(seed + 1, x * 0.5 + lean, y * 0.33 - t * 3.2)
    fine = _noise(seed + 2, x * 1.3, y * 0.9 - t * 5.5)
    value = (1 - v) ** 0.8 * (0.62 + 0.6 * churn) + 0.18 * (fine - 0.5)
    return max(0.0, min(1.0, value))


def _embers(x: int, t: float, w: int, seed: int) -> float:
    """The bed: a slow glow along the bottom, 0 to 1."""
    u = (x - (w - 1) / 2) / (w / 2)
    glow = _noise(seed + 9, x * 0.9, t * 0.35) * (1 - abs(u) ** 3)
    return max(0.0, glow)


def _colour(value: float) -> int:
    return RAMP[min(len(RAMP) - 1, int(value * len(RAMP)))]


def sparks(
    t: float, w: int, h: int, f: float, seed: int, poke: float | None = None
) -> dict[tuple[int, int], Cell]:
    """The sparks in flight at `t` over a fire `w` by `h` cells of
    strength `f`: a cell each. A slot throws one spark a period, the
    same at the same time; a poke `poke` seconds old throws a burst."""
    out: dict[tuple[int, int], Cell] = {}
    cx = (w - 1) / 2

    def fly(rng: random.Random, age: float, life: float) -> None:
        if not 0 <= age < life:
            return
        x0 = cx + rng.uniform(-0.3, 0.3) * w
        speed = rng.uniform(0.5, 1.0) * h  # rows per second
        drift = rng.uniform(-1.5, 1.5)
        swing, phase = rng.uniform(0.3, 1.0), rng.uniform(0, math.tau)
        k = age / life
        up = speed * life * (k - 0.45 * k * k)  # it brakes
        x = x0 + drift * age + swing * math.sin(phase + 5 * age)
        y = h - 1 - 0.3 * h * f - up
        cell = (round(x), round(y))
        if 0 <= cell[0] < w and 0 <= cell[1] < h:
            if k < 0.4:
                out[cell] = {"t": "*", "fg": 227, "bold": True}
            elif k < 0.75:
                out[cell] = {"t": "·", "fg": 214, "bold": True}
            else:
                out[cell] = {"t": ".", "fg": 166}

    if w < 3 or h < 3:
        return out
    for slot in range(max(1, w // 6)):
        base = random.Random(seed * 7919 + slot)
        period, offset = base.uniform(1.6, 4.5), base.uniform(0, 5)
        nth, age = divmod(t + offset, period)
        rng = random.Random(seed * 104729 + slot * 1009 + int(nth))
        if rng.random() < 0.25 + 0.75 * f:  # a low fire throws few
            fly(rng, age, rng.uniform(0.7, 1.5))
    if poke is not None and 0 <= poke < POKED:
        nth = int((t - poke) * 10)  # the burst of this poke
        for i in range(3 + w // 4):
            rng = random.Random(seed * 15485863 + nth * 31 + i)
            fly(rng, poke - rng.uniform(0, 0.8), rng.uniform(0.8, 1.8))
    return out


def _furniture(
    t: float, w: int, h: int, f: float, seed: int
) -> tuple[list[list[Cell]], int]:
    """What burns, with `logs`: its rows, the lowest of the box, and
    how many of them the flames may lick (the fire sits that deep)."""

    def cells(text: str, fg: int, **style: object) -> list[Cell]:
        return [{"t": c, "fg": fg, **style} if c != " " else {"t": " "} for c in text]

    if w <= 5:
        # A candle; on a dish when there is a row for it.
        body = cells("█".center(w) if w < 5 else "▐█▌".center(w), WAX)
        if h < 4:
            return [body], 0
        return [body, cells("▀▀▀".center(w), BRASS)], 0
    if w <= 12:
        bowl = cells("◥" + "█" * (w - 2) + "◤", IRON)
        for x in range(1, w - 1):  # coals show along the rim
            glow = _embers(x, t, w, seed) * (0.5 + 0.5 * f)
            if glow > 0.45:
                bowl[x] = {"t": "▀", "fg": _colour(glow * 0.6), "bg": IRON}
        legs = cells(" /" + " " * (w - 4) + "\\ ", IRON)
        return [bowl, legs], 0
    # Two logs, the shorter across the longer, on andirons.
    long = w - 6
    short = max(5, long * 2 // 3)
    lean = (long - short) // 3

    def log(n: int, row: int) -> list[Cell]:
        out: list[Cell] = []
        for i in range(n):
            x = i + row * 17
            bark = BARK[int(_hash(seed + row, x, 0) * len(BARK))]
            glyph = "▓" if _hash(seed + row, x, 1) < 0.7 else "▒"
            crack = _noise(seed + 20 + row, x * 0.8, t * 0.5)
            # More of them as the fire dies: the logs turn to coals.
            hot = (crack - 0.3 - 0.3 * min(1.0, f)) * 2.2
            if hot > 0.12:  # embers in the cracks of the bark
                out.append({"t": "▒", "fg": _colour(min(0.55, hot)), "bg": 52})
            else:
                out.append({"t": glyph, "fg": bark})
        end: Cell = {"fg": 137}
        return [{"t": "▐", **end}, *out[1:-1], {"t": "▌", **end}]

    low = [{"t": " "} for _ in range(w)]
    low[1], low[w - 2] = {"t": "╨", "fg": IRON}, {"t": "╨", "fg": IRON}
    low[3 : 3 + long] = log(long, 0)
    high = [{"t": " "} for _ in range(w)]
    high[1], high[w - 2] = {"t": "╥", "fg": IRON}, {"t": "╥", "fg": IRON}
    high[3 + lean : 3 + lean + short] = log(short, 1)
    return [high, low], 1


def _bricks(x: int, y: int) -> str:
    """A wall of bricks three cells long, a course a row."""
    return "┬──┴──"[(x + 3 * (y % 2)) % 6]


def scene(
    t: float, w: int, h: int, look: Look | None = None, poke: float | None = None
) -> Rows:
    """The fire at `t` seconds in a box of `w` by `h` cells. `poke`:
    seconds since it was poked, None when it was not."""
    look = look or Look()
    seed = look.seed
    f = _strength(t, look, poke)
    grid: list[list[Cell]] = [[{"t": " "} for _ in range(w)] for _ in range(h)]
    floor = h  # rows of flame: those over the furniture
    licked = 0
    if look.logs:
        rows, licked = _furniture(t, w, h, f, seed)
        floor = h - len(rows)
        grid[floor:] = rows
    rows_of_fire = floor + licked
    top = 2.0 * rows_of_fire  # half rows
    if look.logs and w <= 5:
        top = 2.0 * floor

    def at(x: int, y: float) -> float:
        """Heat at half row `y` from the bed."""
        value = heat(x, y, t, w, top, f, seed)
        if y < 1.0 and w > 5:
            value = max(value, 0.45 * _embers(x, t, w, seed))
        return value

    lit = 0.0  # the heat of the whole, for the light
    for row in range(rows_of_fire):
        y = 2 * (rows_of_fire - 1 - row)  # half rows from the bed, the lower
        for x in range(w):
            if look.blocks:
                low, high = at(x, y + 0.5), at(x, y + 1.5)
            else:
                low = high = at(x, y + 1.0)
            lit += low + high
            under = grid[row][x]
            if under["t"] != " ":
                # A log in the flames: lit from behind.
                if "bg" not in under and max(low, high) >= 0.25:
                    grid[row][x] = {**under, "bg": _colour(0.55 * max(low, high))}
                continue
            if not look.blocks:
                glyph = GLYPHS[min(len(GLYPHS) - 1, int(low * len(GLYPHS)))]
                if glyph != " ":
                    grid[row][x] = {"t": glyph, "fg": _colour(low), "bold": low > 0.5}
            elif low >= 0.07 and high >= 0.07:
                grid[row][x] = {"t": "▀", "fg": _colour(high), "bg": _colour(low)}
            elif low >= 0.07:
                grid[row][x] = {"t": "▄", "fg": _colour(low)}
            elif high >= 0.07:
                grid[row][x] = {"t": "▀", "fg": _colour(high)}
    if look.logs and w <= 5 and floor >= 1:
        # The wick, where the flame stands.
        under = grid[floor - 1][w // 2]
        if under["t"] == " ":
            grid[floor - 1][w // 2] = {"t": "╷", "fg": WICK}
    if look.glow:
        _light(grid, t, w, h, look, lit / max(1, w * rows_of_fire * 2), rows_of_fire)
    if look.sparks:
        for (x, y), cell in sparks(t, w, floor, f, seed, poke).items():
            under = grid[y][x]
            if under["t"] in " ░┬─┴":  # not over the flames
                grid[y][x] = {**cell, "bg": 52} if "bg" in under else cell
    return [_runs(row) for row in grid]


def _light(
    grid: list[list[Cell]], t: float, w: int, h: int, look: Look, lit: float, bed: int
) -> None:
    """The light of the fire on the free cells: a halo that breathes
    with the flames; behind logs, bricks."""
    flicker = 0.75 + 0.5 * _noise(look.seed + 11, 0.0, t * 6.0)
    power = min(1.0, (0.25 + 2.2 * lit) * flicker)
    cx, cy = (w - 1) / 2, bed - 1
    wall = look.logs and w > 12
    for y in range(bed if not wall else h):
        for x in range(w):
            cell = grid[y][x]
            if cell["t"] != " ":
                continue
            near = math.hypot((x - cx) / (w / 2), (y - cy) / max(1.0, bed) * 0.9)
            light = power * max(0.0, 1.15 - near)
            if wall:
                if y >= bed:
                    continue
                level = min(len(LIT) - 1, int((1 - min(1.0, light * 1.3)) * len(LIT)))
                grid[y][x] = {"t": _bricks(x, y), "fg": LIT[level]}
            elif light > 0.62:
                grid[y][x] = {"t": " ", "bg": 52}
            elif light > 0.42:
                grid[y][x] = {"t": "░", "fg": 52}


def _runs(cells: Sequence[Cell]) -> list[Run]:
    """Cells to runs: neighbours of one style joined."""
    out: list[Run] = []
    for cell in cells:
        if out and {**out[-1], "t": ""} == {**cell, "t": ""}:
            out[-1]["t"] = str(out[-1]["t"]) + str(cell["t"])
        else:
            out.append(dict(cell))
    return out


class Poke:
    """The hover: a poke when the pointer comes over the fire, and one
    more each `again` seconds it keeps moving there.

        poke = Poke()
        poke.on_move(x, y)
        scene(t, w, h, look, poke.age())
    """

    def __init__(self, again: float = POKED) -> None:
        self.again = again
        self.at: float | None = None
        self.over = False

    def on_move(self, x: int, y: int, now: float | None = None) -> None:
        now = clock.monotonic() if now is None else now
        if not self.over or self.at is None or now - self.at >= self.again:
            self.at = now
        self.over = True

    def on_leave(self) -> None:
        self.over = False

    def age(self, now: float | None = None) -> float | None:
        """Seconds since the last poke; None when it is spent."""
        if self.at is None:
            return None
        age = (clock.monotonic() if now is None else now) - self.at
        return age if age < POKED else None


def ansi(run: Run) -> str:
    """`run` with SGR escapes."""
    codes = [f"{n};5;{run[k]}" for k, n in (("fg", 38), ("bg", 48)) if k in run]
    codes += [c for k, c in (("bold", "1"), ("dim", "2")) if run.get(k)]
    text = str(run["t"])
    return f"\x1b[{';'.join(codes)}m{text}\x1b[0m" if codes else text


def boxed(rows: Rows, w: int, title: str, colour: bool) -> list[str]:
    """`rows` in a plain box of inner width `w`."""
    paint = ansi if colour else (lambda run: str(run["t"]))
    title = f" {title} " if w >= len(title) + 4 else ""
    out = ["┌" + "─" * w + "┐"]
    for row in rows:
        width = sum(len(str(run["t"])) for run in row)
        out.append("│" + "".join(paint(run) for run in row) + " " * (w - width) + "│")
    out.append("└" + "─" * (w - len(title)) + title + "┘")
    return out


def _look(args: argparse.Namespace) -> Look:
    return Look(
        blocks=args.blocks,
        logs=args.logs,
        cycle=args.cycle,
        sparks=args.sparks,
        glow=args.glow,
        seed=1 if args.seed is None else args.seed,
        burn=args.burn,
    )


def _start(look: Look) -> float:
    """The time a fire starts at: a log well alight."""
    return 0.03 * look.burn if look.cycle else 0.0


def show(args: argparse.Namespace) -> int:
    """`--print`: the variants at `--at`, or one size; no driver."""
    look = _look(args)
    t = _start(look) + args.at
    colour = sys.stdout.isatty()
    for w, h in SIZES if args.print == (0, 0) else (args.print,):
        print("\n".join(boxed(scene(t, w, h, look), w, f"{w}x{h}", colour)))
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    look = _look(args)
    start = clock.monotonic()
    poke = Poke()

    def render(w: int, h: int) -> Rows:
        now = clock.monotonic()
        t = _start(look) + (now - start) * args.speed
        return scene(t, w, h, look, poke.age(now))

    deck = _widgets.Deck(pilot, args)
    widget = deck.add("fire", list(SIZES), render)
    # The next draw shows it: no draw of its own.
    deck.on_hover(
        widget,
        lambda x, y: poke.on_move(x, y) or False,
        lambda: poke.on_leave() or False,
    )
    deck.run(period=PERIOD, seconds=seconds)


def _size(text: str) -> Size:
    try:
        w, h = (int(n) for n in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"not WxH: {text}") from None
    if w < 1 or h < 1:
        raise argparse.ArgumentTypeError(f"not WxH: {text}")
    return (w, h)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("fire", "A fire in a quiet room of the screen.")
    _widgets.options(out, border="round")
    out.set_defaults(border=None)  # `hearth` with `-l`, else `round`
    out.add_argument("-b", "--blocks", action="store_true", help="half blocks")
    out.add_argument(
        "-l", "--logs", action="store_true", help="a candle, a brazier, logs, a mantel"
    )
    out.add_argument(
        "-c", "--cycle", action="store_true", help="it burns down, a log is put on"
    )
    out.add_argument("-k", "--sparks", action="store_true", help="sparks rise")
    out.add_argument("-g", "--glow", action="store_true", help="the light of the fire")
    out.add_argument("-a", "--all", action="store_true", help="-b -l -c -k -g")
    out.add_argument(
        "--burn",
        type=float,
        default=BURN,
        metavar="SECONDS",
        help=f"with -c: from a log to the next [default: {BURN:.0f}]",
    )
    out.add_argument(
        "--speed", type=float, default=1.0, metavar="N", help="N seconds a second"
    )
    out.add_argument("--seed", type=int, help="same seed, same fire")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(0, 0),
        metavar="WxH",
        help="no driver: print every variant, or the size given",
    )
    out.add_argument(
        "--at",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="with --print: that long after the start",
    )
    args = _cli.parse(out, argv)
    if args.burn <= 0:
        out.error("--burn: more than 0")
    if args.all:
        args.blocks = args.logs = args.cycle = args.sparks = args.glow = True
    if args.border is None:
        args.border = "hearth" if args.logs else "round"
    args.border = BORDERS.get(args.border, args.border)
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
