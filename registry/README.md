# ptyrc-store

The registry of ptyrc store apps: a git repository of pointers. It holds
no code and is no root of trust: `ptyrc store add` reads each app's
manifest from its own repository and checks it. What a pointer adds is
a word, a tier, on a commit. Plan: `plan/store-model.md`, Registry.

Kept in the ptyrc repository under `registry/` until it is split out.

## Tiers

| tier | means | set by |
|---|---|---|
| `untested` | proposed and listed; nobody vouches | a pull request, merged |
| `tested` | it runs and looks fine | a name of `MAINTAINERS` marked `tested` or `approved`, not the author |
| `approved` | no danger: that version was read | a name of `MAINTAINERS` marked `approved` |

A tier is a word on a commit, never on an app; a commit not listed is
`untested`. `yanked = true` on a version or on the app withdraws it:
`store add` refuses it without `--force`, `store ls` says it in red.

## Listing an app

1. In the app's repository, committed and pushed:
   `ptyrc dev entry [DIR] > entry.txt`. The first line names the file,
   `apps/<host>/<owner>/<repo>[/<path>].toml`; the rest is its content.
2. A pull request adding that file, or a `[[version]]` to it, `tier =
   "untested"`.
3. To raise a tier: a pull request by one who may, `by` set to that
   name.

```toml
origin = "https://example.org/someone/sand-garden"
manifest = "ptyrc-app.toml"

[[version]]
commit = "3f9c..."                 # the full id
tier = "approved"
by = "maintainer"
date = 2026-11-17
```

## Checks

`ci/check.sh BASE HEAD AUTHOR`, on a pull request
(`ptyrc dev registry-check`):

- only pointer files change, each at the place its origin and manifest
  name: one file per app;
- each new commit is fetched and passes `ptyrc dev check`;
- a tier above `untested` is set by one who may (`MAINTAINERS`), with
  `by` the author;
- a change to a listed version other than its tier raised, a yank, a
  removal, any other file: an `approved` maintainer's.

`ci/merge.sh`, after a merge (`ptyrc dev index`): `index.json` (each
app: origin, id, summary, version, licence, permissions and their
colours, surface, tape, versions and tiers) and `site/index.html`, one
self-contained page from it. Commit `index.json` for clients to read it
(`store search`, `show`; never for trust); publish `site/`.

Both are plain scripts, run locally; `PTYRC` names the binary. Who may
say `tested`, and whether tiers are signed, is to be decided.

## The registry server

A second kind of registry, an HTTP server (`registry/API.md`) people
push to: no git, no account. Its tiers are the same, a word on a
revision.

- Users: nothing to configure; without `store.toml` the default is
  `store.ptyrc.top`. `ptyrc store add NAME`, or `HOST/NAME`. Another:
  `ptyrc store registry add NAME https://HOST --api`.
- Authors: `ptyrc dev check`, then `ptyrc dev push`: the tree packed
  and sent; it prints `ptyrc store add HOST/NAME`. The revision is the
  sha256 of the archive. `ptyrc dev push --git`: the commit at HEAD,
  pushed to its repository first; the app keeps its git origin.
- The token: the first push of a name answers it, kept in
  `<config>/ptyrc/tokens.toml` (0600); every later push wants it. On
  another machine: `--token TOKEN`. Lost: reset on the server.

## Users of a git registry

```
ptyrc store registry add main https://example.org/ptyrc-store
ptyrc store sync
ptyrc store search WORD
```
