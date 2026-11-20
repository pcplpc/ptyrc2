<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The protocol, as a store app sees it

`call(method, params)` sends one request; it returns the `result` or
throws (Rust: `Err`) `{code, message, hint?}`. Coordinates are 0-based,
`x` right, `y` down. `frame`: a frame id or name, default `"raw"` (the
whole screen). `rect`: `[x, y, w, h]`. Durations are session time, ms.
Checked against the driver by `rust/ptyrc-box/tests/protocol.rs`.

## Permissions

Declared in `[permissions]` (the limits: in `[limits]`, Limits),
granted by the user at the start, one yes or no each. Not declared:
never held. A degree (`layout` < `shape` < `words` < `text`) is held
only with every declared degree below it. No screen to ask (a headless
session): what has no answer is granted as declared, each so granted
named in a warning on the stderr of who started it.

Held without asking (the default profile): drawing on its own layer,
behind the text, on its surface; its own sprites and widgets; the size;
`tick`; the keys of its focused widget; the pointer and the clicks on
its own widget, or its own sprite in front of the text.

| name | opens | a no gives | colour |
|---|---|---|---|
| `layout` | `frame.list`, `frame.get`, `frame.at`; topic `frames` | one frame, the raw one | plain |
| `shape` | where there is ink: `screen.shape`, `screen.cursor`, `screen.wait` without `pattern`, `frame.heat`, `frame.idle`; topics `screen`, `cursor`, `text`, `idle`, `colors` | a blank screen (below) | plain |
| `words` | `shape`, and the words listed: `screen.shape {words}`, `screen.find` of a listed word | `shape` | plain, the words shown |
| `text` | every cell: `screen.text`, `screen.cells`, `screen.find`, `screen.wait {pattern}`, `frame.text`, all of `session.info` | `words`, else `shape`, else blank | red |
| `front-widget` | a widget in front | behind | plain |
| `front` | sprites in front (`layer` > 0, `"front"`, `"hud"`); widgets too | behind | amber |
| `screen-wide` | `surface = "screen"`: anywhere | clipped to its frame | amber |
| `keys-arrows` | `input.grab` of keys that are not text: arrows, `home`, `end`, `pageup`, `pagedown`, `insert`, `delete`, `f1`..`f12` | `denied` | plain |
| `keys-grab` | any other `input.grab`; `widget.show {modal: true}` | `denied` | red |
| `keys-read` | topic `input`: every key | subscribed, no event | red |
| `pointer` | topic `hover` elsewhere than its own things: moves anywhere | no event there | plain |
| `clicks` | topic `mouse` elsewhere than its own things: clicks anywhere | no event there | red |
| `clicks-grab` | a click it is told elsewhere taken from the program | the program has it too | red |
| `limit-memory` | `[limits]` `memory-mib`, `canvas-screens`, `log-mib` | the default | plain |
| `limit-time` | `[limits]` `tick-ms`, `init-ms`, `calls-per-second` | the default | plain |
| `clock` | `clock.now`, `clock.zone`: the wall clock, zones | session time as the epoch, UTC, `"degree": "none"` | plain |
| `storage` | `storage.*`: 64 named values, 1 MiB, its own | `denied` | plain |
| `files` | `file.*` of a file or directory the user names (`[options] x = { type = "file" }`); asked when the manifest's default is used | `no-file` | plain |
| `files-write` | `file.write` of a file whose option says `write = true` | `denied` | red |
| `probe-git` | `probe.git`: `git` run by the driver in the program's directory, or a `dir` named | `{"repo": null, "degree": "none"}` | plain |
| `probe-system` | `probe.system`: processes, logins, listening ports, disks | every part empty, `"degree": "none"` | plain |

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
| `app.result` | `value` (any JSON, 64 KiB), `status` (0..255, default 0) | `{}`; the last holds; told in `app.list` as `"result": {"value","status"}`; `ptyrc app ID --wait` prints it and exits with the status | — |
| `clock.now` | | `{"unix_ms","zone","offset_s","abbr","next"}`: the machine's zone; `next`: when the offset changes, ms, or null | `clock` |
| `clock.zone` | `zone` (an IANA name, or `"local"`), `at`: 1..64 instants, ms (absent: now) | `{"zone","offsets":[{"at","offset_s","abbr","next"}]}`; an unknown zone: `bad-params` | `clock` |
| `storage.get` | `key` | `{"value": str \| null}` | `storage` |
| `storage.set` | `key`, `value` (a string, 256 KiB) | `{"used","quota"}`; past 64 keys or 1 MiB: `not-usable`, nothing written; 16 a second | `storage` |
| `storage.remove` | `key` | `{}` | `storage` |
| `storage.list` | | `{"keys":[{"key","size"}],"used","quota"}` | `storage` |
| `file.stat` | `file` (a handle), `name` (a file of a `dir`) | `{"exists", "size", "mtime_ms"}`; absent: `{"exists": false}` | — |
| `file.read` | `file`, `name`, `as` (`"text"`, `"base64"`), `offset`, `len` (1 MiB at most) | `{"text" \| "base64", "size", "mtime_ms", "eof"}`; a file of 16 MiB at most | — |
| `file.list` | `file` (a `dir`) | `{"entries":[{"name","size"}]}`: its regular files of a valid name, 1024 at most | — |
| `file.write` | `file`, `text` or `base64` (the whole, 1 MiB), `if_mtime_ms` | `{"size","mtime_ms"}`; another mtime: `not-usable`; 16 a second with `storage.set` | `files-write`, and `write = true` |
| `probe.git` | `repo` (a `dir` handle; absent: the program's directory), `parts`, `log` (1..200), `since` (a stamp) | `{"repo": {...} \| null, "stamp"}`; `since` the stamp now: `{"unchanged": true, "stamp"}`; a real read a second | `probe-git` |
| `probe.system` | `parts` (`glance`, `logins`, `listeners`, `disks`), `top` (0..16) | `{"glance", "logins", "listeners", "disks"}`; shares between two samples, the first zeros | `probe-system` |

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

Host functions (`clock.*`, `storage.*`, `app.result`, `file.*`,
`probe.*`):

- A `key`: 1 to 64 bytes of `[A-Za-z0-9._-]`, not starting with `.`;
  else `bad-params`. The values are the app's origin's: kept by an
  update, removed by `ptyrc store rm --purge`.
- `clock.*` held, `storage.*`, `file.*`, `probe.*` held read the
  disk or run `git`: not in a `batch` (`bad-request`).
- A file is a handle, never a path: `init(args)` gets `{"file": "f0",
  "name": "TODO.md"}` for an option of type `file` or `dir`. A handle
  not given: `no-file`. `name`: as a `key`. A link in place of the
  file, or of a file of the directory, is not followed: absent. A
  directory moved since it was named: `not-usable`.
- A tick on the second: `period(1000 - unix_ms % 1000)` once, then
  `period(1000)`. At `next`, ask `clock.now` again.

## Topics

`event(topic, data)`, after `subscribe`. Not held: subscribed, no event.

| topic | data | needs |
|---|---|---|
| `size` | `{"size":[c,r]}` | — |
| `draw` | `{"removed":[id],"frame","rows":[from,to]\|null}`: its sprites removed by the driver | — |
| `widgets` | widget info, when it changes | — |
| `move` | `{"frame":id}`: the user moved the app, or its frame left the screen and a pane took its place (its sprites moved along; `size` follows) | — |
| `exit` | `{"status"}`: the program ended | — |
| `frames` | frame infos | `layout` |
| `screen` | `{"rows":[y]}`: screen rows changed | `shape` |
| `text` | `{"frame","rows":[y],"origin","rect"}`: rows whose text changed | `shape` |
| `cursor` | `screen.cursor` without the trail | `shape` |
| `idle` | `{"frame","idle","score","free"}`, when `idle` changes | `shape` |
| `colors` | host colours | `shape` |
| `input` | `{"key","bytes","grabbed":true}` (a grab), `{"key","bytes","widget"}` (a focused widget), `{"released":"escape"}`; every key | `keys-read` for every key; a grab's and a focused widget's come unsubscribed |
| `mouse` | `{"sprite","button","action","mods","at","x","y","frame","pos"}` | — on its own widget or sprite in front; elsewhere `clicks` |
| `hover` | `{"sprite","at","x","y","frame","pos","mods"}`; `sprite` null: left | — on its own widget or sprite in front; elsewhere `pointer` |

- `screen.*` answers the wrapped program's cells, never the composed
  picture: an app never reads its own sprites, nor another's.
- Poll or subscribe: a `call` costs one of 1000 a second. Subscribe to
  `text` or `screen` and read on the event; poll at the tick only when
  that topic is not held. `size` is always worth subscribing.

## Errors

`denied` (not held, or closed), `bad-params`, `bad-request`,
`unknown-method`, `no-frame`, `no-sprite` (also another app's),
`not-usable` (a limit of the driver, a grab held elsewhere), `timeout`,
`gone`, `cancelled`, `no-file`, `internal`; of the host: `limit` (more than the
calls a second), `headless`.

## Headless (`ptyrc dev test`)

- Answers `batch`, `draw.put`, `draw.update`, `draw.remove`,
  `draw.clear`, `draw.list`, `draw.ext`, `draw.exts`,
  `widget.declare`, `widget.show`, `widget.set`, `widget.hide`,
  `widget.ask`, `widget.release`, `widget.list`, `widget.borders`,
  `app.list` (its own entry, `pid` null), `subscribe`, `input.grab`,
  `input.release`, `session.info`, `screen.text`, `screen.shape`,
  `screen.cells`, `screen.find`, `screen.wait`, `screen.cursor`,
  `frame.list`, `frame.get`, `frame.at`, `frame.idle`, `frame.text`,
  `frame.heat`, cut as the driver cuts them. `screen.wait` is answered
  at once: no session time passes in a call; not true now,
  `{"matched": false}`, in a `batch` `bad-request` (the driver would
  wait). Another method of the table above: `headless`
  (the hint says so; a session answers it, `ptyrc dev run`). A name
  not in it: `unknown-method`, with the nearest.
- Host functions, from memory, the driver's answers: `clock.*` from
  `--clock ISO` (default `2000-01-01T00:00:00Z`) advancing with session
  time, local zone `--zone NAME` (default `UTC`); `storage.*` empty,
  or the files of `--storage DIR` (never written back); `app.result`;
  `file.*` on `--file NAME=PATH` (the handle under `NAME` in `init`;
  writes in memory); `probe.*` from `--probe git=FILE.json` (none: the
  not-held answer); `asset.*` from `[app] assets`. The report:
  `result:` (and the status), `storage:`, `files:` (written); `--json`
  their values.
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
  `--event MS:TOPIC:JSON`, delivered when subscribed and held;
  `hover` (`--pointer MS:X,Y`) and `mouse` (`--click MS:X,Y`: press,
  release) on the app's sprite under the cell, heard as the driver
  hears them; `--focus MS:TAG` gives the keys to its widget that asks,
  then `--key` goes to it (its escape and close keys too) and
  `--paste MS:TEXT`; `widgets` when a widget is laid out.
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
surface = "screen"         # "widget" (default), "frame", "screen": where it is clipped
# assets = "assets/"; tape = "demo.tape"
# wait = true            # `ptyrc app ID` waits for its result (app.result)

[options]                  # --NAME VALUE, --NAME=VALUE, -S VALUE; a bool --NAME, --no-NAME
size = { type = "int", default = 20, doc = "one line" }
grab = { type = "bool", default = false, short = "g", needs = ["keys-arrows"], doc = "..." }
season = { type = "string", default = "auto", choices = ["auto", "spring"], doc = "..." }
word = { type = "word", doc = "..." }                   # the user's word
mark = { type = "pattern", list = true, doc = "..." }    # repeated; regex = true
stop = { type = "file", doc = "..." }                    # or "dir"; write = true
# types: int, float, bool, string, word, pattern, file, dir
# reserved: alpha, back, border, frame, front, help, no-wait, prefer, wait; no-NAME

[args]                     # one list of positionals, init(args).NAME
patterns = { type = "pattern", min = 1, max = 16, doc = "..." }

[permissions]              # NAME = "why, one line, 200 at most"
shape = "to stand on the text"
words = { list = ["food", "fish"], why = "to find them" }  # 1..32 words, 3..32 letters or digits

[limits]                   # above the default, up to the ceiling (Limits)
memory-mib = 128
```

- `init(args)`: every option, its default when not given (`null`
  when it has none; `[]` for a list), the positionals under their name.
- `default`: required of `int`, `float`, `bool`, `string`; none of a
  list. Of a `file` or `dir`: a plain name of the working directory,
  used when none is named, asked as `files`.
- `needs`: permissions asked only when the option is given (a bool:
  true); each in `[permissions]`. The pop-up then shows them alone.
- `word`: 3 to 32 letters or digits, else refused at the start. Found
  by `screen.find` and `screen.wait` whatever the degree, shown in
  `screen.shape` from `shape`; not counted in the 32 of `words`.
- `pattern`: 1 to 256 bytes, 16 a run. `screen.find`, `screen.wait`
  with exactly that pattern (and its `regex`) are answered as under
  `text`, `known` too; any other pattern as before.
- `file`, `dir`: require `files` in `[permissions]`; `write = true`
  (a file only) requires `files-write`. The guest gets `{"file":
  "f0", "name": "TODO.md"}`, never a path; reading is granted by the
  naming; a file to write asks `files-write`. Regular files and
  directories only; 16 a run.
- ptyrc's own options, the same for every app and never told to it:
  `--frame F`, `--front`, `--back`, `--prefer SIDE`, `--alpha A` (the
  opacity of its sprites), `--border B` (of its widgets), `--wait`.
- `--help` is made from the manifest; `ptyrc dev check` checks it.

At `init`, `args["ptyrc:manifest"]`: `{"id", "version", "permissions":
[{"name", "words"?}], "granted": [name], "limits": {KEY: N}}`, the
limits granted alone (Rust: `manifest()`).

## Limits

More than a default is declared in `[limits]`, a key each, and asked
as its permission: the pop-up shows the figure ("use up to 128 MiB of
memory"). Granted: applied; a no: the default. A value past the
ceiling: an error at `ptyrc dev check` and at install, naming it.

| | default | `[limits]` key | ceiling | permission |
|---|---|---|---|---|
| `tick`, `event`: late | 50 ms; three late in a row: paused | `tick-ms` | 200 | `limit-time` |
| `tick`, `event`, `stop`: ended | 2 s (`stop`: 500 ms) | | | |
| `init` | 2 s | `init-ms` | 10000 | `limit-time` |
| memory | 64 MiB | `memory-mib` | 512 | `limit-memory` |
| `call` | 1000 a second, then `limit` | `calls-per-second` | 10000 | `limit-time` |
| canvases, all | four screens of cells | `canvas-screens` | 16 | `limit-memory` |
| log | 1 MiB | `log-mib` | 16 | `limit-memory` |
| `period` | 33 ms at least | |
| a JSON argument or answer | 4 MiB | |
| scripts, all | 1 MiB | |
