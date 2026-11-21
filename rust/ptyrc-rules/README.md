# ptyrc-rules

The rules of an app's requests, one copy for the driver (`ptyrc-server`)
and the headless runner (`ptyrc-box`). Pure: no I/O, no clock (time is
an argument). `plan/design.md`, "Permissions".

- `grants`: the permissions, the degrees, `Grants`, the table of what
  opens each method and topic (`METHODS`, `TOPICS`, `gate`), the check
  (`check`, `check_topic`), the ladder.
- `lower`: an app below `text`: what to ask in place of a request
  (`plan`), what to make of the answer (`Cut::apply`), the words found
  and shown, the frame infos, events and `hello` cut, `landing` (where
  an anchor landed, in `draw.list`), `scrub` (hints that name frames
  or sprites); `composes_own` (`screen.text`, `screen.cells`: its own
  sprites alone); `NOWHERE`, the frame of a sprite shown nowhere.
- `nozzle`: the pointer told to an effect: the flags taken out
  (`take_flags`), the tunes queued (`Nozzles<C>`, `C` the owner).
- `surface`: `[app] surface` on its frame (`Surface`), the frame a
  draw is put on (`clip`): a bare sprite of `widget` on `NOWHERE`.

Each side keeps its own words and plumbing: the driver its errors,
identities, surfaces, and the sending of the tunes; the box its report
(`lacks`, the line of the manifest in a hint).
