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

- Alpha. A back `bg` under a panel of the app is hidden; a blend of the
  two needs RGB on both sides, and the default background of the host is
  not known to the driver (OSC 11 would tell). `alpha` per sprite, on
  RGB colours only, once that is.
- Named layers beyond `back` and `front` (`"hud"`, `"wallpaper"`): only
  integers today.
- A panel as a surface: a back sprite clipped to a panel of the app, or
  placed on it by name of colour.
- A drawing next to a word (`--near`) is hidden where the app has text,
  as any back sprite. A hint in `draw.list` (`covered`: share of its
  cells hidden by ink) would help a client choose `front`.
- `Grid::put` gives the spacer of a wide glyph the style of its head;
  the scene relies on it. The front layers do not check that a wide
  sprite glyph lands on one backdrop.
- An effect made by a draw extension cannot be redrawn in place
  (`draw.ext` with `id` replaces content and frames only).

## Heat and idle, second session

Built: see "Heatmap" in `design.md`. Left:

- `screen.wait {idle: true}` and an `idle` event topic, so a client does
  not poll `frame.idle`.
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

- Confetti party mode (`--spawn`) likely flickers less with the confetti
  and sparkles behind the text: use the back layer.

## Client apps, second session

Gaps met while writing the apps; none fixed in the core.

- An effect cannot see the screen: `Effect::render` gets a canvas and a
  time. The sand takes the pane's text as a `walls` parameter and is
  started again when the text changes. An effect given the app cells
  under its canvas would let it react by itself.
- vim splits inside a tmux window are one pane. Reproduce:
  `scripts/demo-frames.sh`, window 1 (`vim -u NONE -N -O a b` in a full
  tmux window, 120x36). Expected: two panes, key `tmux:1:...`. Observed:
  one pane `120x35+0+0`, no key, no `vim` support.
- `less`, page down (space): the view lands on an island (origin
  `0,100000`), `b` brings it back; line scrolls (`j`, `k`) are tracked.
  Frame-anchored sprites stay behind on a page down. `fishes` clamps to
  the new view, so the fishes jump.
- A text-anchored sprite is centred on the word's row and clipped by the
  view: a 7-row flower next to a word on the first rows loses its top.
  An anchor that slides to stay whole would help.
- `Frame.refresh` lists every frame to update one: use `frame.get`.
- No event for idle or for a frame's text change: `sand` polls twice a
  second.
- `quietest` and `idle` name a pane at the time the sprite is added; an
  app that waits for idle has to ask again (`sand` does).
- Party mode behind busy text (120x40, two cells of three inked, one
  burst of 64 over 3 s): 1493 host cells changed between 20 ms samples
  on the back layer against 3681 in front, and none of them a text cell.
