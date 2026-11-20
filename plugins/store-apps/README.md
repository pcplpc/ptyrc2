# Shipped store apps

The apps of ptyrc as store apps, one tree each: `NAME/`. Origin
`ptyrc/NAME`, tier `shipped`; found without `ptyrc store add`, never
fetched, updated or removed by the store; asked by the pop-up as any
app, their grants in `permissions.toml` under `[app."ptyrc/NAME"]`.
Lookup and build: `rust/ptyrc-store/README.md`, Shipped.

The protocol: `sdk/template/PROTOCOL.md`. The script API:
`sdk/js/ptyrc.d.ts` (`ptyrc/deck` for widgets). Rust: `sdk/rust/ptyrc-app`.
Examples: `sdk/js/examples`, `sdk/rust/examples`. The models here:
`confetti` (sprites, effects, a show on its own clock, unit tests),
`sand` (an effect, the pointer relayed, a permission by option).

## A tree

```
NAME/
  ptyrc-app.toml   [app] id = "NAME"; random = [...]
  LICENSE          a copy of the repository's LICENSE.txt
  app.js           or Cargo.toml, src/lib.rs (rustc only, no crate)
  tests.js         unit tests: exports test_*
  tests/CASE.args  the words after `ptyrc dev test NAME/`
  tests/CASE.txt   its screens, written by PTYRC_BLESS=1
  tests/*.screen   files the cases name (--screen tests/x.screen)
```

- `LICENSE`: the store wants the GPL text in each tree; git stores the
  28 copies as one blob. The harness checks each is the repository's.
- A Rust app: `app.wasm` is built from the tree on demand in a
  checkout (`<cache>/ptyrc/shipped/NAME/`, at `ptyrc app`); a release
  installs the trees in `<data>/ptyrc/shipped/` with `app.wasm` beside
  each, built with the release. Nothing is fetched.

## A case

`tests/CASE.args`: words split on white space, `#` to the end of a
line a comment, no quoting (write JSON without spaces). Run in the
tree: paths are relative to it. An app option named as one of `dev
test` (`--size`, `--seed`, `--at`, ...) goes after `--`; before it,
the flag is `dev test`'s (`dev check` and `dev test` say so). Name the
Python test it carries in the first comment line.

The scene steps on its own clock, as the driver's: the screen at `T`
is the same whatever other times `--at` asks.

The user's hands as the driver hears them: `--pointer MS:X,Y`,
`--press MS:X,Y`, `--release MS:X,Y` (`--click`: both at once),
`--key`, `--focus`, `--paste`, `--resize`; screen cells, so that a
case names no sprite id. `--event` only for what no hand gives. An
effect that follows the pointer asks the driver (`"pointer": true`,
`"bomb": true` in its params, `sand`): no pad sprite, no `hover`,
`mouse` relayed by the app.

## The harness

```
cargo test -p ptyrc-cli --test store_apps           # every tree
PTYRC_STORE_APP=NAME cargo test -p ptyrc-cli --test store_apps
PTYRC_BLESS=1 ...                                   # rewrite CASE.txt
```

A start of the runner compiles the interpreter: some 7 s of processor
time in a debug build, 2 s in a release one. The cases run on
`target/release/ptyrc-box` when it was built after the debug one
(`cargo build --release -p ptyrc-box`; 2026-11-20, two trees: 13 s,
176 s of processor time, against 4 s, 49 s); `PTYRC_BOX` names
another (`ptyrc dev test` takes it, as the driver).

Per tree: `ptyrc dev check`; the id its directory; `LICENSE`; each
draw of `random` read by the options; each case against its `.txt`;
`dev test --unit` when `tests.js` exists; a Rust app's `cargo test`.
Needs `sdk/js/interp/build.sh` (else the scripts are skipped, or fail
with `PTYRC_REQUIRE_JS=1`). Bless only after reading the diff.

## Moving an app

1. Read the twin (`plugins/client-apps/python/src/ptyrc_apps/NAME.py`)
   and every test of it (`tests/test_NAME.py`, `test_apps.py`,
   `test_reads.py`). The table of `temp/.../design/readiness.md`: its
   tier, permissions, gaps.
2. The tier: a script, unless the app computes every cell of a
   frame-sized picture at each tick (then Rust).
3. The manifest: `surface` as small as it goes (`frame` for an
   effect on the pane, `widget` for a deck). `[permissions]` as low as
   they go: what a no gives is in `PROTOCOL.md`; an app that works
   lowered says so in its reason. A permission one option needs alone:
   `needs = [...]` on that option, never in the default set.
4. The options: every option of the twin, its short flag, choices,
   `[args]`. Not the generic ones (`-f/--frame`, `--front`, `--border`,
   `--prefer`): the launcher's. Not the test hooks (`--print`,
   `--speed`, `--at`): `dev test`. A bound the manifest cannot say
   (`--bursts 1..10`): checked in `init`, `exit(2)` with a log line.
5. The code: pure parts exported (`spot`, `options`, a `Show` with
   `put`, `heat`, `rnd` injected), so that `tests.js` drives them on a
   clock of its own. Session time is `Date.now()` and `tick(now)`.
6. The tests: each test of the twin carried over, as a unit test or a
   case; one that cannot be carried yet is named in your report with
   the gap that blocks it.
7. `random = [[...], ...]` in `[app]`: the draws of the menu's random
   app (`RANDOM` of `_catalog.py`); `[]`: never drawn; absent: drawn
   bare.
8. `ptyrc dev check NAME`, the harness green, `ptyrc dev run NAME` in a
   session once.

The native twin wins `ptyrc app NAME` while it exists (a note names
both); the store one is `ptyrc app ptyrc/NAME`.

## At the end of a wave (gap 30)

When every app of the wave is moved and its twin's tests are carried:
remove the twin (`ptyrc_apps/NAME.py`, its tests), its entries in
`_catalog.py` (`RANDOM`, `READS`, ...) and `tests/test_reads.py`, its
lines in `scripts/record-tapes.sh`, `demo-apps.sh`, `GALLERY.md` and
the README's commands. Root files are the owner's: ask.
