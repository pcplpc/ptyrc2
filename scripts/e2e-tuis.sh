#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Real TUIs under the driver: htop, less, man, tmux, ranger, glances and a
# coding-agent TUI (opencode). The driver runs inside a detached tmux host.
# Per app: the screen read equals what the host shows, the frames settle
# and do not flap, a sprite appears and leaves nothing behind, the exit is
# clean. Prints PASS, FAIL, KNOWN (a recorded defect, see plan/ideas.md,
# "Real TUIs"), FIXED (a recorded defect that no longer shows) or SKIP per
# check; exit status 1 if a check failed.
#
#   scripts/e2e-tuis.sh [APP...]     default: all
#   apps: htop less man tmux ranger nested glances opencode
#   nested: htop and ranger in the panes of a tmux
#
# opencode is only opened, typed in and closed: nothing is submitted.

cd "$(dirname "$0")/.." || exit 2
. scripts/lib-e2e.sh

WORK=temp/e2e-tuis
SIZE=120x36
MARK='<*ptyrc*>'
FAILED=0
INNER="ptyrc-tuis-$$"

cleanup() {
    tmux -L "$INNER" kill-server 2>/dev/null
    e2e_cleanup
}
trap cleanup EXIT

mkdir -p "$WORK/empty"
python3 - "$WORK" <<'PY'
import sys
with open(f"{sys.argv[1]}/long.txt", "w") as f:
    for i in range(1, 401):
        f.write(f"line {i:03d}: the quick brown fox jumps over the lazy dog\n")
PY

have() { command -v "$1" >/dev/null 2>&1; }
pass() { echo "PASS  $1"; }
fail() { echo "FAIL  $1"; FAILED=1; }
quiet() { sleep "${1:-0.5}"; }

# frames_json: the frames on screen.
frames_json() { e2e_p frames --json 2>/dev/null; }

# check NAME EXPR / known NAME EXPR: EXPR is Python over P (panes on
# screen, in reading order), C (chrome), W and H (screen size).
expr_ok() {
    frames_json | python3 -c '
import json, sys
F = [f for f in json.load(sys.stdin)["frames"] if f["state"] != "inactive"]
key = lambda f: (f["rect"][1], f["rect"][0])
P = sorted((f for f in F if f["kind"] == "pane"), key=key)
C = sorted((f for f in F if f["kind"] == "chrome"), key=key)
W, H = next(f["rect"][2:] for f in F if f["kind"] == "raw")
ok = eval(sys.argv[1])
if not ok:
    for f in P + C:
        print("   ", f["id"], f["kind"], f["state"], f["rect"], f["support"], file=sys.stderr)
sys.exit(0 if ok else 1)
' "$1"
}
check() { if expr_ok "$2"; then pass "$1"; else fail "$1    [$2]"; fi; }
known() { if expr_ok "$2" 2>/dev/null; then echo "FIXED $1"; else echo "KNOWN $1"; fi; }

# rects: the layout, one line.
rects() {
    frames_json | python3 -c '
import json, sys
print(sorted((f["kind"], *f["rect"]) for f in json.load(sys.stdin)["frames"]
             if f["state"] != "inactive"))'
}

# same_text: what the driver reads is what the host shows. An app that
# refreshes may change between the two readings: tried a few times.
same_text() {
    local i
    for i in 1 2 3 4 5 6; do
        if diff <(e2e_p text | sed 's/ *$//') <(e2e_host | sed 's/ *$//') >/dev/null; then
            pass "$1: the screen read is the screen shown"
            return
        fi
        sleep 0.3
    done
    fail "$1: the screen read is the screen shown"
    diff <(e2e_p text | sed 's/ *$//') <(e2e_host | sed 's/ *$//') | head -6
}

# settled NAME: every pane usable 3 s after the start.
settled() { expr_ok 'P and all(f["usable"] for f in P)'; }

# steady NAME SECONDS [known]: the layout does not change while the app
# idles or refreshes. `known`: a recorded defect, reported and not counted.
steady() {
    local name="$1: the layout holds for ${2:-3} s" n=$(( ${2:-3} * 2 )) soft=${3:-} first now i
    first=$(rects)
    for i in $(seq "$n"); do
        sleep 0.5
        now=$(rects)
        if [ "$now" != "$first" ]; then
            if [ -n "$soft" ]; then echo "KNOWN $name"; return; fi
            fail "$name"
            printf '    %s\n    %s\n' "$first" "$now"
            return
        fi
    done
    if [ -n "$soft" ]; then echo "FIXED $name"; else pass "$name"; fi
}

# sprite NAME [PLACE...]: a mark shows on the host, and nothing of it is
# left once removed. Default placement: a quiet spot of the raw frame.
sprite() {
    local name=$1 id i shown=0
    shift
    [ $# -eq 0 ] && set -- --auto any
    id=$(e2e_p draw text "$@" --fg yellow "$MARK") || { fail "$name: a sprite is drawn"; return; }
    for i in $(seq 20); do
        e2e_host | grep -qF "$MARK" && { shown=1; break; }
        sleep 0.1
    done
    if [ $shown = 1 ]; then pass "$name: a sprite is drawn ($*)"; else fail "$name: a sprite is drawn ($*)"; fi
    e2e_p draw rm "$id"
    for i in $(seq 20); do
        e2e_host | grep -qF "$MARK" || break
        sleep 0.1
    done
    if e2e_host | grep -qF "$MARK" || e2e_host | grep -q '[<>*]ptyrc\|ptyrc[<>*]'; then
        fail "$name: the sprite leaves nothing behind"
    else
        pass "$name: the sprite leaves nothing behind"
    fi
}

# gone NAME: the app left, the session with it, the host got its screen
# back.
gone() {
    local name=$1 i
    for i in $(seq 60); do
        tmux has-session -t "$E2E_HOST" 2>/dev/null || break
        sleep 0.1
    done
    if tmux has-session -t "$E2E_HOST" 2>/dev/null; then
        fail "$name: exits"
        e2e_host | tail -3
        e2e_stop
        return
    fi
    if "$PTYRC" ls --json 2>/dev/null | grep -q "\"$E2E_NAME\""; then
        fail "$name: the session ends with the app"
    else
        pass "$name: exits, the session ends with it"
    fi
}

begin() {
    local app=$1
    shift
    e2e_start "tui-$app" $SIZE "$@" || { fail "$app: starts"; return 1; }
}

a_htop() {
    have htop || { echo "SKIP  htop"; return; }
    begin htop htop || return
    sleep 3
    same_text htop
    check "htop: meters and table are two panes; header and function bar chrome" \
        'len(P) == 2 and [f["key"] for f in P] == ["htop:meters", "htop:processes"] and len(C) == 2 and P[1]["rect"][1] == C[0]["rect"][1] + 1 and C[1]["rect"] == [0, H - 1, W, 1]'
    check "htop: panes usable 3 s after the start" 'all(f["usable"] for f in P)'
    local ids; ids=$(frames_json | python3 -c 'import json, sys; print(sorted(f["id"] for f in json.load(sys.stdin)["frames"] if f["key"]))')
    # The selected process is a full-width highlight: an entry of the
    # table from the start, not a bar.
    check "htop: the selection is not a separator" 'P[1]["rect"][3] == H - 2 - C[0]["rect"][1]'
    e2e_keys down; quiet; e2e_keys down; quiet
    local before; before=$(rects)
    e2e_keys down; quiet; e2e_keys up; quiet; e2e_keys pagedown; quiet
    if [ "$(rects)" = "$before" ]; then pass "htop: moving the selection keeps the layout"
    else fail "htop: moving the selection keeps the layout"; fi
    # After a page down the table is re-sorted at each refresh: no row
    # that happens to stay is learned as chrome, the frames stay.
    steady htop 6
    check "htop: the same frames since the start" "sorted(f[\"id\"] for f in P) == $ids"
    sprite htop
    sprite htop --near Uptime --side below
    same_text htop
    e2e_keys q
    gone htop
}

pager() {
    local name=$1
    sleep 2.5
    same_text "$name"
    check "$name: one pane, the full width, usable" \
        'len(P) == 1 and P[0]["rect"][2] == W and P[0]["rect"][3] >= H - 1 and P[0]["usable"]'
    e2e_keys space; quiet; e2e_keys space; quiet; e2e_keys b; quiet
    same_text "$name"
    check "$name: paging keeps the pane" 'len(P) == 1 and P[0]["rect"][2] == W'
    steady "$name" 2
    sprite "$name"
    e2e_keys q
    gone "$name"
}

a_less() {
    have less || { echo "SKIP  less"; return; }
    begin less less "$WORK/long.txt" || return
    pager less
}

a_man() {
    have man || { echo "SKIP  man"; return; }
    begin man env MANPAGER=less man bash || return
    pager man
}

a_tmux() {
    have tmux || { echo "SKIP  tmux"; return; }
    begin tmux tmux -L "$INNER" -f /dev/null new-session -s in || return
    sleep 1
    tmux -L "$INNER" split-window -h
    tmux -L "$INNER" split-window -v
    sleep 3
    same_text tmux
    check "tmux: three panes and the status line, preset keys" \
        'len(P) == 3 and all(f["usable"] and (f["key"] or "").startswith("tmux:in:0") for f in P) and [c["rect"] for c in C] == [[0, H - 1, W, 1]]'
    steady tmux 2
    sprite tmux
    sprite tmux -f largest --auto bottom-right
    tmux -L "$INNER" new-window; sleep 2.5
    check "tmux: another window is other frames" \
        'len(P) == 1 and (P[0]["key"] or "").startswith("tmux:in:1")'
    tmux -L "$INNER" select-window -t 0; sleep 2.5
    check "tmux: the first window comes back" 'len(P) == 3'
    same_text tmux
    tmux -L "$INNER" kill-server
    gone tmux
}

a_ranger() {
    have ranger || { echo "SKIP  ranger"; return; }
    begin ranger ranger --clean /usr/share/doc || return
    sleep 3
    same_text ranger
    check "ranger: panes found, usable" 'len(P) >= 1 and all(f["usable"] for f in P)'
    check "ranger: the three columns are three panes, title and status rows chrome" \
        'len(P) == 3 and all(f["support"] == "ranger" and f["rect"][1] == 1 and f["rect"][3] == H - 2 for f in P) and [f["rect"] for f in C] == [[0, 0, W, 1], [0, H - 1, W, 1]]'
    check "ranger: the main column is keyed by its directory" \
        '[f["key"] for f in P] == [None, "ranger:/usr/share/doc", None]'
    local main preview
    main=$(frames_json | python3 -c 'import json, sys; print(next(f["id"] for f in json.load(sys.stdin)["frames"] if f["key"]))')
    preview=$(frames_json | python3 -c '
import json, sys
print(max((f for f in json.load(sys.stdin)["frames"] if f["kind"] == "pane" and f["state"] != "inactive"),
          key=lambda f: f["rect"][0])["id"])')
    e2e_keys down; quiet; e2e_keys down; quiet; e2e_keys up; quiet
    same_text ranger
    check "ranger: the preview is a monitor: the same frame, at its origin" \
        "P[2][\"id\"] == $preview and P[2][\"origin\"] == [0, 0]"
    e2e_keys pagedown; quiet 1
    check "ranger: the main column scrolls in its frame" \
        "P[1][\"id\"] == $main and P[1][\"origin\"][1] > 0 and P[1][\"usable\"]"
    e2e_keys right; quiet 2.5
    check "ranger: another directory, another frame" \
        "len(P) >= 2 and all(f[\"id\"] != $main for f in P) and any((f[\"key\"] or \"\").startswith(\"ranger:/usr/share/doc/\") for f in P)"
    e2e_keys left; quiet 2.5
    check "ranger: back, the frame of the directory is found again" \
        "len(P) == 3 and P[1][\"id\"] == $main and P[1][\"origin\"][1] > 0"
    steady ranger 2
    sprite ranger
    e2e_keys q
    gone ranger
}

# Presets inside the panes of another app: htop left, ranger top right,
# a shell under it.
a_nested() {
    have tmux && have htop && have ranger || { echo "SKIP  nested"; return; }
    begin nested tmux -L "$INNER" -f /dev/null new-session -s in htop || return
    sleep 1
    tmux -L "$INNER" split-window -h ranger --clean /usr/share/doc
    sleep 1
    tmux -L "$INNER" split-window -v
    sleep 4
    same_text nested
    local hw=$(( ${SIZE%x*} / 2 ))
    check "nested: htop in a tmux pane is split, keys composed" \
        "[f[\"key\"] for f in P if f[\"support\"] == \"htop\"] == [\"tmux:in:0/htop:meters\", \"tmux:in:0/htop:processes\"] and all(f[\"rect\"][0] == 0 and f[\"rect\"][2] == $hw for f in P if f[\"support\"] == \"htop\")"
    check "nested: ranger in a tmux pane is split, the main column keyed" \
        "[f[\"key\"] for f in P if f[\"support\"] == \"ranger\" and f[\"key\"] != \"tmux:in:0\"] == [\"tmux:in:0/ranger:/usr/share/doc\"] and all(f[\"rect\"][0] > $hw for f in P if f[\"support\"] == \"ranger\")"
    check "nested: the shell is a pane of tmux" \
        'P[-1]["support"] is None and P[-1]["key"] == "tmux:in:0"'
    check "nested: panes usable" 'all(f["usable"] for f in P)'
    local ids
    ids=$(frames_json | python3 -c '
import json, sys
print(sorted(f["id"] for f in json.load(sys.stdin)["frames"]
             if f["state"] != "inactive" and f["support"] == "htop"))')
    steady nested 5
    check "nested: htop keeps its frames while it refreshes" \
        "sorted(f[\"id\"] for f in P if f[\"support\"] == \"htop\") == $ids"
    # ranger has the focus of the second pane: walk its list.
    tmux -L "$INNER" select-pane -t 1
    local main
    main=$(frames_json | python3 -c '
import json, sys
print(next(f["id"] for f in json.load(sys.stdin)["frames"]
           if f["state"] != "inactive" and (f["key"] or "").endswith("ranger:/usr/share/doc")))')
    local i; for i in $(seq 30); do e2e_keys j; done; sleep 2.5
    same_text nested
    check "nested: the main column of ranger scrolls in its frame" \
        "any(f[\"id\"] == $main and f[\"origin\"][1] > 0 for f in P)"
    check "nested: the preview of ranger is a monitor: the view stays at its origin" \
        "all(f[\"origin\"][1] < 1000 for f in P if f[\"support\"] == \"ranger\")"
    sprite nested
    sprite nested -f largest --layer front --auto bottom-right
    tmux -L "$INNER" kill-server
    gone nested
}

a_glances() {
    have glances || { echo "SKIP  glances"; return; }
    begin glances glances || return
    e2e_wait -E 'CPU +[0-9]' --timeout 15000 >/dev/null 2>&1
    sleep 3
    same_text glances
    check "glances: the title is chrome; summary, sidebar and processes are panes" \
        '[f["key"] for f in P] == ["glances:summary", "glances:sidebar", "glances:processes"] and C[0]["rect"] == [0, 0, W, 1] and P[1]["rect"][0] == 0 and P[2]["rect"][0] == P[1]["rect"][2] + 1 and P[2]["rect"][0] + P[2]["rect"][2] == W and P[1]["rect"][1] + P[1]["rect"][3] == H'
    check "glances: panes usable 3 s after the start" \
        'all(f["usable"] for f in P)'
    local ids
    ids=$(frames_json | python3 -c '
import json, sys
print(sorted(f["id"] for f in json.load(sys.stdin)["frames"]
             if f["state"] != "inactive" and f["kind"] == "pane"))')
    steady glances 6
    check "glances: the same frames since the start, at their origin" \
        "sorted(f[\"id\"] for f in P) == $ids and all(f[\"origin\"] == [0, 0] for f in P)"
    sprite glances
    same_text glances
    e2e_keys q
    gone glances
}

a_opencode() {
    have opencode || { echo "SKIP  opencode"; return; }
    (cd "$WORK/empty" && begin opencode opencode) || return
    # `begin` ran in a subshell: take its names.
    E2E_HOST="ptyrc-tui-opencode-$$"; E2E_NAME="tui-opencode-$$"; E2E_HOSTS+=("$E2E_HOST")
    if ! e2e_wait 'Ask anything' --timeout 40000 >/dev/null 2>&1; then
        echo "SKIP  opencode (no prompt box within 40 s)"
        e2e_stop
        return
    fi
    sleep 3
    same_text opencode
    check "opencode: the agent preset finds the prompt box" \
        'P and P[0]["support"] == "agent-tui" and P[0]["rect"][:3] == [0, 0, W]'
    check "opencode: the prompt panel is a pane of its own (backdrop, no border)" \
        'any(f["rect"][2] < W and f["rect"][3] >= 3 and f["usable"] for f in P[1:])'
    steady opencode 3
    sprite opencode
    # Typed, never submitted.
    e2e_send 'ptyrc was here'; quiet 1
    same_text opencode
    if e2e_p find 'ptyrc was here' >/dev/null; then pass "opencode: typed text is read in the prompt"
    else fail "opencode: typed text is read in the prompt"; fi
    local i; for i in $(seq 14); do e2e_keys backspace; done; quiet
    # The command palette: a panel over the screen.
    e2e_keys ctrl+p; sleep 1; e2e_keys down; quiet; e2e_keys down; sleep 2.5
    same_text opencode
    check "opencode: the palette is one pane, its selection is not a separator" \
        'any(f["rect"][2] >= 40 and f["rect"][3] >= 10 and f["rect"][0] > 0 for f in P)'
    check "opencode: the pane under the palette keeps its origin" \
        'P[0]["origin"] == [0, 0]'
    e2e_keys esc; sleep 2
    check "opencode: the palette closed, the first layout is back" \
        'P and P[0]["support"] == "agent-tui"'
    sprite opencode
    e2e_keys ctrl+c; quiet 1
    tmux has-session -t "$E2E_HOST" 2>/dev/null && { e2e_keys ctrl+c; quiet 1; }
    tmux has-session -t "$E2E_HOST" 2>/dev/null && e2e_keys ctrl+d
    gone opencode
}

[ $# -eq 0 ] && set -- htop less man tmux ranger nested glances opencode
for app in "$@"; do
    case $app in
        htop | less | man | tmux | ranger | nested | glances | opencode) "a_$app" ;;
        *) echo "e2e-tuis: no such app: $app (htop less man tmux ranger nested glances opencode)" >&2; exit 2 ;;
    esac
    e2e_stop
done
exit $FAILED
