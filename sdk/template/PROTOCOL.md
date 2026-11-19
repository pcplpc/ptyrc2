<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The protocol, as a store app sees it

`call(method, params)` sends one request; it returns the `result` or
throws (Rust: `Err`) `{code, message, hint?}`. Coordinates are 0-based,
`x` right, `y` down. `frame`: a frame id or name, default `"raw"` (the
whole screen). `rect`: `[x, y, w, h]`. Durations are session time, ms.
Checked against the driver by `rust/ptyrc-box/tests/protocol.rs`.

## Permissions

Declared in `[permissions]`, granted by the user at the start, one yes
or no each. Not declared: never held. A degree (`layout` < `shape` <
`words` < `text`) is held only with every declared degree below it.

Held without asking (the default profile): drawing on its own layer,
behind the text; its own sprites and widgets; the size; `tick`; the
keys of its focused widget; the pointer on its own things.

| name | opens | a no gives | colour |
|---|---|---|---|
| `layout` | `frame.list`, `frame.get`, `frame.at`; topic `frames` | one frame, the raw one | plain |
| `shape` | where there is ink: `screen.shape`, `screen.cursor`, `screen.wait` without `pattern`, `frame.heat`, `frame.idle`; topics `screen`, `cursor`, `text`, `idle`, `colors` | a blank screen (below) | plain |
| `words` | `shape`, and the words listed: `screen.shape {words}`, `screen.find` of a listed word | `shape` | plain, the words shown |
| `text` | every cell: `screen.text`, `screen.cells`, `screen.find`, `screen.wait {pattern}`, `frame.text`, all of `session.info` | `words`, else `shape`, else blank | red |
| `front-widget` | a widget in front | behind | plain |
| `front` | sprites in front (`layer` > 0, `"front"`, `"hud"`); widgets too | behind | amber |
| `screen-wide` | `surface = "screen"` | not enforced yet | amber |
| `keys-arrows` | `input.grab` of keys that are not text: arrows, `home`, `end`, `pageup`, `pagedown`, `insert`, `delete`, `f1`..`f12` | `denied` | plain |
| `keys-grab` | any other `input.grab`; `widget.show {modal: true}` | `denied` | red |
| `keys-read` | topic `input`: every key | subscribed, no event | red |
| `pointer` | topic `hover`: moves anywhere | subscribed, no event | plain |
| `clicks` | topic `mouse`: clicks on sprites | subscribed, no event | red |
| `clicks-grab` | nothing yet | | red |
| `clock`, `storage`, `files`, `files-write`, `probe-git`, `probe-system` | nothing yet: accepted, unused | | `files-write` red |

- A reading not held is answered in the same form, cut to the degree
  held, with `"degree": "none" | "layout" | "shape" | "words"` in the
  answer. Never an error: go on with less.
- An act not held: `denied`, the hint naming the permission.
- Every other method of the driver (typing or clicking into the
  program, tapes, layers, focus, other apps, debug): `denied`,
  whatever is held. Topic `apps`: `denied`.

Cut answers:

| asked | held | answered |
|---|---|---|
| `screen.text`, `screen.cells` | `shape`, `words` | the rows of `screen.shape` (`#` for ink, the held words as on screen); `cells`: one run a row, `{"t"}` alone |
| `screen.text`, `.cells`, `.shape` | less than `shape` | empty lines, the size and the rect |
| `screen.shape {words}` | `shape` | no word; `words`: the held words among those asked |
| `screen.find`, `screen.wait {pattern}` | `words` | a held word only, literal, whole, case ignored; else no match (`matched` false at once) |
| `screen.cursor` | less than `shape` | not visible, at 0, 0 |
| `frame.idle`, `screen.wait` without `pattern` | less than `shape` | `{"score":1,"idle":true,"free":1}`; matched at once |
| `frame.heat` | less than `shape` | `9` and `.` everywhere |
| `frame.text` | `shape`, `words`; less | each line as its shape; `{"lines":[],"y0":0}` |
| `frame.list`, `frame.get`, `frame.at` | less than `layout` | the raw frame alone; `frame` of every request ignored |
| `session.info` | less than `text` | `protocol`, `server`, `size`, `headless`; `host_colors` from `shape` |

## Methods

`needs`: the permission; `—`: the default profile.

| method | params | result | needs |
|---|---|---|---|
| `auth` | `token` | done by the host; never call it | — |
| `batch` | `calls`: `[{method, params}]` | `{"results":[{"result"} or {"error"}]}`, one repaint | each call its own |
| `subscribe` | `topics`: `[name]`, `snapshot` (bool: the state now, as events) | `{}`; an unknown topic: `bad-params` | each topic its own |
| `session.info` | | `{"protocol","server","size":[c,r],"headless",...}` | `text` for the rest |
| `screen.text` | `frame`, `rect` | `{"lines":[str],"size":[c,r],"rect":[x,y,w,h]}`, one string a row, trailing spaces cut | `text` |
| `screen.cells` | `frame`, `rect` | `{"rows":[[run]],"size","rect"}`; a run `{"t":text, style...}` | `text` |
| `screen.find` | `pattern`, `regex`, `ignore_case`, `frame`, `known` (remembered rows too) | `{"matches":[{"frame","x","y","len","text","screen":[c,r]\|null}]}` | `text`; a listed word: `words` |
| `screen.shape` | `frame`, `rect`, `words` (≤ 32, 3..32 letters or digits) | as `screen.text`: `#` per inked cell, `##` a wide glyph, a word matched as on screen | `shape`; `words` |
| `screen.cursor` | `trail` | `{"x","y","visible","frame","pos":[x,y],"trail":[[ms,x,y]]}` | `shape` |
| `screen.wait` | `pattern` or `stable_ms` or `idle` (or `gone`), `timeout_ms`, `frame` | `{"matched":bool,"match":...}` | `pattern`: `text`; else `shape` |
| `input.grab` | `keys`: names (absent: all), `escape` (`"ctrl+t"`) | `{"escape":name}`; keys come as topic `input` | `keys-arrows`, `keys-grab` |
| `input.release` | | `{}` | — |
| `frame.list` | | `{"frames":[info],"layout":id}` | `layout` |
| `frame.get` | `frame` | `{"frame":info}` | `layout` |
| `frame.at` | `x`, `y`, or `name` | `{"frame":id,"pos":[x,y]}` | `layout` |
| `frame.text` | `frame`, `y0`, `y1` | `{"lines":[str],"y0":n}`: the remembered buffer | `text` |
| `frame.heat` | `frame` | `{"rows":["0123..."],"classes":[...]}` | `shape` |
| `frame.idle` | `frame` | `{"score":0..1,"idle":bool,"free":0..1}` | `shape` |
| `draw.put` | a sprite (below) | `{"id":n}` | —; in front: `front` |
| `draw.update` | `id`, members to change; `effect` tunes the running one | `{}` | — |
| `draw.remove` | `id` | `{}` | — |
| `draw.clear` | | `{"removed":n}`: its own | — |
| `draw.list` | | `{"sprites":[...]}`: its own | — |
| `draw.ext` | `name`, `function`, `params`, sprite members; `id`: redraw that one | `{"id":n}` | — |
| `draw.exts` | | `{"exts":[{"name","functions"}],"effects":[name]}` | — |
| `widget.declare` | `widgets`: `[{tag, name, size:[w,h], content}]`, `border` | `{"tags":[...]}` | — |
| `widget.show` | `tag`, `frame`, `prefer`, `fill`, `margin`, `title`, `border`, `layer`, `modal` | `{"widget":info}` | —; `modal`: `keys-grab`; in front: `front-widget` |
| `widget.set` | `id`, `variant`, `content`, `title` | `{}` | — |
| `widget.hide` | `id` | `{}` | — |
| `widget.ask` | `id`, `quiet`, `escape`, `close`: the user may focus it | `{"widget":info}`; keys come as topic `input` | — |
| `widget.release` | `id` | `{}` | — |
| `widget.list` | | `{"widgets":[info]}`: its own | — |
| `widget.borders` | | `{"borders":[name]}` | — |
| `app.list` | | its own entry | — |

Sprite (`draw.put`; a `Canvas` makes one):
`{"frame", "anchor", "content": {"rows": [str], "style": {...}},
"frames": [{"rows", "ms"}], "loop", "layer", "alpha": 0..1, "z",
"ttl_ms", "persist", "motion": {"vx","vy","ax","ay","bounce"},
"effect": {"name", "params"}, "size": [w, h]}`. `anchor`: `{"at": [x,
y], "space": "view" | "frame"}`, `{"text": "word", "side": "right" |
"left" | "above" | "below" | "over"}`, `{"auto": {"prefer":
"top-right"}}`, `{"cursor": {"dx": 1, "dy": 0}}`. `layer`: absent or
negative, behind the text; in front only with `front` (else behind).
Effects: `draw.exts`. Limits of the driver: 256 sprites, 16 widgets,
32 subscriptions; past one, `not-usable`.

## Topics

`event(topic, data)`, after `subscribe`. Not held: subscribed, no event.

| topic | data | needs |
|---|---|---|
| `size` | `{"size":[c,r]}` | — |
| `draw` | `{"removed":[id],"frame","rows":[from,to]\|null}`: its sprites removed by the driver | — |
| `widgets` | widget info, when it changes | — |
| `move` | `{"frame":id}`: the user moved the app | — |
| `exit` | `{"status"}`: the program ended | — |
| `frames` | frame infos | `layout` |
| `screen` | `{"rows":[y]}`: screen rows changed | `shape` |
| `text` | `{"frame","rows":[y],"origin","rect"}`: rows whose text changed | `shape` |
| `cursor` | `screen.cursor` without the trail | `shape` |
| `idle` | `{"frame","idle","score","free"}`, when `idle` changes | `shape` |
| `colors` | host colours | `shape` |
| `input` | `{"key","bytes","grabbed":true}` (a grab), `{"key","bytes","widget"}` (a focused widget), `{"released":"escape"}`; every key | `keys-read` for every key; a grab's and a focused widget's come unsubscribed |
| `mouse` | `{"sprite","button","action","mods","at","x","y","frame","pos"}` | `clicks` |
| `hover` | `{"sprite","at","x","y","frame","pos","mods"}` | `pointer` |

- `screen.*` answers the wrapped program's cells, never the composed
  picture: an app never reads its own sprites, nor another's.
- Poll or subscribe: a `call` costs one of 1000 a second. Subscribe to
  `text` or `screen` and read on the event; poll at the tick only when
  that topic is not held. `size` is always worth subscribing.

## Errors

`denied` (not held, or closed), `bad-params`, `bad-request`,
`unknown-method`, `no-frame`, `no-sprite` (also another app's),
`not-usable` (a limit of the driver, a grab held elsewhere), `timeout`,
`gone`, `cancelled`, `internal`; of the host: `limit` (more than the
calls a second), `headless`.

## Headless (`ptyrc dev test`)

- Answers `draw.put`, `draw.update`, `draw.remove`, `draw.clear`,
  `widget.declare`, `widget.show`, `widget.set`, `widget.hide`,
  `subscribe`, `input.grab`, `input.release`, `session.info`,
  `screen.text`, `screen.shape`, `screen.find`, `screen.wait`,
  `screen.cursor`, `frame.list`, `frame.get`, `frame.at`,
  `frame.idle`, cut as the driver cuts them. `screen.wait` is answered
  at once: no session time passes in a call; not true now,
  `{"matched": false}`. Another method of the table above: `headless`
  (the hint says so; a session answers it, `ptyrc dev run`). A name
  not in it: `unknown-method`, with the nearest.
- The permissions are those the manifest declares (`[limits]`:
  `limit-memory`, `limit-time`); `--deny a,b` answers them no. The
  report names each request `refused`, each answer `lowered` and the
  permission it lacked; one not declared is fine if intended: an app
  that declares less is asked less.
- Drawing as the driver draws: in front only with `front` (a widget:
  or `front-widget`); else behind the text.
- Events: `size` on a tape's resize; `screen` and `text` when the
  screen changes (a tape, a `--screen T:FILE`); `input` for a grabbed
  key given by `--key MS:NAME` (`ctrl+t` releases); any topic by
  `--event MS:TOPIC:JSON`, delivered when subscribed and held.
- The screen: `--screen [T:]FILE`, again for each change (text: the
  screen cleared at T, the text written from the top left; escapes
  read as a terminal would; a line wider than the screen wraps, rows
  past the last scroll, as in a terminal, and the report says so),
  `--tape FILE` (a recording, played in session time). `--seed N`:
  `random`. `--styles`: under each screen, a grid of its cells not of
  the default style, a letter each, and the legend; `--json`: per
  screen `styles`, `[y, x, w, "fg=11 bold"]`.

## Manifest (`ptyrc-app.toml`)

```toml
[app]
id = "my-app"              # a-z, 0-9, -; a letter first; 64 at most
version = "0.1.0"
summary = "One line, 200 at most."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"          # or rust = "src/lib.rs", rustc = "1.85"
surface = "screen"         # "widget" (default), "frame", "screen"; not enforced yet
# assets = "assets/"; tape = "demo.tape"

[options]                  # each: --NAME VALUE, --NAME, --no-NAME; init(args).NAME
size = { type = "int", default = 20, doc = "one line" }   # int, float, bool, string
# reserved: alpha, back, frame, front, help, prefer

[permissions]              # NAME = "why, one line, 200 at most"
shape = "to stand on the text"
words = { list = ["food", "fish"], why = "to find them" }  # 1..32 words, 3..32 letters or digits

[limits]                   # only above the default; accepted, not applied yet
memory-mib = 128
```

## Limits

| | default | `[limits]` key |
|---|---|---|
| `tick`, `event`: late | 50 ms; three late in a row: paused | `tick-ms` |
| `tick`, `event`, `stop`: ended | 2 s (`stop`: 500 ms) | |
| `init` | 2 s | `init-ms` |
| memory | 64 MiB | `memory-mib` |
| `call` | 1000 a second, then `limit` | `calls-per-second` |
| canvases, all | four screens of cells | `canvas-screens` |
| log | 1 MiB | `log-mib` |
| `period` | 33 ms at least | |
| a JSON argument or answer | 4 MiB | |
| scripts, all | 1 MiB | |
