#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Rendering checks against a real host: the driver runs inside a detached
# tmux, and what tmux shows is compared with the driver's model of the
# host and with the app screen plus sprites.
#
#   scripts/e2e-render.sh [SCENARIO...]     default: all
#
# PASS/FAIL per scenario; exit status 1 on any failure. A scenario whose
# program is missing is skipped.

set -u
cd "$(dirname "$0")/.."
ROOT=$PWD
BIN=${PTYRC_BIN:-$ROOT/target/release/ptyrc}
[ -x "$BIN" ] || BIN=$ROOT/target/debug/ptyrc
[ -x "$BIN" ] || { echo "no ptyrc binary: cargo build --release -p ptyrc-cli" >&2; exit 2; }
command -v tmux >/dev/null || { echo "SKIP all: no tmux"; exit 0; }

TAG=$$
WORK=$ROOT/temp/e2e-render.$TAG
mkdir -p "$WORK"
FAILED=0
SESSIONS=()

cleanup() {
    for s in "${SESSIONS[@]:-}"; do
        [ -n "$s" ] && tmux kill-session -t "ptyrc-rd-$s" 2>/dev/null
    done
    tmux -L "ptyrc-rd-inner-$TAG" kill-server 2>/dev/null
    rm -rf "$WORK"
}
trap cleanup EXIT

# host NAME COLS ROWS CMD...: the driver on CMD inside a detached tmux.
host() {
    local name=$1-$TAG cols=$2 rows=$3
    shift 3
    SESSIONS+=("$name")
    tmux new-session -d -s "ptyrc-rd-$name" -x "$cols" -y "$rows" \
        "exec $BIN run --name rd-$name -- $*"
    tmux set-option -t "ptyrc-rd-$name" history-limit 100000 >/dev/null
    local i
    for i in $(seq 50); do
        "$BIN" -s "rd-$name" call session.info >/dev/null 2>&1 && return 0
        sleep 0.05
    done
    echo "driver rd-$name did not start" >&2
    return 1
}

p() { local name=$1-$TAG; shift; "$BIN" -s "rd-$name" "$@"; }
keys() { local name=$1; shift; p "$name" keys "$@"; }
hostkeys() { local name=$1-$TAG; shift; tmux send-keys -t "ptyrc-rd-$name" "$@"; }
settle() { p "$1" wait --stable "${2:-150}" --timeout 10000 >/dev/null; }
alive() { tmux has-session -t "ptyrc-rd-$1-$TAG" 2>/dev/null; }

# truth NAME [state]: the host screen equals the driver's host model, which equals
# app + sprites; cursor and pen state agree. Retried: animations move.
# `state` skips the text comparison (tmux prints line-drawing cells as the
# letters it was sent).
truth() {
    local name=$1-$TAG
    python3 - "$BIN" "$name" "${2:-rows}" <<'PY'
import json, subprocess, sys, time, unicodedata
binary, name, mode = sys.argv[1], sys.argv[2], sys.argv[3]
def untab(line):
    # tmux gives back the tabs it was sent; the model holds the blanks.
    out, col = [], 0
    for ch in line:
        if ch == "\t":
            pad = 8 - col % 8
            out.append(" " * pad); col += pad
        else:
            out.append(ch)
            if unicodedata.combining(ch) or unicodedata.category(ch) in ("Mn", "Me", "Cf"):
                continue
            col += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
    return "".join(out)
def once():
    dump = json.loads(subprocess.run(
        [binary, "-s", f"rd-{name}", "--json", "call", "debug.dump"],
        capture_output=True, text=True, check=True).stdout)
    cap = subprocess.run(
        ["tmux", "capture-pane", "-p", "-t", f"ptyrc-rd-{name}"],
        capture_output=True, text=True, check=True).stdout.split("\n")
    cur = subprocess.run(
        ["tmux", "display-message", "-p", "-t", f"ptyrc-rd-{name}",
         "#{cursor_x},#{cursor_y}"],
        capture_output=True, text=True, check=True).stdout.strip()
    rows = dump["size"][1]
    cap = [untab(l).rstrip(" ") for l in cap[:rows]]
    cap += [""] * (rows - len(cap))
    problems = []
    if not dump["in_ground"]:
        return ["app is mid-sequence"]
    if not dump["same_state"]:
        problems.append("host state differs from app state")
    if not dump["host_is_desired"]:
        problems.append("host model differs from app + sprites")
    for y, (model, real) in enumerate(zip(dump["host"], cap)):
        if mode == "rows" and model.rstrip(" ") != real:
            problems.append(f"row {y}: model {model!r} host {real!r}")
            if len(problems) > 6:
                break
    hc = dump["host_cursor"]
    x = min(hc["x"], dump["size"][0] - 1)
    if cur != f"{x},{hc['y']}":
        problems.append(f"cursor: model {x},{hc['y']} host {cur}")
    return problems
def cells(line):
    """(glyph, fg, bg, attrs) per glyph of a line holding SGR sequences."""
    out, fg, bg, attrs, i, col = [], None, None, frozenset(), 0, 0
    names = {1: "bold", 2: "dim", 3: "italic", 4: "underline", 5: "blink",
             7: "inverse", 8: "hidden", 9: "strike"}
    off = {22: ("bold", "dim"), 23: ("italic",), 24: ("underline",), 25: ("blink",),
           27: ("inverse",), 28: ("hidden",), 29: ("strike",)}
    while i < len(line):
        ch = line[i]
        if ch == "\x1b" and line[i + 1:i + 2] == "[":
            j = i + 2
            while j < len(line) and not line[j].isalpha():
                j += 1
            if line[j:j + 1] == "m":
                ps = [int(x.split(":")[0] or 0) for x in line[i + 2:j].split(";")] or [0]
                k = 0
                while k < len(ps):
                    n = ps[k]
                    if n == 0: fg, bg, attrs = None, None, frozenset()
                    elif n in names: attrs = attrs | {names[n]}
                    elif n in off: attrs = attrs - set(off[n])
                    elif 30 <= n <= 37: fg = n - 30
                    elif 90 <= n <= 97: fg = n - 90 + 8
                    elif 40 <= n <= 47: bg = n - 40
                    elif 100 <= n <= 107: bg = n - 100 + 8
                    elif n == 39: fg = None
                    elif n == 49: bg = None
                    elif n in (38, 48, 58) and k + 1 < len(ps):
                        if ps[k + 1] == 5:
                            val, k = ps[k + 2], k + 2
                        else:
                            val, k = tuple(ps[k + 2:k + 5]), k + 4
                        if n == 38: fg = val
                        elif n == 48: bg = val
                    k += 1
            i = j + 1
            continue
        if ch == "\x1b":       # charset designation and the like
            i += 3 if line[i + 1:i + 2] in "()" else 2
            continue
        if ch in "\x0e\x0f":
            i += 1
            continue
        if ch == "\t":
            pad = 8 - col % 8
            out += [(" ", fg, bg, attrs)] * pad
            col += pad
        else:
            out.append((ch, fg, bg, attrs))
            if not (unicodedata.combining(ch) or unicodedata.category(ch) in ("Mn", "Me", "Cf")):
                col += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
        i += 1
    # Trailing default blanks carry nothing.
    while out and out[-1][0] == " " and out[-1][2] is None and "inverse" not in out[-1][3]:
        out.pop()
    # The colour of a blank that shows no ink is not observable.
    return [(c, None if c == " " and not (a & {"underline", "strike", "inverse"}) else f, b,
             a - {"bold", "dim", "italic", "blink"} if c == " " else a) for c, f, b, a in out]
def styles(dump):
    cap = subprocess.run(
        ["tmux", "capture-pane", "-p", "-e", "-N", "-t", f"ptyrc-rd-{name}"],
        capture_output=True, text=True, check=True).stdout.split("\n")
    out = []
    for y, model in enumerate(dump["host_ansi"]):
        real = cap[y] if y < len(cap) else ""
        if "\x0e" in real or "\x1b(0" in real:
            continue
        a, b = cells(model), cells(real)
        if a != b:
            x = next((i for i, (p, q) in enumerate(zip(a, b)) if p != q), min(len(a), len(b)))
            out.append(f"style row {y} col~{x}: model {a[x:x+1]} host {b[x:x+1]}")
            if len(out) > 4:
                break
    return out
_once = once
def once():
    problems = _once()
    if not problems and mode == "rows":
        dump = json.loads(subprocess.run(
            [binary, "-s", f"rd-{name}", "--json", "call", "debug.dump"],
            capture_output=True, text=True, check=True).stdout)
        problems = styles(dump)
    return problems
problems = []
for attempt in range(8):
    try:
        problems = once()
    except subprocess.CalledProcessError as exc:
        problems = [f"{exc.cmd[0]}: {exc.stderr.strip()}"]
    if not problems:
        sys.exit(0)
    time.sleep(0.15)
print("\n".join("    " + p for p in problems))
sys.exit(1)
PY
}

# scrollback NAME FORBIDDEN...: no forbidden text anywhere in the host
# history.
scrollback_clean() {
    local name=$1-$TAG; shift
    local all
    all=$(tmux capture-pane -p -S - -E -1 -t "ptyrc-rd-$name")
    local bad
    for bad in "$@"; do
        if grep -qF -- "$bad" <<<"$all"; then
            echo "    scrollback holds sprite text: $bad"
            return 1
        fi
    done
}

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1${2:+: $2}"; FAILED=1; }
skip() { echo "SKIP $1: $2"; }
need() { command -v "$1" >/dev/null || [ -x "$1" ]; }

# Sprites of every kind on the raw frame.
sprites() {
    local n=$1
    p "$n" draw text --layer front --at -1,0 --bg blue --fg white ' 12:34 ' >/dev/null
    p "$n" draw text --layer front --at 30,3 --fg red 'STATIC' 'SPRITE' >/dev/null
    # Behind the app's text: the default. With a background (a sandwich),
    # farther back, and wide glyphs that need two free cells.
    p "$n" draw text --at 20,6 --fg green 'behindbehindbehindbehind' >/dev/null
    p "$n" draw text --at 0,9 --fg cyan --bg 17 --opaque '~~~~~~~~ ~~~~~~~~ ~~~~~~~~ ~~~~~~~~' >/dev/null
    p "$n" draw text --layer -5 --at 4,7 --bg 22 --opaque '          ' '          ' '          ' >/dev/null
    p "$n" draw text --at 12,4 --fg magenta '背景背景背景背景背景' >/dev/null
    p "$n" draw text --at 10,8 --blend tint --bg 52 'xxxxxxxxxxxxxxxx' >/dev/null
    p "$n" draw text --layer front --auto bottom-right --bg magenta ' auto ' >/dev/null
    p "$n" draw text --layer front --at 41,5 '界面' --fg yellow >/dev/null
    # Opacity: a glaze behind the text, glass and a faded word in front.
    p "$n" draw text --layer wallpaper --at 0,2 --bg '#804000' --opaque --alpha 0.4 '                    ' '                    ' >/dev/null
    p "$n" draw text --layer hud --at 6,5 --bg '#ffffff' --opaque --alpha 0.3 '            ' >/dev/null
    p "$n" draw text --layer hud --at 22,2 --fg '#ff8800' --alpha 0.5 'faded' >/dev/null
    p "$n" call draw.put '{"anchor":{"at":[50,2],"space":"view"},"persist":true,
        "layer":"front",
        "frames":[{"rows":["(o )"],"ms":60},{"rows":["( o)"],"ms":60}],"loop":true}' >/dev/null
}

mkfiles() {
    python3 - "$WORK" <<'PY'
import random, sys
random.seed(7)
words = "the quick brown fox jumps over lazy dog garden river stone cloud winter flower".split()
wide = ["世界", "日本語", "😀", "🌼", "é", "ạ̈", "한글", "ｆｕｌｌ"]
with open(sys.argv[1] + "/long.txt", "w") as f:
    for i in range(1, 401):
        parts = [random.choice(words) for _ in range(random.randint(2, 9))]
        if i % 5 == 0:
            parts.insert(random.randint(0, len(parts)), random.choice(wide))
        if i % 7 == 0:
            parts.insert(1, "\tTAB\t")
        if i % 31 == 0:
            parts *= 6
        f.write(f"line {i:03d}: " + " ".join(parts) + "\n")
PY
}

s_vim() {
    need /usr/bin/vim || { skip vim "no /usr/bin/vim"; return; }
    host vim 90 28 /usr/bin/vim -u NONE -N "$WORK/long.txt" || { fail vim start; return; }
    settle vim; sprites vim
    p vim draw text --near flower --fg red '<flower' >/dev/null
    sleep 1.8
    p vim draw text -f largest --space frame --at 30,12 --bg cyan 'FRAME' >/dev/null
    truth vim || { fail vim "initial"; return; }
    local k
    for k in "ctrl+e" "ctrl+y" "ctrl+d" "ctrl+u" "ctrl+f" "ctrl+b"; do
        local i; for i in $(seq 12); do keys vim "$k"; done
        truth vim || { fail vim "after $k x12"; return; }
    done
    keys vim G; truth vim || { fail vim "G"; return; }
    keys vim g g; truth vim || { fail vim "gg"; return; }
    keys vim ":set nu" enter; settle vim; truth vim || { fail vim "set nu"; return; }
    keys vim 3 j i "typing over sprites " "typing over sprites " "typing over sprites " esc
    truth vim || { fail vim "insert"; return; }
    keys vim ":set nowrap" enter 2 0 z l; truth vim || { fail vim "nowrap zl"; return; }
    keys vim ":set wrap" enter 3 1 G z t; truth vim || { fail vim "wrapped lines"; return; }
    keys vim ":vsplit" enter ":split" enter; settle vim 300; truth vim || { fail vim "splits"; return; }
    keys vim ":q!" enter ":q!" enter ":q!" enter
    sleep 0.3
    alive vim && { fail vim "did not exit"; return; }
    pass vim
}

s_shell() {
    host shell 80 24 /bin/sh || { fail shell start; return; }
    settle shell
    p shell draw text --at -1,0 --bg blue --fg white ' CLOCKSPRITE ' >/dev/null
    p shell draw text --at 30,10 --fg red 'MIDSPRITE' >/dev/null
    truth shell || { fail shell "initial"; return; }
    keys shell "seq 1 20000" enter; settle shell 300
    truth shell || { fail shell "seq"; return; }
    keys shell "yes abcdefghijklmnopqrstuvwxyz0123456789 | head -50000" enter; settle shell 300
    truth shell || { fail shell "yes"; return; }
    keys shell "cat $WORK/long.txt $WORK/long.txt" enter; settle shell 300
    truth shell || { fail shell "cat"; return; }
    scrollback_clean shell CLOCKSPRITE MIDSPRITE || { fail shell "scrollback"; return; }
    # No hole: the tail of seq is contiguous in the history.
    keys shell "clear; seq 1 3000" enter; settle shell 300
    local missing
    missing=$(tmux capture-pane -p -S - -E -1 -t "ptyrc-rd-shell-$TAG" | python3 -c '
import sys
seen = [l.strip() for l in sys.stdin]
nums = [l for l in seen if l.isdigit()]
ok = len(nums) > 2900 and nums == [str(i) for i in range(1, len(nums) + 1)]
print(0 if ok else "some")')
    [ "$missing" = 0 ] || { fail shell "$missing lines of seq missing from the history"; return; }
    # Alternate screen in and out with sprites up.
    if need less; then
        keys shell "less $WORK/long.txt" enter; settle shell; truth shell || { fail shell "less"; return; }
        keys shell space space G; truth shell || { fail shell "less G"; return; }
        keys shell q; settle shell; truth shell || { fail shell "after less"; return; }
    fi
    if need man; then
        keys shell "man sh" enter; settle shell 400; keys shell space
        truth shell || { fail shell "man"; return; }
        keys shell q; settle shell
    fi
    if need htop; then
        keys shell "htop" enter; sleep 1.2
        # htop redraws by itself: only the state invariants.
        truth shell state >/dev/null; keys shell q; settle shell 300
        truth shell || { fail shell "after htop"; return; }
    fi
    # Garbage, then recovery. CAN and ST first: tmux, the host here, stays
    # inside a string that garbage opened until one of them.
    keys shell "head -c 300000 /dev/urandom; printf '\\030\\033\\\\\\\\'; reset; clear" enter
    settle shell 600
    p shell call session.info >/dev/null || { fail shell "driver died on garbage"; return; }
    keys shell "echo recovered" enter; settle shell 300
    truth shell || { fail shell "after garbage and reset"; return; }
    keys shell "exit" enter; sleep 0.3
    alive shell && { fail shell "did not exit"; return; }
    pass shell
}

s_tmux() {
    need /usr/bin/vim || { skip tmux "no /usr/bin/vim"; return; }
    host tmux 100 30 tmux -L "ptyrc-rd-inner-$TAG" -f /dev/null new-session /bin/sh \
        || { fail tmux start; return; }
    settle tmux 300; sprites tmux
    keys tmux "ctrl+b" "%"; settle tmux 300
    keys tmux "ctrl+b" '"'; settle tmux 300
    keys tmux "/usr/bin/vim -u NONE -N $WORK/long.txt" enter; settle tmux 400
    truth tmux || { fail tmux "panes"; return; }
    local i; for i in $(seq 15); do keys tmux ctrl+d; done
    truth tmux || { fail tmux "vim scroll in pane"; return; }
    keys tmux "ctrl+b" o; keys tmux "seq 1 5000" enter; settle tmux 300
    truth tmux || { fail tmux "scroll region"; return; }
    keys tmux "ctrl+b" c; settle tmux 300; truth tmux || { fail tmux "new window"; return; }
    keys tmux "ctrl+b" p; settle tmux 300; truth tmux || { fail tmux "window back"; return; }
    pass tmux
}

s_resize() {
    host resize 80 24 /bin/sh || { fail resize start; return; }
    settle resize; sprites resize
    keys resize "seq 1 200000" enter
    local size
    for size in "100 30" "60 20" "120 40" "40 10" "80 24" "20 5" "90 26"; do
        set -- $size
        tmux resize-window -t "ptyrc-rd-resize-$TAG" -x "$1" -y "$2"
        sleep 0.12
    done
    keys resize ctrl+c; settle resize 300
    p resize call session.info >/dev/null || { fail resize "driver died"; return; }
    keys resize "clear" enter; settle resize 300
    truth resize || { fail resize "after storm"; return; }
    for size in "300 100" "10 3" "2 2" "80 24"; do
        set -- $size
        tmux resize-window -t "ptyrc-rd-resize-$TAG" -x "$1" -y "$2"
        sleep 0.25
        p resize call debug.dump >/dev/null || { fail resize "driver died at $1x$2"; return; }
    done
    keys resize "clear" enter; settle resize 300
    truth resize || { fail resize "after extreme sizes"; return; }
    pass resize
}

s_exit() {
    # The app leaves with sprites up: nothing of them stays on the host.
    host exit 80 24 /bin/sh -c "'echo APPLINE; sleep 1.2; echo BYE'" || { fail exit start; return; }
    p exit draw text --at 20,5 --bg red 'LEFTOVER' >/dev/null
    tmux set-option -t "ptyrc-rd-exit-$TAG" remain-on-exit on >/dev/null
    sleep 1.8
    local shown
    shown=$(tmux capture-pane -p -t "ptyrc-rd-exit-$TAG")
    grep -q LEFTOVER <<<"$shown" && { fail exit "sprite left on the host"; return; }
    grep -q BYE <<<"$shown" || { fail exit "app output lost"; return; }
    # SIGTERM to the driver: host tty back to cooked mode, screen clean.
    SESSIONS+=("term-$TAG")
    tmux new-session -d -s "ptyrc-rd-term-$TAG" -x 80 -y 24 \
        "$BIN run --name rd-term-$TAG -- /bin/sh; stty -a | grep -c -- ' icanon' > $WORK/stty; sleep 5"
    sleep 0.4
    p term draw text --at 20,5 --bg red 'LEFTOVER' >/dev/null
    sleep 0.2
    local pid
    pid=$(p term --json call session.info | python3 -c 'import json,sys; print(json.load(sys.stdin)["driver_pid"])')
    kill -TERM "$pid"; sleep 0.8
    grep -q LEFTOVER <<<"$(tmux capture-pane -p -t "ptyrc-rd-term-$TAG")" \
        && { fail exit "sprite left after SIGTERM"; return; }
    [ "$(cat "$WORK/stty" 2>/dev/null)" = 1 ] || { fail exit "termios not restored after SIGTERM"; return; }
    pass exit
}

s_client() {
    # A client that dies mid-animation takes its sprites along.
    host client 80 24 /bin/sh || { fail client start; return; }
    settle client
    python3 - "rd-client-$TAG" <<'PY' &
import os, socket, sys, time
base = os.environ.get("XDG_RUNTIME_DIR")
base = f"{base}/ptyrc" if base else f"/tmp/ptyrc-{os.getuid()}"
s = socket.socket(socket.AF_UNIX)
s.connect(f"{base}/{sys.argv[1]}.sock")
s.sendall(b'{"id":1,"method":"draw.put","params":{"effect":{"name":"confetti","params":{"duration_ms":60000}}}}\n'
          b'{"id":2,"method":"draw.put","params":{"anchor":{"at":[5,5],"space":"view"},"content":{"rows":["DYING"]}}}\n')
time.sleep(0.8)
PY
    sleep 0.4
    grep -q DYING <<<"$(tmux capture-pane -p -t "ptyrc-rd-client-$TAG")" \
        || { fail client "sprite never shown"; wait; return; }
    truth client state || { fail client "mid-animation"; wait; return; }
    wait
    sleep 0.3
    local n
    n=$(p client --json call draw.list | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["sprites"]))')
    [ "$n" = 0 ] || { fail client "$n sprites survive their client"; return; }
    truth client || { fail client "screen after disconnect"; return; }
    grep -q DYING <<<"$(tmux capture-pane -p -t "ptyrc-rd-client-$TAG")" \
        && { fail client "sprite text left on the host"; return; }
    pass client
}

s_cursor() {
    # Sprites on and around the cursor; a sprite that follows it.
    host cursor 80 24 /bin/sh || { fail cursor start; return; }
    settle cursor
    p cursor draw text --cursor 0,0 --fg red 'X' >/dev/null
    p cursor draw text --cursor 1,-1 --bg blue 'tip' >/dev/null
    p cursor draw text --at 0,0 --bg green '                    ' --opaque >/dev/null
    truth cursor || { fail cursor "initial"; return; }
    local i
    for i in $(seq 30); do keys cursor "word$i "; done
    truth cursor || { fail cursor "typing"; return; }
    keys cursor ctrl+u "printf 'a%.0s' \$(seq 1 80)" enter; settle cursor
    truth cursor || { fail cursor "pending wrap"; return; }
    keys cursor "printf '\\033[5;10r\\033[?6h\\033[2;3Horigin'; sleep 0.5; printf '\\033[?6l\\033[r'" enter
    sleep 0.25; truth cursor || { fail cursor "origin mode"; return; }
    settle cursor 400
    keys cursor "printf '\\033(0lqqk\\033[4h'; sleep 0.5; printf '\\033[4l\\033(B\\n'" enter
    sleep 0.25; truth cursor state || { fail cursor "charset and insert mode"; return; }
    settle cursor 400; keys cursor clear enter; settle cursor; truth cursor || { fail cursor "final"; return; }
    pass cursor
}

s_effects() {
    # A native effect over a scrolling editor: the pile of the sand
    # rests on the text, behind it, and scrolls with it; a running
    # effect takes new parameters in place.
    need /usr/bin/vim || { skip effects "no vim"; return; }
    python3 - > "$WORK/shelves.txt" <<'PY'
for i in range(60):
    print(f"{i:02d} " + ("" if i % 6 else "        ================================="))
PY
    host effects 60 24 /usr/bin/vim -u NONE -N "$WORK/shelves.txt" || { fail effects start; return; }
    settle effects 1700
    local id
    id=$(p effects draw effect -f largest sand '{"rate": 300, "seed": 4}') || { fail effects "sand refused"; return; }
    sleep 1.5
    truth effects state || { fail effects "pouring"; return; }
    p effects draw effect --id "$id" sand '{"rate": 0}' >/dev/null || { fail effects "not tuned"; return; }
    p effects draw effect --id "$id" sand '{"rate": "much"}' >/dev/null 2>&1 && { fail effects "bad parameter taken"; return; }
    sleep 1.5
    truth effects || { fail effects "at rest"; return; }
    # Braille cells of the host, as "row:col".
    grains() {
        tmux capture-pane -p -t "ptyrc-rd-effects-$TAG" | python3 -c '
import sys
shift = int(sys.argv[1])
for y, line in enumerate(sys.stdin.read().split("\n")[:22]):
    for x, ch in enumerate(line):
        if "⠀" < ch <= "⣿" and 0 <= y - shift < 4:
            print(f"{y - shift}:{x}")' "$1" | sort
    }
    # Rows 0 to 3 after the scroll are rows 3 to 6 before it: the pile
    # that rests on the text of row 6.
    local before after
    before=$(grains 3)
    [ "$(wc -l <<<"$before")" -ge 8 ] || { fail effects "no pile on the text: $before"; return; }
    keys effects ctrl+e ctrl+e ctrl+e
    sleep 0.3
    after=$(grains 0)
    [ "$before" = "$after" ] || { fail effects "the pile did not follow the text"; return; }
    truth effects || { fail effects "scrolled"; return; }
    keys effects ctrl+y ctrl+y ctrl+y
    sleep 0.3
    [ "$(grains 3)" = "$before" ] || { fail effects "the pile did not come back"; return; }
    # The text is in front, whole.
    grep -q '06         =================================' \
        <<<"$(tmux capture-pane -p -t "ptyrc-rd-effects-$TAG")" || { fail effects "text hidden"; return; }
    truth effects || { fail effects "back"; return; }
    pass effects
}

mkfiles
ALL="vim shell tmux resize exit client cursor effects"
for scenario in ${*:-$ALL}; do
    "s_$scenario"
done
exit $FAILED
