# SPDX-License-Identifier: GPL-3.0-or-later
"""`cat`: everything on, random events, a few simulated minutes; the
state stays sound and everything goes at the end."""

from __future__ import annotations

import math
import random
import time
from typing import Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import cat

TEXT = "\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n' 0 '0"
MOVING = ("leap", "away", "walk", "run")
#: Acts a cat may hold long: nothing to do.
IDLE = ("sleep", "sit", "groom", "yawn")
LONG = 60.0  # simulated seconds, at most, of any other act
SPIN = 200  # steps, at most, at one instant
WORDS = ("food", "fish", "toy")


def screen(session: Pilot, home: Any, command: str) -> None:
    """Run `command` on the shell; the cats read the text again."""
    session.send(command + "\n")
    time.sleep(0.25)
    if home is not None:
        home.stale = True


def put(session: Pilot, home: Any, row: int, col: int, text: str) -> None:
    screen(session, home, f"printf '\\033[{row};{col}H{text}\\033[20;1H'")


def sound(home: Any) -> None:
    """The links, the counts, the places."""
    fish, yarn = home.fish, home.yarn
    cats = home.cats
    for it in cats:
        if it.fish is not None:
            assert it.fish is fish and fish.holder is it, it.act
        if it.toy is not None:
            assert it.toy is yarn and yarn.player is it, it.act
        if it.meal is not None:
            assert it.meal in home.plates, it.act
            assert it in it.meal.seats.values(), it.act
        assert it.due < math.inf or it.act == "sleep", it.act
    if fish is not None:
        assert fish.left >= 0
        if fish.holder is not None:
            assert fish.holder in cats and fish.holder.fish is fish
    if yarn is not None:
        x0, _, x1, y1 = home.view
        assert yarn.plays >= 0 and len(yarn.points) <= cat.THREAD
        # In the view as read, or over it; where it rests, on a floor.
        assert x0 <= yarn.x <= x1 - cat.BW and yarn.g <= y1, (yarn.x, yarn.g)
        if not yarn.moving:
            assert yarn.px % 2 == 0 and yarn.py % 4 == 0, (yarn.px, yarn.py)
        if yarn.player is not None:
            assert yarn.player in cats and yarn.player.toy is yarn
    for plate in home.plates:
        assert plate.left >= 0
        for side, it in plate.seats.items():
            assert it.meal is plate, (side, it.act)
    still = [it for it in cats if it.act not in MOVING]
    for i, a in enumerate(still):
        for b in still[i + 1 :]:
            apart = a.x + cat.W <= b.x or b.x + cat.W <= a.x
            apart = apart or a.g <= b.g - cat.H or b.g <= a.g - cat.H
            assert apart, ((a.x, a.g, a.act), (b.x, b.g, b.act))


def known(home: Any) -> set[int | None]:
    """The sprites the home holds."""
    out = {it.sprite.id for it in home.cats if it.sprite is not None}
    out |= {plate.sprite.id for plate in home.plates if plate.sprite is not None}
    if home.mat is not None:
        out.add(home.mat[0].id)
    for thing in (home.mouse, home.fish):
        if thing is not None and thing.sprite is not None:
            out.add(thing.sprite.id)
    if home.yarn is not None:
        out |= {s.id for s in (home.yarn.ball, home.yarn.wire) if s is not None}
    return out


def soak(
    session: Pilot, seed: int, count: int, minutes: float, hovers: float = 0.45
) -> None:
    """`hovers`: the share of the events that are moves of the pointer."""
    session.send(f"clear; printf '{TEXT}'\n")
    session.wait_for("0" * 60, frame="raw")
    time.sleep(0.3)
    rng = random.Random(seed)
    home = cat.Home(
        session,
        session.frame("raw"),
        random.Random(seed),
        count=count,
        food=True,
        mice=True,
        fish=True,
        toys=True,
        hunt=True,
    )
    # The threads that fade: the driver removes them, in real time.
    faded: list[tuple[float, int | None]] = []
    wind = home.wind

    def winding(now: float) -> None:
        if home.yarn is not None and home.yarn.wire is not None:
            faded.append((time.monotonic(), home.yarn.wire.id))
        wind(now)

    home.wind = winding
    w, h = home.frame.size
    words: list[tuple[int, int, str]] = []
    since = {id(it): (it.act, 0.0) for it in home.cats}
    now, steps, same = 0.0, 0, 0
    events = 0.5
    screens = 3.0
    moved = False

    def check(final: bool = False) -> None:
        sound(home)
        for it in home.cats:
            act, t0 = since[id(it)]
            if it.act != act:
                since[id(it)] = (it.act, now)
            elif act not in IDLE:
                assert now - t0 < LONG, (act, now - t0)
        if final or steps % 150 == 0:
            ids = {s["id"] for s in session.sprites()}
            fresh = {i for t, i in faded if time.monotonic() - t < 3.0}
            assert ids <= known(home) | fresh, (ids, known(home))
            # Cats, mat, plates, mouse, fish, ball, thread; fading threads.
            assert len(ids) <= count + 1 + cat.PLATES + 4 + len(fresh)

    def event() -> None:
        nonlocal screens, moved
        roll = rng.random()
        if roll < hovers:
            p = home.pointer
            if p is None or rng.random() < 0.3:
                at = (rng.randrange(w), rng.randrange(h))
            else:
                at = (p[0] + rng.randint(-3, 3), p[1] + rng.randint(-1, 1))
            if rng.random() < 0.1:
                at = None
            data: dict[str, Any] = {"sprite": None, "frame": None, "pos": None}
            if at is not None:
                data = {"sprite": 1, "frame": home.frame.id, "pos": list(at)}
            home.on("hover", data, now)
        elif (roll := (roll - hovers) / (1 - hovers)) < 0.3:
            it = rng.choice(home.cats)
            data = {"visible": True, "frame": home.frame.id}
            data["pos"] = [it.x + rng.randint(-8, 14), it.g - rng.randint(0, 4)]
            if rng.random() < 0.2:
                data = {"visible": False}
            home.on("cursor", data, now)
        elif roll < 0.55:
            home.on("idle", {"frame": home.frame.id, "idle": rng.random() < 0.5}, now)
        elif roll < 0.6 and not moved and now > 30:
            moved = True
            home.move(home.frame, now)
        elif now >= screens:
            screens = now + rng.uniform(10.0, 25.0)
            if words and (rng.random() < 0.5 or len(words) >= 3):
                row, col, word = words.pop(rng.randrange(len(words)))
                put(session, home, row, col, " " * len(word))
            elif rng.random() < 0.15:
                words.clear()
                screen(session, home, f"clear; printf '{TEXT}'")
            else:
                row, col = rng.randint(12, 18), rng.randint(1, 70)
                word = rng.choice(WORDS)
                words.append((row, col, word))
                put(session, home, row, col, f" {word} ")
            home.on("screen", {}, now)

    end = minutes * 60
    while now < end:
        due = min(home.due(now), events, end)
        if due <= now:
            same += 1
            assert same < SPIN, [(it.act, it.due) for it in home.cats]
        else:
            same, now = 0, due
        if now >= events:
            events = now + rng.expovariate(1 / 0.8)
            event()
        home.step(now)
        steps += 1
        check()
    assert steps / end < 400, steps / end
    # Everything off, the words gone: all of it goes.
    home.on("hover", {"sprite": None, "frame": None, "pos": None}, now)
    home.on("cursor", {"visible": False}, now)
    screen(session, home, f"clear; printf '{TEXT}'")
    end = now + 60.0
    while now < end:
        home.next_food = home.next_mouse = home.next_fish = home.next_toy = math.inf
        now = max(now + 0.01, min(home.due(now), end))
        home.step(now)
        check()
    assert home.fish is None and home.yarn is None and not home.plates
    assert home.mouse is None
    assert all(it.fish is it.toy is it.meal is None for it in home.cats)
    deadline = time.monotonic() + 3.0
    while time.monotonic() < deadline:
        if len(session.sprites()) <= count + 1:
            break
        time.sleep(0.05)
    check(final=True)
    assert len(session.sprites()) <= count + 1


@pytest.mark.parametrize(
    ("seed", "count"), [(1, 1), (2, 2), (3, 3), (4, 4), (5, 2), (6, 3)]
)
def test_cat_soak(session: Pilot, seed: int, count: int) -> None:
    soak(session, seed, count, minutes=4.0, hovers=0.45 if seed % 2 else 0.05)


def toy(session: Pilot) -> tuple[Any, Any]:
    """A home, one cat, the word `toy` on the screen and its ball."""
    session.send(f"clear; printf '{TEXT}'\n")
    session.wait_for("0" * 60, frame="raw")
    put(session, None, 14, 30, " toy ")
    session.wait_for(" toy ", frame="raw")
    home = cat.Home(session, session.frame("raw"), random.Random(7))
    home.survey(0.0)
    assert home.yarn is not None
    return home, home.yarn


def test_cat_toy_left_played_out(session: Pilot) -> None:
    # Played out, its player gone before it rolled: the ball goes, and
    # its word gets no other.
    home, yarn = toy(session)
    (it,) = home.cats
    yarn.player, it.toy, yarn.plays = it, yarn, 0
    it.quit()
    now = 0.0
    while home.yarn is not None:
        now = max(now + 0.01, home.due(now))
        assert now < 30.0
        home.step(now)
    assert yarn.word in home.eaten


def test_cat_toy_move(session: Pilot) -> None:
    # The frame taken again: the word gets its ball again.
    home, yarn = toy(session)
    home.move(home.frame, 1.0)
    assert home.yarn is None and not home.eaten
    home.survey(2.0)
    assert home.yarn is not None and home.yarn.word == yarn.word


def test_cat_toy_thread_taken(session: Pilot) -> None:
    # The driver took the thread (its rows forgotten): the ball goes all
    # the same, and another comes later.
    home, yarn = toy(session)
    yarn.word = None
    yarn.knock(1, 0.0)
    yarn.step(0.0)
    assert yarn.wire is not None
    yarn.wire.remove()
    home.wind(1.0)
    assert home.yarn is None and home.next_toy < math.inf


def test_cat_spooked_away(session: Pilot) -> None:
    # Hissed at, then out of the view: it comes back at ease.
    home, _ = toy(session)
    (it,) = home.cats
    it.spooked = True
    it.leave(0.0)
    assert not it.spooked
