# Widget ideas

Brainstorming, 2026-11-13. Not a plan, nothing here is frozen: the
verdicts are the owner's, the "how" under each is a first guess to be
argued with. `design.md` stays the contract.

## Verdicts

| idea | kind | verdict |
|---|---|---|
| `window` | decorative | yes; liked for the diversity and the cycles |
| `train` | decorative | yes, promising |
| `gallery` | decorative | yes; wants a diversity of art |
| `fire` | decorative | yes |
| `fishes --tank` | decorative | yes, as an option of `fishes`, not a new app |
| `orrery` | decorative | no: demo seen (`temp/orrery_demo.py`), disliked |
| `bonsai`, nature that grows | decorative | later |
| `tape` | decorative | later: no MPRIS here |
| `life` | decorative | no: done already (`life.py`) |
| `lava` | decorative | no |
| `git` | productive | yes, a full monitor |
| `timer` | productive | yes |
| `note` | productive | yes |
| `pace` | productive | yes, with a leaderboard of favourite words |
| `minimap` | productive | yes, as ideas: start simple, get fancier; no work yet |
| `agent` | productive | noted; no implementation, no integration for now |
| `agenda` | productive | noted; skipped: no agenda to read |
| `ci` | productive | no: no CI to test against |
| `globe` | decorative | yes: sketch seen (`temp/globe_demo.py`), liked |
| `watch` | productive | no: examples seen (`temp/watch_demo.py`), useless |
| `tests` | productive | no: good integrations are too much work |

## Common rules

- Scenes are ambient: they do not react to typing, heat or idle.
- The one reaction wanted: the host mouse hovering over the scene.
- Everything is a widget: variants by size, the driver places them,
  `render(w, h)` pure, time an argument (the session clock), so a
  day or a year can be played fast in tests.

### Hover: how could it be done?

- The `mouse` topic reports the host mouse on a sprite, with `action:
  move` and `at`, the cell in the sprite. The widget's info names its
  sprite (`WidgetInfo.sprite`): a client maps the event to its widget.
- Checked: the driver asks the host for 1002 and 1006 only
  (`mouse.rs`, `HOST_ON`): moves come while a button is held. The
  parser already names a `move`; a hover needs the driver to ask 1003
  while a client wants it (a `subscribe` option, or a topic `hover`),
  and to drop the moves the app did not ask for, as it does drags.
  Until then a hover is a press and drag.
- The owner agrees to the change, if it costs little and takes
  nothing from the app under it. To weigh: 1003 makes the host send a
  report a cell crossed, all read and parsed by the driver; a move
  the app did not ask for must never reach it; the app's own modes
  come back when the last listener goes; selecting text with the
  host mouse is already lost while `mouse` is on, and stays so.
  Asked only while a client wants hovers, never by default.
- A helper in `_widgets.py`: `Deck.on_hover(tag, handler(x, y))`, the
  cell given in the inner box, and a `leave` after a timeout or a
  move on another sprite.

### Checked in the code, 2026-11-13

- Hover: not there yet, a small change of `ptyrc-server` (above),
  agreed in principle.
- Clip: a sprite is clipped to the view of its frame, or to a
  `panel`; never to the box of a widget. A scene is either all in the
  widget content (a `widget.set` a frame), or sprites kept inside the
  `rect` by the client. A clip rect on a sprite, or sprites parented
  to a widget, would let the driver play `train` alone.
- Keys: a client subscribed to `input` gets every burst of the host
  keyboard, `{"bytes": base64}`, grabbed or not (`driver.rs`). `pace`
  can count keys.
- Echo: the driver does not read the termios of the pty, so keys
  cannot be told from a password. `pace` reads its words from the
  screen instead; no change of the driver needed.
- Repository of `git`: `session.info` gives the `pid` of the child;
  `/proc/<pid>/cwd` is where it started or moved. A shell's children
  are not followed.
- Demos in `temp/` (stdout, no driver): `orrery_demo.py` (refused),
  `minimap_demo.py`, `globe_demo.py`, `watch_demo.py`; `--play`
  animates the first three.

### Shared pieces: how could they be done?

- `_sky.py`: `(time of day, day of year, latitude)` to a palette of
  the 256, the place of the sun and of the moon, the moon phase, the
  season. Pure. Used by `window`, `train`, `fire` (the light of the
  room).
- `_strip.py`: terrain strips from a seed (hills, trees, poles,
  houses), tileable or endless, and a compositor of layers with a
  depth each. Used by `window` (still) and `train` (scrolling).
- A native parallax, the owner's idea; the implementing agent
  decides. A draw extension (`plugins/draw-ext/`) with a small sprite
  system of its own: a scene is a box, layers of strips with a depth
  and a speed each, sprites inside it, all clipped to the box and
  played by the driver. A widget names a scene as its content; the
  client sends the scene once and then only what changes (the light,
  a station). `train`, `window`, `fishes --tank` and `fire` would
  stand on it. Against it: more Rust, and a wire form to design. The
  other way, a `widget.set` a frame from Python, works today and
  costs traffic. A first scene in Python would tell what the plugin
  must do.
- `_canvas.py`: braille and half-block canvases; `clock.py` and
  `life.py` each have one to take from.

## Decorative

### `window`

A house window on a countryside: sky, hills, a house or two, trees;
day and night, the seasons, small things that pass.

How could it be done?

- A border preset `window` in the `borders` extension: sill, jambs.
  The mullions are content, not border: they depend on the variant.
- Variants from a porthole (7x3) to a bay (40x12); the small ones
  keep the sky and one hill.
- Sky: a vertical gradient from `_sky`, redrawn once a minute. Sun
  and moon on an arc; the real moon phase; dawn and dusk tints.
- Stars, smoke of a chimney, a bird: sprites with a looped animation
  or a `motion`, played by the driver, so the client sleeps between
  two minutes. To check: a sprite clipped to the box of a widget, or
  everything in the content and a redraw a second.
- Seasons from the date: blossom, green, leaves falling, snow on the
  hills and bare trees. Weather drawn at random by the day (seeded by
  the date: the same all day): clear, clouds, rain, fog, snow.
- Diversity: the landscape is generated from a seed (`--seed`, by
  default one a day or one a session); `--place` picks a kind:
  countryside, sea, mountain, city roofs.
- `--at HH:MM`, `--season`, `--speed N` for tests and GIFs.
- Hover: the curtain moves aside, or a cat on the sill turns its
  head; near a house, its light comes on.

### `train`

The landscape seen from a train car, in layers of different depth.

How could it be done?

- `_strip` layers, far to near: mountains (a column every 2 s), hills
  (0.5 s), wires and poles, trees and houses (fast), now and then a
  blur at the glass. The sky of `_sky` behind.
- The wires sag between two poles: the cue that sells the speed.
- Scrolling: either a redraw of the content at 10 to 20 a second, or
  one wide sprite a layer with a looped `motion`, clipped by the
  widget. The second costs nothing to the client; to check that a
  clip exists. A widget content patched 20 times a second is the
  cost noted for `chase` (`ideas.md`, seventh session).
- Events drawn from the seed: a tunnel (dark, a faint reflection of
  the car), a bridge, a station where it slows and stops a while, a
  train the other way, a town lit at night.
- A border preset `car`: rounded corners, a rail under the glass.
- Diversity: `--line` picks a kind: plain, coast, alps, desert, night
  city.
- Hover: a breath of fog on the glass around the pointer, wiped
  after a few seconds.

### `gallery`

Framed pictures, some of which move a little, hung with the
`painting` border.

How could it be done?

- A picture is a file: frames separated by a marker line, a colour
  map (a character to a colour of the 256), a period, a name, an
  author. A few sizes of one picture make the variants.
- Diversity is the point: a collection shipped in
  `ptyrc_apps/gallery/`, in kinds: portraits, landscapes, still
  lives, animals, abstract, silly. A picture hangs for a while
  (`--every 10m`), the next drawn at random; `--kind`, `--file`,
  `--dir` for the owner's own.
- Movement: two to six frames, slow: a blink, a swell, a fly on the
  fruit, a candle. Looped by the driver when the widget sprite takes
  an animation; else a `Deck` period.
- Procedural pictures next to the drawn ones: a seascape or a night
  sky from `_strip` and `_sky`, never twice the same.
- The title of the widget is the label of the picture.
- Hover: the eyes of a portrait follow the pointer; the label shows
  the author; a click hangs the next.
- Choosing the collection, the owner's way: the implementing agent
  draws many, 50 to 100, of varied kinds, shapes and sizes; a small
  script in `temp/` shows two at a time and the owner picks one, a
  quick duel after the other (Elo, or a merge sort by hand), until
  the lot is ranked; the best 20 to 30 are shipped.
- Contributing a picture needs no code: fits the gallery's "open a
  push request".

### `fire`

A hearth, or a candle in the small variants.

How could it be done?

- The classic heat field: a row of random heat at the bottom, each
  cell the mean of those under it less a cooling, mapped to
  ` .:^*x#` or half-blocks and to a ramp of reds and yellows.
  Half-blocks double the rows.
- 8 to 12 frames a second; a few seconds precomputed and looped by
  the driver would let the client sleep.
- Variants: a candle (3x3), a brazier, a hearth with logs and a
  mantel (border preset `hearth`).
- A slow cycle: the fire burns down over an hour to embers, then a
  log is put on. Sparks as sprites that rise and die.
- Hover: a poke; the flames rise and sparks fly for a few seconds.

### `fishes --tank`

The fishes in a widget instead of the whole pane.

How could it be done?

- `fishes.py` draws sprites on a frame; `_reef.py` holds the looks.
  `--tank`: one widget (border preset `tank`: glass, a water line),
  the swimmers confined to its inner box.
- Either the fishes are sprites over the widget, placed in its
  `rect` and moved when the `widgets` topic says it moved; or they
  are composed in the content. The first keeps the driver-played
  motion; the second is simpler and survives a move.
- Variants: a bowl (9x4, one fish) to a tank (40x10, weeds, bubbles,
  a castle).
- Hover: the fishes come to the pointer, as to a finger on the
  glass; a click drops food.

### Later, or dropped

- `bonsai`, nature that grows: later. A tree that takes a branch a
  commit; a seeded L-system, the state kept in a file.
- `tape`: later, needs MPRIS (`playerctl`), which the owner does not
  have.
- `life`: exists.
- `orrery` (the planets round the sun in braille): refused on the
  demo. Not to be proposed again.
- `lava`: dropped.
- `globe` (a snow globe, flakes in a dome, shaken now and then or
  on a hover): approved on the sketch,
  `python temp/globe_demo.py --play`. To build.

## Productive

### `git`

A full monitor of the repository, in the manner of `monitor`.

How could it be done?

- One app, several tags, each a set of variants, `render(data, w, h)`
  pure, `--monitor LIST`, `--merged`, `--print` as `monitor.py` does.
- Tags:
  - `head`: branch, upstream, ahead and behind, the state (rebase,
    merge, bisect, cherry-pick in progress), `main ↑2 ↓0 ●3 +1 ?4`
    in one line.
  - `status`: staged, unstaged, untracked, with the counts of lines.
  - `log`: the last commits as a small graph, age, subject.
  - `stash`, `branches` (the recent ones, age, ahead and behind),
    `worktrees`.
  - `pulse`: commits a day over the last weeks in braille; time
    since the last commit.
- Data: `git status --porcelain=v2 --branch`, `git log --format=...`,
  `git for-each-ref`, by `subprocess`, on a period (5 s) or when
  `.git/HEAD` and `.git/index` change (their mtime: stdlib only).
- Which repository: `--repo`, else the working directory of the app
  wrapped (`/proc/<pid>/cwd` of the session's child, to check what
  `session.info` gives), else the app's own.
- A warning as a border `chase`: a rebase left open, a tree dirty
  for hours, a branch behind its upstream.
- No write to the repository, never.

### `timer`

A countdown, a pomodoro, an hourglass.

How could it be done?

- `timer 25m`, `timer --pomodoro 25/5 --rounds 4`, `timer --until
  17:30`.
- Variants: `24:13` in one line; a bar; block digits (those of
  `clock`); an hourglass whose sand is the physics of `sand.py`, the
  grains let through at the rate that empties it on time.
- Wall time. `--speed N` for tests and GIFs (1: a second is a
  second), as for the scenes.
- The end: the border chases (`alert`), a burst of `confetti`, the
  modal form if `--modal`. A pomodoro turns the glass over.
- Hover: the time left in digits over the hourglass; a click pauses.

### `note`

A sticky note from a file.

How could it be done?

- `note TODO.md`; the file is read again when its mtime changes.
- Variants: `3/7` done in one line; the first open items; the note
  wrapped to the width, cut with `…`.
- Markdown reduced to what a note needs: `- [ ]`, `- [x]` (dimmed,
  struck), headings bold. The title of the widget is the file name.
- A border preset `postit`: a yellow ground, a folded corner.
- Several notes: one widget a file, or `--merged` showing them in
  turn.
- Read only at first. Later: a click on a box ticks it and writes
  the file; that is the first widget that writes, to weigh.

### `pace`

How fast one types, and a leaderboard of favourite words.

How could it be done?

- Two sources. The pace counts keys: the `input` topic (bytes of
  the host keyboard), counted, never kept.
- The words come from what is shown on screen (`text`: the rows that
  changed), the owner's answer to the password question: a password
  is not echoed, so it is never read. The words are those of the
  screen, whoever wrote them: to weigh whether only rows changed
  near the cursor count as "typed".
- Left out, the implementing agent to tune: sequences of high
  entropy, that look random or like configuration: words of 8 to 12
  letters and more that are not words (hashes, keys, base64, mixed
  case and digits, few vowels, no repeated bigram of the language).
- Pace: keys a minute over a sliding window, a sparkline of the last
  half hour in braille, the best of the session.
- Words: cut on non-letters when a separator is typed, lowered,
  backspace honoured. Dropped: words under 4 letters and a stop list
  of common English words (the, and, that, with, this, from...),
  `--stop FILE` for more, `--min N`.
- Leaderboard: the top N by count, a bar each; variants from "top
  word" in one line to a table of ten with the pace over it.
- Counts in memory only by default; `--save FILE` keeps them across
  sessions.
- Hover on a word: its count and when it was last typed.

### `minimap`

The text of a frame, small, with the view marked. Approved as a
direction; nothing to build yet. As the other apps: simple first,
fancier by options.

How could it be done?

- A row of text is a row of dots: a braille cell holds 2x4 dots.
  Ink, not letters: the shape of the code. Seen in
  `temp/minimap_demo.py`: clear at a line a dot row, blurred when
  several lines share one.
- First: what the driver knows. The rows of the frame it has seen
  (`frames`, `text`), the view as a bright band; what was never
  shown stays blank and fills as one scrolls.
- Then the whole file, when it can be guessed. The frame says its
  support (`"support": "vim"`): the `vim` preset of
  `plugins/tui-support/` could tell the file (the status line, the
  title, `argv` of the session, `/proc/<pid>/cwd`), and the app
  reads it from disk. To weigh: a buffer not saved differs from the
  file; splits and tabs; the lines of the view must be matched to
  those of the file (the ruler, `:set number`, or a search of the
  rows seen). Other pagers and editors by their own preset. Program
  names stay in `tui-support`: the app asks "which file, which
  line", it does not parse vim.
- The file name as the title of the widget; the line count, the
  share seen.
- The line of the cursor highlighted (`cursor` topic), apart from
  the band of the view.
- Heat, two maps mixed: "last edited" (rows whose text changed, from
  `text`) and "last visited" (rows that were in view, from
  `frames`), each fading with time; a hue each, the mix where both.
  The driver's own heatmap is of screen cells; this one is of lines
  of the frame, kept by the app.
- Marks of `marks.py` and matches of a search as coloured dots.
- A click on the map scrolls there. The `mouse` event gives the cell
  in the sprite, so the line; the driver or the preset then scrolls
  the app: keys by the preset (`123G` for vim, `123g` for less), or
  the app's own mouse wheel. A drag scrubs. A tool for any frame
  ("scroll this frame to line N") would serve other apps too.
- Hover: the line number and the text of the line in a tip.
- Variants: a thin column (4xN) at the side of the pane, taller
  first; `--prefer right` by default.

### Noted, not done

- `agent`: for the `agent-tui` preset, "waiting for you" when the
  wrapped agent stops on a prompt, the time since its last output,
  an alarm on the border. No implementation and no integration for
  now.
- `agenda`: the next event of an `.ics` file as a countdown. Skipped:
  the owner has no agenda to read.
- `ci`: dropped, nothing to test against.
- `watch -- CMD` (any command in a widget: a light for the exit
  status, the first lines, cut not wrapped): dropped on the
  examples (`temp/watch_demo.py`), judged useless.
- `tests` (a light and the first failing line): dropped, good
  integrations are too much work.

## Order, if any

A guess, not a commitment: `_sky` and `_strip` with `window`, then
`train`; `git` next to them; `timer`, `note`, `fire`, `gallery`,
`fishes --tank`, `pace`. `minimap` after them, by steps. The hover
change of the driver before the first scene that wants it.
