# SPDX-License-Identifier: GPL-3.0-or-later
"""`cat`: the pointer it hunts, the fish, the ball of yarn."""

from __future__ import annotations

import random
import threading
import time
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import cat

if TYPE_CHECKING:
    from conftest import Host

TEXT = "\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n' 0 '0"
RESTING = ("alert", "sit", "groom", "stare", "puzzled", "purr")


def shown(pilot: Pilot) -> str:
    return "\n".join(pilot.dump()["host"])


def house(session: Pilot, **kw: Any) -> tuple[Any, Callable[..., list[str]]]:
    """A home on two long lines, and its loop with a clock of our own:
    `run` gives the acts the first cat went through."""
    session.send(f"clear; printf '{TEXT}'\n")
    session.wait_for("0" * 60, frame="raw")
    time.sleep(0.3)
    home = cat.Home(session, session.frame("raw"), random.Random(7), **kw)
    now = 0.0

    def run(until: Callable[[], bool], limit: float = 60.0) -> list[str]:
        nonlocal now
        end = now + limit
        acts = [home.cats[0].act]
        while not until():
            assert now < end, [it.act for it in home.cats]
            now = max(now + 0.01, min(home.due(now), end))
            home.step(now)
            if home.cats[0].act != acts[-1]:
                acts.append(home.cats[0].act)
        return acts

    run.now = lambda: now  # type: ignore[attr-defined]
    return home, run


def hover(home: Any, at: tuple[int, int] | None, now: float) -> None:
    data: dict[str, Any] = {"sprite": None, "frame": None, "pos": None}
    if at is not None:
        data = {"sprite": 1, "frame": home.frame.id, "pos": list(at)}
    home.on("hover", data, now)


def write(session: Pilot, row: int, col: int, word: str) -> None:
    session.send(f"printf '\\033[{row};{col}H{word}\\033[20;1H'\n")
    session.wait_for(f" {word} ", frame="raw")
    time.sleep(0.2)


def test_cat_play_art() -> None:
    assert {len(row) for row in cat.FISH} == {cat.FW}
    left = cat.fish_look(3, right=False)["cells"][0]
    assert "".join(run["t"] for run in left) == "><\\\\\\>"
    assert len(cat.fish_look(3, flop=True)["frames"]) == 4
    # The thread: braille, the same over a column whatever its ends.
    wire = cat.thread(3, 20)
    assert len(wire) == 20 and all(0x2800 < ord(ch) < 0x2900 for ch in wire)
    assert cat.thread(5, 18) == wire[2:] and cat.thread(3, 9) == wire[:9]
    fade = cat.thread_look(3, 20, 203, fade=True)
    assert fade["loop"] is False and len(fade["frames"]) == 3
    for pose in ("stare", "wiggle", "bat", "beg", "purr", "run", "hiss", "roll"):
        assert len(cat.POSES[pose]) == 2, pose


def test_cat_pointer(session: Pilot) -> None:
    home, run = house(session, hunt=True)
    (it,) = home.cats
    run(lambda: it.act in RESTING)
    # The mat: a sprite of nothing, as large as the view.
    w, h = home.frame.size
    assert [w, h] in [s["size"] for s in session.sprites()]
    assert it.fun == 0 and not it.stalk(run.now())
    # The pointer far along its ledge: it runs or wiggles, pounces, and
    # has it under its paws.
    ledge = cat.perch(home.spots, it.x, it.g)
    far = ledge.x1 if it.x - ledge.x0 < ledge.x1 - it.x else ledge.x0
    at = (far + 5, it.g - 2)
    hover(home, at, run.now())
    assert home.pointer == at and it.fun >= 0.7
    acts = run(lambda: it.act == "bat")
    assert "leap" in acts and ("wiggle" in acts or "run" in acts), acts
    assert it.holds(at) and it.g == ledge.g
    # Over it, out of its box: up on its hind legs, then a leap up.
    up = (it.x + 5, it.g - cat.H - 2)
    hover(home, up, run.now())
    acts = run(lambda: it.act == "leap")
    assert "beg" in acts, acts
    assert it.landing == (it.x, it.g) and min(g for _, g in it.path) == it.g - 2
    run(lambda: it.act == "stare")
    # Gone: it looks for it.
    hover(home, None, run.now())
    assert it.act == "puzzled" and home.pointer is None
    # It rests under its paws: the cat purrs, until it moves.
    hover(home, (it.x + 5, it.g - 2), run.now())
    acts = run(lambda: it.act == "purr")
    assert "bat" in acts and it.ignored == home.pointer
    assert "♥" in shown(session)
    run(lambda: it.act not in ("purr", "bat", "stare"))
    assert not it.stalk(run.now())
    # On another line: it jumps there.
    other = next(s for s in home.spots if s.g != it.g and s.x1 - s.x0 > 20)
    at = (other.x0 + 15, other.g - 1)
    hover(home, at, run.now())
    run(lambda: it.g == other.g and it.act == "bat")
    assert it.holds(at)
    # A sleeper is poked awake.
    hover(home, None, run.now())
    home.on("idle", {"frame": home.frame.id, "idle": True}, run.now())
    run(lambda: it.act == "sleep")
    hover(home, (it.x + 30, it.g - 2), run.now())
    assert it.act == "sleep"
    hover(home, (it.x + 9, it.g - 2), run.now())
    assert it.act == "alert"


def test_cat_fish(session: Pilot) -> None:
    home, run = house(session, count=2)
    run(lambda: all(it.act in RESTING for it in home.cats))
    write(session, 14, 30, "fish")
    home.stale = True
    run(lambda: home.fish is not None)
    fish = home.fish
    (word,) = session.find(r"\bfish\b", regex=True)
    assert (fish.x, fish.g) == (word.x + 4 + cat.GAP, word.y + 1)
    assert "<///><" in shown(session) or "<\\\\\\><" in shown(session)
    # A cat takes it, carries it away, and eats it there.
    run(lambda: fish.holder is not None)
    first = fish.holder
    assert first.fish is fish and first.act == "bat" and first.beside(fish)
    run(lambda: fish.holder is not None and fish.holder.act == "leap")
    run(lambda: fish.holder is not None and fish.holder.act == "eat")
    eater = fish.holder
    assert eater.beside(fish) and (fish.x, fish.g) != (word.x + 4 + cat.GAP, word.y + 1)
    # Eaten, by one or the other: the word gets no other.
    run(lambda: home.fish is None)
    assert fish.left == 0 and home.eaten == {(word.x, word.y)}
    assert all(it.fish is None for it in home.cats)
    home.stale = True
    run(lambda: not home.stale)
    assert home.fish is None and len(session.sprites()) == 2


def test_cat_fish_stolen(session: Pilot) -> None:
    home, run = house(session, count=2)
    a, b = home.cats
    run(lambda: all(it.act in RESTING for it in home.cats))
    home.fresh(run.now())
    # A fish before the first, that eats it; the other comes for it.
    home.drop((a.x + cat.W, a.g), None, float("inf"))
    fish = home.fish
    fish.holder, a.fish = a, fish
    a.right = True
    a.rest("eat", run.now(), cat.BITE)
    home.rng = b.rng = a.rng = random.Random(1)
    took = hissed = False
    for _ in range(40):
        if home.fish is None:
            break
        fish.left, fish.steals = 3, 0
        owner = fish.holder
        thief = b if owner is a else a
        if owner is None or owner.act != "eat":
            run(lambda: home.fish is None or (fish.holder and fish.holder.act == "eat"))
            continue
        if (aim := home.loot(thief)) is None:
            continue
        thief.quit()
        thief.jump(aim, run.now(), (thief.x, thief.g))
        thief.prey = "steal"
        run(lambda t=thief: t.act != "leap")
        if fish.holder is thief:
            took = True
            assert owner.act == "hiss" and owner.fish is None and thief.act == "bat"
        else:
            hissed = True
            assert owner.act == "hiss" and owner.fish is fish and thief.spooked
            run(lambda o=owner: o.act == "eat")
        if took and hissed:
            break
    assert took and hissed
    if home.fish is not None:
        home.lose(home.fish, run.now())


def test_cat_toy(session: Pilot) -> None:
    home, run = house(session)
    (it,) = home.cats
    run(lambda: it.act in RESTING)
    write(session, 14, 30, "toy")
    home.stale = True
    run(lambda: home.yarn is not None)
    yarn = home.yarn
    (word,) = session.find(r"\btoy\b", regex=True)
    assert (yarn.x, yarn.g) == (word.x + 3 + cat.GAP, word.y + 1)
    assert (yarn.lo, yarn.hi) == (yarn.x - cat.GAP, yarn.x + cat.LANE)
    assert "●" in shown(session)
    plays = yarn.plays
    # The cat comes, bats it; it rolls and leaves its thread.
    acts = run(lambda: yarn.plays < plays and not yarn.run and yarn.x != word.x + 6)
    assert "bat" in acts and yarn.player is it
    assert yarn.ends[0] < yarn.ends[1] and yarn.lo <= yarn.x <= yarn.hi
    assert any(0x2800 < ord(ch) < 0x2900 for ch in shown(session))
    # Played out: on its back in the thread, then the ball goes, the
    # thread after it, and the word gets no other.
    acts = run(lambda: home.yarn is None)
    assert "roll" in acts and yarn.plays == 0 and it.toy is None
    assert home.eaten == {(word.x, word.y)}
    home.stale = True
    run(lambda: not home.stale)
    assert home.yarn is None
    deadline = time.monotonic() + 3
    while len(session.sprites()) > 1 and time.monotonic() < deadline:
        time.sleep(0.05)
    assert len(session.sprites()) == 1 and "●" not in shown(session)


def test_cat_fish_and_toys_served(session: Pilot) -> None:
    home, run = house(session, count=2, fish=True, toys=True)
    run(lambda: home.fish is not None and home.yarn is not None)
    fish, yarn = home.fish, home.yarn
    assert fish.word is None and yarn.word is None
    run(lambda: home.fish is not fish and home.yarn is not yarn, limit=120.0)
    assert home.next_fish < float("inf") or home.fish is not None
    assert home.next_toy < float("inf") or home.yarn is not None


def test_cat_play_options(session: Pilot) -> None:
    args = ["-s", session.session, "--seed", "3", "--fish", "--toys", "--hunt"]
    assert cat.main(args, seconds=2.0) == 0


@pytest.mark.parametrize("scrolled", [False, True])
def test_cat_hunts_the_host_mouse(
    hosted: tuple[Host, Pilot], scrolled: bool, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The real path: a move of the host mouse on the mat, a `hover`
    event, the cat at the pointer; the view scrolled or not."""
    host, pilot = hosted
    pointers: list[tuple[int, int] | None] = []
    point = cat.Home.point

    def told(home: cat.Home, data: dict[str, Any], now: float) -> None:
        point(home, data, now)
        pointers.append(home.pointer)

    monkeypatch.setattr(cat.Home, "point", told)
    if scrolled:
        pilot.send("seq 100 140\n")
        pilot.wait_for("140", frame="raw")
    pilot.send(f"{'' if scrolled else 'clear; '}printf '{TEXT}'\n")
    pilot.wait_for("0" * 60, frame="raw")
    time.sleep(0.3)
    # The pane the cats take, the largest frame but the raw one.
    frame = max(pilot.frames()[1:], key=lambda f: f.size[0] * f.size[1])
    assert (frame.origin[1] > 0) == scrolled, frame.info
    view = list(frame.size)
    status: list[int] = []
    args = ["-s", pilot.session, "--hunt", "--seed", "7"]
    thread = threading.Thread(
        target=lambda: status.append(cat.main(args, seconds=20.0)), daemon=True
    )

    def sprite(size: list[int]) -> dict[str, Any] | None:
        return next((s for s in pilot.sprites() if s["size"] == size), None)

    def wait(until: Callable[[], Any], limit: float, why: object) -> Any:
        deadline = time.monotonic() + limit
        while not (got := until()):
            assert time.monotonic() < deadline, (why, shown(pilot))
            time.sleep(0.02)
        return got

    thread.start()
    try:
        wait(lambda: sprite(view), 5, "no mat")
        it = wait(lambda: (s := sprite([cat.W, cat.H])) and s["screen"] and s, 5, "")
        host.wait_seen(b"\x1b[?1003h")
        # The upper line of zeros, on the screen; the cell over it far
        # from the cat.
        top = next(y for y, row in enumerate(pilot.dump()["host"]) if "0" * 60 in row)
        x = 50 if it["screen"][0] < 25 else 5
        at = (x, top - 2)

        def holds() -> bool:
            s = sprite([cat.W, cat.H])
            if s is None or s["screen"] is None:
                return False
            sx, sy = s["screen"]
            return sx <= at[0] < sx + cat.W and sy <= at[1] < sy + cat.H

        host.move(*at)
        # The pointer is the cell of the frame under the screen cell.
        (rx, ry, _, _), (ox, oy) = frame.info["rect"], frame.origin
        wait(lambda: pointers, 3, "no hover")
        assert pointers == [(at[0] - rx + ox, at[1] - ry + oy)]
        wait(holds, 3, ("no cat at", at, it))
    finally:
        thread.join()
    assert status == [0]
    # The app gone, its mat too.
    wait(lambda: sprite(view) is None, 3, "the mat stays")
