# Rules for every agent of this session (2026-11-20, third: the preparation)

The rules of `plan/sessions/20112026-01-store/rules.md` and
`plan/sessions/20112026-02-ready/rules.md` bind you: read them after
`AGENTS.md`. What changes here:

- Scratch files: `plan/sessions/20112026-03-prep/<your-name>/`.
- The state you start from:
  `plan/handsoff/20261120T174339Z-next-steps.md`. The owner's answers
  to its questions: the last section of `plan/interview.md`. They are
  decided: do not reopen them.
- Git, replacing the rule of the first file: `git add PATHS`, then
  `git diff --cached --name-only` must show your paths alone (another
  agent's file staged: do not commit, wait and look again, then tell
  the pilot), then plain `git commit -m "..."`. Never
  `git commit -- PATHS`. A shared file holding another agent's hunks:
  `git hash-object -w` + `git update-index --cacheinfo`, as before.
  The pre-commit hook stashes unstaged files: commit each slice at
  once, keep nothing uncommitted across another agent's commit.
- Two agents work at once, no more. Build with `-p CRATE`. Tell the
  pilot nothing before your final check; make it on the crates you
  touched.
- Disk: `df -h /` before a large build; under 40 GB free, stop and
  say so.
- Budget: aim at 150k to 200k tokens; 250k is the cap.
- You may not write a report file: the report is your last message.
