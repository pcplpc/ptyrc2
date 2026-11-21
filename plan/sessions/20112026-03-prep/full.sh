#!/bin/sh
# The full run of the handover. Output: temp/$1.txt
cd /home/peon/git/perso/ptyrc/ptyrc2 || exit 1
out=temp/${1:-full}.txt
{
git rev-parse --short HEAD; date; uptime
echo "== interp"; sdk/js/interp/build.sh 2>&1 | tail -3
echo "== build"; cargo build --workspace 2>&1 | tail -3; cargo build --release -p ptyrc-cli -p ptyrc-box 2>&1 | tail -3
echo "== test"; PTYRC_REQUIRE_JS=1 PTYRC_REQUIRE_RUSTC=1 cargo test --workspace --no-fail-fast 2>&1 | grep -E "^test result|FAILED|failed|panicked|^error|Running|ignored" 
echo "== clippy"; cargo clippy --workspace --all-targets 2>&1 | grep -E "^(warning|error)" | sort | uniq -c
echo "== pytest"; uv run pytest -q 2>&1 | tail -15
echo "== sdk"; (cd sdk/rust/tests && cargo test 2>&1 | grep -E "^test result|FAILED"); (cd sdk/rust/ptyrc-app && cargo test 2>&1 | grep -E "^test result|FAILED")
echo "== examples"; for t in sdk/*/examples/*/test.sh; do sh "$t" >/dev/null 2>&1 && echo "ok $t" || echo "FAIL $t"; done
date; uptime
} > "$out" 2>&1
