#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# `ptyrc tmux attach`, `detach` and `restart` against real tmux. Two private servers:
# `host` stands for the terminal, a client of `in` runs in its pane and is
# the one swapped. Prints PASS or FAIL per check; exit status 1 if a
# check failed.
#
#   scripts/e2e-tmux-attach.sh [SCENARIO...]     default: all
#   scenarios: swap apps leave typed fallback outside restart

cd "$(dirname "$0")/.." || exit 2
ROOT=$PWD
PTYRC=${PTYRC_BIN:-$ROOT/target/debug/ptyrc}
WORK=$ROOT/temp/e2e-tmux-attach
FAILED=0

rm -rf "$WORK"; mkdir -p "$WORK/run"
export XDG_RUNTIME_DIR=$WORK/run
# Apps started here are answered yes in a private permissions.toml: no
# pop-up waits for the keyboard. What an app does not declare stays unheld.
export XDG_CONFIG_HOME=$WORK/config
mkdir -p -m 700 "$XDG_CONFIG_HOME/ptyrc"
(umask 077; printf '[app."native/%s"]\nallow = ["layout", "shape", "words", "text", "front-widget", "front", "screen-wide", "keys-arrows", "keys-grab", "keys-read", "pointer", "clicks", "clicks-grab", "clock", "storage", "files", "files-write", "probe-git", "probe-system", "limit-memory", "limit-time"]\n' clock fishes >"$XDG_CONFIG_HOME/ptyrc/permissions.toml")
unset TMUX TMUX_PANE PTYRC_SESSION PTYRC_SOCKET
HOST=(false)
IN=(false)
RUNS=0

cleanup() {
    "${HOST[@]}" kill-server 2>/dev/null
    "${IN[@]}" kill-server 2>/dev/null
}
trap cleanup EXIT

pass() { echo "PASS  $1"; }
fail() { echo "FAIL  $1"; shift; [ $# -gt 0 ] && printf '    %s\n' "$@" >&2; FAILED=1; }
# check NAME CMD...: CMD succeeds.
check() { local name=$1; shift; if "$@"; then pass "$name"; else fail "$name"; fi; }
# eventually 'SHELL': evaluated until it succeeds, 5 s at most.
eventually() {
    local i
    for i in $(seq 100); do eval "$1" >/dev/null 2>&1 && return 0; sleep 0.05; done
    return 1
}
# until_is WANT CMD...: until CMD prints WANT.
until_is() {
    local want=$1 i
    shift
    for i in $(seq 100); do [ "$("$@")" = "$want" ] && return 0; sleep 0.05; done
    return 1
}

# The terminal, with a plain client of a session `main` of two panes.
# $1: words put before the client's command.
start() {
    cleanup
    RUNS=$((RUNS + 1))
    HOST=(tmux -L "ptyrc-e2e-host-$$-$RUNS" -f /dev/null)
    IN=(tmux -L "ptyrc-e2e-in-$$-$RUNS" -f /dev/null)
    "${IN[@]}" new-session -d -s main -n w -x 100 -y 30 "env PS1='in\$ ' bash --norc --noprofile"
    "${IN[@]}" split-window -h -t main "env PS1='in\$ ' bash --norc --noprofile"
    "${IN[@]}" send-keys -t main 'echo hello-pane' Enter
    "${HOST[@]}" new-session -d -s host -x 100 -y 30 "env PS1='host\$ ' bash --norc --noprofile"
    "${HOST[@]}" send-keys -t host "${1:-} ${IN[*]} attach -t main" Enter
    until_is 1 clients || { fail "start: a client"; return 1; }
    SOCKET=$("${IN[@]}" display-message -p -t main '#{socket_path}')
    PANE=$("${IN[@]}" display-message -p -t main '#{pane_id}')
    sleep 0.3
}

clients() { "${IN[@]}" list-clients -t main 2>/dev/null | wc -l; }
option() { "${IN[@]}" show-options -qv -t main "$1"; }
# What the terminal runs, and shows (without the clock of the status line).
running() { "${HOST[@]}" display-message -p -t host '#{pane_current_command}'; }
shown() { "${HOST[@]}" capture-pane -p -t host | sed '$s/"[^"]*" .*$//'; }
# A verb, as typed in a pane of `main`.
p() { TMUX="$SOCKET,0,0" TMUX_PANE=$PANE "$PTYRC" "$@"; }
is() { [ "$1" = "$2" ] || { echo "    got: $1, want: $2" >&2; return 1; }; }

s_swap() {
    start || return
    local before out
    before=$(shown)
    out=$(p tmux attach) || fail "swap: attach exits 0" "$out"
    check "swap: attach names the session" grep -q 'tmux-main' <<<"$out"
    check "swap: the session is named in @ptyrc" is "$(option @ptyrc)" tmux-main
    check "swap: the terminal runs the driver" is "$(running)" ptyrc
    check "swap: one client, the driver's" is "$(clients)" 1
    until_is "$before" shown
    check "swap: the terminal shows the same" is "$(shown)" "$before"
    check "swap: the screen is read" p -s tmux-main find hello-pane
    # A second session: the pane still finds its own.
    "$PTYRC" run --name other --headless -- sleep 30 &
    eventually '"$PTYRC" -s other cursor'
    check "swap: a pane finds its driver among several" p find hello-pane
    check "swap: so does a Python pilot" is "$(TMUX="$SOCKET,0,0" TMUX_PANE=$PANE \
        "$ROOT/.venv/bin/python" -c 'import ptyrc; print(ptyrc.Pilot().info()["session"])')" tmux-main
    check "swap: outside tmux, several is ambiguous" is "$("$PTYRC" cursor >/dev/null 2>&1; echo $?)" 3
    kill %1
    p draw text 3,3 PTYRC-WAS-HERE >/dev/null
    eventually 'shown | grep -q PTYRC-WAS-HERE'
    check "swap: a sprite is on the terminal" grep -q PTYRC-WAS-HERE <(shown)
    out=$(p tmux attach) || fail "swap: attach again exits 0" "$out"
    check "swap: attach again changes nothing" is "$(option @ptyrc) $(clients)" "tmux-main 1"

    p tmux detach || fail "swap: detach exits 0"
    check "swap: detach ends the driver" is "$("$PTYRC" ls 2>/dev/null | grep -c tmux-main)" 0
    until_is tmux running
    check "swap: the terminal runs tmux" is "$(running)" tmux
    until_is 1 clients
    check "swap: one plain client" is "$(clients)" 1
    check "swap: @ptyrc is unset" is "$(option @ptyrc)" ""
    until_is "$before" shown
    check "swap: the terminal shows the same again" is "$(shown)" "$before"
    out=$(p tmux detach 2>&1) || fail "swap: detach again exits 0" "$out"
    check "swap: detach again has nothing to do" grep -q 'not attached' <<<"$out"
}

sprites() { p --json draw ls | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["sprites"]))'; }

s_apps() {
    start || return
    p tmux attach --app clock >/dev/null || fail "apps: attach --app exits 0"
    eventually '[ "$(sprites)" -ge 1 ]'
    check "apps: the app draws from the start" test "$(sprites)" -ge 1
    local n
    n=$(sprites)
    p tmux attach --app 'fishes -a' >/dev/null || fail "apps: attach --app again exits 0"
    eventually '[ "$(sprites)" -gt $n ]'
    check "apps: an app joins a session attached already" test "$(sprites)" -gt "$n"
    check "apps: still one client" is "$(clients)" 1
    p tmux detach
    until_is tmux running
    # Ours alone: an app of another driver on this machine is not counted.
    left() {
        local pid n=0
        for pid in $(pgrep -f "ptyrc-app (clock|fishes)"); do
            tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qx "PTYRC_SOCKET=$WORK/run/.*" && n=$((n + 1))
        done
        echo "$n"
    }
    until_is 0 left
    check "apps: the apps end with the driver" is "$(left)" 0
    local out status
    out=$(p tmux attach --app no-such-app-zz 2>&1); status=$?
    check "apps: an unknown app is told" is "$status $(grep -c 'ended at once' <<<"$out")" "1 1"
    check "apps: and is not fatal to the swap" is "$(option @ptyrc)" tmux-main
    p tmux detach
}

s_leave() {
    start || return
    p tmux attach >/dev/null
    # prefix d: the driver ends with its client, as tmux would.
    "${IN[@]}" detach-client -s main
    until_is bash running
    check "leave: detached by tmux, the terminal is the shell's" is "$(running)" bash
    check "leave: @ptyrc is unset" is "$(option @ptyrc)" ""
    check "leave: no driver" is "$("$PTYRC" ls 2>/dev/null | grep -c tmux-main)" 0
    check "leave: the shell says so" grep -q 'detached' <(shown)
}

s_typed() {
    start || return
    "${IN[@]}" send-keys -t main "$PTYRC tmux attach" Enter
    until_is tmux-main option @ptyrc
    until_is ptyrc running
    check "typed: attach, typed in a pane" is "$(running) $(option @ptyrc)" "ptyrc tmux-main"
    eventually 'shown | grep -q "ptyrc session tmux-main"'
    check "typed: the pane names the session" grep -q 'ptyrc session tmux-main' <(shown)
    "${IN[@]}" send-keys -t main "$PTYRC find hello-pane && $PTYRC tmux detach" Enter
    until_is tmux running
    check "typed: detach, typed in a pane" is "$(running) $(option @ptyrc)" "tmux "
    check "typed: one plain client" is "$(clients)" 1
}

s_fallback() {
    # The driver of the swapped client has no socket directory.
    start "XDG_RUNTIME_DIR=/proc/none" || return
    local out status
    out=$(p tmux attach 2>&1); status=$?
    check "fallback: attach fails" is "$status" 5
    check "fallback: and says why" grep -q 'no socket directory' <<<"$out"
    until_is tmux running
    check "fallback: the terminal runs tmux, plain" is "$(running) $(clients)" "tmux 1"
    check "fallback: nothing is left" is "$(option @ptyrc)$(option @ptyrc-error)" ""
}

s_outside() {
    start || return
    "${IN[@]}" detach-client -s main
    until_is bash running
    local out
    out=$(p tmux attach 2>&1)
    check "outside: no client to swap is refused" is "$? $(grep -c 'no client' <<<"$out")" "3 1"
    # On the terminal, not in tmux: the driver starts there.
    "${HOST[@]}" send-keys -t host "$PTYRC tmux attach -L ${IN[2]} -t main" Enter
    eventually 'p cursor'
    check "outside: attach from the terminal" is "$(running) $(clients) $(option @ptyrc)" "ptyrc 1 tmux-main"
    p tmux detach
    until_is tmux running
    check "outside: detach leaves a plain client" is "$(running) $(clients)" "tmux 1"
}

# The binary of the driver over the client of `main`: the client keeps
# its pid from a swap to the next.
driver() { basename "$(readlink "/proc/$(pgrep -f "tmux wrap -t .* -S $SOCKET" | head -1)/exe")"; }
# The child apps of the driver that run.
children() {
    p --json manage ls | python3 -c 'import json,sys
print(sum(a["kind"] == "child" and a["state"] == "running" for a in json.load(sys.stdin)))'
}

s_restart() {
    start || return
    local before out bin=$PTYRC
    before=$(shown)
    # The binary before and after a build.
    cp "$PTYRC" "$WORK/ptyrc-old"; cp "$PTYRC" "$WORK/ptyrc-new"
    # Not attached: an attach.
    PTYRC=$WORK/ptyrc-old
    out=$(p tmux restart --app clock) || fail "restart: not attached, exits 0" "$out"
    check "restart: not attached, it attaches" is "$(running) $(option @ptyrc)" "ptyrc-old tmux-main"
    eventually '[ "$(sprites)" -ge 1 ]'
    check "restart: a driver, its app" is "$(driver) $(children)" "ptyrc-old 1"
    PTYRC=$WORK/ptyrc-new
    out=$(p tmux restart) || fail "restart: exits 0" "$out"
    check "restart: names the session" grep -q 'tmux-main' <<<"$out"
    check "restart: the session is named in @ptyrc" is "$(option @ptyrc)" tmux-main
    check "restart: the driver is of the binary as it is now" is "$(running) $(driver)" "ptyrc-new ptyrc-new"
    check "restart: one client" is "$(clients)" 1
    until_is "$before" shown
    check "restart: the terminal shows the same" is "$(shown)" "$before"
    eventually '[ "$(sprites)" -ge 1 ]'
    check "restart: its app runs again, once" is "$(children) $(test "$(sprites)" -ge 1; echo $?)" "1 0"
    # Typed in a pane of the session.
    "${IN[@]}" send-keys -t main "$WORK/ptyrc-old tmux restart" Enter
    eventually '[ "$(driver)" = ptyrc-old ] && [ "$(option @ptyrc)" = tmux-main ] && [ "$(children)" = 1 ]'
    check "restart: typed in a pane" is "$(driver) $(option @ptyrc) $(children) $(clients)" "ptyrc-old tmux-main 1 1"
    p tmux detach
    until_is tmux running
    check "restart: detach leaves a plain client" is "$(running) $(clients)" "tmux 1"
    PTYRC=$bin
}

[ $# -eq 0 ] && set -- swap apps leave typed fallback outside restart
for s in "$@"; do
    "s_$s"
done
exit $FAILED
