# SPDX-License-Identifier: GPL-3.0-or-later
# Helpers for end-to-end checks: the driver runs inside a detached tmux,
# which stands for the host terminal. Source this file.
#
#   e2e_start TAG COLSxROWS CMD...   start; sets E2E_HOST (tmux session)
#                                    and E2E_NAME (ptyrc session)
#   e2e_host [-e]                    what the host shows
#   e2e_keys KEY...                  press keys in the app
#   e2e_send TEXT                    type text in the app
#   e2e_wait ARGS...                 `ptyrc wait` on the session
#   e2e_p VERB ARGS...               any client verb on the session
#   e2e_stop                         end the current session
#
#   E2E_RUN_ARGS                     words given to `ptyrc run` by e2e_start
#                                    (`E2E_RUN_ARGS=--no-presets`)
#
# Every session started is killed on exit; nothing else is touched.

E2E_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PTYRC=${PTYRC_BIN:-$E2E_ROOT/target/debug/ptyrc}
E2E_HOSTS=()
E2E_HOST=
E2E_NAME=

e2e_start() {
    local tag=$1 size=$2
    shift 2
    E2E_HOST="ptyrc-$tag-$$"
    E2E_NAME="$tag-$$"
    local cmd
    # shellcheck disable=SC2086
    printf -v cmd '%q ' "$PTYRC" run --name "$E2E_NAME" ${E2E_RUN_ARGS:-} -- "$@"
    tmux new-session -d -s "$E2E_HOST" -x "${size%x*}" -y "${size#*x}" "$cmd" || return 1
    E2E_HOSTS+=("$E2E_HOST")
    local i
    for i in $(seq 100); do
        "$PTYRC" -s "$E2E_NAME" cursor >/dev/null 2>&1 && return 0
        sleep 0.05
    done
    echo "e2e: $E2E_NAME did not start" >&2
    return 1
}

e2e_host() { tmux capture-pane -p "$@" -t "$E2E_HOST"; }
e2e_p() { "$PTYRC" -s "$E2E_NAME" "$@"; }
e2e_keys() { e2e_p keys "$@"; }
e2e_send() { e2e_p send "$*"; }
e2e_wait() { e2e_p wait "$@"; }

e2e_stop() {
    [ -n "$E2E_HOST" ] && tmux kill-session -t "$E2E_HOST" 2>/dev/null
    E2E_HOST=
}

e2e_cleanup() {
    local host
    for host in "${E2E_HOSTS[@]}"; do
        tmux kill-session -t "$host" 2>/dev/null
    done
}
trap e2e_cleanup EXIT
