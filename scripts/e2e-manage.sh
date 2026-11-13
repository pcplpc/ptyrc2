#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The menu of `ptyrc manage` in a real tmux under `ptyrc tmux attach`:
# ctrl+t twice on the terminal, an app started, the frames, a move, a
# pause, the way out. Two private servers, as e2e-tmux-attach.sh: `host`
# stands for the terminal. Prints PASS or FAIL per check; exit status 1
# if a check failed.
#
#   scripts/e2e-manage.sh [--show]     --show: the screens, as tmux has them

cd "$(dirname "$0")/.." || exit 2
ROOT=$PWD
PTYRC=${PTYRC_BIN:-$ROOT/target/debug/ptyrc}
WORK=$ROOT/temp/e2e-manage
SHOW=0; [ "${1:-}" = --show ] && SHOW=1
FAILED=0

rm -rf "$WORK"; mkdir -p "$WORK/run"
export XDG_RUNTIME_DIR=$WORK/run PATH=$ROOT/.venv/bin:$PATH
unset TMUX TMUX_PANE PTYRC_SESSION PTYRC_SOCKET PTYRC_HOTKEY
command -v ptyrc-app >/dev/null || { echo "no ptyrc-app: uv sync"; exit 2; }
HOST=(tmux -L "ptyrc-e2e-mhost-$$" -f /dev/null)
IN=(tmux -L "ptyrc-e2e-min-$$" -f /dev/null)

cleanup() {
    "${HOST[@]}" kill-server 2>/dev/null
    "${IN[@]}" kill-server 2>/dev/null
}
trap cleanup EXIT

pass() { echo "PASS  $1"; }
fail() { echo "FAIL  $1"; FAILED=1; [ $SHOW = 1 ] || shown | sed 's/^/    /' >&2; }
shown() { "${HOST[@]}" capture-pane -p -t host; }
keys() { "${HOST[@]}" send-keys -t host "$@"; }
# sees NAME TEXT: the terminal shows TEXT within 5 s.
sees() {
    local i
    for i in $(seq 100); do shown | grep -qF -- "$2" && { pass "$1"; return 0; }; sleep 0.05; done
    fail "$1"
}
# gone NAME TEXT: the terminal no longer shows TEXT within 5 s.
gone() {
    local i
    for i in $(seq 100); do shown | grep -qF -- "$2" || { pass "$1"; return 0; }; sleep 0.05; done
    fail "$1"
}
# has NAME 'SHELL': evaluated until it succeeds, 5 s at most.
has() {
    local i
    for i in $(seq 100); do eval "$2" >/dev/null 2>&1 && { pass "$1"; return 0; }; sleep 0.05; done
    fail "$1"
}
picture() { [ $SHOW = 1 ] && { echo "--- $1"; shown; }; return 0; }
m() { "$PTYRC" -s tmux-main "$@"; }

"${IN[@]}" new-session -d -s main -n w -x 110 -y 32 "env PS1='in\$ ' bash --norc --noprofile"
"${IN[@]}" split-window -h -t main "env PS1='in\$ ' bash --norc --noprofile"
"${IN[@]}" send-keys -t main 'seq 1 12' Enter
"${HOST[@]}" new-session -d -s host -x 110 -y 32 "env PS1='host\$ ' bash --norc --noprofile"
keys "${IN[*]} attach -t main" Enter
has "a client" '[ "$("${IN[@]}" list-clients -t main | wc -l)" = 1 ]'
SOCKET=$("${IN[@]}" display-message -p -t main '#{socket_path}')
PANE=$("${IN[@]}" display-message -p -t main '#{pane_id}')
TMUX="$SOCKET,0,0" TMUX_PANE=$PANE "$PTYRC" tmux attach >/dev/null || fail "tmux attach"
has "the driver" 'm call session.info'
# The panes settle: frames are usable 1.5 s after.
sleep 2

keys C-t C-t
sees "ctrl+t twice: the menu" "apps of tmux-main"
sees "the foot" "esc leave"
has "the keyboard is the menu's" 'm call session.info | grep -q "\"grabbed\": true"'
picture "the menu"

keys s
sees "s: the apps to start" "clock     one clock in a quiet"
picture "start"
keys Down Enter
sees "the form" "start clock: options"
picture "the form"
keys Enter
has "enter: the clock runs" 'm manage ls | grep -q "running.*clock"'
sees "its row" "● clock"
has "its widget" 'm widgets | grep -q clock'

keys f
sees "f: the frames" "pane"
keys Down Down
picture "frames"
has "the frame selected is shown on the screen" 'm call draw.list | grep -q "\"tint\""'
keys Escape
gone "esc: back, the tint gone" "enter apps"

keys m
sees "m: where to" "move clock to"
sees "it says how" "moves live"
picture "move"
keys Down Down Enter
has "the clock is on that frame" 'm manage ls --json | grep -q "\"frames\":\[[1-9]"'

keys p
has "p: paused" 'm manage ls | grep -q "paused.*clock"'
sees "its mark" "‖ clock"
has "hidden with it" '! m text --sprites 2>/dev/null | grep -q "clock ╯"'
picture "paused"
keys p
has "p: running" 'm manage ls | grep -q "running.*clock"'

keys Escape
gone "esc: the menu is gone" "apps of tmux-main"
has "the keyboard is the app's" 'm call session.info | grep -q "\"grabbed\": false"'
has "the manager left the table" '! m manage ls | grep -q manage'
keys 'echo typed-in' Enter
sees "keys reach the pane again" "typed-in"
picture "after"

keys C-t C-t
sees "again" "● clock"
keys x
has "x: stopped" '! m manage ls | grep -q "running.*clock"'
keys C-t
gone "ctrl+t leaves" "apps of tmux-main"

[ $FAILED = 0 ] && echo "all passed"
exit $FAILED
