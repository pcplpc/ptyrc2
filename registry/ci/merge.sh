#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# After a merge, at the root of the registry's repository: index.json and
# site/index.html from the pointers. Publishing them is the host's.
# PTYRC: the binary [default: ptyrc].
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/site"
exec "${PTYRC:-ptyrc}" dev index "$root" --out "$root/index.json" \
    --html "$root/site/index.html"
