# Findings: third proof, 2026-11-20 (saved by the pilot from the agent's report)

Confined to `--help` and what `ptyrc dev new` writes. Looks outside: none.
Run before the `timing` agent's commits (`--timing` did not yet fail a
slow call).

- `ember` (Rust; `canvas::Half`, `mix`, `Sight`, `dice::Seeded`,
  `Canvas::sprite()` against a counter's): passed, 4 iterations. 6
  cases, 5 unit tests. Tick on 200x60, `dev test --timing`, 200 ticks:
  40 sparks 1.07 ms mean, 1.61 max; 400 sparks with text and pointer
  1.37 mean, 2.20 max.
- `pad` (script; deck, click and wheel, `--grab` by `needs`, storage,
  the frame at start): passed, 2 iterations. 9 cases, 5 unit tests.

1. Rust README line 84: `size() -> (columns, rows)`; it returns
   `Result<(u32,u32), Error>`. `subscribe` returns a `Result` too: not
   said.
2. `Half` (README 168-174) has no transparent pixel: a full-screen
   `Half` covers every cell. Holes punched by hand where both pixels
   are the background.
3. Whether `subscribe` adds to or replaces earlier subscriptions (a
   `Sight` subscribes too): not said. It adds.
4. A sprite behind the text needs `pointer` for `hover` (PROTOCOL.md
   line 22 implies it): never said outright for a canvas.
5. `dev test` has no wheel flag; `--event T:mouse:{...}` works; the
   widget's sprite id is not documented (1 held).
6. `--timing` help: "host calls in it"; the line printed has none.
7. `ptyrc.d.ts` line 548 cites `sdk/js/README.md`, not shipped.
8. `dev check` prints "asks keys-arrows, layout, storage": not that
   `keys-arrows` is asked only with `--grab`.
9. An act denied in `init` ends a script app `exit: 1` (AGENTS.md
   109-112 say so); no scaffold example catches it.
10. `Sight` `Options` fields and `Half::put` are typed only in the
    sources, not in the README table.
11. `dev new` writes `surface = "screen"` with a TODO: nothing helps
    choose (`frame` and `widget` guessed).
12. The frame told at start is 0 headless: whether `layout` is needed
    to see a real one live is not said, and cannot be tested headless.
13. `cargo test` in `test.sh` leaves a `Cargo.lock` the scaffold's
    `.gitignore` does not list.

Good: AGENTS.md and the help of `dev test` were enough for nearly
everything; the refused-option line; the hidden-widget note; the
`dropped`, `refused`, `lowered` lines; `--storage-out`, `--cases`.
