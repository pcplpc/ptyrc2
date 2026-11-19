# {{id}}: a ptyrc store app

A script that draws over a terminal program wrapped by ptyrc. It runs
in a sandbox (WebAssembly): no file, no network, no clock but `tick`.

## Files

| | |
|---|---|
| `ptyrc-app.toml` | the manifest: id, version, entry, `[options]`, `[permissions]` |
| `app.js` | the app; it may import `./other.js` files of this tree |
| `ptyrc.d.ts` | the API, typed: the whole of it. Read it first |
| `test.sh`, `expected.txt` | the test: the screen at fixed times |
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
- `call(method, params)`: a request of the protocol (`draw.put`,
  `widget.show`, `session.info`, `screen.text`, ...); throws a
  `PtyrcError` with `code` and `hint` when refused.
- The size: `call("session.info").size` is `[columns, rows]`; then
  `call("subscribe", {topics: ["size"]})` and `event("size", {size: [c, r]})`.

## Rules

- No timer, no promise across calls, no `fetch`, no `console`:
  `tick` is the only clock. `Date.now()` is session time.
  `Math.random()` comes from the host (seeded in a test).
- Limits: `tick`, `event` 50 ms (three late in a row: paused), 2 s
  (ended); `init` 2 s; memory 64 MiB; `call` 1000 a second; all
  canvases four screens of cells; scripts 1 MiB.
- Sprites go behind the program's text unless `layer: "front"`.
- Keep the run deterministic: everything from `now`, `args` and
  `Math.random()`.

## Commands

```
ptyrc dev check            the manifest, the licence, the sizes: what the store checks
./test.sh                  ptyrc dev test against expected.txt
./test.sh --update         after a change meant to change the screen
ptyrc dev test . --size 80x24 --for 3s --at 0,1s,3s [--screen F | --tape F] [--seed N]
ptyrc dev run              in a live session (ptyrc run -- $SHELL), restarted on save
```

`ptyrc dev test` prints the screen at each time (`--- T ms`), then
`refused at T ms: METHOD (CODE)`, `limit met: NAME`, `ended: ...`.
The app's log goes to stderr.

## Errors

An uncaught error is a log line `file:line:column: what`, then its
frames, then `hint:` the cure when known. In `init` or a module it
ends the app (`ended: exit: 1`); in `tick` or `event` a `PtyrcError`
is logged once and the app goes on. `ended: time`: a call ran past its
bound (a loop); `ended: memory`: 64 MiB.
