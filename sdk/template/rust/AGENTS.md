# {{id}}: a ptyrc store app

Rust built for `wasm32-unknown-unknown` that draws over a terminal
program wrapped by ptyrc. It runs in a sandbox: no file, no network,
no thread, no clock but `tick`.

## Files

| | |
|---|---|
| `ptyrc-app.toml` | the manifest: id, version, entry, `[options]`, `[permissions]` |
| `src/lib.rs` | the app |
| `vendor/ptyrc-app/` | the guest crate, vendored; `README.md` there is the API, whole |
| `Cargo.toml`, `Cargo.lock` | keep the lock in the tree |
| `test.sh`, `expected.txt` | the test: the screen at fixed times |
| `LICENSE.txt` | GPL-3.0-or-later; the only licence accepted |

## The API

```rust
use ptyrc_app::{App, Canvas, Cell, Color, Value, attr, call, json, log, period};
impl App for MyApp {
    fn init(args: Value) -> Self { .. }          // once; args: the [options], parsed
    fn tick(&mut self, now_ms: u64) { .. }       // every period(ms); ms since start
    fn event(&mut self, topic: &str, data: Value) {}  // topics: subscribe(&["size"])
    fn stop(&mut self) {}                        // optional; 500 ms to end
}
ptyrc_app::app!(MyApp);
```

- `Canvas::new(w, h)`: cells; `set`, `text`, `fill`, `clear`;
  `present(&json!({"anchor": {"at": [x, y], "space": "view"}}))` shows
  it, again after a change. `ch: None` is a hole. Negative `at`: the
  right or bottom edge of the canvas from the right or bottom, `-1` the
  last cell.
- `call(method, params) -> Result<Value, Error>`: a request of the
  protocol (`draw.put`, `widget.show`, `session.info`, ...);
  `Error { code, message, hint }`.
- The size: `ptyrc_app::size()`, then `subscribe(&["size"])` and
  `event("size", {"size": [c, r]})`.

## Rules

- No `std::thread`, `std::time`, `std::fs`, `std::net`; `println!`
  prints nothing: `log`. `tick` is the only clock; `random*` comes from
  the host (seeded in a test).
- Limits: `tick`, `event` 50 ms (three late in a row: paused), 2 s
  (ended); `init` 2 s; memory 64 MiB; `call` 1000 a second; all
  canvases four screens of cells.
- A panic logs `panic: ...` and ends the app (`ended: trap`).
- Sprites go behind the program's text unless `"layer": "front"`.
- Keep the run deterministic: everything from `now_ms`, `args` and
  `random`.
- The store does not install Rust apps yet; `dev check` says so.

## Commands

```
rustup target add wasm32-unknown-unknown    once
ptyrc dev check            the manifest, the licence, the sizes: what the store checks
./test.sh                  ptyrc dev test (it builds) against expected.txt
./test.sh --update         after a change meant to change the screen
ptyrc dev test . --size 80x24 --for 3s --at 0,1s,3s [--screen F | --tape F] [--seed N] [--NAME VALUE]
ptyrc dev test . --deny shape --key 500:up --event 1s:TOPIC:JSON    a no, a key, an event
ptyrc dev run [--NAME VALUE]   in a live session (ptyrc run -- $SHELL), rebuilt on save
```

`ptyrc dev test` prints the screen at each time (`--- T ms`), then
`refused at T ms: METHOD (CODE)`, `lowered at T ms: METHOD, to the
degree D`, `dropped at T ms: ...` (a key or an event that reached
nobody), `not declared: NAME; the line of the manifest ...`, `limit
met: NAME`, `ended: ...`; `--json`: the same as one JSON value.

The app holds what `[permissions]` declares; `--deny` answers a no, as
the driver does: a reading cut to a lesser degree (`"degree"` in the
answer), an act `denied`. Options of `[options]`: `--NAME VALUE`,
`--NAME`, `--no-NAME`, as `ptyrc app` takes them; after `--` when a
name is one of ptyrc's.
The app's log and cargo's go to stderr.
