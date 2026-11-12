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
