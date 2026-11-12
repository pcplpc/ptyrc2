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
| `confetti` | a native effect started with one call, behind the text; `--bursts N` at once at random offsets, each on a cold spot of the heatmap; persistent sprites: the app exits, the driver plays them; `--party`: a round every 3 s |
| `clock` | automatic placement, the front layer, a sprite updated in place, a draw extension |
| `flower` | a drawing tied to a word (`near=`), opened by the client, then animated by the driver |
| `fishes` | sprites in frame coordinates: they scroll with the text; the cursor read each tick; one batch per tick |
| `snake` | the screen read as a map: glyphs are walls; one sprite redrawn in place each step |
| `sand` | `Frame.is_idle` to choose a pane; a native simulation from a draw extension, with the pane's text as walls |

`scripts/demo-apps.sh` runs them on a scrolling vim buffer;
`scripts/demo-frames.sh` gives each pane of a tmux session its own
drawing. Both take `--check`.

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
- **On what**: `frame=` an id or `raw`, `active`, `largest`, `quietest`,
  `idle`. `pilot.panes()` lists the panes found on screen; a pane is
  drawable once it is `usable`: a new layout settles for 1.5 s first.
  `_cli.pick` falls back to the raw frame where there is no pane.
- **When the frame leaves**: a pane out of sight (another tmux window)
  keeps its sprites; drawing on it answers `not-usable`. A loop starts
  each step with `_cli.on_screen(frame)` and wraps it in
  `_cli.in_sight()`.
- **Idle**: `frame.is_idle` is true once 80 % of a pane has been still
  for 10 s. A big animation waits for it, and leaves when the text
  changes (`sand.py`).
- **What**: `rows` (a space is transparent), `cells` (styled runs),
  `frames` (an animation the driver plays alone), `effect` (a native
  generator), `pilot.ext(...)` (a draw extension).
- **In which layer**: the app's text is layer 0. `layer="back"` (the
  default) draws behind it, in the cells it leaves free; `layer="front"`
  over it; integers order more layers (`-2` is behind `-1`), `z` orders
  inside one. A `bg` behind the text shows under it: a sandwich.
- **How it mixes**: `blend="tint"` keeps the app's glyphs and applies
  your colours.
- **Motion from the client**: change a sprite with `Sprite.set` or
  `Sprite.move`; group the changes of one step in `with pilot.batch():`,
  they show in one repaint.
- **Reading**: `pilot.text()`, `pilot.find()`, `Frame.known()` (what a pane
  remembers outside its view), `pilot.cursor(trail=True)`,
  `pilot.events("screen", "cursor", "frames")`.

`_cli.ticks(period)` paces a loop and yields the measured step;
`_cli.hold(pilot)` keeps an app with nothing to send connected.

Read `confetti.py` and `clock.py` first, then `flower.py`, `fishes.py`,
`snake.py`, `sand.py`.

## Tests

`uv run pytest plugins/client-apps/python`: each app against a headless
driver. An app runs until ^C or the end of its session; a test stops it
with `main(argv, seconds=...)`, or with a stand-in that raises
`KeyboardInterrupt` (`confetti`).
