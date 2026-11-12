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
    pilot.draw([" 12:00 "], auto="top-right", bg="blue")  # a quiet corner
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
| frames | `frames`, `panes`, `frame`, `frame_at`; `Frame.text`, `known`, `find`, `heat`, `to_screen`, `to_frame`, `draw` |
| draw | `draw`, `put`, `effect`, `ext`, `sprites`, `clear`; `Sprite.set`, `move`, `update`, `info`, `remove` |
| other | `batch`, `events`, `subscribe`, `resize`, `debug_overlay`, `dump`, `call` |

Errors are `PtyrcError` with a `code` (`no-frame`, `bad-params`, `gone`...);
`wait_for` raises `TimeoutError`. The wire protocol is in `plan/design.md`.

`uv run pytest python/ptyrc` runs the tests against a headless driver.
