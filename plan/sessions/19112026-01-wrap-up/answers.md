# Answers of the owner, 2026-11-19 and 20

To `questions.md`, by its numbers. Taken in an interview; the owner's
words where they add to the option.

## Done since

- 1: the root `Cargo.lock` is committed (`1cbd76f`).
- 2: the dev profile for `cranelift-codegen` is in the root
  `Cargo.toml` (allowed once).
- Root `README.md`: a section "Store apps" with the warning (allowed
  once). Root files are the owner's again.
- 3: `plan/store-model.md`, the hunk of `a9a0023`: the owner was told
  to read it; not confirmed read.

## Decided

- 4 **QuickJS, not Boa.** Spike (QuickJS-ng 0.18.0, measured): 1.0 MiB
  against 6.2; 12000 `set` in 7.7 to 7.9 ms against 50 to 57; first
  start 0.18 to 0.22 s against about 1 s. Needs clang, `wasm-ld`, the
  wasi sysroot (119 MB); the guest builds apart. To do: the error
  format `file:line:col` and hints; a refusal logged once; an app must
  not import `ptyrc:native`; an endless promise chain must end at the
  time limit; `Math.random` from the host's `random`; `sand`, `snake`,
  events; the notices of wasi-libc; then Boa removed. Spike:
  `.claude/worktrees/agent-aae6824a65261b426/temp/quickjs-spike/`.
- 5 `unsafe`: allowed in `ptyrc-box`, one module, for the cache of the
  compiled interpreter; the FFI of the guest crates accepted. With
  QuickJS starting in 0.2 s the cache may not be worth it: measure
  before writing it.
- 6 to 16: all confirmed as built.
- 17: `text` red; `clock` and `storage` asked (plain, ticked); three
  colours.
- 18 Headless and a store app: granted as declared, each permission
  granted named in a warning on stderr.
- 19 No full OS profile: start and log (as now), every platform.
- 20 A Rust build: `rustc` alone now (std and `ptyrc-app` only: no
  crates.io dependency, no `build.rs`, no macro crate; `ptyrc-app`
  loses `serde_json`; `dev check` says so). cargo under the full
  profile kept as a later tier (`BUILD-PROFILE.md`).
- 21 `ptyrc-app`: vendored.
- 22 Registry: stays in this repository for now. `tested`: the
  `MAINTAINERS` list, never the author; an optional `note`; signed
  tiers later.
- 23 The 2 ms bound: an agent looked. Landed (`9aa2f75`): the CSI fast
  path, the SGR writer; a colour a cell 2.89 to 3.00 ms against 3.33
  to 3.44. Not landed, by the owner's choice: the host pen set without
  parsing the colours; with it 2.44 to 2.67 ms. The owner chose on a
  figure of 0.1 ms for that part; measured, it is about 0.3 ms: ask
  again. Patch:
  `.claude/worktrees/agent-afdd8c805524eb3e2/temp/repaint-opt/repaint.patch`.
  Under 2 ms is not reachable; 3 ms accepted.
- 24 macOS: as 19.

## Asked by the owner

- The text cursor (`screen.cursor`, topic `cursor`) is read at degree
  `shape`; the mouse pointer is the permission `pointer`. Two things.
