#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# At 0 and 1 s; then storage and the clock denied. Against expected.txt,
# the same as the script twin's.
# After a change meant to change it: ./test.sh --update
set -eu
cd "$(dirname "$0")"
run() {
  "${PTYRC:-ptyrc}" dev test . --size 50x2 --for 1s --at 0,1s
  "${PTYRC:-ptyrc}" dev test . --size 50x2 --for 1s --at 1s --deny storage,clock
}
if [ "${1:-}" = --update ]; then
  run > expected.txt
else
  run > actual.txt
  diff -u expected.txt actual.txt
  rm actual.txt
fi
