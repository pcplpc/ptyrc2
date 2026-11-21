# Handover: prompt for the next pilot session

You are the pilot of a team of agents in
`/home/peon/git/perso/ptyrc/ptyrc2`, continuing the work on
permissions and the app store of ptyrc2. A first session ran on
2026-11-19 and stopped at a checkpoint asked by the owner.

## Read, in this order

1. `AGENTS.md` (rules; they bind you and every agent).
2. `plan/sessions/19112026-01-wrap-up/report.md` (what exists),
   `questions.md` (what the owner must answer: ask whether they have),
   `todolist.md` (what is left, in order).
3. `plan/store-model.md` (the plan; long: read it once, it is what
   every brief points to). `plan/design.md` is the contract: grep it,
   do not read it whole.
4. The READMEs written for agents: `rust/ptyrc-box/README.md` (ABI),
   `rust/ptyrc-store/README.md`, `sdk/template/PROTOCOL.md`,
   `sdk/js/README.md`, `sdk/rust/ptyrc-app/README.md`,
   `rust/ptyrc-store/BUILD-PROFILE.md`.

## First acts

- Check the tree: `git status`, `cargo build --workspace`, `cargo
  test --workspace --no-fail-fast`, `cargo clippy --workspace
  --all-targets`, `uv run pytest`. Build the interpreter first
  (`cargo build --release --manifest-path sdk/js/interp/Cargo.toml`)
  and set `PTYRC_REQUIRE_JS=1`, else the script tests skip.
- The last runner slice was cut by the checkpoint. Its ten items:
  styles in the `dev test` report; `screen.find`/`wait` headless;
  trailing empty rows of `screen.shape`; `ptyrc:manifest` in `init`;
  the wording of `not declared`; `--screen` wrapping; `[limits]`
  applied with `limit-*`; headless `front` as the driver, an honest
  template clock; several `--screen T:FILE`; `unsubscribe` out of the
  box README. Read `git log` on `rust/ptyrc-box`, `rust/ptyrc-cli`,
  `sdk/` to see which landed (its report, if it came, is appended
  below).
- Then `todolist.md` from the top. The largest piece left is the host
  functions (clock, storage, assets, result, files, probes), then
  Migration.

## How the first session worked, and what it learned

- Budget: about 250k tokens an agent session, the pilot too. Opus
  agents with a written brief for nearly everything; a Fable fork only
  where the slice shapes the design (it inherits the pilot's context:
  fork early, while the pilot is light). Never resume an agent near
  its cap: a fresh one with a narrow brief.
- A brief that worked: what to read first (files, sections); the
  slice as numbered items; what is NOT theirs; who else is in the
  tree and where; the git rules; the final report wanted, with a word
  limit and "choices for the owner to confirm".
- Three or four agents at once, not six: the load reached 75 and the
  timing tests failed everywhere. Tell agents to build with `-p`.
- One agent at a time in `driver.rs`/`grants.rs` when possible; two
  collided twice.
- Git, in every brief:
  - no trailers, no session links (`AGENTS.md` overrides the default
    attribution); terse messages;
  - never `git stash`, `git add -A`, `git checkout .`; never the root
    `Cargo.lock`, `uv.lock`, root files;
  - stage and commit in ONE command: a file left staged was taken by
    another agent's plain `git commit` (`e0f6bab`);
  - a shared file with another's hunks (`plan/design.md` above all):
    `git hash-object -w` + `git update-index --cacheinfo`, then plain
    `git commit`; `git add` on it takes the other's hunks (`0cdb7c2`);
  - format only one's own files (`rustfmt --edition 2024 FILE`);
    `cargo fmt -p` reformats the others' uncommitted work;
  - the pre-commit hook stashes unstaged files about a second.
- A flaky test: compare with a worktree at the base commit (`git
  worktree add temp/wt-base COMMIT`, its own `CARGO_TARGET_DIR`)
  before believing a regression.
- The proof (an agent confined to a `ptyrc dev new` directory writes
  an app) is cheap, 50k tokens, and finds what the docs lack: run it
  again after each change of the SDK or the tools.
- Report to the owner at each agent's return: what landed, what was
  verified and by whom, what departs from the plan.
- When something breaks the plan or is the owner's to choose, do not
  decide in silence: take the modest path, write the question down.

## Report of the last runner slice

Commits `4281fd1` (ptyrc-box), `13a87ea` (ptyrc-cli, template, docs).
Stopped at the checkpoint; nothing left staged. The agent did not
rerun the full suites after its last fixes.

Done: 1 styles (`--styles`, `styles` in JSON); 2 headless serves
`screen.find`, `screen.wait`, `screen.cursor`, `frame.list/get/at/idle`,
lowered as the driver; 5 wording (`refused:` / `lowered:`); 6 the
`--screen` wrap noted; 8 headless front as the driver, the template
clocks behind; 9 `--screen T:FILE` repeated; 10 `unsubscribe` out.

Partly:
- 4 `ptyrc:manifest`: the runner takes `--manifest JSON` and `dev test`
  passes it. Left: the driver passing it (`rust/ptyrc-server/src/store.rs`,
  and one line near `driver.rs:2246`), `manifest()` in the JS prelude
  (a native, the interpreter rebuilt) and in the Rust SDK, the docs.
- 7 `[limits]`: the runner applies them up to fixed ceilings when
  `limit-memory` / `limit-time` is held; `dev test --deny limit-memory`.
  Left: the driver passing them at spawn; the ceiling error at `dev
  check`; the Limits text of `PROTOCOL.md`.

Not started: 3 (`shape.rs` keeps every row; only the report trims
trailing empty rows: check, document).

Found on the way:
- `ptyrc_store::manifest::PERMISSIONS` lacks `limit-*`, and the
  server's `declared()` ignores `[limits]`: the pop-up never asks for
  a limit.
- A store app restarted keeps its old argv: the granted list and the
  limits are stale until a fresh start.
- Headless does not serve `screen.cells`, `frame.text`, `frame.heat`.
