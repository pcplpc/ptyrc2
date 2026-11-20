#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The cases (tests/CASE.args, each report against tests/CASE.txt), then
# the unit tests (cargo test). Exits 1 when one fails.
# After a change meant to change a report: ./test.sh --update
set -eu
cd "$(dirname "$0")"
ptyrc=${PTYRC:-ptyrc}
bless=
[ "${1:-}" = --update ] && bless=--bless
failed=0
"$ptyrc" dev test . --cases $bless || failed=1
cargo test -q >&2 || failed=1
exit $failed
