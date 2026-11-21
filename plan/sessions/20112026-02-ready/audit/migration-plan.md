# Migration plan, 2026-11-20 (audit at `6bd169c`)

The 26 Python apps left (`readiness.md` but `confetti`, `sand`), each
checked against what is built. Method: a draft `ptyrc-app.toml` an app
(`drafts/NAME/`, made by `gen.py` from `ptyrc-app --catalog`, a stub
entry), `ptyrc dev check` on each (`check.txt`); probe apps under `dev
test` for what was doubtful (`t/w1`, `t/p2`, `t/p3`, `t/a1`); grep for
the rest. Not done: no app was ported, no live driver was run.

**Result.** The 26 drafts pass `dev check` (7 Rust stubs built by
`rustc`). Every method, topic and host function the apps use is
answered headless. Nothing blocks a first app. Five things should be
settled before 26 manifests and 400 tests are written on top of them
(a 1 to 5), and one mechanism is missing: shared code (b).

## (a) Findings, in order

Blocking means: wrong or copied 20 times if left.

| # | finding | where | size | fix |
|---|---|---|---|---|
| 1 | **Headless does not clip to the surface.** A bare sprite of a `surface = "widget"` app is drawn by `dev test` (`t/w1`: text and a confetti effect on the screen); the driver puts it nowhere (`driver/own.rs:113`, `NOWHERE`). A case of `timer -b` would be blessed and show nothing live. `frame`, `screen` without `screen-wide`: not checked, same code path | `rust/ptyrc-box/src/headless.rs`; the rule: `ptyrc-server/src/driver/own.rs` `Own::clip` | S (M inside the one crate of rules) | headless applies `clip`; a row in `tests/protocol.rs` |
| 2 | **No default frame.** A store app starts on `active` (`driver.rs:2452`); the manifest has no key. Python defaults: `largest` (bricks, cat, fishes, flock, flower, life, minimap, pond, shore, snake, stars), `raw` (rain, snow, confetti), `idle` (sand), `all` (marks). Same on one pane; differs under tmux | `ptyrc-store/src/manifest.rs` (`[app] frame = "largest"`: a name the driver resolves), `driver.rs`, `dev test`, `PROTOCOL.md` | S | the key; or the owner accepts `active` (question 1) |
| 3 | **No option without a default** (`[options.seed]: a default is wanted`), though `PROTOCOL.md` says `null` when it has none. 40 options of the 26 apps have none: `--seed` (16 apps), `--count`, `--shy`, `--phase`, `--max-depth`, `--hold`, `--timeout`, `--tz`, `--until`, `--pomodoro`, `--at HH:MM`, `--kinds`, `--monitor`, `--title`, `--default`, 9 choices. The drafts use sentinels (`-1`, `""`, `"auto"`) | `manifest.rs`, `options.rs` (`resolve`, `help`) | S | `default` optional for `int`, `float`, `string` (a choice too): `null` in `init` |
| 4 | **`--seed`.** 16 apps declare it; it collides with `dev test --seed` (after `--`); the SDK's `random` is the host's, seeded by `dev test` only. Either each app keeps its own generator and option, or the launcher takes `--seed N` and seeds the runner's `random` (one implementation, the cases and the user's run alike) | `options.rs` (`Launcher`), `app.start`, the runner's argv | S to M | owner (question 2); recommended: the launcher's |
| 5 | **Shared code** has no path: see (b) | harness, SDKs | S + M + M | (b) |
| 6 | No Braille or half-block canvas in either SDK (`Canvas` is cells). Python: `_canvas.Braille`, `Half`, `mix`, `xterm`, drawn by 13 apps (clock, timer, pace, git, minimap, train, window; cat, life, flock, stars, pond, shore) | `sdk/js/interp/prelude.js`, `sdk/rust/ptyrc-app` | M (a tier) | part of (b) 1 |
| 7 | No `Sight` (the shape of the view kept by the topics `frames`, `text`, `cursor`; `_cli.py:388`): bricks, snake, fishes, flower (script), flock, life, pond, shore, stars (Rust) | both SDKs | M | part of (b) 1 |
| 8 | No tool writes a `--probe git=FILE.json` or `system=` fixture; `git` (24 tests) and `monitor` (34) would write the JSON by hand from the contract | `ptyrc-cli/src/dev*` (`ptyrc dev probe git [DIR]`, `system`: the driver's answer printed) | S | before wave 7 |
| 9 | `--key` does not reach an app that holds `keys-read` (`dropped: to the program, not grabbed`); `pace` must write `--event T:input:{"key","bytes"}` | `headless.rs` | S | deliver as the driver does |
| 10 | The deck has no click handler; `mouse` on its own widget is delivered (`t/w1`, `--click` on a widget cell: press and release with `sprite`). `timer --click` handles `mouse` itself | `deck.js`, `deck.rs` | S | `click(x, y, widget)` beside `hover`; not blocking |
| 11 | `[args]` is one list. `ask QUESTION OPTION*` becomes one list whose first is the question; `--help` loses the names | `manifest.rs` | S | leave; or several entries, all but the last single |
| 12 | `marks -e` cannot be said: a pattern is held with its `regex` as declared (`lower.rs:72`), not by a flag of the run. Draft: positionals literal, `--regex RE` (`-e RE`, repeated). With it `marks` needs no `text` (checked: the user's patterns are found under `--deny text,words`, `known` and `ignore_case` too) | the app | — | owner (question 5) |
| 13 | Headless has one pane: no case can show a second frame. The Python tests of follow, heir, move (`test_move` 3, `test_apps` 3, one of `minimap`) are the driver's now (`driver/heir.rs`) | — | — | not carried, said in each report |
| 14 | Small: `asset` names are relative to the `assets` directory (`a.pic`; `ptyrc.d.ts` says `pictures/ship.pic`). `sdk/rust/ptyrc-app/README.md:248,257` still names `serde_json`. A `--screen T:FILE` change sends `screen` and `text` twice. `draw.list` in the tick of the `draw.put` tells an anchored sprite `visible: false`, `screen: null` (read it at the next tick: `cat`). A widget is not placed in the first 500 ms of a run (proof finding 5) | docs, `headless.rs` | S | with the proof's findings |

Checked and fine: `[options]` `short` upper case (`-S`, `-W`, `-H`);
`list` with `choices`; a `string` with `needs` (`--until`); a default
file in `[args]` (`note`); lists of `file` and `dir` (`gallery`);
`--no-NAME` of a bool true by default (`--no-seconds`, `--no-hud`,
`--no-grass`); `random` with a positional (`timer 5m`). Headless:
`hover`, `mouse` on its own sprite behind the text with `pointer`,
`clicks` (and dropped, named, when denied); `--focus`, `--key`,
`--paste` to a deck `Field`, `result` and `exit` (`t/a1`, the whole of
`ask`); `modal` with `needs`; `screen.find` of a `word` and `pattern`
option; `anchor {text}`; `frame.list`, `frame.text`, `frame.heat`,
`screen.cursor`; `assets()`, a `dir` listed and a child read, a `file`
read; `storage` from `--storage DIR`; `--event` for `input`, `cursor`;
`screen`, `text` on a `--screen T:FILE`. Widget sizes of the picture
apps are 48x11 at most (`train`): a script holds them.

## (b) Shared code

What is shared, and by whom (`from ._x import`, and apps importing
apps):

| code | lines | script apps | Rust apps |
|---|---|---|---|
| `_widgets.Deck`, `ui.Editor`, `ui.Listing`, `cut`, `fit`, `wrap`, `Keys` | | done: `ptyrc/deck` | done: `deck.rs` |
| `_cli.follow`, `heir`, `Nozzle` | | done: the driver | |
| `_canvas`: `Braille`, `Half`, `mix`, `xterm` | 272 | pace, train, window (clock, timer, git draw Braille inline) | cat (life, flock, stars, pond, shore, minimap inline) |
| `_cli.Sight`, `is_word`, `found` | 110 | bricks, snake, fishes, flower | flock, life, pond, shore, stars |
| `ui.Model`, `Line`, `Choice`, `Text`, `foot` | ~500 | ask alone (note: `foot`) | |
| `_arcade` | 269 | snake, bricks | life: `spark`, `burst` |
| `_reef` | 817 | fishes | cat: `mirror`, `pad`, `paint` |
| `_sky` | 248 | train, window | pond: `season_of` |
| `_strip` | 876 | train, window | shore: `Pointer` |
| `_line`, `_mouser` | 213, 730 | train; window | |
| `stars.Grid`, `noise`, `runs`, `phase_of`; `life.DOTS`; `snake.walls_of` | ~200 | | stars, pond, shore, life |
| `_patterns`, `_bricks` | 1077, 426 | bricks | life |

Today: a script imports files of its own tree only (`"./lib/x.js"`), a
Rust app has `ptyrc-app` and its own modules; `prelude.js` and
`deck.js` are compiled into the interpreter. Nothing reaches two trees.

**Recommended: two places, by kind.**

1. **What any author would want goes into the SDKs, both tiers, as
   twins**: `Braille`, `Half` and the colour helpers (finding 6);
   `Sight` (finding 7). In `"ptyrc"` (or `"ptyrc/canvas"`) and
   `ptyrc-app`; typed in `ptyrc.d.ts`; `test_canvas.py` (6) and the
   two `Sight` tests carried. It is API (version 1): keep it small.
   Cost: M a tier, two agents in parallel (`sdk/js`, `sdk/rust`); the
   interpreter grows by some KiB.
2. **What belongs to a family of our apps is copied into each tree**,
   from one source: `plugins/store-apps/_shared/js/{arcade,reef,sky,
   strip}.js`, `_shared/rust/{grid}.rs`, copied as `NAME/lib/x.js` or
   `NAME/src/x.rs`. The harness (`ptyrc-cli/tests/store_apps.rs`)
   checks each copy is the source, byte for byte, as it does `LICENSE`
   (`store_apps.rs:158`); `PTYRC_BLESS=1` (or `PTYRC_SYNC=1`) copies.
   git stores one blob. `_shared` is not listed as an app (`is_name`:
   a letter first; no manifest). Each tree stays whole: "an app is one
   tree", the store, the build and the runner unchanged. Cost: S (the
   check, a paragraph of the README). The unit tests of a shared file
   live beside it and run once.

Rejected: a path out of the tree (`import "../_shared/x.js"`, a `lib`
key resolved by `dev` and the shipped loader): the store, the runner
and the Rust build each need the rule (M+), and a shipped app would no
longer be what a store app is. Everything in the prelude: app families
frozen as API, the interpreter heavier for every app.

Cross-tier cost of the families is small: the Rust side wants only
`spark`/`burst`, `mirror`/`pad`/`paint`, `season_of`, `Pointer`:
written once more in the Rust tree that needs them, under 100 lines
each. No tier needs to change for it.

## (c) One row an app

Draft: `dev check` on `drafts/NAME/`: all 26 `ok`. Size: S under an
hour, M a session (250k), L more than one. Tests: Python tests of the
app (its files and its tests of `test_apps.py`), of which with a live
driver.

| app | tier | wave | draft asks | what it waits for | tests (live) | size | model |
|---|---|---|---|---|---|---|---|
| rain | S | 1 | layout | 2 (frame) | 1 (1) | S | sand |
| snow | S | 1 | layout; `--hover`, `-a`: pointer (told to the effect: none, as sand) | 2 | 7 (3) | S | sand |
| marks | S | 1 | shape, front, screen-wide | 12 (owner); 2 (`all`) | 3 (1) | S | confetti |
| flower | S | 1 | shape, words, front; `-a`: text | 7 (`Sight`), 3 (`--count`) | 7 (2) | M | confetti |
| fire | S, W | 1 | none | 3, 4 (`--seed`) | 12 (1) | M | clock |
| globe | S, W | 1 | none | 3, 4 | 13 (1) | M | clock |
| fishes | S | 1 | shape | 7; `_shared/js/reef`; question 4 (`--tank`) | 30 (3) | L | confetti; clock for the tank |
| snake | S | 2 | shape; `--grab`: keys-arrows | 7; `_shared/js/arcade`; the SDK example is a reduced one | 14 (6) | M | sdk/js/examples/snake |
| bricks | S | 2 | shape; `--grab`: keys-arrows | after snake (`arcade`); `_bricks`; gap 17 (tick of `-m`) to measure | 29 (3) | L | snake |
| life | R | 2 | shape; `--cursor`, `-a`: pointer | 6, 7 (Rust); `_patterns` | 18 (4) | L | — (the Rust model) |
| flock | R | 2 | shape, pointer | 6, 7 | 12 (2) | L | life |
| shore | R | 2 | shape; `-a`: pointer | after stars (`grid`) | 17 (3) | L | life |
| cat | R | 2 | shape, words; `--hunt`, `-a`: pointer | 6; 17 tests live, 5 soaks: the heaviest | 24 (17) | L+ | life |
| timer | S, W | 2/3 | `--modal`: keys-grab, front-widget; `--until`: clock | 1 (`-b`: surface, question 6); 10; `--click` collides with `dev test` | 24 (1) | L | clock |
| clock | S, W | 3 | clock | 6 (digits in Braille: inline is enough) | 13 (2) | M | — (the deck model) |
| stars | R | 3 | shape, pointer, clock | 6, 7; writes `_shared/rust/grid` | 15 (3) | L | life |
| pond | R | 3 | shape, pointer, clock | after stars | 15 (2) | L | stars |
| train | S, W | 3 | clock | 6 (script); writes `_shared/js/{sky,strip}`, `_line` | 20 (1) + sky 6, strip 18 | L | clock |
| window | S, W | 3 | clock | after train; `_mouser` | 30 (2) | L | train |
| ask | S, W | 4 | none; `wait = true` | `ui.Model` and its three kinds in the tree; 11 | 8 (1) | M | clock; `t/a1` |
| pace | S, W | 5 | shape, text, keys-read, clock, storage, files (`--stop`) | 6 (script), 9 | 17 (1) | L | clock |
| note | S, W | 6 | files, files-write | nothing | 19 (2) | L | the proof's `notes`; clock |
| gallery | S, W | 6 | files (`--file`, `--dir`) | nothing; `pictures/` as `assets` | 16 (1) | M | clock |
| minimap | R, W | 6 | shape, text (+ layout to list frames) | 6 (Rust); `deck.rs` | 15 (1) | M | life; `sdk/rust/examples/clockface` |
| git | S, W | 7 | probe-git, clock, files (`--repo`) | 8 | 24 (3) | L | clock |
| monitor | S, W | 7 | probe-system, clock | 8 | 34 (2) | M | clock |

Options every draft drops: `-f/--frame`, `--border`, `--prefer`,
`--front` (the launcher's), `--print`. Dropped as test hooks: `--speed`
and `--at` of fire, globe, timer; `--speed` of gallery, pace;
`--colour` of gallery, `--file` of minimap, `--seed` of pace, `--json`
of ask, `--save` of pace (storage). Kept, the app's: `--speed` of
snake, bricks, life, train, window; `--at` of flock, shore, train,
window (after `--` under `dev test`, as `--file` of gallery, `--click`
of timer, `--seed`).

Tests: about 440 for the 26 apps and 48 of the shared modules. Every
kind can be carried: logic as unit tests (`tests.js`, `cargo test`);
`--print WxH` as a unit test of `render(w, h)` or a case at a `--size`
that picks the variant; a live session as a case on `--screen`,
`--tape`; the host mouse as `--pointer`, `--press`; real time as
session time. Not carried: two panes (finding 13); `test_reads.py`
(5), `test_catalog.py` (3): the manifest and a `--deny` case say it;
the long soaks of `cat` (5): to measure headless (`--for 600s`) before
promising them.

## (d) Order, and who can go together

Trees are independent: the harness finds them (`read_dir`), no list to
edit. Shared files: `_shared/*` (its writer first), the models list of
`plugins/store-apps/README.md`, and at the end of a wave `_catalog.py`,
`test_reads.py`, the scripts (gap 30: one agent, alone). Three or four
agents at once.

0. **Before any app** (in parallel where files differ): findings 1
   (with the crate of rules), 2, 3, 4; (b) 2 the `_shared` check; (b)
   1 the canvases and `Sight`, an agent a tier; 9, 10, 14 with the
   proof's findings.
1. **Models**: `clock` (deck, `clock`, zone) and `life` (Rust,
   `Sight`, Braille, a click). They prove step 0; the guide gains
   them.
2. **Wave 1**, together: rain + snow + marks (one agent), flower,
   fire, globe. Then fishes (writes `reef`).
3. **Games**: snake then bricks, one agent after the other (`arcade`).
   **Scenes**: stars (writes `grid`), then pond, shore, flock
   together. cat alone, last of the wave, two sessions.
4. **Decks**: timer, ask, gallery, note in parallel; train (writes
   `sky`, `strip`), then window.
5. pace; minimap; after finding 8: git, monitor.
6. At each wave's end: gap 30.

## (e) For the owner

1. A default frame in the manifest (`[app] frame = "largest"`), or
   every store app starts on `active`? Recommended: the key.
2. `--seed`: the launcher's (seeds `random`), or each app's own?
   Recommended: the launcher's; the 16 options go.
3. An option without a default (`null`)? Recommended: yes, before the
   manifests are written.
4. `fishes --tank` reads nothing but shares the id: `fishes` then
   asks `shape` for a tank that needs none. A second app `tank`?
   Recommended: yes (the reef in `_shared`).
5. `marks`: positionals literal, `-e RE` takes a value; no `text`
   asked. Its default was every frame: `surface = "screen"` and
   `screen-wide` (amber), or the frame alone?
6. `timer -b` (confetti): `surface = "frame"` (the widget and the
   confetti on its pane, nothing asked) or `"screen"` (amber)? Draft:
   `frame`.
7. The hooks dropped and kept above (`--speed`, `--at`): as listed?
8. Shared code: SDK for canvases and `Sight`, `_shared` copies for
   the families, as (b)?
9. `ask --json` dropped: `--wait` prints the value. Not checked: how
   `--wait` prints a list (`-m`) against a string.
