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

```py
Connected to "vim /home/plcp/.vimrc"
>>> pilot.argv
['vim', '/home/plcp/.vimrc']
>>> pilot.size
(110, 13)
>>> pilot.cursor()
Cursor(x=0, y=0, visible=True, frame=2, trail=(), pos=(0, 0))
>>> pilot.find("syntax on")
[Match(frame=0, x=0, y=10, len=9, text='syntax on', screen=(0, 10))]
>>> pilot.send("jjj$")
>>> pilot.cursor().x
35
>>> pilot.send("ohello world!")
>>> pilot.keys("esc")
>>> pilot.text()[4]
'" hello world!'
>>> pilot.draw(["<3"], near="nocompatible", fg="red")
Sprite(1)
>>> pilot.keys(":q!", "enter")
>>> pilot.connected
False
```

For example, if you want to pilot manually:

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

Perfect weather for a coding session:

![rain](doc/rain.gif)

Freeze-cold `htop` monitoring:

![sand](doc/snow.gif)

Sand pouring on your screen:

![sand](doc/sand.gif)

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

Honestly, the codebase is still full of slop:

- this human have NOT pour in the time (yet) to make it good to look at :(

You can still point your agent at it to write custom apps.

## License

This project uses `GPL-3.0-or-later` where applicable.

See `LICENSE.txt`.
