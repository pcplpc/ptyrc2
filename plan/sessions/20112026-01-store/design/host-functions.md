# Host functions: design, 2026-11-20

For implementers. Follows `plan/store-model.md` (Migration,
Permissions, Never) and `readiness.md` (the gaps, by number). Marks:
**[proposed]** mine; **[owner]** a question, listed at the end.

## Rules for all

- No new import: each is a method of the protocol, through `call`,
  checked by the driver in `grants.rs` (`METHODS`). `tests/protocol.rs`
  of `ptyrc-box` and `PROTOCOL.md` take the new rows.
- No path ever crosses to the guest or from it. A file is a handle the
  launcher made from what the user typed; storage and assets are
  reached by a name the host checks.
- A name (storage key, asset, child of a directory): components of
  `[A-Za-z0-9._-]`, 1 to 64 bytes each, none `.` or `..`, none
  starting with `.`; `/` between components for assets only, 4 deep at
  most. Else `bad-params`, the hint giving the rule. Opened with
  `O_NOFOLLOW | O_CLOEXEC`, a regular file or nothing.
- Bytes: `"as": "text"` (UTF-8, invalid bytes replaced) or `"base64"`.
  One answer carries 1 MiB of content at most (a JSON answer is bound
  to 4 MiB by the runner); more: `offset`, `len`.
- Served by the driver, on a worker (`hostio.rs`): the request is
  answered `Reply::Pending` and completed from the worker's channel,
  as `screen.wait` is completed later. The loop never waits on a disk
  or on `git`. One worker, requests in order; a queue of 64, then
  `not-usable` (`host busy`). Exception: assets (below).
- A connection with no identity (the user's hand): `clock.*` answered;
  the rest `not-usable`, `no app: storage belongs to an app`. A native
  app (`native/NAME`) is served as a store app is.
- Counted in the 1000 calls a second. `storage.set`, `file.write`: 16 a
  second an app, then `not-usable` (`limit` is the runner's code, not
  the driver's) **[as built, slice A]**.
- A request answered on the worker is pending: not in a `batch`
  (`bad-request`), as `screen.wait`. `clock.*` not held and
  `app.result` are answered at once, in a batch too **[as built]**.
- Headless (`ptyrc-box/src/headless.rs`) serves each from memory, the
  same answers, nothing of the machine read unless an option names it.
  The report gains sections: `result`, `storage`, `files` (written).

| method | permission | not held |
|---|---|---|
| `clock.now`, `clock.zone` | `clock` (plain, asked) | session time as the epoch, UTC; `"degree": "none"` |
| `storage.get`, `.set`, `.remove`, `.list` | `storage` (plain, asked) | `denied` |
| `asset.read`, `asset.list` | none | |
| `app.result` | none | |
| `file.read`, `.stat`, `.list` | a handle given by the user | `no-file` |
| `file.write` | the handle, and `files-write` (red) | `denied` |
| `probe.git` | `probe-git` (plain) | `{"repo": null, "degree": "none"}` |
| `probe.system` | `probe-system` (plain) | every part empty, `"degree": "none"` |

`grants.rs`: `Gate::Needs` for `storage.*`, `file.write`; a new
`Gate::Lowered(Permission)` (answered, cut, never `denied`) for
`clock.*`, `probe.*`; `Open` for `asset.*`, `app.result`, `file.read`,
`file.stat`, `file.list` (the handle is the check). New error code:
`no-file`.

## Clock

```
clock.now {}
  -> {"unix_ms": 1795867200123, "zone": "Europe/Paris",
      "offset_s": 3600, "abbr": "CET", "next": 1806195600000}
clock.zone {"zone": "Asia/Tokyo" | "local", "at": [unix_ms, ...]}   # 1..64 instants; absent: now
  -> {"zone": "Asia/Tokyo",
      "offsets": [{"at": ..., "offset_s": 32400, "abbr": "JST", "next": null}]}
```

- `next`: the instant the offset changes next, or null: an app asks
  again then, not every second.
- Unknown zone: `bad-params`, hint `an IANA name, as Europe/Paris;
  "local": the machine's`. A zone name: components of
  `[A-Za-z0-9._+-]` (`Etc/GMT+5`), 1 to 64 bytes, none starting with
  `.`, 3 at most; the file, links followed, must lie in the database
  **[as built]**.
- Not held: `unix_ms` is the session time of the app (0 at its start),
  zone `UTC`, offset 0, `"degree": "none"`. `clock.zone` not held:
  `{"zone": "UTC", "offsets": [{"at", "offset_s": 0, "abbr": "UTC",
  "next": null}], "degree": "none"}`, `at` absent: the session time
  **[as built]**. A clock then counts from
  midnight of 1970: wrong and plainly so, the app runs on.
- The zone database: the system's (`$TZDIR`, `/usr/share/zoneinfo`;
  local: `$TZ`, `/etc/localtime`), read by a TZif reader of ours in
  `ptyrc-server/src/zone.rs` (v2+ body, the POSIX footer for instants
  past the last transition), tested against `date`. No database: every
  zone but `UTC` and `local` (offset 0) is `not-usable`. **[owner]**
- Time is not in `ptyrc-core`: `hostio` takes the wall time from the
  server's clock (`PTYRC_CLOCK_RATE` does not scale it).
- Local time to an instant (`timer --until 17:30`): in the app, from
  `clock.now` (offset, `next`). No method.
- A tick on the second: `period(1000 - unix_ms % 1000)` once, then
  `period(1000)`. Documented, no phase argument.
- Headless: `--clock 2026-11-20T12:00:00Z` (default
  `2000-01-01T00:00:00Z`), advancing with session time; `--zone NAME`
  the local zone (default `UTC`; a named zone reads the system
  database: the same on every machine that has it). **[as built]**:
  `zone.rs` moved to `ptyrc-core` (std and `rustix` only; time an
  argument). The runner reads no file after its profile: the database
  is read whole before it, when `clock` is held (some 1800 files).
  `--zone` takes a POSIX rule too (`UTC-2`); `--clock` an offset
  (`+01:00`) and milliseconds.
- SDK. JS: `now(): {unixMs, zone, offsetS, abbr, next}`,
  `zone(name, at?)`; `Date.now()` stays session time. Rust:
  `clock::now() -> Wall`, `clock::zone(name, &[at])`, a small
  `civil(unix_ms, offset_s) -> (y, m, d, h, mi, s, weekday)` in both
  (the guest has no calendar; `Date` in a script has one).
- Tests: held and not held; an unknown zone; a DST change crossed with
  `--clock`; 65 instants refused; the hostile one: `zone` as a path
  (`../../etc/passwd`, an absolute path): `bad-params`, no file opened
  outside the database.

## Storage

A table of named values, not a directory **[proposed]**: nothing to
traverse, a quota that is a sum.

```
storage.get    {"key": "scores"}                 -> {"value": "..." | null}
storage.set    {"key": "scores", "value": "..."} -> {"used": 5123, "quota": 1048576}
storage.remove {"key": "scores"}                 -> {}
storage.list   {}                                -> {"keys": [{"key", "size"}], "used", "quota"}
```

- `value`: a string (JSON, or base64 by the app), 256 KiB at most.
  64 keys, 1 MiB an app **[owner]**. Over: `not-usable`, hint `storage:
  1048576 bytes at most, N used`. Nothing is written on error.
- On disk: `<state>/ptyrc/storage/<origin, percent-encoded as the
  apps tree>/<key>`, 0700/0600; a write is a temp file and a rename.
  A native app: `native%2FNAME`. `store rm --purge` removes it
  (`install::purge_storage`, exists).
- Belongs to the origin, not the commit: kept by an update.
- Manifest: `[permissions] storage = "why"`. Pop-up: plain, ticked.
- Headless: empty, in memory; `--storage DIR`: loaded from the files
  of DIR (a key a file), not written back; `dev test --storage-out
  DIR` writes the end there. The report: a key written, removed, or
  given and not written; `storage` not held, the storage given unused.
  `--json` the values. [as built]
- SDK. JS: `storage.get(key)`, `.set(key, value)`, `.remove(key)`,
  `.keys()`; Rust: the same in `storage::`. Values are strings.
- Tests: quota by one value, by the sum, by the count; a key `..`,
  `a/b`, `.x`, empty, 65 bytes, a NUL; two apps do not see each other
  (two origins, the same key); a native app; a symlink planted in the
  directory under a key's name is not followed; `--purge`.

## Assets

```
asset.list {}                                      -> {"assets": [{"name": "pictures/ship.pic", "size": 812}]}
asset.read {"name": "pictures/ship.pic", "as": "text", "offset": 0, "len": 1048576}
  -> {"text": "...", "size": 812, "eof": true}
```

- The tree under `[app] assets` of the manifest, as installed. No
  permission. No such asset: `no-file`, the hint the nearest name.
- **Served by the runner**, not the driver **[proposed, owner]**: it
  reads the assets (8 MiB at most: the bound of a tree) with the
  scripts, before its OS profile, and answers `asset.*` itself in
  `Core::call` before the socket. No check is skipped (none exists
  for assets), the driver gains no code, headless is the same code,
  and `dev run` restarts on a change already. The driver's table
  lists `asset.*` as `Open` and answers `not-usable` if ever reached.
- The runner takes `--assets DIR` (live and headless), read whole
  before the profile: regular files, links not followed, names outside
  the rule skipped. The launcher passes it: `dev test`, `dev run` do;
  the driver's `store.rs` (`base`) is to add `--assets TREE/ASSETS`.
  `asset.*` in a `batch`: `bad-request` (`not allowed in a batch`):
  the batch goes to the driver, not the runner **[as built]**.
- `dev check`: `assets` names a directory of the tree; names keep the
  rule; counted in the tree's bounds.
- SDK. JS: `asset(name): string`, `assetBytes(name): Uint8Array`,
  `assets(): string[]`. Rust: `asset::text`, `asset::bytes`, `asset::list`.
- Tests: `../ptyrc-app.toml`, `/etc/passwd`, `a//b`, a name of the
  tree outside `assets`; a symlink in the directory at `dev check`
  (refused by the install already); `offset` past the end.

## Result

```
app.result {"value": <any JSON>, "status": 0}   -> {}
```

- Kept by the driver with the app's entry; the last one holds; 64 KiB.
  `status`: 0..255, what the caller's command exits with (default 0;
  the runner's own failure wins).
- Told in the app's info (`app.list`, the `apps` event at its exit):
  `"result": {"value", "status"} | null` (the info's `status` is the
  exit status already) **[as built]**. Then the app calls `exit`. A
  restart clears it.
- The caller: `ptyrc app ID ... --wait` starts, waits for the exit,
  prints the value (a string as it is, a list a line each, else JSON;
  `--json`: always JSON), control characters escaped (`\u{1b}`), to a
  terminal glyphs only, and exits with the status. `--no-wait` undoes
  `wait = true`; `wait`, `no-wait` are reserved option names. Done in
  slice A **[as built]**. `[app] wait = true` in the manifest makes it the default
  (`ask`). Stopped without a result: status 1, nothing printed.
- No permission: the caller asked.
- Headless: the report's `result:` line and status.
- SDK: `result(value, status?)` in both.
- Tests: larger than 64 KiB; twice; an escape sequence in a string
  value does not reach the caller's terminal raw; the app killed.

## Files

A file is the user's hand (principle 4): named at the start, by an
option or a positional the manifest types `file` or `dir`.

```toml
[options]
stop = { type = "file", doc = "words to leave out" }
save = { type = "file", write = true, doc = "where the counts are kept" }
dir  = { type = "dir", list = true, doc = "pictures" }      # list: repeated

[args]
files = { type = "file", write = true, min = 0, max = 16 }

[permissions]
files = "the files named"            # required by a `file` or `dir` type: dev check
files-write = "a box ticked is written back"   # required by `write = true`
```

- The launcher (`ptyrc app`, `manage`, `dev run`: the process that
  has the user's working directory) makes each path absolute and
  canonical, and sends `app.start {store, args, files: [{"handle":
  "f0", "path": "/abs/TODO.md", "kind": "file" | "dir", "write":
  true}]}`. Only a connection of the user's can: `app.start` is
  `Never` for apps. The driver keeps the list with the app's entry
  (a restart keeps it); it is in no file. **[as built]** Each entry
  bears `"default": true` when it is the manifest's default; the
  driver checks each path again (canonical, a regular file or a
  directory) and takes `write` from the manifest, a client's `write`
  above it refused. `App::file(handle) -> Named {path, kind,
  writable}` (`apps.rs`), `Driver::file_of(conn, handle)`
  (`driver/own.rs`): what `file.*` calls.
- The guest gets, in `init(args)`, in place of the path: `{"file":
  "f0", "name": "TODO.md"}` (the last component, as typed). 16 handles
  a run.
- Read: granted by the naming; nothing asked, nothing remembered.
  Write: the handle is writable only when the manifest says `write`
  and `files-write` is held (red, ticked by hand); else `file.write`
  is `denied`, the app goes on reading. **[as built]** `files` and
  `files-write` of a manifest with a file option are declared for a
  run only as needed: `files` when a default file is used,
  `files-write` when a file to write is named.

```
file.stat  {"file": "f0", "name"?: "child"}  -> {"exists": true, "size": 1532, "mtime_ms": ...}
file.read  {"file": "f0", "name"?, "as", "offset", "len"}
  -> {"text" | "base64", "size", "mtime_ms", "eof"}
file.write {"file": "f0", "text" | "base64", "if_mtime_ms"?: ...}
  -> {"size", "mtime_ms"}
file.list  {"file": "d0"}  -> {"entries": [{"name": "ship.pic", "size": 812}]}
```

- `file.write`: the whole content, 1 MiB at most; a temp file beside
  it, the mode kept, a rename (as `note.py` does). `if_mtime_ms`: not
  the file's now: `not-usable`, `changed since it was read`; `note`
  ticks a box so. A file that does not exist is created (0600).
- A directory: `file.list`: its regular files, 1024 at most, names
  that keep the rule; `name` reads one. Never deeper, never written.
- A file to read: 16 MiB at most, by pieces of 1 MiB.
- An unknown handle, or a string that is not one: `no-file`. A path in
  `file`: `no-file`; the hint: `a file is named by the user at the
  start: [options] x = { type = "file" }`.
- No topic: `file.stat` once a second is cheap.
- A default path (`note`: `TODO.md` of the working directory): a
  relative, plain name in the manifest's `default`, resolved by the
  launcher; it is the app's wish, so shown in the pop-up (`read
  TODO.md of the directory it is started in`) and asked as `files`
  **[owner]**.
- Headless: `--file NAME=PATH` (NAME: the option or `ARG#N`): read
  once into memory; writes stay in memory, named in the report
  (`files: save written, 312 bytes`), the content in `--json`; the
  real file is never written. `--file NAME=DIR` for a `dir`.
  **[as built, headless]**: handles `f0`, `f1`, ... in the order
  given; `init` gets `args[NAME] = {"file", "name"}` (given twice: a
  list). `ARG#N` not yet (no `[args]`). Writable with `files-write`
  alone (the manifest's `write` read once the option types land). A
  path that does not exist: a file that does not exist (`stat`:
  `{"exists": false}`; `read`: `no-file`; `write` creates it). The
  `mtime_ms` of a write: the clock (`--clock` plus session time).
  `file.list` or `name` on a file: `bad-params`; `name` absent on a
  directory for `read`, `stat`: `bad-params`; a child absent: as a
  file absent. `file.write` on a directory: `bad-params`; `text` or
  `base64`, one; `if_mtime_ms` not the file's (or the file absent):
  `not-usable`, `file.write: changed since it was read`. `storage.set`
  and `file.write` share the 16 a second. `file.*` in a batch:
  `bad-request`, after the check (a write not held: `denied`).
- SDK. JS: `file(handle).text()`, `.bytes()`, `.stat()`,
  `.write(text, {ifMtime})`, `.list()`, `.child(name)`. Rust: `File`.
- Tests: a handle never given; another app's handle name; `name`
  `../x`, `a/b`, a symlink child (not followed); a write without
  `files-write`, without `write` in the manifest; a write past 1 MiB;
  the file replaced by a symlink between two calls (`O_NOFOLLOW` on
  the last component; the canonical parent re-checked); a FIFO named
  by the user (refused at the start: regular files and directories
  only); the list kept across `app.restart`.

## Probes

Data, not means: the driver reads the machine, the app gets the
answer. The parsers are ported from `_gitprobe.py` and `_probe.py` to
a crate with no clock and no I/O in its parsers, `rust/ptyrc-probe`
(the fixtures of `test_git.py`, `test_monitor.py` carried over); the
driver's worker runs them.

### `probe.git`

```
probe.git {"repo"?: "d0", "parts"?: [...], "log"?: 30, "since"?: "stamp"}
  -> {"repo": {...} | null, "stamp": "..."}   |   {"unchanged": true, "stamp"}
```

- `repo`: a `dir` handle (`git --repo PATH`); absent: the working
  directory of the wrapped program (`/proc/<pid>/cwd` of the session's
  child, followed when it moves), as `git.py` does. No repository
  there: `"repo": null`.
- `parts`, default all: `head`, `changes`, `log`, `stashes`,
  `branches`, `worktrees`, `days`, `state`. `log`: 1..200 commits.
- `since`: the stamp of the last answer (`_gitprobe.Watch`: mtimes of
  `HEAD`, the index, the refs); the same: `unchanged`, nothing run.
  One real read a second an app; sooner: the last answer.

| part | members (as `_gitprobe`) |
|---|---|
| `repo` | `name` (the last component of the root; no path), `now` |
| `head` | `oid`, `branch`, `upstream`, `ahead`, `behind`, `gone`, `initial` |
| `changes` | `[{kind, code, path, added, deleted}]` (`kind`: staged, unstaged, untracked, conflict; `path` in the repository), 500 at most, and `counts` |
| `log` | `[{oid, at, author, subject, refs, graph}]` |
| `stashes` | `[{n, at, subject}]` |
| `branches` | `[{name, current, at, upstream, ahead, behind, gone, subject}]` |
| `worktrees` | `[{name, oid, branch, flags, current}]` (`name`: last component) |
| `days` | commits a day, 112 days, today last |
| `state` | `state`, `state_since`, `dirty_since` |

- The driver runs `git`, the commands of `_gitprobe.COMMANDS` and no
  other, arguments fixed by us, none from the guest but `log`'s
  count: `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`,
  `GIT_CONFIG_NOSYSTEM`, `-c core.fsmonitor=false -c
  core.hooksPath=/dev/null -c core.pager=cat`, no stdin, 2 s and
  4 MiB a command. A repository is foreign data: no command that runs
  a filter, a hook or `textconv` (`diff-files`, `diff-index` with
  `--numstat --no-ext-diff --no-textconv`). To audit before it lands.
- Not held: `{"repo": null, "degree": "none"}`. No `git`: `repo: null`.

### `probe.system`

```
probe.system {"parts"?: ["glance", "logins", "listeners", "disks"], "top"?: 5}
  -> {"glance": {...}, "logins": {...}, "listeners": [...], "disks": [...]}
```

| part | members (as `_probe`) |
|---|---|
| `glance` | `cpu`, `cores` (shares 0..1), `load` (3), `memory` `{total, available, swap_total, swap_free}`, `uptime`, `rx`, `tx` (bytes a second), `top`: `[{pid, name, cpu, mem}]`, 16 at most |
| `logins` | `now`, `seats`: `[{user, origin, ttys, login, idle, what}]` |
| `listeners` | `[{addr, port, user, process}]` |
| `disks` | `[{point, device, size, used, free}]` |

- Shares are between two samples: the driver keeps the last sample an
  app; the first answer has zeros. A sample a second at most; the
  scan of `/proc/<pid>/fd` (listeners) every 5 s, as `_probe.OWNED`.
- Linux (`/proc`, `/sys`, utmp, `/run/systemd/sessions`). Elsewhere:
  each part empty, `"unsupported": true`.
- Not held: every part empty, `"degree": "none"`.
- Both probes: plain while no sink exists (plan); the pop-up says
  `runs git in the directory of your program`, `sees the processes,
  the logins, the listening ports`.
- Headless: `--probe git=FILE.json`, `--probe system=FILE.json`: the
  answer, as written; none: the not-held answer, noted in the report.
  Not held: answered at once, in a batch too; held: pending, so
  `bad-request` in a batch **[as built, headless]**.
- Tests: the parser fixtures; a repository whose config names an
  fsmonitor, a pager, an external diff, a `textconv`: none runs (a
  marker file is not created); `repo` a handle of a file, of another
  app; `log` 10000; a `git` that hangs (the 2 s).

## The launcher's options (gap 3)

Parsed by `ptyrc app` for every store app, before the app's own;
reserved in the manifest (`RESERVED_OPTIONS`, add `border`). The app
is not told: it learns nothing by them.

| option | `app.start` member | effect, in the driver |
|---|---|---|
| `--frame F` | `frame` (exists) | the surface **[as built]**: absent, `[app] frame` (a name of `manifest::FRAMES`, resolved by the driver at the start), else `active` |
| `--front`, `--back` | `front: true \| false` | the app's band in front or behind for this run, whatever it asks; nothing asked (the user's hand). Absent: the manifest's wish under `front`/`front-widget` |
| `--prefer P` | `prefer` | the default of `widget.show {prefer}` and of `anchor.auto.prefer`, over the app's |
| `--alpha A` | `alpha` | multiplies the opacity of the band, 0..1 **[as built]**: the `alpha` of its sprites (`draw.put`, `draw.ext`, `draw.update`); widgets have none |
| `--seed N` **[as built]** | `seed` (a `u64`) | the runner's `--seed N`: `random` (and `Math.random`) drawn as `ptyrc dev test --seed N` draws it; absent, `/dev/urandom`. `seed` reserved in `[options]`. Kept in the argv: a restart draws the same. `dev run --seed N`, `dev test -- --seed N` the same |
| `--border B` **[owner]** | `border` | the default border of its widgets, over the app's **[as built]**: of `widget.show` and of `widget.declare {border}`; a name `widget.borders` does not list: `bad-params` at the start |

Kept in the app's entry (restart, `manage`: the same menu rows for
every store app). `dev test` and `dev run` take the same options.

## Options of the manifest (gaps 1, 2, 4)

```toml
[options]
grab  = { type = "bool", default = false, short = "g", needs = ["keys-arrows"], doc = "..." }
season = { type = "string", default = "auto", choices = ["auto", "spring", "summer"], doc = "..." }
word  = { type = "word", default = "flower", doc = "..." }     # the user's word: held for the run
mark  = { type = "pattern", list = true, doc = "..." }          # screen.find held to it
[args]
patterns = { type = "pattern", min = 1, max = 16, doc = "..." }
```

- `needs`: permissions asked only when the option is given (a bool:
  true); each must be in `[permissions]` (the reason is there). The
  pop-up shows the difference (the plan's Remembered). `dev test`
  holds them when the option is given.
- `word`: 3..32 letters or digits (the rule of `words`); joins
  `Grants.words` for the run, not shown as the app's, not counted in
  the 32. A value outside the rule: refused by the launcher, the hint
  saying so (no silent rise to `text`). **[as built]** In
  `Grants.hand.words`: found by `screen.find`/`screen.wait` at any
  degree (as a pattern of the user is), shown in `screen.shape`,
  `screen.text` from `shape`. A `default` of a `word` or `pattern` is
  the app's: not held (declare it in `words`).
- `pattern`: a literal or a regex (`regex = true`), 256 bytes; held
  in `Grants.patterns`; `screen.find`, `screen.wait {pattern}` with
  exactly that pattern are answered as under `text`, `known` too; any
  other pattern as today. 16 a run.
- `[args]`: one positional list, typed, `min`/`max`; `init(args)`
  gets it under its name. **[as built]** 16 values at most, a list
  option too. `short = "h"` refused (help). `choices`: of a `string`
  only. The reader: `ptyrc_store::options::read`; the check:
  `options::resolve`; `--help`: `options::help`.
- `default` **[as built]**: optional. None: a `bool` is false, any
  other `null` in `init`, no default in `--help` or the menu; `min`,
  `max`, `choices` rule a value given.

## Primitives that use without reading

| the app wants | primitive | state |
|---|---|---|
| next to a word | `anchor {text}` | exists; open to store apps with no `words` — check that `draw.list` and errors tell nothing of the landing (gap 16) |
| a free corner | `anchor {auto: {prefer}}` | exists |
| behind the text | its band | exists |
| follow the scroll | `anchor {at, space: "frame"}` | exists |
| rest on the text | effects `sand`, `rain`, `snow` | exist (`plugins/draw-ext`) |
| follow the cursor | `anchor {cursor}` | exists |
| the pointer told to an effect | `"effect": {"params": {"pointer": true, "bomb": true}}`: the driver feeds `hover`, and the press and release on the frame, to the effect; replaces `_cli.Nozzle` and its pad. `bomb` takes the clicks: `clicks-grab` | missing; S in `own.rs`, the effects of `draw-ext` |
| follow, flee the pointer | `anchor {pointer: {dx, dy}}`; `motion {flee: "pointer" \| "cursor", radius}` | missing; M in `ptyrc-core` scene |
| quiet while typing | sprite member `"when": "idle"`: drawn only while its frame is idle | missing; S (the pool gates by frame state already) |
| fade where busy | sprite member `"fade": "heat"`: alpha a cell scaled by `1 - heat` | missing; M in the composer; measure against the 3 ms bound |
| the surface follows its pane | the driver re-homes `Surface::Frame` and the app's sprites to the heir (`_cli.heir`'s rule) and sends `move` | missing (gap 12); M |

None but the last and the first row of "missing" blocks a migration.

## Slices

One agent at a time in `driver.rs`/`grants.rs`: A, then D, then F.
The others touch neither.

| slice | what | files | after | size |
|---|---|---|---|---|
| A | `hostio` worker, `Gate::Lowered`, `no-file`; `clock.*` (`zone.rs`), `storage.*`, `app.result` (+ `apps.rs` info); `design.md`, `PROTOCOL.md` rows | `ptyrc-server`: `driver.rs`, `grants.rs`, new `hostio.rs`, `zone.rs`, `storage.rs`, `driver/hostfn.rs` (`host.rs` is the host terminal); `ptyrc-proto` (error code) | `driver-a` | M+ |
| B | Headless: every host function from memory, `--clock`, `--zone`, `--storage`, `--file`, `--probe`, the report sections; assets served by the runner (live and headless); gaps 6 and 7 (`batch` and the rest, `--pointer`, `--click`, `--focus`) | `ptyrc-box` (`headless.rs`, `core.rs`, `main.rs`, `grants.rs` copy), `ptyrc-cli/src/dev.rs` | `tools-c` out of `ptyrc-box`; parallel to A (this file is the contract) | M, perhaps two sessions |
| C | `rust/ptyrc-probe`: the git and system parsers, the sampler, the `git` runner with its hardening, fixtures. No driver | new crate | now | M+ |
| D | Manifest and launcher: gaps 1, 2, 3, 4; `file`/`dir` types and `app.start {files}`; `--wait` | `ptyrc-store/src/manifest.rs`, `ptyrc-server/src/store.rs`, `driver.rs` (`app.start`), `grants.rs`/`lower.rs` (words, patterns), `driver/own.rs` (front by hand, alpha, prefer), `ptyrc-cli/src/storeapp.rs`, `manage` | A | M+ |
| E | SDKs: `clock`, `storage`, `asset`, `file`, `result`, base64, `civil`; the deck of widgets, the field and list of `ui`; a `Canvas` as widget content; `ptyrc.d.ts`, READMEs, template `AGENTS.md` | `sdk/js` (after `quickjs`), `sdk/rust/ptyrc-app` (with the removal of `serde_json`, gap 10), `ptyrc-core` widgets if `grid` is missing | `quickjs`; B for its tests | M, two agents (one a tier) |
| F | Driver, second round: `file.*`, `probe.*` wired to C, gap 12 (the surface follows), gap 13 (`clicks-grab` on its own pad), gap 16 | `driver.rs`, `own.rs`, `host/*.rs`, `mouse.rs` | A, C, D | M+ |
| G | `plugins/store-apps/`: the layout, the harness (`dev check`, `dev test` against kept screens), how a shipped app is found (gap 9); `dev test --unit` (gap 8) | `plugins/store-apps/`, `ptyrc-cli/src/dev.rs`, `sdk/js/interp`, `ptyrc-store` | `quickjs`, the owner's answers 5 and 10 | M |

Then the proof again (an agent with `dev new` alone writes a clock
with a zone and a stored setting), and wave 1.

## For the owner

1. **The zone database.** A TZif reader of ours over the system's
   database (about 300 lines, no dependency), or a crate (`jiff`,
   `chrono-tz`: the database compiled in, megabytes). *Recommended:
   ours.*
2. **A permission by option** (`needs`), as the plan's Remembered
   says, or everything declared asked at the first start.
   *Recommended: `needs`*; `snake` without `--grab` should ask nothing.
3. **Storage**: a table of 64 named values, 1 MiB an app, fixed; no
   `[limits]` key for more at first. *Recommended.*
4. **A default file** in the manifest (`note`: `TODO.md` of the
   working directory): allowed as a plain relative name, asked as
   `files` with its name shown; or refused (`note` then wants its file
   named each time). *Recommended: allowed.*
5. **Shipped store apps**: origin `ptyrc/NAME`, installed with ptyrc
   (no `store add`), tier `shipped`; the pop-up asks them as any app
   (plain ones ticked). *Recommended.* Or granted as declared without
   a pop-up when nothing is red.
6. **`--border`** a launcher option with `--prefer`, `--front`.
   *Recommended: yes*; 14 apps parse it today.
7. **`probe.git` with no directory named** reads the repository of
   the wrapped program's working directory; paths of roots and
   worktrees are cut to their last component. *Recommended.*
8. **Assets served by the runner** from memory, not by the driver.
   *Recommended*; it departs from "the driver serves everything".
9. **`pace`**: its counts in `storage`; `--save FILE` kept as an
   option or dropped. *Recommended: storage, `--save` dropped.*
10. **Shipped Rust apps** (`life`, `flock`, `shore`, `cat`, `stars`,
    `pond`, `minimap`): built on the user's machine at install
    (needs `rustc` and the target on every machine that runs ptyrc),
    or built with the release, as the interpreter is. *Recommended:
    with the release; "source travels" holds for the store.* Or all
    seven as scripts if QuickJS measures fast enough: to measure on
    `life` first.
11. **`flower -a`, `flower --word` outside the rule** rose silently to
    `text`. *Recommended:* `-a` needs `text` (`needs`); a word outside
    the rule is refused.
