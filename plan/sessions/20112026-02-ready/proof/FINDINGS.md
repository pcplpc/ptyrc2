# Findings: second proof, 2026-11-20 (saved by the pilot from the agent's report)

Confined to `--help` and what `ptyrc dev new` writes. Looks outside: none.

- `tiles` (script, deck, `storage`): passed, 5 iterations. 7 cases
  (hover, click, focus and key, stored second run, small screen, seed,
  storage denied), 6 unit tests.
- `drift` (Rust, `pointer`): passed, 7 iterations. 7 cases, 6 native
  tests. Tick at 200x60, 2000 particles: 46 us native; headless 2.1 ms
  at 300 particles, 5.8 ms at 2000, host drawing included.

1. `dev test --cases` on a Rust app: the cases rebuild into
   `./target/ptyrc` at once and race ("Directory not empty", "not made
   by a build", a missing `.rlib`; a false hint "toolchain older ...
   rustup update"). One case alone, or `./test.sh` in sequence, pass.
   Fix: build once before the cases, or lock the build directory.
2. The deck has no click handler; `mouse` subscribed by hand; `action`
   values and the space of `at` not documented (guess: outer cell with
   the border; `rect` + 1 worked).
3. No tool reports tick time: only `limit met` at 50 ms; nothing in
   `--json`. Wanted: ticks, mean and max guest time.
4. `storage:` report lines mix preloaded and written keys; with
   `--storage DIR --deny storage` the directory's values are listed.
5. A second run is hand-made: `--storage` is never written back.
   Wanted: `--storage-out DIR` or a documented pattern.
6. `hover` on the app's own sprite behind the text needs `pointer`:
   worked out from the table and a `dropped ... not held` line. What
   "`sprite` null: left" means for a canvas covering the frame: unclear.
7. Two case runners (`test.sh`, `--cases`): nothing says which is
   canonical.
8. Whether `random_u32()` counts toward 1000 calls a second: not said.
9. `PROTOCOL.md` line 8 cites `rust/ptyrc-box/tests/protocol.rs`.
10. `dev check` prints `ok`, exits 0 with a TODO summary the install
    refuses.
11. The option summary leaves out `choices`, shows ranges.
12. `dev run` not tried (no live session).

Good: the hidden-widget note, the `dropped ...` lines, the option-bound
errors.
