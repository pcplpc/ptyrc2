> This has been generated using an agent and has not been reviewed.
>
> Experimental use only.

# ptyrc

The pty remote controller: wrap a terminal program, read its screen, draw
on it, drive it. Selenium for terminals.

```sh
ptyrc run vim notes.md        # wrap; the terminal behaves as before
ptyrc text                    # from another shell: what is on screen
ptyrc find flower             # where
ptyrc keys esc : q enter      # type
ptyrc frames                  # the panes found on screen
ptyrc draw text --auto top-right ' 12:00 '
ptyrc app fishes              # or: sand, snow, snake, bricks, cat...
```

Read: `text`, `find`, `cursor`, `wait`, `frames`, `heat`, `idle`, `snap`,
`shot` (SVG, HTML, ANSI), `events`. Drive: `send`, `keys`, `click`,
`grab`, `attach`. Draw: `draw`, behind the app's text by default, fixed
on the text when it scrolls. Debug: `show`, `debug overlay`,
`run --record` and `replay` (a recorded session, served as a live one).

Presets: tmux, vim, ranger, htop, glances, coding-agent TUIs; pagers and
the rest need none. `plan/design.md` is the reference.

## Repository

- `rust/ptyrc-vt` terminal model and repair painter;
- `rust/ptyrc-core` frames, heatmap, placement, scene;
- `rust/ptyrc-proto` wire protocol;
- `rust/ptyrc-server` the driver;
- `rust/ptyrc-cli` the `ptyrc` binary;
- `plugins/tui-support/` per-app presets (Rust);
- `plugins/draw-ext/` extra draw functions (Rust);
- `plugins/client-apps/python/` `ptyrc-app`: confetti, clock, fishes, sand
  and friends;
- `python/ptyrc` the Python pilot library;
- `scripts/` end-to-end tests, demos, bench;
- `plan/` intent, design, decisions, ideas.

## Development

```sh
cargo test --workspace
uv sync && uv run pytest
scripts/e2e-frames.sh; scripts/e2e-tuis.sh; scripts/e2e-render.sh
scripts/demo-apps.sh; scripts/demo-frames.sh      # to watch
```

## License

`GPL-3.0-or-later`. See `LICENSE.txt`.
