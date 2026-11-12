#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The client apps on a scrolling vim buffer: a flower next to its word,
# fishes behind the text, a clock, a cat, marks on two words, rain, a
# confetti party.
#
#   scripts/demo-apps.sh              watch; scroll with ^E ^Y ^D ^U, :q ends
#   scripts/demo-apps.sh snake        the snake instead of the fishes
#   scripts/demo-apps.sh brick        the brick breaker, all of it
#   scripts/demo-apps.sh blocks       the falling pieces, all of it
#   scripts/demo-apps.sh --check      self-test, detached
set -u
. "$(dirname "${BASH_SOURCE[0]}")/lib-demo.sh"
demo_setup

text=temp/demo-apps.txt
for i in $(seq 1 400); do
    case $((i % 40)) in
    12) echo "    here grows a flower" ;;
    8 | 9 | 10 | 11 | 13 | 14 | 15 | 16) echo ;; # room for it: cold cells
    *) [ $((i % 4)) -eq 0 ] && echo || echo "line $i: the quick brown fox jumps over the lazy dog" ;;
    esac
done >"$text"

e2e_start demo-apps 110x34 /usr/bin/vim -u NONE -N "$text" || exit 1
e2e_wait 'line 1:' >/dev/null

demo_app flower --word flower
demo_app clock
demo_app cat
demo_app marks 'lazy dog' fox
demo_app rain
if [ "${1:-}" = snake ]; then
    demo_app snake
elif [ "${1:-}" = brick ]; then
    demo_app brick -a
elif [ "${1:-}" = blocks ]; then
    demo_app blocks -a
else
    demo_app fishes --count 7
    [ "${1:-}" = --check ] || demo_app confetti --party --sparkle
fi

if [ "${1:-}" != --check ]; then
    demo_show
    exit 0
fi

shown() { demo_json draw ls "sum(1 for s in d['sprites'] if s['visible'])"; }
# The flower's bold word: a tint in frame coordinates.
flower_row() { demo_json draw ls "[s['screen'][1] for s in d['sprites'] if s['blend'] == 'tint' and s['size'][0] == 6][0]"; }
# The marks: tints as wide as their words, eight cells and three.
marks_shown() { demo_json draw ls "sorted({s['size'][0] for s in d['sprites'] if s['blend'] == 'tint' and s['visible']} - {6})"; }
cat_shown() { demo_json draw ls "sum(1 for s in d['sprites'] if s['size'] == [11, 3] and s['layer'] == -1 and s['visible'])"; }
rain_shown() { demo_json draw ls "sum(1 for s in d['sprites'] if s['kind'] == 'effect:rain' and s['visible'])"; }
flower_shown() { demo_json draw ls "sum(1 for s in d['sprites'] if s['kind'] == 'animation' and s['layer'] == -1 and s['visible'])"; }
# Screen row minus frame row of every fish in sight: the scroll, negated.
fish_shift() { demo_json draw ls "sorted({s['screen'][1] - s['anchor']['at'][1] for s in d['sprites'] if s['anchor'].get('space') == 'frame' and s['visible']})"; }

demo_ok "flower, clock and fishes are up" demo_until 8 test "$(sleep 3; shown)" -ge 6
demo_ok "a fish shows on the host" demo_until 5 eval 'e2e_host | grep -qF -e "<*//><" -e "><\\\\*>"'
demo_ok "the flower shows, swaying" demo_until 10 eval 'test "$(flower_shown)" -ge 1'
demo_ok "the cat is on the text" demo_until 10 eval 'test "$(cat_shown)" -eq 1'
demo_ok "the cat shows on the host" demo_until 10 eval 'e2e_host | grep -qF "/\\_/\\"'
demo_ok "both patterns are marked" demo_until 5 eval 'test "$(marks_shown)" = "[3, 8]"'
demo_ok "the counts are displayed" demo_until 5 eval 'e2e_host | grep -q " fox  *[0-9]"'
demo_ok "it rains" demo_until 5 eval 'test "$(rain_shown)" -eq 1 && e2e_host | grep -q "[│╎]"'
before=$(flower_row)
e2e_keys ctrl+e ctrl+e ctrl+e
sleep 0.5
demo_ok "the flower follows its word when the text scrolls" test "$(flower_row)" -eq $((before - 3))
demo_ok "fishes hold frame rows: they scrolled with the text" test "$(fish_shift)" = "[-3]"
demo_ok "the marks are on the new lines too" demo_until 5 eval 'test "$(demo_json draw ls "sum(1 for s in d[\"sprites\"] if s[\"blend\"] == \"tint\" and s[\"visible\"] and s[\"screen\"][1] >= 30)")" -ge 2'
demo_stop_apps
demo_ok "the apps leave nothing behind" demo_until 5 test "$(sleep 1; demo_json draw ls "len(d['sprites'])")" -eq 0
demo_ok "the host shows the app's screen" test "$(e2e_host | sed 's/ *$//')" = "$(e2e_p text | sed 's/ *$//')"
e2e_keys esc
e2e_send ':q!'
e2e_keys enter
demo_ok "the session ends with vim" demo_until 5 eval '! e2e_p cursor'
exit $DEMO_FAILED
