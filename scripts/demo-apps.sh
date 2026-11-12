#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The client apps on a scrolling vim buffer: a flower next to its word,
# fishes behind the text, a clock, a confetti party.
#
#   scripts/demo-apps.sh              watch; scroll with ^E ^Y ^D ^U, :q ends
#   scripts/demo-apps.sh snake        the snake instead of the fishes
#   scripts/demo-apps.sh --check      self-test, detached
set -u
. "$(dirname "${BASH_SOURCE[0]}")/lib-demo.sh"
demo_setup

text=temp/demo-apps.txt
for i in $(seq 1 400); do
    case $((i % 40)) in
    12) echo "    here grows a flower" ;;
    *) [ $((i % 4)) -eq 0 ] && echo || echo "line $i: the quick brown fox jumps over the lazy dog" ;;
    esac
done >"$text"

e2e_start demo-apps 110x34 /usr/bin/vim -u NONE -N "$text" || exit 1
e2e_wait 'line 1:' >/dev/null

demo_app flower --word flower
demo_app clock
if [ "${1:-}" = snake ]; then
    demo_app snake
else
    demo_app fishes --count 7
    [ "${1:-}" = --check ] || demo_app confetti --spawn --rate 10 --sparkle
fi

if [ "${1:-}" != --check ]; then
    demo_show
    exit 0
fi

shown() { demo_json draw ls "sum(1 for s in d['sprites'] if s['screen'])"; }
flower_row() { demo_json draw ls "[s['screen'][1] for s in d['sprites'] if 'text' in s['anchor']][0]"; }
# Screen row minus frame row of every fish in sight: the scroll, negated.
fish_shift() { demo_json draw ls "sorted({s['screen'][1] - s['anchor']['at'][1] for s in d['sprites'] if s['anchor'].get('space') == 'frame' and s['screen']})"; }

demo_ok "flower, clock and fishes are up" demo_until 8 test "$(sleep 3; shown)" -ge 6
demo_ok "a fish shows on the host" demo_until 5 eval 'e2e_host | grep -qF -e "<*//><" -e "><\\\\*>"'
demo_ok "the flower shows on the host" eval 'e2e_host | grep -qF "( / \\ )"'
before=$(flower_row)
e2e_keys ctrl+e ctrl+e ctrl+e
sleep 0.5
demo_ok "the flower follows its word when the text scrolls" test "$(flower_row)" -eq $((before - 3))
demo_ok "fishes hold frame rows: they scrolled with the text" test "$(fish_shift)" = "[-3]"
demo_stop_apps
demo_ok "the apps leave nothing behind" demo_until 5 test "$(sleep 1; demo_json draw ls "len(d['sprites'])")" -eq 0
demo_ok "the host shows the app's screen" test "$(e2e_host | sed 's/ *$//')" = "$(e2e_p text | sed 's/ *$//')"
e2e_keys esc
e2e_send ':q!'
e2e_keys enter
demo_ok "the session ends with vim" demo_until 5 eval '! e2e_p cursor'
exit $DEMO_FAILED
