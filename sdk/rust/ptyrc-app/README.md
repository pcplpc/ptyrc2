# ptyrc-app

A ptyrc store app in Rust: a guest of ABI 1 for
`wasm32-unknown-unknown`. The app implements `App`; `app!` exports
it. No `extern`, no `unsafe` in the app.

The methods, topics, permissions, manifest and limits: `PROTOCOL.md`,
at the root of an app made by `ptyrc dev new`.

## An app

```
myapp/
  ptyrc-app.toml    the manifest (PROTOCOL.md, Manifest): rust = "src/lib.rs"
  LICENSE.txt       the GPL text
  src/lib.rs        and its modules
  Cargo.toml        optional: for cargo (`cargo test`, an editor)
  vendor/ptyrc-app/ optional: this crate, for cargo
```

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
use ptyrc_app::{App, Canvas, Cell, Color, Value, json, period};

struct Clock { canvas: Canvas }

impl App for Clock {
    fn init(_args: Value) -> Clock {
        period(1000);
        Clock { canvas: Canvas::new(8, 1).expect("canvas") }
    }
    fn tick(&mut self, now_ms: u64) {
        let s = now_ms / 1000;
        self.canvas.fill(Cell { fg: Color::Palette(11), ..Cell::default() });
        self.canvas.text(0, 0, &format!("{:02}:{:02}", s / 60, s % 60));
        self.canvas.present(&json!({"anchor": {"at": [-8, 0], "space": "view"}}));
    }
}

ptyrc_app::app!(Clock);
```

## The build

By `rustc` alone, at the install and by `ptyrc dev check`, `test`,
`run`: std and this crate, ptyrc's own copy of it. No other crate, no
`build.rs`, no procedural macro, no `#![feature]`, no `#[link]`;
`include_str!`, `include_bytes!`, `#[path]` name a file of the tree.
`rustup target add wasm32-unknown-unknown` once. With a `Cargo.toml` (`ptyrc-app` by path, `crate-type
= ["cdylib"]`), `cargo test` runs the app's unit tests natively (Unit
tests).

## The model

- The host drives: `init` once, then `tick` at the period asked and
  `event` for each topic subscribed, one at a time, never two at once.
  No thread, no sleep. `now_ms`: session time since start; the wall
  clock: `clock::now` (Host functions).
- `tick`, `event`: late past 50 ms, three late in a row: paused; ended
  past 2 s. `init`: 2 s. `call`: 1000 a second. Memory: 64 MiB
  (`PROTOCOL.md`, Limits).
- A panic logs `panic: ...` and ends the app (`trap`). Nothing
  survives the end.
- `std` works; there is no path, socket, clock, environment, stdout or
  thread (`std::fs`, `std::time` fail). `println!` prints nothing: use
  `log`.

## API

| item | |
|---|---|
| `trait App` | `init(args: Value) -> Self`; `tick(&mut self, now_ms: u64)`, `event(&mut self, topic: &str, data: Value)`, `stop(&mut self)`: optional. `args`: the arguments and options, parsed; `Null` when not JSON |
| `app!(T)` | exports `init`, `tick`, `event`, `stop` for `T`. Once per module |
| `call(method, params) -> Result<Value, Error>` | a protocol request (`PROTOCOL.md`); `params` a JSON object, `Value::Null` for `{}`; `Ok`: the `result` |
| `Error { code, message, hint: Option }` | the protocol's codes (`unknown-method`, `denied`, ...); also `limit` (the rate), `gone` (the driver), `headless` (a method a test run does not answer), `alloc`, `bad-answer`; `Error::new(code, message)`, `.with_hint(hint)` |
| `period(ms)` | the tick wanted; under 33: 33; 0: none (the default) |
| `log(text)` | a line of the app's log; 1 MiB in all |
| `random(&mut [u8])`, `random_u32()`, `random_below(n)` | from the host; `random_below`: `0..n` |
| `exit(code) -> !` | the end |
| `manifest() -> Option<&Manifest>` | what the app is told of itself at `init` (`PROTOCOL.md`, Manifest): `id`, `version`, `permissions`, `words`, `granted`, `limits` (those granted); `holds(name)`. `None` before `init`, or run without a manifest |
| `subscribe(&[topics])` | `size`, `draw`, `mouse`, `hover`, ... (`subscribe` of the Protocol) |
| `size() -> (columns, rows)` | `session.info`; event `size` `{"size": [c, r]}` |
| `put(sprite) -> id` | `draw.put` of a sprite as JSON (`PROTOCOL.md`, Methods) |
| `text(x, y, rows, style) -> id` | `draw.put` of text rows at `(x, y)` of the raw frame; `style`: `{"fg": 11, "bold": true}`; a space is a hole |
| `remove(id)` | `draw.remove` |
| `Value`, `Map`, `json!`, `json::parse` | JSON (below) |
| `Run { t, fg, bg, attrs }` | a run of a row (`cells`); `Run::new(t).fg(c).attrs(a)`, `with(t)`, `to_value()`; `Color::Below`: absent |
| `cells(&str) -> u32`, `width(char) -> u32` | columns on a terminal: a wide glyph 2, a mark 0 |
| `deck`, `clock`, `storage`, `asset`, `File`, `result`, `base64` | below |

Anything else goes through `call`: `draw.update`, `widget.*`,
`screen.*`, `frame.*`, `batch`, effects (`put(json!({"effect":
{"name": "confetti", "params": {}}}))`). A request beyond the app's
grants answers `denied`, or a result cut to the degree held
(`"degree"` in it). `screen.*` reads the program's cells, never a
sprite.

### JSON

`json`, a module of its own (no dependency):

| | |
|---|---|
| `Value` | `Null`, `Bool(bool)`, `Number(f64)`, `String(String)`, `Array(Vec<Value>)`, `Object(Map)`; `Map`: `BTreeMap<String, Value>` |
| `json!(...)` | JSON syntax; a value is a literal, an array, an object or any expression of a type `Into<Value>` (numbers, `bool`, `&str`, `String`, `Vec`, slices, arrays, `Option`, `Value`, references to these). Keys: string literals, `(expr)` for one computed |
| `v["k"]`, `v[0]` | never panics: `Null` where nothing is. `v["k"] = x`: `Null` becomes an object |
| `get(k)`, `get_mut(k)`, `pointer("/a/0")` | `Option` |
| `as_str`, `as_bool`, `as_f64`, `as_i64`, `as_u64`, `as_array`, `as_object` (`_mut`), `is_null`, `take` | `as_i64`, `as_u64`: an integral number in range |
| `v == "x"`, `v == 3`, `v == true` | |
| `v.to_string()` | compact JSON; a number not finite: `null` |
| `json::parse(&str)`, `json::parse_bytes(&[u8])`, `"..".parse::<Value>()` | `Err(json::Error { at, message })` |

A number is an `f64`: an integer is exact up to 2^53; `3.0` is the
integer 3. Keys are kept sorted.

### Canvas

A buffer of cells in the guest's memory, shown as one sprite.

| | |
|---|---|
| `Canvas::new(w, h) -> Option<Canvas>` | `None`: refused (more cells in all than four screens) |
| `width()`, `height()` | |
| `set(x, y, Cell)`, `get(x, y) -> Cell` | outside: ignored, `Cell::default()` |
| `set_char(x, y, Option<char>)`, `set_fg`, `set_bg(x, y, Color)`, `set_attrs(x, y, u16)` | one member of a cell |
| `text(x, y, &str)` | one char a cell, rightward; the colours kept |
| `fill(Cell)`, `clear()` | every cell; `clear`: nothing drawn |
| `runs() -> Vec<Vec<Run>>` | the cells as runs: a widget's content (`Content::from(&mut canvas)`); a cell with no glyph a blank of no style |
| `present(&place) -> bool` | shows it: `draw.put` the first time, then `draw.update` of what changed; nothing when nothing did. `place`: the members of a sprite but its content (`anchor`, `frame`, `layer`, `z`, `alpha`, `ttl_ms`, `motion`, ...) or `Value::Null`. `false`: refused, the reason logged |
| drop | `release`: the sprite removed |

- `Cell { ch: Option<char>, fg: Color, bg: Color, attrs: u16 }`; the
  default: nothing drawn, colours of the cell below.
- `ch: None`: the cell below shows. A space is a hole unless `bg` is
  set. A control character: a space. A wide glyph is written in its
  first cell; leave the next `None`.
- `Color::Below` (that of the cell below), `Default` (the terminal's),
  `Palette(0..=255)`, `Rgb(r, g, b)`.
- `attr::BOLD | DIM | ITALIC | UNDERLINE | INVERSE | STRIKE | BLINK`.
- No opacity a cell: `alpha` in the place, for the whole canvas.
- `anchor` `{"at": [x, y]}`: a cell of the raw frame; `"space":
  "view"` with negative coordinates counts from the right and bottom.

### Deck

`ptyrc_app::deck`: the twin of the script module `ptyrc/deck`: widgets the driver places, in the variant that
fits, kept drawn. The deck holds the handlers, the app its state:
`Deck<S>` takes `&mut S` at `add`, `tick`, `event` and gives it to each
handler, so `self.deck.tick(now, &mut self.state)` borrows two fields.

```rust
struct Watch { since: u64 }
struct App1 { deck: Deck<Watch>, watch: Watch }
// init:
let mut deck = Deck::new(json!({"border": "round", "prefer": "top-right"}));
deck.add(Spec::new("clock", [("long", 8, 1), ("short", 5, 1)],
        |s: &mut Watch, w, _h, widget| Content::from(clock(widget.now - s.since, w)))
    .key(|s, name, widget| name == "r" && { s.since = widget.now; true }),
    &mut watch)?;
// tick: self.deck.tick(now_ms, &mut self.watch)?;
// event: self.deck.event(topic, &data, &mut self.watch)?;
```

| | |
|---|---|
| `Deck::new(show)` | `show`: defaults of `widget.show` (`border`, `title`, `frame`, `prefer`, `fill`, `margin`, `layer`, `modal`; another member panics); `every`: `tick` renders each variant; `hover_timeout`: `Some(1500)` ms; `now`: the session time of the last `tick` |
| `add(Spec, &mut S) -> Result<&Widget>` | declares (each variant rendered once) and shows; the first subscribes `widgets`, `move`; `hover`, `leave`: `hover`; `key`, `paste`: `widget.ask {quiet: true}` (`Spec::ask` overrides) |
| `Spec::new(tag, sizes, render)` | `sizes`: `[(w, h), ...]` (named `WxH`) or `[(name, w, h), ...]`; `render(state, w, h, &Widget) -> Content`; `.key`, `.paste(state, text, &Widget)`, `.hover(state, x, y, &Widget)` (a cell of the variant), `.leave`, `.closed(state, &Widget)`, `.show(json)`: `true`, something changed: drawn again |
| `Content` | `Rows(Vec<String>)` (a space a hole), `Cells(Vec<Vec<Run>>)`; `From` a `&str` (lines by `\n`), `Vec<&str>`, `Vec<String>`, `Vec<Run>` (one row), `Vec<Vec<Run>>`, `&mut Canvas` (as `cells`: `widget.set` refuses `grid`) |
| `tick(now, &mut S)` | a hover past its timeout ended; each widget shown rendered, the contents that changed sent in one `batch` (one by one where `batch` answers `headless`) |
| `event(topic, &data, &mut S) -> Result<bool>` | `widgets` (a widget whose variant, focus or `asks` changed is drawn at once), `move`, `hover`, `input` (keys of its focused widget); `false`: not the deck's |
| `draw(only, every, &mut S)`, `resize(tag, sizes, &mut S)`, `move_to(frame, &mut S)`, `drop(tag, &mut S)`, `leave(&mut S)` | as `ptyrc/deck`'s `draw`, `resize`, `move`, `drop`, `leave` |
| `widgets()`, `widget(tag)`, `widget_mut(tag)` | |
| `Widget` | `tag`, `info` (the driver's), `hidden`, `now`; `id()`, `variant()`, `size()`, `rect()`, `placed()`, `over()`, `asks()`, `focused()`; `set(content, variant, title)`, `hide()`, `ask(&opts)`, `release()` |
| `Keys` | `feed(&data) -> Option<Fed>`: `Fed::Key(name)`, `Fed::Paste(text)` (bracketed paste put together); `Keys::name(bytes)`: `shift+up`, ... |
| `Field` | a line and its cursor (a byte offset): `text`, `cursor`, `insert`, `key(name) -> bool`, `render(w: u32, focused, placeholder, &style) -> Vec<Run>`: the cursor an inverse cell (in a text report of `dev test`: `--styles`) |
| `Listing` | `count`, `sel`, `top` (`usize`, indices); `key(name, letters) -> bool`, `window(h: u32) -> Range<usize>`, `mark(h: u32) -> String` |
| `cut(text, n: u32) -> String`, `fit(&[Run], w: u32) -> Vec<Run>`, `wrap(text, w: u32, n: u32) -> Vec<String>` | in cells: cut with `…`; a row of exactly `w` cells; `n` lines at most, cut at blanks |

Widths and heights are `u32` throughout, as `render` gets them:
`fit(&row, w)` inside a render needs no cast.

### Host functions

Requests of the protocol (`PROTOCOL.md`, Methods), typed. Each returns
`Result<_, Error>`.

| | |
|---|---|
| `clock::now() -> Wall` | `unix_ms`, `zone`, `offset_s`, `abbr`, `next` (the next change of offset); `clock` not held: session time from 1970, UTC, `degree: Some("none")` |
| `clock::zone(name, &[at]) -> Vec<Offset>` | `at`, `offset_s`, `abbr`, `next` of an IANA zone or `"local"`; `at` empty: now |
| `clock::civil(unix_ms, offset_s) -> Civil` | `year`, `month`, `day`, `hour`, `minute`, `second`, `weekday` (0: Sunday); pure |
| `clock::until(unix_ms, every_ms) -> u32` | the `period` to the next multiple of `every_ms` of the wall clock, plus `every_ms` while under 33; pure |
| `storage::get(key) -> Option<String>`, `set(key, value) -> used`, `remove(key)`, `keys()` | strings; permission `storage`, else `denied` |
| `asset::text(name)`, `bytes(name)`, `list()` | the tree `[app] assets`; absent: `no-file` |
| `File::of(&args["opt"])`, `File::all`, `File::new("f0")` | a file or directory the user named (an option of type `file`, `dir`): `handle`, `name`; `text()`, `bytes()`, `text_with_mtime() -> (String, i64)`, `bytes_with_mtime()`, `stat() -> Stat`, `write(text, if_mtime_ms: Option<i64>) -> Stat`, `write_bytes`, `list()`, `child(name)` |
| `result(value, status)` | what `ptyrc app ID --wait` prints, and its exit status |
| `base64::encode(&[u8])`, `decode(&str) -> Option` | RFC 4648, padded; pure |

Reads go by pieces of 1 MiB. `text`: UTF-8, invalid bytes replaced.
A file changed between two pieces: `not-usable`.

A write that keeps a change made by someone else since the read: the
mtime of the read guards it; `stat()` after `text()` leaves a window.

```rust
let (text, mut mtime) = file.text_with_mtime()?;
// ... the user edits `text`; then:
match file.write(&text, Some(mtime)) {
    Ok(stat) => mtime = stat.mtime_ms,              // the guard of the next write
    Err(e) if e.code == "not-usable" => { /* changed since: read it again */ }
    Err(e) => log(&e.to_string()),                  // denied: not held
}
```

A tick on the second: at each tick, `period(clock::until(
clock::now()?.unix_ms, 1000))`; asked again each time, it does not
drift.

## Unit tests

`cargo test` builds the app natively: `#[cfg(test)]` tests of `src/`.
There, a `Canvas` is cells in memory (`present` shows nothing, `runs`
reads them), `log` writes to stderr, `random` is a fixed sequence,
`exit` panics, and `call` answers `native` unless
`ptyrc_app::native::serve(|method, params| ...)` answers it for the
thread:

```rust
use ptyrc_app::{Error, json, storage};
ptyrc_app::native::serve(|method, _params| match method {
    "storage.get" => Ok(json!({"value": "3"})),
    "storage.set" => Err(Error::new("denied", "storage: not held")),
    _ => Ok(json!({})),
});
assert_eq!(storage::get("runs").unwrap().as_deref(), Some("3"));
assert_eq!(storage::set("runs", "4").unwrap_err().code, "denied");
```

The handler is the thread's: each test sets its own.

## Test

`ptyrc dev test DIR` builds the app and runs it headless: the screen at
each time as text, the requests refused, the limits met, the end
(`ptyrc dev new NAME --rust`: an app with its test). Session time is
an argument, `random` comes from a seed: the same report every time.
Sprites, canvases and widgets are drawn as the driver draws them, over
an empty screen, a text or a tape. A method of the driver headless
does not answer: `headless`; a name the driver does not know:
`unknown-method` (`PROTOCOL.md`, Headless). `--deny NAME`: a
permission answered no; `--screen F`, `--tape F`: the screen; `--key
MS:NAME`, `--event MS:TOPIC:JSON`: what the user does.
