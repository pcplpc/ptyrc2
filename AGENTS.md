# ptyrc2 - instructions for agents

The pty remote controller: wrap a terminal program, read its screen, draw
on it, drive it.

Read `plan/intent.md`, `plan/design.md` (binding contract) and
`plan/interview.md` first. Ideas go to `plan/ideas.md`.

## Rules

- Work only inside this repository. Scratch files go in `temp/` (ignored).
- Never write the name of a coding assistant in code or tests; use
  `agent` / `Agent` / `AGENT`. The preset for coding-agent TUIs is
  `agent-tui`.
- License: GPL-3.0-or-later. `SPDX-License-Identifier: GPL-3.0-or-later` at
  the top of every source file.
- `temp/ptyrc/` is the owner's first version: take from it freely.
- Coordinates are 0-based, `x` right, `y` down, everywhere.
- The core is app-agnostic. Anything that names a program lives in
  `plugins/tui-support/`.

## Rust

- Edition 2024, one workspace at the root. `cargo test --workspace`,
  `cargo clippy --workspace --all-targets`, `cargo fmt`.
- No `unsafe` outside `ptyrc-server`. No async runtime.
- No clock in `ptyrc-vt` and `ptyrc-core`: time is an argument.
- A new dependency goes in the crate's own `Cargo.toml`.

## Python

- Python >= 3.13, uv workspace, `src/` layout, hatchling. Stdlib only in
  `python/ptyrc`.
- `from __future__ import annotations`; sorted `__all__` per module.
- `uv run pytest`; `uv run ruff check`; `uv run ruff format`.

## Documentation

Exact and minimal. Plain, sober, precise; no filler.

## Git

- Small commits, terse messages (`ptyrc-vt: DECSTBM`). No co-authorship
  trailers, no session links.
- Several agents share this tree. Commit only your own paths:
  `git add <your dir> && git commit -m "..." -- <your dir>`. Never
  `git add -A`, never commit `Cargo.lock`, `uv.lock` or the root files,
  never rewrite history. Retry if the index is locked.
