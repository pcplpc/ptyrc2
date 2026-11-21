# ptyrc-box

The runner of store apps: one WebAssembly guest, its limits, its
canvases. `plan/store-model.md` is the plan; this file is the ABI.

```
ptyrc-box [--profile | --no-profile] [--manifest JSON] [--assets DIR] [--seed N] [--script] MODULE [ARGS]
ptyrc-box --headless [--profile | --no-profile] [--manifest JSON] [--assets DIR] [--script] MODULE [ARGS]
          [--size CxR] [--for T] [--at T,...] [--seed N]
          [--tape FILE | --screen [T:]FILE...] [--json] [--styles] [--timing]
          [--grant NAME,...] [--deny NAME,...] [--words WORD,...]
          [--event T:TOPIC:JSON]... [--key T:KEY]... [--pointer T:X,Y]...
          [--click T:X,Y]... [--press T:X,Y]... [--release T:X,Y]...
          [--focus T:TAG]... [--paste T:TEXT]... [--resize T:CxR]...
          [--clock ISO] [--zone NAME] [--storage DIR] [--storage-out FILE] [--file NAME=PATH]...
          [--write-file NAME=PATH]... [--probe git|system=FILE.json]...
          [--user-word WORD]... [--user-pattern TEXT]... [--user-regex RE]...
          [--front | --back] [--prefer P] [--alpha A] [--border B]
          [--surface widget|frame|screen]
ptyrc-box --unit FILE [--profile | --no-profile] [--manifest JSON] --script APP.js [ARGS]
```

Runs `MODULE` (a binary module; with `--script`, `APP.js`) in the
session of `PTYRC_SOCKET`, presenting `PTYRC_TOKEN`. `ARGS`: the JSON
given to `init`, default `{}`. The log is stderr; once `init`
returned, the line `ptyrc-box: init done` (`ptyrc app` waits for it).
SIGTERM: `stop`, then the end. `--seed N`: `random` drawn as a
headless run of seed `N` draws it; absent, from `/dev/urandom`.
Status: that of `exit`; 0 after SIGTERM; 70 a guest ended by the
runner; 2 usage; 5 the connection, or the OS profile refused.

`--manifest JSON`: the manifest as its reader (the driver, `ptyrc dev
test`) read it: handed to `init` under `ptyrc:manifest`; its
`[limits]` applied as far as `granted` (headless: the grants) holds
`limit-memory`, `limit-time`. `frame`: the app's frame at the start,
given by the driver (headless: 0, raw); told as given from `layout`
up the ladder (`shape`, `words`, `text` above it), else 0.

`--assets DIR`, live and headless: the tree `asset.list`, `asset.read`
answer (Imports, `call`).

`--unit FILE --script APP.js`: the unit tests of a script app. `FILE`,
a script of the tree of `APP.js` by relative path; its exports
`test_*` run in order, each in an `event` under the limits of one but
time: 10 s a test, 60 s the file (`script::UNIT_TEST`, `UNIT_FILE`).
No session, no `--headless`; `call` refused. The report on stdout;
status 0 when all passed, else 1 (`script::unit`).

`--headless`: no session; `headless::run` (Library). The report on
stdout (`--json`: one JSON value), the log on stderr. `T`: ms, or
`500ms`, `2s`. Defaults: 80x24, 1 s, the end, seed 1. Status: 0, or
that of `exit`, or 70.

- `--tape FILE`: a tape played under the app (`Under::Tape`); it gives
  the size, so no `--size`, no `--screen`. `--screen [T:]FILE`, again
  for each change: a text file as the program's screen from `T` on
  (default 0; `Under::Screens`).
- `--styles`: under each screen, a grid of its styles: a letter per
  style that is not the default, and the legend (`Report::text`).
- `--timing`: the wall time of each `tick` and `event` entered, host
  calls in it (what the bound `tick-ms`, 50 ms, measures). A line for
  each call over the bound, in order, `slow at T ms: tick, X ms` or
  `slow at T ms: event TOPIC, X ms` (`T` session time), then a last
  line, `ticks: N; a tick, wall time: mean M ms, max X ms; events: E,
  max Y ms` (the events part when one came). A slow call: status 1,
  unless the app's own is not 0. Not in the report otherwise: it
  differs from run to run, and the report is compared whole (`ptyrc
  dev test --cases`). `--json` has it always: `ticks`, `tick_ms`
  (`{mean, max}`, null without a tick), `events`, `event_ms` (`{max}`,
  null without one), `slow` (`[{t, call, topic, ms}]`, `topic` of an
  event); the status as above with `--timing`. A measure of the
  machine: run alone, on a quiet machine, with the release runner.
- `--grant`: the permissions declared; `--deny`: those answered no;
  `--words`: the words of `words`. Given, the requests are checked as
  the driver checks a store app, by the same code (`ptyrc-rules`,
  `grants`, `lower`): a reading above the degree held is cut to it,
  marked `"degree"`; an act not held, a method closed to apps:
  `denied`, the hint naming the permission; a method the driver's
  table does not list (`ptyrc_rules::grants::METHODS`): `denied`, `not
  open to apps`, as live; `asset.*`, the runner's, open. `draw.list`
  below `text`: where an anchor landed told at the degree that finds
  it (`lower::landing`). `screen.text`, `screen.cells {sprites:
  true}`: the app's own sprites composed alone. `screen.text`, `screen.shape`, `screen.cells`: the
  view of `frame` and `rect`, as the driver reads it.
  `--deny` alone: every permission but these. Neither: the whole
  session. `--user-word`: a word the user gave, found at any degree,
  shown from `shape`; `--user-pattern`, `--user-regex`: a pattern the
  user gave, `screen.find`, `screen.wait` of exactly it answered as
  under `text` (`lower::plan`).
- `--front`, `--back`, `--prefer`, `--alpha`, `--border`: the
  launcher's options, on the app's requests as the driver sets them
  (`own_params`).
- `--surface`: `[app] surface` (`ptyrc dev test` gives it), on the one
  frame (`raw`); with the grants, a draw is clipped as the driver
  clips it (`ptyrc-rules`, `surface::clip`): `widget`, a bare sprite
  on `NOWHERE` (not shown, a note), a widget as named; `frame`, and
  `screen` without `screen-wide`, on the frame; never refused.
- Host functions from memory (`hostfn.rs`), the driver's answers.
  `--clock ISO`: the wall time at 0 (default `2000-01-01T00:00:00Z`),
  advancing with session time. `--zone NAME`: the local zone (a name
  of the system database, or a POSIX rule; default `UTC`); the
  database is read whole before the profile when `clock` is held;
  else the zone alone, checked, unused (a note).
- `--storage DIR`: the values, a file a key: its name the key
  (`[A-Za-z0-9._-]`), its content the value (UTF-8); other entries
  skipped; read once, never written back. `--storage-out FILE`: the
  storage at the end, one JSON object (key: value), FILE opened before
  the profile (`ptyrc dev test --storage-out DIR` writes DIR from it).
- `--file NAME=PATH`: a file or a directory read once, its handle
  (`f0`, ...) given to `init` under `NAME` (a handle there already is
  kept); read only. `--write-file NAME=PATH`: the same, the manifest's
  `write = true`: writes, with `files-write`, stay in memory.
- `--probe git=FILE.json`, `system=`: the answer as written (`ptyrc
  dev probe git [DIR]`, `ptyrc dev probe system` print the driver's
  probe now: a fixture); none: the not-held answer, and a note.
  `probe.git`, as the driver: one read a second of session time;
  within it the same params (but `since`) get the last answer, others
  `not-usable`.
- The report, beside the screens: `result:`, `storage:` (a line a key: written by
  the app, `KEY = "VALUE"` or `KEY removed`; given and not written,
  `KEY = "VALUE" (given, not written)`; a value cut past 60
  characters, the size then given; `storage` not held, one line: the
  storage given not used), `files:` (written); `--json`: the values
  whole (`storage` at the end, `storage_given`, `storage_written`,
  `storage_held`), the contents. Status: that of `app.result` when the
  run ends well.
- `--event T:TOPIC:JSON`: an event at `T`, to a topic subscribed and
  held. `--key T:KEY`: a key named as `input.send` names it, to the
  app when it grabbed it (`input.grab`); `ctrl+t` releases the grab;
  else to the program, heard as the driver's: `input {"bytes"}`
  (base64) to the app subscribed holding `keys-read`, else `dropped`.
- `--pointer T:X,Y`, `--click T:X,Y`: `hover`, `mouse` on the app's
  sprite at the cell (topmost, no effect), heard on its widget or a
  sprite in front, else with `pointer`, `clicks`. `--press T:X,Y`,
  `--release T:X,Y`: the left button down, up (`--click`: both at
  once); a release after a press on a sprite is of that sprite, `at`,
  `frame`, `pos` null off it. An effect put with `"pointer": true`,
  `"bomb": true` in its params is tuned as the driver tunes it
  (`ptyrc-rules`, `nozzle`): `hover`, `bomb` `[x, y, ms]` or `[x, y,
  ms, dx, dy]`; no permission; a press on it without `clicks-grab`:
  `lacks`.
- `--focus T:TAG`: its widget that asks is focused; keys then go to it
  (`{"key", "bytes", "widget"}`), its escape and close keys end it
  (`released`). `--paste T:TEXT`: `{"widget", "paste"}`; under a grab,
  key by key, the rest to the program; else to the program,
  bracketed. `--resize T:CxR`: the host resized, as a tape's resize:
  the screen first, then `size`.

## OS profile

`profile::apply`: the third wall (plan, Model). Live: once the module
is read, compiled and instantiated, and the socket open; before `init`.
Headless: once the files (`--tape`, `--screen`, `--assets`,
`--storage`, `--file`, `--probe`, the zones) are read; the report
goes to stdout, open already. Not undone. Linux, every thread:

- Landlock, every right handled, no rule: no file opened, created,
  executed; ABI 4 on, no TCP bind or connect; ABI 6 on, no abstract
  unix socket, no signal outside the process; ABI 8 on, every thread
  (before: the calling thread only).
- `PR_SET_NO_NEW_PRIVS`.
- seccomp. Allowed: `read`, `write`, `recvfrom`, `sendto`, `recvmsg`,
  `sendmsg` and their vectors, `close`, `futex`, memory (`mmap`,
  `mprotect`, `mremap`, `madvise`, `brk`, `munmap`, `membarrier`),
  time, sleeps and `restart_syscall`, `sched_yield`,
  `sched_getaffinity`, signals to itself (`rt_sig*`, `sigaltstack`,
  `tgkill` of its own pid), `getrandom`, `exit*`, threads (`clone`
  with `CLONE_THREAD`; `clone3`: `ENOSYS`), `setsockopt`
  `SO_RCVTIMEO`/`SO_SNDTIMEO`, `fcntl F_GETFD`, `prctl
  PR_SET_NAME`/`PR_GET_NAME`. The process killed: `execve*`, `ptrace`,
  `process_vm_*`, `process_madvise`, `pidfd_getfd`, `io_uring_*`,
  `bpf`, `perf_event_open`, `userfaultfd`, keys, namespaces, mounts,
  `open_by_handle_at`, modules, `kexec*`, `reboot`, `fork`, `vfork`:
  no path of the runner makes them. The rest: `EPERM` (`open*`,
  `socket`, `connect`, `kill`, ...): std and the engine probe some of
  them on a cold path (cgroups, a panic), and a probe must fail, not
  end the app.

What applies is logged (`profile: ...`, live), what the kernel lacks
always (`profile: MISSING: ...`): the profile is best effort.
`PTYRC_PROFILE=full`: refused instead (status 5). `--no-profile`: none,
for debugging, said on stderr. Elsewhere than Linux: none, said.

The engine makes no memfd (`memory_init_cow(false)`): a module may be
compiled under the profile (headless, the tests). `tests/profile.rs`:
the profile from host code; `tests/abi.rs`, `tests/hostile/`,
`tests/script.rs` run under it.

## Guest ABI, version 1

A core module, little-endian, `wasm32`. No WASI, no start function
expected (one runs under the bound of `init`). Every import is a
function of the module `ptyrc` listed below; any other import: the
module does not start (`bad-import`). A module imports what it uses:
an import added to the list (`sprite`) leaves a module built before
it as it was; a module that imports it does not start on a runner
that lacks it.

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
| `sprite` | `(id) -> i64` | the id of the canvas's sprite (that of `mouse`, `hover`); -1: not presented, or no such canvas |
| `release` | `(id)` | forget the canvas, remove its picture; the host calls `free` on its cells |
| `period` | `(ms)` | the tick wanted; under 33: 33; 0: none (the default). A period counts from its call |
| `log` | `(ptr, len)` | a line of the app's log |
| `random` | `(ptr, len)` | random bytes; `len` at most 65536 |
| `exit` | `(code)` | ends the app; does not return |

`call`

- `params`: a JSON value, or `len` 0 for `{}`.
- The answer is JSON, in memory the host took with `alloc`:
  `(ptr << 32) | len`. The guest frees it. 0: `alloc` gave none.
- `{"result": ...}`, or `{"error": {"code", "message", "hint"?}}`. The
  codes of the protocol, and `limit` (the rate), `gone` (the driver),
  `headless` (a run without a session answers only `headless::SERVED`
  and the host functions, `hostfn::is_host`).
- `asset.list`, `asset.read`: answered by the runner from the tree of
  `--assets DIR`, read whole before the profile (8 MiB at most; regular
  files, links not followed, names by the rule of `assets::valid_name`);
  none given, an empty tree. Never sent to the driver; in a `batch`,
  `bad-request`.
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
  Its id, for `mouse` and `hover`: `sprite`; a sprite removed by the
  driver and put again by `present` has a new one.

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
| stack | 1 MiB | `stack` |

Time is bounded by epoch interruption (a thread, 10 ms): wall time,
the host's included. Headless, lateness is session time, which a call
does not take: no `late`, no pause, whatever the load
(`Runner::untimed`); the wall time of a call, and a call over the
bound, are `--timing`'s. The bounds that end the app (`time`) stay
wall time. A limit met is logged once (`limit: NAME`):
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
  same report every time. The scene and the widgets step when the
  driver would (`Headless::next_wake`), between the times asked: the
  screen at `T` does not depend on the other times of `at`. A step
  follows `init`. Topic `idle` as the driver's `watch_idle`; a screen
  nothing was written on (`Empty`) is idle from the start. Never
  `late` nor paused (Limits).
- The screen: `Options::under`: `Empty`; `Text` (written from the top
  left, `\n` a line, escapes read); `Screens` (texts as `Text`, each
  from its time on: the screen cleared, the text written, the topics
  told as the driver tells them); `Tape` (`Tape::parse`, the format of
  `plan/design.md`, Tapes: output and resizes, in session time, `t /
  1000` ms; its size is the screen's).
- Drawn by the scene and the widgets of `ptyrc-core`, as the driver
  draws: canvases, text sprites (`rows`, `cells`), `frames`, anchors,
  layers (behind the text by default), `ttl_ms`, `motion`, the
  effects and borders of the driver's draw extensions
  (`plugins/draw-ext`, `headless::draw_ext`), widgets (laid out as the
  driver lays them out, `plan/design.md`, Layout: on cells still for
  1 s, none before; off the cursor's row and a row each side; a cell
  from the edges of the view; a larger variant taken after 2 s of
  looks; focused: laid out again, over the text if no room holds
  it). An unknown effect: the hint lists the known. Not: `draw.ext`,
  the pop-up, other clients.
- `random`: xorshift64 from the seed mixed (splitmix64): each seed its
  own.
- `Report`: the screens at each time as text (one string a row,
  trailing spaces cut), the requests (`refused`: the code; `lowered`:
  the degree; `content`, `frames` and `rows` left out), the styles of
  each screen (`[y, x, w, style]`), the log, the limits met, the end,
  the permissions lacking (`lacks`: refused or lowered, declared or
  not, with the line of the manifest that would ask for it), what was
  injected and reached nobody (`dropped`), `notes` (a screen file that
  wraps or scrolls; a widget hidden at a screen shown past 1 s: its
  smallest variant, the screen's size). `Report::to_json`,
  `Report::text(styles)`; without styles, a note when a screen holds
  `inverse`, `hidden` or `underline` cells. `Report::timing`: the wall
  time of the ticks and events, the calls over `tick-ms`
  (`Timing::slow`; `Timing`: equal to any other, so that two runs
  compare equal), `Report::timing_line`, `Report::slow_lines`.
- `live::run`: a session (`Live::new(Client, log)`, `.seeded(N)`), real
  time times `PTYRC_CLOCK_RATE`. Ticks are due as headless
  (`ticks::Ticks`): a period asked, changed or not, counts from its
  call; none asked, each tick one period after the last was due; a
  tick missed is not made up for.
- `Runner<H: Host>`: `load`, `init`, `tick`, `event`, `stop`; `Host` is
  the outside (`call`, `size`, `log`, `random`).
- `engine.rs` alone names wasmtime. One engine a process, its epoch
  advanced by one thread; a module of 256 KiB and more is compiled
  once a process (the last four are kept).

### Session

`hello`. The guest gets none: no export, no event. Live, the runner
reads the driver's `hello` at connect (cut below `text`: `protocol`,
`server`, `size`, `headless`) and keeps its `size` (80x24 when absent)
as the screen of the canvas bound. Headless, there is no connection;
`session.info` answers `{"protocol": 1, "server": "ptyrc-box", "size":
[columns, rows], "headless": true}`, as the driver at the degree
`none`. A guest asks `session.info` for the size at `init`.

The event `size`, `{"size": [columns, rows]}`, to a guest subscribed
(`call("subscribe", {"topics": ["size"]})`). Live: from the driver at
each resize of the host, the runner's bound updated before the guest
sees it; with `snapshot: true`, the size now, after the answer.
Headless: at each resize of the tape or `--resize`, the screen
resized first;
`--event T:size:JSON` delivers the data alone, the screen unchanged;
`snapshot` is ignored.

## Speed

`tests/live.rs` (ignored; needs `target/release/ptyrc`): a 200x60
canvas repainted whole, 30 Hz, a headless driver; it asserts the
bound of the plan, one colour: 2 ms of the driver a frame
(`PTYRC_SPEED_SLACK`: a factor on it, for a loaded machine). Processor
time a frame, 90 frames, 30 Hz held; 2026-11-19, the two drivers run
in turn on one machine (16 cores, load 10 to 20):

| | before | now |
|---|---|---|
| one colour: driver | 2.44 ms | 1.67 to 1.78 ms |
| a colour a cell: driver | 3.9 to 4.0 ms | 3.0 to 3.3 ms |
| runner, one colour (a colour a cell) | 0.33 ms (1.5 ms) | the same |

On a quieter machine (runner 0.22 ms): 1.44 ms and 2.8 ms. The bound
holds for one colour; a colour a cell is over it.

Inside the driver (`ptyrc-server`, `speed_200x60`, ignored: the
median a frame, in process), one colour (a colour a cell):

| | |
|---|---|
| `draw.update`: base64, the canvas | 0.25 ms (0.46 ms) |
| tick, compose, repair | 0.80 ms (1.80 ms) |
| of which compose, all rows changed | 0.52 ms |
| one cell changed: `draw.update`, paint | 0.005 ms, 0.09 ms |

The composition is kept (`scene::Composer`) and only the rows that
changed are composed again: one cell changed costs a row, not the
screen (0.009 ms against 0.6 ms). What is left with a colour a cell
is the repair: some 200 KB of SGR a frame, written and fed to the
model of the host (1.3 ms of the 1.8).

2026-11-20, a whole CSI parsed in one pass and the colours of an SGR
written from a table (`ptyrc-vt`), the two drivers in turn (load 2):
a colour a cell, 2.89 to 3.00 ms against 3.33 to 3.44 ms; one colour
unchanged.

## Tests

- `tests/abi.rs`: the ABI against small guests.
- `tests/hostile/`: guests that must end contained.
- `tests/live.rs`: above.
- `tests/ticks.rs`: a live run (a driver of the test's own) against
  headless: the tick times.
- `tests/profile.rs`: the OS profile.
- `tests/script.rs`: script apps and hostile scripts. They need the
  interpreter: `sdk/js/interp/build.sh`. Without it they pass, testing nothing,
  and a line `script tests SKIPPED` is printed past the capture;
  `PTYRC_REQUIRE_JS=1`: they fail.

Guests are text modules (`tests/common`: the imports, a bump
allocator).

## Not done

- Paused: nothing resumes it but `Runner::resume`; the manager is not
  told.
- The compiled interpreter is not kept on disk: each start compiles
  it (2026-11-20, 16 cores, load 60: 0.4 s of wall time, 3.3 s of
  processor time, in a release build; 1.1 s, 12 s in a debug one).
  `Module::deserialize` is `unsafe`: allowed in `ptyrc-box`, one
  module, for this cache (owner), once measured worth it.
- `wasmi` where wasmtime does not build.
- The OS profile: macOS; W^X after the drop (live, no `PROT_EXEC`
  once compiled); the driver setting `PTYRC_PROFILE=full`.
