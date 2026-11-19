# sdk/js

The script tier of store apps (`plan/store-model.md`, Scripts): a
JavaScript interpreter that is itself a guest of ABI 1
(`rust/ptyrc-box/README.md`), and the API scripts are written against.

| | |
|---|---|
| `ptyrc.d.ts` | the API, typed: the whole of it. An app is written from this file |
| `interp/` | the interpreter guest (`ptyrc-js`): Boa, the module `"ptyrc"` (`src/prelude.js`), the loader |
| `examples/confetti/` | an app |
| `../template/` | what `ptyrc dev new` writes (embedded in `ptyrc`) |

## Build

```
rustup target add wasm32-unknown-unknown
cargo build --release --manifest-path sdk/js/interp/Cargo.toml
```

The module: `sdk/js/interp/target/wasm32-unknown-unknown/release/ptyrc_js.wasm`,
found there by a `ptyrc-box` of `target/<profile>/`.
It is not in the repository: built with a release, not on the user's
machine. `interp/` is a workspace of its own; its `Cargo.lock` is kept.

The runner looks for it (`ptyrc_box::script::interpreter_path`): the
file named by `PTYRC_JS`; else `ptyrc-js.wasm` beside the program; else
`../share/ptyrc/ptyrc-js.wasm` from there.

```
PTYRC_JS=.../ptyrc_js.wasm ptyrc-box --script examples/confetti/app.js '{"rate": 30}'
```

## The interpreter

Boa 0.22 (`boa_engine`, MIT or Unlicense: compatible with the GPL), no
default feature. QuickJS, the plan's first choice, wants a C library
for WebAssembly: the build has `clang --target=wasm32` and `wasm-ld`,
no `libc` (2026-11-19). Boa is Rust alone.

The module imports nine functions of `ptyrc` and nothing else: no
WASI, no `env`, no binding glue.

- The clock of the engine is session time: `Date.now()` is the `now`
  of the last `tick`, 0 at the start.
- Random numbers (`Math.random`, the engine's own) come from the
  import `random`: a seed in a test run.
- No timer, no `fetch`, no `console`: the names are not defined; the
  error says what to use.

Size: 6.2 MiB (`opt-level = 3`; 3.8 MiB at `s`, slower).

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
- The interpreter takes them out; the script's `init(args)` gets the
  rest.

## Modules

- `"ptyrc"`: the API. No file answers to that name: a file `ptyrc.js`
  is `"./ptyrc.js"`.
- `"./x.js"`, `"../lib/y.js"`: files of the tree, by path relative to
  the importing file. Outside the tree, absolute, a URL: refused.
- `"ptyrc:native"`, the host functions under the API, is served to the
  API alone, before any file of the app is read.

## Errors

A line of the app's log: `file:line:column: what`, the frames of the
app, the cure when known.

```
app.js:1:24: ReferenceError: setTimeout is not defined
    at init (app.js:1:24)
  hint: no timer: ask a period (period(50)) and count in tick(now)
```

A syntax error has its own line and column. A runtime error has those
the engine gives: of the function it was thrown in, or of the call
(Boa 0.22), not always of the statement.

- In a module or in `init`: the app ends, status 1.
- In `tick`, `event`, `stop`: a `PtyrcError` (a request refused) is
  logged once and the app goes on; any other ends it.

## Limits

The runner's. A loop without end: `time` (the epoch). Memory without
end: the growth is refused at 64 MiB, the script gets a `RangeError`
or the interpreter stops (`trap`). Recursion: a `RuntimeLimitError` of
the engine, or `stack`.

## Speed

Measured 2026-11-19, a 200x60 canvas, 30 Hz, a headless run, release
(`cargo test --release -p ptyrc-box --test script speed -- --ignored
--nocapture`):

| a tick that | costs |
|---|---|
| sets every cell, `set` 12000 times, a colour a cell | 63 ms (the runner's diff included): over the bound of a tick |
| fills 60 rows, `fill` 60 times | 0.5 ms |
| does 100000 steps of integer arithmetic | 15 ms |

- A `set`: about 4 µs. A step of a loop: about 150 ns: a hundred
  times native, more than the plan's guess for QuickJS (tens).
- A start: 1.0 s the first time in a process (Cranelift compiles the
  module, in parallel), 3 ms after it. A runner is one process an app:
  each app pays the second until the compiled module is kept on disk.

A script paints by runs (`fill`, `text`) and by the cells that
change, not every cell every tick: a simulation over every cell goes
to Rust.

## Tests

`rust/ptyrc-box/tests/script.rs`, through `headless::run`: the API,
the errors, the hostile scripts (loops, memory, recursion, a module
named as the API, the natives asked for, imports outside the tree,
reserved members as arguments, floods, escape bytes), the example
twice. Where the interpreter is not built, they pass and a line `script
tests SKIPPED` says so.

`rust/ptyrc-cli/tests/dev.rs`: `ptyrc dev new`, `check`, `test`, `run`
on the template; every example passes `dev check` and `dev test`.

## Not done

- The compiled module is not kept between starts: each start compiles
  it (see Speed).
- A promise rejected and never awaited is silent.
- The examples `sand`, `snake`.
