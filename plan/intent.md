# Intent

The owner's brief for ptyrc2, 2026-11-12. Verbatim first, digest after.
`plan/design.md` is the design that grows from it; `plan/interview.md` holds
the decisions taken with the owner since.

## Brief (verbatim)

> I'm the owner of an old project of mine, ptyrc or « pty remote controller »
> that I wrote by hand with love and that'll find in ./temp/ptyrc/ [...]
> The goal of ptyrc2 (the new version of ptyrc I want) is to have like the
> first ptyrc version, a selenium-like ability to "remote control" a pty,
> mostly to 1/ read what's on screen 2/ display shiny animations on the text
> on the screen or mini-helpful tools (an example would be confetti, or
> drawing a flower next to the word "flower", or displaying what time is it
> with a small colorful clock in an area of the screen) 3/ expose all the
> useful tools & handles like a « pty remote controller » would do (in a
> selenium/playwright fashion, think "debug tools for terminals") -- the
> inital goal would be to get a demo working with a new fast rust backend, a
> clear protocol between the pilot & the server, and a new « ptyrc <verb> »
> CLI that's clean and simple for most workflows.
>
> One new thing would be the default support of vim-like / tmux-like displays
> (not specifically vim or tmux, any TUI with scrollable content or panes
> would do). The idea would be natively supporting basic window-based
> workflows (the pilot knows about where it's drawn where) with convenience
> tools on the astraction of a "frame" (raw window frame is the most basic
> one, covering all the screen, and has basics like color-less escape-less
> window for easy user interactions, APIs to resolve the coordinates of
> keywords or grep the screen, etc) and then, extended with new features in a
> "extended frame" that supports scrolling (window only shows a part of the
> frame) where it is expected the app to have a scroll-like behavior (any
> direction) that we probe & detect and remember an extended buffer in all
> direction (for ex for drawing a flower specific X/Y on the frame next to a
> word, and when user scrolls, the word "flower" moves relative to the view,
> but not relative to the frame, the user thus see the flower move with the
> word) this imply a distinct concept of a view & a frame.
>
> Finally, the concept of "dynamic frame" is that it detects lines or static
> parts of the UI (with heuristics, like probing for UTF8 lines that tmux or
> claude-code uses, etc) and automatically segment the screen in different
> panes, each allocated an extended frame. And upon sudden whole-screen
> changes (if not ambiguous with a big scroll) the dynamic frame can choose
> to allocate another pane (so for example if someone has a tmux open, split
> in 4 panes, and 5 times, the ptyrc dynamic frame can recognize that and
> track when user change between panes, tabs, etc).
>
> The overall of all of this is a collection of basic tools that enable a
> remote client to interact with what we see (just like confetti) but
> improved with many heuristics to make easy to draw on virtual surfaces
> (like a long file in vim that we scroll, or a multi-pane multi-tab tmux, or
> opencode or claude-code or other famous TUI, recognizing smoothly when
> things change, reconfigure, new apps appear, disappear, etc) and using all
> these, we finally expose a easy high-level "magic" draw functions, which
> instead of letting the user do work to track TUI states or find a right
> place to do work, they can just say « I want to draw somewhere on the
> screen a 6x18 floating clock, prefer top-right corner » or « draw a 8x8
> flower anchored next to the word "flower" » etc (we'll have likely a
> ./plugins/client-apps/python where we'll put things like confetti, flower,
> etc)
>
> one thing that again I have on the head, is that we don't want flicker or
> bugs, so we need to have the concept of "safe heatmap" (like an alpha mask
> on top of a frame that accumulates which parts are most often free and
> static, and which part are moving, over long periods of time), a pool of
> dynamic frame candidates & panes that wait some time (1-3s) for the screen
> to be stabilized before becoming accessible (so when switching between
> commands, opening a new TUI, rearranging panes, it's easy for the ptyrc to
> stop drawing on parts that changed, and wait for new heuristics to kick in)
>
> i'm available for interviews if you need high-level decisions, I'll trust
> you to do a long session of good incremental work, record in
> plan/intent.md my initial intent and in plan/design.md the design as it
> increases, and do work until the full thing works smoothly & seamlessly,
> with good dynamic examples, one or more of your creation (like a snake
> game that plays alone and eat apples while naviagitng around words in a
> scrollable vim, or a sand simulation in an inactive tmux pane)
>
> last minute thinking of me, likely the core is app-agnostic (no probe of
> vim tmux or claude code) but let's have a ./plugins/tui-support/ where we
> can write per-app improved support (like predefined behaviors for frame
> heuristics or view behavior etc) and ./plugins/draw-ext/ for new custom
> draw functions or features (things like drawing animations can be in the
> native core, so its fast and don't need constant socket or network coms,
> but ppl may want to add things like "draw circle" or "draw letters"
> extensions)

## Digest

Goals, in the owner's order:

1. Read the screen of a wrapped pty.
2. Draw on it: animations, small tools (confetti, flower, clock).
3. Expose the handles of a remote controller, selenium/playwright style:
   debug tools for terminals.

First milestone: a demo on a fast Rust backend, a clear pilot/server
protocol, a clean `ptyrc <verb>` CLI.

Concepts:

- **Raw frame**: the whole screen. Plain text without colours or escapes,
  keyword coordinates, grep.
- **Extended frame**: a surface larger than what is shown. The app scrolls
  (any direction); scrolling is probed, detected, and the content seen is
  remembered in all directions. A *view* is the visible part of a *frame*.
  A drawing anchored on the frame moves with the content when the user
  scrolls.
- **Dynamic frame**: segments the screen into panes from static structure
  (box-drawing lines and the like), one extended frame per pane. A sudden
  whole-screen change that is not a large scroll may allocate a new pane
  set; earlier ones are recognised again (tmux windows, tabs).
- **Safe heatmap**: per frame, a long-running mask of which cells are
  usually free and static, and which move.
- **Candidate pool**: frames and panes wait 1-3 s of stability before they
  become usable. Drawing stops on what changed until the heuristics settle.
- **Magic draw**: "a 6x18 clock, prefer top-right", "an 8x8 flower next to
  the word flower". Placement and tracking are the server's job.

Constraints:

- No flicker, no bugs.
- App-agnostic core: no probing of a specific TUI.
- Animations run in the native core, not over the socket.

Layout wanted:

- `rust/`, `python/` packages and modules.
- `plugins/client-apps/python/`: confetti, flower, clock, and the like.
- `plugins/tui-support/`: per-app presets for frame and view heuristics.
- `plugins/draw-ext/`: extra draw functions (circle, letters).
- `plan/intent.md` (this file), `plan/design.md`.

Deliverable: the whole thing working smoothly, with dynamic examples; at
least one original (a self-playing snake that eats apples around the words
of a scrolling vim buffer; falling sand in an idle tmux pane).

## Inherited from ptyrc 1

`temp/ptyrc/` is the owner's first version (Python, pyte, JSON lines over
localhost TCP, `ptyrc-driver` / `ptyrc-pilot`). Kept: the driver/pilot
split, pass-through of the wrapped app, `draw` / `draw2d` / animations,
`interact`, `show`, `input`, tasks, the confetti example. Take from it
freely.
