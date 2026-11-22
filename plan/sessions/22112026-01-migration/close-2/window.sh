#!/bin/sh
# window -c -l -p --speed 600, the pointer on the sill: the cat hunts.
cd /home/peon/git/perso/ptyrc/ptyrc2/plugins/store-apps || exit 1
export PTYRC_BOX=/home/peon/git/perso/ptyrc/ptyrc2/target/release/ptyrc-box
P=/home/peon/git/perso/ptyrc/ptyrc2/target/release/ptyrc
PT="--pointer 2000:30,16 --pointer 2500:40,16 --pointer 3000:50,16 --pointer 4000:25,16 --pointer 6000:45,15 --pointer 8000:55,17"
until [ "$(cut -d. -f1 /proc/loadavg)" -lt 4 ]; do sleep 5; done
$P dev test window/ --for 12s --timing $PT "$@" -- -c -l -p --speed 600 2>&1 | grep -E "^ticks|^slow|dropped"
