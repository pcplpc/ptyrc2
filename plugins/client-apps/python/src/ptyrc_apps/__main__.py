# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app <name> [options]`: run a client app."""

from __future__ import annotations

import importlib
import sys
from collections.abc import Sequence

from . import APPS

__all__ = ["main"]


def usage() -> str:
    width = max(map(len, APPS))
    lines = ["usage: ptyrc-app <name> [options]", "", "apps:"]
    lines += [f"  {name:{width}}  {doc}" for name, doc in sorted(APPS.items())]
    lines += ["", "`ptyrc-app <name> --help` lists the options of an app."]
    lines += ["`ptyrc-app demo SEED NTH` runs one of them: see `ptyrc_apps.demo`."]
    return "\n".join(lines)


def main(argv: Sequence[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv or argv[0] in ("-h", "--help"):
        print(usage())
        return 0 if argv else 2
    name, rest = argv[0], argv[1:]
    if name == "demo":
        return importlib.import_module("ptyrc_apps.demo").main(rest)
    if name not in APPS:
        print(f"ptyrc-app: no app named {name}\n\n{usage()}", file=sys.stderr)
        return 2
    return importlib.import_module(f"ptyrc_apps.{name}").main(rest)


if __name__ == "__main__":
    raise SystemExit(main())
