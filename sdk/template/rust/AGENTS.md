# {{id}}: a ptyrc store app

Rust built for `wasm32-unknown-unknown` that draws over a terminal
program wrapped by ptyrc. It runs in a sandbox: no path, no network,
no thread; the files it reads are those the user names, the wall
clock is asked.

## Files

| | |
|---|---|
| `ptyrc-app.toml` | the manifest: id, version, entry, `[options]`, `[permissions]`; replace each `TODO` |
| `PROTOCOL.md` | the methods, topics, permissions, manifest and limits |
| `src/lib.rs` | the app, and its modules under `src/` |
| `vendor/ptyrc-app/` | the guest crate, ptyrc's copy; `README.md` there is the API, whole |
| `Cargo.toml` | for cargo only (`cargo test`, an editor); the store does not read it |
| `test.sh` | every case, then the unit tests; status 1 when one fails |
| `tests/CASE.args`, `tests/CASE.txt` | a case: the words after `ptyrc dev test .`, and its report |
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
- A widget (a small window the driver places, in the size that fits):
  `ptyrc_app::deck`: `Deck<S>`, `deck.add(Spec::new(tag, [("big", 17,
  3), ("line", 5, 1)], |state, w, h, widget| content).key(|state, name,
  widget| changed), &mut state)`; give it every `tick` and `event` with
  `&mut state`. `Field`, `Listing`, `cut`, `fit`, `wrap`.
- Host functions: `clock::now()`, `clock::zone(name, &[at])`,
  `clock::civil(unix_ms, offset_s)` (permission `clock`);
  `storage::{get, set, remove, keys}` (`storage`); `asset::{text,
  bytes, list}` (`[app] assets`); `File::of(&args["opt"])`: `text`,
  `bytes`, `text_with_mtime`, `stat`, `write(text, if_mtime_ms)`,
  `list`, `child` (an option of type `file` or `dir`); `result(value,
  status)`; `base64::{encode, decode}`; `clock::until(unix_ms,
  every_ms)` (a tick on the second: `PROTOCOL.md`).
- A write that must not clobber a change made since the read: `let
  (text, mtime) = f.text_with_mtime()?`, then `f.write(&edited,
  Some(mtime))`; changed since: `Err` `not-usable`.
- `call(method, params) -> Result<Value, Error>`: a request of the
  protocol (`PROTOCOL.md`); `Error { code, message, hint }`.
- JSON is `ptyrc_app::json`, not serde: `json!({"k": [x, 1], "s":
  name})`, `v["k"][0].as_u64()` (`Null` where nothing is, never a
  panic), `v == "x"`, `v.to_string()`, `json::parse(text)`. A number is
  an `f64`.
- The size: `ptyrc_app::size()`, then `subscribe(&["size"])` and
  `event("size", {"size": [c, r]})`.

## Rules

- No `std::thread`, `std::time`, `std::fs`, `std::net`; `println!`
  prints nothing: `log`. `tick` is the only clock; `random*` comes from
  the host (seeded in a test).
- Limits: `tick`, `event` 50 ms (three late in a row: paused), 2 s
  (ended); `init` 2 s; memory 64 MiB; `call` 1000 a second; all
  canvases four screens of cells (`PROTOCOL.md`).
- A panic logs `panic: ...` and ends the app (`ended: trap`).
- Sprites go behind the program's text unless `"layer": "front"` and
  `front` held; headless as the driver.
- `screen.*` reads the program's cells, never a sprite. Subscribe to
  `text` or `screen` and read on the event; headless sends them when
  the screen changes (a tape, `--screen T:FILE`).
- Keep the run deterministic: everything from `now_ms`, `args` and
  `random`.
- Built by `rustc` alone, as the store builds it: std and `ptyrc-app`
  only. No other crate, no `build.rs`, no procedural macro, no
  `#![feature]`, no `#[link]`, no `extern` block; `include_str!`,
  `include_bytes!`, `#[path]` name a file of this tree, as a literal.
  `dev check` names what breaks the rule.
- `cargo test` runs the tests of `src/` natively (`#[cfg(test)]`, as
  `src/lib.rs` shows): a `Canvas` is cells in memory (`runs()` reads
  them), `call` answers `native` unless `ptyrc_app::native::serve`
  answers it (`Ok(json!(..))`, `Err(Error::new("denied", "..."))`);
  the screen is `./test.sh`'s.

## Commands

```
rustup target add wasm32-unknown-unknown    once
ptyrc dev check            what the store checks, and its build
cargo test                 the unit tests of src/, natively (`dev test --unit` is for scripts)
./test.sh                  each case (dev test builds), then cargo test
./test.sh --update         rewrites each tests/CASE.txt: after a change meant to change it
ptyrc dev test . --size 80x24 --for 3s --at 0,1s,3s [--screen F | --tape F] [--seed N] [--NAME VALUE]
ptyrc dev test . --deny shape --key 500:up --event 1s:TOPIC:JSON    a no, a key, an event
ptyrc dev run [--NAME VALUE]   in a live session (ptyrc run -- $SHELL), rebuilt on save
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
The app's log and rustc's messages go to stderr.
