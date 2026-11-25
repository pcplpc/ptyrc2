# ptyrc-store

Store apps without the engine: the manifest, the origin, the fetch, the
tree check, the build of a Rust app, the install, the registry, its
index, page and review; a registry server (`registry/API.md`): its
archive, HTTP by `curl`, push, tokens. Plan: `plan/store-model.md`. Not here: the
runner, the CLI (`ptyrc store`, `ptyrc dev` in `ptyrc-cli`), the
grants.

Every fallible call returns `Result<T, Error>`; `Error { kind, message,
hint }`: `message` says what was refused (manifest errors as
`ptyrc-app.toml:LINE: [section] key: ...`), `hint` the cure.

## Manifest

- `manifest::parse(&str)`, `manifest::parse_bytes(&[u8])` -> `Manifest
  { app, permissions, limits, options, args }`.
- `App { id, version, summary, license, api, entry: Entry::Script |
  Entry::Rust, rustc, assets, surface, frame, tape, wait, random }`;
  `surface` absent: `Widget`; `frame`: one of `manifest::FRAMES`
  (`active`, `idle`, `largest`, `quietest`, `raw`), the frame of a
  start without `--frame`, absent `None` (the driver's `active`); `all`
  refused; `random`: the option words of each draw of the menu's random
  app (`[[], ["-a"]]`), `[]` never drawn, absent `[[]]`; bounds
  `RANDOM_MAX`.
- `Permission { name, why, words }`; names from `manifest::PERMISSIONS`;
  `words = { list = [...], why = "..." }`: 1 to 32 words, 3 to 32
  letters or digits, distinct ignoring case.
- `Limit { name, value }`: keys, defaults and ceilings in
  `manifest::LIMITS`; a value exceeds its default, not its ceiling (the
  runner's). Granted by `limit-memory` or `limit-time`
  (`limit_permission`), never declared in `[permissions]`.
- `AppOption { name, ty: OptionType, default: Option<OptionValue>, doc,
  short, choices, list, needs, write, regex, min, max }`; `min`, `max`
  bound a value of an `int` or `float` of `[options]`
  (`AppOption::range`: `1 to 10`). `default` optional: a `bool`
  without one is `false`; any other `None`, `null` in `init`, shown
  nowhere; `min`, `max`, `choices` rule a value given. `[args]`:
  `Positional { option, min, max }`, one list, `min`, `max` the count
  of values. Types `int`,
  `float`, `bool`, `string`, `word`, `pattern`, `file`, `dir`. Refused
  at parse: a name in `RESERVED_OPTIONS` or `no-NAME`; a short twice or
  `h`; `choices` but of a `string`, a default outside them; `min`, `max`
  but of an `int` or `float`, not of its type, `min > max`, a default
  outside; a list of `bool`, a default of a list but a plain file name;
  `write` but of a `file`, `regex` but of a `pattern`; a `needs` name
  not in `[permissions]`; a `file` or `dir` without `files`, `write`
  without `files-write`.

## Options

`options`: one reader of a command line, one check of what reaches
the driver.

- `read(manifest, words, cwd) -> Option<Launch>`: the launcher's
  options (`Launcher { frame, front, prefer, alpha, border, wait, seed
  }`; `--seed N`, a `u64`: the member `seed` of `app.start`), then the
  app's: `--NAME VALUE`, `--NAME=VALUE`, `-S VALUE`, `-SVALUE`,
  `-abc` of bools, `--no-NAME`, `NAME=VALUE`; positionals for
  `[args]`, all after `--`. Files: `name_path` (absolute, canonical; a
  regular file or a directory; a file to write may be absent, its
  directory not), a handle each (`f0`, ...), the default file of the
  working directory when none is named. `None`: `--help`. Checked by
  `resolve` before it returns. `Launch::params(store)`: the params of
  `app.start`.
- `resolve(manifest, args, files) -> Resolved { init, needs, words,
  patterns, files }`: each value of its type and rule (a `word` by
  the rule of `words`, a `pattern` 1 to 256 bytes, 16 a run, compiled when `regex`, a number
  within `min`, `max`), each handle given once and of its kind; the defaults filled in; `needs`
  of what was given (`files` for a default file, `files-write` for a
  file to write). `Resolved::asked(manifest)`: the permissions
  declared for that run. `check_named(path, kind, write)`: the
  driver's check of a path again.
- `help(manifest, program)`: `--help` of a store app. A bool on by
  default shown as `--no-NAME` (`AppOption::flag`), `[default:
  --NAME]`; so in the menu of `ptyrc manage` and the list of `dev test`.
- Errors: `ErrorKind::Usage`.

## Origin

- `Origin::parse_spec("host/owner/repo#path@REF")` -> `(Origin,
  Option<Reference>)`; `Origin::parse` without `@REF`. Forms:
  `host/owner/repo`, `https://host/owner/repo[.git]`,
  `local:/abs/dir` (dev, tests); `#path` the app's directory. Hosted (an
  app of a registry server): `host/NAME`, `https://host/NAME`, NAME by
  the rule of `[app] id`, no `#path`; `http://` only for `127.0.0.1` or
  `localhost` (a port allowed). A hosted host holds a `.` or is
  loopback: `ptyrc/NAME` stays refused. `host:port/...` is accepted
  bare, so every `id()` parses again.
- `id()`: the normalised identity (`host/NAME` for hosted, loopback
  too); `dir_name()`: it as one file name; `remote()`: `Remote::Https |
  Local | Registry(base)` (`https://host`, `http://host` for loopback);
  `sub()`; `host()`; `is_hosted()`, `name()`; `registry_path()`
  (`apps/<host>/<NAME>.toml` for hosted; `:` as `_`).
- `origin::parse_base(base) -> host`: `https://HOST`, or `http://` and a
  loopback HOST; nothing after the host.
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
  the GPL-3 text at the app root (`LICENSE`, `COPYING`, bare, `.md`
  or `.txt`, case ignored: `LICENSE_FILES`), the entry,
  `assets` (each name under it by the runner's rule, `ASSET_RULE`),
  `tape`.
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
  paths in messages from the app's root. One build at a time in `work`:
  the caller's. `build::lock(path)`: an exclusive lock on `PATH.lock`
  (left), held until dropped; `ptyrc dev` and `Shipped::build` take it,
  an install holds the store's lock.
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

- `Dirs { data, state, cache, config, shipped }`: `Dirs::from_env()`
  (XDG, else `HOME`; `shipped` by `shipped::root`), `Dirs::under(root)`
  (`shipped`: `root/shipped`). Paths: `apps()`, `app(o)`, `tree(o, c)`,
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
  app without one, a script app with one: refused. So is a manifest
  whose `manifest::todos` is not empty: `[app] summary`, a reason, a
  `doc` that starts with `TODO` (the template's); `ptyrc dev check`
  notes them.
- The old tree is kept until the new one has started once: an
  `Installed` carries `previous` (the last commit that started) and
  `started`. `install::started(dirs, o)` records the start, clears
  `previous` and prunes.
- `install::resolve(dirs, ID|ORIGIN) -> (Installed, Option<Shipped>)`:
  an installed app, else a shipped one (`ptyrc/NAME`, or its id when no
  installed app has it). `install::locate(dirs, ID|ORIGIN)` ->
  `(Installed, tree dir)`: what `ptyrc app` starts. `install::all`: the
  lock's apps, then the shipped ones.
- `install::list`, `install::trees(dirs, o)`, `install::prune(dirs, o)`
  (all but the commit in use and `previous`), `install::remove(dirs,
  ID|ORIGIN)` (lock entry and trees), `install::purge_storage(dirs, o)`,
  `install::purge_grants(dirs, o)` (the table `[app."<origin>"]` of
  `permissions.toml`).
- `Lock::load`, `save`, `get(origin)`, `resolve(ID|ORIGIN)`.

## Shipped

The store apps of ptyrc itself (`plugins/store-apps/README.md`).

- `shipped::root(data)`: `$PTYRC_STORE_APPS` (absolute; not a directory:
  none); else `plugins/store-apps/` of the first ancestor of the
  program's directory that has it (a checkout); else
  `<data>/ptyrc/shipped/` (an installed ptyrc). The first found is the
  only one.
- A tree `NAME/` (its `[app] id`); origin `ptyrc/NAME`, which
  `Origin::parse` refuses: none is fetched, added or collides with one.
  `Shipped::installed()`: tier `Tier::Shipped`, commit the version of
  ptyrc, `started`. Not in `apps.lock`; `install::remove` does not know
  it.
- `shipped::list(dirs)`, `shipped::get(dirs, NAME|ptyrc/NAME)`.
- `Shipped::tree(dirs)`: a script's tree; a Rust app's, when `app.wasm`
  is in it (installed); else its build in `<cache>/ptyrc/shipped/NAME/`
  when built from the tree as it is (a hash of its files, `PTYRC_APP`,
  ptyrc's version), else `ErrorKind::Build`, "not built".
  `Shipped::build(dirs, on_build)`: `build::build` (`unconfined`: our
  own sources) into that cache, the tree copied beside `app.wasm`;
  nothing fetched. Under `build::lock` of the cache: a second caller
  waits, then takes the first one's build.

## Archive

`tar`: a hosted app's source, gzip over ustar, in memory.

- `tar::pack(tree, checked) -> Vec<u8>`: the checked files and
  directories, sorted; files 0644, directories 0755 (name with `/`),
  uid/gid 0, no user or group name, mtime 0; ustar, `prefix` for a long
  path (refused past `prefix` 155 + `name` 100); gzip of mtime 0, no
  name, OS 255. One tree, one byte string.
- `TarTree::parse(bytes) -> TarTree`, a `Tree`: inflated to
  `MAX_INFLATED` (16 MiB) at most, never more held; magic `ustar\0` `00`
  or `ustar  \0`; types `0`, `\0`, `5` only (a link, a device, a fifo,
  pax `x`/`g`, GNU `L`/`K`/`S`: the archive refused); checksums
  verified; sizes octal (no base-256); a leading `./` dropped, `./`
  itself skipped; paths by the rule of `check`; no path twice, no file
  as a directory; `MAX_ENTRIES` (1024); ends at a zero block or EOF,
  only zeros after. Parent directories implied are listed. `check` it
  then, as a git tree.
- `sha256_hex(bytes)`.

## HTTP

`http`: the only module that speaks HTTP, by `curl` from `PATH`. The
environment cleared but `PATH`, `SSL_CERT_FILE`, `SSL_CERT_DIR` (no
`HOME`: no `.curlrc`, `.netrc`; no proxy); `-q` first; `--proto` and
`--proto-redir` `=https` (`=http` for a loopback base); no redirect;
connect 15 s, all 300 s; `--max-filesize` and our own bound on stdout,
curl killed past it. URL, headers, token: curl's configuration on stdin,
never `argv`; a body: a 0600 file in a 0700 directory of `TMPDIR`,
removed after.

- `http::get(base, path, cap) -> Response { status, body, etag }`;
  `http::post(base, path_and_query, body, content_type, token, cap)`.
  `path` already encoded (`http::encode`: all but `A-Za-z0-9-._~` as
  `%XX`). A 3xx: refused; 4xx, 5xx: the server's `{"error", "hint"}`
  as message and hint, `ErrorKind::Fetch`. No `curl`: `Fetch`, a hint.
- `http::is_token`: printable ASCII, no `"` or `\`, 512 bytes at most.

## Hosted

- `hosted::fetch_archive(base, rev) -> TarTree`: `GET
  /v1/blob/<rev>.tar.gz`, `MAX_ARCHIVE` (8 MiB), its sha256 `rev`.
- `hosted::fetch_module(base, rev, sha256) -> Vec<u8>`: `GET
  /v1/blob/<rev>.wasm`, `BOUNDS.module`, its sha256 the index's, every
  import of `ptyrc`.
- `hosted::pick(origin, pointer, want)`: `choose`; no pointer: refused
  (`ptyrc store sync`).
- `hosted::resolve_name(registries, NAME) -> Option<Origin>`: the
  hosted origin of the first registry server, its copy present, that
  lists NAME.
- `push::push_tar(base, archive, description, token) -> Pushed { name,
  kind, rev, tier, token, created }`: `POST /v1/push/tar`; `rev` must be
  the archive's sha256, `name` an id. `push::push_git(base, origin,
  commit, manifest_text, manifest_path, description, token)`: `POST
  /v1/push/git`, an https git origin, a 40-hex commit; `name` must be
  the origin's id. `created`: 201.
- `tokens::get(dirs, host, name)`, `tokens::set(dirs, host, name,
  token)`: `Dirs::tokens()`, `<config>/ptyrc/tokens.toml`, 0600, by a
  rename; `["host"] name = "token"`, a git app's name its origin id.

## Registry

- `Pointer::parse(name, src)`: `origin`, `manifest`, `yanked`,
  `[[version]] { commit, tier, by, date, yanked }`; `Pointer::origin()`.
- `Registries::load(dirs)` from `store.toml` (`[[registry]] name, url |
  path | api`): `url` https or the absolute path of a git repository,
  fetched to `<cache>/ptyrc/registry/<name>/`; `path` a directory read
  as it is; `api` a registry server (`parse_base`), its index written to
  `<cache>/ptyrc/registry/<name>/`. No `store.toml`:
  `registry::default_api()`: `DEFAULT_API` (`ptyrc`,
  `https://store.ptyrc.top`), none when `PTYRC_NO_DEFAULT_REGISTRY` is
  set (tests); debug builds: `PTYRC_TEST_DEFAULT_REGISTRY` its URL in
  place; by `Registries::defaults`. An empty `store.toml`: none. `add(dirs,
  name, source, path)`, `add_api(dirs, name, base)`, `to_toml`, `save`.
- `lookup(origin)`: the first that lists it; a registry absent here is
  an error. `lookup_present(origin) -> Present { found, absent }`: absent
  ones passed over, those before `found` named.
- `Registry { name, dir, url, api }`; `pointers()`: every pointer
  under `apps/`; `remote()` (`Remote::Registry` for `api`);
  `max_age()`: `MAX_AGE_API` for `api`, else `MAX_AGE`; `api_host()`.
- `choose(Option<&Pointer>, &Want { reference, latest, force }) -> Pick
  { reference, tier }`; after fetching a name or `HEAD`,
  `registry::tier_of(pointer, commit, force)`. A hosted pointer has no
  head: `HEAD`, `--latest` or nothing listed of a tier give
  `registry::latest(pointer)`, the last version not yanked (`--force`:
  the last); none, or `@NAME`: refused.
- `Tier::Untested < Tested < Approved < Shipped`; `shipped` in a
  pointer: refused.

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

An `api` registry: `GET /v1/index.json` (`MAX_INDEX`), `format` 1 only,
unknown fields ignored, written in the same layout: per app a pointer at
its `registry_path()`, and an `index.json` of `Index`. `tar`: origin
`<base>/NAME`, the registry's own host (a `url` that says otherwise:
left out), `rev` 64 hex. `git`: `url` and `manifest_path`, whose origin
id must be `name`, `rev` 40 hex. `rev` -> `commit`, `tier`, `by`,
`date`, `yanked`; `shipped`, an odd field: the version left out; an app
that does not parse or agree: left out. Text cleaned of control
characters and bounded. `Fetched { commit }`: the sha256 of the index.
`MAX_AGE_API` 60 s.

## Index and page

- `index::build(dirs, registry, remote) -> Index`: each pointer, each
  listed commit fetched and checked (`fetch_checked`); the fields of the
  version `choose` picks: `id`, `summary`, `version`, `license`,
  `surface`, `tape`, `commit`, `tier`, `permissions { name, why, colour,
  words }`; each version's `version` or `error`. From a registry server,
  also `kind`, `description`, and per version `entry`, `wasm`, `size`
  (absent otherwise, not written). `Index::{to_json, parse, load, get}`;
  `FORMAT` 1. Read for display only, never for trust.
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
4. `Repo::open`, `fetch`, `tree` (hosted: `hosted::pick`,
   `fetch_archive`), `check`, `tier_of`; `untested`: ask the user.
5. A Rust app: `build::build` in `<cache>/ptyrc/build/<origin>/`,
   removed after; hosted, without `rustc` or `--prebuilt`:
   `hosted::fetch_module` of the `wasm` the index lists.
6. `install::install_built(..., date)`, `install::prune`.
