#!/bin/sh
# Timing of the apps timed under load or skipped: release ptyrc-box, load under 4.
cd /home/peon/git/perso/ptyrc/ptyrc2/plugins/store-apps || exit 1
export PTYRC_BOX=/home/peon/git/perso/ptyrc/ptyrc2/target/release/ptyrc-box
P=/home/peon/git/perso/ptyrc/ptyrc2/target/release/ptyrc
BIG=/home/peon/git/perso/ptyrc/ptyrc2/plan/sessions/22112026-01-migration/games-snake/big.screen
quiet() { until [ "$(cut -d. -f1 /proc/loadavg)" -lt 4 ]; do sleep 5; done; }
t() { # NAME SIZE ARGS...
    n=$1; s=$2; shift 2; quiet
    r=$($P dev test "$n/" --size "$s" --for 10s --timing "$@" 2>/dev/null | grep -E "^ticks:|^slow" | tail -3 | tr '\n' ' ')
    echo "$n $s [$*] load $(cut -d' ' -f1 /proc/loadavg): $r"
}
t flower 80x24 --screen flower/tests/garden.screen -- -a --count 0
t flower 200x60 --screen "$BIG" -- -a --count 0
t fire 80x24 -- -a
t fire 200x60 -- -a
t marks 80x24 --screen marks/tests/fruit.screen -- apple -i -e "p[ea]+r"
t marks 200x60 --screen "$BIG" -- dog lazy -i -e "o[a-z]+" -e "q[a-z]+"
t fishes 80x24 --screen fishes/tests/fox.screen -- -a
t fishes 200x60 --screen "$BIG" -- -a
t stars 80x24 --screen stars/tests/words.screen -- -a
t stars 200x60 --screen "$BIG" -- -a
t gallery 80x24 -- -a --every 3
t gallery 200x60 -- -a --every 3
t monitor 80x24 --probe system=monitor/tests/full.json
t monitor 200x60 --probe system=monitor/tests/full.json
