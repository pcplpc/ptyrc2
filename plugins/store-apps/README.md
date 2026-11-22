# Shipped store apps

The apps of ptyrc as store apps, one tree each: `NAME/`. Origin
`ptyrc/NAME`, tier `shipped`; found without `ptyrc store add`, never
fetched, updated or removed by the store; asked by the pop-up as any
app, their grants in `permissions.toml` under `[app."ptyrc/NAME"]`.
Lookup and build: `rust/ptyrc-store/README.md`, Shipped.

The protocol: `sdk/template/PROTOCOL.md`. The script API:
`sdk/js/ptyrc.d.ts` (`ptyrc/deck` for widgets). Rust:
`sdk/rust/ptyrc-app/README.md`. Examples: `sdk/js/examples`,
`sdk/rust/examples`. The models here:

- `confetti`: sprites, effects, a show on its own clock, unit tests.
- `sand`: an effect, the pointer relayed, a permission by option.
- `clock`: a deck; variants by size, the wall clock, a zone, a tick on
  the second; code shared as modules of the tree; an option refused in
  `init`.
- `life`: Rust; a Braille canvas, a `Sight`, draws from the seed,
  permissions by option (`needs`), values refused by the manifest.

## A tree

```
NAME/
  ptyrc-app.toml   [app] id = "NAME"; random = [...]
  LICENSE.txt      the repository's, as `ptyrc dev new` writes it
  app.js           or Cargo.toml, Cargo.lock, src/lib.rs (A Rust app)
  tests.js         unit tests: exports test_*
  x.js, x.tests.js a module and its tests; tests.js re-exports them
  tests/CASE.args  the words after `ptyrc dev test NAME/`
  tests/CASE.txt   its report, written by `dev test --cases --bless`
  tests/*.screen   files the cases name (--screen tests/x.screen)
```

- `LICENSE.txt`: the store wants the GPL text in each tree; git stores
  the copies as one blob. The harness checks each is the repository's.
- A module: code another app wants too (the twin's `_x`, or a part of
  another app it imports) is a file of its own, importing the SDK's
  modules alone. Its tests beside it, `x.tests.js`; `dev test --unit`
  runs `tests.js` alone: `export { test_x } from "./x.tests.js"`. What
  any author would want is the SDK's (`"ptyrc/canvas"`: the twin's
  `_canvas`; `"ptyrc/sight"`: `_cli.Sight`).

## Shared code: `_shared/js/`, `_shared/rust/`

Code of a family of our apps (`_arcade`, `_reef`, `_sky`, `_strip`,
the fonts of `clock` and `timer`) has one source, copied into each
tree that uses it: each tree stays whole, the store and the runner
know nothing of it. `_shared` is not an app: no manifest, a name the
store refuses. `PTYRC_SYNC=1` (the harness) writes each copy from its
source.

- `_shared/js/x.js`: copied anywhere in a tree (`NAME/x.js`,
  `NAME/lib/x.js`); a file of a tree named as one of `_shared/js/` is
  its copy, checked byte for byte as `LICENSE.txt` is. Its tests,
  `_shared/js/x.tests.js`, are not copied: the harness runs them once.
  Tenants: `digits.js` (`clock`; `timer` next), `reef.js` (`fishes`,
  `tank`), `arcade.js`, `twister.js` (`snake`; `bricks` next;
  `twister.js`: Python's `random.Random`, the twin's draws for a seed).
- `_shared/rust/x.rs`: copied as `NAME/src/x.rs`, declared `mod x;`
  there; never `lib.rs`, `main.rs`. Its unit tests inside
  (`#[cfg(test)]`), run once by the harness, natively, with
  `ptyrc-app`. No tenant yet; the first: the grid of `stars` (`Grid`,
  `noise`, `runs`, `phase_of`), for `pond` and `shore`.

## A case

`tests/CASE.args`: words split on white space, `#` to the end of a
line a comment, no quoting (write JSON without spaces). Run in the
tree: paths are relative to it. The first comment line names the
Python test it carries. An app option named as one of `dev test`
(`--size`, `--at`, ...) goes after `--`; before it, the flag is `dev
test`'s (`dev check` and `dev test` say so).

```
ptyrc dev test NAME/ --cases [CASE...] [--bless]
```

Each case (those named alone), a line: `ok`, or `FAIL` and a diff
(`-` expected, `+` got, two lines around); status 1 when one fails.
`--bless` writes each `CASE.txt` from the run. The cases run at once;
a Rust app is built once before them. It is the canonical runner: the
harness runs it, a tree's `test.sh` calls it.

A run that ends with a status other than 0 fails its case, unless the
report says that status (`ended: exit: N`, `result: VALUE (status
N)`), and so `CASE.txt` holds the line: an option refused in `init`
(`exit(2)`) is a case (`clock/tests/zone`, `life/tests/speed`). A
value the manifest refuses (`choices`, `min`) stops `dev test` before
the app, status 2: the report is the launcher's line, `ptyrc: --NAME:
WHY` (`life/tests/kind`).

The scene steps on its own clock, as the driver's: the screen at `T`
is the same whatever other times `--at` asks.

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

A second run from the first one's storage: `ptyrc dev test .
--storage-out tests/stored` once, the directory kept in the tree; the
case `--storage tests/stored`. Not a case writing what another reads.

`--timing`, never in a case: the wall time of the ticks and events;
each one over `tick-ms` named (`slow at T ms: ...`), status 1. Run it
alone, on a quiet machine, with the release runner (A Rust app, Cost).

## The harness

```
cargo test -p ptyrc-cli --test store_apps           # every tree
PTYRC_STORE_APP=NAME cargo test -p ptyrc-cli --test store_apps
PTYRC_BLESS=1 ...                                   # --cases --bless
PTYRC_SYNC=1 ...                                    # copies of _shared written
```

Per tree: `ptyrc dev check`; the id its directory; `LICENSE.txt`; the
copies of `_shared/`; each draw of `random` read by the options; each
case against its `.txt` (`dev test --cases`); `dev test --unit` when
`tests.js` exists; a Rust app's `cargo test`. Needs
`sdk/js/interp/build.sh` (else the scripts are skipped, or fail with
`PTYRC_REQUIRE_JS=1`) and `rustc` with its wasm target (else the Rust
apps are skipped, or fail with `PTYRC_REQUIRE_RUSTC=1`). Bless only
after reading the diff.

A start of the runner compiles the interpreter: 7 to 12 s of
processor time in a debug build, 2 to 3.3 s in a release one, by the
load (2026-11-20). The cases run on `target/release/ptyrc-box` when it
was built after the debug one (`cargo build --release -p ptyrc-box`;
2026-11-20, two trees: 13 s, 176 s of processor time, against 4 s,
49 s); `PTYRC_BOX` names another (`ptyrc dev test` takes it, as the
driver).

## Moving an app

1. Read the twin (`plugins/client-apps/python/src/ptyrc_apps/NAME.py`)
   and every test of it (`tests/test_NAME.py`, `test_apps.py`,
   `test_reads.py` and its `OPTIONS`; `_catalog.py`: `RANDOM`, `ASKS`,
   `READS`), and what it imports. Its row in
   `plan/sessions/20112026-02-ready/audit/migration-plan.md` (c); its draft in
   `plan/sessions/20112026-02-ready/manifest/drafts/NAME/`.
2. The tier: a script, unless the app computes every cell of a
   frame-sized picture at each tick (then Rust).
3. The manifest: `surface` as small as it goes (`frame` for an
   effect on the pane, `widget` for a deck). `[permissions]` as low as
   they go: what a no gives is in `PROTOCOL.md`; an app that works
   lowered says so in its reason. A permission one option needs alone:
   `needs = [...]` on that option, never in the default set.
   `keys-arrows` is amber (unticked in the pop-up): asked so by the
   option that grabs (`snake --grab`, `bricks --grab`).
4. The options: every option of the twin, its short flag, choices,
   `[args]`. Not the launcher's (`ptyrc app NAME --help`, options of
   ptyrc: `--frame`, `--front`, `--back`, `--prefer`, `--alpha`,
   `--border`, `--seed`, `--wait`): no `seed` option. The twin's
   default frame (`_cli.parser(..., frame=)`; none given: `raw`) is
   `[app] frame` unless it is `active`; its default border and side
   (`_widgets.options`) go to `new Deck({border})`, which the
   launcher's flags override. Not the test hooks (`--print`,
   `--speed`, `--at`): `dev test`, but those Decided keeps. An option
   the twin gave no default has none (`null` in `init`). `--no-X` of
   the twin: `X = { type = "bool", default = true }`. A bound or a rule
   the manifest cannot say (`--bursts 1..10`, two options exclusive):
   checked in `init`, `exit(2)` with a log line; live, `ptyrc app`
   prints the line and exits 2.
5. The code: pure parts exported (`spot`, `options`, a `Show` with
   `put`, `heat`, `rnd` injected; `clock`: the faces of a `civil` time,
   a `Wall` with `now`, `zone` injected), so that `tests.js` drives
   them on a clock of its own. Session time is `Date.now()` and
   `tick(now)`; the wall clock `now()`, `civil`, and `period(until(
   unixMs, 1000))` at each tick: the ticks fall on the second, live
   and headless. A `Deck` cannot run in `--unit` (no session): keep
   `render(w, h)` a call of a pure face, test the faces, and the deck
   in cases. Colours are numbers: the twin's names `bright-cyan` 14,
   `bright-magenta` 13, `bright-yellow` 11, `bright-black` 8, `white`
   7, `cyan` 6.
   - The frame: told at `init`, `manifest().frame` (Rust:
     `Manifest::frame`): `--frame`, else `[app] frame`, else `active`,
     as the driver resolves it; 0 below `layout`, and headless. A
     `Sight` reads it unless one is named; `move` names the next. No
     frame option of the app's own.
   - `subscribe` adds topics, never removes one: a `Sight`, a `Deck`
     and the app subscribe side by side; the app gets each event once
     and passes it on.
   - A pointer event on a canvas names its sprite: compare `sprite` of
     `hover`, `mouse` with `canvas.sprite()` (Rust: `Canvas::sprite()`):
     `null` (`None`) before the first `present`; a new id once the
     driver removed it and `present` put it again.
6. The tests: each test of the twin carried over, as a unit test or a
   case; one that cannot be carried yet is named in your report with
   the gap that blocks it. A test of `--print` carries as the cases of
   the faces; a live test of `test_apps.py` as a case; `test_reads.py`
   as a case with nothing `lowered` but what `--deny` asks; a refusal
   (`init`'s or the manifest's) as a case (A case).
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

   With a host terminal (`ptyrc run` on a pty, no `--headless`) the
   pop-up asks the permissions declared, and `ptyrc app` waits on it.
   Answer beforehand, in the driver's own config directory, before
   `ptyrc run`:

   ```
   mkdir -p -m 700 $D/config/ptyrc
   (umask 077; printf '[app."ptyrc/NAME"]\nallow = ["shape", "pointer"]\n' \
     > $D/config/ptyrc/permissions.toml)
   ```

   A table per app, `[app."ORIGIN"]` (`ptyrc/NAME` shipped, the origin
   of a store app); `allow`, `deny`: arrays of permission names on one
   line (`rust/ptyrc-server/src/permissions.rs`). Never the user's own
   file. `--headless` asks nothing: what is declared is granted, less
   what the file denies. Tested 2026-11-20, `ptyrc/life` on a pty:
   with the file `ptyrc app` returns, granted `shape`, `pointer`;
   without, it waits on the pop-up.

## A Rust app

The model: `life`. Beside the lines above:

- `Cargo.toml`: `[workspace]` (the tree is no member of the
  repository's), `crate-type = ["cdylib"]`, `ptyrc-app = { path =
  "../../../sdk/rust/ptyrc-app" }`: for `cargo test` alone; the store
  builds `src/lib.rs` by `rustc` with its own copy. The tree's
  `Cargo.lock` committed.
- `[app] rustc = "1.88"`: the floor of every Rust store app.
- `app.wasm` is built from the tree on demand in a checkout
  (`<cache>/ptyrc/shipped/NAME/`, at `ptyrc app`); a release installs
  the trees in `<data>/ptyrc/shipped/` with `app.wasm` beside each,
  built with the release. Nothing is fetched.
- The SDK's modules, not the twin's: `ptyrc_app::canvas` (`Braille`,
  `Half`, `mix`, `xterm`: `_canvas`), `ptyrc_app::sight` (`Sight`:
  `_cli.Sight`), `ptyrc_app::dice` (`Dice`, `Host`, `Seeded`);
  `ptyrc_app::deck` for widgets, `.click` on a press and a wheel turn
  (`sdk/rust/ptyrc-app/README.md`, Drawing, Deck). Family code:
  `_shared/rust/`.
- Draws: the pure code takes `&mut impl Dice`; `init` seeds a `Seeded`
  from `Host` (the launcher's `--seed`), a unit test from a number.
- Time: steps counted from `now_ms` (`life`: generation `now_ms *
  speed / 1000`), never one a tick: the world does not hang on when
  the ticks fall.
- Cost: a tick measured on 200x60 with the release runner (`cargo
  build --release -p ptyrc-box`; `ptyrc dev test . --size 200x60 --for
  10s --timing`), against the 50 ms bound of `tick-ms`. `life`: 1.1 ms,
  3.6 ms with `-a`.

## Decided (`plan/interview.md`, 2026-11-20)

Beside what the steps above state (`needs`, `keys-arrows`, `--seed`):

- `ask`: no `--json`; `ptyrc --json app ask` prints the value as JSON;
  without it a string as it is, a list a line each.
- `fishes --tank` is an app of its own, `tank`; the reef in `_shared`.
- `marks` draws on its frame alone: `surface = "frame"`, no
  `screen-wide`. `timer -b`: `surface = "frame"`.
- Test hooks dropped and kept as `migration-plan.md` (c) lists:
  dropped `--speed`, `--at` of fire, globe, timer; `--speed` of
  gallery, pace; `--colour` of gallery, `--file` of minimap, `--seed`
  of pace, `--json` of ask, `--save` of pace. Kept, the app's:
  `--speed` of snake, bricks, life, train, window; `--at` of flock,
  shore, train, window (after `--` under `dev test`).
- `mix` rounds half up (the twin: half to even): a carried case that
  differs from the twin by one colour step is blessed.

The native twin wins `ptyrc app NAME` while it exists (a note names
both); the store one is `ptyrc app ptyrc/NAME`.

## At the end of a wave (gap 30)

When every app of the wave is moved and its twin's tests are carried:
remove the twin (`ptyrc_apps/NAME.py`, its tests), its entries in
`_catalog.py` (`RANDOM`, `READS`, ...) and `tests/test_reads.py`, its
lines in `scripts/record-tapes.sh`, `demo-apps.sh`, `GALLERY.md` and
the README's commands. Root files are the owner's: ask.
