# ptyrc-probe

What the driver reads of the machine for store apps: `probe.git`,
`probe.system` (`temp/20112026-01-store/design/host-functions.md`,
"Probes"). Ported from `_gitprobe.py` and `_probe.py`. Parsers
(`git::parse`, `system::parse`): text in, typed values out, no clock,
no I/O. Readers: blocking, run on the probe lane of the driver's
worker (`ptyrc-server`, `driver/hostfn.rs`).

## `git`

- `git::probe(dir, &Ask, Now) -> Value`; `probe_with(program, ...)`
  runs another `git` (tests). `Ask::from_json(params)`: `parts`
  (default all: `head`, `changes`, `log`, `stashes`, `branches`,
  `worktrees`, `days`, `state`), `log` (default 30, clamped to 1..200),
  `since`; `repo` is the driver's. Err: the hint for `bad-params`.
  `Now { at, offset }`: epoch seconds, zone offset east of UTC (for
  `days`).
- Answer: `{"repo": {...}, "stamp": "..."}`; `{"repo": null, "stamp":
  ""}` with no repository, no `git`, or a `git` that fails `rev-parse`;
  `{"unchanged": true, "stamp"}` when `since` equals the stamp now.
  `git::not_held()`: `{"repo": null, "degree": "none"}`.
- `repo`: `name` (last component of the root), `now`, and per part:
  `head {oid, branch, upstream, ahead, behind, gone, initial}`;
  `changes [{kind, code, path, added, deleted}]` (500 at most) and
  `counts {staged, unstaged, untracked, conflict}` (all);
  `log [{oid, at, author, subject, refs, graph}]`;
  `stashes [{n, at, subject}]`;
  `branches [{name, current, at, upstream, ahead, behind, gone, subject}]`;
  `worktrees [{name, oid, branch, flags, current}]` (`name`: last
  component); `days` (112 counts, today last); `state`, `state_since`,
  `dirty_since`. Lists 500 at most. No absolute path in an answer.
- The stamp: a hash of the git and common directories, the parts, the
  count, and the mtimes of `HEAD`, `index`, `logs/HEAD`, the files of
  an open operation, `refs/stash`, `FETCH_HEAD`, `packed-refs`,
  `worktrees`, `refs/heads`, `config`. Checked without `git` (`.git`
  found as git finds it, `git::discover`). The work tree is not
  watched: a file changed and not added keeps the stamp. The rate (one
  real read a second an app) is the driver's.

## Audit: what a repository can make `git` run

The verbs are those of `run::COMMANDS`; each argv is fixed in
`run::Cmd` (only `log`'s count and `days`' first second vary). Every
command: no stdin, stderr dropped, its own process group, killed with
the group past 2 s or 4 MiB of output; every `GIT_*` of the driver's
environment removed, then `GIT_OPTIONAL_LOCKS=0`,
`GIT_TERMINAL_PROMPT=0`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_ATTR_NOSYSTEM=1`,
`GIT_NO_LAZY_FETCH=1`, `LC_ALL=C`; `--no-pager --no-optional-locks`
and `-c` (which wins over the repository's config):
`core.fsmonitor=false core.hooksPath=/dev/null core.pager=cat
core.quotePath=false core.attributesFile=/dev/null
log.showSignature=false color.ui=false protocol.allow=never
submodule.recurse=false`.

| vector | where | closed by |
|---|---|---|
| `core.fsmonitor` | `status`, `diff-*` | `-c core.fsmonitor=false` |
| hooks (`post-index-change`, ...) | an index write | no write (optional locks off); `core.hooksPath=/dev/null` |
| pager (`core.pager`, `pager.<cmd>`) | every verb | `--no-pager`, `core.pager=cat`, no tty |
| `filter.<d>.clean`/`process` via `.gitattributes` | `status`, `diff-*` read a stat-dirty file | `--attr-source=<empty tree>`: no attribute of the tree or index |
| the same via `core.attributesFile` | idem | `-c core.attributesFile=/dev/null` |
| the same via `info/attributes` | idem | each `filter=NAME` there (macros too) given `filter.NAME.{clean,smudge,process}=` and `required=false`; more than 32, or unreadable: `status`, `diff-*` not run |
| `diff.external`, `diff.<d>.command`, `GIT_EXTERNAL_DIFF` | a patch | no patch (`--numstat`), `--no-ext-diff`, the variable removed |
| `diff.<d>.textconv` | idem | `--no-textconv` |
| `log.showSignature` + `gpg.program` | `log`, `stash list` | `-c log.showSignature=false`, `--no-show-signature` |
| lazy fetch of a partial clone (`remote.*.uploadpack`, `core.sshCommand`, `ext::`) | any missing object | `GIT_NO_LAZY_FETCH=1`, `protocol.allow=never` |
| submodules (their own config) | `status`, `diff-*` | `--ignore-submodules=all`, `submodule.recurse=false` |
| `alias.*` | | no effect: the verbs are built in |
| `include.path`, `includeIf` | config | reads config only; what it sets is covered above |
| `color.ui=always` | `log --graph` | `--no-color`, `color.ui=false` |
| `credential.helper`, `core.askPass`, `http.*`, `core.gitProxy` | network | no network: no fetch, `protocol.allow=never` |

Left: `core.worktree` may point the work tree at another directory:
its files are listed as changes (names relative, no root path). The
user's global config is read (not the system's). `safe.directory`
refuses a repository of another user: `repo: null`.

Tests (`tests/git_repo.rs`): a repository whose config and attributes
name a marker command at each place of the table but submodules and
the network; after probes no marker exists, while plain `git status`
and `git log` run the fsmonitor, a hook, the filters and `gpg`. A
partial clone missing a tree: no `uploadpack` (plain `git status`
runs it). A fake `git` that hangs is cut
at 2 s with its child; 5 MiB of output gives nothing.

## `system`

- `Sampler::new()`, kept by the caller, one an app;
  `Sampler::probe(&Ask, now) -> Value`, `now` epoch seconds.
  `Ask::from_json`: `parts` (default all: `glance`, `logins`,
  `listeners`, `disks`), `top` (default 5, clamped to 16).
  `system::not_held(&ask)`: each part empty, `"degree": "none"`.
- `glance {cpu, cores, load, memory {total, available, swap_total,
  swap_free}, uptime, rx, tx, top [{pid, name, cpu, mem}]}`: shares
  and rates between the last two samples; the first answer, or one
  after `now` went back, has zeros and an empty `top`. A sample is
  kept 1 s: sooner, the last two again.
- `logins {now, seats [{user, origin, ttys, login, idle, what}]}`:
  `/run/utmp`, else `/run/systemd/sessions`; live leaders only; a tty
  name with `..` or empty components is not looked up.
- `listeners [{addr, port, user, process}]`: `/proc/net/tcp{,6}`, not
  loopback; owners from `/proc/*/fd`, scanned again when the sockets
  change or after 5 s; users from `/etc/passwd`.
- `disks [{point, device, size, used, free}]`: `/proc/mounts`, block
  devices, not loop nor images; one a filesystem.
- Not Linux: each part asked empty, `"unsupported": true`.
- Clock ticks, page size, `statvfs`: `rustix`.
