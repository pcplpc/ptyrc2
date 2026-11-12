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
- Colour blending on RGB hosts: `tint` with alpha.
- Scrollbar or percentage indicators as a generic view hint.
- A heatmap persisted per app across sessions.

## From the owner, end of the first session

- Layer sandwich with transparency: background layers behind the app's
  text (the default), layers in front of it, ordered; the app's text is
  one layer among them. Today: `blend` (`over`, `behind`, `tint`) and `z`.
  Missing: `behind` as the default, a named layer order where the app text
  has a place, per-cell alpha.
- Showcase apps: sprite handling and scrollable frames, on vim, less, more.
- Confetti `--spawn --rate N` (per minute; default 20).
- Fishtank living in an extended frame: fishes keep frame coordinates and
  scroll with the content.
- A demo of the dynamic frames (tmux panes and windows, vim splits).
- Try: htop, less, man, tmux, ranger, glances, opencode.

## From the first session's work

- `frame.get {frame}` instead of `frame.at {name}`.
- Update a sprite made by a draw extension in place (the big clock
  re-creates its sprite each minute).
- One width function for the emulator and the scene.
- A finite animation added while the raw frame is not yet usable (250 ms
  after the first screen) plays unseen: hold its clock until shown.
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
- A forgotten island takes its lines; sprites fixed on its rows stay in
  the scene, never shown. Emit an event and let the scene drop them.
- `frames.update` at 200x60: 200 µs when every row changed, 80 µs when
  nothing did. `Screen::scan` is 140 µs and 45 µs of these, the two
  heatmaps (raw and pane) 35 µs in both; the trackers 25 µs. Hash rows
  before cells in the scan; update heat lazily, on read or on a tick.
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
