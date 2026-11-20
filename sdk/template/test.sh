#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Each case tests/CASE.args: `ptyrc dev test .` and its words, the
# report against tests/CASE.txt (a difference: tests/CASE.got). Then
# the unit tests: `dev test --unit` (tests.js), `cargo test` (Cargo.toml).
# Exits 1 when one fails. After a change meant to change a report:
#   ./test.sh --update    (or PTYRC_BLESS=1 ./test.sh)
# CASE.args: words split on white space, `#` to the end of a line a
# comment, no quoting. An option of the app named as one of dev test's
# goes after `--`.
set -eu
cd "$(dirname "$0")"
ptyrc=${PTYRC:-ptyrc}
bless=${PTYRC_BLESS:-}
[ "${1:-}" = --update ] && bless=1
failed=0
for args in tests/*.args; do
  [ -e "$args" ] || { echo "no case: tests/CASE.args" >&2; exit 1; }
  case=${args%.args}
  status=0
  set -f # the words are not globbed
  "$ptyrc" dev test . $(sed 's/#.*//' "$args") > "$case.got" || status=$?
  set +f
  if [ "$status" -ne 0 ]; then
    echo "FAIL $case: ptyrc dev test exited $status" >&2
    failed=1
  elif [ -n "$bless" ]; then
    mv "$case.got" "$case.txt"
    echo "wrote $case.txt"
  elif [ ! -f "$case.txt" ]; then
    echo "FAIL $case: no $case.txt (./test.sh --update writes it)" >&2
    failed=1
  elif diff -u "$case.txt" "$case.got"; then
    rm "$case.got"
    echo "ok   $case"
  else
    echo "FAIL $case: the report differs from $case.txt (./test.sh --update if meant)" >&2
    failed=1
  fi
done
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
