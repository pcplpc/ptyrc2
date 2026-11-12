#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Heat and idle against real programs: a shell, vim, htop. The driver runs
# inside a detached tmux host. Prints PASS, FAIL or SKIP per check; exit
# status 1 if a check failed.
#
#   scripts/e2e-heat.sh [SCENARIO...]     default: all
#   scenarios: shell vim monitor

cd "$(dirname "$0")/.." || exit 2
. scripts/lib-e2e.sh

VIM=/usr/bin/vim
WORK=temp/e2e-heat
FAILED=0

mkdir -p "$WORK"
seq -f 'line %03g of a file that does not move' 1 200 >"$WORK/long.txt"

have() { command -v "$1" >/dev/null 2>&1 || [ -x "$1" ]; }

# check NAME EXPR: EXPR is Python over I (`frame.idle` of the raw frame),
# C (its classes, one string per row) and share(letter).
check() {
    local name=$1 expr=$2
    if python3 - "$expr" \
        "$(e2e_p --json idle 2>/dev/null)" \
        "$(e2e_p --json heat 2>/dev/null)" <<'PY'
import json, sys
I = json.loads(sys.argv[2])
C = json.loads(sys.argv[3])["classes"]
def share(letter):
    return sum(row.count(letter) for row in C) / max(1, sum(map(len, C)))
if not eval(sys.argv[1]):
    print("   ", I, {k: round(share(k), 3) for k in "htp."}, file=sys.stderr)
    sys.exit(1)
PY
    then
        echo "PASS  $name"
    else
        echo "FAIL  $name"
        FAILED=1
    fi
}

s_shell() {
    e2e_start heat-shell 100x30 env PS1='$ ' bash --norc --noprofile || return
    e2e_send 'echo hello'; e2e_keys enter
    check "shell: just started, not idle" 'not I["idle"] and I["score"] < 0.2'
    sleep 11
    check "shell: left alone, idle" 'I["idle"] and I["score"] == 1.0 and I["free"] > 0.9'
    if e2e_p idle >/dev/null; then echo "PASS  shell: ptyrc idle exits 0"; else echo "FAIL  shell: ptyrc idle exits 0"; FAILED=1; fi
    e2e_send 'clear'; e2e_keys enter; sleep 0.5
    check "shell: clear of a nearly empty screen keeps it idle" 'I["idle"] and share("h") < 0.05'
    e2e_send 'seq 200'; e2e_keys enter; sleep 0.5
    check "shell: output makes the text columns hot, not the empty ones" \
        'share("h") > 0.02 and share(".") > 0.8 and I["score"] < 1.0'
    e2e_stop
}

s_vim() {
    have $VIM || { echo "SKIP  vim"; return; }
    e2e_start heat-vim 100x30 $VIM -u NONE -N -n "$WORK/long.txt" || return
    sleep 11
    check "vim: left alone, idle, text is static" 'I["score"] == 1.0 and share("t") > 0.2 and share("h") == 0'
    e2e_keys ctrl+l; sleep 0.3
    # The redraw drops the file message of the last row; nothing else.
    check "vim: a redraw of the same text is no change" \
        'I["idle"] and all("h" not in row for row in C[:-1])'
    e2e_keys ctrl+f; sleep 0.3
    check "vim: a page down heats the digits only" '0 < share("h") < 0.1 and I["idle"]'
    e2e_stop
}

s_monitor() {
    have htop || { echo "SKIP  monitor"; return; }
    e2e_start heat-monitor 100x30 htop || return
    sleep 14
    check "monitor: hot where numbers change, static elsewhere" \
        'share("h") > 0.005 and share("h") < 0.6 and share("t") + share(".") + share("p") > 0.4'
    check "monitor: the score is the static share" 'abs(I["score"] - (1 - share("h"))) < 0.15'
    e2e_stop
}

[ $# -eq 0 ] && set -- shell vim monitor
for s in "$@"; do
    "s_$s"
done
exit $FAILED
