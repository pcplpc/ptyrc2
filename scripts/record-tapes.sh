#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Record the tapes of rust/ptyrc-cli/tests/tapes/ and their snapshots.
#
#   scripts/record-tapes.sh            record less.tape and tmux.tape again
#   scripts/record-tapes.sh --snap     keep the tapes, write the snapshots
#
# A tape is a headless session driven by the keys below, one every
# 2.5 s. A snapshot is taken 0.1 s before each key, and 2.5 s after the
# last. Read the diff of a .snap before committing it: it is what the
# test (tests/tapes.rs) will hold the frames to.
set -eu
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PTYRC=${PTYRC_BIN:-$ROOT/target/debug/ptyrc}
OUT=$ROOT/rust/ptyrc-cli/tests/tapes
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

# record NAME SIZE KEYS -- CMD...: KEYS is a list of `ptyrc keys` lines.
record() {
    local name=$1 size=$2 keys=$3 session="tape-$1-$$" line
    shift 4
    (cd "$WORK" && env -i PATH="$PATH" TERM=xterm-256color HOME="$WORK" \
        SHELL=/bin/sh PS1='$ ' LESSHISTFILE=- XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
        "$PTYRC" run --name "$session" --headless --size "$size" \
        --record "$OUT/$name.tape" --record-input -- "$@") &
    until "$PTYRC" -s "$session" cursor >/dev/null 2>&1; do sleep 0.05; done
    while IFS= read -r line; do
        sleep 2.5
        # shellcheck disable=SC2086
        "$PTYRC" -s "$session" keys $line
    done <<<"$keys"
    wait
}

# The times of the snapshots: before each key, and after the last.
times() {
    python3 - "$1" <<'PY'
import json, sys
keys = [json.loads(l)["t"] for l in open(sys.argv[1]) if '"i"' in l[-80:] or '"ib"' in l[-80:]]
# Keys of one `ptyrc keys` call are one record or a few, close together.
marks = [t for i, t in enumerate(keys) if i == 0 or t - keys[i - 1] > 1_000_000]
at = [(t - 100_000) / 1e6 for t in marks[:-1]]
at.append(at[-1] + 2.5)
print(",".join(f"{t:.3f}" for t in at))
PY
}

if [ "${1:-}" != --snap ]; then
    seq 1 200 | sed 's/^/line number /' >"$WORK/numbers.txt"
    record less 60x16 'space
space
j j j
b
G
g
q' -- less numbers.txt

    cat >"$WORK/tmux.conf" <<'CONF'
set -g status-left '[tape] '
set -g status-right ''
set -g default-command 'PS1="$ " sh'
set -g escape-time 0
CONF
    record tmux 80x24 'seq space 1 space 40 enter
ctrl+b %
echo space right enter
ctrl+b c
echo space second enter
ctrl+b p
ctrl+b d' -- tmux -L "tape-$$" -f tmux.conf new
    tmux -L "tape-$$" kill-server 2>/dev/null || true
fi

for name in less tmux; do
    "$PTYRC" replay "$OUT/$name.tape" --snap --at "$(times "$OUT/$name.tape")" >"$OUT/$name.snap"
    echo "$name: $(wc -c <"$OUT/$name.tape") bytes, snapshots at $(times "$OUT/$name.tape")"
done
