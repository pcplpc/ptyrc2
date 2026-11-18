# Notes: app permissions

Not done. Notes for a later design, taken with the owner on 2026-11-15.
Sandboxing is another topic, after this one.

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

- **Default profile**: what an app has without asking. Enough for most
  of the shipped apps.
- **Extra permissions**: an app declares what it needs beyond the
  profile. The first time it runs, the driver asks the user in a pop-up:
  the app, what it asks, yes or no.
- **Remembered**: the answer is written to a plain configuration file
  in ptyrc's configuration directory, and not asked again.

## Permissions (draft)

| name | covers | in the default profile |
|---|---|---|
| `draw` | `draw.*`, `layer.*`, `widget.*`; topics `draw`, `widgets`, `move`, `size`, `colors`, `exit` | yes |
| `screen` | `screen.*`, `frame.*`; topics `screen`, `text`, `cursor`, `frames`, `idle` | yes |
| `pointer` | topics `mouse`, `hover`, on its own sprites | yes |
| `keys-read` | topic `input` | no |
| `keys-grab` | `input.grab`, the keys named | no |
| `type` | `input.send`, `input.click` | no |
| `apps` | `app.start`, `app.stop`, `app.pause`, `app.restart`, `app.move` of others | no |
| `session` | `session.resize`, `session.colors`, `session.handoff`, `tape.*` | no |
| `debug` | `debug.*` | no |

Open: whether `screen` belongs in the default (a screen holds secrets
too; nearly every shipped app reads it). `app.list` and one's own entry
need nothing. `screen` split by degree (none, `layout`, `shape`,
`words`, `text`): `plan/store-model.md`; `screen.shape` answers the
`shape` and `words` degrees.

Outside the protocol, for the day apps are sandboxed (declared now,
enforced then): `files` (paths), `proc`, `exec` (programs), `network`.
Default: none.

What the shipped apps would declare, as they are today:

| app | extra |
|---|---|
| `snake --grab`, `bricks --grab` | `keys-grab` (the arrows) |
| `pace` | `keys-read`; `files` for `--words`, `--save` |
| `git` | `exec` (`git`), `proc` (`/proc/<pid>/cwd`), `files` (the repository) |
| `monitor` | `proc`, `files` (`/sys`, utmp, the systemd sessions) |
| `note`, `gallery` | `files` (those named) |
| `manage` | `apps`, `keys-grab`: the driver's own, never asked |
| the others | nothing |

`git` runs `git status` where the wrapped program stands: a repository
that is not the user's work can run code through its configuration
(filters, submodules). To settle with `exec`.

## Declaring

In the catalog (`ptyrc-app --catalog`, `_catalog.py`): one more member
per app, `permissions`, a list; an option may add its own (`--grab`).
The menu shows it before the app is started. A test holds each app to
what it declares (requests made; imports of `os`, `subprocess`,
`socket`, `pathlib`).

## Who is asking

A connection must be tied to an app in a way the app cannot choose.

- A child: the driver started it and knows its argv. Today a connection
  is matched by the process group of its peer pid (`Apps::attach`); a
  pid can be used again. Sturdier: a secret per child in its
  environment (`PTYRC_TOKEN`), sent as the first request.
- A client the driver did not start (`ptyrc text`, a script, `ptyrc
  python`): it is the user's own hand. Either full rights (as tmux), or
  the default profile unless started with a flag. To decide. If full,
  an app escapes by connecting from a process outside its group: this
  holds only with the token, and fully only with a sandbox.
- The identity that is remembered: the app's name and where it comes
  from (the path of `ptyrc-app`, or of the program), so that another
  program of the same name is asked again. A hash of the file is
  dearer and breaks at each upgrade.

## Asking

- A modal widget of the driver (`widget.show {modal: true}`, border
  `alert`), keys taken as by a grab: `y`, `n`, escape is no. Drawn by
  the driver, not by a client: an app must not be able to draw a
  look-alike over it, nor answer it (`input.send` does not reach a
  grab; keep it so).
- It names the app, its origin, each permission in plain words.
- Answers: once, always, no, never.
- Headless, or no terminal: no pop-up; refused, unless granted in the
  file or on the command line (`run --app NAME --allow keys-grab`).
- Refused: the request fails with a new code (`denied`), the hint
  naming the permission and how to grant it. An app must go on without.
- Asked at start for what is declared; a request beyond what was
  declared is refused without a pop-up.

## The file

`$XDG_CONFIG_HOME/ptyrc/permissions.toml`, else
`~/.config/ptyrc/permissions.toml`; mode 0600, directory 0700, written
by the driver alone (a new file, moved in place). Read at each start
of an app. Plain enough to edit by hand:

```toml
[default]
allow = ["draw", "screen", "pointer"]

[app.snake]
origin = "/home/me/.local/bin/ptyrc-app"
allow = ["keys-grab"]

[app.pace]
origin = "/home/me/.local/bin/ptyrc-app"
allow = ["keys-read"]
deny = ["files"]
```

An app must not be able to write it: true only once apps are
sandboxed. Until then it guards against a careless app, not a hostile
one. Say so in the README.

## Order

1. The permission names, the check in `rpc.rs`, the code `denied`;
   everything allowed: no change of behaviour.
2. `permissions` in the catalog; the test.
3. The token; the default profile enforced for children.
4. The file, `--allow`.
5. The pop-up.
6. The sandbox (another topic).
