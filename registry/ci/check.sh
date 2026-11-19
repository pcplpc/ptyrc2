#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The check of a pull request, at the root of the registry's repository:
#   ci/check.sh BASE HEAD AUTHOR
# BASE: the branch merged into; HEAD: the commit proposed; AUTHOR: as in
# MAINTAINERS. PTYRC: the binary [default: ptyrc]. Status 1 if a problem.
set -eu
if [ $# -ne 3 ]; then
    echo "usage: ci/check.sh BASE HEAD AUTHOR" >&2
    exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
exec "${PTYRC:-ptyrc}" dev registry-check --repo "$root" \
    --maintainers "$root/MAINTAINERS" --author "$3" -- "$1" "$2"
