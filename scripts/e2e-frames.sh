#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Frames against real programs: vim, tmux, a shell, a pager, an agent-like
# TUI. The driver runs inside a detached tmux host. Prints PASS, FAIL or
# SKIP per check; exit status 1 if a check failed.
#
#   scripts/e2e-frames.sh [SCENARIO...]     default: all
#   scenarios: vim vim-nu vim-nowrap vim-splits tmux shell pager agent resize

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
        'pane()["origin"] == [0, 47] and pane()["extent"] == [0, 71]'
    e2e_p find --known -f "$(pane_id)" 'step 003' >/dev/null \
        && echo "PASS  agent: early steps are found in the buffer" \
        || { echo "FAIL  agent: early steps are found in the buffer"; FAILED=1; }
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

for scenario in "${@:-vim vim-nu vim-nowrap vim-splits tmux shell pager agent resize}"; do
    for one in $scenario; do
        case $one in
            vim) s_vim ;;
            vim-nu) s_vim_nu ;;
            vim-nowrap) s_vim_nowrap ;;
            vim-splits) s_vim_splits ;;
            tmux) s_tmux ;;
            shell) s_shell ;;
            pager) s_pager ;;
            agent) s_agent ;;
            resize) s_resize ;;
            *) echo "unknown scenario: $one" >&2; exit 2 ;;
        esac
    done
done
exit $FAILED
