# Plan: store model

Not done. Planned with the owner, 2026-11-17 and 18. Follows
`plan/notes-app-permissions.md` (the check, the token, the pop-up, the
file): this is the sandbox that makes permissions hold, and the way
apps travel.

Marks: **[decided]** by the owner; **[proposed]** the plan's choice,
the owner's to overturn; **[TBD]** left to a later task.

Permissions: the mechanism is [decided] (the screen of checkboxes, yes
or no, remembered, lowered answers); the list is drafted (Permissions,
The list), its names and colours [proposed]. To be scheduled as a
series of tasks; the sandbox does not wait on it.

## Wanted

- Anyone writes an app, anyone installs it from a git URL.
- No security issue: a boundary simple enough to stay whole.
- Permissions listed before the first start; the dangerous ones red,
  each opted in by hand.
- A registry: authors add the URL of a repository and where its
  manifest is; users browse.
- Needs: (1) a real sandbox, (2) drawing at nearly native speed,
  (3) something agents write well.

## The threat

Today trust is the user: an app has the session, and the account. An
app from a stranger may

1. read the screen and the keys (secrets, passwords);
2. type into the wrapped program: a shell is the whole account;
3. reach the files, the network, other programs, as any process;
4. draw a lie (a false prompt, a false permission pop-up);
5. eat the processor, the memory, the screen.

A permission file alone stops none of 3, and so none of the rest: the
app rewrites the file.

## Decided

- The sandbox is WebAssembly, provided agents write for it well (the
  proof, milestone 2).
- Two tiers: scripts (JavaScript, QuickJS) and Rust.
- What travels is source, never a module. Rust is built on the user's
  machine. With cargo or without: taken up again 2026-11-18 (Rust).
- GPL-3.0-or-later, everything: the SDK, `ptyrc.d.ts`, the template,
  every app.
- The registry is a git repository of pointers. Three tiers bound to a
  commit: `untested`, `tested` ("it runs and looks fine"), `approved`
  ("no danger").
- Every shipped app is to become a store app.
- Where an app may draw follows from its manifest and its permissions.

## Why WebAssembly

| | boundary | speed | verdict |
|---|---|---|---|
| OS sandbox around a native process | the kernel: wide, per platform; Python and its packages inside | native | breaks often; installing dependencies is itself running foreign code |
| An embedded script language | the interpreter's host functions | slow | simple; one language for ever |
| A JavaScript runtime with permissions | V8 and its permission layer | fast | a large foreign binary; the permissions are theirs |
| **WebAssembly, no ambient authority** | the import table | near native | taken |

The boundary is a list we write: no file, no socket, no clock, no
process exists for the guest unless we hand it a function. Deny by
default costs nothing to keep.

## Model

```
origin (git) ──store add──> <data>/ptyrc/apps/<origin>/<commit>/
                              ptyrc-app.toml  sources  assets/

client ──app.start {store: origin}──> driver
driver ──spawn, token──> ptyrc-box (one per app) ──> guest
   ^                        |
   └──── socket, token ─────┘     every request checked in rpc.rs
```

- **Guest**: one core WebAssembly module. No WASI. It imports from one
  module, `ptyrc`; a module that imports anything else does not start.
- **Runner**: `ptyrc-box`, ours, a child of the driver as any app
  (`apps.rs`: pause, stop, log, the table: unchanged). It loads the
  module, connects to the socket, and is the only thing the guest can
  call.
- **Driver**: knows who asks and checks each request
  (`notes-app-permissions.md`). One place enforces, for store apps and
  native ones alike.
- Two walls: the guest cannot leave the module; the connection cannot
  ask beyond its grants. A third (milestone 6), cheap because the
  runner is one program of ours: once connected it drops to an OS
  profile (Landlock, seccomp: one socket, no file), so a hole in the
  engine is not the account.
- Not in the driver's process: a crash, a leak or a loop of a guest is
  a child's; the driver gains no dependency.
- Engine [proposed]: wasmtime (Cranelift, no async). Behind the
  runner, so it can change; `wasmi` (an interpreter, pure Rust) where
  wasmtime does not build.

### Who is asking

- A store app is started by origin, not by argv: `app.start {store:
  ORIGIN, args}`. The driver finds the installed tree itself, makes a
  token, starts `ptyrc-box` with it (`PTYRC_TOKEN`), and binds the
  origin and its grants to the connection that presents the token.
- A client cannot name another program as a store app, nor a store app
  present itself as another.
- The pop-up, when an answer is missing, is the driver's, before the
  runner is started.
- A native app is asked too (Permissions, Native apps): a token per
  child, the grants bound to the connection that presents it.
- A connection without a token is the user's hand (`ptyrc text`, a
  script, `ptyrc python`): the whole session, as today.

### Guest ABI, version 1

Host-driven: no thread, no blocking, time an argument (as the core).

| export | when |
|---|---|
| `init(args)` | once: arguments and options, JSON |
| `tick(now_ms)` | at the period asked, 30 Hz at most; session time |
| `event(topic, data)` | a subscribed topic, JSON |
| `stop()` | optional: SIGTERM; 500 ms to finish |
| `alloc`, `free` | the host writes arguments into guest memory |

| import (`ptyrc`) | |
|---|---|
| `call(method, params) -> result` | a protocol request, JSON, answered before it returns; beyond the grants: a lowered answer, or `denied` (Permissions) |
| `canvas(w, h) -> id`, `cells(id) -> ptr`, `present(id, place)` | a cell buffer in guest memory; the runner sends what changed |
| `period(ms)` | the tick wanted; 0: events only |
| `log(text)` | to the app's log |
| `random(ptr, len)` | |
| `exit(code)` | |

- [proposed] One `call`, not an import per permission: the whole
  protocol through one hole, nothing to keep in step when it grows,
  one check (the driver's). What agents learn for one client serves
  here.
- A canvas cell: 16 bytes: a scalar value (0: nothing drawn), `fg`,
  `bg` (RGB, a palette index or default, with an opacity), attributes.
  A glyph of several scalars goes through `call`.
- `present` becomes `draw.put` / `draw.update` with `cells`, the
  changed rows only. If that is not enough (Speed), a binary sprite is
  added to the protocol: nothing changes for the guest.
- Events reach the guest between calls, in order; a `call` in flight
  holds them.
- No state survives a restart. What must: `storage` (Permissions).

### Limits

By the runner, per app; the manifest may ask more, the user grants.

| | default |
|---|---|
| memory | 64 MiB |
| a `tick`, an `event` | 50 ms; three late in a row: paused, named in the manager |
| `init` | 2 s |
| `call` | 1000 a second |
| canvas cells, in all | 4 times the screen |
| log | 1 MiB, then dropped |

By the driver, per app: sprites, widgets, subscriptions (numbers with
the implementation). Past a limit: the request fails, the hint says
which.

### Never

No permission grants these; an app that needs one is not a store app.

- Run a program. Open a file by path. Open a socket.
- Read or write the grants, another app's storage, the configuration.
- Draw over, or answer, a pop-up of the driver.

The Python apps of `plugins/client-apps/` stay native, named `native`
in the menu, until each has moved (Migration). They are asked as store
apps are (Permissions, Native apps); the wall around them is missing
until then.

### Speed

- The guest's own loops: near native (Rust), an interpreter (scripts).
- A frame: a diff of the canvas, then one request. Bound to hold,
  measured at milestone 1: a 200x60 canvas repainted whole at 30 Hz
  costs the driver under 2 ms a frame and shows no tearing.
- Animations that need no thought stay effects of the core
  (`effect`, `motion`): started by one `call`, run without the guest.

## Tiers of authoring

One ABI under both. What agents need is the loop, not the format: no
toolchain to install, an error that names its cure, the screen given
back as text.

### Scripts

- The app is `app.js` and what it imports beside itself. Nothing is
  built.
- The runner loads one interpreter module shipped with ptyrc and gives
  it the script. The interpreter is a guest of ABI 1 like any other:
  the sandbox is the same, and a hole in the interpreter stays inside
  it.
- The interpreter [decided 2026-11-20]: QuickJS-ng built to
  WebAssembly, 1.0 MiB (`sdk/js/README.md`), built with ptyrc's
  release, not on the user's machine.
- The API: `ptyrc.d.ts`, one file, typed, the whole of it.

  ```js
  import { call, Canvas, period } from "ptyrc";
  export function init(args) { period(50); }
  export function tick(now) { /* draw */ }
  export function event(topic, data) {}
  ```

  No timer, no promise that outlives a call, no `fetch`: `tick` is the
  only clock. ES modules, relative imports inside the tree only.
- Speed: tens of times slower than native on its own loops. Enough
  for a clock, a game, a few hundred sprites at 30 Hz; cells are
  painted by host calls. A simulation over every cell goes to Rust.
- The review of `approved` reads what runs.

### Rust

- `ptyrc-app`, one crate for `wasm32-unknown-unknown`: `Canvas`,
  `call`, typed helpers over the protocol, the events. The same names
  as the script API. Small enough to read whole.
- [decided] Built at install, from source.
- The danger of a build is not the module it makes (that one is
  sandboxed): it is the build itself. A build script and a procedural
  macro are programs run as the user, before any sandbox; `rustc`
  alone still reads any file (`include_str!`, `#[path]`). Either way
  the compiler must run under an OS profile (milestone 6). Once it
  does, cargo inside it costs little more than `rustc` alone.
- [decided, 20] `rustc` alone now: std and `ptyrc-app` (vendored:
  ptyrc's copy, decision 21), no other crate, no build script, no
  macro crate. Nothing of the tree runs; `rustc` reads under the
  profile (`rust/ptyrc-store/BUILD-PROFILE.md`). `dev check` names what
  breaks the rule.
- Later tier: cargo under the full profile, in two steps: `cargo
  fetch --locked` (the network, no code run), then `cargo build
  --offline --frozen` confined; a build script can then do nothing but
  build. Spiked, not wired.
- Without the profile (a platform that has none): no Rust app is built
  from the store. No tier lifts this; `--force` does, and says what it
  means.
- Licences of the crates (the cargo tier): compatible with the GPL,
  checked from their manifests at `check`.
- Bounds on a build: 300 s, 4 GiB, a module of 16 MiB.
- The user needs `rustc` and the target: the install says how to get
  them. A script app needs neither.

### Tools

```
ptyrc dev new NAME [--rust]     a repository that runs: manifest, app,
                                a test, AGENTS.md (one page: the API,
                                the commands)
ptyrc dev check [DIR]           what the install and the registry
                                check: manifest, licence, sizes, the
                                build, the imports
ptyrc dev test [DIR] [--tape F] [--size CxR] [--for T] [--at T,...]
               [--deny P,...] [--key T:KEY] [--event T:TOPIC:JSON]
               [--json] [--NAME VALUE...]
                                the app on a tape or an empty screen,
                                headless, the clock an argument; the
                                screen at each T as text, the requests
                                refused or lowered, the permissions
                                lacking, the limits met. It holds what
                                it declares; --deny: a no, answered as
                                the driver answers it. The same run
                                every time
ptyrc dev run [DIR] [--NAME VALUE...]
                                in a live session; started again when
                                a file changes; SIGTERM, SIGINT: the
                                app stopped (`stop`), then the end
ptyrc dev entry [DIR]           the file to add to the registry
```

- Errors carry the hint, as the protocol does: what was refused, the
  line of the manifest that would ask for it.
- Examples, each in both tiers: `confetti`, `sand`, `snake`.

## Permissions

The mechanism is [decided] (owner, 2026-11-18), as on a phone: asked
once, a yes or a no per permission, remembered, and a no does not
break the app. The list follows it; its names and colours are
[proposed]. Not done: to be scheduled as a series of tasks.

### Mechanism

**Declared.** The manifest declares each permission with its reason
(a native app: its catalog entry). Nothing beyond the default profile
is had without a grant. What is not declared is never asked and never
granted.

**Asked.** Before the first start the driver shows the pop-up: a
screen between the request to start and the start.

```
┌ cat ─ native ──────────────────────────────────┐
│ This app asks to:                              │
│                                                │
│ [x] see where the text is, not what it says    │
│ [x] see these words: food, fish, toy           │
│                                                │
│ space: tick   enter: start   esc: cancel       │
└────────────────────────────────────────────────┘
```

- It names the app, its origin, its tier and commit (a native app:
  `native`), and each permission declared, in plain words, with the
  app's reason: one checkbox each.
- [decided] An answer is yes or no, per permission. No "once", no
  "never": one screen, always remembered.
- Plain permissions come ticked; red ones come unticked and are
  ticked by hand.
- The degrees of a ladder (`shape`, `words`, `text`) are one line
  each; unticking one unticks those above it.
- `enter`: the answers are written, the app starts with them. `esc`:
  nothing is written, the app is not started.
- An app that declares nothing beyond the default profile starts
  without the screen.
- The driver's own modal, above every layer, the keys taken: no app
  covers it, draws its mark, or answers it (`input.send` does not
  reach it). Every start goes through `app.start` (the menu, `ptyrc
  app`, `run --app`, the hotkey): one screen for all.

**Remembered.** The driver writes the answers to `permissions.toml`,
keyed by origin; a no is kept as a yes is, and is not asked again. The
guest cannot reach the file.

```toml
[app."example.org/someone/sand-garden"]
allow = ["shape"]
deny = ["words"]

[app."native/flower"]
allow = ["shape", "words"]
deny = ["text"]
```

- Asked again only for what has no answer: an update, or an option
  (`flower -a`: `text`), that declares something new. The screen then
  shows the difference alone.
- Changed at will: an entry per app in `ptyrc manage` opens the same
  screen, filled from the file. A change holds from the next request
  on.
- `ptyrc store rm --purge` removes the answers.

**A no lowers, it does not break.** [decided] An app denied a
permission still calls the same methods.

- Reading: the answer is cut to what was granted, in the same form.
  The degrees of the screen are a ladder; a request above the grant
  is answered at the grant.

  | asked | granted | answered |
  |---|---|---|
  | `screen.text`, `screen.cells`, `frame.text` | `words`, `shape` | the same rows, as `screen.shape` gives them; no style |
  | `screen.shape` with words | `shape` | the shape, no word shown |
  | `screen.shape`, `frame.heat`, `frame.idle`, `screen.cursor` | less than `shape` | a blank screen of that size; all cold; no cursor |
  | `screen.find`, `screen.wait` on a pattern | less than `text` | the declared words when `words` is held; else no match |
  | `frame.list`, `frame.get` | less than `layout` | the app's own surface, as one frame |
  | a topic of a degree not held (`text`, `cursor`, `idle`, `hover`, `input`) | | subscribed; no event comes |

- A lowered answer bears a mark, `"degree": NAME` (the degree it was
  cut to; `"none"`): an honest app tells a blank screen from a refused
  one, and may say so. The mark tells the app nothing it could not
  ask.
- Drawing: where a lesser form exists, it is taken: in front, denied,
  is drawn behind.
- An act with no lesser form (a grab of keys, a named file, a probe,
  a limit above the default) fails `denied`; the hint names the
  permission. The app goes on without.
- A method closed to store apps whatever the grants (Ways out) fails
  `denied`, never lowered.

**Headless.** [decided] No screen can be shown. The app starts with
what it declares, granted; when the file holds answers for it, those
are used instead, and what has no answer is granted as declared.
Nothing is written. Stricter for strangers' apps (`untested`: refused
unless `--allow`): [TBD] with the registry.

**Limits.** More than a default limit (memory, time) is declared,
asked and remembered the same way; a no keeps the default.

### Native apps

The shipped Python apps are asked as store apps are, from the first
task on: the screen, the file and the lowered answers do not wait for
the sandbox.

- Declared in the catalog (`_catalog.py`): `reads` (the degree, the
  words, the options that raise it) is there; the keys, the pointer
  and the rest are to add in the same form.
- Who asks: the driver gives each child a token (`PTYRC_TOKEN`); the
  client library presents it first; the grants are bound to that
  connection. Keyed `native/NAME` in the file.
- What it holds: a careless app, not a hostile one. A native app is a
  process of the user: it may connect without its token, read the
  file, or do anything else the account can. Only a store app is
  walled. The README says so.

### The list

[proposed] Names, colours and lesser forms; from the degrees below.
Default: had without asking, not shown. Plain: shown, ticked. Red:
shown, unticked, ticked by hand.

| name | opens | colour | denied |
|---|---|---|---|
| (default) | drawing on its own layer, behind; its own sprites and widgets; the size of its surface; `tick`; the keys of its focused widget; the pointer on its own things; session time; assets; a result to its caller | | |
| `layout` | the frames and panes as rectangles | plain | its surface as one frame |
| `shape` | where there is ink; the heat; the cursor; idle; that the text changed | plain | a blank screen |
| `words` | `shape`, and the words listed | plain, the words shown | `shape` |
| `text` | every cell and its style; titles; what the frames remember; `find` | red | `words`, else `shape`, else blank |
| `front-widget` | a widget in front, with the driver's border and the app's name | plain | behind |
| `front` | bare sprites in front; a tint or an opacity over the text | amber | behind |
| `screen-wide` | `surface = "screen"`: anywhere, not its frame alone | amber | its frame |
| `keys-arrows` | a grab of named keys that are not text | plain, the keys shown | `denied` |
| `keys-grab` | a grab of other named keys; a modal | red | `denied` |
| `keys-read` | every key (`input`) | red | no event |
| `pointer` | movements anywhere (`hover`, `move`) | plain | no event |
| `clicks` | clicks anywhere (`mouse`) | red | no event |
| `clicks-grab` | clicks taken from the wrapped program | red | `denied` |
| `clock` | the wall clock, a zone | plain | session time |
| `storage` | a directory of its own, a quota | plain | `denied` |
| `files` | a file the user named: read; `files-write`: written | the user's hand (principle 4); written: red | `denied` |
| `probe-git`, `probe-system` | the answers of the probes | plain; red with a sink | an empty answer |
| `limit-*` | more memory, more time | plain | the default |
| never | typing and clicking into the program; `session.*`, `tape.*`, `debug.*`, other apps; the network; the clipboard; running a program | not grantable | `denied` |

- A permission held with a sink is said in one line of the screen
  (principle 6). No sink exists at first.
- Every method and topic of the protocol is to be listed with the
  permission that opens it; one not listed is closed to store apps.

### The owner's ideas, 2026-11-18

1. **Reading the screen has degrees.** Many apps need where the text
   is, not what it says. `shape`; `shape` and some words named in the
   manifest (`cat`: `food`, `fish`, `toy`, nothing else); the full
   text. Three permissions, not one.
2. **An app does not choose its layer.** It draws on a layer of its
   own, given by ptyrc: by default behind the text. The user may put
   an app in front (`--front`, per app). An app that wants to be in
   front by itself asks for a privilege: drawing over the text without
   warning confuses.
3. **The features are ptyrc's.** Widgets, frames, layers, front and
   back, placement: generic primitives on ptyrc's side. Apps use them;
   they do not rebuild them. Permissions then sit on primitives.
4. **The mouse is a permission of its own**, apart from the screen
   and from the keys: its movements, its clicks.
5. **An app that just draws reads nothing**: neither the shape nor
   the content of the text (`stars`). Reading nothing is the default.

### Principles

Drawn from these and from the threat. [proposed]

1. **A permission is a property of a primitive.** The guest has no
   power of its own; the list of permissions is the list of
   primitives whose use must be told to the user. A feature built in
   the app instead of asked of ptyrc is one the user cannot be told
   about, nor take back.
2. **Use without reading.** A primitive may use what the app may not
   read, as long as nothing comes back. "Draw this next to the word
   `flower`": the driver finds the word, places, follows the scroll;
   the app learns nothing, and asks nothing. Each such primitive
   removes a reason to read:

   | the app wants | the primitive | the app learns |
   |---|---|---|
   | to sit next to a word | an anchor on a word or a pattern | nothing |
   | a free corner | placement by the heat (`prefer`) | nothing |
   | to stay behind the text | its layer | nothing |
   | to follow the scroll | an anchor in the frame | nothing |
   | to rest on, or flow around, the text | an effect of the core that looks (`sand`, `rain`) | nothing |
   | to follow or flee the cursor | an anchor on the cursor; a motion that avoids it | nothing |
   | to keep quiet while the user types | gating by idle, in the driver | nothing |
   | to fade where the text is busy | a blend by heat | nothing |

3. **What comes back is read.** An answer of such a primitive (was it
   placed, where, is it hidden) is worth the permission that would
   have told it: without it, no answer. Errors and hints too: a hint
   that names a frame, an app, a word is scrubbed for a store app.
4. **The user's hand is not a permission.** What the user gives at the
   start is granted by giving it: a file named, a word to look for
   (`marks PATTERN`), `--front`, the frame. These options are ptyrc's,
   the same for every app, parsed by the launcher, not by the app:
   `--front` / `--back`, `--frame`, `--prefer`, `--alpha`. What the
   manifest asks as its default (`layer = "front"`) is the app's
   wish: that is the permission.
5. **An app sees its own.** Sprites, widgets, layers, apps, frames:
   a store app lists and touches its own objects only. No app reads
   what another drew (else one with a source and one with a sink make
   a pair). `screen` answers the wrapped program's cells, never the
   composed picture.
6. **A source is harmless without a sink.** Red: a sink; a capture of
   keys; a source of the top degree; a source together with a sink,
   said in one line.
7. **What is in front can be told apart.** Whatever covers the text
   is either chrome of the driver (a widget's border, bearing the
   app's name) or was asked for. One key of the driver, never an
   app's: hide everything store apps drew, show who drew what. The
   cure for a lie and for a nuisance alike.

### Degrees: what an app learns of the screen

| degree | answers | leaks |
|---|---|---|
| none | the size of the surface it was given, and when it changes | nothing |
| `layout` | the size; the frames and panes as rectangles, without key or title; usable or not | how the screen is split |
| `shape` | where there is ink; the heat; the backdrop colour; the cursor; idle; that something changed | lengths of words and lines, the rhythm of typing |
| `words` | `shape`, and where the words of the manifest are | those words, and that they are there |
| `text` | every cell and its style; titles and frame keys; what the frames remember; `find` of anything | everything shown |

- None is the default (owner): an app that just draws is told the
  size of its surface and no more. It draws behind the text all the
  same: the layer hides what the text covers, the app does not know
  where. `layout`, `shape`: plain; `words`: plain, the words shown in
  the pop-up; `text`: red, or plain only while no sink exists. [TBD]
- `words` must not add up to `text`. Whole words only (bounded by
  what is not a letter or a digit), literal, no pattern, three
  characters at least, 32 at most, case ignored (`screen.shape`). The
  alphabet as 26 words gives nothing; a dictionary does not fit.
- A word the user typed at the start is the user's (principle 4): not
  counted, not shown as the app's.
- Titles and frame keys are text: `ranger:<dir>` is a path. Under
  `text`, a frame has an id and a rectangle.
- The `hello` of a store app is cut to its degree: no `argv`, no pid,
  no session name under `text`.
- Timing is a source: topics that fire when the text changes belong to
  `shape`; under it, only `tick` and the size.
- In the protocol: `screen.shape` (design, Protocol), the answer of
  `shape` and `words`; no permission checked yet. [decided] The name
  `screen.shape`. [decided] One `#` per inked cell, a wide glyph
  `##`; a cell of a matched word as on screen. [decided] 32 words at
  most, each 3 to 32 letters or digits, whole, literal, case ignored.
  [decided] Box-drawing is ink. [decided] The list rides on the
  request (`words`) until the manifest exists; each app declares it
  in the catalog (`_catalog.READS`). The effects have it already
  (`Under::ink`, `heat`, `row`).
- Still needed, not done: the answers lowered to the grant
  (Mechanism): `screen.text` and `screen.cells` cut to the shape,
  `screen.find` held to the declared words, the topics filtered per
  degree, the mark `degree`; colour and heat under `shape`
  (`screen.shape` answers neither); a shape of what the frames
  remember.

Per shipped app, as measured (`_catalog.READS`, held by
`tests/test_reads.py`; `layout` is not told apart from none there):

| degree | apps |
|---|---|
| none | `clock`, `timer`, `ask`, `note`, `gallery`, `globe`, `train`, `window`, `fire`, `monitor`, `git`; `rain`, `snow`, `sand` (effects of the core); `fishes --tank` |
| `shape` | `snake`, `bricks`, `life`, `stars`, `pond`, `shore`, `flock`, `fishes` (the cursor), `confetti` (the heat) |
| `words` | `cat` (`food`, `fish`, `toy`); `flower` (`flower`; `--word`: the user's) |
| `text` | `pace` (what is typed against what is shown); `marks` (the user's patterns, in what the frames remember); `minimap` (what the frames remember); `flower -a`; `flower --word` with a word outside the rules |

To lower further:

- `flower`: an anchor on a word (principle 2): none.
- `confetti`, `fishes`, `stars`, `pond`, `shore`: a primitive that
  uses the heat, the ink or the cursor without answering: none.
- `minimap`: a shape of what the frames remember: `shape`.
- `marks`: the patterns are the user's (principle 4): a `find` held
  to them, not `text`.
- `flower -a`: runs of ink in place of words: `shape`.

### Degrees: where an app draws

- **Its layer.** Each app has a band of its own, between two bands of
  the driver; `z` orders inside it. A store app names no layer:
  `layer.define` and the fixed names are not its to use.
- **Behind**, the default: the app's text stays whole. Nothing asked.
- **In front, by the user**: `--front`, the manager. Nothing asked.
- **In front, by the app**: a permission. Degrees inside it:
  - a widget in front: a box with the driver's border and the app's
    name; the least;
  - bare sprites in front: over the text with no chrome; more;
  - what changes the text without covering it (a `tint`, an `alpha`
    that drowns it, ink the colour of the backdrop): counts as front.
- **How far**: `surface`: its widgets; the frame it was put on; the
  screen.
- **How long**: a sprite that outlives its app (`persist`) is kept
  attributable and bounded; likely not for store apps at first.
- **Modal**: a modal widget takes the keys: a capture; counted with
  the keys, not with drawing.
- Reserved to the driver, never granted: the border `alert`, the mark
  of its pop-ups, the top band.

### Degrees: keys

| | |
|---|---|
| keys of its own focused widget | the user focused it: nothing asked |
| a grab of named keys that are not text (arrows, function keys) | plain, the keys shown |
| a grab of other named keys | red |
| a modal | as a grab of everything, while shown |
| every key read (`input`) | red |

One topic, `input`, carries the first and the last today: the check
must tell them apart.

### Degrees: the mouse

Its own permissions (owner), apart from the screen and the keys.

| | learns | |
|---|---|---|
| nothing | | the default |
| the pointer on its own sprites and widgets: enter, leave, a click there | that the user pointed at it | nothing asked, or the least [TBD] |
| movements anywhere (`hover`, `move`) | where the user looks and reads; with `shape` or `text`, what | a permission |
| clicks anywhere (`mouse`) | what the user selects and presses in the wrapped program | a permission, above movements |
| clicks taken from the wrapped program | as a grab of keys: a capture | above both |

- Movements and clicks are two: `cat --hunt` follows the pointer and
  needs no click; `timer --click` needs a click on itself and no
  movement elsewhere.
- Primitives that use the pointer without reading it (principle 2):
  a sprite that follows, flees or looks at the pointer; a highlight
  on hover. `cat --hunt`, the fishes, `flock` may then ask nothing.
- The shipped apps, first guess: on its own things: `timer --click`,
  `life`, the widgets (`_widgets.py`); movements anywhere: `cat
  --hunt`, `flock`, `pond`, `shore`, `stars` (hover), unless the
  primitive serves; clicks anywhere: none.

### Ways out, and the rest

- Typing and clicking into the wrapped program (`input.send`,
  `input.click`): the account. Never, or native only.
- `session.*`, `app.*` on others, `tape.*` (a tape is the screen and
  the keys written to a file), `debug.*`: the user's and the
  driver's; never for a store app.
- The network; a named file written; the clipboard (never).
- Harmless on their own: a result to the caller, private storage, the
  wall clock, assets.
- Probes (`probe.git`, `probe.system`): sources about the machine,
  not the screen; degrees of their own.

### Colours

[decided] Three (2026-11-20): plain (listed, one yes for
all); amber (can confuse, cannot leak: in front, the whole screen, a
grab of arrows; listed apart, one yes); red (can leak or act: each
ticked by hand).

### Tasks

To schedule, in this order; each leaves the tree working.

1. The token for children; the grants bound to a connection; the
   check in `rpc.rs`, everything allowed.
2. The catalog declares the rest (keys, pointer, front) beside
   `reads`; the launcher hands the declaration to `app.start`; the
   test of `test_reads.py` widened to it.
3. `permissions.toml`: read, written, the headless rule.
4. The lowered answers of the screen, the topics filtered, the mark
   `degree`; `denied` for the acts.
5. The pop-up; the entry in `ptyrc manage`.
6. The rest of the list, each with its primitive: front and the layer
   per app, the keys told apart, the pointer, the clock, storage,
   files, probes.

Beside them:

- Confirm the names, the colours and the default profile of The list.
- Move what an app rebuilds into a primitive where one is missing
  (principle 1; "To lower further", above).
- The generic options of the launcher (principle 4): take `--front`,
  `-a` and the like out of the apps.
- List every method and topic of the protocol with the degree that
  opens it; a method not listed is closed to store apps.
- The owner's first leaning, 2026-11-17: no sink at first; reading
  all keys allowed, red.
- `notes-app-permissions.md`: the first notes, aligned on this plan;
  where they differ, this plan holds.

### Drawing is not harmless

Milestone 0, before any of this is called safe:

- Text of a sprite or a widget reaches the terminal as glyphs only: no
  control character, no escape. To audit: OSC 52 (the clipboard),
  OSC 8 (a link carries data out), the title, the bell.
- A look-alike: a false `sudo` prompt, a false ptyrc pop-up. The
  driver's modal is above every layer; it bears a mark no app can
  draw there. The layer, `surface` and the key that hides all
  (principle 7) keep an app where it was put.

## Manifest

`ptyrc-app.toml`, at the root of the repository or where the origin
says (`URL#path`).

```toml
[app]
id = "sand-garden"                 # shown; the identity is the origin
version = "0.3.0"
summary = "Sand that piles on your text."
license = "GPL-3.0-or-later"       # no other is accepted
api = 1                            # ABI version
script = "app.js"                  # or: rust = "src/lib.rs"
rustc = "1.85"                     # Rust only: the oldest that builds
assets = "assets/"                 # read-only, nothing asked
surface = "widget"                 # widget | frame | screen
frame = "largest"                  # active (default) | largest | quietest | idle | raw
tape = "demo.tape"                 # a replay, for the store page

[permissions]                      # names TBD; each with its reason
shape = "the cats walk on the lines"
words = { list = ["food", "fish", "toy"], why = "they go to them" }

[limits]                           # only what exceeds the defaults
memory-mib = 128

[options]
rate = { type = "int", default = 20, doc = "grains a second" }
```

- The identity is the origin: `host/owner/repo[#path]`. No global
  name, nothing to squat; `id` is for the eye. `ptyrc app ID` when one
  installed app has it, else the origin.
- `[options]` is the catalog of the app: the menu of `ptyrc manage`
  and `--help` come from it; the guest receives them parsed.
- `surface` [proposed]: `widget`: its own boxes; `frame`: the frame it
  was started on or moved to; `screen`: anywhere. Shown in the pop-up,
  enforced by the driver. No layer is named: behind, unless the user
  says otherwise or the app holds the permission (Permissions).
- `[options]` are the app's own. The layer, the frame, the corner,
  the opacity are options of ptyrc, the same for every app.
- `frame` [as built]: the frame a start without `--frame` draws on, a
  name the driver resolves to one frame; absent: `active`. `all` is
  refused: a surface is one frame (`surface = "screen"`: anywhere).
- `assets`: reached by name through `call`, read-only.
- Bounds: the manifest 64 KiB; the tree 8 MiB and 512 files; a script
  1 MiB. Regular files only: a symlink, a submodule, a path with `..`:
  refused.
- GPL: `license` is `GPL-3.0-or-later` and the tree holds the licence
  text; else refused by `check`, the install and the registry.

## Install

```
ptyrc store add ORIGIN[@REF] [--latest] [--force]
ptyrc store rm ID|ORIGIN [--purge]      --purge: its storage, its grants
ptyrc store update [ID|ORIGIN...]
ptyrc store ls                          origin, id, version, commit, tier
ptyrc store search WORD ; ptyrc store show ID|ORIGIN
ptyrc app ID|ORIGIN [OPTION...]         start it (a native name first)
```

`add`:

1. The version: `@REF` as given; else the one the registry lists with
   the best tier, the newest among equals; not listed, or `--latest`:
   the head, `untested`.
2. Fetch without a work tree: a bare fetch of that commit, depth 1,
   `https` only (a local path for `dev`), no prompt, no system or user
   configuration, no submodule, no hook, no filter, no LFS. The files
   are read with `git cat-file`; nothing is checked out.
3. Check, as `ptyrc dev check`. Rust: build.
4. Copy to `<data>/ptyrc/apps/<origin>/<commit>/`, read-only; write
   the lock.
5. Nothing is granted yet: the pop-up comes at the first start.

- `untested` is said in red and wants `y`; `yanked` is refused unless
  `--force`.
- `update`: by hand, never by itself. The same rule picks the version:
  an app held at an `approved` commit does not move to an untested
  head unless `--latest`.
- The old tree is kept until the new one has started once.

Where things are:

| | |
|---|---|
| `<data>/ptyrc/apps/<origin>/<commit>/` | the tree; `app.wasm` when built |
| `<data>/ptyrc/apps.lock` | origin, commit, tier and date at install |
| `<state>/ptyrc/storage/<origin>/` | the app's own storage |
| `<cache>/ptyrc/registry/`, `<cache>/ptyrc/git/` | the registry, the fetches |
| `<config>/ptyrc/permissions.toml` | the grants |
| `<config>/ptyrc/store.toml` | the registries, in order |

All 0700/0600, written by `ptyrc` and the driver alone.

## Registry

[decided] A git repository of pointers (`ptyrc-store`). It holds no
code and is no root of trust for the sandbox: the client reads the
manifest itself. What it adds is a word on a version.

One file per app, `apps/<host>/<owner>/<repo>[/<path>].toml`:

```toml
origin = "https://example.org/someone/sand-garden"
manifest = "ptyrc-app.toml"

[[version]]
commit = "3f9c..."
tier = "approved"
by = "maintainer"
date = 2026-11-17

[[version]]
commit = "a41b..."
tier = "untested"
```

| tier | means | set by |
|---|---|---|
| `untested` | proposed and listed; nobody vouches | a pull request, merged |
| `tested` | it runs and looks fine | someone other than the author |
| `approved` | no danger: that version was read | a maintainer |

- A tier is a word on a commit, never on an app. A version not listed
  is `untested`, whatever the earlier ones were.
- `yanked = true` on a version or an app.
- To list an app: `ptyrc dev entry` writes the file; a pull request
  adds it.
- CI on a pull request: one file changed per app; the manifest is at
  that commit and passes `ptyrc dev check`; a tier above `untested` is
  set by who may.
- CI on merge: `index.json` (origin, id, summary, version, licence,
  permissions, surface, versions and tiers) and a static page from it:
  the summary, the permissions in their colours, the tier, the tape
  played.
- The client fetches the repository, shallow, or `index.json`.
  `store.toml` may name other registries (a team's own); the first
  that lists an origin speaks for it.

[TBD], later: who may say `tested` and what a tester writes down;
whether tiers are signed.

## Migration

[decided] Every shipped app a store app in the long run. What each
asks today (`plugins/client-apps/python`; from its imports and
requests, not a full reading):

| app | beyond drawing and reading the screen |
|---|---|
| `confetti`, `rain`, `snow`, `fire`, `globe`, `flower`, `marks`, `sand`, `fishes` | nothing |
| `flock`, `life`, `shore`, `cat` | the pointer (hover), idle |
| `snake`, `bricks` | a grab of the arrows (`--grab`) |
| `timer --click` | the mouse on its own sprite |
| `ask` | the keys of its focused widget; an answer to its caller |
| `clock`, `timer`, `stars`, `pond`, `train`, `window` | the wall clock, a time zone |
| `pace` | every key; a words file, a save file |
| `gallery` | shipped pictures; files and directories named |
| `minimap` | a file named (`--file`) |
| `note` | reads and rewrites the files named |
| `git` | runs `git`; `/proc/<pid>/cwd`; the repository |
| `monitor` | `/proc`, `/sys`, utmp, the systemd sessions |
| `manage` | the apps, the keys: the driver's own, never a store app |

None types, reaches the network, or starts a program. What the host
must offer for them, each to be named and coloured by the permission
task:

- **The wall clock** and the offset of a zone at a date: the host has
  the zone database, the guest does not.
- **A result**: one value handed back to whoever started the app.
- **Storage**: a directory of its own, a quota, reached by name.
- **Named files** [proposed]: a path the user gave (an argument, the
  menu), never one the app made up. The runner opens it and hands the
  guest a handle. Read, or written: that file only.
- **Assets**: the app's own tree, read-only.
- **Probes** [proposed], for `git` and `monitor`: the part that
  touches the machine moves into ptyrc and comes back as data
  (`probe.git`, `probe.system`). Written once, by us; an app gets the
  answer, not the means.
- `--print` (a look on stdout, for the tests) becomes `ptyrc dev
  test`.

Waves: the first row (the examples); the pointer and the games; the
clocks; `ask`; `pace`; named files (`note`, `gallery`, `minimap`); the
probes (`git`, `monitor`). A Python app is removed when its twin is at
least as good and its tests are carried over.

## Code

| | |
|---|---|
| `rust/ptyrc-box` | the runner: engine, ABI, limits, canvas diff. Its own binary: `ptyrc` and the driver do not link the engine |
| `rust/ptyrc-store` | manifest, fetch, check, lock, registry, the Rust build. No engine |
| `rust/ptyrc-server` | the token, `app.start {store}`, the check, `denied`, the pop-up, a layer per app, `surface`, the answers cut to a degree, the key that hides all, the per-app limits |
| `rust/ptyrc-cli` | `store`, `dev`; `app` resolves store apps |
| `sdk/rust/ptyrc-app` | the guest crate |
| `sdk/js` | the interpreter guest, its prelude, `ptyrc.d.ts` |
| `sdk/template` | what `dev new` writes |
| `plugins/store-apps/` | the shipped apps as they move, each a tree with its manifest |

No `unsafe` of ours outside `ptyrc-server`; no async runtime.

## Tests

- `ptyrc-store`: manifests good and bad; fetch against local bare
  repositories (a symlink, a submodule, `..`, a tree too large, a
  commit that is not there); the choice of a version by tier.
- `ptyrc-box`: the ABI against small guests; `dev test` gives the same
  text twice.
- **Hostile guests**, a standing suite (`rust/ptyrc-box/tests/hostile/`),
  each must end contained, the session unharmed: a loop in `tick`; a
  loop in `init`; memory grown without end; an import outside `ptyrc`;
  a WASI import; a flood of `call`; a canvas of 2^32 cells; a pointer
  out of its memory handed to `call`; a million sprites; escape bytes
  and control characters in every text member; a look-alike of the
  pop-up; a recursion to the end of the stack; a script that defines
  its own `ptyrc` module. A hole found later becomes a guest here.
- The driver: a store app cannot present another's token; a request
  beyond the grants is lowered or `denied`, each row of the table of
  Mechanism; a no is kept and not asked again; an option that
  declares more is asked, the difference alone; headless: the
  declared, or the file; the pop-up is not answered by
  `input.send`, not covered by a sprite; an app lists no object of
  another; under `text` no answer, event, error or hint holds a
  character of the screen (a screen of known text, every method
  called, the answers searched); the declared words do not add up.
- Every moved app: the tests of its Python twin, through `dev test`.

## Milestones

0. **Ground**, in the driver, no store yet: the token and `app.start
   {store}`; the check in `rpc.rs`, everything allowed; the audit of
   drawn text; the modal no app can cover. Shared with the tasks of
   Permissions (1), which need no store.
1. **Runner**: `ptyrc-box`, ABI 1, the limits, the first hostile
   guests; `ptyrc-app` enough for `confetti`; the speed bound
   measured.
2. **Scripts and tools**: the interpreter guest, `ptyrc.d.ts`, `dev
   new`, `check`, `test`, `run`; `confetti`, `sand`, `snake` in both
   tiers. **The proof**: an agent given the template and nothing else
   writes a new app that passes `check` and `test` in one session.
   Failed: the choice of WebAssembly is taken up again before going
   on.
3. **Install**: the manifest, `store add`, `rm`, `update`, `ls`, the
   lock, `ptyrc app` and the menu of `ptyrc manage`. Script apps only.
4. **Permissions**: the tasks of Permissions, for native apps first
   (they need no store, and may run beside 1 to 3), then store apps
   from their manifest. Declared, asked, remembered, lowered; the
   degrees of reading; a layer per app, `surface`, the key that hides
   all; the primitives that use without reading, as the apps need
   them.
5. **Registry**: the repository, its CI, the index, the page, `store
   search`, `show`, the tiers in the client.
6. **OS profile**: the runner's, the build's; then Rust apps from
   the store, built at install with cargo.
7. **Migration**, by waves, each host function with its wave.

1 to 3 want nothing from the permission list. Until 4 is done a store
app runs with the default profile and no more; the README says the
store is not yet safe for strangers' apps.

## TBD

- The names, the colours and the default profile of The list, to
  confirm; the host functions of Migration with it.
- Headless and a stranger's app: granted as declared, or refused
  unless `--allow`.
- cargo under the OS profile, as proposed, or `rustc` alone.
- `tested`: who, what is written down; tiers signed or not.
- macOS: the runner is portable, its OS profile is another.
