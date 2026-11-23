<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The protocol, as a store app sees it

`call(method, params)` sends one request; it returns the `result` or
throws (Rust: `Err`) `{code, message, hint?}`. Coordinates are 0-based,
`x` right, `y` down. `frame`: a frame id or name, default `"raw"` (the
whole screen). `rect`: `[x, y, w, h]`. Durations are session time, ms.
`ptyrc dev test` answers as the driver does (Headless, below).

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
| `layout` | `frame.list`, `frame.get`, `frame.at`; topic `frames` | one frame, its surface's, told as 0 | plain |
| `shape` | where there is ink: `screen.shape`, `screen.cursor`, `screen.wait` without `pattern`, `frame.heat`, `frame.idle`; topics `screen`, `cursor`, `text`, `idle`, `colors` | a blank screen (below) | plain |
| `words` | `shape`, and the words listed: `screen.shape {words}`, `screen.find` of a listed word | `shape` | plain, the words shown |
| `text` | every cell: `screen.text`, `screen.cells`, `screen.find`, `screen.wait {pattern}`, `frame.text`, all of `session.info` | `words`, else `shape`, else blank | red |
| `front-widget` | a widget in front | behind | plain |
| `front` | sprites in front (`layer` > 0, `"front"`, `"hud"`); widgets too | behind | amber |
| `screen-wide` | `surface = "screen"`: anywhere | clipped to its frame | amber |
| `keys-arrows` | `input.grab` of keys that are not text: arrows, `home`, `end`, `pageup`, `pagedown`, `insert`, `delete`, `f1`..`f12` | `denied` | amber |
| `keys-grab` | any other `input.grab`; `widget.show {modal: true}` | `denied` | red |
| `keys-read` | topic `input`: every key | subscribed, no event | red |
| `pointer` | topic `hover` elsewhere than its own things: moves anywhere | no event there | plain |
| `clicks` | topic `mouse` elsewhere than its own things: clicks anywhere | no event there | red |
| `clicks-grab` | a click it is told elsewhere, or told to its effect (`bomb`), taken from the program | the program has it too | red |
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
| `frame.list`, `frame.get`, `frame.at` | less than `layout` | its surface's frame alone, told as frame 0 at 0, 0; `frame` of every request ignored |
| a frame's cells: `origin`, `frame.at`, a match's `x`, `y` (a pending wait's: at its answer), `pos` of `mouse`, `hover`; an anchor `at` in frame cells | less than `shape` | told unscrolled: `origin` 0, 0, each cell one of the view; the anchor put in view cells (a negative coordinate: off the view, in frame cells) |
| `x`, `y`, `frame`, `pos` of `mouse`, `hover` | less than `layout` | less the corner of its surface's view; `frame` 0; `pos` the same as `x`, `y` |
| its widgets, `surface = "widget"` | less than `layout` | in their own cells, wherever placed: a widget info's `rect` at 0, 0; `x`, `y`, `pos` of a report on it less its corner (negative off it); not in `draw.list` |
| `session.info` | less than `text` | `protocol`, `server`, `size`, `headless`; `host_colors` from `shape` |

## Methods

`needs`: the permission; `—`: the default profile.

| method | params | result | needs |
|---|---|---|---|
| `auth` | `token` | done by the host; never call it | — |
| `batch` | `calls`: `[{method, params}]` | `{"results":[{"result"} or {"error"}]}`, one repaint | each call its own |
| `subscribe` | `topics`: `[name]`, `snapshot` (bool: the state now, as events) | `{}`; adds to the topics held, never removes one; an unknown topic: `bad-params`, none added | each topic its own |
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
| `draw.list` | | `{"sprites":[...]}`: its own; where an anchor landed (`screen`, `visible`, `covered`) told at the degree that finds it (`text`: `words` for a listed word, else `text`; `cursor`, `auto`: `shape`), else `screen` null, `visible` false; `frame` 0 below `layout` | — |
| `draw.ext` | `name`, `function`, `params`, sprite members; `id`: redraw that one | `{"id":n}` | — |
| `draw.exts` | | `{"exts":[{"name","functions"}],"effects":[name]}` | — |
| `widget.declare` | `widgets`: `[{tag, name, size:[w,h], content}]`, `border` | `{"tags":[...]}` | — |
| `widget.show` | `tag`, `frame`, `prefer`, `fill`, `margin`, `title`, `border`, `layer`, `modal` | `{"widget":info}` | —; `modal`: `keys-grab`; in front: `front-widget` |
| `widget.set` | `id`, `variant`, `content`, `title` | `{}` | — |
| `widget.hide` | `id` | `{}` | — |
| `widget.ask` | `id`, `quiet`, `escape`, `close`: the user may focus it | `{"widget":info}`; keys come as topic `input` | — |
| `widget.release` | `id` | `{}` | — |
| `widget.list` | | `{"widgets":[info]}`: its own; `frame` 0 below `layout` | — |
| `widget.borders` | | `{"borders":[name]}` | — |
| `app.list` | | its own entry | — |
| `app.result` | `value` (any JSON, 64 KiB), `status` (0..255, default 0) | `{}`; the last holds; told in `app.list` as `"result": {"value","status"}`; `ptyrc app ID --wait` prints it and exits with the status | — |
| `clock.now` | | `{"unix_ms","zone","offset_s","abbr","next"}`: the machine's zone; `next`: when the offset changes, ms, or null | `clock` |
| `clock.zone` | `zone` (an IANA name, or `"local"`), `at`: 1..64 instants, ms (absent: now) | `{"zone","offsets":[{"at","offset_s","abbr","next"}]}`; an unknown zone: `bad-params` | `clock` |
| `storage.get` | `key` | `{"value": str \| null}` | `storage` |
| `storage.set` | `key`, `value` (a string, 256 KiB) | `{"used","quota"}`; past 64 keys or 1 MiB: `not-usable`, nothing written; 16 a second | `storage` |
| `storage.remove` | `key` | `{}` | `storage` |
| `storage.list` | | `{"keys":[{"key","size"}],"used","quota"}` | `storage` |
| `asset.list` | | `{"assets":[{"name","size"}]}`: the files of `[app] assets`, names relative to that directory (`ship.pic`, `sub/a.pic`) | — |
| `asset.read` | `name`, `as` (`"text"`, `"base64"`), `offset`, `len` (1 MiB at most) | `{"text" \| "base64", "size", "eof"}`; none of that name: `no-file` | — |
| `file.stat` | `file` (a handle), `name` (a file of a `dir`) | `{"exists", "size", "mtime_ms"}`; absent: `{"exists": false}` | — |
| `file.read` | `file`, `name`, `as` (`"text"`, `"base64"`), `offset`, `len` (1 MiB at most) | `{"text" \| "base64", "size", "mtime_ms", "eof"}`; a file of 16 MiB at most | — |
| `file.list` | `file` (a `dir`) | `{"entries":[{"name","size"}]}`: its regular files named as a `key`, by name, 1024 at most; the others left out, unsaid | — |
| `file.write` | `file`, `text` or `base64` (the whole, 1 MiB), `if_mtime_ms` | `{"size","mtime_ms"}`; another mtime: `not-usable`; 16 a second with `storage.set` | `files-write`, and `write = true` |
| `probe.git` | `repo` (a `dir` handle; absent: the program's directory), `parts`, `log` (1..200), `since` (a stamp) | `{"repo": {...} \| null, "stamp"}`; `since` the stamp now: `{"unchanged": true, "stamp"}`; a real read a second | `probe-git` |
| `probe.system` | `parts` (`glance`, `logins`, `listeners`, `disks`), `top` (0..16) | `{"glance", "logins", "listeners", "disks"}`; shares between two samples, the first zeros | `probe-system` |

Sprite (`draw.put`; a `Canvas` makes one):
`{"frame", "anchor", "content": {"rows": [str], "style": {...}},
"frames": [{"rows", "ms"}], "loop", "layer", "alpha": 0..1, "z",
"ttl_ms", "persist", "motion": {"vx","vy","ax","ay","bounce"},
"effect": {"name", "params"}, "size": [w, h]}`. `anchor`: `{"at": [x,
y], "space": "view" | "frame"}` (`view`: a negative `x` puts the
sprite's right edge `-x - 1` columns from the view's last, `y`
likewise from the bottom), `{"text": "word", "side": "right" |
"left" | "above" | "below" | "over"}`, `{"auto": {"prefer":
"top-right"}}`, `{"cursor": {"dx": 1, "dy": 0}}`. `layer`: absent or
negative, behind the text; in front only with `front` (else behind).
Effects: `draw.exts`. `params` of an effect, the driver's, of
`draw.put` and `draw.update`: `"pointer": true`, the driver tunes
`hover` (the canvas cell under the host pointer, or null); `"bomb":
true`, on the release of a press on its canvas, `bomb` (`[x, y, ms]`,
`[x, y, ms, dx, dy]` released elsewhere). The app is told nothing. The
press is taken from the program with `clicks-grab`. Limits of the driver: 256 sprites, 16 widgets,
32 subscriptions; past one, `not-usable`.

Host functions (`clock.*`, `storage.*`, `app.result`, `file.*`,
`probe.*`):

- A `key`, a `name` of `file.*`: 1 to 64 bytes of `[A-Za-z0-9._-]`,
  not starting with `.`; else `bad-params`. The values are the app's origin's: kept by an
  update, removed by `ptyrc store rm --purge`.
- `clock.*` held, `storage.*`, `file.*`, `probe.*` held read the
  disk or run `git`: not in a `batch` (`bad-request`).
- A file is a handle, never a path: `init(args)` gets `{"file": "f0",
  "name": "TODO.md"}` for an option of type `file` or `dir`. A handle
  not given: `no-file`. `name`: as a `key`. A link in place of the
  file, or of a file of the directory, is not followed: absent. A
  directory moved since it was named: `not-usable`.
- A tick on the second: at each tick, `clock.now`, then
  `period(until(unix_ms, 1000))`: `1000 - unix_ms % 1000`, plus 1000
  when under 33 (a `period` under 33 is 33). Asked each tick, it does
  not drift. `until`: `ptyrc.d.ts`, `clock::until`. At `next`, the
  offset changes.

## Topics

`event(topic, data)`, after `subscribe`. Not held: subscribed, no event.

| topic | data | needs |
|---|---|---|
| `size` | `{"size":[c,r]}` | — |
| `draw` | `{"removed":[id],"frame","rows":[from,to]\|null}`: its sprites removed by the driver | — |
| `widgets` | widget info of its own, when it changes; `frame` 0 below `layout` | — |
| `move` | `{"frame":id}`: the user moved the app, or its frame left the screen and a pane took its place (its sprites moved along; `size` follows); 0 below `layout`; not told to `surface = "widget"` below `layout` | — |
| `exit` | `{"status"}`: the program ended | — |
| `frames` | frame infos | `layout` |
| `screen` | `{"rows":[y]}`: screen rows changed | `shape` |
| `text` | `{"frame","rows":[y],"origin","rect"}`: rows whose text changed | `shape` |
| `cursor` | `screen.cursor` without the trail | `shape` |
| `idle` | `{"frame","idle","score","free"}`, when `idle` changes | `shape` |
| `colors` | host colours | `shape` |
| `input` | `{"key","bytes","grabbed":true}` (a grab), `{"key","bytes","widget"}` (a focused widget), `{"released":"escape"}`; every key | `keys-read` for every key; a grab's and a focused widget's come unsubscribed |
| `mouse` | `{"sprite","button","action","mods","at","x","y","frame","pos"}` (below) | — on its own widget or sprite in front; elsewhere `clicks` |
| `hover` | `{"sprite","at","x","y","frame","pos","mods"}` (below); `sprite` null: left | — on its own widget or sprite in front; elsewhere `pointer` |

- `mouse`, `hover`: `x`, `y` the screen cell; `at` `[x, y]` the cell of
  the sprite under it (a widget's sprite is its outer box: the border
  is row and column 0; the inner box starts at `rect` plus the border's
  width), null when none; `frame`, `pos` the frame there and the cell
  in it; `mods` of `shift`, `alt`, `ctrl`. `action`: `press`,
  `release`, `drag` (moved, a button down), `move` (moved, none down),
  `wheel`. `button`: `left`, `middle`, `right`, `wheel-up`,
  `wheel-down`, `wheel-left`, `wheel-right`, `none`. The release and
  the drags of a press go to the sprite pressed, `at` null when off it.
  A deck turns them into `click(x, y, button, widget)` and `hover(x,
  y, widget)` on the inner box (`ptyrc.d.ts`). Its own sprite behind
  the text (a canvas without `layer`) is not its own thing here:
  `hover` on it needs `pointer`, `mouse` needs `clicks`. `sprite` of a canvas:
  `canvas.sprite()` (Rust: `Canvas::sprite`), `null` before its first
  `present`; a new id when the driver removed it and `present` put it
  again.
- `screen.*` answers the wrapped program's cells. With `text`,
  `screen.text`, `screen.cells {sprites: true}` compose its own
  sprites alone over them: an app never reads another's.
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
  or the files of `--storage DIR` (a file a key: its name the key, its
  content the value; read once); `--storage-out DIR` writes the storage
  at the end into DIR, as `--storage` reads it; `app.result`;
  `file.*` on a file option given (`--NAME PATH`, or `--file
  NAME=PATH`; `ARG#N` names the `N`th value of `[args]`, from 0:
  `[args] todo = { type = "file", min = 1, max = 1, doc = "..." }`,
  `ptyrc dev test . --file ARG#0=todo.md`, as `ptyrc app ID
  todo.md`): its
  handle in `init`, writes in memory, only with `write = true` and
  `files-write`; `probe.*` from `--probe git=FILE.json` (written by
  `ptyrc dev probe git [DIR]`, `system=` by `ptyrc dev probe system`:
  the driver's probe now; none: the not-held answer; `probe.git` one read a second, as the driver); `asset.*` from `[app] assets`. The report:
  `result:` (and the status), `storage:` (a line a key the app wrote,
  `KEY = "VALUE"` or `KEY removed`; a key given and not written, `KEY =
  "VALUE" (given, not written)`; the value cut past 60 characters;
  `storage` not held: one line, the storage given not used), `files:`
  (written); `--json` their values whole.
- The end of `--for`: `stop`, as at SIGTERM, after the last screen;
  what it writes is in the report.
- A second run from the first one's storage: `ptyrc dev test .
  --storage-out tests/stored` once, the directory kept in the tree;
  the case `--storage tests/stored`. Not one case writing what another
  reads: the cases run at once.
- `--timing`: the wall time of the calls, host calls in them (what the
  50 ms bound of `tick-ms` measures: a test run is never `late` nor
  paused, whatever the load). A line for each `tick` or `event` over
  the bound, `slow at T ms: tick, X ms` or `slow at T ms: event TOPIC,
  X ms`, and status 1; a last line, the ticks run, the mean and the
  max of one, the events and their max. Not in the report otherwise,
  which a case compares whole; `--json` has `ticks`, `tick_ms`,
  `events`, `event_ms`, `slow` always. Run it alone, on a quiet
  machine, with the release runner.
- The permissions are those the manifest declares for the run (a
  permission tied to an option by `needs`, or `files`, `files-write`
  to a file option: only when that option asks it; `[limits]`:
  `limit-memory`, `limit-time`); `--deny a,b` answers them no. A
  `word` or `pattern` option given is held as the driver holds it.
  The launcher's options (`--front`, `--back`, `--prefer`, `--alpha`,
  `--border`) apply as under `ptyrc app`; one pane: `--frame` and
  `[app] frame` change nothing. The
  report names each request `refused`, each answer `lowered` and the
  permission it lacked; one not declared is fine if intended: an app
  that declares less is asked less.
- Drawing as the driver draws: in front only with `front` (a widget:
  or `front-widget`); else behind the text. `draw.list` tells where an
  anchor landed at the degree that finds it; `screen.text`,
  `screen.cells {sprites: true}` compose the app's own sprites.
- Widgets laid out as the driver lays them out: on cells without a
  change for 1 s (none before 1000 ms); never on the row of a visible
  cursor, and a row kept from it, as from text (2 columns, 1 row);
  the outer rectangle a cell from the edges of the view (`margin`). A
  blank screen's cursor at the top left: a widget at the top right sits
  on row 2. A larger variant that fits is taken after 2 s; focused, the
  widget is laid out again, over the text when no room holds it. A
  widget hidden at a screen shown: a `note:` line names it, its
  smallest variant and the screen's size.
- Events: `size` on a tape's resize or a `--resize MS:CxR`; `screen` and `text` when the
  screen changes (a tape, a `--screen T:FILE`); `idle` as the driver
  tells it (a frame's idleness changed; a screen nothing was written
  on is idle from the start); `input` for a key given by `--key
  MS:NAME`: grabbed (`ctrl+t` releases), else on its way to the
  program, `{"bytes"}` with `keys-read` (a `--paste` too, bracketed);
  any topic by
  `--event MS:TOPIC:JSON`, delivered when subscribed and held;
  `hover` (`--pointer MS:X,Y`) and `mouse` (`--press MS:X,Y`,
  `--release MS:X,Y`; `--click MS:X,Y` both at once) on the app's
  sprite under the cell, heard as the driver hears them, a release of
  the sprite pressed; the same moves and clicks told to an effect put
  with `pointer`, `bomb` as the driver tells them; `--focus MS:TAG` gives the keys to its widget that asks,
  then `--key` goes to it (its escape and close keys too) and
  `--paste MS:TEXT`; `widgets` when a widget is laid out.
- The screen: `--screen [T:]FILE`, again for each change (text: the
  screen cleared at T, the text written from the top left; escapes
  read as a terminal would; a line wider than the screen wraps, rows
  past the last scroll, as in a terminal, and the report says so),
  `--tape FILE` (a recording, played in session time). `--seed N`:
  `random` (and `Math.random`), as `ptyrc app ID --seed N` draws it live;
  after `--` the launcher's, the same (both: refused). `--styles`: under each screen, a grid of its cells not of
  the default style, a letter each, and the legend; `--json`: per
  screen `styles`, `[y, x, w, "fg=11 bold"]`. Without `--styles`, a
  `note:` when a screen holds `inverse`, `hidden` or `underline`
  cells, which the text does not show.
- A flag of `dev test` is its own before `--`; an option of the app of
  the same name (`--zone`, `--size`, ...) goes after `--`. The
  flag given, a note on stderr says dev test took it. A flag of one
  value given twice: refused, status 2, nothing on stdout.

## Manifest (`ptyrc-app.toml`)

```toml
[app]
id = "my-app"              # a-z, 0-9, -; a letter first; 64 at most
version = "0.1.0"
summary = "One line, 200 at most."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"          # or rust = "src/lib.rs", rustc = "1.88"
surface = "frame"          # "widget" (default), "frame", "screen": below
# frame = "largest"       # without --frame: active (default), largest, quietest, idle, raw
# assets = "assets/"; tape = "demo.tape"
# wait = true            # `ptyrc app ID` waits for its result (app.result)
# random = [[], ["--grab"]]  # the draws of the menu's random app: words of
#                            # [options], each checked by dev check; []: never;
#                            # absent: [[]], bare, never drawn if bare is refused

[options]                  # --NAME VALUE, --NAME=VALUE, -S VALUE; a bool --NAME, --no-NAME
size = { type = "int", default = 20, min = 1, max = 99, doc = "one line" }
grab = { type = "bool", default = false, short = "g", needs = ["keys-arrows"], doc = "..." }
season = { type = "string", default = "auto", choices = ["auto", "spring"], doc = "..." }
word = { type = "word", doc = "..." }                   # the user's word
mark = { type = "pattern", list = true, doc = "..." }    # repeated; regex = true
stop = { type = "file", doc = "..." }                    # or "dir"; write = true
# types: int, float, bool, string, word, pattern, file, dir
# reserved: alpha, back, border, frame, front, help, no-wait, prefer, seed, wait; no-NAME

[args]                     # one list of positionals, init(args).NAME
patterns = { type = "pattern", min = 1, max = 16, doc = "..." }  # min, max: how many

[permissions]              # NAME = "why, one line, 200 at most"
shape = "to stand on the text"
words = { list = ["food", "fish"], why = "to find them" }  # 1..32 words, 3..32 letters or digits

[limits]                   # above the default, up to the ceiling (Limits)
memory-mib = 128
```

- `init(args)`: every option, its default when not given (`null`
  when it has none; `[]` for a list), the positionals under their name.
- `default`: optional; none: a `bool` is false, any other `null`. None
  of a list. `min`, `max`, `choices` rule a value given; `--help` and
  the menu show no default when there is none.
- `min`, `max`: of an `int` or `float` in `[options]`, numbers of its
  type, inclusive, `min <= max`, the default within; each value (each
  of a list) outside is refused at the start, naming the option and
  the bound; `--help` and the menu show them. In `[args]`, `min`,
  `max` count the positionals (0 to 16), never bound a value. Of a `file` or `dir`: a plain name of the working directory,
  used when none is named, asked as `files`.
- `needs`: permissions asked only when the option is given (a bool:
  true); each in `[permissions]`. The pop-up then shows them alone.
- `word`: 3 to 32 letters or digits, else refused at the start. Found
  by `screen.find` and `screen.wait` whatever the degree, shown in
  `screen.shape` from `shape`; not counted in the 32 of `words`.
- `pattern`: 1 to 256 bytes, 16 a run; with `regex`, a regular
  expression `screen.find` compiles; else refused at the start.
  `screen.find`, `screen.wait` with exactly that pattern (and its
  `regex`) are answered as under `text`, `known` too; any other
  pattern as before.
- `file`, `dir`: require `files` in `[permissions]`; `write = true`
  (a file only) requires `files-write`. The guest gets `{"file":
  "f0", "name": "TODO.md"}`, never a path; reading is granted by the
  naming; a file to write asks `files-write`. Regular files and
  directories only; 16 a run.
- ptyrc's own options, the same for every app and never told to it:
  `--frame F`, `--front`, `--back`, `--prefer SIDE`, `--alpha A` (the
  opacity of its sprites), `--border B` (of its widgets), `--wait`,
  `--no-wait`, `--seed N` (its `random` drawn from `N`, as under `dev test --seed
  N`; absent, from `/dev/urandom`: an app keeps no seed option of its
  own).
- `surface`, where its sprites and widgets are clipped, never refused
  (nothing in the answers tells it): `widget`, its widgets alone,
  placed on the frame it starts on, which they follow (a `frame`
  named by `widget.show` taken with `layout` alone), a bare sprite (a
  canvas) put but never shown; `frame`, the view of the frame it
  starts on, which it follows (`move`); `screen`, anywhere, with
  `screen-wide` declared and held (amber), else as `frame`.
  Headless, one pane: the three draw alike but `widget`.
- `--help` is made from the manifest; `ptyrc dev check` checks it.
  A `summary`, a reason or a `doc` that starts with `TODO`: a note of
  `dev check`, refused by the install.

At `init`, `args["ptyrc:manifest"]`: `{"id", "version", "permissions":
[{"name", "words"?}], "granted": [name], "limits": {KEY: N}, "frame"}`,
the limits granted alone; `frame`: the id of the frame its surface is
on at the start (that of `--frame`, else `[app] frame`, else `active`;
`move` names the next). Live: that id with a degree held (`layout`
or above); below `layout` 0, and the `frame` of every request is
ignored: reads and draws are of the frame its draws are put on (raw
for `widget`, `screen` with `screen-wide`), told as frame 0 at the
top-left of the screen (`frame.get`: `id` 0, `rect` at 0, 0; a
match's `screen` from there). A widget of `widget` is placed on the
frame it starts on all the same, and told in its own cells (Cut
answers): nothing it reads tells that frame but the size of its
widget, which the room there chose.
Headless: 0, the one frame, raw; a live id is seen only under `ptyrc
dev run`. A `Sight` reads it by default (Rust: `manifest()`).

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
