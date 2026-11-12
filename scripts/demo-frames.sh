#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Dynamic frames over a tmux session: every pane is found and gets its
# own drawing. Window 0: vim (fishes), a noisy pane (a clock), an idle
# shell (sand, once it has been still for 10 s). Window 1: two vim
# splits. Window 2, when ranger is installed: ranger beside a shell; its
# columns are panes inside the tmux pane, and a mark fixed on a row of
# the list follows it (page down there with ^F: the mark stays on
# entry-061). Switch windows with ^B n: the frames of a window are
# found again, with their drawings, when it comes back.
#
#   scripts/demo-frames.sh            watch; ^B n switches, ^B d ends
#   scripts/demo-frames.sh --check    self-test, detached
set -u
. "$(dirname "${BASH_SOURCE[0]}")/lib-demo.sh"
demo_setup

text=temp/demo-frames.txt
seq 1 300 | sed 's/.*/line &: the quick brown fox jumps over the lazy dog/' >"$text"
list=$PWD/temp/demo-frames-dir
mkdir -p "$list" && (cd "$list" && seq -f 'entry-%03g' 1 120 | xargs touch)
inner=ptyrc-demo-$$                    # its own tmux server, never the default
vim="/usr/bin/vim -u NONE -N -n"      # -n: one file in two windows, no swap prompt
noise='while :; do od -An -N12 -tx1 /dev/urandom; sleep 0.3; done'
e2e_start demo-frames 120x36 tmux -L "$inner" -f /dev/null \
    new-session "$vim $text" \; \
    split-window -h "sh -c '$noise'" \; \
    split-window -v "env PS1='idle\$ ' sh" \; \
    new-window "$vim -O $text scripts/demo-frames.sh" \; \
    select-window -t 0 \; select-pane -t 0 || exit 1
ranger=
if command -v ranger >/dev/null 2>&1; then
    ranger=1
    tmux -L "$inner" new-window "ranger --clean $list" \; \
        split-window -h -l 40 "env PS1='beside\$ ' sh" \; \
        select-pane -t 0 \; select-window -t 0
fi
trap 'demo_stop_apps; tmux -L "$inner" kill-server 2>/dev/null; e2e_cleanup' EXIT

# Panes of the layout on screen, as "id" lines, chosen by an expression
# over one frame info `f`.
panes() { demo_json frames "' '.join(str(f['id']) for f in d['frames'] if f['kind'] == 'pane' and f['usable'] and ($1))"; }
three() { [ "$(panes True | wc -w)" -eq 3 ]; }
demo_until 10 three || { echo "FAIL three panes found"; exit 1; }
in_vim=$(panes "f['support'] == 'vim'")
noisy=$(panes "f['rect'][0] > 0 and f['rect'][1] == 0")
quiet=$(panes "f['rect'][0] > 0 and f['rect'][1] > 0")

demo_app fishes -f "$in_vim" --count 5
demo_app clock -f "$noisy"
demo_app sand -f idle

# The mark: fixed on frame row 60 of ranger's list, out of sight until
# the list is scrolled there.
main=
if [ -n "$ranger" ]; then
    tmux -L "$inner" select-window -t 2
    listed() { [ -n "$(panes "(f['key'] or '').startswith('tmux:0:2/ranger:')")" ]; }
    demo_until 10 listed || { echo "FAIL ranger's list found"; exit 1; }
    main=$(panes "(f['key'] or '').startswith('tmux:0:2/ranger:')")
    e2e_p draw text -f "$main" --at 11,60 --space frame '<= mark' >/dev/null
    [ "${1:-}" = --check ] && tmux -L "$inner" select-window -t 0
fi

if [ "${1:-}" != --check ]; then
    demo_show
    exit 0
fi

# Frames that hold a sprite in sight.
drawn() { demo_json draw ls "' '.join(map(str, sorted({s['frame'] for s in d['sprites'] if s['screen']})))"; }
demo_ok "three panes: vim $in_vim, noisy $noisy, quiet $quiet" test -n "$in_vim" -a -n "$noisy" -a -n "$quiet"
demo_ok "fishes in vim, a clock on the noisy pane" demo_until 8 eval '[ "$(drawn)" = "$(echo $in_vim $noisy | tr " " "\n" | sort -n | xargs)" ]'
demo_ok "the quiet pane becomes idle, the noisy one does not" demo_until 20 eval \
    '[ "$(demo_json frames "[f[\"is_idle\"] for f in d[\"frames\"] if f[\"id\"] in ($quiet, $noisy)]")" = "[False, True]" ]'
demo_ok "sand falls in the idle pane" demo_until 8 eval 'drawn | grep -qw "$quiet"'
tmux -L "$inner" select-window -t 1
demo_ok "window 1: two vim splits, other panes" demo_until 8 eval '[ "$(panes "f[\"support\"] == \"vim\" and (f[\"key\"] or \"\").startswith(\"tmux:0:1/vim:\")" | wc -w)" -eq 2 ] && ! panes True | grep -qw -e "$in_vim" -e "$noisy" -e "$quiet"'
demo_ok "drawings of window 0 are out of sight" test -z "$(drawn)"
tmux -L "$inner" select-window -t 0
demo_ok "window 0 is back with the same frames" demo_until 8 eval '[ "$(panes True | xargs -n1 | sort -n | xargs)" = "$(echo $in_vim $noisy $quiet | xargs -n1 | sort -n | xargs)" ]'
demo_ok "and their drawings" demo_until 8 eval 'drawn | grep -qw "$in_vim" && drawn | grep -qw "$noisy"'
if [ -n "$main" ]; then
    tmux -L "$inner" select-window -t 2
    demo_ok "window 2: ranger's columns are panes inside the tmux pane, beside the shell" demo_until 8 eval \
        '[ "$(panes "f[\"support\"] == \"ranger\"" | wc -w)" -ge 2 ] && panes True | grep -qw "$main" && [ "$(panes "f[\"support\"] != \"ranger\"" | wc -w)" -eq 1 ]'
    demo_ok "the mark is out of sight at the top of the list" test -z "$(e2e_host | grep -F "<= mark")"
    demo_ok "the list paged down: the mark is on entry-061" demo_until 12 eval \
        'tmux -L "$inner" send-keys -t 2.0 C-f; sleep 0.4; e2e_host | grep -q "entry-061.*<= mark"'
    tmux -L "$inner" send-keys -t 2.0 k k k
    demo_ok "three entries up: it stays on it" demo_until 8 eval 'sleep 0.5; e2e_host | grep -q "entry-061.*<= mark"'
    e2e_p draw clear >/dev/null
    tmux -L "$inner" select-window -t 0
fi

demo_stop_apps
demo_ok "the apps leave nothing behind" demo_until 5 test "$(sleep 1; demo_json draw ls "len(d['sprites'])")" -eq 0
tmux -L "$inner" kill-server
demo_ok "the session ends with tmux" demo_until 5 eval '! e2e_p cursor'
exit $DEMO_FAILED
