> This has been produced with assistance of generative coding agents.
>
> This file was written by a human. Use at your own risk.

# `ptyrc2` – the pty remote controller

This started as [`ptyrc`](https://github.com/pcplpc/ptyrc) a work done by hand
with the love of a human, but now with slop and unreviewed code.

The initial goal was to wrap a CLI application like [`vim`](https://vim.org) to
add confetti to its output.

![confetti](doc/confetti.gif) # TODO make a new one

# Quick setup

This is not yet published on [PyPI](https://pypi.org):

```sh
# install instructions assuming https://github.com/pcplpc/ptyrc2 as url
```

Wrap any application with `ptyrc` as follows:

```sh
ptyrc run vim ~/.vimrc
```

You can then connect to it and send it text to type:

```sh
ptyrc send 'iHello world!' && ptyrc keys esc
```

You can use `ptyrc python` to start an interactive session:

```py
Connected to "/usr/bin/vim /home/plcp/.vimrc"
>>> # TODO generate this example
```

You can also start a user-provided app that draws on screen:

```sh
ptyrc app bricks
```

If no pilot is attached `ctrl+t ctrl+t` twice starts one at random.

## Development

You can use:

```sh
cargo test --workspace
uv sync && uv run pytest
```

Honestly, the codebase is still full of slop, and this human haven't pour in
the time (yet) to make it good to look at.

You can still point your agent at it to write custom apps.

## License

`GPL-3.0-or-later`. See `LICENSE.txt`.
