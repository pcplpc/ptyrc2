# ptyrc (Python)

Pilot library for the ptyrc driver: read the screen of a wrapped terminal
program, type into it, draw on it. Standard library only.

```python
from ptyrc import Pilot

with Pilot() as pilot:  # the session of $PTYRC_SESSION, or the only one
    print("\n".join(pilot.text()))  # what is on screen
    for m in pilot.find("flower"):  # where: frame and screen coordinates
        print(m.x, m.y, m.screen)
    pilot.keys("esc", ":w", "enter")  # type
    pilot.wait_for("written", timeout=2)  # wait

    pane = pilot.frame("active")  # the pane holding the cursor
    pane.draw(["<3"], near="flower", fg="red")  # follows the word
    pane.draw(["note"], at=(40, 120), space="frame")  # row 120 of the document
    pilot.draw(
        [" 12:00 "], auto="top-right", bg="blue", layer="front"
    )  # a quiet corner
    pilot.effect("confetti")
```

`Pilot.spawn(argv)` starts a headless driver, for tests and automation:

```python
with Pilot.spawn(["vim", "notes.md"], size=(100, 30)) as pilot:
    pilot.wait_for("notes.md")
    assert "TODO" in "\n".join(pilot.text())
```

| | |
|---|---|
| read | `text`, `cells`, `find`, `cursor`, `wait_for`, `info`, `size` |
| type | `send`, `keys`, `paste`, `write` |
| frames | `frames`, `panes`, `frame`, `frame_at`; `Frame.text`, `known`, `find`, `heat`, `heat_classes`, `idle`, `check_idle`, `is_idle`, `to_screen`, `to_frame`, `draw` |
| draw | `draw`, `put`, `effect`, `ext`, `sprites`, `clear`; `Sprite.set`, `ext`, `move`, `update`, `tune`, `info`, `remove` |
| other | `batch`, `events`, `event`, `subscribe`, `resize`, `debug_overlay`, `dump`, `call` |
| tools | `locator`, `Frame.locator`, `click`, `grab`, `release`, `shown`, `snapshot`; `spawn(record=)`, `Pilot.replay`, `tape` |

Events: `pilot.subscribe("idle", "cursor", snapshot=True)` sends the
state now as the first events. `pilot.events(*topics, timeout=)` yields
`(topic, data)` and ends after `timeout` seconds of silence;
`pilot.event(timeout)` is one event, or None. Both raise `PtyrcError`
(`gone`) when the connection is closed.

Inside `with pilot.batch():`, the calls that read (`text`, `find`,
`sprites`, `Sprite.info`) return a list or a dict that is filled when
the batch is sent; the others return None.

A `Locator` is a place named by the text there, looked up at each use:

```python
total = pilot.locator("Total:")
total.wait()
assert total.right(2).read(5) == "42.00"  # 2 cells after the match
pilot.frame("active").locator("OK").click()  # as the app's mouse mode wants

with Pilot.replay("session.tape") as replay:  # written by spawn(record=...)
    replay.tape.seek(2.5)  # tape seconds; the clock stops there
    assert "TODO" in "\n".join(replay.text())
```

Sprites are drawn behind the app's text unless `layer="front"`; the app
is layer 0, negative layers are behind it.

Errors are `PtyrcError` with a `code` (`no-frame`, `bad-params`, `gone`...);
`wait_for` raises `TimeoutError`. The wire protocol is in `plan/design.md`.

`uv run pytest python/ptyrc` runs the tests against a headless driver.
