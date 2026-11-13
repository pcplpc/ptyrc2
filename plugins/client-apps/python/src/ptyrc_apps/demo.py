# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app demo SEED NTH`: the app the demo of `ptyrc run` shows as
its `NTH`, counted from 0. The driver runs it on ctrl+t typed twice.

The apps come round by round, each round all of them once, shuffled;
none comes twice in a row. An app with a full version (`-a`) is shown
with it one time in two. `SEED` gives the order: one seed, one order.
"""

from __future__ import annotations

import importlib
import random
import sys
from collections.abc import Sequence

from . import APPS

__all__ = ["VARIANTS", "main", "pick"]

#: Arguments an app is shown with, a tuple per version; none: not shown.
#: An app not named here is shown bare.
VARIANTS: dict[str, tuple[tuple[str, ...], ...]] = {
    "bricks": ((), ("-a",)),
    "confetti": ((), ("-a",)),
    "fishes": ((), ("-a",)),
    "flower": (("-a",),),  # bare, it wants its word on the screen
    "marks": (),  # it wants patterns
    "sand": (("--now",),),
    "snake": ((), ("-a",)),
}


def order(seed: int, round_: int) -> list[str]:
    """The apps of a round, shuffled; its first is not the last of the
    round before."""
    names = sorted(name for name in APPS if VARIANTS.get(name, ((),)))
    random.Random(f"{seed}/{round_}").shuffle(names)
    if round_ > 0 and len(names) > 1 and names[0] == order(seed, round_ - 1)[-1]:
        names[0], names[1] = names[1], names[0]
    return names


def pick(seed: int, nth: int) -> list[str]:
    """The `nth` app of the demo and its arguments."""
    names = order(seed, nth // len(order(seed, 0)))
    name = names[nth % len(names)]
    variants = VARIANTS.get(name, ((),))
    return [name, *random.Random(f"{seed}:{nth}").choice(variants)]


def main(argv: Sequence[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    try:
        seed, nth = map(int, argv)
        if nth < 0:
            raise ValueError
    except ValueError:
        print("usage: ptyrc-app demo SEED NTH", file=sys.stderr)
        return 2
    name, *rest = pick(seed, nth)
    return importlib.import_module(f"ptyrc_apps.{name}").main(rest)


if __name__ == "__main__":
    raise SystemExit(main())
