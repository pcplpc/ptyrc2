# Migration readiness, 2026-11-20

Superseded for the per-app state by
`plan/sessions/20112026-02-ready/audit/migration-plan.md` (audit at `6bd169c`):
its rows, findings and waves hold where the two differ.

What each Python app of `plugins/client-apps/python/src/ptyrc_apps/`
needs to become a store app, against what a store app has today
(`sdk/template/PROTOCOL.md`, `rust/ptyrc-server/src/grants.rs`,
`rust/ptyrc-store/src/manifest.rs`, `rust/ptyrc-box/src/headless.rs`).
From the catalog (`ptyrc-app --catalog`), the imports and the requests
of each app; the long apps were not read line by line. Host functions:
`host-functions.md`.

## What a store app lacks today, in one paragraph

Every method and topic the apps use is open or lowered for a store app
(`grants.rs`), but for `app.result`-like ends, files, the wall clock
and the probes, which do not exist. The holes are elsewhere: the
manifest's options are four scalar types, long names only, no
positional, no permission tied to an option; the launcher parses none
of the generic options it reserves; no widget helper exists in either
SDK, and 14 of 28 apps are decks of widgets; headless answers no
`batch`, no pointer, no focus, no clock; scripts have no unit tests;
nothing says how a shipped store app is found and built.

## Per app

Tier: **S** script, **R** Rust. Rust where the app computes every cell
of a frame-sized picture at each tick (QuickJS: 12000 `set` in 8 ms;
a 200x60 screen is 12000 cells, a Braille one 96000 dots). A deck app
draws a widget of 200x60 at most, usually under 40x12: script.

`W`: a deck of widgets (`_widgets.Deck`). Generic options every app
parses itself today and must lose: `-f/--frame` (all); `--border`,
`--prefer`, `--front` (all `W`, and `cat`, `confetti`, `flower`,
`life`, `snake`). `--print WxH`, `--at`, `--speed`: test hooks,
replaced by `dev test --size`, `--at`, session time as an argument.

| app | tier | permissions (catalog) | protocol beyond `draw.*` | host functions | options the manifest cannot say | tests need | wave |
|---|---|---|---|---|---|---|---|
| `confetti` | S (exists) | `shape` (heat) | effects `confetti`, `sparkle` | | `-a` | | 1 |
| `rain` | S | `layout` | effect `rain`; topics `frames`, `move` | | `-t` | effects headless (ok) | 1 |
| `snow` | S | `layout`; `--hover`: `pointer` | effect `snow`, tuned; `hover` on a pad sprite | | `-a` | a pointer | 1 |
| `sand` | S (exists) | `layout`; `--bomb`: `clicks`, `clicks-grab` | effect `sand`; `hover`, `mouse` on a pad | | `-a` | a pointer, a click | 1 |
| `fire` | S, W | none | `widget.*` | | short flags, `-a` | | 1 |
| `globe` | S, W | none | `widget.*` | | short flags | | 1 |
| `flower` | S | `words` (`flower`), `front`; `-a`: `text` | `screen.find`, anchor on text | | `--word WORD`: the user's word; `-a` raises to `text` | `--screen` (ok) | 1 |
| `marks` | S | `text`, `front` | `screen.find {known}`, topics `screen`, `frames`, `draw` | | `PATTERN+` positional; `-e`, `-i` | `--screen` at several times (ok) | 1 |
| `fishes` | S | `shape`; `--tank`: none, W | `batch`; cursor | | `--tank` lowers the degree | `batch` headless; a cursor | 1 |
| `flock` | R | `shape`, `pointer` | `hover`, `batch` | | short flags | a pointer | 2 |
| `life` | R | `shape`; `--cursor`: `pointer` | `hover`, `mouse` on its own sprite | | `--kinds A,B` (a string), short flags | a click on its sprite | 2 |
| `shore` | R | `shape`; `-a`: `pointer` | `hover` | | | a pointer | 2 |
| `cat` | R | `words`; `--hunt`: `pointer` | topics `idle`, `cursor`, `screen`, `frames`, `hover`; `draw.list` (where its sprites are) | | | idle, cursor events (`--event`, ok) | 2 |
| `snake` | S (exists) | `shape`; `--grab`: `keys-arrows` | `input.grab` | | `--grab` asks only when given | | 2 |
| `bricks` | S | `shape`; `--grab`: `keys-arrows` | `input.grab`, `batch` | | as `snake` | 1060 lines of physics tests: unit tests in the app | 2 |
| `timer` | S, W | `--modal`: `keys-grab`, `front-widget`; `--until`: `clock` | `mouse` on its widget, modal, effect `confetti` | `clock` (`--until`) | `DURATION?` positional; `--pomodoro W/B` | a click on its widget; `--clock` | 2/3 |
| `clock` | S, W | `clock` | `widget.*`; ticks on the wall second | `clock`, a zone (`--tz`) | | `--clock`, `--zone` | 3 |
| `stars` | R | `shape`, `pointer`, `clock` | `hover` | `clock` (the date: moon) | | `--clock` | 3 |
| `pond` | R | `shape`, `pointer`, `clock` | `hover` | `clock` (date: moon, season) | choice `--season` | `--clock` | 3 |
| `train` | S, W | `clock` | `widget.*` | `clock` (local time of day, date) | choices, `--at HH:MM` | `--clock` | 3 |
| `window` | S, W | `clock` | `widget.*` | `clock` | choices | `--clock` | 3 |
| `ask` | S, W | none (keys of its focused widget) | `widget.ask`, `input` of the widget, modal? no | **result** to the caller, an exit status | `QUESTION OPTION*` positionals; `--timeout` | focus and keys of a widget; the result in the report | 4 |
| `pace` | S, W | `text`, `keys-read`, `clock`; `--save`: `files`, `files-write` | topics `input` (`bytes`, base64), `cursor`, `text`; `screen.text`, `screen.cursor` | `clock` (dates), **storage** (the counts), `files` (`--stop FILE`, `--save FILE`) | file options | keys read (`--event input`), `--file`, storage | 5 |
| `note` | S, W | `files`, `files-write` | `widget.ask`, keys, paste | `files`: read, `stat` (mtime), atomic rewrite | `FILE*` positional, default `TODO.md` of the working directory | `--file`, the write in the report | 6 |
| `gallery` | S, W | `--file`, `--dir`: `files` | `widget.*`, `hover` on its widget | **assets** (`pictures/*.pic`), `files`, a directory listed | repeated `--file`, `--dir` | assets headless, `--file` | 6 |
| `minimap` | R, W | `text` | `frame.list`, `frame.get`, `frame.text`, `screen.text`; topics `text`, `frames`, `cursor` | `files` (`--file`: a text shown in place of the frame; default: its own source, to drop) | `--mark PATTERN`: the user's pattern | `frame.text` headless | 6 |
| `git` | S, W | `probe-git`, `clock` | `widget.*` | **`probe.git`** (`_gitprobe.Repo`), the working directory of the program, `--repo PATH`; `clock` (local dates) | `--monitor LIST`, `--repo` (a directory) | `--probe git=FILE` | 7 |
| `monitor` | S, W | `probe-system`, `clock` | `widget.*` | **`probe.system`** (`_probe`: glance, logins, listeners, disks) | | `--probe system=FILE` | 7 |

Shared code to port once: `_widgets.Deck` (14 apps), `ptyrc.ui`
(`ask`, `note -i`: a text field, a list), `_canvas` (Braille, Half:
`cat`, `pace`, `train`, `window`, `globe`), `_cli.Sight` (the shape of
the view, kept by topics), `_cli.follow`/`heir`, `_cli.Nozzle`,
`_sky`, `_strip`, `_reef`, `_arcade`.

## Gaps that block, by the first wave that needs them

Sizes: S under an hour of an agent, M a session, L several.

### Before any wave

| # | gap | where | size |
|---|---|---|---|
| 1 | **Manifest options.** Today: `int`, `float`, `bool`, `string`, long names. Missing: a short flag (`short = "a"`); `choices`; positionals (`[args]`: name, `min`, `max`); a repeated option (a list); types `file`, `dir` (host-functions, Files), `word`, `pattern` (gap 4) | `ptyrc-store/src/manifest.rs`, `ptyrc-server/src/store.rs` (`init_args`), `ptyrc-cli/src/storeapp.rs`, `dev.rs`, `PROTOCOL.md` | M |
| 2 | **A permission tied to an option** (`snake --grab`: `keys-arrows`; `flower -a`: `text`; `timer --modal`). The plan has it ("asked again only for ... an option that declares something new"); the manifest declares all or nothing. `needs = ["keys-arrows"]` on an option; `declared()` adds them when the option is given | `manifest.rs`, `store.rs` (`declared`), the pop-up's difference, `dev check`, `dev test` | M |
| 3 | **The launcher's generic options** (principle 4): `--front`/`--back`, `--frame`, `--prefer`, `--alpha`, proposed `--border`: reserved in the manifest, parsed nowhere. `app.start {store}` takes `frame` only | `ptyrc-cli/src/storeapp.rs`, `driver.rs` (`app.start`), `driver/own.rs` (the band), the menu of `manage`, `dev test`, `dev run` | M |
| 4 | **The user's words and patterns** (principle 4): `flower --word W`, `marks PATTERN`, `minimap --mark P`. A value of type `word` joins the words held for that run; a `pattern` opens `screen.find` (and `known`) for that pattern alone. Without it `marks` asks `text`, red, to find one word | `grants.rs` (`Grants.words`, a `patterns` list), `lower.rs` (`screen.find`), `store.rs` | M |
| 5 | **A deck of widgets in both SDKs**: variants by size, `widget.declare/show/set`, the topics `widgets`, `move`, hover and keys of a widget, one batch a period; a `Canvas` as a widget's content (`widget.set {content}` takes `rows`, `cells`; check `grid`, add it if absent). `ptyrc.ui`'s field and list for `ask`, `note` | `sdk/js` (a module beside the prelude, or `ptyrc/deck`), `sdk/rust/ptyrc-app`; `ptyrc-core` widgets for `grid` | M |
| 6 | **Headless methods**: `batch`, `draw.list`, `draw.exts`, `draw.ext`, `widget.ask`, `widget.release`, `widget.list`, `widget.borders`, `frame.heat`, `frame.text`, `screen.cells`, `app.list` answer `headless` today. `batch` is in nearly every app | `ptyrc-box/src/headless.rs`, `tests/protocol.rs` | M |
| 7 | **Headless hands**: a pointer and a click hit-tested on the scene (`--pointer MS:X,Y`, `--click MS:X,Y`: the `hover`/`mouse` event with the app's own sprite id, which `--event` cannot know); a widget focused and its keys (`--focus MS:TAG`, then `--key`); a paste | `headless.rs`, `ptyrc-cli/src/dev.rs` | M |
| 8 | **Unit tests of a script**: `test_bricks.py` (1060 lines), `test_strip`, `test_reef`, `test_sky`, `test_line`, `test_canvas`, the parsers: pure logic, no screen. A Rust app has `cargo test`; a script has nothing. `ptyrc dev test --unit [FILE]`: the interpreter runs `tests.js`, each exported `test_*` function, an `assert` in the prelude, the report | `sdk/js/interp`, `ptyrc-box` (`--script` mode), `dev.rs`, template | M |
| 9 | **Shipped store apps**: `plugins/store-apps/NAME/` exists nowhere. To decide and build: their origin (`ptyrc/NAME`?), found without `store add`, their tier, whether the pop-up asks; `ptyrc app NAME` when a native twin exists (done: `fdeda44`); one harness that runs `dev check` and `dev test` on each tree against kept screens (`expected/*.txt`), in `cargo test` or `pytest` | `ptyrc-store` (a shipped registry or a built-in tree), `ptyrc-cli`, `plugins/store-apps/`, the justfile | M |
| 10 | **Rust tier usable**: `ptyrc-app` without `serde_json` (decision 20; its `Value`, `json!` are the API: every Rust app changes); the `rustc`-only build at install; shipped Rust apps: built on the user's machine or with the release (owner) | `sdk/rust/ptyrc-app`, `ptyrc-store` (build), `dev check` | L |
| 11 | **QuickJS** in place of Boa: the speed the tiers above assume | `sdk/js/interp` (running: agent `quickjs`) | — |

### Wave 1 (the examples)

| # | gap | where | size |
|---|---|---|---|
| 12 | **The surface follows its frame.** `_cli.follow`/`heir`: when a pane goes, the app moves to the pane that took its place; it reads `frames` (`layout`) for it. For a store app the driver must re-home the surface (`Surface::Frame`) and its sprites itself: check `driver/own.rs`; else `rain`, `snow`, `sand` ask `layout` only to follow | `driver/own.rs`, `ptyrc-core` pool | M |
| 13 | **The pointer told to an effect** (`_cli.Nozzle`): `sand --hover/--bomb`, `snow --hover` relay `hover`/`mouse` of a pad sprite to `draw.update {effect}`. Works for hover under the default profile (its own sprite). `--bomb` needs the clicks on the pad kept from the program: `clicks-grab` opens "nothing yet" | `driver/own.rs`, `grants.rs`, `mouse.rs` | S (grab on its own sprite) |
| 14 | `-a/--all` and option groups: an app option, fine once gap 1 gives `short` | | — |

### Wave 2 (pointer, games)

| # | gap | where | size |
|---|---|---|---|
| 15 | Rust apps (gap 10) for `flock`, `life`, `shore`, `cat`, later `stars`, `pond`, `minimap` | | — |
| 16 | `draw.list` on its own sprites tells where an anchor landed (a hole of `todolist.md`): `cat` uses `Sprite.info`. Decide: told under `shape`, hidden under it | `driver/own.rs`, `lower.rs` | S |
| 17 | A tick of 50 ms for a simulation in a script (`bricks -m`, many balls): measure after QuickJS; `[limits] tick-ms` exists | | S |

### Wave 3 (clocks)

| # | gap | where | size |
|---|---|---|---|
| 18 | `clock.now`, `clock.zone`; `dev test --clock`, `--zone` | host-functions, Clock | M |
| 19 | A tick on the wall second (`_widgets.py:352`): `period(1000)` has no phase. Enough: the app asks `period(1000 - unix_ms % 1000)` once, then 1000; document it | SDK docs | S |

### Wave 4 (`ask`)

| # | gap | where | size |
|---|---|---|---|
| 20 | `app.result`; `ptyrc app ID` in the foreground waits, prints the result, exits with the app's status; `run --app`, scripts that call `ptyrc-app ask` | host-functions, Result; `ptyrc-cli`, `apps.rs` | M |
| 21 | The text field and the list of `ptyrc.ui` (gap 5) | | — |

### Wave 5 (`pace`)

| # | gap | where | size |
|---|---|---|---|
| 22 | `storage.*` | host-functions, Storage | M |
| 23 | base64 in a script (`input` events carry `bytes`): `atob`/`btoa` or `decode64` in the prelude | `sdk/js/interp` | S |
| 24 | `file.*`, read and write | host-functions, Files | M |

### Wave 6 (files)

| # | gap | where | size |
|---|---|---|---|
| 25 | `asset.*` | host-functions, Assets | S |
| 26 | A directory named (`gallery --dir`): `file.list`, a child read by plain name | Files | S |
| 27 | `note`: `file.stat` (mtime), a rewrite guarded by the mtime read; a default file of the working directory (owner) | Files | S |

### Wave 7 (probes)

| # | gap | where | size |
|---|---|---|---|
| 28 | `probe.git`, `probe.system`: the parsers of `_gitprobe.py` (546 lines) and `_probe.py` (684) in Rust, their fixtures carried over (`test_git.py`, `test_monitor.py`) | host-functions, Probes; a crate `ptyrc-probe` | L |
| 29 | A worker for what blocks (git, a slow file): the driver's loop must not wait on it | `ptyrc-server` (`hostio.rs`) | M |

### At the end of each wave

| # | gap | size |
|---|---|---|
| 30 | The Python twin removed: `_catalog.py`, `tests/test_reads.py`, `RANDOM` (the menu's "random app": `random = [[...]]` in the manifest, read by `manage`), `scripts/record-tapes.sh`, `demo-apps.sh`, `GALLERY.md`, the README's commands | S a wave |

## Not blocking

The primitives that use without reading (anchor on the pointer, a
motion that avoids the cursor, gating by idle, a blend by heat) lower
what an app asks; each app can move with what it declares today
(`shape`, `pointer`). After the migration, or with the app that gains
most (`flower`: anchor on a word: none in place of `words`).
`host-functions.md`, Primitives.

## Counts

Blocking gaps, without the two already running or cross-references:
S 8, M 14, L 2 (the Rust tier; the probes).
