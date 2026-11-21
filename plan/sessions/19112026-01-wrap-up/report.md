# Report: permissions and the store, 2026-11-19

One pilot session, 24 sub-agent runs. Plan: `plan/store-model.md`.
Everything is on `main`, from `be8324d` on (`git log 2d0ccb7..`).
Not committed, the owner's: the root `Cargo.lock`.

## State by milestone

| | milestone | state |
|---|---|---|
| 0 | Ground | done: token, `auth`, the check, the audit of drawn text, the modal no app covers |
| 1 | Runner | done: `ptyrc-box`, ABI 1, limits, hostile guests. Speed bound: held for one colour, not for a colour per cell |
| 2 | Scripts and tools | done: interpreter guest (Boa), `ptyrc.d.ts`, `dev new/check/test/run/entry`, `confetti`, `sand`, `snake` in both tiers. The proof: passed twice |
| 3 | Install | done: `store add/rm/update/ls/show/search`, the lock, `ptyrc app ID`, the `manage` menu. Script apps only |
| 4 | Permissions | tasks 1 to 5 done; task 6 partly (see below) |
| 5 | Registry | done but for hosting: sync, `index.json`, the page, the PR check as local scripts |
| 6 | OS profile | the runner's: done (Landlock, seccomp). The build's: designed, a spike worked, not wired |
| 7 | Migration | not started. The host functions are not written |

## What exists

**Driver (`rust/ptyrc-server`)**
- `grants.rs`: permissions, the ladder none < layout < shape < words <
  text, a table of every method and topic; a test ties it to the
  dispatch. `lower.rs`: the cut answers, each marked `"degree"`.
- A token per child (`PTYRC_TOKEN`), `auth` first; accepted only from
  the child's process group. `app.enrol`: the same for a foreground
  `ptyrc app NAME`, bound to the caller's pid.
- `permissions.rs`: `permissions.toml` (hand parser), `resolve`, the
  headless rule. `popup.rs`, `driver/asks.rs`: the pop-up; only host
  keys answer it. `g` in `ptyrc manage` reopens it.
- `store.rs`: `app.start {store}`: the driver finds the tree, asks,
  spawns `ptyrc-box`. `driver/own.rs`: an app sees its own objects; a
  band per store app, behind unless `front` is held; 256 sprites, 16
  widgets, 32 subscriptions; `surface` enforced by clipping.
- `veil.rs`: ctrl+t `h`: hide all, show who drew what, restore.
- Keys and pointer told apart: own widget by default; elsewhere needs
  `keys-read`, `pointer`, `clicks`, `clicks-grab`.
- Reserved looks lowered to plain for narrowed apps.
- `content.grid`, `draw.update {rows}`; a dirty-row `Composer`; a cap
  of 2^20 cells a canvas.
- Glyph policy (`ptyrc_vt::drawn`): controls to a space, hidden
  characters dropped; `glyph_tests.rs`.

**Runner (`rust/ptyrc-box`)**: wasmtime 49 behind `engine.rs`; ABI in
its README; `headless` (real scene, tape, `--screen`, `--deny`,
`--key`, `--event`); `live`; `--script`; `profile.rs`.

**Store (`rust/ptyrc-store`)**: manifest, origin, hardened fetch (64
MiB bound), check, install, lock, `StoreLock`, registry choice, sync,
index, page, PR review. `BUILD-PROFILE.md`.

**SDK (`sdk/`)**: `rust/ptyrc-app`; `js/interp` (Boa 0.22, 6.2 MiB),
`ptyrc.d.ts`; `template/` with `PROTOCOL.md`; examples.

**Native apps**: each declares its permissions in `_catalog.py`
(`ASKS`), held to it by `test_reads.py`; started narrowed.

**`registry/`**: skeleton, `ci/check.sh`, `ci/merge.sh`.

## Measures

| | |
|---|---|
| 200x60 repaint, one colour, driver | 1.67–1.78 ms (bound: 2) |
| the same, a colour per cell | 3.0–3.3 ms |
| script: 12000 `set` in a tick | 63 ms (limit: 50) |
| script: first start in a process | 1.0 s |
| proof 1 (`bounce`) | passed, 9 iterations |
| proof 2 (`marquee`), after `PROTOCOL.md` | passed, 5 iterations |

## Tests

- Rust workspace: green at the last full run (mid-session). Since
  then each agent ran its own crates.
- `uv run pytest`: 1170 passed (last agent's run).
- The failures seen during the day were compared to `2d0ccb7`: none
  is a regression. Fixed: `test_animation_and_ttl`, `test_life_all`,
  `test_stars`, `test_flock`. Open: `test_pace::test_live` (the app's
  heuristic, under load); the flicker check of `scripts/e2e-render.sh`
  (the checker is wrong, on both trees).
- `ptyrc-box` hostile timing tests fail on a loaded machine; they pass
  alone.
- Not run by anyone today: `scripts/e2e-*.sh` but `e2e-render`.

## Where the work departs from the plan

- Boa, not QuickJS: no libc for wasm here. 6.2 MiB, slow per cell.
- The check is in `grants.rs`, called from `driver.rs`; not `rpc.rs`.
- `<origin>` on disk: one percent-encoded name.
- `rain`, `snow`, `sand` (Python) declare `layout`, not none.
- `unsafe` in `sdk/js/interp` and `sdk/rust/ptyrc-app` (the FFI).
- `--deny` copies the lowering rules into `rust/ptyrc-box/src/grants.rs`;
  a test pins the copy.
- `update` holds any drop of tier unless `--latest`.

## Accidents

- `e0f6bab`: bears a `ptyrc-core` message, holds `ptyrc-box` work of
  another agent too. Content right, history kept.
- `0cdb7c2`: took another agent's paragraph of `plan/design.md` early.
- The pre-commit hook stashes unstaged files about a second a commit.
- Load reached 75: too many builds at once.

## Last runner slice

Cut by the checkpoint (`4281fd1`, `13a87ea`): seven items of ten
done, two partly (`ptyrc:manifest`, `[limits]`: the runner side only),
one not started. Detail in `handover.md`.

## Final check of the tree

Run by the pilot after every agent had concluded, 20:41, load 9:

- `cargo build --workspace`, and `--release -p ptyrc-cli -p
  ptyrc-box`: built.
- `PTYRC_REQUIRE_JS=1 cargo test --workspace --no-fail-fast`: no
  failure (script tests run, not skipped).
- `uv run pytest`: 1170 passed.
- `cargo clippy --workspace --all-targets`: one warning, in
  `ptyrc-box` ("very complex type"). To fix.
- `git status`: the root `Cargo.lock` alone. 85 commits since
  `2d0ccb7`.
- Not run: `scripts/e2e-*.sh`; `sdk/rust/tests` (its own workspace).
