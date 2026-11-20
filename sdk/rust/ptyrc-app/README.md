# ptyrc-app

A ptyrc store app in Rust: a guest of ABI 1 (`rust/ptyrc-box/README.md`)
for `wasm32-unknown-unknown`. The app implements `App`; `app!` exports
it. No `extern`, no `unsafe` in the app.

The methods, topics, permissions, manifest and limits: `PROTOCOL.md`,
at the root of an app made by `ptyrc dev new` (here:
`sdk/template/PROTOCOL.md`).

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
`run` (`rust/ptyrc-store/README.md`, Build): std and this crate, ptyrc's
own copy of it. No other crate, no `build.rs`, no procedural macro, no
`#![feature]`, no `#[link]`; `include_str!`, `include_bytes!`, `#[path]`
name a file of the tree. `rustup target add wasm32-unknown-unknown`
once. By hand, the same:

```
rustc --edition 2024 --target wasm32-unknown-unknown -C opt-level=s -C panic=abort \
  -C codegen-units=1 --crate-type rlib --crate-name ptyrc_app -o out/libptyrc_app.rlib \
  ptyrc-app/src/lib.rs
rustc --edition 2024 --target wasm32-unknown-unknown -C opt-level=s -C panic=abort \
  -C codegen-units=1 -C lto -C strip=symbols --crate-type cdylib --crate-name app \
  --extern ptyrc_app=out/libptyrc_app.rlib -L dependency=out -o out/app.wasm src/lib.rs
ptyrc-box out/app.wasm '{"opt": 1}'
```

`ptyrc-box` runs in a session (`PTYRC_SOCKET`); headless, through the
library (Test). With a `Cargo.toml` (`ptyrc-app` by path, `crate-type
= ["cdylib"]`), `cargo test` runs the tests of pure logic natively; a
test that reaches the host does not link.

## The model

- The host drives: `init` once, then `tick` at the period asked and
  `event` for each topic subscribed, one at a time, never two at once.
  No thread, no sleep, no clock but `now_ms` (session time since start).
- `tick`, `event`: late past 50 ms, three late in a row: paused; ended
  past 2 s. `init`: 2 s. `call`: 1000 a second. Memory: 64 MiB
  (`PROTOCOL.md`, Limits).
- A panic logs `panic: ...` and ends the app (`trap`). Nothing
  survives the end.
- `std` works; there is no file, socket, clock, environment, stdout or
  thread. `println!` prints nothing: use `log`.

## API

| item | |
|---|---|
| `trait App` | `init(args: Value) -> Self`; `tick(&mut self, now_ms: u64)`, `event(&mut self, topic: &str, data: Value)`, `stop(&mut self)`: optional. `args`: the arguments and options, parsed; `Null` when not JSON |
| `app!(T)` | exports `init`, `tick`, `event`, `stop` for `T`. Once per module |
| `call(method, params) -> Result<Value, Error>` | a protocol request (`PROTOCOL.md`); `params` a JSON object, `Value::Null` for `{}`; `Ok`: the `result` |
| `Error { code, message, hint: Option }` | the protocol's codes (`unknown-method`, `denied`, ...); also `limit` (the rate), `gone` (the driver), `headless` (a method a test run does not answer), `alloc`, `bad-answer` |
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

## Test

`ptyrc dev test DIR` builds the app and runs it headless: the screen at
each time as text, the requests refused, the limits met, the end
(`ptyrc dev new NAME --rust`: an app with its test). Underneath,
`ptyrc_box::headless::run`: session time an argument, `random` from a
seed, the same report every time; sprites, canvases and widgets drawn
as the driver draws them, over an empty screen, a text or a tape. It
answers `draw.put`, `.update`, `.remove`, `.clear`, `widget.declare`,
`.show`, `.set`, `.hide`, `subscribe`, `input.grab`, `.release`,
`session.info`, `screen.text`, `screen.shape`; another method of the
driver: `headless`; a name the driver does not know: `unknown-method`.
`ptyrc dev test --deny NAME`: a permission answered no; `--screen F`,
`--tape F`: the screen; `--key MS:NAME`, `--event MS:TOPIC:JSON`:
what the user does (`PROTOCOL.md`, Headless).

```rust
// dev-dependencies: ptyrc-box, ptyrc-store (rust/), serde_json
use ptyrc_box::headless::{Options, run};
use ptyrc_store::{DirTree, build, check};
let tree = DirTree::new("myapp");
let checked = check(&tree)?;
let toolchain = build::Toolchain::find()?;
let wasm = build::build(&tree, &checked, Path::new("work"), &toolchain,
    &build::Options::default())?.wasm;
let report = run(&wasm, &Options { size: (80, 24), duration_ms: 1000,
    at: vec![0, 500], args: serde_json::json!({}), seed: 1,
    ..Options::default() })?;
print!("{report}");   // the screens, the requests refused, the limits met, the end
assert_eq!(report.end, None);
```

`sdk/rust/tests`: builds `sdk/rust/examples/confetti` as the store
does, checks that every import is of `ptyrc`, runs it twice and
compares the text.

```
cd sdk/rust/tests && cargo test
```

## Examples

`sdk/rust/examples/confetti`: pops of colour, the twin of the Python
`confetti` without `--fancy`, `--random`, `--fireworks`, `--sparkle`;
`one-shot` exits once the last pop is played.

`sdk/rust/examples/sand`: the effect `sand` of the driver, one
`draw.put`, no permission; where the effect is missing (the test
host), a pile of its own. `sdk/rust/examples/snake`: the text is the
wall (`screen.shape`); `grab`: the arrows steer it (`input.grab`);
each denied, it plays on. Both are twins of `sdk/js/examples/`: the
same seed, the same screens.
