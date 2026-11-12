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
