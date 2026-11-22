# sdk/js

The script tier of store apps (`plan/store-model.md`, Scripts): a
JavaScript interpreter that is itself a guest of ABI 1
(`rust/ptyrc-box/README.md`), and the API scripts are written against.

| | |
|---|---|
| `ptyrc.d.ts` | the API, typed: the whole of it. An app is written from this file |
| `interp/` | the interpreter guest (`ptyrc-js.wasm`): QuickJS-ng and `ptyrc.c` (the natives, the loader), the modules `"ptyrc"` (`prelude.js`), `"ptyrc/deck"` (`deck.js`), `"ptyrc/canvas"` (`canvas.js`), `"ptyrc/sight"` (`sight.js`), `build.sh`, `NOTICES` |
| `examples/clockface/` | a deck of widgets: two sizes, a canvas, a key and a click of its widget, unit tests (the deck on a host of its own); the model of a deck app |
| `examples/dots/` | a curve in Braille over the frame, kept a cell from the text by a `Sight`, `shape` denied; the unit tests of `"ptyrc/canvas"` and `"ptyrc/sight"` |
| `examples/confetti/` | paper falling behind the text, `rate` 1 to 200 by the manifest; no test of its own (`rust/ptyrc-cli/tests/storeapp.rs`) |
| `examples/sand/` | one effect of the driver started, no permission; the twin of `sdk/rust/examples/sand` |
| `examples/tally/` | storage, the wall clock, `civil`, the result, each denied; the twin of `sdk/rust/examples/tally` |
| `examples/snake/` | `screen.shape`, `input.grab` of the arrows, each denied in turn; the twin of `sdk/rust/examples/snake` |
| `../template/` | what `ptyrc dev new` writes (embedded in `ptyrc`) |

## Build

```
sdk/js/interp/build.sh
```

Needs `clang` and `wasm-ld` (LLVM 19 or later), `curl`, `tar`,
`sha256sum`. Three inputs, pinned by their SHA-256 in the script,
fetched into `sdk/js/interp/target/cache/` when absent (122 MB, the
sysroot most of it), never committed: QuickJS-ng 0.18.0, the
wasi-sysroot and `libclang_rt.builtins` of wasi-sdk 34.

The module: `sdk/js/interp/target/ptyrc-js.wasm`, found there by a
`ptyrc-box` of `target/<profile>/`. It is not in the repository: built
with a release, not on the user's machine; `interp/NOTICES` ships
beside it.

The runner looks for it (`ptyrc_box::script::interpreter_path`): the
file named by `PTYRC_JS`; else `ptyrc-js.wasm` beside the program; else
`../share/ptyrc/ptyrc-js.wasm` from there.

```
PTYRC_JS=.../ptyrc-js.wasm ptyrc-box --script examples/confetti/app.js '{"rate": 30}'
```

## The interpreter

QuickJS-ng 0.18.0 (MIT), compiled with wasi-libc (MIT, its parts
musl, cloudlibc, dlmalloc) for `wasm32-wasip1`, linked as a reactor.
`interp/NOTICES`: the texts.

The module imports nine functions of `ptyrc` and nothing else: the
WASI functions the libc calls are defined in `ptyrc.c` (the clock,
`random_get`, `fd_write` to the log; the rest fail). The runner
refuses any other import.

- The clock is session time: `Date.now()` is the `now` of the last
  `tick`, 0 at the start; the time zone is UTC. No `performance`.
- `Math.random` draws from the import `random`: a seed in a test run.
- No timer, no `fetch`, no `console`: the names are not defined; the
  error says what to use.

Size: 1.0 MiB (`-O2`).

## How the sources get in

In the JSON of `init`, under two reserved members; the ABI is
unchanged.

```json
{"ptyrc:sources": {"app.js": "...", "lib/util.js": "..."},
 "ptyrc:main": "app.js",
 "rate": 30}
```

- The runner builds it: `script::sources(dir)` (every `.js` and `.mjs`
  of the tree; 1 MiB, 512 files) and `script::init_args(main, sources,
  args)`. The reserved members win over arguments of their names.
- `ptyrc:manifest`, set by the runner from `--manifest`: what the app
  may know of its manifest, the grants and limits among it, and its
  frame at the start (`frame`); `manifest()` returns a copy, `null`
  without one.
- `ptyrc:unit`, set by `--unit`: the tests run, not `init`.
- The interpreter takes the four out; the script's `init(args)` gets
  the rest.

## Modules

- `"ptyrc"`: the API.
- `"ptyrc/deck"`: widgets kept drawn (`Deck`, `Widget`), and the
  pieces of an interactive one (`Keys`, `Field`, `Listing`, `cut`,
  `fit`, `wrap`); on `"ptyrc"` alone. `deck.tick(now)` sends what
  changed in one `batch` (one by one where `batch` answers `headless`);
  until a `widgets` event names the variant shown, it renders each. A
  `Canvas` goes as `cells` (runs).
- `"ptyrc/canvas"`: `Braille` (2 by 4 dots a cell), `Half` (two pixels
  of colour a cell, glyphs over them), colours (`mix`, `xterm`,
  `palette`, `named`, `hex`, `channels`); pure, the twin of
  `_canvas.py`.
- `"ptyrc/sight"`: `Sight`, the shape of the view of a frame kept by
  the topics `frames`, `text`, `size`, `move`, `cursor` (`_cli.Sight`);
  by default of the app's frame told at `init`.
- The four are compiled only when imported. No file answers to their
  names: a file `ptyrc.js` is `"./ptyrc.js"`.
- `"./x.js"`, `"../lib/y.js"`: files of the tree, by path relative to
  the importing file. Outside the tree, absolute, a URL: refused.
- `"ptyrc:native"`, the host functions under the API, is served to the
  API alone, before any file of the app is read.

## Host functions

In `"ptyrc"`, over `call` (`PROTOCOL.md`, Methods; the same names as
`ptyrc-app`'s): `now()`, `zone(name, at)`, `civil(unixMs, offsetS)`,
`until(unixMs, everyMs)` (pure); `storage.get`, `.set`, `.remove`,
`.keys`; `asset(name)`, `assetBytes(name)`, `assets()`; `file(handle)`
and its `text`, `bytes`, `textWithMtime`, `bytesWithMtime`, `stat`,
`write`, `list`, `child`; `result(value, status)`. Members in
camelCase (`unixMs`, `offsetS`, `mtimeMs`); reads by pieces of 1 MiB,
a file changed between two: `not-usable`. `Date.now()` stays session
time.

A write that keeps a change made by someone else since the read: the
mtime of the read guards it; `stat()` after `text()` leaves a window.

```js
let { text, mtimeMs } = f.textWithMtime();
// ... the user edits `text`; then:
try {
  mtimeMs = f.write(text, { ifMtime: mtimeMs }).mtimeMs; // the guard of the next write
} catch (e) {
  if (e.code !== "not-usable") throw e; // changed since: read it again
}
```

A tick on the second: at each tick, `period(until(now().unixMs,
1000))`; asked again each time, it does not drift.

## Bytes

The engine's base64: `Uint8Array.fromBase64`, `toBase64`, `atob`,
`btoa`. `TextEncoder`, `TextDecoder`: the prelude's, UTF-8 only, an
invalid sequence a U+FFFD; `{ fatal: true }`: a `TypeError`.

## Unit tests

`ptyrc dev test --unit [FILE]` (`ptyrc-box --unit FILE --script
APP.js`): FILE (`tests.js`) of the app's tree loaded with `"ptyrc:unit":
true` in `init`; `init` of the app is not called. Each export `test_*`,
in the order of the names, runs in an `event` of its own, under its
limits but time: 10 s a test (and for loading FILE), 60 s the file in
all (the rest not run); `ok   NAME` or `FAIL NAME` and the error as logged
(`file:line:col`); then `P passed, F failed`. A test ended by the
runner (`time`, `stack`): the rest not run. Status 0 when all passed.
No session: `call` answers `not-usable`; a `Sight` takes a `call` of
the test's own (`examples/dots/sight.tests.js`). `assert`,
`assertEqual` (deep), `assertThrows`: of `"ptyrc"`.

## Errors

A line of the app's log: `file:line:column: what`, the frames of the
app, the cure when known.

```
app.js:2:3: ReferenceError: setTimeout is not defined
    at init (app.js:2:3)
  hint: no timer: ask a period (period(50)) and count in tick(now)
```

The place: of the statement, in the first frame of the app (frames of
the API are left out); of the token, for a syntax error. The text:
`describe` in `prelude.js`, handed to `ptyrc.c` once.

- In a module or in `init`: the app ends, status 1.
- In `tick`, `event`, `stop`: a `PtyrcError` (a request refused) is
  logged once and the app goes on; any other ends it.

## Limits

The runner's. A loop without end, a queue of jobs without end
(`then`, `queueMicrotask`, `await` in a loop): `time` (the epoch); the
jobs run inside the call that queued them. Memory without end: the
growth is refused at 64 MiB, the script gets `InternalError: out of
memory` and the app ends. Recursion: a `RangeError` past 352 KiB of
the interpreter's stack (about 1000 frames of a small function); the
parser nested deeper: `stack`, the runner's.

## Speed

Measured 2026-11-20, a 200x60 canvas, 30 Hz, a headless run, release
(`cargo test --release -p ptyrc-box --test script speed -- --ignored
--nocapture`), on a loaded machine (three runs):

| a tick that | costs |
|---|---|
| sets every cell, `set` 12000 times, a colour a cell | 8.5 to 8.9 ms (the runner's diff included) |
| fills 60 rows, `fill` 60 times | 0.44 ms |
| does 100000 steps of integer arithmetic | 6.5 to 6.7 ms |

- A `set`: about 0.7 µs. A step of a loop: about 65 ns.
- `"ptyrc/canvas"`, a `Braille` of 200x60 (`dev test --timing`, a
  release runner, 2026-11-20, a loaded machine, three runs): 96000
  dots by `set`, then `rows`, 60 `text` and `present`: 41 to 48 ms a
  tick, against the 50 ms bound; `set` alone 41 to 45 ms (0.45 µs a
  dot). The same cells by `bits` (`fill(255)`), `rows`, `text`,
  `present`: 2.1 to 2.8 ms. `examples/dots`, a curve of 600 segments
  and the text spared: 12.6 ms.
- A start: 0.18 to 0.20 s the first time in a process (Cranelift
  compiles the module), 1.1 ms after it. A runner is one process an
  app: each app pays the first.

A script paints by runs (`fill`, `text`) and by the cells that
change, not every cell every tick: a simulation over every cell goes
to Rust. Braille: lines and points (thousands of dots a tick) are cheap;
every dot of a frame each tick is at the bound.

## Tests

`rust/ptyrc-box/tests/script.rs`, through `headless::run`: the API,
the errors, the hostile scripts (loops, memory, recursion, a module
named as the API, the natives asked for, imports outside the tree,
reserved members as arguments, floods, escape bytes), `manifest()`,
the example twice. Where the interpreter is not built, they pass and a line `script
tests SKIPPED` says so. `sdk/rust/tests/tests/host.rs`: `textWithMtime`
and a write guarded by it, `until`, as their Rust twins.

`rust/ptyrc-cli/tests/dev.rs`: `ptyrc dev new`, `check`, `test`, `run`
on the template; every example passes `dev check`, `dev test` and
`dev test --unit`: the modules `"ptyrc/canvas"`, `"ptyrc/sight"` are
tested by `examples/dots/tests.js` (`test_canvas.py`, the `Sight` of
`_cli.py`), the deck's handlers by `examples/clockface/tests.js`.

## Not done

- The compiled module is not kept between starts: each start compiles
  it (see Speed).
- A promise rejected and never awaited is silent.
