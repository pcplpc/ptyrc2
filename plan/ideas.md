# Ideas

Backlog. Not a commitment; moved to `design.md` when taken.

- Fishes `<*//><` that flee the cursor (cursor trail in the frame). [owner]
- Run over a whole tmux session and track its windows and panes. [owner]
- JavaScript client apps next to the Python ones. [owner]
- Compositor mode behind the same scene, for hosts without a real terminal.
- `ptyrc record` / `ptyrc replay`: a burst log with timings; replay through
  `Vt` for bug reports and regression tests.
- `ptyrc attach`: `show` plus keys forwarded, as `interact` in ptyrc 1.
- Input interception: a client grabs keys (a game played in a pane), with
  an escape key that always releases.
- Mouse: synthesize clicks at a match (`click("OK")`), report host clicks
  on sprites.
- Screenshot verbs: `ptyrc text --ansi > shot.ans`, SVG export.
- Selectors à la playwright: `frame("vim").text("TODO").right(2)`.
- Reactive sprites: hide on hover of the cursor, dodge typed text.
- Scrollbar or percentage indicators as a generic view hint.
- A heatmap persisted per app across sessions.

## From the owner, end of the first session

- Layer sandwich with transparency: background layers behind the app's
  text (the default), layers in front of it, ordered; the app's text is
  one layer among them. Today: `blend` (`over`, `behind`, `tint`) and `z`.
  Built: back by default, named layers around the app, `alpha`.
- Showcase apps: sprite handling and scrollable frames, on vim, less, more.
- Built, then superseded by `--party`: confetti `--spawn --rate N` (per
  minute; default 20).
- Fishtank living in an extended frame: fishes keep frame coordinates and
  scroll with the content.
- A demo of the dynamic frames (tmux panes and windows, vim splits).
- Try: htop, less, man, tmux, ranger, glances, opencode.

## From the first session's work

- `frame.get {frame}` instead of `frame.at {name}`.
- Islands of an extended frame are never collected.

## From the owner, after the first session

- CLI errors must help: `ptyrc run` on a command that is missing or not
  executable should say which, why, and what to try; same care for no
  session, several sessions, a bad frame, a bad key name.
- TUIs clean an area by drawing spaces, a black background or similar,
  and redraw unchanged text identically. These are not updates. The
  frames should tell them apart: compare what a cell shows before and
  after rather than counting writes, and separate what is background
  from what is foreground in a TUI. The heatmap should build on that:
  `activity` from visible change only, `occupancy` from foreground only.

## From the owner, opening the second session (2026-11-12)

Notes, not commitments. They extend "background against foreground" above.

- Pagers and scrolling: better out of the box (`less`, `more`, `man`).
- Tell a TUI's background from its foreground. A write that clears an
  already clean area is not an update; an empty background is not "hot".
- A big grey panel, as opentui apps draw, is not a full area: it is
  background of another colour.
- What does not move is not hot. The heat overlay should tell hot redraws
  (red) from static text (light blue).
- This must fit the layers and transparency work: what a background layer
  shows through is the same notion of background.
- `check_idle()`: a score in `0..=1`, the share of the frame that is
  static (pure green on the heatmap, about 10 s without change).
  `is_idle`: a boolean property, true from 80 %. Use: pick a frame fit for
  a big animation (sand in an empty pane).
- Team: the session agent pilots; forked siblings own disjoint paths and
  may delegate bounded pieces to sub-agents.

## Layers, second session

Built: see `design.md`, "Layers". Left:

- A panel as a surface: a back sprite clipped to a panel of the app, or
  placed on it by name of colour.
- A drawing next to a word (`--near`) is hidden where the app has text,
  as any back sprite; `covered` in `draw.list` tells a client.
- `Grid::put` gives the spacer of a wide glyph the style of its head;
  the scene relies on it.
- An effect made by a draw extension cannot be redrawn in place
  (`draw.ext` with `id` replaces content and frames only).

## Heat and idle, second session

Built: see "Heatmap" in `design.md`. Left:

- The 150 ms grace is a constant; an app that clears and redraws more
  slowly still heats what it redraws.
- A scroll is a visible change of every cell that differs: a pager stays
  mostly idle on regular text, hot on prose. Heat that follows the frame
  rows rather than the view is another model.
- `still` restarts when a pane is resized or attached again; an inactive
  frame reports an idle score of 0.
- `quietest` still orders by mean activity; `idle` is the new name.
- The backdrop of a frame counts every cell, text cells included; at
  most 32 colours are counted (gradients).
- The heat overlay shows panes only, not the raw frame nor chrome.

## From the owner, during the second session

- Built: confetti party mode (`--spawn`, now `--party`) likely flickers
  less with the confetti and sparkles behind the text: use the back layer.

## From the scrolling work, second session

- `more` going back (`b`) feeds the earlier page below: on the main
  screen that is new output on the tape, as the scrollback has it. The
  page gets new frame rows; a drawing fixed on the first copy does not
  show on the second. Same for `cat file` twice. Telling the two apart
  needs more than the screen.
- A percentage (`--More--(20%)`, `50%`) as a view hint: an estimate of
  the row, good to order islands, not to place them.
- Two pager pages in one burst (a key held, a slow host) feed more than a
  screen: taken for a jump, an island, merged when walked into.
- A jump fed in two bursts and caught by a hint: the first burst's rows
  may have been remembered a few rows off; they are rewritten when shown
  again. Forget what was committed since the last agreeing hint.
- `frames.update` at 200x60, after row stamps and lazy heat: 0.5 µs
  when nothing changed, 17 µs for one row (trackers and segmentation),
  500 µs for a full repaint (trackers 270, scan 120, heat 111 per map).
- An empty main-screen frame stays inactive behind a full-screen app
  started at once (`ptyrc run less file`).
- Preset detection on a one-row change waits up to 1 s. A preset whose
  only sign appears on one row and nothing else changes after is seen on
  the next change.
- A shell cleared and printing the same output again (`clear; ls`
  twice) is placed on the rows of the first copy: the view is found in
  the buffer. Right for drawings fixed on the text, wrong as a history.
- `scripts/e2e-frames.sh monitor` (htop) fails about one run in two on a
  loaded machine, before this work too: a pane stays candidate.

## Real TUIs, second session

Found with `scripts/e2e-tuis.sh` (120x36); `KNOWN` lines there point here.
Fixed in the segmentation: a row is a bar only when one backdrop covers
it; a highlight that moves without its text is a list entry; a panel of
another backdrop is a pane; the agent preset knows the `┃ ... ╹▀▀▀` box.

Open, in the frames (`frames/mod.rs`, `frames/ext.rs`):

- htop: the selected row is a bar until it first moves (rows 10 and 11
  are chrome, the table starts at 12). A list highlight could be told at
  once from a bar by its place: not at an edge, not next to a rule.
- opencode: the session view (transcript, sidebar) was not seen: it
  needs a prompt submitted to a model. The preset is checked on the home
  screen only. Messages of a transcript are panels too: expect candidate
  panes that come and go while it scrolls.
- less, man: open and segment as one pane; the prompt row is not chrome.
  Paging is in `scripts/e2e-frames.sh`.

Other:

- The method list for "did you mean" and the frame names live in
  `ptyrc-server/src/hints.rs`: add new ones there.

## Client apps, second session

Gaps met while writing the apps; none fixed in the core.

- Built: an effect sees the screen (`Effect::look`: rows of the view,
  cursor). The sand reads its walls there. Left: text only, no styles;
  rows of the view, not of the canvas, when the two differ.
- `less`, page down (space): the view lands on an island (origin
  `0,100000`), `b` brings it back; line scrolls (`j`, `k`) are tracked.
  Frame-anchored sprites stay behind on a page down. `fishes` clamps to
  the new view, so the fishes jump.
- Party mode behind busy text (120x40, two cells of three inked, one
  burst of 64 over 3 s): 1493 host cells changed between 20 ms samples
  on the back layer against 3681 in front, and none of them a text cell.

## From the owner, late in the second session

- Built: confetti bursts spawn at random places on the screen, cold
  areas of the heatmap first (`confetti.spot`; always, `--anywhere` is
  gone).
- Built: confetti flags. `--bursts N` (at most 10) at random offsets in
  3 s on distinct cold spots; `--party` repeats the round every 3 s;
  `--count` is `--size`; `--seconds`, `--seed`, `--rate` gone.
- Built: no `--seconds` in any app. Tests stop an app with
  `main(argv, seconds=...)`; the demos kill the app processes.
- Built: fishes, 8 by default. One left out of the view by a scroll
  waits 2 to 5 s, then comes back where the text hides most of it, one
  fish a tick at most; the jump after a `less` page down is gone.
- Built: fishes, half of them shy (`--shy N`): each hides behind the
  nearest text, 4 to 10 s, then swims to another spot; fear first.
- Built: fishes `-d`, `-b`, `-r`, `-a`: decorations standing on text,
  bubbles, random species and events; populated areas remembered in
  frame cells (`_reef.py`, see `design.md`). Left: decorations are
  placed on the text of the first visit, a later edit above one leaves
  it behind the new text; nothing stands where there is no text; the
  memory dies with the app.
- Built: snake `-e`, `-x`, `-l`, `-a`: particles, golden apples and a
  shaded body, foes that bite the tail off, beams that burn
  (`_arcade.py`, see `design.md`). Left: a particle is drawn over the
  head for a moment; the snake does not fight back; no score shown.
- Built: brick, a brick breaker that plays alone; `-r`, `-b`, `-m`,
  `-e`, `-a` (`brick.py`, `_bricks.py`, see `design.md`). Left: the
  terrain dies with the app; a racket crossing the first row shows
  through the gaps of the score; on a blank view a ball is juggled
  from edge to edge; capsules sink only down; the two rackets do not
  fight for a ball.
- Built: blocks, falling pieces in braille that play alone; `--sideways`,
  `--count`, `--cursor`, `--chains`, `--effects`, `--score`, `-a`
  (`blocks.py`, `_blocks.py`, see `design.md`). Left: the heap dies
  with the app, its blocks scrolled out of sight are forgotten; pieces cross each other; a piece
  whose way is cut by another is lost; the cursor gives a place, not
  a turn; a gap of one cell between two words is a line.
- Built: sand in braille, one stream, avalanches, strata, kept across
  resizes and text changes, `--cursor` (see `design.md`). No cursor, or
  on column 0: the spout of a sand without `--cursor`. Built in the
  fifth session: the pile scrolls with the text; a heavy stream spreads.

## Events, third session

Built: `screen.wait {idle}`, topics `idle` and `draw`, `visible` and
`covered` in `draw.list`, sliding text anchors (see `design.md`). Left:

- The idle watch is a 250 ms poll in the driver, not a deadline.
- No text-change event; `sand` still looks every 0.5 s for a replaced
  frame (`heir`).
- No CLI flag for `slide: false`. The `sand` extension's parameters are
  unchecked.
- `scripts/demo-frames.sh` `drawn()` reads `screen`; `visible` is the
  field. Most `demo_until` checks in `demo-apps.sh` evaluate once.

## Presets, third session

Built: `ranger`, `htop`; vim splits in tmux (advice was cleared by a
tick while a layout was pending). Left:

- htop menus (sort, setup) are left to the generic segmentation.

## Presets, fifth session

Built: `segment` asked per rectangle of the generic segmentation
(ranger, htop, glances split inside a tmux pane, keys composed); a
`Tuning` per pane (`tuning(&self, pane)`, `PaneView::area`): the ranger
preview is a monitor and keeps its frame; `glances`; a table entry is
not a bar (htop without its preset: the selection is in the table from
the start). See `design.md`, "Dynamic frame" and "Plugins". Left:

- The panes of a preset are final: no preset inside a pane of a preset
  (htop in a vim `:terminal` when vim segments the screen; vim has no
  `segment` today, so it works there).
- A preset is asked of rectangles between rules. An app in a pane cut
  by a bar that is not its own (above a status bar of another app, no
  rule between) is reached only if the cut leaves it whole.
- Presets found on a rectangle are detected again at most once a second
  on a one-row change; `segment` runs at every segmentation while a
  screen preset (tmux) is there.
- Table entries: rows of one shape only (columns). A highlight in a
  list of names of unequal lengths is a bar until it moves, as before.
- htop without its preset: the function bar is not chrome (mixed
  backdrops). `scripts/lib-e2e.sh` cannot pass `--no-presets`: the
  table-entry rule is under unit tests only.
- glances: other layouts (`-1`, `-2`, `-4`, a narrow screen without the
  `TASKS` row) are left to the generic segmentation; the alerts under
  the processes are part of `glances:processes`; a sidebar under 8
  cells is not split.
- ranger: an inverse run inside a text preview is read as a column;
  `draw_borders` not tried.

## Monitors and overlays, third session

Built: monitors are told at the first repaint (shape, no cursor, in
place); rows under an overlay do not vote; `Tuning.monitor`. Left:

- A monitor that shows a cursor and repaints more than 3 s apart
  (`ptyrc run watch -n 5 ls -l`, then `ptyrc frames`) makes an island
  per refresh.
- A pane first seen under an overlay takes the overlay's rows; rows
  under an overlay show their old text in `frame.text`.
- A columnar file in an editor that hides its cursor: a jump painted in
  place would be taken for a repaint.
- htop without its preset: the selected row is chrome until it moves.
- `scripts/e2e-frames.sh pages` (less): one run in eight, extent
  `[0,145]` for `[0,144]`, the `:` prompt row remembered at rows 29, 58,
  87. Not diagnosed.
- `scripts/lib-e2e.sh` cannot pass `--no-presets`: the monitor scenario
  runs with the htop preset on. An `E2E_RUN_ARGS` would test the
  generic path.

## Layers, third session

Built: host colours (OSC 10, 11), `alpha`, `wallpaper` and `hud` (see
`design.md`). Left:

- The named layers are a fixed table; a client cannot declare names.
- An opaque back colour is still hidden under an app panel; only a
  glaze shows there. A back glyph on an inverse app cell does not fade.
- A lone ESC as the last byte of a read is forwarded, not held: a reply
  split exactly there reaches the app.
- No event when the host colours change; no method to set them at run
  time.
- Draw extensions and effects take `alpha` only through the sprite
  fields. Mixes are linear in sRGB.

## Client apps, third session

Built: `cat`, `marks`, `rain` (see `design.md`). Gaps met:

- An effect cannot read the heat: `Under` has rows and cursor only.
  `rain` keeps a stillness per column from text diffs and misses
  changes of style.
- `Under.rows` are strings: a wide glyph or a tab shifts the columns
  after it (`ls /` with a tab: the drops are one cell off). `rain` and
  `sand` index by char.
- No snapshot when subscribing: `cat` asks `frame.idle` and
  `screen.cursor` once at start. `subscribe {snapshot: true}`.
- The `cursor` event is in screen cells, with no frame; `frame` and
  `pos`, as `frame.at` gives, would spare a stale origin.
- `Pilot.events(timeout=)` ends the same on silence and on a closed
  connection; the apps test `pilot.connected`.
- `Pilot.find` and `Sprite.info` do not work inside `batch()`; `marks`
  calls `screen.find` and reads `batch.results` itself.
- `draw.list` has no style: two tints are told apart by size only.
- `auto` placement on a full screen lands on text; `covered` is 0 for
  front sprites.
- `_cli.parse` uses `parse_args`; `marks ERROR -i WARN` needs
  `parse_intermixed_args`.

## Clients, fifth session

Built: `subscribe {snapshot}`; `cursor` in frame cells; topic `text`;
the idle watch on a deadline; `style` in `draw.list`; in Python, reads
inside `batch()`, `Pilot.event`, `gone` raised by `events` on a closed
connection; `_cli.parse` intermixed (see `design.md`). Left:

- `sand.heir` stays, driven by the topic `frames`. Two tmux panes with
  little text resized together kept their frames when tried (100x30 to
  70x30); the case where they do not was not found again, not proven
  gone.
- Topic `text` compares the text of view rows by hash, in the server:
  a change of style is not told; a frame back from inactive is compared
  with what it showed when it left. `snake`, `brick`, `blocks`,
  `fishes`, `flower` still read the text on a period; `rain` and
  `sand` read it in the driver.
- No snapshot for `screen`, `text`, `draw`, `input`, `exit`.
- A hot cell whose `still` passed 10 s makes the idle watch look again
  20 ms later, until the class follows.
- Not started: host colours set at run time and their event; layer
  names declared by a client.
- `Pilot.cursor`, `wait_for`, `wait_idle`, `frame`, `frames` refuse or
  queue blindly inside `batch()`.

## Tools, fifth session

Built: tapes (`run --record`, `replay`, `tape`, `snap`, `--check`),
`attach`, `input.grab`, `input.click`, `shot`, locators (see
`design.md`, "Tools"). From the top of this file: record and replay,
attach, input interception, mouse clicks, screenshots, selectors. Left:

- A tape does not hold the host colours nor `--no-presets`: a replay
  takes the defaults and the presets of its own command line.
- A tape goes forward only: no rewind short of replaying. A `Vt` is
  `Clone`; the frames and the scene are not.
- `replay` is headless: it is watched with `show` or `attach`. No
  export to asciicast.
- Paused, a replay stops every timer: `screen.wait` neither settles nor
  times out; a client that waits must move the tape from another
  connection.
- Typed bytes on a tape (`i`) are not replayed; a tape could drive a
  live app (`ptyrc play-keys`), with the waits its `o` records imply.
- `attach` sends the bytes of this terminal as they are: a terminal in
  normal cursor-key mode facing an app in application mode sends
  `ESC [ A`; most apps take both. No mouse, no paste brackets, no
  resize after the start (`--resize` is read once); a detach key split
  across two reads is not seen.
- `attach --sprites` and `show` poll or redraw whole screens; a
  `host` topic with the composed changes would do both.
- A grab sees the keys the host keyboard sends, split as they come: a
  sequence cut by a read is two keys. Modified arrows (`ESC [ 1;5 C`)
  have no name: taken by a grab of all, never by a set.
- Host mouse reports are not parsed: no click on a sprite, no hover. A
  grab of all gives them raw (`key: null`).
- `input.click`: no drag, no modifiers, no double click; modes 1005 and
  1015 are written as X10.
- `shot`: no cursor, no hyperlinks, no blink; `--svg` trusts the viewer
  for a monospace font (`textLength` keeps the grid). No PNG.
- `Locator.wait` looks again every 20 to 200 ms; `Locator.all` is the
  view only (`known` rows have no screen cell).
- The snapshots of `tests/tapes/` hold the frames to today's answers:
  a deliberate change of the heuristics needs `scripts/record-tapes.sh
  --snap` and a read of the diff.

## Effects, fifth session

Built: `Under` in cells, over the canvas rect, with the heat on
request and the frame position; `Effect::tune`, `draw.update {effect}`,
`draw effect --id`; a glyph opacity for effects (`Canvas::put_faded`);
the sand scrolls with the text and spreads under a heavy stream; rain
on the heat; `snow` (see `design.md`, "Effects"). Left:

- `Scene::tick` (no grid) shows an effect nothing: only the engine
  calls `tick_on`. `Surfaces::text` is gone.
- `Under` is the screen: rows of the frame out of the view are not
  shown to an effect. The sand keeps grains out of sight frozen; it
  does not know the text they rest on changed until it is back.
- A shift (`FrameEvent::Shift`: splice, island merge) is not told to
  an effect: grains and snow on those rows stay a few rows off until
  the text under them eats or drops them.
- Sand: a horizontal scroll loses the grains that leave by a side. The
  attic is per effect, 1024 rows; it dies with the sprite.
- Sand under a heavy stream: grains pass through the heap sideways
  (`reach`), a liquid more than a sand above 1000 a second.
- The heat shown to an effect is the whole view's (`HeatView`, five
  vectors), copied every 200 ms per effect that asks.
- `tune` on `confetti` and `sparkle`: refused, not implemented.
- `draw.list` does not show the parameters of an effect.
- Snow: a flake born on row 0 does not settle there; no snow next to a
  word of its own row, so a ragged right margin holds little; `▁`..`█`
  need a font with block elements.
- `sand.py`, `rain.py`: their docstrings and README rows still describe
  the stillness kept by the rain and a pile that does not scroll.
  `snow.py` repeats the loop of `sand.follow` to send its gusts between
  two events. `Sprite.tune(params)` in the Python pilot
  would spare `update(effect={"name": ..., "params": ...})`.
