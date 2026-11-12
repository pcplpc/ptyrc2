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
| `flower` | flowers of random size and colours planted on cold cells next to the matches of a word (`--count N`, 0: all), the word bolded with a tint; `-a`: anywhere, next to `--count` words drawn from the screen (default 5; 0: 10 to 30, redrawn each minute), one renewed every 10 s, no bold; grown by the client, swayed by the driver; frame coordinates |
| `fishes` | sprites in frame coordinates: they scroll with the text; one scrolled out of the view goes after a few seconds and comes back behind the text; half of them shy, hiding behind the text; the cursor read each tick, the text once a second; one batch per tick. `-d`: decorations standing on the text, behind or in front of the fishes; `-b`: bubbles, each one request with a `motion`; `-r`: random species, events (a creature crossing, an object sinking, a hook); `-a`: all. Populated areas are remembered in frame cells (`_reef.Reef`) |
| `snake` | the screen read as a map: glyphs are walls; one sprite redrawn in place each step. `-e`: particles, each one request with a `motion`, golden apples, a shaded body; `-x`: foes that bite the tail off; `-l`: beams from wall to wall that burn what they cross; `-a`: all (`_arcade`) |
| `sand` | `Pilot.wait_idle` to choose a pane; a native simulation from a draw extension that reads the pane's text, its walls, by itself; `--cursor` pours on the cursor's column |
| `cat` | an app driven by events (`idle`, `cursor`, `screen`, `frames`), asleep on the socket between two things to do; it stands on lines of text in frame coordinates (`cat.ledges`), sleeps when the pane is idle, and jumps away from the cursor, from text written over it (`covered`) and back into the view after a scroll (`visible`) |
| `marks` | a highlighter: one tint per match of each pattern (`-e` regex, `-i`), in frame coordinates and on remembered lines too (`find(known=True)`), so the driver scrolls them with the text; searches of a look in one batch; topic `draw`; a display of the counts placed by the driver |
| `rain` | a native effect from a draw extension that sees the text: drops fall behind it and splash on the top of its lines, on the cursor and on the floor; less rain on columns that changed in the last 10 s |

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
  for 10 s. A big animation waits for it (`pilot.wait_idle`, `sand.py`);
  topic `idle` says when it comes and goes (`cat.py`).
- **What became of a sprite**: `Sprite.info()` has `visible` (some of
  it is in the view) and `covered` (share of a back sprite under the
  app's text); topic `draw` lists the sprites removed with rows the
  driver forgot.
- **Events, not ticks**: an app with nothing to animate waits on
  `pilot.events(..., timeout=...)`: it wakes on a change, or when its
  next step is due (`cat.py`, `marks.py`).
- **A frame replaced**: a pane keeps its frame through a resize when the
  driver knows its text again; a nearly empty one gets a new frame, and
  the sprites stay with the old, inactive one. `sand.heir` finds the
  new pane; `Sprite.update(frame=...)` moves the sprite, state kept.
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

Read `confetti.py`, `clock.py` and `rain.py` first, then `marks.py`,
`cat.py`, `flower.py`, `fishes.py`, `snake.py`, `sand.py`.

## Tests

`uv run pytest plugins/client-apps/python`: each app against a headless
driver. An app runs until ^C or the end of its session; a test stops it
with `main(argv, seconds=...)`, or with a stand-in that raises
`KeyboardInterrupt` (`confetti`).
