# Findings: two apps from `ptyrc dev new`, docs only

Read: `--help` of `ptyrc`, `dev`, `dev new`, `dev check`, `dev test`; the
generated `AGENTS.md`, `PROTOCOL.md`, `ptyrc.d.ts`,
`vendor/ptyrc-app/README.md`, `vendor/ptyrc-app/src/*.rs` (written by
`dev new` into the app). Nothing else of the repository.

## 1. worldclock (script): passed, ~5 iterations

`dev check` clean (one note), `./test.sh`: 5 unit tests (`fmt.js`) and 8
cases: big variant local zone and `-- --zone Europe/Paris`; small (20x6);
key toggle (`--focus 1200:clock --key 1500:f`), stored; second run
`--storage cases/storage-12h`; clock, storage, both denied.

1. `dev test . --zone Asia/Tokyo --deny clock` exits 2: `ptyrc-box: --zone
   Asia/Tokyo: not a zone of the database, nor a POSIX rule (UTC-2)`. Fine
   without `--deny clock`; `--zone UTC` fine with it. A bug; the message
   misleads. Workaround: no `--zone` in denied cases.
2. App option `zone` clashes with dev test's `--zone`. The note names the
   cure (`-- --zone VALUE`), but is printed on every run, even when the
   app's is correctly after `--`, and by `dev check`. It never says which
   one took the value.
3. `--storage DIR` layout undocumented; guessed one file per key, content
   the value: worked. Report `storage: format 2 bytes` shows no value
   (only `--json`).
4. A widget that fits nowhere is silently absent (`--size 20x5`): empty
   screen, nothing reported. The widget sits 2 rows below the top
   (margin documented 1). Two probing runs.
5. No widget at 0 ms (first `widgets` event later); undocumented.
6. 40x7: small variant, then big shown over the text once focused;
   unexplained.
7. Template `test.sh` has one case, one `expected.txt`; no multi-case
   pattern, no check of the `dev test` status.
8. Template manifest keeps `summary = "A clock in the top right corner."`
   and `surface = "screen"`; `dev check` accepts them.
9. "Tick on the second" formula in PROTOCOL.md ignores that `period` < 33
   is raised to 33.

## 2. notes (Rust): passed, ~5 iterations

`dev check` clean, `./test.sh`: 4 native tests (`src/lines.rs`; the
no-clobber path via `native::serve` answering `not-usable`) and 4 cases:
file given (`--file ARG#0=cases/todo.md`), small, write allowed
(`--json` shows the written file), write refused (`--deny files-write`).

10. `[args] file = { type = "file", write = true, min = 1, max = 1 }`:
    `dev check`: `ptyrc: [app] random: []: file: 1 at least` / `each draw:
    words of the app's options, as ptyrc app takes them`. I never wrote
    `random`; the hint omits the cure. Guessed `random = []` from the
    comment "`[] never`": worked.
11. `deck::fit(&row, w)` takes `usize`, render gets `u32`: 4 E0308. README
    gives no types for `fit`, `cut`, `wrap`.
12. `File::text()` drops `file.read`'s `mtime_ms`; `stat()` after reading
    leaves a window where a change is clobbered. No safe recipe given.
13. `[args]` of type `file` and `--file ARG#0=PATH` pieced together from
    PROTOCOL.md (only a pattern example) and `dev test --help`.
14. Duplicate `--size` is an error, but my test script hid it and recorded
    an empty screen: template gives no status check (as 7).
15. README points at `sdk/js/ptyrc.d.ts`, `sdk/rust/examples/*`,
    `sdk/rust/tests`, `ptyrc_box::headless`: absent from the app dir.
16. `Error` has a private constructor; the `serve` example shows only `Ok`.
17. `attr::INVERSE` cursor invisible in text reports (`--styles` only);
    switched to a `> ` mark.

## Looks outside the allowed files

None.
