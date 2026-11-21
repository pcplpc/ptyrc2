# Work left

In the order I would take it. Sizes: S under an hour of an agent, M one
agent session, L several.

## First: check the checkpoint

- [x] `cargo build --workspace`, `cargo test --workspace`, `uv run
      pytest`: green at the checkpoint (`report.md`, Final check).
- [ ] S One clippy warning in `ptyrc-box` ("very complex type").
- [ ] S Run `scripts/e2e-*.sh` and `sdk/rust/tests`: not run at the
      checkpoint.
- [ ] S The last runner slice (ten items, `handover.md`): see what
      landed, finish the rest.
- [ ] S `sdk/template/PROTOCOL.md` says `surface` is "not enforced
      yet": it is. Also the pointer rules changed (own widget by
      default).

- [ ] M `[limits]` end to end: `limit-*` in
      `ptyrc_store::manifest::PERMISSIONS`; the server's `declared()`
      reads `[limits]` so the pop-up asks; the driver passes the
      granted limits and `ptyrc:manifest` at spawn; `manifest()` in
      both SDKs; the ceiling error at `dev check`.
- [ ] S A store app restarted keeps its old argv: grants and limits
      stale until a fresh start.

## Holes known

- [ ] S `screen.text {sprites: true}` answers every app's sprites
      when `text` is held (principle 5).
- [ ] S `draw.list` on an app's own sprites tells where an anchor on
      text landed, and the `NOWHERE` frame id.
- [ ] S Live, a store app calling an unknown method gets `denied`,
      not `unknown-method`.
- [ ] S `ptyrc app ls`, `ptyrc app log` print names and logs raw on
      stdout (the audit).
- [ ] M Driver-emitted sequences to close per permission:
      `session.colors {ask}` (OSC 10/11), mouse modes on subscribe,
      the bell on ctrl+t ctrl+t, `input.send {paste}` ending the
      bracketed paste.
- [ ] S Regional-indicator pairs and emoji modifiers merging across
      cells on a host with mode 2027.
- [ ] S The hotkey starts `manage`, which declares nothing.

## Permissions, task 6: the rest

- [ ] L Host functions, each with its permission: the wall clock and
      a zone (`clock`); `storage` (through the driver: the runner
      holds no file after its profile); `assets`; a result to the
      caller; named files (`files`, `files-write`); `probe-git`,
      `probe-system`. Today these permissions open nothing.
- [ ] M The generic options of the launcher (principle 4): `--front`,
      `--frame`, `--prefer`, `--alpha` out of the apps.
- [ ] M Primitives that use without reading (principle 2): an anchor
      on a word, on the cursor; a blend by heat; gating by idle. Then
      lower `flower`, `confetti`, `fishes`, `stars`, `pond`, `shore`,
      `minimap`, `marks` ("To lower further").
- [ ] S `permissions.toml` on `store rm --purge`: done; check the
      `manage` entry for store apps shows tier and commit.
- [ ] M One copy of the lowering rules: move `grants.rs` and
      `lower.rs` to `ptyrc-core` or a small crate; `ptyrc-box` and the
      server use it. The colour table: the server takes
      `ptyrc_store::manifest::COLOURS`.

## Runner and tools

- [ ] M The on-disk cache of the compiled interpreter (wants
      `unsafe`: question 5).
- [ ] S Resume of a paused app; the manager told ("paused, named in
      the manager").
- [ ] S `wasmi` where wasmtime does not build.
- [ ] S `hello` and the `size` event written down in the box README.
- [ ] S `scripts/e2e-render.sh`: the flicker checker's model is wrong
      (negative `wasted`).
- [ ] S `test_pace::test_live`: the app's word count under load.

## Store

- [ ] L Rust apps from the store: the build under the profile
      (`rust/ptyrc-store/BUILD-PROFILE.md`), the licences of the
      crates at `check`, the bounds (300 s, 4 GiB, 16 MiB).
- [ ] M Registry: hosting, a CI file, `author` and `note` in a
      pointer, the tape played on the page.
- [ ] S `store update` at restart: a restart reuses the old tree.
- [ ] S Native apps named `native` in the menu.

## Migration (milestone 7)

- [ ] L By waves, each Python app a store app, its tests carried
      over through `dev test`: the first row; the pointer and the
      games; the clocks; `ask`; `pace`; named files; the probes.
      `plugins/store-apps/` does not exist yet.

## Documents

- [ ] M `plan/store-model.md`: mark what is done; the tables of apps
      (`rain`, `snow`, `sand`: `layout`); the decisions taken.
- [ ] S `plan/design.md`: the glyph paragraph of the audit (in
      `report` of that agent; text below).
- [ ] S Root `README.md`: the store, `dev`, "the store is not yet
      safe for strangers' apps" (the plan asks for it). Root file:
      the owner's, or allowed once.
- [ ] S The doc pass of `/home/peon/CLAUDE.md` over the new code.

The glyph paragraph:

> Text from a client reaches the host as glyphs only. Where a
> character enters a cell (`Canvas::draw`, the widget lay-out, the
> menu), a control character (C0, DEL, C1) is a space and a hidden one
> (`ptyrc_vt::is_hidden`: format characters, line and paragraph
> separators, variation selectors 15 and 16) is dropped; widths are
> counted after. The painter prints no control nor hidden character
> whatever a cell holds; its own sequences are CSI, G0 and the app's
> OSC 8. Border glyphs and layer names are refused when not plain.
> `glyph_tests.rs` is the regression suite.
