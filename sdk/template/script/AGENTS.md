# {{id}}: a ptyrc store app

A script that draws over a terminal program wrapped by ptyrc. It runs
in a sandbox (WebAssembly): no path, no network; the files it reads
are those the user names, the wall clock is asked.

## Files

| | |
|---|---|
| `ptyrc-app.toml` | the manifest: id, version, entry, `[options]`, `[permissions]`; replace each `TODO` |
| `PROTOCOL.md` | the methods, topics, permissions, manifest and limits |
| `app.js` | the app; it may import `./other.js` files of this tree |
| `ptyrc.d.ts` | the API, typed: the whole of it. Read it first |
| `test.sh` | every case, then the unit tests; status 1 when one fails |
| `tests/CASE.args`, `tests/CASE.txt` | a case: the words after `ptyrc dev test .`, and its report |
| `tests.js` | unit tests: each export `test_*` (`ptyrc dev test --unit`) |
| `LICENSE.txt` | GPL-3.0-or-later; the only licence accepted |

## The API

```js
import { call, Canvas, period, log, random, exit, style, rgb } from "ptyrc";
export function init(args) {}      // once; args: the [options], parsed
export function tick(now) {}       // every period(ms); now: ms since start
export function event(topic, data) {}  // topics asked: call("subscribe", {topics: ["size"]})
export function stop() {}          // optional; 500 ms to end
```

- `new Canvas(w, h)`: cells; `set`, `text`, `fill`, `clear`;
  `present({anchor: {at: [x, y], space: "view"}})` shows it, again
  after a change. A cell never set is a hole. Negative `at`: the right
  or bottom edge of the canvas from the right or bottom, `-1` the last
  cell.
- A widget (a small window the driver places, in the size that fits):
  `import { Deck } from "ptyrc/deck"`; `deck.add(tag, {big: [17, 3],
  small: [5, 1]}, (w, h) => rows or a Canvas, {key(name) {}})`; give it
  every `tick(now)` and `event(topic, data)`. `ptyrc.d.ts`, "ptyrc/deck".
- Host functions (`ptyrc.d.ts`): `now()`, `zone(name, at)`,
  `civil(unixMs, offsetS)` (permission `clock`), `until(unixMs,
  everyMs)` (a tick on the second: `PROTOCOL.md`); `storage.get`,
  `.set`, `.remove`, `.keys` (strings; `storage`); `asset(name)`,
  `assetBytes`, `assets()` (`[app] assets`); `file(args.opt).text()`,
  `.bytes()`, `.textWithMtime()`, `.stat()`, `.write(text, {ifMtime})`,
  `.list()`, `.child(name)` (an option of type `file` or `dir`);
  `result(value, status)`.
- A write that must not clobber a change made since the read: `const
  {text, mtimeMs} = f.textWithMtime()`, then `f.write(edited, {ifMtime:
  mtimeMs})`; changed since: `PtyrcError` `not-usable`.
- `call(method, params)`: a request of the protocol (`PROTOCOL.md`);
  throws a `PtyrcError` with `code` and `hint` when refused.
- The size: `call("session.info").size` is `[columns, rows]`; then
  `call("subscribe", {topics: ["size"]})` and `event("size", {size: [c, r]})`.

## Rules

- No timer, no promise across calls, no `fetch`, no `console`:
  `tick` is the only clock. `Date.now()` is session time.
  `Math.random()` comes from the host (seeded in a test).
- Limits: `tick`, `event` 50 ms (three late in a row: paused), 2 s
  (ended); `init` 2 s; memory 64 MiB; `call` 1000 a second; all
  canvases four screens of cells; scripts 1 MiB (`PROTOCOL.md`).
- Sprites go behind the program's text unless `layer: "front"` and
  `front` held; headless as the driver.
- `screen.*` reads the program's cells, never a sprite. Subscribe to
  `text` or `screen` and read on the event; headless sends them when
  the screen changes (a tape, `--screen T:FILE`).
- Keep the run deterministic: everything from `now`, `args` and
  `Math.random()`.

## Commands

```
ptyrc dev check            the manifest, the licence, the sizes: what the store checks
./test.sh                  each case, then dev test --unit
./test.sh --update         rewrites each tests/CASE.txt: after a change meant to change it
ptyrc dev test --unit [FILE]   the test_* exports of tests.js (or FILE): ok/FAIL, file:line:col
ptyrc dev test . --size 80x24 --for 3s --at 0,1s,3s [--screen F | --tape F] [--seed N] [--NAME VALUE]
ptyrc dev test . --deny shape --key 500:up --event 1s:TOPIC:JSON    a no, a key, an event
ptyrc dev run [--NAME VALUE]   in a live session (ptyrc run -- $SHELL), restarted on save
```

A case: `tests/NAME.args`, words split on white space, `#` to the end
of a line a comment, no quoting (JSON without spaces); a new one:
write the `.args`, then `./test.sh --update`. A report that differs
is left in `tests/NAME.got`.

`ptyrc dev test` prints the screen at each time (`--- T ms`), then
`refused at T ms: METHOD (CODE)`, `lowered at T ms: METHOD, to the
degree D`, `dropped at T ms: ...` (a key or an event that reached
nobody), `refused: NAME, ...` or `lowered: NAME, ...` (a permission
not held; not declared is fine if intended: declaring less is good),
`limit met: NAME`, `ended: ...`; `--json`: the same as one JSON value,
with the styles; `--styles`: a grid of the styles under each screen.

The app holds what `[permissions]` declares (names: `PROTOCOL.md`);
`--deny NAME,...` answers a no, as the driver does: a reading cut to a
lesser degree (`"degree"` in the answer), an act `denied`, a topic
silent. `--screen F`: a text under the app; `--tape F`: a recording.
`--key MS:NAME`: a host key (to a grab); `--event MS:TOPIC:JSON`. A
method headless does not answer: `headless`; one that does not exist:
`unknown-method`. Options of `[options]`: `--NAME VALUE`,
`--NAME`, `--no-NAME`, as `ptyrc app` takes them; after `--` when a
name is one of ptyrc's.
The app's log goes to stderr.

## Errors

An uncaught error is a log line `file:line:column: what`, then its
frames, then `hint:` the cure when known. In `init` or a module it
ends the app (`ended: exit: 1`); in `tick` or `event` a `PtyrcError`
is logged once and the app goes on. `ended: time`: a call ran past its
bound (a loop); `ended: memory`: 64 MiB.
