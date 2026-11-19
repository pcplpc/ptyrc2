#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Over screen.txt: the screen at 0, 5 s and 10 s; then steered by the
# arrows, ctrl+t ending it; then the grab denied: it plays alone.
# Against expected.txt.
# After a change meant to change it: ./test.sh --update
set -eu
cd "$(dirname "$0")"
run() {
  "${PTYRC:-ptyrc}" dev test . --size 40x10 --screen screen.txt --for 10s --at 0,5s,10s
  "${PTYRC:-ptyrc}" dev test . --size 40x10 --for 1s --grab \
    --key 300:up --key 600:left --key 900:ctrl+t
  "${PTYRC:-ptyrc}" dev test . --size 40x10 --for 1s --grab --deny keys-arrows
}
if [ "${1:-}" = --update ]; then
  run > expected.txt
else
  run > actual.txt
  diff -u expected.txt actual.txt
  rm actual.txt
fi
