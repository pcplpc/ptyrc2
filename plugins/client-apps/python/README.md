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
| `confetti` | the pops of the first ptyrc: finite animations of 3 by 3 cells, four colours a burst, behind the text; `--bursts N` at once at random offsets, each shot on a cold spot of the heatmap; a round every 3 s until interrupted; `--one-shot`: one round of persistent sprites: the app exits, the driver plays them. `--fancy`: big explosions, a native effect started with one call; `--random`: pops of several kinds, explosions of random size, scale, shape and colours; `--fireworks`: a rocket first, one sprite with a `motion` that brakes; `--sparkle`: twinkles; `-a`: all |
| `clock` | automatic placement, the front layer, a sprite updated in place, a draw extension |
| `flower` | flowers of random size and colours planted on cold cells next to the matches of a word (`--count N`, 0: all), the word bolded with a tint; `-a`: anywhere, next to `--count` words drawn from the screen (default 5; 0: 10 to 30, redrawn each minute), one renewed every 10 s, no bold; grown by the client, swayed by the driver; frame coordinates |
| `fishes` | sprites in frame coordinates: they scroll with the text; one scrolled out of the view goes after a few seconds and comes back behind the text; half of them shy, hiding behind the text; the cursor and the text told by the driver (`_cli.Sight`), not asked; one batch per tick. `-d`: decorations standing on the text, behind or in front of the fishes; `-b`: bubbles, each one request with a `motion`; `-r`: random species, events (a creature crossing, an object sinking, a hook); `-a`: all. Populated areas are remembered in frame cells (`_reef.Reef`) |
| `snake` | the screen read as a map: glyphs are walls, read again when the topic `text` says so (`_cli.Sight`); one sprite redrawn in place each step. `-e`: particles, each one request with a `motion`, golden apples, a shaded body, the score; `-x`: foes that bite the tail off; `-l`: beams from wall to wall that burn what they cross; `-a`: all (`_arcade`) |
| `bricks` | a brick breaker that plays alone on the screen read as a map: the ball bounces off the glyphs, the sides of the view and the bricks; the top and the bottom are open. The ball is half a cell high, `▀` or `▄`. One racket, `(━━━)`, moved in two dimensions and hit on either side: it foresees the ball, gets in its way before it leaves, and takes the shot that breaks the most bricks; idle, it hits a safe ball on purpose when that breaks more. Text does not stop a racket (under it, it is hidden, and the ball meets the text); it keeps off the text when it can. Bricks of 3 cells, `⟦=⟧`, in frame cells, laid a group at a time all over the view, far from each other, where a ball gets: two bricks in three in large groups, a colour a row: medium walls of 2 rows by 3 to 5 bricks (a row or a pile where the place is tight), now and then a big wall of 3 or 4 rows by 6 to 8; half in groups of 2 or 3; a group goes 10 to 20 s after it is laid, a second later for each brick over three, dim for its last second. The last ball out: the score at 0. 30 s with no brick broken: the game starts again on a new map, the scores stay. `-r`: hard `⟦≡⟧`, loose `⟨=⟩`, bumper `⟦o⟧` and bomb `⟦*⟧` bricks; `-b`: capsules, two broken bricks in five: two balls for one (three times in five), wide, twin, fire, small, fast; `-m`: two rackets, two scores, bricks first; face to face, one keeps away for 2 to 3 s; `-e`: particles, a trail, combos, a best score; `-a`: all (`_bricks`); `--grab`, not in `-a`: the arrows of the host keyboard move the racket (with `-m`, the bottom one; the other plays alone), ctrl+t gives them back and ends |
| `sand` | `Pilot.wait_idle` to choose a pane; a native simulation from a draw extension that reads the pane's cells, its walls, by itself; the pile is fixed on frame rows and scrolls with the text; `--cursor` pours on the cursor's column |
| `cat` | an app driven by events (`idle`, `cursor`, `screen`, `frames`), asleep on the socket between two things to do; it stands on lines of text in frame coordinates (`cat.ledges`), sleeps when the pane is idle, and jumps away from the cursor, from text written over it (`covered`) and back into the view after a scroll (`visible`). `--count N`: cats that share one read of the text (`cat.Home`) and keep off each other, a coat each; `-r`: coats and features (ears, eyes, nose) drawn. The word `food` on screen gets a plate next to it, where the cats come and eat; `--food`: a plate on a ledge now and then; `--mice`: a mouse walks a ledge now and then, the cats pounce at it; `-a`: `-r --food --mice`, two cats unless `--count` |
| `marks` | a highlighter: one tint per match of each pattern (`-e` regex, `-i`), in frame coordinates and on remembered lines too (`find(known=True)`), so the driver scrolls them with the text; searches of a look in one batch; topic `draw`; a display of the counts placed by the driver |
| `rain` | a native effect from a draw extension that sees the text: drops fall behind it and splash on the top of its lines, on the cursor and on the floor; less rain on columns that changed in the last 10 s (the heat, given to the effect) |
| `snow` | a native effect that sees cells and heat: flakes fall behind the text; with `--cover`, snow lies over the lines of text, a cell away from the words of its own row, and on the floor, in smooth piles up to three rows deep; it melts where the screen changed lately and around the cursor; it lies on frame rows, so it scrolls with the text; the wind is tuned in place (`Sprite.tune(wind=...)`), the effect keeps its state |

In a `ptyrc run` with no client connected, ctrl+t typed twice runs one
of them on the session; twice again replaces it by the next
(`demo.py`: rounds of all the apps shuffled, the `-a` version one time
in two; `marks` is not shown). `ptyrc-app` must be in `PATH`, or
`PTYRC_DEMO` name the command: `PTYRC_DEMO="uv run ptyrc-app demo"`.

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
  `_cli.in_sight()`; or with `sight.look()` and `sight.drawing()`.
- **Idle**: `frame.is_idle` is true once 80 % of a pane has been still
  for 10 s. A big animation waits for it (`pilot.wait_idle`, `sand.py`);
  topic `idle` says when it comes and goes (`cat.py`).
- **What became of a sprite**: `Sprite.info()` has `visible` (some of
  it is in the view) and `covered` (share of a back sprite under the
  app's text); topic `draw` lists the sprites removed with rows the
  driver forgot.
- **Events, not ticks**: an app with nothing to animate waits on
  `pilot.events(..., timeout=...)` or `pilot.event(timeout)`: it wakes
  on a change, or when its next step is due (`cat.py`, `marks.py`).
  Silence ends the wait; a session that ended raises `PtyrcError`
  (`gone`), which `_cli.run` takes for a normal end: no loop tests the
  connection.
- **The state at start**: `pilot.subscribe(..., snapshot=True)` sends
  what is (idle, cursor, frames, size) as the first events (`cat.py`).
- **Topic `text`**: per frame, the frame rows whose text changed, and
  the origin after a scroll; a row that scrolls into view is not a
  change. Topic `cursor` tells the cursor in frame cells (`frame`,
  `pos`).
- **A game on a tick, a map on events**: `_cli.Sight(pilot, frame)`
  subscribes to `frames`, `text` and, when asked, `cursor`. Each tick
  starts with `sight.look()`: it takes the events waiting and reads
  the text of the view again only when told (`sight.changed`,
  `sight.rows`, `sight.cursor`). A quiet screen costs no read
  (`snake.py`, `bricks.py`, `fishes.py`, `flower.py`).
- **A frame replaced**: a pane keeps its frame through a resize, a
  nearly empty one too when it is the only such pane. Otherwise it gets
  a new frame, and the sprites stay with the old one.
  `_cli.follow` waits on the topic `frames`, `_cli.heir` finds the new
  pane in the event's data, in proportion of the resize;
  `Sprite.update(frame=...)` moves the sprite, state kept. Its `beats`
  are the app's own work between two events (`snow.py`).
- **An effect tuned**: `Sprite.tune(wind=2)` gives a running effect
  new parameters; it keeps its state (`snow.py`).
- **A score**: `_arcade.Board`, one sprite in view cells with opaque
  blanks, sent when it changes.
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
  `pilot.events("screen", "text", "cursor", "frames")`. Inside
  `batch()`, `text`, `find`, `sprites` and `Sprite.info` return a list
  or a dict filled when the batch is sent (`marks.py`).

`_cli.ticks(period)` paces a loop and yields the measured step;
`_cli.hold(pilot)` keeps an app with nothing to send connected;
`_cli.follow` does, and follows a frame replaced.

Read `confetti.py`, `clock.py` and `rain.py` first, then `marks.py`,
`cat.py`, `flower.py`, `fishes.py`, `snake.py`, `bricks.py`,
`sand.py`.

## Tests

`uv run pytest plugins/client-apps/python`: each app against a headless
driver. An app runs until ^C or the end of its session; a test stops it
with `main(argv, seconds=...)`, or with a stand-in that raises
`KeyboardInterrupt` (`confetti`).
