# ptyrc-box

The runner of store apps: one WebAssembly guest, its limits, its
canvases. `plan/store-model.md` is the plan; this file is the ABI.

```
ptyrc-box [--script] MODULE [ARGS]
ptyrc-box --headless [--script] MODULE [ARGS] [--size CxR] [--for T]
          [--at T,...] [--seed N] [--tape FILE | --screen FILE] [--json]
          [--grant NAME,...] [--deny NAME,...] [--words WORD,...]
          [--event T:TOPIC:JSON]... [--key T:KEY]...
```

Runs `MODULE` (a binary module; with `--script`, `APP.js`) in the
session of `PTYRC_SOCKET`, presenting `PTYRC_TOKEN`. `ARGS`: the JSON
given to `init`, default `{}`. The log is stderr. SIGTERM: `stop`, then
the end. Status: that of `exit`; 0 after SIGTERM; 70 a guest ended by
the runner; 2 usage; 5 the connection.

`--headless`: no session; `headless::run` (Library). The report on
stdout (`--json`: one JSON value), the log on stderr. `T`: ms, or
`500ms`, `2s`. Defaults: 80x24, 1 s, the end, seed 1. Status: 0, or
that of `exit`, or 70.

- `--grant`: the permissions declared; `--deny`: those answered no;
  `--words`: the words of `words`. Given, the requests are checked as
  the driver checks a store app (`grants.rs`): a reading above the
  degree held is cut to it, marked `"degree"`; an act not held, a
  method closed to apps: `denied`, the hint naming the permission.
  `--deny` alone: every permission but these. Neither: the whole
  session.
- `--event T:TOPIC:JSON`: an event at `T`, to a topic subscribed and
  held. `--key T:KEY`: a key named as `input.send` names it, to the
  app when it grabbed it (`input.grab`); `ctrl+t` releases the grab.

## Guest ABI, version 1

A core module, little-endian, `wasm32`. No WASI, no start function
expected (one runs under the bound of `init`). Every import is a
function of the module `ptyrc` listed below; any other import: the
module does not start (`bad-import`).

All integers are `i32` unless said. A text is `(ptr, len)`: UTF-8
bytes in the guest's memory, no terminator. JSON is a text.

### Exports

| export | signature | |
|---|---|---|
| `memory` | memory | the only one |
| `alloc` | `(len) -> ptr` | `len` bytes, any alignment; 0: none left. Called by the host, also from inside an import |
| `free` | `(ptr, len)` | what `alloc` gave, with its `len` |
| `init` | `(ptr, len)` | once, first. JSON: the arguments and options |
| `tick` | `(now_ms: i64)` | at the period asked; session time since the app started |
| `event` | `(topic_ptr, topic_len, data_ptr, data_len)` | a topic subscribed (`call("subscribe", ...)`); `data`: JSON |
| `stop` | `()` | optional. SIGTERM; 500 ms |

- The host writes an argument with `alloc`, calls the export, then
  calls `free`. The bytes are the guest's to read during the call only.
- The host calls one export at a time; none while another runs.
  Events come between calls, in order.
- An export that traps ends the app. Nothing survives.

### Imports (`ptyrc`)

| import | signature | |
|---|---|---|
| `call` | `(method_ptr, method_len, params_ptr, params_len) -> i64` | a protocol request, answered before it returns |
| `canvas` | `(w, h) -> id` | a buffer of `w * h` cells; 0: refused |
| `cells` | `(id) -> ptr` | its cells; 0: no such canvas |
| `present` | `(id, place_ptr, place_len) -> status` | show the canvas; 0, or 1: refused (the reason is logged) |
| `release` | `(id)` | forget the canvas, remove its picture; the host calls `free` on its cells |
| `period` | `(ms)` | the tick wanted; under 33: 33; 0: none (the default) |
| `log` | `(ptr, len)` | a line of the app's log |
| `random` | `(ptr, len)` | random bytes; `len` at most 65536 |
| `exit` | `(code)` | ends the app; does not return |

`call`

- `params`: a JSON value, or `len` 0 for `{}`.
- The answer is JSON, in memory the host took with `alloc`:
  `(ptr << 32) | len`. The guest frees it. 0: `alloc` gave none.
- `{"result": ...}`, or `{"error": {"code", "message", "hint"?}}`. The
  codes of the protocol, and `limit` (the rate), `gone` (the driver),
  `headless` (a run without a session answers `draw.put`, `update`,
  `remove`, `clear`, `widget.declare`, `show`, `set`, `hide`,
  `subscribe`, `unsubscribe`, `session.info`, `screen.text`,
  `screen.shape`, `input.grab`, `input.release` only).
- Methods and topics: `plan/design.md`, Protocol.

`canvas`, `cells`

- The host allocates the cells with `alloc`, zeroed: `w * h * 16`
  bytes, rows first. The pointer holds until `release`.
- Refused: no cell, or more cells in all than four screens.
- A cell, 16 bytes:

  | bytes | |
  |---|---|
  | 0..4 | the scalar value, `u32`. 0: nothing drawn (the cell below shows). A control character, a value that is no scalar, a glyph of no width: a space |
  | 4..8 | `fg`: a colour |
  | 8..12 | `bg`: a colour |
  | 12..14 | attributes, `u16`: 1 bold, 2 dim, 4 italic, 8 underline, 16 inverse, 32 strike, 64 blink |
  | 14..16 | 0 |

- A colour, 4 bytes: `[0, 0, 0, 0]` that of the cell below;
  `[1, 0, 0, 0]` the terminal's default; `[2, index, 0, 0]` the
  palette; `[3, r, g, b]`.
- A wide glyph is written in its first cell; the next cell is
  skipped. In the last column: a space.
- A space is a hole unless the cell has a `bg`.
- No opacity by cell: `alpha` in the place, for the whole canvas.

`present`

- `place`: a JSON object, or `len` 0 for `{}`: the members of a sprite
  (`plan/design.md`, scene) other than its content: `anchor`, `frame`,
  `z`, `alpha`, `layer`, `blend`, `ttl_ms`, `motion`, ... `id`,
  `content`, `frames`, `loop`, `effect`, `persist`, `rows` are dropped.
- The first time: `draw.put`, the cells as `content.grid` (`plan/design.md`,
  scene), equal cells as one and its repeats. Then `draw.update`, with
  `rows` when a cell changed (the rows changed, trailing holes left
  out), the place when it changed; neither: no request.
- A canvas is one sprite of the app's connection: it dies with it.

### Limits

| | default | past it |
|---|---|---|
| memory | 64 MiB | the growth is refused (`memory.grow`: -1); more at the start: `memory` |
| `tick`, `event` | 50 ms | late; three in a row: paused (no `tick`, no `event`) |
| `tick`, `event` | 2 s | the app ends: `time` |
| `init`, the start function | 2 s | `time` |
| `stop` | 500 ms | `time` |
| `call` | 1000 a second of session time | answered `limit` |
| canvas cells, in all | 4 screens | `canvas` answers 0 |
| log | 1 MiB | dropped |
| a text read by the host | 4 MiB | `bad-pointer` |
| stack | 512 KiB | `stack` |

Time is bounded by epoch interruption (a thread, 10 ms): wall time,
the host's included. A limit met is logged once (`limit: NAME`):
`memory`, `late`, `paused`, `call`, `canvas`, `log`.

How an app ends (`Error::name`): `bad-module`, `bad-import`,
`bad-export`, `memory`, `time`, `bad-pointer` (a pointer or a length
outside the memory), `stack`, `trap`, `exit`, `gone`.

## Script apps

`--script`: the guest is the interpreter of `sdk/js` (`PTYRC_JS`; else
`ptyrc-js.wasm` beside the program; else `../share/ptyrc/` from there;
else, from `target/<profile>/` of the source tree, the module built in
`sdk/js/interp/target/`). The ABI is the same. The scripts of the
directory of `APP.js` (`.js`, `.mjs`; 1 MiB, 512 files) ride in the JSON
of `init`, under two reserved members the runner sets, whatever `ARGS`
holds:

```json
{"ptyrc:sources": {"app.js": "...", "lib/util.js": "..."},
 "ptyrc:main": "app.js"}
```

`script::sources(dir)`, `script::init_args(main, sources, args)`,
`script::interpreter()`. See `sdk/js/README.md`.

## Library

```rust
use ptyrc_box::headless::{Options, run};
let report = run(&wasm, &Options { size: (80, 24), duration_ms: 1000,
    at: vec![0, 500], ..Options::default() })?;
print!("{report}");
```

- `headless::run`: session time an argument, `random` from a seed: the
  same report every time. Lateness is wall time: a guest near 50 ms a
  tick may differ.
- The screen: `Options::under`: `Empty`; `Text` (written from the top
  left, `\n` a line, escapes read); `Tape` (`Tape::parse`, the format
  of `plan/design.md`, Tapes: output and resizes, in session time,
  `t / 1000` ms; its size is the screen's).
- Drawn by the scene and the widgets of `ptyrc-core`, as the driver
  draws: canvases, text sprites (`rows`, `cells`), `frames`, anchors,
  layers (behind the text by default), `ttl_ms`, `motion`, the
  effects and borders of the driver's draw extensions
  (`plugins/draw-ext`, `headless::draw_ext`), widgets (laid out once
  the screen is still, under a second). An unknown effect: the hint
  lists the known. Not: `draw.ext`, the pop-up, other clients.
- `random`: xorshift64 from the seed mixed (splitmix64): each seed its
  own.
- `Report`: the screens at each time as text (one string a row,
  trailing spaces cut), the requests (`refused`: the code; `lowered`:
  the degree; `content`, `frames` and `rows` left out), the log, the
  limits met, the end, the permissions lacking (`lacks`: denied, or
  not declared with the line of the manifest that would ask for it),
  what was injected and reached nobody (`dropped`). `Report::to_json`.

The size. `session.info` answers `{"protocol": 1, "server",
"size": [columns, rows], "headless": true}` (headless: `"server":
"ptyrc-box"`), as the driver answers at the degree `none`. A change:
the event `size`, `{"size": [columns, rows]}`, to an app subscribed
(`call("subscribe", {"topics": ["size"]})`); headless, at each resize
of the tape.
- `live::run`: a session (`Live::new(Client, log)`), real time times
  `PTYRC_CLOCK_RATE`.
- `Runner<H: Host>`: `load`, `init`, `tick`, `event`, `stop`; `Host` is
  the outside (`call`, `size`, `log`, `random`).
- `engine.rs` alone names wasmtime. One engine a process, its epoch
  advanced by one thread; a module of 256 KiB and more is compiled
  once a process (the last four are kept).

## Speed

`tests/live.rs` (ignored; needs `target/release/ptyrc`): a 200x60
canvas repainted whole, 30 Hz, a headless driver. Measured 2026-11-19:

| | driver, a frame | runner, a frame |
|---|---|---|
| | content as JSON runs, whole | `grid`, rows changed |
|---|---|---|
| one colour: driver | 2.1 ms | 2.5 ms |
| one colour: runner | 0.6 ms | 0.3 ms |
| a colour a cell: driver | 6.9 ms | 3.9 ms |
| a colour a cell: runner | 13.8 ms | 1.5 ms |

Processor time a frame, 90 frames. 30 Hz held. The bound of the plan
(2 ms for the driver) is not. Where the driver's time goes, `grid`,
timed inside it, one colour (a colour a cell):

| | |
|---|---|
| read, JSON | 0.35 ms |
| `draw.update`: base64, the canvas | 0.7 ms |
| `compose`, a back layer | 1.2 ms |
| `repair`, to the host | 0.4 ms (1.6 ms) |

Before, as JSON runs: the runs built and encoded took the runner's
time; read and parsed, most of the driver's. The framing is no
longer the cost: what is left is the composition of the whole screen
and its repair, at each frame, whatever changed.

## Tests

- `tests/abi.rs`: the ABI against small guests.
- `tests/hostile/`: guests that must end contained.
- `tests/live.rs`: above.
- `tests/script.rs`: script apps and hostile scripts. They need the
  interpreter: `cargo build --release --manifest-path
  sdk/js/interp/Cargo.toml`. Without it they pass, testing nothing,
  and a line `script tests SKIPPED` is printed past the capture;
  `PTYRC_REQUIRE_JS=1`: they fail.

Guests are text modules (`tests/common`: the imports, a bump
allocator).

## Not done

- Paused: nothing resumes it but `Runner::resume`; the manager is not
  told.
- The compiled interpreter is not kept on disk (1 s a start, 20 s in
  a debug build): `Module::deserialize` is `unsafe`, and `unsafe` is
  kept to `ptyrc-server`.
- `wasmi` where wasmtime does not build.
- The OS profile (milestone 6).
