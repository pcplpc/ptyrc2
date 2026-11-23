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
  Cargo.lock        written by cargo; committed
  vendor/ptyrc-app/ optional: this crate, for cargo
```

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
use ptyrc_app::{App, Canvas, Cell, Color, Value, json, period};

struct Clock { canvas: Canvas }

impl App for Clock {
    fn init(_args: Value) -> Clock {
        period(1000);
        Clock { canvas: Canvas::new(5, 1).expect("canvas") }
    }
    fn tick(&mut self, now_ms: u64) {
        let s = now_ms / 1000;
        self.canvas.fill(Cell { fg: Color::Palette(11), ..Cell::default() });
        self.canvas.text(0, 0, &format!("{:02}:{:02}", s / 60 % 100, s % 60));
        // The right edge of the canvas on the last column of the view.
        self.canvas.present(&json!({"anchor": {"at": [-1, 0], "space": "view"}}));
    }
}

ptyrc_app::app!(Clock);
```

## The build

By `rustc` alone, at the install and by `ptyrc dev check`, `test`,
`run`: std and this crate, ptyrc's own copy of it. No other crate, no
`build.rs`, no procedural macro, no `#![feature]`, no `#[link]`;
`include_str!`, `include_bytes!`, `#[path]` name a file of the tree.
`rustup target add wasm32-unknown-unknown` once. `[app] rustc =
"1.88"`: the floor of every Rust store app; this crate builds with
1.85. With a `Cargo.toml` (`ptyrc-app` by path, `crate-type =
["cdylib"]`), `cargo test` runs the app's unit tests natively (Unit
tests).

## The model

- The host drives: `init` once, then `tick` at the period asked and
  `event` for each topic subscribed, one at a time. No thread, no
  sleep. `now_ms`: session time since the start; the wall clock:
  `clock::now` (Host functions).
- `tick`, `event`: late past 50 ms; three late in a row: paused; ended
  past 2 s. `init`: 2 s. `call`: 1000 a second. Memory: 64 MiB
  (`PROTOCOL.md`, Limits). Headless (`dev test`) counts in session
  time: never late nor paused; the wall time: `dev test --timing`.
- A panic logs `panic: ...` and ends the app (`trap`). Nothing
  survives the end.
- `std` works without path, socket, clock, environment, stdout or
  thread (`std::fs`, `std::time` fail). `println!` prints nothing: use
  `log`.

## API

Every request returns `Result<_, Error>`; nothing panics on a refusal.

| item | |
|---|---|
| `trait App` | `fn init(args: Value) -> Self`; optional: `fn tick(&mut self, now_ms: u64)`, `fn event(&mut self, topic: &str, data: Value)`, `fn stop(&mut self)` (500 ms). `args`: the options and positionals, parsed |
| `app!(T)` | exports `init`, `tick`, `event`, `stop` of `T`. Once per module |
| `call(method: &str, params: Value) -> Result<Value, Error>` | a request (`PROTOCOL.md`, Methods); `params` an object, `Value::Null` for `{}`; `Ok`: the `result` |
| `Error { code: String, message: String, hint: Option<String> }` | the protocol's codes (`denied`, `bad-params`, ...); of the SDK: `limit` (the rate), `gone`, `headless`, `alloc`, `bad-answer`. `Error::new(code, message)`, `.with_hint(hint)`; `Display` |
| `period(ms: u32)` | the tick wanted; under 33: 33; 0: none (the default) |
| `log(text: &str)` | a line of the app's log; 1 MiB in all |
| `random(buf: &mut [u8])`, `random_u32() -> u32`, `random_below(n: u32) -> u32` | from the host (`--seed`); `random_below`: `0..n`, 0 for 0 |
| `exit(code: i32) -> !` | the end |
| `manifest() -> Option<&'static Manifest>` | what the app is told of itself at `init`; `None` before `init`, or run without one |
| `Manifest` | `id`, `version: String`; `permissions`, `words`, `granted: Vec<String>`; `limits: Vec<(String, u64)>` (granted); `frame: Option<u64>` (below); `holds(&self, name: &str) -> bool`; `sees(&self, degree: &str) -> bool`: that degree or one above it held (`layout` < `shape` < `words` < `text`) |
| `subscribe(topics: &[&str]) -> Result<(), Error>` | adds topics; never removes one (no unsubscribe). A `Sight`, a `Deck` subscribe as well: the app gets each event once, and passes it on. An unknown topic: `bad-params`, none added |
| `size() -> Result<(u32, u32), Error>` | `(columns, rows)` of the screen (`session.info`); a change: event `size` `{"size": [c, r]}` |
| `put(sprite: Value) -> Result<u64, Error>` | `draw.put` of a sprite as JSON; its id |
| `text(x: i32, y: i32, rows: &[&str], style: Value) -> Result<u64, Error>` | a text sprite at `(x, y)`; `style` `{"fg": 11, "bold": true}`; a space a hole; its id |
| `remove(id: u64) -> Result<(), Error>` | `draw.remove` |
| `Run { t: String, fg: Color, bg: Color, attrs: u16 }` | a run of a row (`cells`); `Run::new(t).fg(c).bg(c).attrs(a)`, `run.with(t) -> Run` (same style), `to_value() -> Value`; `Color::Below`: the member absent |
| `cells(text: &str) -> u32`, `width(c: char) -> u32` | columns on a terminal: a wide glyph 2, a mark 0 |
| `canvas`, `sight`, `dice`, `deck`, `clock`, `storage`, `asset`, `File`, `result`, `base64`, `native` | below |

Anything else goes through `call`: `draw.update`, `widget.*`,
`screen.*`, `frame.*`, `batch`, effects (`put(json!({"effect":
{"name": "confetti", "params": {}}}))`). A request beyond the app's
grants answers `denied`, or a result cut to the degree held
(`"degree"` in it). `screen.*` reads the program's cells, never a
sprite.

`Manifest::frame`: the id of the frame the app's surface is on at the
start (`--frame`, else `[app] frame`, else `active`); `move` names the
next. Live, it is that id only from `layout` up (`sees("layout")`);
below: 0, and the `frame` of every request is ignored (its surface). Headless
(`dev test`): 0, the one frame, raw. A live id is seen only by `ptyrc
dev run`, with a degree declared and granted.

### JSON

`ptyrc_app::json`, no dependency:

| | |
|---|---|
| `Value` | `Null`, `Bool(bool)`, `Number(f64)`, `String(String)`, `Array(Vec<Value>)`, `Object(Map)`; `Map`: `BTreeMap<String, Value>` |
| `json!(...)` | JSON syntax; a value is a literal, an array, an object or an expression of a type `Into<Value>` (numbers, `bool`, `&str`, `String`, `Vec`, slices, arrays, `Option`, `Value`, references to these). Keys: string literals; `(expr)` for one computed |
| `v["k"]`, `v[0]` | never panics: `Null` where nothing is. `v["k"] = x`: a `Null` becomes an object |
| `get(k)`, `get_mut(k)`, `pointer("/a/0")` | `Option` |
| `as_str`, `as_bool`, `as_f64`, `as_i64`, `as_u64`, `as_array`, `as_object` (and `_mut`), `is_null`, `take` | `Option`; `as_i64`, `as_u64`: an integral number in range |
| `v == "x"`, `v == 3`, `v == true` | |
| `v.to_string()` | compact JSON; a number not finite: `null` |
| `json::parse(&str)`, `json::parse_bytes(&[u8])`, `"..".parse::<Value>()` | `Result<Value, json::Error>`; `json::Error { at, message }` |

A number is an `f64`: an integer is exact up to 2^53; `3.0` is the
integer 3. Keys are kept sorted.

### Canvas

A buffer of cells in the guest's memory, shown as one sprite.

| | |
|---|---|
| `Canvas::new(w: u32, h: u32) -> Option<Canvas>` | `None`: refused (no cell, or more cells in all than four screens) |
| `width(&self) -> u32`, `height(&self) -> u32` | |
| `set(&mut self, x: u32, y: u32, cell: Cell)`, `get(&mut self, x: u32, y: u32) -> Cell` | outside: ignored, `Cell::default()` |
| `set_char(x, y, ch: Option<char>)`, `set_fg(x, y, c: Color)`, `set_bg(x, y, c: Color)`, `set_attrs(x, y, attrs: u16)` | one member of a cell |
| `text(&mut self, x: u32, y: u32, s: &str)` | one char a cell, rightward; the colours kept |
| `fill(&mut self, cell: Cell)`, `clear(&mut self)` | every cell; `clear`: nothing drawn |
| `runs(&mut self) -> Vec<Vec<Run>>` | the cells as runs (a widget's `cells`); a cell with no glyph a blank of no style |
| `present(&mut self, place: &Value) -> bool` | shows it: `draw.put` the first time, then `draw.update` of what changed; nothing when nothing did. `place`: the members of a sprite but its content (`anchor`, `frame`, `layer`, `z`, `alpha`, `ttl_ms`, `motion`, ...), or `Value::Null`. `false`: refused, the reason logged |
| `sprite(&self) -> Option<u64>` | its sprite's id once presented: the `sprite` of `mouse`, `hover` on it. A new id when the driver removed it and `present` put it again. `None`: before, refused, or natively. An app that calls it needs a runner that has it |
| drop | released: the sprite removed |

- `Cell { ch: Option<char>, fg: Color, bg: Color, attrs: u16 }`;
  `Cell::default()`: nothing drawn, the colours of the cell below.
- `ch: None`: the cell below shows. A space is a hole unless `bg` is
  set. A control character: a space. A wide glyph goes in its first
  cell; leave the next `None`.
- `Color::Below` (that of the cell below; the default), `Default` (the
  terminal's), `Palette(u8)`, `Rgb(u8, u8, u8)`.
- `attr::BOLD | DIM | ITALIC | UNDERLINE | INVERSE | STRIKE | BLINK`
  (`u16`).
- No opacity a cell: `alpha` in the place, for the whole canvas.
- `anchor` `{"at": [x, y], "space": "view"}`: a cell of the view.
  Negative `x`: the canvas's right edge `-x - 1` columns from the
  view's last; `-1`: on it. `y` likewise from the bottom. `"space":
  "frame"`: a cell of the frame, which scrolls.
- The pointer on a canvas: `hover`, `mouse` with `sprite` its
  `sprite()`, `at` the canvas cell. Behind the text (the default), it
  needs `pointer` (`hover`) and `clicks` (`mouse`) declared; in front
  (`"layer": "front"`, `front` held), none, and a click on it is taken
  from the program.

### Drawing

`ptyrc_app::canvas`: the twin of the script module `ptyrc/canvas`
(same names, snake_case; a script getter is a method). Pure; `draw`,
`paint` write into a `Canvas`. A colour is `Color::Rgb`, or
`Color::Palette(16..=255)` read as its value; another panics.

| | |
|---|---|
| `DOTS: [[u8; 2]; 4]` | `DOTS[y][x]`: the bit of the dot `(x, y)` of a cell |
| `glyph(bits: u8) -> Option<char>` | `U+2800 + bits`; `None` for 0 |
| `from_grid(grid: u8) -> u8` | `const`; the bits of a cell from bits row by row (bit `2y + x` the dot `(x, y)`) |
| `Braille::new(width: u32, height: u32) -> Braille` | `2 * width` by `4 * height` dots in `width` by `height` cells, all off; a side below 1 panics |
| `width() -> u32`, `height() -> u32` | cells |
| `bits() -> &[u8]`, `bits_mut() -> &mut [u8]` | a byte a cell, `(cx, cy)` at `cy * width + cx` |
| `set(x: i64, y: i64, on: bool)`, `get(x: i64, y: i64) -> bool` | a dot; outside: ignored, off |
| `line(x0: f64, y0: f64, x1: f64, y1: f64, on: bool)` | `max(1, round(max(\|dx\|, \|dy\|)))` steps, each dot rounded half up |
| `clear() -> &mut Braille` | every dot off |
| `cell(cx: u32, cy: u32) -> u8`, `set_cell(cx: u32, cy: u32, bits: u8)`, `glyph(cx: u32, cy: u32) -> Option<char>` | a cell's bits (0 outside); its glyph, `None` with no dot on |
| `cells() -> Vec<(u32, u32)>`, `rows() -> Vec<String>` | the cells with a dot on, row by row; a string a row, a space where no dot is on |
| `draw(&self, canvas: &mut Canvas, x: u32, y: u32, style: Option<&Run>) -> &Braille` | each row written at `(x, y)`, spaces included; `style`: its `fg`, `bg`, `attrs` on each cell (a space a hole unless it has a `bg`); `None`: the colours kept |
| `paint(&self, canvas: &mut Canvas, x: u32, y: u32, fg: impl Fn(u32, u32) -> Color)` | a cell with a dot: its glyph in `fg(cx, cy)`; another: nothing drawn |
| `NEAR: u32` | 24: two pixels are alike when their channels differ by this at most, summed |
| `Half::new(width: u32, height: u32, color: Color) -> Half` | `width` by `2 * height` pixels in `width` by `height` cells, every pixel `color`: the top pixel the `fg` of `▀`, the bottom its `bg`; glyphs over them; a side below 1 panics |
| `width() -> u32`, `height() -> u32` | cells; pixels: `width` by `2 * height` |
| `fill(color: Color) -> &mut Half`, `get(x: i64, y: i64) -> Option<Color>` | every pixel, the glyphs kept; a pixel, `None` outside |
| `set(x: i64, y: i64, color: Color, alpha: f64)`, `rect(x: i64, y: i64, w: i64, h: i64, color, alpha)`, `disc(cx: f64, cy: f64, r: f64, color, alpha)` | `alpha` under 1: `mix(old, color, alpha)`; `disc`: the pixels whose centre `(x + .5, y + .5)` is within `r` |
| `put(cx: i64, cy: i64, text: &str, fg: Option<Color>, bg: Option<Color>)` | glyphs from the cell `(cx, cy)` rightward, a char each; `fg` `None`: `Color::Below`; `bg` `None`: the mean of the cell's two pixels |
| `ink(braille: &Braille, fg: Option<Color>, tint: f64, rough: f64)` | each cell with a dot, no glyph, two pixels alike: the Braille glyph, `bg` the pixel under most dots, `fg` mixed over it by `tint`; pixels unlike: `fg` mixed by `rough` into each pixel under a dot |
| `rows(&self, deep: bool) -> Vec<Vec<Run>>` | colours of the 256 (`xterm`); `deep`: `Rgb`. Cells alike one run, a blank joins the run before; ink on two pixels the 256 tell apart left out |
| `draw(&self, canvas: &mut Canvas, x: u32, y: u32, deep: bool) -> &Half` | every cell at `(x, y)` |
| `mix(a: Color, b: Color, t: f64) -> Color` | `t` clamped to `0..=1`, each channel rounded half up |
| `channels(c: Color) -> [u8; 3]`, `hex(c: Color) -> String` | `"#rrggbb"` |
| `xterm(c: Color) -> u8` | `16..=255`: the nearest of the 240 fixed colours (weights 3, 4, 2, a lost chroma counted) |
| `palette(n: u8) -> Option<Color>`, `named(name: &str) -> Option<u8>` | the value of `16..=255`, `None` under 16; `black` .. `white`, and `bright-` each: `0..=15` |

A `Half` has no transparent pixel: every cell it draws has a `bg`, so
a `Half` over the whole view hides the text. To leave holes, empty
the cells after `draw`:

```rust
half.draw(&mut canvas, 0, 0, false);
for cy in 0..half.height() {
    for cx in 0..half.width() {
        let (x, y) = (i64::from(cx), 2 * i64::from(cy));
        if half.get(x, y) == Some(SKY) && half.get(x, y + 1) == Some(SKY) {
            canvas.set(cx, cy, Cell::default());   // the text below shows
        }
    }
}
```

`ptyrc_app::sight`: the twin of `ptyrc/sight`: the shape of the view
of a frame, kept current by events, never read at a period.

| | |
|---|---|
| `Options { frame: Value, words: Vec<String>, text: bool, cursor: bool }` | `Options::default()`: `frame` the app's frame told at `init` (`Manifest::frame`), else `"raw"`; no words; the shape; no cursor |
| `Sight::new(options: Options) -> Sight` | subscribes `frames`, `text`, `size`, `move` (and `cursor`); reads `frame.get` |
| `event(&mut self, topic: &str, data: &Value) -> bool` | `true`: `frames`, `text`, `cursor`, taken; `false`: any other, `size` and `move` read (a `move` names the frame read from then on) |
| `look(&mut self) -> bool` | at the tick: reads the view again when an event said so (`screen.shape {frame, words}`; `screen.text` with `text`); `false`: the frame not usable, or the read failed (logged; read at the next `look`) |
| `usable(&self) -> bool`, `touch(&mut self)`, `ink(&self, x: i64, y: i64) -> bool` | the frame known and on screen; read again at the next `look`; `rows[y][x]` not a blank, `false` outside |
| fields | `frame: Value`, `words`, `text`; `changed: bool` (the last `look` read the view); `rows: Vec<String>` (a string a row padded to the width: `#` ink, a blank, a word as shown; or the text); `size: (u32, u32)`; `rect: Option<[i64; 4]>`; `cursor: Option<(i64, i64)>` (view cells; `None`: hidden, or `shape` not held); `degree: Option<String>` (what the read was cut to; `None`: whole); `info: Value` (`frame.get`) |

```rust
let sight = Sight::new(Options { cursor: true, ..Options::default() });
```

Below `layout` (`sees("layout")` false): the raw frame alone, no
`frames`, no `frame.get`.

`ptyrc_app::dice`: draws for code that takes its source as an
argument: the same run for a seed.

| | |
|---|---|
| `trait Dice` | `next_u64(&mut self) -> u64`; provided: `below(n: usize) -> usize`, `below_u64(n: u64) -> u64` (`0..n`, 0 for 0), `unit() -> f64` (`[0, 1)`), `uniform(a: f64, b: f64) -> f64` (`[a, b)`), `chance(p: f64) -> bool`, `weighted(&[f64]) -> Option<usize>` (an index by weight; `None` when they sum to nothing) |
| `Host` | the host's `random` (`--seed` of the launcher) |
| `Seeded::new(seed: u64) -> Seeded` | splitmix64; seeded from `Host`, or by a test |

### Deck

`ptyrc_app::deck`: the twin of the script module `ptyrc/deck`:
widgets the driver places, in the variant that fits, kept drawn. The
deck holds the handlers, the app its state: `Deck<S>` takes `&mut S`
at `add`, `tick`, `event` and gives it to each handler, so
`self.deck.tick(now, &mut self.state)` borrows two fields.

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

Handlers are `'static` closures (`impl FnMut(..) + 'static`); one
returns `true` when something changed: drawn again.

| | |
|---|---|
| `Deck::new(show: Value) -> Deck<S>` | `show`: defaults of `widget.show` (`border`, `title`, `frame`, `prefer`, `fill`, `margin`, `layer`, `modal`; another member panics), or `Null` |
| fields | `every: bool` (`tick` renders each variant); `hover_timeout: Option<u64>` (`Some(1500)` ms); `now: u64` (the session time of the last `tick`) |
| `add(&mut self, spec: Spec<S>, state: &mut S) -> Result<&Widget, Error>` | declares (each variant rendered once) and shows. The first subscribes `widgets`, `move`; `hover`, `leave`: `hover`; `click`: `mouse`; `key`, `paste`: `widget.ask {quiet: true}` (`Spec::ask` overrides) |
| `Spec::new(tag: &str, sizes: impl Into<Sizes>, render: impl FnMut(&mut S, u32, u32, &Widget) -> Content) -> Spec<S>` | `sizes`: `[(w, h), ...]` (named `WxH`) or `[(name, w, h), ...]`, arrays or slices; `render(state, w, h, widget)` |
| `.key(FnMut(&mut S, &str, &Widget) -> bool)`, `.paste(FnMut(&mut S, &str, &Widget) -> bool)` | a key name of the focused widget; a bracketed paste |
| `.hover(FnMut(&mut S, u32, u32, &Widget) -> bool)`, `.leave(FnMut(&mut S, &Widget) -> bool)` | the pointer on a cell of the variant; gone, or past `hover_timeout` |
| `.click(FnMut(&mut S, u32, u32, &str, &Widget) -> bool)` | a press or a wheel turn on a cell of the variant; `button`: `left`, `middle`, `right`, `wheel-up`, `wheel-down`, `wheel-left`, `wheel-right` |
| `.closed(FnMut(&mut S, &Widget))`, `.ask(opts: Value)`, `.show(show: Value)` | closed by the user, out of the deck; `widget.ask` members; `widget.show` members of this widget |
| `Content` | `Rows(Vec<String>)` (a space a hole), `Cells(Vec<Vec<Run>>)`; `From` `&str` (lines by `\n`), `String`, `Vec<&str>`, `Vec<String>`, `Vec<Run>` (one row), `Vec<Vec<Run>>`, `&mut Canvas` (its `runs()`); `to_value() -> Value` |
| `tick(&mut self, now: u64, state: &mut S) -> Result<(), Error>` | ends a hover past its timeout; renders each widget shown and sends the contents that changed in one `batch` (one by one where `batch` answers `headless`) |
| `event(&mut self, topic: &str, data: &Value, state: &mut S) -> Result<bool, Error>` | takes `widgets` (a widget whose variant, focus or `asks` changed is drawn at once), `move`, `hover`, `mouse`, `input` (keys of its focused widget); `false`: not the deck's |
| `draw(&mut self, only: Option<&str>, every: bool, state: &mut S) -> Result<(), Error>` | renders now: the widget `only`, or all; `every`: each variant |
| `resize(&mut self, tag: &str, sizes: impl Into<Sizes>, state: &mut S) -> Result<(), Error>` | other sizes: declared again |
| `move_to(&mut self, frame: Value, state: &mut S) -> Result<(), Error>` | every widget shown again on `frame` (the event `move` does it) |
| `drop(&mut self, tag: &str, state: &mut S)`, `leave(&mut self, state: &mut S) -> Result<(), Error>` | out of the deck, the driver not told; ends the hover under way |
| `widgets() -> impl Iterator<Item = &Widget>`, `widget(tag) -> Option<&Widget>`, `widget_mut(tag) -> Option<&mut Widget>` | in the order added |
| `Widget` | fields `tag: String`, `info: Value` (the driver's last), `hidden: bool`, `now: u64`; `id() -> Option<u64>`, `variant() -> Option<&str>`, `size() -> Option<(u32, u32)>` (inner), `rect() -> Option<[i64; 4]>` (outer, in the cells `hover`, `mouse` are told in: the screen's; below `layout`, `surface = "widget"`, its own, at 0, 0), `placed() -> bool`, `over() -> bool`, `asks() -> &Value`, `focused() -> bool` |
| `Widget::set(&self, content: Option<Content>, variant: Option<&str>, title: Option<&str>) -> Result<(), Error>`; `hide`, `ask(opts: &Value)`, `release` | `widget.set` (the variant shown when `None`); `widget.hide`, `.ask`, `.release`: `(&mut self) -> Result<(), Error>` |
| `Keys` | `Keys::default()`; `feed(&mut self, data: &Value) -> Option<Fed>`: `Fed::Key(String)`, `Fed::Paste(String)` (a bracketed paste put together); `reset(&mut self)`; `Keys::name(bytes: &[u8]) -> Option<String>`: `shift+up`, ... |
| `Field { text: String, cursor: usize }` | a line and its cursor (a byte offset); `Field::new(text: &str)` (the cursor at its end), `insert(&mut self, text: &str)`, `key(&mut self, name: &str) -> bool`, `render(&mut self, w: u32, focused: bool, placeholder: &str, style: &Run) -> Vec<Run>`: the cursor an inverse cell (a text report of `dev test` shows it with `--styles`) |
| `Listing { count, sel, top: usize }` | `Listing::new(count: usize, sel: usize)`; `key(&mut self, name: &str, letters: bool) -> bool`, `window(&mut self, h: u32) -> Range<usize>`, `mark(&self, h: u32) -> String` (`3/12`, or `""` when it fits) |
| `cut(text: &str, n: u32) -> String`, `fit(row: &[Run], w: u32) -> Vec<Run>`, `wrap(text: &str, w: u32, n: u32) -> Vec<String>` | in cells: cut with `…`; a row of exactly `w` cells; `n` lines at most, cut at blanks |

Widths and heights are `u32` throughout, as `render` gets them.

### Host functions

Requests of the protocol (`PROTOCOL.md`, Methods), typed. Each but the
pure ones returns `Result<_, Error>`.

| | |
|---|---|
| `clock::now() -> Result<Wall, Error>` | `Wall { unix_ms: i64, zone: String, offset_s: i32, abbr: String, next: Option<i64>, degree: Option<String> }`; `next`: the next change of offset. `clock` not held: session time from 1970, UTC, `degree: Some("none")` |
| `clock::zone(zone: &str, at: &[i64]) -> Result<Vec<Offset>, Error>` | `Offset { at, offset_s, abbr, next }` of an IANA zone or `"local"`; `at` empty: now |
| `clock::civil(unix_ms: i64, offset_s: i32) -> Civil` | `year: i64`, `month`, `day`, `hour`, `minute`, `second`, `weekday` (`u32`; 0: Sunday); pure |
| `clock::until(unix_ms: i64, every_ms: u32) -> u32` | the `period` to the next multiple of `every_ms` of the wall clock, plus `every_ms` while under 33; pure |
| `storage::get(key) -> Result<Option<String>, Error>`, `set(key, value) -> Result<u64, Error>` (bytes used), `remove(key) -> Result<(), Error>`, `keys() -> Result<Vec<String>, Error>` | strings; permission `storage`, else `denied` |
| `asset::text(name) -> Result<String, Error>`, `bytes(name) -> Result<Vec<u8>, Error>`, `list() -> Result<Vec<String>, Error>` | the tree `[app] assets`, names relative to it; absent: `no-file` |
| `File::of(arg: &Value) -> Option<File>`, `File::all(arg: &Value) -> Vec<File>`, `File::new(handle: &str) -> File` | a file or directory the user named (an option of type `file`, `dir`: `File::of(&args["opt"])`); fields `handle`, `name: String` |
| `File` methods | `text() -> Result<String, Error>`, `bytes() -> Result<Vec<u8>, Error>`, `text_with_mtime() -> Result<(String, i64), Error>`, `bytes_with_mtime()`, `stat() -> Result<Stat, Error>` (`Stat { exists, size: u64, mtime_ms: i64 }`), `write(text: &str, if_mtime_ms: Option<i64>) -> Result<Stat, Error>`, `write_bytes(&[u8], Option<i64>)`, `list() -> Result<Vec<String>, Error>`, `child(name) -> File` |
| `result(value: Value, status: u8) -> Result<(), Error>` | what `ptyrc app ID --wait` prints, and its exit status |
| `base64::encode(&[u8]) -> String`, `decode(&str) -> Option<Vec<u8>>` | RFC 4648, padded; pure |

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
MS:NAME`, `--event MS:TOPIC:JSON`, `--pointer MS:X,Y`, `--click
MS:X,Y`: what the user does.
