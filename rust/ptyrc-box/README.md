# ptyrc-box

The runner of store apps: one WebAssembly guest, its limits, its
canvases. `plan/store-model.md` is the plan; this file is the ABI.

```
ptyrc-box MODULE [ARGS]
```

Runs `MODULE` (a binary module) in the session of `PTYRC_SOCKET`,
presenting `PTYRC_TOKEN`. `ARGS`: the JSON given to `init`, default
`{}`. The log is stderr. SIGTERM: `stop`, then the end. Status: that of
`exit`; 0 after SIGTERM; 70 a guest ended by the runner; 2 usage; 5 the
connection.

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
  `headless` (a run without a session answers `draw.*`, `subscribe`,
  `unsubscribe`, `session.info`, `screen.text`, `screen.shape` only).
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
  `content`, `frames`, `loop`, `effect`, `persist` are dropped.
- The first time: `draw.put`. Then `draw.update`, with the content when
  a cell changed, the place when it changed; neither: no request. The
  content is sent whole (see Speed).
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

## Library

```rust
use ptyrc_box::headless::{Options, run};
let report = run(&wasm, &Options { size: (80, 24), duration_ms: 1000,
    at: vec![0, 500], ..Options::default() })?;
print!("{report}");
```

- `headless::run`: an empty screen, session time an argument, `random`
  from a seed: the same report every time. `Report`: the screens at
  each time (canvases with an anchor `at`; another anchor: the origin),
  the requests (`refused`: the code), the log, the limits met, the end.
  Lateness is wall time: a guest near 50 ms a tick may differ.
- `live::run`: a session (`Live::new(Client, log)`), real time times
  `PTYRC_CLOCK_RATE`.
- `Runner<H: Host>`: `load`, `init`, `tick`, `event`, `stop`; `Host` is
  the outside (`call`, `size`, `log`, `random`).
- `engine.rs` alone names wasmtime.

## Speed

`tests/live.rs` (ignored; needs `target/release/ptyrc`): a 200x60
canvas repainted whole, 30 Hz, a headless driver. Measured 2026-11-19:

| | driver, a frame | runner, a frame |
|---|---|---|
| one colour (60 runs) | 2.1 ms | 0.6 ms |
| a colour a cell (12000 runs) | 6.9 ms | 13.8 ms |

30 Hz held in both. The bound of the plan (2 ms) is not: the content
goes as JSON runs, whole. Wanted of the protocol: a binary content,
rows patched alone. Nothing changes for the guest.

## Tests

- `tests/abi.rs`: the ABI against small guests.
- `tests/hostile/`: guests that must end contained.
- `tests/live.rs`: above.

Guests are text modules (`tests/common`: the imports, a bump
allocator).

## Not done

- Paused: nothing resumes it but `Runner::resume`; the manager is not
  told.
- A screen under the headless run (a tape).
- `wasmi` where wasmtime does not build.
- The OS profile (milestone 6).
