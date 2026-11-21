# One copy of the rules

Where: a new crate `rust/ptyrc-rules` (lib only). Dependencies:
`serde_json`, `ptyrc-proto` (`RpcError`), `ptyrc-vt` (`char_width`),
`ptyrc-core` (`FrameId`, `RAW`): all four already under `ptyrc-box` and
`ptyrc-server`; nothing new in the box. Not `ptyrc-core`: it names no
method of the protocol today, and the rules are the protocol's.

Pure functions over plain data: no I/O, no clock (a press's time is a
`Duration` argument), no state but `Nozzles`, a table the caller feeds.

- `grants`: `Degree`, `Permission`, `Hand`, `Grants`, `Gate`, `Verdict`,
  `Why`, `METHODS`, `TOPICS`, `gate`, `check`, `check_topic`, `ladder`.
- `lower`: `Plan`, `Cut`, `plan`, `found`, `shown`, `mask`, `frame_info`,
  `event`, `scrub`, `hello`, `landing`, `NOWHERE`, `composes_own`.
- `nozzle`: `take_flags`, `Nozzles<C>` (`C`: the driver's `ConnId`, the
  box's `()`), `Click`.

The driver keeps its words (`denied`, `Identity`, `Surface`) and its
plumbing; `grants.rs`, `lower.rs`, `driver/nozzle.rs` re-export. The box
keeps its report (`lacks`, the manifest line in hints) and its adapters
(`grants.rs`: names as strings, `declared`; `nozzle.rs`: its API).

## Differences found, and what was kept

The driver's behaviour kept everywhere; the box changed where it differed.

1. Box `screen.find` without `pattern`, below `text`: answered whole (a
   leak). Now `{"matches": []}`, as the driver.
2. Box `frame.at` at `layout` and above: marked `"degree"`. Now
   unmarked (`Pass`), as the driver. Box `frame.list` there: asked with
   the app's params; now `{}`. `frame.get`: only `frame`.
3. Box `screen.wait {pattern}` of a held word: `timeout_ms` was dropped,
   `gone` passed to `screen.find` too; now the driver's members.
4. Box `pattern: null`: taken as a pattern; now as none (`has`).
5. Box errors `no-frame`, `no-sprite` below `text`: kept their hint;
   now scrubbed (`lower::scrub`).
6. Box gate of a method not listed (`asset.*`, unknown): `Open`; the
   driver's: denied `Unlisted`. Kept per side (`gate` gives `None`; the
   box opens it: `asset.*` is the runner's, unknown is refused later).
7. Box landing: frame 0 below `layout`, never `NOWHERE` mapped; now the
   driver's with `surface` `RAW` (the box has no `NOWHERE` frame: same
   answers).
8. Box `Grants`: names as strings, unknown names held. Now `Permission`;
   a name that is no permission is ignored (`main.rs` checks names).
9. Box nozzle: no drag, release by `press: false`, ms as `u64`; now the
   driver's `Duration`, `takes` always true (the box reports the lack).
10. Hints of `denied`: kept per side (the box names the manifest line).
11. `screen.shape`: the box's engine answers the whole screen; `frame`,
   `rect` asked by the plan are ignored there. Not a rule: left.

Tests changed: `ptyrc-box` `tests/protocol.rs` (the source greps of the
driver's tables, of `own.rs`, `nozzle.rs`: gone; the tables read from
`ptyrc_rules`); `grants.rs` of the box (`holds_word` → `lower::shown`;
its landing test, in `ptyrc-rules` now); `python/ptyrc/tests/
test_degrees.py` (reads `ptyrc-rules/src/grants.rs`). The driver's
behaviour tests: unchanged.
