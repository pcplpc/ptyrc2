# Rules for every agent of this session (2026-11-20)

Repository: `/home/peon/git/perso/ptyrc/ptyrc2`. Work only inside it.
Scratch files: `plan/sessions/20112026-01-store/<your-name>/`. Never remove
anything outside the repository.

Read `AGENTS.md` first: it binds you. Then what your brief names.
`plan/store-model.md` is the plan (long; read the sections your brief
names). `plan/design.md` is the contract: grep it, do not read it whole.
The owner's decisions: `plan/sessions/19112026-01-wrap-up/answers.md`.

## Code

- Never write the name of a coding assistant in code, tests or docs:
  `agent` / `Agent` / `AGENT`.
- SPDX header on every new source file. No `unsafe` outside
  `ptyrc-server` (and what already has it). No async runtime.
- Docs: exact and minimal, plain, sober, precise; no filler. Where you
  change behaviour, change the README / `plan/design.md` paragraph that
  states it, in the same style.
- Tests with every change. A flaky test: compare with a worktree at
  your base commit (`git worktree add plan/sessions/20112026-01-store/<you>/wt
  COMMIT`, its own `CARGO_TARGET_DIR`) before calling it a regression.

## Builds

Other agents build in this tree at the same time. Build and test with
`-p CRATE`, never the whole workspace until your final check. Script
tests need the interpreter (`sdk/js/interp/build.sh`) and `PTYRC_REQUIRE_JS=1`, else they skip.
Timing tests of `ptyrc-box` fail under load and pass alone.

## Git

- Small commits, terse messages in the style of `git log` (`ptyrc-box:
  headless: ...`). NO trailers, NO co-authorship, NO session links.
- Several agents share this tree, uncommitted work included.
  - Never `git stash`, `git add -A`, `git checkout .`, `git restore .`,
    never rewrite history. Never commit the root `Cargo.lock`, `uv.lock`
    or root files (`README.md`, `Cargo.toml`, ...): tell the pilot.
  - Stage and commit in ONE command line: `git add PATHS && git commit
    -m "..." -- PATHS`. Nothing left staged.
  - A shared file holding another agent's hunks (`plan/design.md` above
    all): `git hash-object -w` + `git update-index --cacheinfo` for your
    version, then plain `git commit`. `git add` would take theirs.
  - Format only your own files: `rustfmt --edition 2024 FILE`. Not
    `cargo fmt`.
  - Retry if the index is locked.
- Commit as you go: a slice that works is committed before the next.

## When unsure

What breaks the plan, or is the owner's to choose: take the modest
path, and write the question in your report. Do not decide in silence.

## Final report

Under 400 words: commits; what was verified and how (commands, counts);
what is left; what departs from the plan; "choices for the owner to
confirm". Facts only: a test not run is said not run.
