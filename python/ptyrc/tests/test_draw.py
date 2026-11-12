# SPDX-License-Identifier: GPL-3.0-or-later
"""Drawing: anchors, content, lifetime, batches."""

from __future__ import annotations

import time

from ptyrc import Pilot, PtyrcError, sprite_spec
from tests_support import until


def shown(pilot: Pilot) -> list[str]:
    """What the host shows: the app screen with the sprites."""
    return pilot.dump()["host"]


def test_sprite_spec() -> None:
    assert sprite_spec(["a"]) == {
        "anchor": {"at": [0, 0], "space": "view"},
        "content": {"rows": ["a"]},
    }
    spec = sprite_spec("a\nb", near="x", side="below", gap=2, fg="red", bold=True)
    assert spec["anchor"] == {"text": "x", "side": "below", "gap": 2}
    assert sprite_spec("a", near="x", slide=False)["anchor"]["slide"] is False
    assert spec["content"] == {"rows": ["a", "b"], "style": {"fg": "red", "bold": True}}
    spec = sprite_spec(frames=[["a"], {"rows": ["b"], "ms": 5}], frame_ms=50, loop=2)
    assert [f["ms"] for f in spec["frames"]] == [50, 5] and spec["loop"] == 2
    spec = sprite_spec(effect="confetti", auto=True, size=(10, 4), ttl=1.5, z=3)
    assert spec["effect"] == {"name": "confetti"}
    assert spec["anchor"] == {"auto": {"prefer": "any"}}
    assert (spec["size"], spec["ttl_ms"], spec["z"]) == ([10, 4], 1500, 3)


def test_anchors(shell: Pilot) -> None:
    shell.send("echo a fl''ower here\n")
    word = shell.wait_for("a flower here")

    at = shell.draw(["AT"], at=(40, 5))
    edge = shell.draw(["EDGE"], at=(-1, -1))
    near = shell.draw(["<-"], near="flower", layer="front")
    over = shell.draw(["^^"], near="flower", side="below")
    auto = shell.draw(["AUTO", "AUTO"], auto="top-right")
    follow = shell.draw(["C"], cursor=(2, 0))

    assert until(lambda: at.info()["screen"]) == [40, 5]
    assert at.visible and at.info()["covered"] == 0
    # Behind the text: half of `HI` is under the `a` of the echo.
    behind = shell.draw(["HI"], at=(word.x, word.y))
    assert until(lambda: behind.info()["covered"] == 0.5)
    # Below the screen: a position, nothing to see.
    below = shell.draw(["LOW"], at=(0, 40), space="frame")
    assert until(lambda: below.info()["screen"]) == [0, 40]
    assert not below.visible
    behind.remove()
    below.remove()
    assert edge.info()["screen"] == [76, 23]
    assert near.info()["screen"] == [word.x + 2 + 6 + 1, word.y]
    assert over.info()["screen"] == [word.x + 2 + 2, word.y + 1]
    cursor = shell.cursor()
    assert follow.info()["screen"] == [cursor.x + 2, cursor.y]
    spot = until(lambda: auto.info()["screen"])
    assert spot[0] >= 60 and spot[1] <= 4, spot

    lines = shown(shell)
    assert lines[word.y].startswith("a flower <- here") or "<-" in lines[word.y]
    assert lines[5][40:42] == "AT" and lines[23].endswith("EDGE")

    # The cursor moves: its sprite follows. The word goes: its sprites hide.
    shell.send("abc")
    until(lambda: follow.info()["screen"] == [cursor.x + 5, cursor.y])
    shell.send("\x15clear\n")
    shell.wait_for("flower", gone=True)
    assert until(lambda: near.info()["screen"] is None)
    assert not near.visible
    assert "<-" not in "".join(shown(shell))
    assert len(shell.sprites()) == 6
    assert shell.clear() == 6
    assert until(lambda: shown(shell) == shell.text())


def test_content_and_blend(shell: Pilot) -> None:
    shell.send("echo XXXXXXXX\n")
    row = shell.wait_for(r"^XXXXXXXX", regex=True).y

    # Spaces are transparent, unless opaque.
    holes = shell.draw(["a a"], at=(0, row), layer="front")
    until(lambda: shown(shell)[row].startswith("aXaXXXXX"))
    holes.set(["b b"], opaque=True)
    until(lambda: shown(shell)[row].startswith("b bXXXXX"))
    holes.remove()

    # The default layer is behind the app: seen where it left no ink.
    behind = shell.draw(["......"], at=(6, row))
    until(lambda: shown(shell)[row].startswith("XXXXXXXX...."))
    assert behind.info()["layer"] == -1
    behind.update(layer="front")
    until(lambda: shown(shell)[row].startswith("XXXXXX......"))
    behind.remove()

    # Tint: the app's glyphs, the sprite's colours.
    shell.draw(["####"], at=(0, row), blend="tint", bg="blue")
    until(lambda: len(shell.sprites()) == 1 and shell.sprites()[0]["screen"])
    assert shown(shell)[row].startswith("XXXXXXXX")
    assert shell.clear() == 1

    # Styled runs.
    cells = [[{"t": "ab", "fg": "red"}, {"t": "cd", "bg": 21, "bold": True}]]
    sprite = shell.draw(cells=cells, at=(20, 10))
    until(lambda: shown(shell)[10][20:24] == "abcd")
    sprite.move(30, 10)
    until(lambda: shown(shell)[10][30:34] == "abcd")
    assert shown(shell)[10][20:24].strip() == ""
    assert sprite.info()["kind"] == "content"
    sprite.remove()
    sprite.remove()  # already gone: fine


def test_layers(shell: Pilot) -> None:
    assert sprite_spec(["a"], layer="front", z=2)["layer"] == "front"
    shell.send("echo the app text\n")
    row = shell.wait_for(r"^the app text", regex=True).y

    # A sandwich: background, a drawing in front of it, the app's text,
    # a head-up display in front of all.
    field = shell.draw(["." * 20], at=(0, row), fg="blue", bg=17, layer=-2)
    fish = shell.draw(["<><"], at=(12, row), fg="yellow")
    hud = shell.draw(["[hud]"], at=(15, row), layer="front", opaque=True)
    until(lambda: shown(shell)[row].startswith("the.app.text<><[hud]"))
    assert [s["layer"] for s in shell.sprites()] == [-2, -1, 1]
    # It swims behind the words and shows between them.
    fish.move(6, row)
    until(lambda: shown(shell)[row].startswith("the.app>text...[hud]"))

    # The app's glyphs keep their colours and stand on the field's.
    assert shell.dump()["app"][row].startswith("the app text")
    runs = shell.cells()[row]
    assert runs[0]["t"].startswith("the") and "bg" not in runs[0]
    try:
        shell.draw(["x"], layer=0)
        raise AssertionError("layer 0 is the app's")
    except PtyrcError as exc:
        assert exc.code == "bad-params"
    for sprite in (field, fish, hud):
        sprite.remove()
    assert until(lambda: shown(shell) == shell.text())


def test_animation_and_ttl(shell: Pilot) -> None:
    anim = shell.draw(frames=[["1"], ["2"], ["3"]], frame_ms=60, loop=False, at=(50, 3))
    seen = set()
    deadline = time.monotonic() + 3
    while anim.info() is not None and time.monotonic() < deadline:
        seen.add(shown(shell)[3][50:51])
        time.sleep(0.01)
    assert anim.info() is None, "a finite animation removes itself"
    assert {"2", "3"} <= seen

    forever = shell.draw(frames=[["x"], ["y"]], frame_ms=40, at=(50, 3))
    time.sleep(0.3)
    assert forever.info() is not None

    brief = shell.draw(["ttl"], at=(60, 3), ttl=0.2)
    until(lambda: shown(shell)[3][60:63] == "ttl")
    until(lambda: brief.info() is None)
    assert shown(shell)[3][60:63].strip() == ""


def test_effects_and_extensions(shell: Pilot) -> None:
    exts = shell.exts()
    assert {"confetti", "sparkle"} <= set(exts["effects"])
    assert {e["name"] for e in exts["exts"]} >= {"shapes", "bigtext"}

    burst = shell.effect("confetti", {"duration_ms": 500, "count": 40})
    assert burst.info()["kind"] == "effect:confetti"
    until(lambda: any(c in "".join(shown(shell)) for c in "❋✦*·"))
    until(lambda: burst.info() is None)
    assert until(lambda: shown(shell) == shell.text()), "confetti left cells behind"

    circle = shell.ext(
        "shapes", "circle", {"r": 2, "fg": "cyan"}, anchor={"at": [30, 8]}
    )
    assert circle.info()["size"] == [8, 4]
    big = shell.ext(
        "bigtext", "text", {"text": "OK"}, anchor={"at": [-1, -1], "space": "view"}
    )
    assert big.info()["size"] == [7, 3]
    until(lambda: "▀" in "".join(shown(shell)[21:]))
    # In place: same sprite, new drawing.
    big.ext("bigtext", "text", {"text": "OKAY"})
    assert big.info()["size"] == [15, 3]
    assert len(shell.sprites()) == 2
    try:
        shell.ext("shapes", "spiral")
        raise AssertionError("an unknown function is refused")
    except PtyrcError as exc:
        assert exc.code == "bad-params"


def test_sprites_die_with_their_owner(shell: Pilot) -> None:
    with Pilot(shell.session) as other:
        other.draw(["mine"], at=(10, 10))
        kept = other.draw(["kept"], at=(10, 11), persist=True)
        until(lambda: shown(shell)[10][10:14] == "mine")
        assert shell.clear() == 0, "clear() leaves the others' sprites"
    until(lambda: shown(shell)[10][10:14].strip() == "")
    assert shown(shell)[11][10:14] == "kept"
    assert [s["id"] for s in shell.sprites()] == [kept.id]
    assert shell.clear(all=True) == 1


def test_batch(shell: Pilot) -> None:
    with shell.batch() as batch:
        a = shell.draw(["A"], at=(5, 5))
        b = shell.draw(["B"], at=(6, 5))
        assert a.id is None, "ids come with the batch"
        shell.call("draw.remove", id=424242)
        shell.call("screen.cursor")
    assert a.id is not None and b.id == a.id + 1
    assert [type(r) for r in batch.results] == [dict, dict, PtyrcError, dict]
    assert batch.results[2].code == "no-sprite"
    until(lambda: shown(shell)[5][5:7] == "AB")

    with shell.batch():
        a.move(5, 6)
        b.move(6, 6)
    until(lambda: shown(shell)[6][5:7] == "AB")
    assert shown(shell)[5][5:7].strip() == ""


def test_debug_overlay(vim: Pilot) -> None:
    vim.debug_overlay("frames")
    until(lambda: any("#" in line and "x" in line for line in shown(vim)[:1]))
    vim.debug_overlay("heat")
    vim.debug_overlay("off")
    until(lambda: shown(vim) == vim.text())
    dump = vim.dump()
    assert dump["same_state"] is True and dump["size"] == [60, 20]
