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
`sand` (an effect, the pointer relayed, a permission by option),
`clock` (a deck: variants by size, the wall clock, a zone, a tick on
the second; code shared with other apps as modules of the tree).

## A tree

```
NAME/
  ptyrc-app.toml   [app] id = "NAME"; random = [...]
  LICENSE          a copy of the repository's LICENSE.txt
  app.js           or Cargo.toml, src/lib.rs (rustc only, no crate)
  tests.js         unit tests: exports test_*
  x.js, x.tests.js a module and its tests; tests.js re-exports them
  tests/CASE.args  the words after `ptyrc dev test NAME/`
  tests/CASE.txt   its report, written by `dev test --cases --bless`
  tests/*.screen   files the cases name (--screen tests/x.screen)
```

- `LICENSE`: the store wants the GPL text in each tree; git stores the
  28 copies as one blob. The harness checks each is the repository's.
- A module: code another app wants too (the twin's `_canvas`, `_x`,
  or a part of another app it imports) is a file of its own, importing
  `"ptyrc"` alone, so that it can go unchanged into the SDK or
  `_shared/js/`. Its tests beside it, `x.tests.js`; `dev test --unit`
  runs `tests.js` alone: `export { test_x } from "./x.tests.js"`.
  `clock`: `braille.js` (`_canvas.Braille`), `digits.js` (the fonts
  and layout `timer` takes from `clock`).
- A Rust app: `app.wasm` is built from the tree on demand in a
  checkout (`<cache>/ptyrc/shipped/NAME/`, at `ptyrc app`); a release
  installs the trees in `<data>/ptyrc/shipped/` with `app.wasm` beside
  each, built with the release. Nothing is fetched.

## A case

`tests/CASE.args`: words split on white space, `#` to the end of a
line a comment, no quoting (write JSON without spaces). Run in the
tree: paths are relative to it. An app option named as one of `dev
test` (`--size`, `--at`, ...) goes after `--`; before it,
the flag is `dev test`'s (`dev check` and `dev test` say so). Name the
Python test it carries in the first comment line.

```
ptyrc dev test NAME/ --cases [CASE...] [--bless]
```

Each case (those named alone), a line: `ok`, or `FAIL` and a diff
(`-` expected, `+` got, two lines around); status 1 when one fails.
`--bless` writes each `CASE.txt` from the run. The harness below runs
it; `sdk/template/test.sh` is the same loop in shell.

The scene steps on its own clock, as the driver's: the screen at `T`
is the same whatever other times `--at` asks.

A case passes only when the app ends with status 0: an option refused
in `init` (`exit(2)`) is a unit test of the function that refuses it
(`clock`: `options(args, host)`, `test_bad_zone`).

The wall clock: `--clock ISO` (default `2000-01-01T00:00:00Z`),
`--zone NAME` the machine's (default `UTC`), `--deny clock` the time
since the start. Widgets: none placed before 1000 ms, nor for 1 s
after a `--resize`; never on the cursor's row and the next (a blank
screen: rows 0 and 1); a larger variant after 2 s. Ask `--at` after
those.

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
PTYRC_BLESS=1 ...                                   # --cases --bless
```

A start of the runner compiles the interpreter: some 7 s of processor
time in a debug build, 2 s in a release one. The cases run on
`target/release/ptyrc-box` when it was built after the debug one
(`cargo build --release -p ptyrc-box`; 2026-11-20, two trees: 13 s,
176 s of processor time, against 4 s, 49 s); `PTYRC_BOX` names
another (`ptyrc dev test` takes it, as the driver).

Per tree: `ptyrc dev check`; the id its directory; `LICENSE`; each
draw of `random` read by the options; each case against its `.txt`
(`dev test --cases`); `dev test --unit` when `tests.js` exists; a Rust
app's `cargo test`.
Needs `sdk/js/interp/build.sh` (else the scripts are skipped, or fail
with `PTYRC_REQUIRE_JS=1`). Bless only after reading the diff.

## Moving an app

1. Read the twin (`plugins/client-apps/python/src/ptyrc_apps/NAME.py`)
   and every test of it (`tests/test_NAME.py`, `test_apps.py`,
   `test_reads.py` and its `OPTIONS`; `_catalog.py`: `RANDOM`, `ASKS`,
   `READS`), and what it imports. Its row in
   `temp/20112026-02-ready/audit/migration-plan.md` (c); its draft in
   `temp/20112026-02-ready/manifest/drafts/NAME/`.
2. The tier: a script, unless the app computes every cell of a
   frame-sized picture at each tick (then Rust).
3. The manifest: `surface` as small as it goes (`frame` for an
   effect on the pane, `widget` for a deck). `[permissions]` as low as
   they go: what a no gives is in `PROTOCOL.md`; an app that works
   lowered says so in its reason. A permission one option needs alone:
   `needs = [...]` on that option, never in the default set.
4. The options: every option of the twin, its short flag, choices,
   `[args]`. Not the generic ones (`-f/--frame`, `--front`, `--border`,
   `--prefer`, `--seed`): the launcher's. The twin's default frame
   (`_cli.parser(..., frame=)`; none given: `raw`) is `[app] frame`
   unless it is `active`; its default border and side (`_widgets.options`)
   go to `new Deck({border})`, which the launcher's flags override. Not
   the test hooks (`--print`, `--speed`, `--at`): `dev test`. An option
   the twin gave no default has none (`null` in `init`). `--no-X` of
   the twin: `X = { type = "bool", default = true }`. A bound or a rule
   the manifest cannot say (`--bursts 1..10`, two options exclusive):
   checked in `init`, `exit(2)` with a log line; live, `ptyrc app`
   says `running` and the line is in `ptyrc manage log ID` alone.
5. The code: pure parts exported (`spot`, `options`, a `Show` with
   `put`, `heat`, `rnd` injected; `clock`: the faces of a `civil` time,
   a `Wall` with `now`, `zone` injected), so that `tests.js` drives
   them on a clock of its own. Session time is `Date.now()` and
   `tick(now)`; the wall clock `now()`, `civil`, and `period(until(
   unixMs, 1000))` at each tick (live, 2026-11-20: a `period` asked in
   `tick` applies a tick late, `ptyrc-box/src/live.rs`; the ticks fall
   anywhere in the second; headless they are on it). A `Deck` cannot run in `--unit` (no
   session): keep `render(w, h)` a call of a pure face, test the faces,
   and the deck in cases. Colours are numbers: the twin's names
   `bright-cyan` 14, `bright-magenta` 13, `bright-yellow` 11,
   `bright-black` 8, `white` 7, `cyan` 6.
6. The tests: each test of the twin carried over, as a unit test or a
   case; one that cannot be carried yet is named in your report with
   the gap that blocks it. A test of `--print` carries as the cases of
   the faces; a live test of `test_apps.py` as a case; `test_reads.py`
   as a case with nothing `lowered` but what `--deny` asks.
7. `random = [[...], ...]` in `[app]`: the draws of the menu's random
   app (`RANDOM` of `_catalog.py`); `[]`: never drawn; absent: drawn
   bare.
8. `ptyrc dev check NAME`, the harness green, the app in a session
   once. Without a human, a driver of its own (`D` a directory of
   `temp/`, mode 700; `cargo build -p ptyrc-cli -p ptyrc-box -p
   ptyrc-server` first):

   ```
   export XDG_RUNTIME_DIR=$D XDG_CONFIG_HOME=$D/config XDG_DATA_HOME=$D/data
   export XDG_STATE_HOME=$D/state XDG_CACHE_HOME=$D/cache
   export PTYRC_STORE_APPS=$PWD/plugins/store-apps
   export PTYRC_BOX=$PWD/target/debug/ptyrc-box
   export PTYRC_JS=$PWD/sdk/js/interp/target/ptyrc-js.wasm
   setsid ptyrc run --name s --headless --size 80x24 -- cat </dev/null &
   ptyrc -s s app ptyrc/NAME ...        # or: ptyrc -s s dev run plugins/store-apps/NAME
   ptyrc -s s shot --ansi --sprites --no-cursor
   ptyrc -s s manage ls; ptyrc -s s manage log ID; ptyrc -s s manage stop ID
   ```

   Compare the shot with the case of the same size; then stop the
   driver (`kill` its pid).

The native twin wins `ptyrc app NAME` while it exists (a note names
both); the store one is `ptyrc app ptyrc/NAME`.

## At the end of a wave (gap 30)

When every app of the wave is moved and its twin's tests are carried:
remove the twin (`ptyrc_apps/NAME.py`, its tests), its entries in
`_catalog.py` (`RANDOM`, `READS`, ...) and `tests/test_reads.py`, its
lines in `scripts/record-tapes.sh`, `demo-apps.sh`, `GALLERY.md` and
the README's commands. Root files are the owner's: ask.
