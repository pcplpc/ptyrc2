# Follow-ups

Everything left open by the sessions of 2026-11-20 to 2026-11-23,
one list. Sources: the handoffs
`plan/handsoff/20261120T174339Z-next-steps.md`,
`20261120T233632Z-next-steps.md`, `20261122T051242Z-next-steps.md`,
and the agents' reports of 2026-11-23 (scratch:
`plan/sessions/23112026-01-polish/`, ignored by git). Decisions:
`plan/interview.md`. Ideas, not follow-ups: `plan/ideas.md`.

Taken 2026-11-23. "Checked": seen still true in the tree that day;
otherwise carried from its source as written.

## For the owner to decide

1. **The Python twins.** Kept for now (2026-11-23); `ptyrc app NAME`
   starts the twin, the store app is `ptyrc app ptyrc/NAME`. Remove
   all, app by app, or flip the default first. What removal consists
   of: `plan/sessions/22112026-01-migration/twins-removal.md`.
2. **`Cargo.lock`**: modified, not committed (`ptyrc-store` depends
   on `regex` since `1664143`). The owner's file.
3. **The pre-commit hook stashes unstaged files** during every
   commit and restores them after. With several agents in the tree,
   one's uncommitted file can vanish during another's commit (seen
   once, `bricks/game.js`, restored from a copy). Agents keep a
   scratch copy; the durable fix is in the hook's configuration.
4. **A tint with no layer** goes behind the text and shows its `bg`
   alone (the band rule of `plan/design.md`). In front by default
   when the app holds `front`? `marks` asks `layer: "front"` since
   `620d946`.
5. **`draw.list` below `shape`** shows the rewritten anchor
   (`space: "view"`), and for a negative anchor `at + origin`: the
   scroll origin told to an app that should not know it. The fix
   keeps each sprite's asked anchor in driver and headless: not
   small.
6. **`minimap` at scale.** A frame keeps 50,000 lines
   (`FrameConfig::max_lines`), not 100k. At 50k a draw is 38 ms
   (68 ms the first), and the `frame.text` answer (2.1 MB) about
   113 ms from a debug server (release not measured): every read
   tick late. Raise `max_lines`, keep, or make the read cheaper.
7. **`bricks` `test_twin_two_rules_bonuses`**: 3.7 to 4.1 s of 10.
   Under 2 s needs a search reused across steps or a split on a
   saved state not checked against the twin. Accept, or go on.
8. **A dusk timing check for `window`.** `--timing` never goes in a
   case, a unit test has no clock: nothing automatic catches the
   regression fixed in `7d57996`, `2005dbe`. Manual:
   `plan/sessions/23112026-01-polish/window/measure.sh`.
9. **An app holding `layout`** that shows a widget without naming a
   frame gets its surface's frame since `d40e8fc` (was raw). No
   shipped app holds `layout`. Confirm.
10. **`--probe`** is one file a run. Many values a run:
    `--probe NAME=[T:]FILE` repeated, the latest at or before `now`,
    the CLI, the docs, a case for `git` or `monitor`; about 40 lines.
11. **`file.list`** leaves out names outside `[A-Za-z0-9._-]`
    without notice: documented (`e4eb3c7`). A count of the names
    left out would change the protocol in four places.
12. **The session's scratch scripts** under `plan/sessions/`: 113
    `ruff` findings when named; `ruff check` skips them since the
    directory is ignored. Leave, or fix.
13. **`origin`**: `git@github.com:pcplpc/ptyrc2.git` in
    `.git/config` on 2026-11-23, where the memory note of
    2026-11-22 expects the `https` form. The owner's change,
    presumably; the note to be corrected if so.
14. **GitHub**: whether the old `fork/*` branches and
    `backup-before-identity` were deleted on `origin` is not known
    from here (`git ls-remote --heads origin`).

15. **Several widgets, a report off all of them** (a widget app
    below `layout`, `11dfb01`): told from the corner of the first
    placed widget in `widget.list` order. Keep, or tell none unless
    exactly one widget is placed.
16. **`size`**, the screen's, is still told to a widget app below
    `layout`. With what `pointer` reports it narrows where the
    widget is. Keep telling it?
17. **`frame.list` lists a closed pane's frame as `inactive`**
    after the pane is gone (seen live, 2026-11-23). Intended?

## Left to do, decided or needing no decision

18. **The `--check` scripts** (`demo-apps.sh`, `demo-frames.sh`,
    `e2e-manage.sh`, `e2e-tmux-attach.sh`) still run the Python
    twins. With item 1.
19. **`window/app.js`** wraps the colour lookup in its own `Map`
    (`NEAREST`, 65,536 entries) over the SDK's `xterm`, now cached
    itself (`ba42fd3`). Dropping it: measure first. Checked.
20. **Cases and unit tests go red under load**: wall-clock limits
    (`limit met: late`, `paused`, the 10 s of a unit test). Seen
    again on 2026-11-23 at loads 47 to 180: `gallery/tests/wall`,
    `tank` `test_bounds`, `window` `test_twin_14`, `ask` and `pace`
    unit runs; all green alone. Since 2026-11-20: keep the limits
    out of what a case compares, or count them without the wall
    clock. Until then `store_apps` runs alone.
21. **No live run** yet for the store `rain`, `snow`, `flower`,
    `fire`.
22. **No `tank` case on a split layout**: headless has one pane.
    The driver test
    `a_widget_is_placed_on_its_frame_told_nothing_of_it` covers the
    placement.
23. **`plan/design.md` and the Python README** describe the twins'
    `marks` ("every pane, else raw") and `ask`. With item 1.
24. **`bricks`**: the twin's tests not ported one by one (six twin
    games instead). A racket under a falling brick picks the
    leftmost of equally safe places (the twin: set order): the game
    stops matching the twin's from there; not a defect to a player.
25. **`dev test` has no wheel flag** (`--event T:mouse:{...}`
    serves). Checked.
26. **`rust/ptyrc-store/src/tree.rs`** bounds `.js` at 1 MiB, not
    `.mjs`. Checked.
27. **The note of a widget with no room** gives the screen's size,
    not the largest open room: `rooms` of `ptyrc-core` is
    `pub(crate)`. Checked.
28. **A widget stays cut off 1 s after a resize**, headless and
    most likely live. Not checked live.
29. **The doc pass** of `/home/peon/CLAUDE.md` over the code and
    READMEs written since 2026-11-20; the 12 stale items listed on
    2026-11-20 (`plan-docs/stale.md`, no longer in the tree) not
    re-checked.
30. **The proof a third time** (the brief of `proof-2`, session of
    2026-11-20), not run since the SDK round and the migration.
31. **Agents over budget**: 258k, 258k of a 220k stop on
    2026-11-23; 267k, 314k, 259k of 250k on 2026-11-22. Narrower
    briefs, or a lower stop.
32. **A commit wider than its message**: `c71f942` (`marks`) holds
    the guide's line on integer keys, another agent's. Content
    right. As `12989df` before it.

## Not found

33. **A dead duplicate line in the SDK's `Deck::leave`** (handoff of
    2026-11-22): none in `deck.rs`, `deck.js` or their history.

## Closed on 2026-11-23, for the record

| what | where |
|---|---|
| `window` at dusk: worst tick 56 to 67 ms, now 12.5 to 14.7 ms | `7d57996`, `2005dbe`, `654e877` |
| `xterm` in both SDKs: pruned search, the same colour each of the 2^24 | `ba42fd3`, `2433712` |
| The interpreter's `Map`/`Set` hash patched by `build.sh`; a test; the guide | `4af6225` |
| `fishes` first tick at 200x60: 98 to 25 ms; `flower -a`: 34 to 22 ms | `4fedfea`, `0785450`, `859e78a`, `471ce21` |
| `bricks`: the test 5.6 to 3.7 s | `aa3f1d8`, `eefff50`, `020dc6c` |
| `timer --modal` live; denied, no crash | `853fa00` |
| `sand`, `confetti` live on a pane away from the origin | no change needed |
| A bad regex refused by the launcher | `1664143`, `bf5cff5` |
| Headless calls `stop` | `cac698a` |
| `marks`: tints in front; `frame = "active"` | `620d946`, `c71f942` |
| A widget placed on `[app] frame` below `layout`, told nothing of it | `d40e8fc` |
| `ask`: 15 options said, a 16th refused in a case | `9858cf8` |
| `pace` trims its oldest words before the cap | `e0b6555` |
| `ruff check` clean (`plan/sessions/` ignored) | `cc3b7d8` |
| A widget app below `layout` hears every `hover`, `mouse` in its widget's cells; none placed, none told | `11dfb01`, `08b138f`, `4fd6e39` |
| The widget apps live on three panes: placed, pointer right, after a pane closed too; no app changed | transcripts: `plan/sessions/23112026-01-polish/apps/` |
| Stale branches `backup/pre-trailer-strip`, `side`; worktree `temp/squash-wt` | removed |

## Of note

Eleven of the twelve widget apps name `frame = "raw"`, as their
twins: the placement on `[app] frame` shows for `tank` alone, or
under `--frame`.
