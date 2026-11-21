# Drawing and sight API, script SDK (agent `sdk-js`, 2026-11-20)

Settled before the implementation; `sdk/js/ptyrc.d.ts` is the contract
once committed. The Rust twin (`sdk-rust`) mirrors the names: a JS
method `x(a, b = 1)` is a Rust method `x(a, b)`; a JS getter is a Rust
method of no argument; camelCase becomes snake_case.

Colours here: the number of `rgb(r, g, b)` ("ptyrc"), in Rust
`Color::Rgb(r, g, b)`. Inputs also take `"#rrggbb"`. A palette number
is `0..255`. Coordinates 0-based, `x` right, `y` down; a dot, a pixel
or a cell outside is ignored (a read outside: `false` / `null`).

## `"ptyrc/canvas"` (Rust: `ptyrc_app::canvas`)

Braille: `2 * width` by `4 * height` dots in `width` by `height` cells.

| name | what |
|---|---|
| `DOTS` | `DOTS[y][x]`: the bit of the dot `(x, y)` of a cell, `[[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]]` |
| `new Braille(width, height)` | all dots off; `RangeError` unless two integers >= 1 |
| `.width`, `.height` | cells |
| `.bits` | `Uint8Array` (Rust `Vec<u8>`), `width * height`, the cell `(cx, cy)` at `cy * width + cx`; the glyph is `U+2800 + bits` |
| `.set(x, y, on = true)` | a dot |
| `.get(x, y) -> bool` | |
| `.line(x0, y0, x1, y1, on = true)` | every dot from `(x0, y0)` to `(x1, y1)`: `n = max(1, round(max(|dx|, |dy|)))` steps, each dot rounded (`_canvas.Braille.line`) |
| `.clear() -> this` | every dot off |
| `.glyph(cx, cy) -> string` | the cell's glyph; `""` when no dot of it is on |
| `.cells() -> [cx, cy][]` | the cells with a dot on, row by row |
| `.rows() -> string[]` | a string a row of cells, a space where no dot is on: a widget's content, or `draw.*` rows |
| `.draw(canvas, x = 0, y = 0, style?) -> this` | each row written on a `Canvas` at `(x, y)` as `canvas.text` (a blank is a hole unless `style` has a `bg`) |

Half blocks: `width` by `2 * height` pixels of colour in `width` by
`height` cells, the top pixel the foreground of `▀`, the bottom its
background; glyphs over them.

| name | what |
|---|---|
| `NEAR` | 24: two pixels are alike when the sum of their channel differences is at most this |
| `new Half(width, height, color = rgb(0, 0, 0))` | |
| `.width`, `.height` | cells (pixels: `width` by `2 * height`) |
| `.fill(color) -> this` | every pixel; the glyphs kept |
| `.get(x, y) -> color \| null` | a pixel |
| `.set(x, y, color, alpha = 1)` | a pixel; `alpha` under 1: `mix(old, color, alpha)` |
| `.rect(x, y, w, h, color, alpha = 1)` | |
| `.disc(cx, cy, r, color, alpha = 1)` | the pixels whose centre `(x + .5, y + .5)` is within `r` of `(cx, cy)` |
| `.put(cx, cy, text, fg = null, bg = null)` | glyphs from the cell `(cx, cy)`, one a code point; `bg` null: the mean of the two pixels under |
| `.ink(braille, fg = null, tint = 1, rough = 0)` | the cells of `braille` with a dot, where no glyph is and the two pixels are alike: the glyph, its `bg` the pixel under most dots, `fg` mixed over it by `tint`; where they differ, `fg` mixed by `rough` into each pixel under a dot (`_canvas.Half.ink`) |
| `.rows(deep = false) -> Run[][]` | rows of runs `{t, fg?, bg}`: colours of the 256 (`xterm`), `deep`: `"#rrggbb"`; cells alike one run, a blank joins the run before it; ink on two pixels the 256 tell apart is left out |
| `.draw(canvas, x = 0, y = 0, deep = false) -> this` | the cells on a `Canvas` at `(x, y)` |

Colours:

| name | what |
|---|---|
| `mix(a, b, t) -> color` | from `a` (`t` 0) to `b` (`t` 1), `t` clamped, each channel rounded |
| `channels(c) -> [r, g, b]` | |
| `hex(c) -> "#rrggbb"` | |
| `xterm(c) -> 16..255` | the nearest of the 240 fixed colours of the 256 (`_canvas.xterm`: weights 3, 4, 2, a lost chroma counted) |
| `palette(n) -> color` | the value of colour `n`, `16..255`; `RangeError` else |
| `named(name) -> 0..15` | `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, and `bright-` each: the 16 the terminal sets; `RangeError` else |

## `"ptyrc/sight"` (Rust: `ptyrc_app::sight`)

The shape of the view of a frame, kept current by events (`_cli.Sight`):
nothing is read at a period.

| name | what |
|---|---|
| `new Sight({frame = "raw", words = [], text = false, cursor = false, call})` | subscribes `frames`, `text`, `size`, `move` (and `cursor`); `call`: the request function (default `call` of "ptyrc"; a fake one in unit tests) |
| `.event(topic, data) -> bool` | takes `frames`, `text`, `cursor` (true); reads `size`, `move` (a `move` names the frame read from then on) and returns false for them |
| `.look() -> bool` | at the tick: reads the view again when an event said it changed (`screen.shape {frame, words}`, or `screen.text` with `text`); false while the frame is not usable (off screen) |
| `.changed` | the last `look` read the view |
| `.rows -> string[]` | the view, one string a row, padded to the width: `#` ink, blank, a word of `words` as shown (`text`: the text) |
| `.size -> [w, h]` | the view's size |
| `.rect -> [x, y, w, h] \| null` | the frame on the screen |
| `.cursor -> [x, y] \| null` | in view cells; null: hidden, or `shape` not held |
| `.degree -> string \| null` | what the last read was cut to (`"none"`, `"shape"`, `"words"`); null: whole |
| `.frame` | the frame read (id or name) |
| `.usable -> bool` | |
| `.touch()` | read again at the next `look` |
| `.ink(x, y) -> bool` | `rows[y][x]` is not a blank; false outside |

Degrees: `layout` refused: the raw frame, no `frames` event; `shape`
refused: blank rows of the size, `cursor` null, `degree: "none"`;
`words` held: the words of the permission shown; `text` refused with
`text: true`: the rows of the shape.

## `"ptyrc/deck"` additions

| name | what |
|---|---|
| `AddOptions.click(x, y, button, widget)` | a press (`action` `press`) or a wheel turn on the cell `(x, y)` of the inner box (border excluded): `button` `left`, `middle`, `right`, `wheel-up`, `wheel-down`, `wheel-left`, `wheel-right`; returns `false`: nothing changed. Subscribes `mouse` |
| `new Deck({..., call})` | `call`: the request function; a fake one makes a deck usable under `--unit` |
| `deck.event("mouse", data)` | true |
