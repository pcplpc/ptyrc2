# ptyrc-app

A ptyrc store app in Rust: a guest of ABI 1 (`rust/ptyrc-box/README.md`)
for `wasm32-unknown-unknown`. The app implements `App`; `app!` exports
it. No `extern`, no `unsafe` in the app.

## An app

```
myapp/
  Cargo.toml        crate-type = ["cdylib"], depends on ptyrc-app
  Cargo.lock
  ptyrc-app.toml    the manifest (plan/store-model.md, Manifest)
  LICENSE.txt       the GPL text
  src/lib.rs
```

```toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-or-later"

[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
ptyrc-app = { path = "../ptyrc-app" }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
panic = "abort"
strip = true
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

```
cargo build --release --target wasm32-unknown-unknown
# target/wasm32-unknown-unknown/release/myapp.wasm
ptyrc-box target/wasm32-unknown-unknown/release/myapp.wasm '{"opt": 1}'
```

`rustup target add wasm32-unknown-unknown` once. `ptyrc-box` runs in a
session (`PTYRC_SOCKET`); headless, through the library (Test).

## The model

- The host drives: `init` once, then `tick` at the period asked and
  `event` for each topic subscribed, one at a time, never two at once.
  No thread, no sleep, no clock but `now_ms` (session time since start).
- `init`, `tick`, `event`: 50 ms each (`init` 2 s); three late in a row:
  paused. `call`: 1000 a second. Memory: 64 MiB.
- A panic logs `panic: ...` and ends the app (`trap`). Nothing
  survives the end.
- `std` works; there is no file, socket, clock, environment, stdout or
  thread. `println!` prints nothing: use `log`.

## API

| item | |
|---|---|
| `trait App` | `init(args: Value) -> Self`; `tick(&mut self, now_ms: u64)`, `event(&mut self, topic: &str, data: Value)`, `stop(&mut self)`: optional. `args`: the arguments and options, parsed; `Null` when not JSON |
| `app!(T)` | exports `init`, `tick`, `event`, `stop` for `T`. Once per module |
| `call(method, params) -> Result<Value, Error>` | a protocol request (`plan/design.md`, Protocol); `params` a JSON object, `Value::Null` for `{}`; `Ok`: the `result` |
| `Error { code, message, hint: Option }` | the protocol's codes; also `limit` (the rate), `gone` (the driver), `headless` (not answered without a session), `alloc`, `bad-answer` |
| `period(ms)` | the tick wanted; under 33: 33; 0: none (the default) |
| `log(text)` | a line of the app's log; 1 MiB in all |
| `random(&mut [u8])`, `random_u32()`, `random_below(n)` | from the host; `random_below`: `0..n` |
| `exit(code) -> !` | the end |
| `subscribe(&[topics])` | `size`, `draw`, `mouse`, `hover`, ... (`subscribe` of the Protocol) |
| `size() -> (columns, rows)` | `session.info`; event `size` `{"size": [c, r]}` |
| `put(sprite) -> id` | `draw.put` of a sprite as JSON (`plan/design.md`, scene) |
| `text(x, y, rows, style) -> id` | `draw.put` of text rows at `(x, y)` of the raw frame; `style`: `{"fg": 11, "bold": true}`; a space is a hole |
| `remove(id)` | `draw.remove` |
| `Value`, `json!`, `json` | `serde_json` |

Anything else goes through `call`: `draw.update`, `widget.*`,
`screen.*`, `frame.*`, `batch`, effects (`put(json!({"effect":
{"name": "confetti", "params": {}}}))`). A request beyond the app's
grants answers `denied` or a lowered result.

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

`ptyrc_box::headless::run`: an empty screen, session time an argument,
`random` from a seed: the same report every time. It answers `draw.*`,
`subscribe`, `unsubscribe`, `session.info`, `screen.text`,
`screen.shape`; the rest: `headless`. It shows canvases (not text
sprites) anchored `at`.

```rust
// dev-dependencies: ptyrc-box = { path = ".../rust/ptyrc-box" }, serde_json
use ptyrc_box::headless::{Options, run};
let wasm = std::fs::read("target/wasm32-unknown-unknown/release/myapp.wasm")?;
let report = run(&wasm, &Options { size: (80, 24), duration_ms: 1000,
    at: vec![0, 500], args: serde_json::json!({}), seed: 1,
    ..Options::default() })?;
print!("{report}");   // the screens, the requests refused, the limits met, the end
assert_eq!(report.end, None);
```

`sdk/rust/tests`: builds `sdk/rust/examples/*` for the target, checks
that every import is of `ptyrc`, runs each twice and compares the text.

```
cd sdk/rust/tests && cargo test
```

## Example

`sdk/rust/examples/confetti`: pops of colour, the twin of the Python
`confetti` without `--fancy`, `--random`, `--fireworks`, `--sparkle`;
`one-shot` exits once the last pop is played.
