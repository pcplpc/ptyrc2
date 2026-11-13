# SPDX-License-Identifier: GPL-3.0-or-later
# `just` lists the recipes.

# List the recipes.
default:
    @just --list

# Check that cargo and uv are installed.
check:
    @command -v cargo >/dev/null || { echo "cargo missing: https://rustup.rs"; exit 1; }
    @command -v uv >/dev/null || { echo "uv missing: https://docs.astral.sh/uv/"; exit 1; }
    @echo "cargo and uv found"

# Install `ptyrc` (~/.cargo/bin), `ptyrc-app` and `ptyrc-python` (~/.local/bin).
install: check
    cargo install --locked --path rust/ptyrc-cli
    uv tool install --force --reinstall \
        --with ./python/ptyrc --with-executables-from ptyrc \
        ./plugins/client-apps/python
    @echo "done: try 'ptyrc run vim', then 'ptyrc app confetti' from another terminal"

# Remove what `install` installed.
uninstall:
    -cargo uninstall ptyrc-cli
    -uv tool uninstall ptyrc-apps

# Build `target/release/ptyrc` and the `.venv` of the source tree.
build: check
    cargo build --release
    uv sync

# Wrap a program, built from source: `just run htop`.
run *ARGS: build
    ./target/release/ptyrc run {{ ARGS }}

# Run the Rust and Python tests.
test:
    cargo test --workspace
    uv run pytest

# Lint and check formatting.
lint:
    cargo clippy --workspace --all-targets
    cargo fmt --check
    uv run ruff check
    uv run ruff format --check

# Format the sources.
fmt:
    cargo fmt
    uv run ruff format
