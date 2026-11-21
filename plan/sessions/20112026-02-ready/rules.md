# Rules for every agent of this session (2026-11-20, second)

The rules of `plan/sessions/20112026-01-store/rules.md` bind you: read it after
`AGENTS.md`. What changes here:

- Scratch files: `plan/sessions/20112026-02-ready/<your-name>/`.
- The state you start from: `plan/sessions/20112026-01-store/handover.md`.
  The contract of what exists: `plan/sessions/20112026-01-store/design/`
  (`host-functions.md`, `readiness.md`, `deck-api.md`); keep its
  `[as built]` lines true when you change what they state.
- Goal of the session, given by the owner: everything ready, and of
  good quality, to migrate the 26 remaining Python apps to the store
  model. The migration itself is not done now.
- The root `Cargo.lock` is committed. Never commit it; if your work
  changes it (a new crate, a dependency), say so in your report.
- Budget: aim at 150k to 200k tokens; 250k is the cap. Your final
  check and your report must fit under it: stop adding work at 200k.
  What does not fit goes in the report as "left".
- A pilot's full test run is in progress when you start (some ten
  minutes): read first. Build with `-p CRATE`. Script tests need
  `PTYRC_REQUIRE_JS=1` (the interpreter is built: rebuild it with
  `sdk/js/interp/build.sh` only if you change `sdk/js/interp`).
- A test red by another agent's uncommitted work is not yours: name
  it in the report, do not fix their files.
- Do not wait on long background runs: a check that takes over ten
  minutes is cut to the crates you touched.
