# The registry server: protocol, version 1

One HTTP server, one directory of data. It lists apps and keeps the
archives pushed to it. It is no root of trust: a client checks every
tree itself, and an app runs in the sandbox under its grants whatever
the server says. What the server adds: a name, a tier on a revision,
and, for a Rust app, a module built from the archive.

Two kinds of app:

| kind | name | revision | the source is |
|---|---|---|---|
| `tar` | the manifest's `[app] id` | sha256 of the `.tar.gz`, 64 hex | on the server |
| `git` | `host/owner/repo[#path]` | the commit, 40 hex | in its repository; never cloned here |

A client names a `tar` app `HOST/NAME` (`HOST` the server's), a `git`
app by its origin, as without a registry.

## Data

```
untested.sqlitedb  tested.sqlitedb  approved.sqlitedb
untested/          tested/          approved/
maintainers.toml
```

A version is a row of one database; for a `tar` app its archive
`<tier>/<rev>.tar.gz`, and its module `<tier>/<rev>.wasm` when built, in
the directory of the same tier. Raising a tier moves the row and the
files. The owners (`name`, sha256 of the token) are a table of
`untested.sqlitedb`.

## Errors

A status of 4xx or 5xx, and `{"error": TEXT, "hint": TEXT|null}`.

| status | when |
|---|---|
| 400 | a body, an archive or a manifest refused |
| 401 | the name is taken and the token is absent or wrong |
| 403 | a maintainer's token that may not say that tier |
| 404 | no such app, revision or file |
| 413 | a body past its bound |
| 429 | too many pushes from that address |
| 507 | the registry is full |

## Read

`GET /v1/index.json`: everything listed. `ETag`; `If-None-Match`
answers 304.

```json
{"format": 1, "apps": [{
  "name": "confetti",
  "kind": "tar",
  "yanked": false,
  "description": "Paper falls behind your text.",
  "manifest_path": "ptyrc-app.toml",
  "versions": [{
    "rev": "9f2c...",
    "tier": "untested",
    "by": null,
    "date": "2026-11-25",
    "yanked": false,
    "id": "confetti",
    "version": "0.1.0",
    "summary": "Paper falls behind your text.",
    "license": "GPL-3.0-or-later",
    "surface": "frame",
    "entry": "script",
    "permissions": [{"name": "keys", "why": "...", "words": []}],
    "size": 5120,
    "wasm": null
  }]
}]}
```

- `apps` sorted by `name`; `versions` oldest first.
- A `git` app: `name` its origin id, `url` (`https://host/owner/repo`)
  and `manifest_path` (the manifest's path in the repository) set.
- `entry`: `script` or `rust`. `wasm`: sha256 of the module built from
  that archive, or null. `size`: the archive's, bytes; null for `git`.
- `by`: the maintainer who set the tier.
- The fields of the manifest are what the server read at the push, for
  display. Never for trust.

`GET /v1/search?q=WORD`: `{"apps": [...]}`, the apps of the index whose
name, id, summary or description holds WORD, case ignored.

`GET /v1/blob/<rev>.tar.gz`: the archive. `GET /v1/blob/<rev>.wasm`:
the module built from it. Both immutable.

`GET /`: one page, the list. `GET /healthz`: `ok`.

## Push

No account. The first push of a name answers a token, once; every later
push, and a yank, of that name wants it: `Authorization: Bearer TOKEN`.
A lost token is reset on the server (`server.py token NAME`).

`POST /v1/push/tar?description=TEXT`, the body the `.tar.gz`
(`description` percent-encoded, optional: the manifest's summary).

`POST /v1/push/git`, the body JSON:

```json
{"origin": "https://host/owner/repo", "path": "apps/sand",
 "commit": "3f9c...", "manifest": "[app]\nid = ...",
 "manifest_path": "apps/sand/ptyrc-app.toml", "description": "..."}
```

`path` null or the app's directory; `manifest` the manifest's text at
that commit, as the pusher read it.

Both answer 201 (200 when that revision was listed already):

```json
{"name": "confetti", "kind": "tar", "rev": "9f2c...",
 "tier": "untested", "token": "..."}
```

`token` only when the name was created by this push.

### What the server checks

An archive: gzip, then ustar; 8 MiB packed, 8 MiB and 512 entries
unpacked; regular files and directories only; relative paths, no `..`,
no `.git`, no control character; `ptyrc-app.toml` at the root, 64 KiB,
valid TOML with `[app] id` (a lower-case letter, then lower-case letters, digits,
`-`; 64 at most), `version`, `summary`, `license`
(`GPL-3.0-or-later`), `entry`. Never unpacked to disk by the server.
The full check (`ptyrc dev check`) is the pusher's before and every
client's after.

A `git` push: `origin` https, `commit` 40 hex, the manifest as above.
Nothing is fetched: what it says is the pusher's word, shown, and read
again from the repository by the client that installs.

### Bounds

| | default |
|---|---|
| the data directory | 500 MB; a push past it: 507 |
| an archive | 8 MiB |
| a JSON body | 128 KiB |
| a description | 2000 characters |
| untested versions kept, an app | 20; the oldest goes |
| pushes, an address | 30 an hour |
| names created, an address | 5 a day |

## Tiers

`POST /v1/tier`, `{"name": ..., "rev": ..., "tier": "tested"}`, a
maintainer's token. `maintainers.toml`:

```toml
[[maintainer]]
name = "someone"
may = "approved"                  # or "tested"
token_sha256 = "..."
```

`tested` may say `untested`, `tested`; `approved` any. Locally, no
token: `server.py tier NAME REV TIER`.

`POST /v1/yank`, `{"name": ..., "rev": REV|null, "yanked": true}`: the
owner's token, or an `approved` maintainer's. `rev` null: the app.

## Modules

`builder.py`, a process apart from the server: for each `tar` version
whose `entry` is `rust` and that has no module, the archive is unpacked
in a new directory and built in a container without network
(`ptyrc dev build DIR --out FILE`); the module goes to
`<tier>/<rev>.wasm`, its sha256 to the index. A build that fails is
noted and not tried again.

A client with `rustc` builds from the archive itself. One without may
take the module: it checks its sha256 against the index and that every
import is of `ptyrc`. The module then runs as any: in the sandbox,
under the grants of the manifest in the archive. What the client cannot
check is that the module is that source's; the server's word.
