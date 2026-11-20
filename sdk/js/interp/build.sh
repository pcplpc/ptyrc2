#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The script interpreter: QuickJS-ng built to a guest of ABI 1.
# Needs clang and wasm-ld (LLVM 19 or later: `#embed`), curl, tar,
# sha256sum. The three inputs are fetched into target/cache/ when
# absent and checked against the hashes below.
#
#   sdk/js/interp/build.sh        ->  sdk/js/interp/target/ptyrc-js.wasm
set -eu
cd "$(dirname "$0")"

QJS=quickjs-ng-0.18.0.tar.gz
QJS_URL=https://github.com/quickjs-ng/quickjs/archive/refs/tags/v0.18.0.tar.gz
QJS_SHA=28bcd89aa3683fd2a946537e5ce3f11cc46b5cbf3383c7561aaef3df5ff9f1dd
SYS=wasi-sysroot-34.0.tar.gz
SYS_URL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/$SYS
SYS_SHA=9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be
RT=libclang_rt-34.0.tar.gz
RT_URL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/$RT
RT_SHA=eee3e634dcf71aa22b1333391623cf5c9965a637dc428a27b1a858c026c587f1

CACHE=target/cache
OUT=target/obj
mkdir -p "$CACHE" "$OUT"

# fetch FILE URL SHA256: into the cache, checked, unpacked once.
fetch() {
  if ! [ -f "$CACHE/$1" ]; then
    curl -sSfL -o "$CACHE/$1.part" "$2"
    mv "$CACHE/$1.part" "$CACHE/$1"
  fi
  echo "$3  $CACHE/$1" | sha256sum -c --quiet - || {
    echo "build.sh: $CACHE/$1: not the pinned file; remove it" >&2
    exit 1
  }
  dir=$CACHE/${1%.tar.gz}
  if ! [ -d "$dir" ]; then
    mkdir -p "$dir.part"
    tar -xzf "$CACHE/$1" -C "$dir.part" --strip-components=1
    mv "$dir.part" "$dir"
  fi
}
fetch "$QJS" "$QJS_URL" "$QJS_SHA"
fetch "$SYS" "$SYS_URL" "$SYS_SHA"
fetch "$RT" "$RT_URL" "$RT_SHA"

Q=$CACHE/quickjs-ng-0.18.0
ROOT=$CACHE/wasi-sysroot-34.0
BUILTINS=$CACHE/libclang_rt-34.0/wasm32-unknown-wasip1/libclang_rt.builtins.a
CF="--target=wasm32-wasip1 --sysroot=$ROOT -O2 -DNDEBUG -D_GNU_SOURCE -fno-exceptions -I$Q"

pids=
for f in quickjs libregexp libunicode dtoa; do
  clang $CF -w -c "$Q/$f.c" -o "$OUT/$f.o" &
  pids="$pids $!"
done
clang $CF -std=gnu23 -Wall -Wextra -Wno-unused-parameter -c ptyrc.c -o "$OUT/ptyrc.o" &
for p in $pids $!; do wait "$p"; done
clang --target=wasm32-wasip1 --sysroot="$ROOT" -O2 -nostartfiles \
  -Wl,--no-entry -Wl,-z,stack-size=1048576 -Wl,--strip-all -Wl,--gc-sections \
  "$OUT"/quickjs.o "$OUT"/libregexp.o "$OUT"/libunicode.o "$OUT"/dtoa.o "$OUT"/ptyrc.o \
  -nodefaultlibs -lc "$BUILTINS" -o target/ptyrc-js.wasm
ls -l target/ptyrc-js.wasm
