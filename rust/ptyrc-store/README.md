# ptyrc-store

Store apps without the engine: the manifest, the origin, the fetch, the
tree check, the install and the registry. Plan: `plan/store-model.md`.
Not here: the Rust build, the runner, the CLI (`ptyrc store`, `ptyrc
dev` in `ptyrc-cli`), the grants.

Every fallible call returns `Result<T, Error>`; `Error { kind, message,
hint }`: `message` says what was refused (manifest errors as
`ptyrc-app.toml:LINE: [section] key: ...`), `hint` the cure.

## Manifest

- `manifest::parse(&str)`, `manifest::parse_bytes(&[u8])` -> `Manifest
  { app, permissions, limits, options }`.
- `App { id, version, summary, license, api, entry: Entry::Script |
  Entry::Rust, rustc, assets, surface, tape }`; `surface` absent:
  `Widget`.
- `Permission { name, why, words }`; names from `manifest::PERMISSIONS`;
  `words = { list = [...], why = "..." }`: 1 to 32 words, 3 to 32
  letters or digits, distinct ignoring case.
- `Limit { name, value }`: keys and defaults in `manifest::LIMITS`; a
  value must exceed its default.
- `AppOption { name, default: OptionValue, doc }`; the type is the
  default's. Names in `RESERVED_OPTIONS` are ptyrc's.

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
  A Rust app: `ErrorKind::Unsupported`, "Rust apps are not built yet";
  `install::supported(checked)` says it before.
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
  path`); `lookup(origin)`: the first that lists it. A registry by `url`
  is read from `<cache>/ptyrc/registry/<name>/`; fetching it is the
  caller's. `Registry::pointers()`: every pointer under `apps/`.
- `choose(Option<&Pointer>, &Want { reference, latest, force }) -> Pick
  { reference, tier }`; after fetching a name or `HEAD`,
  `registry::tier_of(pointer, commit, force)`.
- `Tier::Untested < Tested < Approved`.

## A `store add`, in order

1. `StoreLock::take`.
2. `Origin::parse_spec`; `Registries::load(&dirs)?.lookup(&origin)`.
3. `choose`.
4. `Repo::open`, `fetch`, `tree`, `check`, `tier_of`; `untested`: ask
   the user.
5. `install::install(..., date)`, `install::prune`.
