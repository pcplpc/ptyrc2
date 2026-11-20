# Build profile

The build of a Rust app under an OS profile: `plan/store-model.md`,
Tiers of authoring > Rust, milestone 6.

## Wired: `rustc` alone (decision 20)

`build::build` (`README.md`, Build), at `ptyrc store add` and by
`ptyrc dev check`, `test`, `run`.

- No code of the tree runs at a build: no build script, no procedural
  macro exists without cargo. What is left is `rustc` reading a file
  (`include_str!`, `include_bytes!`, `#[path]`) into the module, which
  can then not send it anywhere but can draw it.
- `build::sources` refuses what says so in the text; the profile is the
  wall (a path built by `concat!`, a macro passing one along).
- `ptyrc-app` is ptyrc's: its sources embedded in the binary
  (`include_str!`, `build::PTYRC_APP`), written to the work directory at
  each build. Never fetched, never the tree's copy; no install path to
  find or to go stale; the version is that of ptyrc. 40 KiB.

Two runs of the toolchain's `rustc` (`<sysroot>/bin/rustc`, not the
rustup proxy), the environment cleared but `TMPDIR`, from
`<work>/ptyrc-app/` then `<work>/app/`. Each from a thread of its own
that applies the profile to itself and spawns `rustc`, which inherits
it; the process that asks is not restricted. Linux:

| | |
|---|---|
| Landlock, read and execute | the sysroot (`rustc`, `rust-lld`, the standard library), `/usr/lib`, `/usr/lib64`, `/lib`, `/lib64` |
| Landlock, read | `<work>/app/` (the tree), `<work>/ptyrc-app/`, `/etc/ld.so.cache` |
| Landlock, write | `<work>/out/`, `<work>/tmp/`, `/dev/null` |
| Landlock, network and scopes | ABI 4: no TCP bind or connect; ABI 6: no abstract unix socket, no signal out |
| bounds | 300 s for both runs, then the process group killed; `RLIMIT_AS` 4 GiB, `RLIMIT_FSIZE` 64 MiB, `RLIMIT_CORE` 0 a process; the module 16 MiB, checked after |
| the module | imports from `ptyrc` only |

Tested (`tests/build.rs`, `build::tests`): `rustc` refused a file out
of the work directory with the scan bypassed; `include_str!` of
`/etc/passwd`, `#[path = "/etc/passwd"]`, `#[link(wasm_import_module =
"wasi_snapshot_preview1")]`, `#![feature]`, `extern crate proc_macro`,
`build.rs`, a dependency: refused before; an `extern` block of the app's
own: refused by `rust-lld` (undefined); an endless `const`: ended at
the bound of time; a macro that doubles: ended; a static of 2 MiB
against a bound of 1: refused; of 8 MiB: `rustc` ended by
`RLIMIT_FSIZE`.

No profile (no Landlock, not Linux): `store add` refuses, unless
`--force`, which builds without one and says what it means; `ptyrc dev`
builds the author's own tree without one, and says so.

## Not held

- No network namespace: UDP is not seen by Landlock. No code of the tree
  runs, `rustc` and `rust-lld` send nothing; the cargo tier needs it.
- `/proc` is not readable: `rustc` does without it.
- The bound of memory is per process; one build is two processes at
  most (`rustc`, `rust-lld`).
- seccomp: none for the build.
- A toolchain whose libraries are outside the paths above (NixOS): the
  build fails, not open.

## Later tier: cargo under the full profile (spike, 2026-11-19)

Kept for a tier with crates.io dependencies, build scripts and macros.

1. `cargo fetch --locked --target wasm32-unknown-unknown`,
   `CARGO_HOME=<build>/home`. The network; no code of the tree runs.
2. `cargo build --offline --frozen --release --target
   wasm32-unknown-unknown`, `CARGO_TARGET_DIR=<build>/target`,
   `TMPDIR=<build>/tmp`, the environment cleared, confined as above
   plus: read and execute `/usr` (the host linker `cc`, for build
   scripts and macros), read `<build>/home`, `/proc`; a namespace
   `unshare(CLONE_NEWUSER | CLONE_NEWNET)` (no network at all).

Spike: Linux 7.2, Landlock ABI 10, `landlock` 0.4.7, `nix` 0.31, no
`unsafe`. `sdk/rust/examples/confetti` with `serde_json` then: fetched
(5 crates, 2 s), built under the profile in 3 to 4 s. A probe crate
whose `build.rs` tries `~/.bashrc`, `/etc/passwd`, `~/`, `/tmp`, TCP
and UDP out, a unix socket: all refused; `exec /usr/bin/id`: done,
inside the profile. `RulesetStatus::PartiallyEnforced`: rights for
directories asked on a file rule are dropped; no effect.

Remains for that tier: path dependencies; a cgroup for the memory of
the whole build; the process group for the time; a fresh `/proc`;
`/usr` narrowed to the linker; seccomp; licences of the fetched crates
at `check`; macOS.
