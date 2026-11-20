# ptyrc-store

Store apps without the engine: the manifest, the origin, the fetch, the
tree check, the build of a Rust app, the install, the registry, its
index, page and review. Plan: `plan/store-model.md`. Not here: the
runner, the CLI (`ptyrc store`, `ptyrc dev` in `ptyrc-cli`), the
grants.

Every fallible call returns `Result<T, Error>`; `Error { kind, message,
hint }`: `message` says what was refused (manifest errors as
`ptyrc-app.toml:LINE: [section] key: ...`), `hint` the cure.

## Manifest

- `manifest::parse(&str)`, `manifest::parse_bytes(&[u8])` -> `Manifest
  { app, permissions, limits, options, args }`.
- `App { id, version, summary, license, api, entry: Entry::Script |
  Entry::Rust, rustc, assets, surface, tape }`; `surface` absent:
  `Widget`.
- `Permission { name, why, words }`; names from `manifest::PERMISSIONS`;
  `words = { list = [...], why = "..." }`: 1 to 32 words, 3 to 32
  letters or digits, distinct ignoring case.
- `Limit { name, value }`: keys, defaults and ceilings in
  `manifest::LIMITS`; a value exceeds its default, not its ceiling (the
  runner's). Granted by `limit-memory` or `limit-time`
  (`limit_permission`), never declared in `[permissions]`.
- `AppOption { name, ty: OptionType, default: Option<OptionValue>,
  doc, short, choices, list, needs, write, regex }`; `[args]`:
  `Positional { option, min, max }`, one list. Types `int`, `float`,
  `bool`, `string`, `word`, `pattern`, `file`, `dir`. Refused at
  parse: a name in `RESERVED_OPTIONS` or `no-NAME`; a short twice or
  `h`; `choices` but of a `string`, a default outside them; a list of
  `bool`, a default of a list but a plain file name; `write` but of a
  `file`, `regex` but of a `pattern`; a `needs` name not in
  `[permissions]`; a `file` or `dir` without `files`, `write` without
  `files-write`.

## Options

`options`: one reader of a command line, one check of what reaches
the driver.

- `read(manifest, words, cwd) -> Option<Launch>`: the launcher's
  options (`Launcher { frame, front, prefer, alpha, border, wait }`),
  then the app's: `--NAME VALUE`, `--NAME=VALUE`, `-S VALUE`, `-SVALUE`,
  `-abc` of bools, `--no-NAME`, `NAME=VALUE`; positionals for
  `[args]`, all after `--`. Files: `name_path` (absolute, canonical; a
  regular file or a directory; a file to write may be absent, its
  directory not), a handle each (`f0`, ...), the default file of the
  working directory when none is named. `None`: `--help`. Checked by
  `resolve` before it returns. `Launch::params(store)`: the params of
  `app.start`.
- `resolve(manifest, args, files) -> Resolved { init, needs, words,
  patterns, files }`: each value of its type and rule (a `word` by
  the rule of `words`, a `pattern` 1 to 256 bytes, 16 a run), each
  handle given once and of its kind; the defaults filled in; `needs`
  of what was given (`files` for a default file, `files-write` for a
  file to write). `Resolved::asked(manifest)`: the permissions
  declared for that run. `check_named(path, kind, write)`: the
  driver's check of a path again.
- `help(manifest, program)`: `--help` of a store app.
- Errors: `ErrorKind::Usage`.

## Origin

- `Origin::parse_spec("host/owner/repo#path@REF")` -> `(Origin,
  Option<Reference>)`; `Origin::parse` without `@REF`. Forms:
  `host/owner/repo`, `https://host/owner/repo[.git]`,
  `local:/abs/dir` (dev, tests); `#path` the app's directory.
- `id()`: the normalised identity; `dir_name()`: it as one file name;
  `remote()`: `Remote::Https | Remote::Local`; `sub()`;
  `registry_path()`.
- `Reference::Head | Name(ref) | Commit(hex)`; `Reference::parse`.

## Fetch and check

```rust
let repo = Repo::open(&dirs.git(&origin))?;          // bare, ours
let commit = repo.fetch(&origin.remote(), &pick.reference)?;
let tree = repo.tree(&commit, origin.sub())?;        // GitTree
let checked = check(&tree)?;                         // or DirTree::new(dir)
```

- `fetch`: depth 1, no tags; `https` only, `file` for a local origin;
  cleared environment, no user or system configuration, no prompt,
  credential, hook, submodule, lazy fetch; `fsckObjects`. Bounded:
  past `MAX_FETCH` (64 MiB of packs; `Repo::max_fetch`) git and its
  children are killed and the packs removed.
- `check(&dyn Tree) -> Checked { manifest, files, dirs, size }`: paths
  (no `..`, `.git`, absolute, control), regular files and directories
  only, 512 files, 8 MiB, a `.js` 1 MiB, the manifest 64 KiB and valid,
  the GPL-3 text as `LICENSE`/`COPYING` at the app root, the entry,
  `assets`, `tape`.
- `check_all`: every problem (`Vec<Error>`, never empty); what needs
  the manifest is skipped without it. `check` is its first.
- `DirTree` skips, at its root, `.git` and a directory holding
  `CACHEDIR.TAG` (cargo's `target/`).

## Build

A Rust app by `rustc` alone (decision 20): std and `ptyrc-app`, no
other crate, no build script, no macro crate. `BUILD-PROFILE.md`: the
profile, what it holds, what is left.

```rust
let toolchain = build::Toolchain::find()?;            // PTYRC_RUSTC, else rustc
let built = build::build(&tree, &checked, &work, &toolchain, &build::Options::default())?;
install::install_built(&dirs, &origin, &tree, &checked, Some(&built.wasm), ...)?;
```

- `Toolchain::find`: `PTYRC_RUSTC` or `rustc` on `PATH`, asked its
  sysroot from `/`; then `<sysroot>/bin/rustc`, not the rustup proxy.
  No `rustc`, or no standard library for `wasm32-unknown-unknown`:
  `ErrorKind::Build`, the hint says how to get them.
- `build::sources(tree, checked) -> Vec<Error>`: the rule (`RULE`,
  named in each hint). Refused: `build.rs`; in `Cargo.toml` a
  dependency other than `ptyrc-app` (`[dev-dependencies]` stay the
  author's), `[package] build`, `[lib] proc-macro`; a root `app.wasm`;
  in the sources `#![feature]`, `#[link]`, `#[proc_macro*]`,
  `#![crate_type]`, `extern crate` of a crate but `std`, `core`,
  `alloc`, `ptyrc_app`; `include_str!`, `include_bytes!`, `include!`,
  `#[path]` but with a literal path inside the tree; one of these macros
  named other than as called. `vendor/ptyrc-app/` is not read.
- `build::build(tree, checked, work, toolchain, options) -> Built {
  wasm, profile, missing, rustc, messages }`: `sources` first; the
  manifest's `rustc` against the toolchain; in `work` (made, or emptied
  if it holds `.ptyrc-build`): the tree but `vendor/ptyrc-app/`, and
  `PTYRC_APP`, the sources of `sdk/rust/ptyrc-app` embedded in this
  crate. `ptyrc-app` as an rlib, then the app as a `cdylib` with
  `--extern ptyrc_app`; `--edition 2024`, `-C opt-level=s`, `panic=abort`,
  `codegen-units=1`, `lto`, `strip=symbols`; the environment cleared;
  paths in messages from the app's root.
- Each `rustc` under the profile (Linux, Landlock), from a thread that
  restricts itself and spawns it; the rest of the process is not
  restricted. No profile: refused, unless `Options::unconfined` (`store
  add --force`; `ptyrc dev`, the author's own tree): built, `missing`
  says why.
- `Bounds` (`BOUNDS`): 300 s for both runs, then the process group
  killed; `RLIMIT_AS` 4 GiB; `RLIMIT_FSIZE` four times the module; the
  module 16 MiB. `build::imports(wasm)`: every import is of `ptyrc`,
  else refused (the runner refuses it too).
- `build::vendored_differs`: the files of `vendor/ptyrc-app/` that are
  not `PTYRC_APP` (`ptyrc dev check` says so).

## Install

- `Dirs { data, state, cache, config }`: `Dirs::from_env()` (XDG, else
  `HOME`), `Dirs::under(root)`. Paths: `apps()`, `app(o)`, `tree(o, c)`,
  `lock()`, `store_lock()`, `storage(o)`, `git(o)`, `registry()`,
  `permissions()`, `store()`.
- `StoreLock::take(dirs, on_wait)`: one writer of the trees and the
  lock, an exclusive `flock`; the caller of every write holds it. Not
  reentrant.
- `install::install(dirs, origin, tree, checked, commit, tier, date)`
  -> `Installed`: copies to `<data>/ptyrc/apps/<origin>/<commit>/`
  (directories 0700, files 0400) and records it in `apps.lock` (0600).
  A script app. `install::install_built(..., Some(module), ...)`: a Rust
  app, its module written as `app.wasm` at the root of the tree; a Rust
  app without one, a script app with one: refused.
- The old tree is kept until the new one has started once: an
  `Installed` carries `previous` (the last commit that started) and
  `started`. `install::started(dirs, o)` records the start, clears
  `previous` and prunes.
- `install::locate(dirs, ID|ORIGIN)` -> `(Installed, tree dir)`: what
  `ptyrc app` starts.
- `install::list`, `install::trees(dirs, o)`, `install::prune(dirs, o)`
  (all but the commit in use and `previous`), `install::remove(dirs,
  ID|ORIGIN)` (lock entry and trees), `install::purge_storage(dirs, o)`,
  `install::purge_grants(dirs, o)` (the table `[app."<origin>"]` of
  `permissions.toml`).
- `Lock::load`, `save`, `get(origin)`, `resolve(ID|ORIGIN)`.

## Registry

- `Pointer::parse(name, src)`: `origin`, `manifest`, `yanked`,
  `[[version]] { commit, tier, by, date, yanked }`; `Pointer::origin()`.
- `Registries::load(dirs)` from `store.toml` (`[[registry]] name, url |
  path`): `url` https or the absolute path of a git repository, fetched
  to `<cache>/ptyrc/registry/<name>/`; `path` a directory read as it is.
  None by default. `add(dirs, name, source, path)`, `to_toml`, `save`.
- `lookup(origin)`: the first that lists it; a registry absent here is
  an error. `lookup_present(origin) -> Present { found, absent }`: absent
  ones passed over, those before `found` named.
- `Registry::pointers()`: every pointer under `apps/`; `remote()`.
- `choose(Option<&Pointer>, &Want { reference, latest, force }) -> Pick
  { reference, tier }`; after fetching a name or `HEAD`,
  `registry::tier_of(pointer, commit, force)`.
- `Tier::Untested < Tested < Approved`.

## Sync

`sync::sync(registry, now, max_age) -> Synced`: a `url` registry whose
copy is older than `max_age` (`None`: any) is fetched with `Repo` (head,
depth 1, the same hardening, `MAX_REGISTRY_FETCH`); its `apps/**.toml`
(1 MiB each, 100 000, 64 MiB in all, no hidden component) and
`index.json` (32 MiB) read with `git cat-file` and written to a new
copy, swapped in; the bare repository removed. `Local | Fresh |
Fetched { commit } | Stale { error, since } | Missing { error }`: a
failure keeps the old copy. `sync::stamp`: when, and which commit.
`now` is an argument; `MAX_AGE` a day. The caller holds the store lock.

## Index and page

- `index::build(dirs, registry, remote) -> Index`: each pointer, each
  listed commit fetched and checked (`fetch_checked`); the fields of the
  version `choose` picks: `id`, `summary`, `version`, `license`,
  `surface`, `tape`, `commit`, `tier`, `permissions { name, why, colour,
  words }`; each version's `version` or `error`. `Index::{to_json,
  parse, load, get}`; `FORMAT` 1. Read for display only, never for
  trust.
- `page::render(index, title)`: one HTML file, inline style, no script,
  no external resource, a CSP of `default-src 'none'`. Every text
  escaped (`page::escape`).
- `manifest::COLOURS`, `colour(name) -> Colour::{Plain, Amber, Red}`:
  the pop-up's table (`ptyrc-server`, `popup::told`), duplicated and
  pinned by `ptyrc-cli/tests/store.rs`. The server is to take it from
  here.

## Review

`review::review(base, head, author, maintainers, check) -> Review {
problems, checked }`, two trees of a registry: only pointer files
change, each where its origin says; a new commit fetched and checked by
`check`; a tier above `untested` set by one `Maintainers::may` (`NAME
[tested|approved]` per line), with `by` the author; any other change to
a listed version, a yank, a removal, another file: an `approved`
maintainer's. `Repo::commit(rev)` resolves BASE and HEAD.

## A `store add`, in order

1. `StoreLock::take`.
2. `Origin::parse_spec`; `Registries::load(&dirs)?`; `sync` each, a
   day old; `lookup_present(&origin)`: an absent registry before the
   one found wants `@REF` or `--latest`.
3. `choose`.
4. `Repo::open`, `fetch`, `tree`, `check`, `tier_of`; `untested`: ask
   the user.
5. A Rust app: `build::build` in `<cache>/ptyrc/build/<origin>/`,
   removed after.
6. `install::install_built(..., date)`, `install::prune`.
