#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Frames against real programs: vim, tmux, a shell, a pager, an agent-like
# TUI. The driver runs inside a detached tmux host. Prints PASS, FAIL or
# SKIP per check; exit status 1 if a check failed.
#
#   scripts/e2e-frames.sh [SCENARIO...]     default: all
#   scenarios: vim vim-nu vim-nowrap vim-splits tmux shell pager pages
#              pages-status pages-share man more agent monitor
#              monitor-generic monitor-slow resize

cd "$(dirname "$0")/.." || exit 2
. scripts/lib-e2e.sh

VIM=/usr/bin/vim
WORK=temp/e2e-frames
FAILED=0
INNER="ptyrc-inner-$$"

cleanup() {
    tmux -L "$INNER" kill-server 2>/dev/null
    e2e_cleanup
}
trap cleanup EXIT

mkdir -p "$WORK"
python3 - "$WORK" <<'EOF'
import random, sys
out = sys.argv[1]
random.seed(1)
words = ("the quick brown fox jumps over lazy dog garden river stone cloud "
         "winter flower apple window silver").split()
with open(f"{out}/long.txt", "w") as f:
    for i in range(1, 301):
        f.write(f"line {i:03d}: " + " ".join(random.choices(words, k=random.randint(3, 9))) + "\n")
with open(f"{out}/other.txt", "w") as f:
    for i in range(1, 121):
        f.write(f"other {i:03d} " + " ".join(random.choices(words, k=random.randint(2, 6))) + "\n")
with open(f"{out}/wide.txt", "w") as f:
    for i in range(1, 201):
        f.write(f"W{i:03d} " + " ".join(random.choices(words, k=random.randint(20, 45))) + "\n")
EOF

have() { command -v "$1" >/dev/null 2>&1 || [ -x "$1" ]; }

# check NAME EXPR: EXPR is Python over F (frames on screen, by id), P
# (panes), A (all frames, inactive too) and pane(i) (i-th pane in reading
# order).
check() {
    local name=$1 expr=$2
    if e2e_p frames --all --json 2>/dev/null | python3 -c '
import json, sys
A = json.load(sys.stdin)["frames"]
F = {f["id"]: f for f in A if f["state"] != "inactive"}
P = sorted((f for f in F.values() if f["kind"] == "pane"),
           key=lambda f: (f["rect"][1], f["rect"][0]))
def pane(i=0): return P[i]
def ids(): return [f["id"] for f in P]
ok = eval(sys.argv[1])
if not ok:
    for f in P:
        print("   ", f, file=sys.stderr)
sys.exit(0 if ok else 1)
' "$expr"; then
        echo "PASS  $name"
    else
        echo "FAIL  $name    [$expr]"
        FAILED=1
    fi
}

# known FRAME FILE [FROM]: the remembered lines equal the file's.
known() {
    local name=$1 frame=$2 file=$3
    if e2e_p call frame.text "{\"frame\":$frame}" --json 2>/dev/null | python3 -c '
import json, sys
res = json.load(sys.stdin)
lines, y0 = res["lines"], res["y0"]
want = [l.rstrip("\n") for l in open(sys.argv[1])]
bad = [(y0 + i, l) for i, l in enumerate(lines)
       if l and not (0 <= y0 + i < len(want) and want[y0 + i].startswith(l))]
if bad or not any(lines):
    print("   ", bad[:3], len(lines), file=sys.stderr)
sys.exit(1 if bad or not any(lines) else 0)
' "$file"; then
        echo "PASS  $name"
    else
        echo "FAIL  $name"
        FAILED=1
    fi
}

settle() { sleep "${1:-2}"; }
quiet() { sleep "${1:-0.35}"; }
vimcmd() { e2e_keys esc; e2e_send ":$1"; e2e_keys enter; quiet; }
pane_id() { e2e_p frames --json | python3 -c '
import json, sys
P = sorted((f for f in json.load(sys.stdin)["frames"] if f["kind"] == "pane" and f["state"] != "inactive"),
           key=lambda f: (f["rect"][1], f["rect"][0]))
print(P[int(sys.argv[1])]["id"])' "${1:-0}"; }

s_vim() {
    have $VIM || { echo "SKIP  vim"; return; }
    e2e_start fr-vim 100x30 $VIM -u NONE -N -n "$WORK/long.txt" || return
    settle
    check "vim: one stable pane, preset, command line is chrome" \
        'len(P) == 1 and pane()["usable"] and pane()["support"] == "vim" and pane()["rect"] == [0, 0, 100, 29]'
    local id; id=$(pane_id)
    e2e_keys ctrl+e ctrl+e ctrl+e; quiet
    check "vim: three lines down" 'pane()["origin"] == [0, 3]'
    e2e_keys ctrl+d; quiet; e2e_keys ctrl+d; quiet
    check "vim: two half pages" 'pane()["origin"] == [0, 31]'
    e2e_keys G; quiet
    check "vim: G goes to an island, same frame" "pane()['id'] == $id and pane()['origin'][1] >= 100000"
    e2e_keys g g; quiet
    check "vim: gg comes back to row 0" "pane()['id'] == $id and pane()['origin'] == [0, 0]"
    e2e_send '/line 150'; e2e_keys enter; quiet
    e2e_keys g g; quiet
    local i; for i in $(seq 22); do e2e_keys ctrl+d; sleep 0.1; done; quiet
    check "vim: walking to the end merges the islands" \
        'pane()["origin"] == [0, 271] and pane()["extent"] == [0, 299]'
    known "vim: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_keys g g; quiet
    e2e_send '5GoNEW LINE'; e2e_keys esc; quiet 0.5
    check "vim: a line opened keeps the origin" 'pane()["origin"] == [0, 0]'
    e2e_p call frame.text "{\"frame\":$id,\"y0\":4,\"y1\":7}" --json | grep -q '"NEW LINE","line 006' \
        && echo "PASS  vim: the opened line is spliced in" \
        || { echo "FAIL  vim: the opened line is spliced in"; FAILED=1; }
    e2e_send 'dd'; quiet 0.5
    known "vim: deleted again, the buffer is the file" "$id" "$WORK/long.txt"
    e2e_send 'gg'; quiet
    e2e_keys ctrl+f; quiet; e2e_keys ctrl+f; quiet; e2e_keys ctrl+f; quiet
    check "vim: ctrl+f, a page with two rows of overlap, is a scroll" \
        "ids() == [$id] and pane()['origin'][1] == 81 and pane()['extent'][0] == 0 and pane()['extent'][1] < 300"
    e2e_stop
}

s_vim_nu() {
    have $VIM || { echo "SKIP  vim-nu"; return; }
    e2e_start fr-nu 100x30 $VIM -u NONE -N -n -c 'set nu' "$WORK/long.txt" || return
    settle
    check "vim-nu: the number column is left chrome" 'pane()["rect"] == [4, 0, 96, 29] and pane()["origin"] == [0, 0]'
    e2e_send '150G'; quiet
    check "vim-nu: a jump is placed by the numbers" 'pane()["origin"][1] in range(121, 150) and pane()["extent"][1] < 1000'
    vimcmd 'set nonu'; e2e_keys j; quiet
    check "vim-nu: numbers off, content rect back" 'pane()["rect"] == [0, 0, 100, 29] and pane()["origin"][0] == 0'
    known "vim-nu: the remembered buffer is the file" "$(pane_id)" "$WORK/long.txt"
    e2e_stop
}

s_vim_nowrap() {
    have $VIM || { echo "SKIP  vim-nowrap"; return; }
    e2e_start fr-nowrap 80x24 $VIM -u NONE -N -n -c 'set nowrap' "$WORK/wide.txt" || return
    settle
    e2e_send '20zl'; quiet
    check "vim-nowrap: 20 columns right" 'pane()["origin"] == [20, 0]'
    e2e_send '40zl'; quiet
    check "vim-nowrap: 40 more" 'pane()["origin"] == [60, 0]'
    e2e_keys ctrl+d; quiet
    check "vim-nowrap: half a page down, wherever vim re-centres" \
        'pane()["origin"][1] == 11 and pane()["extent"][1] < 1000'
    known "vim-nowrap: the remembered buffer is the file" "$(pane_id)" "$WORK/wide.txt"
    e2e_stop
}

s_vim_splits() {
    have $VIM || { echo "SKIP  vim-splits"; return; }
    e2e_start fr-split 100x30 $VIM -u NONE -N -n "$WORK/long.txt" || return
    settle
    local first; first=$(pane_id)
    vimcmd "split $WORK/other.txt"
    check "vim-splits: the new panes wait" 'len(P) == 2 and not any(f["usable"] for f in P)'
    settle
    check "vim-splits: two stable panes; the first file kept its frame, below" \
        "len(P) == 2 and all(f['usable'] for f in P) and pane(1)['id'] == $first"
    e2e_keys ctrl+d; quiet
    check "vim-splits: scrolling one window leaves the other" 'pane(0)["origin"][1] > 0 and pane(1)["origin"] == [0, 0]'
    check "vim-splits: keys name the files" \
        'pane(0)["key"].endswith("other.txt") and pane(1)["key"].endswith("long.txt")'
    local top; top=$(pane_id 0)
    vimcmd "vsplit $WORK/wide.txt"; settle
    check "vim-splits: three panes" "len(P) == 3 and $top in ids() and $first in ids()"
    vimcmd 'q'; settle
    check "vim-splits: closed, two panes again, same frames" "ids() == [$top, $first]"
    vimcmd 'q'; settle
    check "vim-splits: closed, one pane, the first frame" "ids() == [$first] and pane()['rect'] == [0, 0, 100, 29]"
    vimcmd "tabnew $WORK/other.txt"; settle
    check "vim-splits: a tab is another frame" "len(P) == 1 and pane()['id'] != $first"
    e2e_send 'gt'; settle
    check "vim-splits: back to the first tab, the first frame" "ids() == [$first] and pane()['origin'] == [0, 0]"
    known "vim-splits: the first frame still holds its file" "$first" "$WORK/long.txt"
    e2e_stop
}

T() { tmux -L "$INNER" "$@"; }

s_tmux() {
    have tmux || { echo "SKIP  tmux"; return; }
    e2e_start fr-tmux 120x36 tmux -L "$INNER" -f /dev/null new-session -s work env PS1='$ ' bash --norc --noprofile || return
    settle 2.2
    check "tmux: one pane above the status line, keyed by window" \
        'len(P) == 1 and pane()["rect"] == [0, 0, 120, 35] and pane()["key"] == "tmux:work:0"'
    local a; a=$(pane_id)
    T split-window -h env PS1='$ ' bash --norc --noprofile; settle 2.2
    check "tmux: two panes, the first kept its frame" "len(P) == 2 and pane(0)['id'] == $a and all(f['usable'] for f in P)"
    T split-window -v env PS1='$ ' bash --norc --noprofile; T select-pane -t 0
    T split-window -v env PS1='$ ' bash --norc --noprofile; settle 2.2
    check "tmux: four panes" 'len(P) == 4 and all(f["usable"] for f in P)'
    local four; four=$(e2e_p frames --json | python3 -c '
import json, sys
print(sorted(f["id"] for f in json.load(sys.stdin)["frames"] if f["kind"] == "pane" and f["state"] != "inactive"))')
    T send-keys -t 0 'for i in $(seq 300); do echo "row $i of the output"; done' Enter; quiet 0.8
    check "tmux: output in one pane leaves the others at rest" \
        "sorted(ids()) == $four and all(f['origin'] == [0, 0] for f in P[1:])"
    T new-window env PS1='$ ' bash --norc --noprofile; settle 2.2
    check "tmux: another window, another frame and key" \
        "len(P) == 1 and pane()['id'] not in $four and pane()['key'] == 'tmux:work:1'"
    if have $VIM; then
        T new-window "$VIM -u NONE -N -n $WORK/long.txt"; settle 2.2
        check "tmux: vim in a window gets the vim preset" 'len(P) == 1 and pane()["support"] == "vim"'
        local v; v=$(pane_id)
        T send-keys C-d; quiet
        check "tmux: vim scrolls inside tmux" 'pane()["origin"][1] > 0'
    fi
    T select-window -t 0; quiet 0.3
    check "tmux: back to window 0: the same four frames, waiting" \
        "sorted(ids()) == $four and not any(f['usable'] for f in P)"
    settle 2.2
    check "tmux: and stable again" "sorted(ids()) == $four and all(f['usable'] for f in P)"
    if have $VIM; then
        T select-window -t 2; settle 2.2
        check "tmux: back to the vim window: its frame" "ids() == [$v]"
        T select-window -t 0; settle 2.2
    fi
    T resize-pane -t 0 -x 40; settle 2.2
    check "tmux: a pane resized keeps the frames" "sorted(ids()) == $four"
    T select-pane -t 1; quiet; T resize-pane -Z; settle 2.2
    check "tmux: zoom: one pane, one of the four" "len(P) == 1 and pane()['id'] in $four"
    T resize-pane -Z; settle 2.2
    check "tmux: unzoom: the four again" "sorted(ids()) == $four"
    T kill-server
    e2e_stop
}

s_shell() {
    have bash || { echo "SKIP  shell"; return; }
    e2e_start fr-shell 100x30 env PS1='$ ' bash --norc --noprofile || return
    settle
    local id; id=$(pane_id)
    e2e_send 'seq 1 10'; e2e_keys enter; quiet
    check "shell: output on a nearly empty screen is not a jump" "pane()['id'] == $id and pane()['origin'] == [0, 0]"
    e2e_send 'seq 1 200'; e2e_keys enter; quiet
    check "shell: 200 lines at once: scrolled by as much" "pane()['id'] == $id and pane()['origin'] == [0, 183]"
    e2e_send 'for i in $(seq 40); do echo "tick $i"; sleep 0.03; done'; e2e_keys enter; settle 2.5
    check "shell: a slow loop scrolls line by line" "pane()['origin'] == [0, 224] and pane()['extent'][0] == 0"
    e2e_p find --known -f "$id" 'tick 7' >/dev/null \
        && echo "PASS  shell: scrolled-off output is found in the buffer" \
        || { echo "FAIL  shell: scrolled-off output is found in the buffer"; FAILED=1; }
    if have $VIM; then
        e2e_send "$VIM -u NONE -N -n $WORK/long.txt"; e2e_keys enter; settle
        check "shell: vim on the alternate screen is another frame" "len(P) == 1 and pane()['id'] != $id and pane()['support'] == 'vim'"
        e2e_send ':q'; e2e_keys enter; settle
        check "shell: vim gone, the shell frame is back where it was" "ids() == [$id] and pane()['origin'][1] in (225, 226)"
    fi
    local i; for i in $(seq 12); do e2e_send "clear; echo run $i"; e2e_keys enter; quiet 0.25; done
    check "shell: cleared twelve times: same frame, the extent is the screen's island" \
        "ids() == [$id] and pane()['extent'][1] - pane()['extent'][0] < 30"
    e2e_p call frame.text "{\"frame\":$id}" --json 2>/dev/null | python3 -c '
import json, sys
lines = json.load(sys.stdin)["lines"]
sys.exit(0 if len(lines) <= 30 and any("run 12" in l for l in lines) else 1)' \
        && echo "PASS  shell: frame.text without a range is the screen's island" \
        || { echo "FAIL  shell: frame.text without a range is the screen's island"; FAILED=1; }
    e2e_stop
}

s_pager() {
    have less || { echo "SKIP  pager"; return; }
    e2e_start fr-pager 100x30 less "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_keys j j j; quiet
    check "pager: three lines down" "pane()['origin'][1] == 3"
    e2e_keys space; quiet
    e2e_keys g; quiet
    check "pager: a page, then g: back at the top" "pane()['id'] == $id and pane()['origin'][1] == 0"
    local i; for i in $(seq 12); do e2e_keys d; sleep 0.1; done; quiet
    check "pager: half pages to line 181" "pane()['origin'][1] == 180"
    known "pager: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_stop
}

# mark_at NAME ROW: the host shows the mark `<MK` on screen row ROW, or
# nowhere when ROW is `-`.
mark_at() {
    local name=$1 row=$2 got
    got=$(e2e_host | grep -n '<MK' | cut -d: -f1 | tr '\n' ' ')
    if [ "$row" = - ] && [ -z "$got" ] || [ "$got" = "$((row + 1)) " ]; then
        echo "PASS  $name"
    else
        echo "FAIL  $name    [host rows with the mark: ${got:-none}]"
        FAILED=1
    fi
}

# Whole pages have no row in common with the page before: the distance
# comes from how the screen was fed.
s_pages() {
    have less || { echo "SKIP  pages"; return; }
    e2e_start fr-pages 100x30 env LESS= less "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_p draw text -f "$id" --at 90,40 --space frame '<MK' >/dev/null
    local i; for i in 1 2 3 4; do e2e_keys space; quiet; done
    check "pages: four pages down, one frame, no island" \
        "ids() == [$id] and pane()['origin'][1] == 116 and pane()['extent'] == [0, 144]"
    for i in 1 2 3; do e2e_keys b; quiet; done
    check "pages: three pages up" "pane()['origin'][1] == 29"
    mark_at "pages: a drawing fixed on frame row 40 is back on screen row 11" 11
    e2e_keys b; quiet
    check "pages: back at the top" "ids() == [$id] and pane()['origin'][1] == 0"
    mark_at "pages: and off screen at the top" -
    known "pages: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_send '  '; quiet
    check "pages: two pages typed at once" "ids() == [$id] and pane()['origin'][1] == 58"
    e2e_send 'bb'; quiet
    check "pages: and two back" "ids() == [$id] and pane()['origin'][1] == 0 and pane()['extent'] == [0, 144]"
    e2e_keys G; quiet
    check "pages: G feeds a whole screen: a jump, to an island" "ids() == [$id] and pane()['origin'][1] >= 100000"
    e2e_p draw clear >/dev/null
    local island; island=$(e2e_p frames --json | python3 -c '
import json, sys
print([f["origin"][1] for f in json.load(sys.stdin)["frames"] if f["kind"] == "pane" and f["state"] != "inactive"][0])')
    e2e_p draw text -f "$id" --at 90,$((island + 5)) --space frame '<MK' >/dev/null
    quiet; mark_at "pages: a drawing fixed on the island, row 5" 5
    for i in 1 2 3 4 5; do e2e_keys b; quiet; done
    check "pages: walking up into known rows merges the island" \
        "ids() == [$id] and pane()['origin'][1] == 126 and pane()['extent'] == [0, 299]"
    e2e_keys G; quiet
    check "pages: G again: found in the buffer" "pane()['origin'][1] == 271"
    mark_at "pages: the drawing moved with the island: row 5" 5
    e2e_keys g; quiet
    check "pages: g clears and repaints: found again at the top" "ids() == [$id] and pane()['origin'][1] == 0"
    e2e_stop
}

# A prompt that tells the line (`less -M`, `man`): rows are absolute.
# A percentage on the prompt (`less -m`): islands in the order of the
# file, near their rows.
s_pages_share() {
    have less || { echo "SKIP  pages-share"; return; }
    e2e_start fr-share 100x30 env LESS= less -m "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_keys space; quiet
    check "pages-share: one page, the prompt row is chrome at once" \
        "pane()['origin'][1] == 29 and pane()['rect'] == [0, 0, 100, 29] and pane()['extent'] == [0, 57]"
    e2e_keys G; quiet
    check "pages-share: G: (END), an island near the last rows" \
        "ids() == [$id] and 200250 <= pane()['origin'][1] <= 200290"
    e2e_keys g; quiet
    e2e_send '50%'; quiet
    check "pages-share: 50%: an island between the two, near the middle" \
        "ids() == [$id] and 100100 <= pane()['origin'][1] <= 100150"
    e2e_keys g; quiet
    check "pages-share: g: the top again" "ids() == [$id] and pane()['origin'][1] == 0"
    e2e_send '50%'; quiet
    local i; for i in 1 2 3 4; do e2e_keys b; quiet; done
    check "pages-share: walked into the first rows: merged, exact" \
        "ids() == [$id] and pane()['origin'][1] < 60 and pane()['extent'][0] == 0 and pane()['extent'][1] < 300"
    known "pages-share: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_stop
}

s_pages_status() {
    have less || { echo "SKIP  pages-status"; return; }
    e2e_start fr-pstat 100x30 env LESS= less -M "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_p draw text -f "$id" --at 90,160 --space frame '<MK' >/dev/null
    e2e_keys space; quiet
    check "pages-status: a page down" "pane()['origin'][1] == 29"
    e2e_keys G; quiet
    check "pages-status: G lands on its line, no island" \
        "ids() == [$id] and pane()['origin'][1] == 271 and pane()['extent'][1] == 299"
    e2e_send 150g; quiet
    check "pages-status: 150g" "pane()['origin'][1] == 149"
    mark_at "pages-status: a drawing on a row never seen before shows there" 11
    e2e_keys g; quiet
    check "pages-status: g" "ids() == [$id] and pane()['origin'][1] == 0 and pane()['extent'] == [0, 299]"
    known "pages-status: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_stop
}

s_man() {
    have man && have less || { echo "SKIP  man"; return; }
    e2e_start fr-man 100x30 env MANPAGER=less LESS= MANWIDTH=90 man bash || return
    settle 3
    local id; id=$(pane_id)
    e2e_keys space; quiet; e2e_keys space; quiet
    check "man: two pages down" "ids() == [$id] and pane()['origin'][1] == 58"
    e2e_keys G; settle 1.5
    check "man: G lands on its line, no island" \
        "ids() == [$id] and 1000 < pane()['origin'][1] < 100000 and pane()['extent'][0] == 0"
    e2e_keys g; quiet 0.8
    check "man: g" "ids() == [$id] and pane()['origin'][1] == 0"
    e2e_send '/^SIGNALS'; e2e_keys enter; quiet 0.8
    check "man: a search lands on its line" "ids() == [$id] and 500 < pane()['origin'][1] < 100000"
    e2e_p find -f "$id" SIGNALS >/dev/null \
        && echo "PASS  man: the hit is on screen" \
        || { echo "FAIL  man: the hit is on screen"; FAILED=1; }
    e2e_keys b; quiet
    e2e_keys space; quiet
    check "man: a page up and down around the hit" "ids() == [$id] and 500 < pane()['origin'][1] < 100000"
    e2e_stop
}

# `more` runs on the main screen: pages scroll into the scrollback.
s_more() {
    have more || { echo "SKIP  more"; return; }
    e2e_start fr-more 100x30 more "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_p draw text -f "$id" --at 90,70 --space frame '<MK' >/dev/null
    local i; for i in 1 2 3; do e2e_keys space; quiet; done
    check "more: three pages, one frame, no island" \
        "ids() == [$id] and pane()['origin'][1] == 87 and pane()['extent'][0] == 0 and pane()['extent'][1] < 120"
    mark_at "more: a drawing fixed on frame row 70 went up with the text" -
    e2e_keys enter; quiet
    check "more: one line" "pane()['origin'][1] == 88"
    known "more: the remembered buffer is the file" "$id" "$WORK/long.txt"
    e2e_stop
}

s_agent() {
    have python3 || { echo "SKIP  agent"; return; }
    cat > "$WORK/agent_tui.py" <<'EOF'
"""A stand-in for a coding-agent TUI: a transcript streaming on the main
screen above a rounded prompt box pinned at the bottom."""
import shutil, sys, time
cols, rows = shutil.get_terminal_size()
out = sys.stdout
def draw_box():
    inner = cols - 2
    for line in ("╭" + "─" * inner + "╮", "│ > " + " " * (inner - 3) + "│",
                 "╰" + "─" * inner + "╯", "  ? for shortcuts".ljust(cols)):
        out.write(line + "\r\n")
    out.write("\x1b[4A")
def emit(line):
    out.write("\x1b[J" + line[:cols] + "\r\n")
    draw_box()
    out.flush()
draw_box()
out.flush()
for i in range(1, int(sys.argv[1]) + 1):
    emit(f"● step {i:03d}: reading file_{i}.rs and thinking about it")
    time.sleep(0.04)
    if i % 5 == 0:
        emit(f"  ⎿ {i * 7} lines")
time.sleep(60)
EOF
    e2e_start fr-agent 100x30 python3 "$WORK/agent_tui.py" 60 || return
    settle 4.5
    check "agent: the preset is seen, the prompt box is chrome" \
        'pane()["support"] == "agent-tui" and pane()["rect"] == [0, 0, 100, 25] and pane()["usable"]'
    check "agent: the transcript is one frame, scrolled line by line" \
        'pane()["extent"][1] - pane()["extent"][0] == 71 and pane()["origin"][1] - pane()["extent"][0] == 47'
    e2e_p call frame.text "{\"frame\":$(pane_id)}" --json | python3 -c '
import json, sys
lines = json.load(sys.stdin)["lines"]
steps = [l for l in lines if l.startswith("● step")]
sys.exit(0 if steps == [f"● step {i:03d}: reading file_{i}.rs and thinking about it" for i in range(1, 61)] else 1)' \
        && echo "PASS  agent: the remembered transcript is complete and in order" \
        || { echo "FAIL  agent: the remembered transcript is complete and in order"; FAILED=1; }
    e2e_p find --known -f "$(pane_id)" 'step 003' >/dev/null \
        && echo "PASS  agent: early steps are found in the buffer" \
        || { echo "FAIL  agent: early steps are found in the buffer"; FAILED=1; }
    e2e_stop
}

s_monitor() {
    have htop || { echo "SKIP  monitor"; return; }
    e2e_start fr-monitor 100x30 htop || return
    settle 1
    local before; before=$(e2e_p frames --json | python3 -c '
import json, sys
print(sorted(f["id"] for f in json.load(sys.stdin)["frames"] if f["state"] != "inactive"))')
    settle 2
    check "monitor: usable once settled, with the frames of the first layout" \
        "len(P) >= 1 and all(f['usable'] for f in P) and sorted(F) == $before"
    settle 4
    check "monitor: and keeps its frames" "sorted(F) == $before"
    e2e_stop
}

# The same without the htop preset: the generic rules tell a monitor.
s_monitor_generic() {
    have htop || { echo "SKIP  monitor-generic"; return; }
    E2E_RUN_ARGS=--no-presets e2e_start fr-mongen 100x30 htop || return
    settle 3
    check "monitor-generic: no preset, usable once settled" \
        "len(P) >= 1 and all(f['usable'] and f['support'] is None for f in P)"
    local head; head=$(e2e_host | grep -n 'PID USER' | head -1 | cut -d: -f1)
    check "monitor-generic: the header is chrome, the table starts under it, selected row included" \
        "any(f['kind'] == 'chrome' and f['rect'][1] == $((head - 1)) and f['rect'][3] == 1 for f in F.values()) \
         and any(f['rect'][1] == $head and f['rect'][3] >= 10 for f in P)"
    local before; before=$(e2e_p frames --all --json | python3 -c '
import json, sys
print(sorted(f["id"] for f in json.load(sys.stdin)["frames"]))')
    settle 5
    check "monitor-generic: refreshes spend no frame and no island" \
        "sorted(f['id'] for f in A) == $before and all(f['extent'][1] < 1000 for f in P)"
    e2e_stop
}

# A table repainted where it is every 3.2 s, the cursor shown: an editor
# could paint a jump so. The third even repaint is a monitor's.
s_monitor_slow() {
    have python3 || { echo "SKIP  monitor-slow"; return; }
    cat > "$WORK/slow_monitor.py" <<'EOF'
import sys, time
n = 0
while True:
    sys.stdout.write("\x1b[H")
    for i in range(20):
        sys.stdout.write(f"{n * 97 + i * 13:6} proc-{(n * 7 + i * 3) % 50:<3} {n + i}.{n % 10}%\x1b[K\r\n")
    sys.stdout.flush()
    n += 1
    time.sleep(3.2)
EOF
    e2e_start fr-monslow 100x30 python3 "$WORK/slow_monitor.py" || return
    settle 2
    local id; id=$(pane_id)
    settle 9
    local at; at=$(e2e_p frames --json | python3 -c '
import json, sys
print([f["origin"][1] for f in json.load(sys.stdin)["frames"] if f["kind"] == "pane"][0])')
    settle 7
    check "monitor-slow: after two repaints, the same rows and frame" \
        "ids() == [$id] and pane()['origin'][1] == $at and pane()['usable']"
    e2e_stop
}

s_resize() {
    have $VIM || { echo "SKIP  resize"; return; }
    e2e_start fr-resize 100x30 $VIM -u NONE -N -n "$WORK/long.txt" || return
    settle
    local id; id=$(pane_id)
    e2e_keys ctrl+d; quiet
    tmux resize-window -t "$E2E_HOST" -x 80 -y 24; quiet 0.4
    check "resize: the frame is kept, and waits" "ids() == [$id] and not pane()['usable']"
    settle
    check "resize: stable at the new size, same rows" \
        "ids() == [$id] and pane()['usable'] and pane()['rect'] == [0, 0, 80, 23] and pane()['origin'][1] in range(14, 22)"
    e2e_stop
}

[ -x "$PTYRC" ] || { echo "no binary at $PTYRC: cargo build -p ptyrc-cli" >&2; exit 2; }
have tmux || { echo "tmux is required as the host" >&2; exit 2; }

for scenario in "${@:-vim vim-nu vim-nowrap vim-splits tmux shell pager pages pages-status pages-share man more agent monitor monitor-generic monitor-slow resize}"; do
    for one in $scenario; do
        case $one in
            vim) s_vim ;;
            vim-nu) s_vim_nu ;;
            vim-nowrap) s_vim_nowrap ;;
            vim-splits) s_vim_splits ;;
            tmux) s_tmux ;;
            shell) s_shell ;;
            pager) s_pager ;;
            pages) s_pages ;;
            pages-status) s_pages_status ;;
            pages-share) s_pages_share ;;
            man) s_man ;;
            more) s_more ;;
            agent) s_agent ;;
            monitor) s_monitor ;;
            monitor-generic) s_monitor_generic ;;
            monitor-slow) s_monitor_slow ;;
            resize) s_resize ;;
            *) echo "unknown scenario: $one" >&2; exit 2 ;;
        esac
    done
done
exit $FAILED
