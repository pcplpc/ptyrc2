# SPDX-License-Identifier: GPL-3.0-or-later
"""`_sky`: the sun, the moon, the light and the colours of a time."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

import pytest
from ptyrc_apps import _sky

JUNE, DECEMBER = datetime(2026, 6, 21), datetime(2026, 12, 21)
FULL = _sky.NEW_MOON + timedelta(days=_sky.SYNODIC * 100.5)


def at(day: datetime, hour: float, lat: float = 47.0, cover: float = 0.0) -> _sky.Sky:
    return _sky.at(day + timedelta(hours=hour), lat, cover)


def test_sun() -> None:
    assert at(JUNE, 12).sun == pytest.approx(90 - 47 + 23.44, abs=0.5)
    assert at(DECEMBER, 12).sun == pytest.approx(90 - 47 - 23.44, abs=0.5)
    assert at(JUNE, 0).sun < -15 and at(DECEMBER, 0).sun < -60
    # It rises on the left of a view to the south, and sets on the right.
    assert at(JUNE, 6).sun_x < 0.5 == at(JUNE, 12).sun_x < at(JUNE, 18).sun_x
    # Under the equator the view is to the north: the other way.
    assert at(JUNE, 6, -35).sun_x > 0.5 > at(JUNE, 18, -35).sun_x
    assert at(DECEMBER, 12, -35).sun > at(JUNE, 12, -35).sun
    # Long days in June: the sun is up at 5 and at 19, not in December.
    assert at(JUNE, 5).sun > 0 < at(JUNE, 19).sun
    assert at(DECEMBER, 7).sun < 0 > at(DECEMBER, 17).sun
    assert -90 <= _sky.at(JUNE, 89).sun <= 90  # the latitude is clamped


def test_moon() -> None:
    assert _sky.phase_of(_sky.NEW_MOON) == 0
    assert _sky.phase_of(FULL) == pytest.approx(0.5)
    aware = FULL.replace(tzinfo=UTC).astimezone()
    assert _sky.phase_of(aware) == pytest.approx(0.5)
    full = _sky.at(FULL.replace(hour=0, minute=0))
    assert full.lit > 0.95 and full.moon > 10  # opposite the sun: up at midnight
    assert 0.3 < full.moon_x < 0.7
    new = _sky.at((FULL + timedelta(days=_sky.SYNODIC / 2)).replace(hour=12))
    assert new.lit < 0.05 and abs(new.moon_x - new.sun_x) < 0.1
    assert full.moonlight > 0.5 and new.moonlight == 0


def test_seasons() -> None:
    days = {"spring": 110, "summer": 200, "autumn": 290, "winter": 20}
    for name, day in days.items():
        assert _sky.season_of(day) == name
    assert _sky.season_of(360) == "winter"
    assert _sky.season_of(200, -30) == "winter" and _sky.season_of(20, -30) == "summer"
    assert at(JUNE, 12).season == "summer" and at(JUNE, 12, -30).season == "winter"
    assert set(days) == set(_sky.SEASONS)


def test_weather() -> None:
    seen = set()
    for day in range(730000, 730400):
        for season in _sky.SEASONS:
            kind = _sky.weather(day, 7, season)
            assert kind in _sky.WEATHERS
            assert kind == _sky.weather(day, 7, season)
            assert not (kind == "snow" and season in ("spring", "summer"))
            seen.add(kind)
    assert seen == set(_sky.WEATHERS)
    assert any(_sky.weather(d, 1) != _sky.weather(d, 2) for d in range(50))


def test_light() -> None:
    noon, night = at(JUNE, 12), at(JUNE, 0)
    assert noon.light == 1 and night.light == 0
    assert noon.stars == 0 and night.stars > 0.6
    assert at(JUNE, 0, cover=1.0).stars == 0
    assert 0 < at(JUNE, 4).light < 1
    assert noon.ambient == (255, 255, 255)
    assert sum(night.ambient) < 300
    green = (90, 160, 40)
    assert noon.shade(green) == green
    assert sum(night.shade(green)) < sum(green) / 3
    # Far off, the land takes the colour of the horizon.
    far, horizon = noon.shade(green, 1.0), noon.colour(0.5, 0.9)
    assert sum(abs(a - b) for a, b in zip(far, horizon, strict=True)) < sum(
        abs(a - b) for a, b in zip(green, horizon, strict=True)
    )


def test_colours() -> None:
    for day in (JUNE, DECEMBER):
        for hour in range(48):
            for cover in (0.0, 0.6, 1.0):
                sky = at(day, hour / 2, cover=cover)
                for u in (0.0, 0.5, 1.0):
                    for v in (0.0, 0.5, 1.0):
                        c = sky.colour(u, v)
                        assert len(c) == 3 and all(0 <= k <= 255 for k in c)
                assert all(0 <= k <= 255 for k in sky.ambient)
    noon, night, dusk = at(JUNE, 12), at(JUNE, 0), at(JUNE, 19.8)
    assert noon.colour(0.5, 0)[2] > 200 > noon.colour(0.5, 0)[0]  # blue
    assert sum(night.colour(0.5, 0)) < 120
    # The low sun reddens the horizon, most on its side.
    west, east = dusk.colour(dusk.sun_x, 1), dusk.colour(0.0, 1)
    assert west[0] > west[2] and west[0] >= east[0]
    # Clouds grey it.
    grey = at(JUNE, 12, cover=1.0).colour(0.5, 0.3)
    assert max(grey) - min(grey) < 40
    assert at(JUNE, 12, cover=1.0).ambient < noon.ambient
