# ptyrc-apps

Client apps for ptyrc: small programs that draw on a wrapped terminal.

```sh
ptyrc run vim notes.md            # in one terminal
ptyrc-app confetti                # from another
ptyrc-app clock --prefer bottom-right --big
python -m ptyrc_apps.clock -s NAME
```

| app | what it shows |
|---|---|
| `confetti` | a native effect started with one call |
| `clock` | automatic placement, a sprite updated in place, a draw extension, a batch |

## Writing an app

An app is a module with `main(argv) -> int`, listed in `APPS`
(`src/ptyrc_apps/__init__.py`). `_cli` gives it the common options and a
connection:

```python
from ptyrc import Pilot
from . import _cli


def app(pilot: Pilot, args) -> None:
    note = pilot.draw(["hello"], frame=args.frame, auto="top-right", fg="yellow")
    for nth, _ in enumerate(pilot.find("TODO")):
        pilot.draw(["<-"], near="TODO", nth=nth, fg="red")
    pilot.wait_for("done", timeout=60)


def main(argv=None) -> int:
    out = _cli.parser("hello", "Say hello.")
    return _cli.run(app, _cli.parse(out, argv))
```

What to know:

- **Sprites die with the connection.** Return, crash or ^C: the driver
  removes what the app drew. Pass `persist=True` to leave something behind.
- **Where**: `at=(x, y)` with `space="view"` (fixed on the pane; negative
  from the right and bottom) or `space="frame"` (moves when the content
  scrolls); `near="word"` (follows the word); `auto="top-right"` (a free,
  still spot, chosen and kept by the driver); `cursor=(dx, dy)`.
- **On what**: `frame=` an id or `raw`, `active`, `largest`, `quietest`.
  `pilot.panes()` lists the panes found on screen; a pane is drawable once
  it is `usable`.
- **What**: `rows` (a space is transparent), `cells` (styled runs),
  `frames` (an animation the driver plays alone), `effect` (a native
  generator), `pilot.ext(...)` (a draw extension).
- **How it mixes**: `blend="over"`, `"behind"` (the app's text stays on
  top), `"tint"` (the app's glyphs, your colours).
- **Motion from the client**: change a sprite with `Sprite.set` or
  `Sprite.move`; group the changes of one step in `with pilot.batch():`,
  they show in one repaint.
- **Reading**: `pilot.text()`, `pilot.find()`, `Frame.known()` (what a pane
  remembers outside its view), `pilot.cursor(trail=True)`,
  `pilot.events("screen", "cursor", "frames")`.

`confetti.py` and `clock.py` are meant to be read first.

## Tests

`uv run pytest plugins/client-apps/python`: each app against a headless
driver.
