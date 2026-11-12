# SPDX-License-Identifier: GPL-3.0-or-later
# Helpers for the demos, on top of lib-e2e.sh. Source this file.
#
#   demo_app NAME ARGS...     start a client app on the session
#   demo_stop_apps            end them
#   demo_json VERB... EXPR    python expression over the JSON of a verb (`d`)
#   demo_until SECONDS CMD... retry CMD until it succeeds
#   demo_ok TEXT CMD...       one line of the self-test
#   demo_show                 attach the terminal to the demo, until it ends

. "$(dirname "${BASH_SOURCE[0]}")/lib-e2e.sh"
DEMO_PIDS=()
DEMO_FAILED=0

demo_setup() {
    cd "$E2E_ROOT" || exit 1
    [ -x "$PTYRC" ] || cargo build -q -p ptyrc-cli || exit 1
    [ -x .venv/bin/ptyrc-app ] || uv sync -q || exit 1
    mkdir -p temp
    trap 'demo_stop_apps; e2e_cleanup' EXIT
}

demo_app() {
    .venv/bin/ptyrc-app "$@" -s "$E2E_NAME" &
    DEMO_PIDS+=($!)
}

demo_stop_apps() {
    [ ${#DEMO_PIDS[@]} -gt 0 ] && kill "${DEMO_PIDS[@]}" 2>/dev/null
    wait 2>/dev/null
    DEMO_PIDS=()
}

demo_json() {
    local expr=${*: -1}
    e2e_p "${@:1:$#-1}" --json 2>/dev/null |
        python3 -c "import json,sys; d=json.load(sys.stdin); print($expr)"
}

demo_until() {
    local end=$((SECONDS + $1))
    shift
    until "$@" >/dev/null 2>&1; do
        [ $SECONDS -ge $end ] && return 1
        sleep 0.2
    done
}

demo_ok() {
    local text=$1
    shift
    if "$@" >/dev/null 2>&1; then
        echo "PASS $text"
    else
        echo "FAIL $text"
        DEMO_FAILED=1
    fi
}

demo_show() {
    if [ -n "${TMUX:-}" ]; then
        tmux switch-client -t "$E2E_HOST"
        while tmux has-session -t "$E2E_HOST" 2>/dev/null; do sleep 0.5; done
    else
        tmux attach -t "$E2E_HOST"
    fi
}
