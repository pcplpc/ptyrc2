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
