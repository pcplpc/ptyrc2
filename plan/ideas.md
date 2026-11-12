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

## Real TUIs, second session

Found with `scripts/e2e-tuis.sh` (120x36); `KNOWN` lines there point here.
Fixed in the segmentation: a row is a bar only when one backdrop covers
it; a highlight that moves without its text is a list entry; a panel of
another backdrop is a pane; the agent preset knows the `┃ ... ╹▀▀▀` box.

Open, in the frames (`frames/mod.rs`, `frames/ext.rs`):

- htop, glances: panes are re-allocated at each refresh until the monitor
  rule holds. Keys: none, wait. Expected: pane ids fixed from the first
  layout, usable after `settle_ms`. Observed: htop ids 3,6 -> 7,8 -> 9,10
  (usable at 5.6 s instead of 1.8 s); glances a new id for the stats pane
  and for the table every 2 s for 7 s, again at 11.7 s. A sprite on such
  a pane is hidden at each change. The chrome around changes too (uptime,
  clock), so the "jump when the chrome is unchanged" rule never applies.
- htop: `down down pagedown`, wait 6 s. Expected: the table pane keeps
  rows 11..35. Observed: it shrinks, once to 11..34 (the key bar, right)
  and in one run to 20..34: rows of the re-sorted table that stayed are
  learned as chrome.
- htop: the selected row is a bar until it first moves (rows 10 and 11
  are chrome, the table starts at 12). A list highlight could be told at
  once from a bar by its place: not at an edge, not next to a rule.
- opencode: `ctrl+p` (palette, a centred panel). Expected: the pane under
  it keeps its origin; the panel is an overlay. Observed: the pane jumps
  to an island (origin 0,100000). The cells under a nested pane should
  not vote in the parent's scroll detection.
- opencode: the session view (transcript, sidebar) was not seen: it
  needs a prompt submitted to a model. The preset is checked on the home
  screen only. Messages of a transcript are panels too: expect candidate
  panes that come and go while it scrolls.
- ranger: one pane. The three columns have no border and no backdrop;
  only a preset can tell them (`plugins/tui-support/ranger`): column
  widths from the `column_ratios` look of the status rows.
- less, man: open and segment as one pane; the prompt row is not chrome.
  Paging is in `scripts/e2e-frames.sh`.

Other:

- `draw ext shapes circle '{"r":"x"}'` is accepted: a draw extension
  ignores a parameter of the wrong type.
- `Pilot.frame()` raises its own `no-frame` without the driver's hint.
- The method list for "did you mean" lives in `ptyrc-server/src/hints.rs`
  and the frame names too: add `frame.idle`, `idle` and new names there.
