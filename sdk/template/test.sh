#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The app headless: the screen at 0, 0.5 s and 1 s, against expected.txt.
# After a change meant to change it: ./test.sh --update
set -eu
cd "$(dirname "$0")"
run() { "${PTYRC:-ptyrc}" dev test . --size 40x4 --for 1s --at 0,500,1000; }
if [ "${1:-}" = --update ]; then
  run > expected.txt
else
  run > actual.txt
  diff -u expected.txt actual.txt
  rm actual.txt
fi
