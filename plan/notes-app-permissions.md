# Notes: app permissions

Not done. First notes, taken with the owner on 2026-11-15; aligned on
`plan/store-model.md` (Permissions) on 2026-11-18. Where the two
differ, that plan holds: the mechanism, the list, the tasks are there.
Kept here: the problem, what the shipped apps ask, who is asking.

## Problem

Trust is the user, as tmux (`plan/design.md`, Protocol): a connection
has the whole session. A client app that draws confetti can also

- read every key of the host keyboard (topic `input`), or take them
  (`input.grab`);
- type into the wrapped program (`input.send`, `input.click`);
- run any program (`app.start`), stop or pause the other apps;
- read the screen and what the frames remember.

An app written by someone else is given all of it.

## Idea

Permissions per app, held by the driver, checked at each request.
Decided since (`plan/store-model.md`, Mechanism):

- **Default profile**: what an app has without asking: drawing on its
  own layer, its own widgets and sprites, the size of its surface.
  Reading the screen is not in it.
- **Declared**: an app declares what it needs beyond the profile. The
  first time it runs, the driver shows a screen: the app, one checkbox
  per permission, yes or no.
- **Remembered**: the answers, the noes too, are written to a plain
  file in ptyrc's configuration directory, and not asked again.
- **A no lowers**: the app calls the same methods and is answered
  less (a blank screen, the shape in place of the text, no event);
  an act with no lesser form fails `denied`.

## Permissions

The names of the first draft, and what became of them
(`plan/store-model.md`, The list):

| first draft | now |
|---|---|
| `draw` | the default profile, behind; in front: `front-widget`, `front` |
| `screen` | split by degree: none, `layout`, `shape`, `words`, `text`; none is the default. `screen.shape` answers `shape` and `words` |
| `pointer` | on its own things: the default; anywhere: `pointer`, `clicks`, `clicks-grab` |
| `keys-read` | `keys-read` |
| `keys-grab` | `keys-arrows` (named keys that are not text), `keys-grab` |
| `type`, `apps`, `session`, `debug` | never granted to a store app; the user's hand and the driver's |
| `files`, `proc`, `exec`, `network` | `files` (a file the user named); `proc` and `exec` become probes (`probe-git`, `probe-system`); the network: never |

`app.list` and one's own entry need nothing.

What the shipped apps would declare, as they are today, beside what
they read of the screen (`plan/store-model.md`, the table of degrees):

| app | extra |
|---|---|
| `snake --grab`, `bricks --grab` | `keys-arrows` |
| `pace` | `keys-read`; `files` for `--words`, `--save` |
| `flock`, `life`, `shore`, `pond`, `stars`, `cat --hunt` | `pointer`, unless a primitive serves |
| `git` | `probe-git` (today: runs `git`, reads `/proc/<pid>/cwd` and the repository) |
| `monitor` | `probe-system` (today: `/proc`, `/sys`, utmp, the systemd sessions) |
| `note`, `gallery`, `minimap` | `files` (those named) |
| `clock`, `timer`, `stars`, `pond`, `train`, `window` | `clock` |
| `manage` | the apps, a grab: the driver's own, never asked |
| the others | nothing |

`git` runs `git status` where the wrapped program stands: a repository
that is not the user's work can run code through its configuration
(filters, submodules). To settle in the probe.

## Declaring

In the catalog (`ptyrc-app --catalog`, `_catalog.py`), per app; an
option may add its own (`--grab`, `flower -a`). Done for reading the
screen: the member `reads` (the degree, the words, the options that
raise it), held by `tests/test_reads.py` (the requests made). To add
in the same form: the keys, the pointer, the rest; the imports of
`os`, `subprocess`, `socket`, `pathlib`. The launcher hands the
declaration to `app.start`. A store app declares in its manifest.

## Who is asking

A connection must be tied to an app in a way the app cannot choose.

- A child: the driver started it and knows its argv. Today a connection
  is matched by the process group of its peer pid (`Apps::attach`); a
  pid can be used again. Sturdier: a secret per child in its
  environment (`PTYRC_TOKEN`), sent as the first request.
- A client the driver did not start (`ptyrc text`, a script, `ptyrc
  python`): the user's own hand, full rights, as tmux. Decided. An app
  then escapes by connecting without its token: the token holds a
  careless app; only the sandbox holds a hostile one.
- The identity that is remembered: a store app, its origin; a native
  app, `native/NAME`.

## Asking

- A modal of the driver, the keys taken as by a grab. Drawn by the
  driver, not by a client: an app must not be able to draw a
  look-alike over it, nor answer it (`input.send` does not reach a
  grab; keep it so).
- It names the app, its origin, each permission declared in plain
  words: one checkbox each. Plain ones ticked, red ones ticked by
  hand. `space`, `enter` (start), escape (not started, nothing
  written).
- Answers: yes or no, per permission, remembered. No "once", no
  "never".
- Asked at start for what is declared and has no answer; an option or
  an update that declares more: the difference alone. A request
  beyond what was declared is never asked.
- Changed later from `ptyrc manage`: the same screen.
- Headless, or no terminal: no screen. The app starts with what it
  declares; when the file holds answers for it, with those.
- A no: the answer is lowered, with the mark `degree`; an act with no
  lesser form fails `denied`, the hint naming the permission. An app
  must go on without.

## The file

`$XDG_CONFIG_HOME/ptyrc/permissions.toml`, else
`~/.config/ptyrc/permissions.toml`; mode 0600, directory 0700, written
by the driver alone (a new file, moved in place). Read at each start
of an app. Plain enough to edit by hand:

```toml
[app."native/cat"]
allow = ["shape", "words"]

[app."native/pace"]
allow = ["shape"]
deny = ["text", "keys-read"]
```

A native app must not be able to write it: true only once apps are
sandboxed. Until then it guards against a careless app, not a hostile
one. Say so in the README.

## Order

`plan/store-model.md`, Permissions, Tasks.
