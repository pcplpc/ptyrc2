# Handover: store, session of 2026-11-20

For the next pilot. The session before: `plan/sessions/19112026-01-wrap-up/`
(`handover.md` there holds the working method: briefs, git rules,
load; it still applies). This session: `git log b0ba1d2..` (about 105
commits), 19 agent runs.

Goal given by the owner: everything ready to migrate the shipped
Python apps to the store model, without migrating them yet.

## Read, in this order

1. `AGENTS.md`; `plan/sessions/20112026-01-store/rules.md` (the rules handed to
   every agent: point each brief at it).
2. This file.
3. `plan/sessions/20112026-01-store/design/readiness.md` (the audit: one row an
   app, the gaps by wave), `host-functions.md` (the contract of the
   host functions, options, primitives; `[as built]` lines say what
   exists; at its end the owner's questions), `deck-api.md`.
4. `plugins/store-apps/README.md` (the migration guide),
   `sdk/template/PROTOCOL.md`, `sdk/js/ptyrc.d.ts`,
   `sdk/rust/ptyrc-app/README.md`, `rust/ptyrc-box/README.md`,
   `rust/ptyrc-store/README.md`, `rust/ptyrc-probe/README.md`.
5. `plan/sessions/20112026-01-store/proof/FINDINGS.md` (what a stranger tripped
   on: the next thing to fix).

## State at the end

Every agent has reported; none is running. The three last reports
(`minmax`, `headless-3`, `e2e-i`) are at the end of this file.

A full run was made by the pilot after every agent had reported
(15:26, load 20; log: `final.txt`), with the commands below: no test
failed (82 test binaries, 927 tests, script and Rust-build tests run,
not skipped), clippy on the workspace printed no warning, `uv run
pytest`: 1170 passed (203 warnings, 2 this morning: to look at), both
SDK suites and the 8 example `test.sh` passed. Not run in it: the
`scripts/e2e-*.sh` (see `e2e-i` below). Run the same again first:

```
sdk/js/interp/build.sh
cargo build --workspace && cargo build --release -p ptyrc-cli -p ptyrc-box
PTYRC_REQUIRE_JS=1 PTYRC_REQUIRE_RUSTC=1 cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets
uv run pytest -q                      # 1170 passed at b0ba1d2
(cd sdk/rust/tests && cargo test); (cd sdk/rust/ptyrc-app && cargo test)
for t in sdk/*/examples/*/test.sh; do sh "$t" || echo "FAIL $t"; done
```

Baseline of this morning: `baseline.txt` (all green, one clippy
warning since fixed). Flaky under load, pass alone:
`storeapp::a_store_app_from_a_repository_to_the_screen`,
`test_stars::test_the_host_mouse_in_the_grass`, `test_train::test_app`,
`e2e-frames` (`monitor`), the hostile timing tests of `ptyrc-box`.

## What landed

| piece | state | where |
|---|---|---|
| `[limits]`, `ptyrc:manifest`, grants anew at restart, headless warning, three colours | done | `2ce5cb9`, `5ec5065`, `4a9545f` |
| QuickJS in place of Boa | done; 1.0 MiB, 12000 `set` 8.5 to 8.9 ms, start 0.18 to 0.20 s | `sdk/js/interp` (`build.sh`: clang, `wasm-ld`, three pinned inputs fetched) |
| Runner stack 1 MiB (owner), JS limit 352 KiB | done | `0c07d8a` |
| Host functions in the driver: `clock.*`, `storage.*`, `app.result` (`--wait`), `file.*`, `probe.git`, `probe.system`; `asset.*` by the runner | done | `hostio.rs` (two lanes), `driver/hostfn.rs`, `storage.rs`, `files.rs`, `ptyrc-core/src/zone.rs`, `rust/ptyrc-probe`, `ptyrc-box/src/assets.rs` |
| The same, headless: `--clock --zone --storage --file --probe`; `batch`, `widget.*`, `draw.list`; `--pointer --click --focus --paste --resize`; the `idle` and `widgets` events; a fixed stepping cadence | done | `ptyrc-box/src/{headless.rs,hostfn.rs}` |
| Manifest options: `short`, `choices`, `list`, `[args]`, `word`, `pattern`, `file`, `dir`, `needs`; launcher `--front --back --prefer --alpha --border`; the user's words and patterns; file handles | done | `ptyrc-store/src/options.rs`, `5a3c79e` |
| Rust tier: `ptyrc-app` with no dependency (own JSON), built by `rustc` alone under Landlock at `store add`, `dev check/test/run` | done | `ptyrc-store/src/build.rs` |
| SDKs: the deck (`ptyrc/deck`, `deck.rs`), host helpers, `dev test --unit` (scripts), `cargo test` (Rust); examples `clockface`, `tally` in both tiers | done | `sdk/` |
| Shipped store apps: `plugins/store-apps/NAME`, origin `ptyrc/NAME`, tier `shipped`; the harness; `confetti`, `sand` moved (Python twins kept); the guide | done | `rust/ptyrc-cli/tests/store_apps.rs` (`PTYRC_BLESS=1`, `PTYRC_STORE_APP=NAME`) |
| Driver: the surface follows its pane; the pointer told to an effect; `draw.list` landing by degree; own sprites in `screen.text`; mouse modes narrowed | done | `driver/heir.rs`, `driver/nozzle.rs`, `6a0ce73`, `c676515`, `8e99c8d` |
| Small holes of the old todolist (clippy, e2e scripts, `manage ls/log` glyphs, `test_pace`, box README) | done | `tools-c` commits |
| The proof (a stranger writes a script clock with zone and storage, a Rust notes app with a file) | passed, about 5 iterations each, no look outside | `proof/` |
| Root `Cargo.lock` | committed `6cb1a03` by the owner's word | |

## Left before the migration, in order

1. What the three last reports leave (end of this file): small.
2. **The proof's findings** (`proof/FINDINGS.md`), one agent, after
   nobody is in `ptyrc-box`/`dev`:
   - bug: `dev test --zone Asia/Tokyo --deny clock` exits 2 ("not a
     zone of the database"); the zone database is read only when
     `clock` is held;
   - `dev check` complains of an `[app] random` never written, no cure
     in the hint;
   - a widget that fits no variant vanishes with no report line; margin
     2 against the documented 1;
   - the collision note (`--zone`, `--size`, `--seed`) on every run,
     never saying who took the value;
   - `--storage DIR` layout undocumented; the report shows sizes only;
   - Rust SDK: `File::text()` drops `mtime_ms` (the no-clobber recipe
     has a window): return it with the text, in both SDKs;
   - `deck::fit`/`cut`/`wrap` types (`usize` against `u32`), absent
     from the README;
   - the template `test.sh` ignores the exit status of `dev test`;
   - the template README points outside the app directory;
   - the smaller ones listed there.
   Then run the proof again (brief: the `proof` agent's, in this
   session's transcript; ask for a deck app with a pointer and a Rust
   simulation this time).
3. **Not done, known**:
   - `grid` as a widget's content (the driver refuses it; a deck sends
     `cells`): about 50 lines in `ptyrc-core/src/widgets`; worth it for
     many-coloured canvases only.
   - Widgets are not moved to the heir pane (sprites and the surface
     are).
   - One lane for all probes: an app's hung `git` (16 s worst case)
     delays the others' probes.
   - `pace` loses a word when the cursor leaves the row before the
     next look (`tools-c`).
   - `plan/design.md`: `--unit`, the deck, are not in it.
   - Not tested: `probe.system` off Linux; `ptyrc-cli` clippy was last
     confirmed by `headless-2` only for its three crates.
   - The lowering rules still live twice (`ptyrc-server/src/{grants,
     lower}.rs`, `ptyrc-box/src/grants.rs`, pinned by tests); grown
     today on both sides. Moving them to one crate is M and worth doing
     before 26 more apps lean on headless.
   - Primitives that use without reading, not blocking: `anchor
     {pointer}`, `motion {flee}`, `"when": "idle"`, `"fade": "heat"`
     (`host-functions.md`, Primitives).
   - Old todolist items untouched: the on-disk cache of the compiled
     interpreter (measured: not worth it now), resume of a paused app,
     `wasmi`, registry hosting, `store update` at restart, regional
     indicators with mode 2027.
4. **Documents**: `plan/store-model.md` to mark done/decided (only its
   Rust and Colours parts were touched); the glyph paragraph for
   `plan/design.md` (text in the old `todolist.md`); the doc pass of
   `/home/peon/CLAUDE.md` over today's code and READMEs (many hands
   wrote them).
5. **Migration** (milestone 7), by the waves of `readiness.md`, each
   agent following `plugins/store-apps/README.md`. Suggested first: one
   deck app (`clock`) and one Rust app (`life`) as models beside
   `confetti` and `sand`, before fanning out.

## Decided by the owner today

- The interpreter for users without clang: a build server later;
  nothing now.
- The runner's stack: 1 MiB.
- The root `Cargo.lock`: committed (once, by word; root files stay the
  owner's).
- `min`/`max` on numeric options: supported.

## Taken on the design's recommendation, not confirmed

The owner was told and did not object; each is reversible.

1. Zone database: our TZif reader over the system's files, no crate.
2. `needs`: a permission asked only when its option is given.
3. Storage: 64 named values, 1 MiB an app.
4. A default file in a manifest allowed (asked as `files`, shown).
5. Shipped apps: origin `ptyrc/NAME`, tier `shipped`, asked by the
   pop-up as any app.
6. `--border` a launcher option.
7. `probe.git` with no directory: the wrapped program's working
   directory; roots cut to their last component.
8. Assets served by the runner, not the driver.
9. `pace`: counts in storage, `--save` dropped (not built: migration).
10. Shipped Rust apps built with the release.
11. `flower -a` needs `text`; a word outside the rule refused (not
    built: migration).

## Choices made by agents, for the owner to confirm

- Amber permissions come unticked, ticked by hand a row (plan: "listed
  apart, one yes").
- A restart keeps the manifest of the start, re-reads the answers.
- The headless warning: store apps only; the client's stderr and the
  app's log.
- Over quota or rate: `not-usable`; a result too large: `bad-params`;
  `storage.get` on a planted link: `null`.
- `--wait` escapes control characters to a pipe too; `manage log` to a
  pipe is raw.
- A typed file asks nothing; `files` only for a default file;
  `files-write` only when a file to write is given. A default `word`
  is not held unless declared in `words`. Does `--back` put a modal
  behind too?
- `probe.system` `disks`: absolute mount points and devices. The
  user's global git config is read by the probe. The git stamp does
  not watch the work tree.
- `probe.git` again within a second with other params: `not-usable`.
  A symlink in place of a named file: absent on read, replaced on
  write.
- `zone.rs` in `ptyrc-core` (the crate reads files, gains `rustix`);
  the whole zone database read up front when `clock` is held.
- Rust build: no Landlock: refused unless `--force`; `dev` builds
  unconfined and says so; `[dev-dependencies]` allowed; `/usr/lib`,
  `/lib` readable; NixOS toolchains fail. Shipped Rust apps built
  unconfined.
- An effect told the pointer turns on the host mouse with no
  permission (takes plain selection from the user). The hotkey's
  `manage` exempt from the checks.
- An installed app hides a shipped one of the same id.
- The deck is a second module `"ptyrc/deck"`; Rust `Deck<S>` takes
  `&mut S`; `civil` weekday 0 is Sunday; lists return names only.
- An option colliding with a `dev test` flag: before `--` the tool's,
  after it the app's (not an error).
- Not landed, still the owner's from before: the host pen patch
  (answer 23: about 0.3 ms, not 0.1), the read of `git show a9a0023 --
  plan/store-model.md`.

## For the owner's root files

- justfile:
  ```
  store-apps *args:
      PTYRC_REQUIRE_JS=1 cargo test -p ptyrc-cli --test store_apps {{args}}
  ```
- Root `Cargo.toml`, dev profile: `opt-level = 3` for `regalloc2`,
  `cranelift-frontend`, `-entity`, `-bforest`, `-control`,
  `wasmtime-cranelift`, `wasmtime-environ`, `wasmparser`, `gimli`: a
  debug start of a script app from 7.5 s to 4.25 s of CPU (measured by
  `headless-2` in a separate target directory).

## What this session learned about piloting

- Nine of the sixteen agents that finished passed 250k tokens (257k
  to 293k), most in their final
  checks: brief at 150k to 200k "aim", 250k "cap", and say the final
  check and report must fit.
- An agent "waiting on its own background work" for an hour was
  waiting on a full test run under load: ask for the report.
- One fork early (the audit and the design, 190k) paid for the day:
  every later brief pointed at its two files, and agents kept them
  true with `[as built]`.
- Two collisions: a commit built from a stale blob undid another's
  hunks in `dev/new.rs` (restored by the owner of the hunks); shipped
  apps leaked into every test sandbox (fixed, `4b17664`). Both from
  two agents in `rust/ptyrc-cli` at once: `dev*`, `tests/` are the
  crowded files.
- Uncommitted work of one agent turns another's final check red
  (`store_apps` screens, `protocol.rs` rows): tell each brief which
  tests are red and whose.
- Concurrent cargo runs wrote a duplicate entry into the uncommitted
  `Cargo.lock` several times; committing the lock ended it.

## Reports that came after this was written

### `minmax` (done)

Commits `6df9044`, `422c110`, `54775c3`. `min`/`max` on `int`/`float`
options: checked at manifest parse (type, `min <= max`, the default
inside), enforced by `options::read`/`resolve` on every value (list
items too), shown in `--help` and the `manage` row. `[args]`
`min`/`max` still count positionals. confetti's bound moved to its
manifest. Verified: `cargo test -p ptyrc-store`; `store_apps` (3),
`storeapp` (13) with `PTYRC_REQUIRE_JS=1`; `ptyrc-box --test protocol`
(4); clippy on `ptyrc-store` clean.

Left: `ptyrc dev test --help` lists options without the bounds, and
its error repeats the name (`--bursts: bursts: 11: 1 to 10`);
`sdk/rust/examples/confetti` still clamps `bursts` in code.

### `headless-3` (done)

Commits `786a46b`, `1e1c420`, `3cb1e94`, `1714165`, `9e4ac8e`.
- Headless tells the pointer to an effect as the driver does
  (`rust/ptyrc-box/src/nozzle.rs`, a copy of `driver/nozzle.rs`, pinned
  by `tests/protocol.rs`); `--press MS:X,Y`, `--release MS:X,Y`.
- The three `sand` trees use the primitive; the store app's pad is
  gone; `--bomb` and `-a` need `clicks-grab` alone. Re-blessed:
  `bomb.txt` at 3000 ms (grains thrown up after the release; the old
  screen showed no blast); `hover.txt` identical.
- Headless mirrors: `draw.list` landing by degree (`grants::landing`),
  own sprites alone in `screen.text`/`screen.cells {sprites}`.
- `storeapp::a_shipped_app_through_a_live_driver`: `ptyrc app
  ptyrc/confetti` found, granted with the warning, drawn; `ptyrc app
  confetti` runs the native twin, the note names both.
- Verified: `PTYRC_REQUIRE_JS=1 PTYRC_REQUIRE_RUSTC=1 cargo test
  --no-fail-fast -p ptyrc-box -p ptyrc-cli`: 268 passed, 0 failed;
  clippy clean on both; the 8 example `test.sh`; `dev check` on the
  three sand trees.

Left: no `--drag` (the blast's `dx, dy` come from the release cell).
A third copy of driver rules now lives in the box (`nozzle.rs`): one
more reason for the single crate of rules.

For the owner: the primitive tells the effect the pointer even when
another sprite covers it (the Python pad did not); headless reports a
press not taken as a `clicks-grab` lack and a hidden landing as a
lack, the driver reports neither.

### `e2e-i` (done)

Commits `ca28e92`, `6bd169c`.
- A frame the user gives is the app's surface, whatever its degree:
  `app.move` moves the surface and its sprites as `heir.rs` does, then
  tells `move` and `size`; a native app below `layout` given a frame
  (`app.start {frame}`, `app.restart {frame}` (new), `app.move`) gets
  `Surface::Frame(N)`. `ptyrc manage` sends `-f`/`--frame` as `frame`.
  Tests: `manage::a_native_app_below_layout_is_where_the_user_puts_it`,
  `storeapp::a_store_app_is_where_the_user_moves_it`.
- `uv run pytest -q`: 1170 passed. `test_stars` alone: 20/20 at HEAD,
  20/20 at base. `PTYRC_REQUIRE_JS=1 cargo test --no-fail-fast -p
  ptyrc-server -p ptyrc-cli`: 331 passed, 0 failed. Clippy on
  `ptyrc-server`: clean.
- The six e2e scripts at HEAD, in sequence, load about 10:

  | script | ok/fail | failures |
  |---|---|---|
  | manage | 26/1, then fixed (`6bd169c`: a stale column width) | none left |
  | frames | 98/4 | pages, pages-share, man, monitor-generic: pass alone (29/29) |
  | heat | 10/0 | |
  | render | 8/2 | vim insert, tmux scroll: pass alone |
  | tmux-attach | 47/1 | "the pane names the session": passes alone |
  | tuis | 91/1 | "man: one pane, full width": passes alone |

  No regression of the driver found. The e2e scripts are not reliable
  under load: run them alone, one at a time.

For the owner: `app.move` still wants the app subscribed to `move`;
an app below `layout` is told the real frame id in `move` and 0 in
`draw.list` (should `move` say 0?); a native app restarted with
declarations that raise it to `layout` keeps an old surface entry
that `heir` still follows.

