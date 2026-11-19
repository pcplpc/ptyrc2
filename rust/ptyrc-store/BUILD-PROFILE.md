# Build profile (design, spike)

The build of a Rust app under an OS profile: `plan/store-model.md`,
Tiers of authoring > Rust, milestone 6. Not wired into `store add`.

## Steps

1. `cargo fetch --locked --target wasm32-unknown-unknown`,
   `CARGO_HOME=<build>/home`. The network; no code of the tree runs.
   Not confined. `--locked`: the tree's `Cargo.lock`, checksums from it.
2. `cargo build --offline --frozen --release --target
   wasm32-unknown-unknown`, `CARGO_TARGET_DIR=<build>/target`,
   `TMPDIR=<build>/tmp`, the environment cleared, the toolchain's
   `cargo` and `rustc` named (not the rustup proxy). Confined: the
   profile is applied by the parent before the spawn and inherited.

The profile of step 2 (Linux):

| | |
|---|---|
| Landlock, read and execute | the sysroot (`rustc --print sysroot`), `/usr` (the host linker `cc`, for build scripts and macros) |
| Landlock, read | the tree, `<build>/home` (the fetched crates), `/etc/ld.so.cache`, `/proc` |
| Landlock, write | `<build>/target`, `<build>/tmp`, `/dev/null` |
| Landlock, network and scopes | no TCP bind or connect; no abstract unix socket; no signal out |
| namespace | `unshare(CLONE_NEWUSER \| CLONE_NEWNET)`: no network at all (UDP: Landlock does not see it) |
| bounds | `RLIMIT_AS` 4 GiB a process; 300 s, then kill; the module 16 MiB, checked after |

## Spike, 2026-11-19

Linux 7.2, Landlock ABI 10 (the crate's 9), `landlock` 0.4.7 and `nix`
0.31, no `unsafe`. A program in `temp/build-spike` (not kept).

- `sdk/rust/examples/confetti`: fetched (5 crates, 2 s), built under
  the profile (`serde_core`, `serde_json`, `zmij` run build scripts;
  release, LTO) in 3 to 4 s; 77 528 bytes. The path dependency
  `sdk/rust/ptyrc-app` had to be added to the reads.
- A probe crate whose `build.rs` tries: `~/.bashrc`, `/etc/passwd`
  read; `~/`, `/tmp` written; TCP and UDP out; a unix socket: all
  refused. `exec /usr/bin/id`: done, inside the same profile.
- Landlock reports `PartiallyEnforced`: rights for directories asked
  on a file rule (`/dev/null`, `/etc/ld.so.cache`) are dropped. No
  effect on the result.
- `RLIMIT_AS` 4 GiB did not stop rustc and LLVM on this tree.

## Remains

- Path dependencies: a store tree may have none outside itself; the
  SDK crate `ptyrc-app` comes from the registry, or the tree vendors it.
- The bound of memory is per process: the whole build wants a cgroup
  (`systemd-run --user --scope -p MemoryMax=4G`, or the cgroup of the
  driver's own). The bound of time kills cargo, not its children: a
  process group, or the cgroup.
- `/proc`: readable whole. Landlock should refuse the `ptrace`-checked
  files of processes outside the domain (`environ`, `mem`), not
  `cmdline` (not tested); a mount namespace with a fresh `/proc` would
  close it.
- `/usr` is readable and executable whole: any program of the system
  runs, confined. Narrower: the linker and its libraries alone.
- seccomp for the build (no `ptrace`, `io_uring`, `bpf`): as the
  runner's, the kill class only.
- No Landlock (before 5.13) or no user namespaces: no Rust build from
  the store (the plan), `--force` aside.
- Licences of the fetched crates, from their manifests, at `check`.
- macOS: TBD (plan).
