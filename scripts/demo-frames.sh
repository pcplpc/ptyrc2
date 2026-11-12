#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Dynamic frames over a tmux session: every pane is found and gets its
# own drawing. Window 0: vim (fishes), a noisy pane (a clock), an idle
# shell (sand, once it has been still for 10 s). Window 1: two vim
# splits. Switch windows with ^B n: the frames of a window are found
# again, with their drawings, when it comes back.
#
#   scripts/demo-frames.sh            watch; ^B n switches, ^B d ends
#   scripts/demo-frames.sh --check    self-test, detached
set -u
. "$(dirname "${BASH_SOURCE[0]}")/lib-demo.sh"
demo_setup

text=temp/demo-frames.txt
seq 1 300 | sed 's/.*/line &: the quick brown fox jumps over the lazy dog/' >"$text"
inner=ptyrc-demo-$$                    # its own tmux server, never the default
vim="/usr/bin/vim -u NONE -N"
noise='while :; do od -An -N12 -tx1 /dev/urandom; sleep 0.3; done'
e2e_start demo-frames 120x36 tmux -L "$inner" -f /dev/null \
    new-session "$vim $text" \; \
    split-window -h "sh -c '$noise'" \; \
    split-window -v "env PS1='idle\$ ' sh" \; \
    new-window "$vim -O $text scripts/demo-frames.sh" \; \
    select-window -t 0 \; select-pane -t 0 || exit 1
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
demo_ok "window 1: other panes" demo_until 8 eval '[ -n "$(panes True)" ] && ! panes True | grep -qw -e "$in_vim" -e "$noisy" -e "$quiet"'
demo_ok "drawings of window 0 are out of sight" test -z "$(drawn)"
tmux -L "$inner" select-window -t 0
demo_ok "window 0 is back with the same frames" demo_until 8 eval '[ "$(panes True | xargs -n1 | sort -n | xargs)" = "$(echo $in_vim $noisy $quiet | xargs -n1 | sort -n | xargs)" ]'
demo_ok "and their drawings" demo_until 8 eval 'drawn | grep -qw "$in_vim" && drawn | grep -qw "$noisy"'
demo_stop_apps
demo_ok "the apps leave nothing behind" demo_until 5 test "$(sleep 1; demo_json draw ls "len(d['sprites'])")" -eq 0
tmux -L "$inner" kill-server
demo_ok "the session ends with tmux" demo_until 5 eval '! e2e_p cursor'
exit $DEMO_FAILED
