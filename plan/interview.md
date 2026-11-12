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
