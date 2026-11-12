# Interview, 2026-11-12

- Rendering: pass-through and repair, not a compositor. ptyrc is meant to
  run over a whole tmux session; a compositor inside a compositor makes no
  sense.
- Frames keep the recent cursor positions, so drawings can react to the
  cursor (fishes `<*//><` afraid of it).
- Ideas are noted in `plan/` as they come: `plan/ideas.md`.
- Git: `git init` in the repository; small terse commits, no co-authorship
  trailer, no session link.
- Plugins: `tui-support` and `draw-ext` are Rust crates compiled into the
  server behind traits. `plugins/client-apps/python/` stays Python.
  JavaScript client apps later, out of scope now.
- License: GPL-3.0-or-later.

## Later the same day

- Handoffs: when a topic is better served by a fresh session, write
  `plan/handsoff/<YYYYMMDDTHHMMSSZ>-next-steps.md` and keep working here.
  The owner starts the new session.
- Likely first handoff: the client apps (sand filling a pane, aquarium
  with fishes afraid of the cursor, better animated confetti, snake around
  the words of a long scrolling vim buffer, floating clock). Smooth,
  elegant, fast, responsive; written as examples people learn from.
- Order: frames, extended frames, dynamic frames, debug tools, tests
  against real apps and polish come first; the apps after.

## Closing the first session

- Layers, "as a sandwich", with proper transparency in frames. Default:
  the app's text is in front of our drawings; we draw behind it, in the
  background. Multi-layer scenes must be easy: a static background, in
  front animations, in front again the app's text, and so on.
- Client apps are a showcase of sprite handling and of scrollable frames.
  Not only vim: `less` and `more` should mostly work out of the box, with
  no preset.
- Confetti app: `--spawn --rate <rate>` spawns `<rate>` confetti per
  minute; `--spawn` alone is 20 per minute, one every 3 s.
- Fishtank: inside a scrollable frame; fishes align to the extended
  frame, not to the window.
- A good demo of the dynamic frames is wanted.
- TUIs to try: htop, less, man, tmux, ranger, glances, opencode.
- Sessions are kept around 200k tokens between handoffs.

## Opening the second session

- The session agent pilots a team: forked siblings, each on its own
  topic and paths, each free to delegate bounded pieces to sub-agents.
- Notes on background against foreground, static against hot, idle
  frames and pagers: `plan/ideas.md`, "opening the second session".
- Siblings work in git worktrees, `temp/wt-<topic>` on `fork/<topic>`,
  merged into `main` by the pilot. The post-commit timestamp hook stashes
  unstaged files while it runs: a shared tree would lose a sibling's
  edits.

## Confetti flags, late in the second session

Supersedes `--spawn --rate` above.

- A burst picks a cold spot by default; no flag for it. `--anywhere`,
  `--seconds`, `--seed` and `--rate` go.
- `--bursts N`: N bursts at different places, each starting after
  `random() * 3` seconds so they are not aligned; at most 10 at once.
- `--spawn` becomes `--party` ("party mode"): it keeps going, with the
  `--bursts` bursts in parallel, same random offsets.
- `--count` becomes `--size`.
- `--help` stays simple.

## Apps, end of the second session

- No `--seconds` ("stop after this many seconds") in any default app.
- Fishes: 8 by default. Those left out of the screen by a scroll despawn
  slowly, and respawn behind the text.
- Fishes: by default half of them, drawn at random at birth, are
  attracted by text and hide behind it.
- Sand: braille dots, not blocks. One thin stream from a free place
  near the middle of the top, chosen to fall on text; a pile that
  stacks, then falls by gravity; small physics, avalanches. Colours
  change very slowly, so the stream lays strata. A resize eats the sand
  it covers, the rest stays. Text written on the sand clears those
  cells only. `--cursor`: the stream falls in the cursor's column; no
  cursor, or on the first column: out of scope.

## Flower, third session

- No `--side`: a flower is planted in the nearest cold spot, its root 3
  to 10 columns from the word and 1 to 3 rows off its row, so it is seen.
- The match is redrawn in bold over the text.
- No `--nth`: flowers spawn next to random matches, new ones as they
  come and go. `--count N` (default 1): N matches get a flower; 0, all.
- No `--color`: random sizes and colours.
- `-a`, `--anywhere`: a pool of words seen on the screen, 3 to 5, and
  flowers next to their matches. No bold then.
- `-a` draws flowers anywhere: `--count` (number of flowers) is the
  size of the pool, 5 by default. Every 10 s one word leaves the pool,
  then another comes in. `-a --count 0`: the size is drawn between 10
  and 30, periodically.

## Fishes aquarium

- `-d`, `--decorate`: decorations that stand on text (small castles,
  sea shells, algae), behind or in front of the fishes, always behind
  the text.
- `-b`, `--bubbles`: animated dim bubbles from the bottom, slowly to
  the top.
- `-r`, `--random`: random types of fishes as they respawn, and events
  (objects that fall in the water, and so on).
- `-a`, `--all`: everything. The basic fishes stay basic; with `-a` it
  is a very fancy aquarium, scrolled in vim.
- Decorations, bubbles and animations remember the areas already
  populated.
- Only beautiful ASCII art and good animations; nothing bad.

## Snake arcade

- The snake gets fancy with `--effects` (`-e`): particles when it eats,
  and so on.
- Two or three creative flags as for the fishes, enemies and lasers
  among them: `-x`, `--enemies`; `-l`, `--lasers`.
- `-a`, `--all`: everything.

## Brick

- `brick`: close to the snake. Plain: an arcade brick breaker, coloured
  bricks, a ball and a racket, played by the app.
- A 2D scrollable map: bricks spawn at times on top of words, mostly
  below them; the racket is hit on any side, both sides are used; the
  racket works to keep the ball in the screen; the ball bounces off
  words too.
- `--rules`: bricks that fall when touched and can kill a racket,
  bricks with several lives, bricks that bounce the ball in a fancy
  way, bricks that explode with particles.
- `--bonuses`: the classics: two rackets, several balls, a faster
  ball, a ball on fire for 5 s that breaks all on its path.
- `--multiplayer`: two rackets compete, each has its score.
- `-a`, `--all`: everything; plain by default, the fanciest with `-a`.
- Never ending. The score is back to 0 when the ball leaves the
  screen; new areas spawn on demand when bricks are few; scrolled, the
  terrain persists.
- Look: racket `[===]`, 5 cells; `[=]`, 3 cells, on a bad bonus; ball
  `o`; bricks prettier than dashes, braille.

## Blocks

- `blocks`: falling pieces of four blocks, drawn in braille. Plain by
  default, as the snake; the computer plays alone.
- Pieces come from the top; from the right with `--sideways`.
- A full line between two boundaries (words or the sides of the
  screen) goes and gives points; lines are rows, columns with
  `--sideways`.
- Coloured by default. `--chains`: when a line breaks, the pieces of
  the same colour touching a block just broken break too, from one to
  the next: red to red to red, not to the green beside.
- `--effects`: particles on broken pieces. `--score`: a score is shown;
  the higher, the faster. `-a`, `--all`: all that is cosmetic.
- `--cursor`: the falling piece follows the user's cursor: blocks
  played in vim. `--count`: pieces falling at once, 3 by default; with
  `--cursor` the player has the closest.
