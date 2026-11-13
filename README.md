> This has been produced with assistance of generative coding agents.
>
> This file was written by a human. Use at your own risk.

# `ptyrc2` – the pty remote controller

This started as [`ptyrc`](https://github.com/pcplpc/ptyrc) a work done by hand
with the love of a human, but now with slop and unreviewed code.

The initial goal was to wrap a CLI application like [`vim`](https://vim.org) to
add confetti to its output.

![confetti](doc/confetti-all.gif)

See [gallery](#Gallery) below.

# Usage

Wrap any application with `ptyrc` as follows:

```sh
ptyrc run vim ~/.vimrc
```

Start it with a client app already on it:

```sh
ptyrc run --app bricks -- vim ~/.vimrc
```

You can use `ptyrc python` to start an interactive session:

![pilot](doc/interact.gif)

Running `ptyrc python script.py` runs the script with `pilot` defined.

## Gallery

If you already started a driver, you can bind a user app as follows:

```sh
ptyrc app bricks
```

You can also directly start a user app with the driver:

```sh
ptyrc run --app fishes -- vim LICENSE.txt
```

You then get fishes:

![fishes](doc/fishes.gif)

----

You can also get the perfect weather to appreciate becoming obsolete:

![rain](doc/rain.gif)

----

You can cool down your terminal as the planet gets hotter too:

![sand](doc/snow.gif)

----

You can easily sandbox any of your favorite applications:

![sand](doc/sand.gif)

----

If you want to work on your ADHD as you are reviewing bad code:

![snake](doc/snake.gif)

----

If you want to feel productive as you take notes on meetings:

![bricks](doc/bricks.gif)

----

Then finally, you remember what matters in life:

![cats](doc/cats.gif)

----

These were made by a human banging its head on an agent, no code written.

Open a push request to add your own :)

# Install from source

It's not published
on [crates.io](https://crates.io)
nor [PyPI](https://pypi.org), install from source.

Likely requires Linux, a Rust toolchain, `Python 3.13`
and optionally [just](https://just.systems).

```sh
git clone https://github.com/pcplpc/ptyrc2
cd ptyrc2
just install
```

This installs `ptyrc` into:

- `~/.cargo/bin` for the `ptyrc` native driver;
- `ptyrc-app` and `ptyrc-python` into `~/.local/bin` for pilot clients;
- both must be in your `PATH` to work.

Without using `just` you can:

```sh
cd ptyrc2
cargo install --locked --path rust/ptyrc-cli
uv tool install \
    --with ./python/ptyrc \
    --with-executables-from ptyrc \
    ./plugins/client-apps/python
```

Use `just uninstall` to remove them.

To try without installing anything outside the clone:

```sh
just run vim        # cargo build --release && uv sync, then ptyrc run vim
```

Use double `ctrl+t ctrl+t` to start a random user app in an idle driver:

![ctrl-t](doc/ctrl-t.gif)


## Development

You can use:

```sh
just test           # cargo test --workspace && uv run pytest
just lint           # clippy, rustfmt, ruff
```

Honestly, the codebase is still full of slop, it needs way more work.

You can still point your agent at it to write custom apps.

## License

This project uses `GPL-3.0-or-later` where applicable.

See `LICENSE.txt`.
