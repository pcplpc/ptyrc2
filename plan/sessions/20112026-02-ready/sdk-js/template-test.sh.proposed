#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The cases: `ptyrc dev test . --cases`, each tests/CASE.args against
# tests/CASE.txt (`ok`, or `FAIL` and a diff). Then the unit tests:
# `dev test --unit` (tests.js), `cargo test` (Cargo.toml). Exits 1 when
# one fails. After a change meant to change a report:
#   ./test.sh --update    (or PTYRC_BLESS=1 ./test.sh)
# CASE.args: words split on white space, `#` to the end of a line a
# comment, no quoting. An option of the app named as one of dev test's
# goes after `--`.
set -eu
cd "$(dirname "$0")"
ptyrc=${PTYRC:-ptyrc}
bless=
[ "${1:-}" = --update ] || [ -n "${PTYRC_BLESS:-}" ] && bless=--bless
failed=0
"$ptyrc" dev test . --cases $bless || failed=1
if [ -f tests.js ]; then
  "$ptyrc" dev test . --unit || failed=1
fi
if [ -f Cargo.toml ]; then
  if command -v cargo > /dev/null; then
    cargo test --quiet || failed=1
  else
    echo "cargo test SKIPPED: no cargo" >&2
  fi
fi
exit $failed
