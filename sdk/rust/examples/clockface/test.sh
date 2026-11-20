#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The unit tests (cargo); then the widget in a wide screen at 0, 2 s, 3 s, and in
# a narrow one: the driver picks the line. Against expected.txt.
# After a change meant to change it: ./test.sh --update
set -eu
cd "$(dirname "$0")"
cargo test -q >&2
run() {
  "${PTYRC:-ptyrc}" dev test . --size 40x8 --for 3s --at 0,2s,3s
  "${PTYRC:-ptyrc}" dev test . --size 20x6 --for 1s --at 1s
}
if [ "${1:-}" = --update ]; then
  run > expected.txt
else
  run > actual.txt
  diff -u expected.txt actual.txt
  rm actual.txt
fi
