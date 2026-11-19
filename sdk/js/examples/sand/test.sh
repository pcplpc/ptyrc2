#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The screen at 1 s, 3 s and 6 s, against expected.txt. The test host
# has no draw extension: what shows is the app's own pile (see app.js).
# After a change meant to change it: ./test.sh --update
set -eu
cd "$(dirname "$0")"
run() { "${PTYRC:-ptyrc}" dev test . --size 40x8 --for 6s --at 1s,3s,6s --arg rate=120; }
if [ "${1:-}" = --update ]; then
  run > expected.txt
else
  run > actual.txt
  diff -u expected.txt actual.txt
  rm actual.txt
fi
