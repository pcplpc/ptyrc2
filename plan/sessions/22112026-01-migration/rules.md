# Rules for every agent of this session (2026-11-22: the migration)

The rules of `plan/sessions/20112026-01-store/rules.md`,
`plan/sessions/20112026-02-ready/rules.md` and
`plan/sessions/20112026-03-prep/rules.md` bind you: read them after
`AGENTS.md`. What changes here:

- Scratch files: `plan/sessions/22112026-01-migration/<your-name>/`
  (commit nothing there but what a later agent needs).
- The state you start from:
  `plan/handsoff/20261120T233632Z-next-steps.md`; the owner's answers:
  `plan/interview.md`, "Before the migration" and "Clean slate". They
  are decided: do not reopen them.
- Your guide: `plugins/store-apps/README.md`. Your row and the
  findings: `plan/sessions/20112026-02-ready/audit/migration-plan.md`.
  Your draft manifest:
  `plan/sessions/20112026-02-ready/manifest/drafts/NAME/`.
- Your ground: `plugins/store-apps/NAME/` for the apps of your brief,
  and what the brief names beside. Anything else you find wrong (the
  SDK, the runner, the guide): fix it if it is small and blocks you,
  in a commit of its own made at once; otherwise say it in your
  report.
- The Python twin stays: do not remove or edit it. Do not edit
  `_catalog.py`, `test_reads.py`, the scripts (the wave's end).
- Done, for an app: its look and behaviour those of the twin at the
  twin's default frame; the twin's `seed` option dropped; the twin's
  tests carried over as unit tests and cases; `ptyrc dev check`
  clean; `dev test --cases` and `--timing` green (timing on a quiet
  machine: `/proc/loadavg` under 4); the harness
  `cargo test -p ptyrc-cli --test store_apps` green; a live run if
  the brief asks one.
- Git: as the third rules file. Small commits, one an app at least.
  No trailers. Never `git stash`, never `git add -A`.
- Two agents work at once. Build with `-p CRATE`.
- Budget: aim at 150k to 200k tokens; 250k is the cap. Short of
  budget: commit what is green, stop, and say what is left.
- The report is your last message, 30 lines at most: the commits, what
  is done per app, what differs from the twin, what is left, what the
  next agent of your family must know.
