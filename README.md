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
```

## Repository

- `rust/ptyrc-vt` terminal model and repair painter;
- `rust/ptyrc-core` frames, heatmap, placement, scene;
- `rust/ptyrc-proto` wire protocol;
- `rust/ptyrc-server` the driver;
- `rust/ptyrc-cli` the `ptyrc` binary;
- `plugins/tui-support/` per-app presets (Rust);
- `plugins/draw-ext/` extra draw functions (Rust);
- `plugins/client-apps/python/` confetti, clock, snake and friends;
- `python/ptyrc` the Python pilot library;
- `plan/` intent, design, decisions, ideas.

## Development

```sh
cargo test --workspace
uv sync && uv run pytest
```

## License

`GPL-3.0-or-later`. See `LICENSE.txt`.
