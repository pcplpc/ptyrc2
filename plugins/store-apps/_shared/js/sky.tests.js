// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of sky.js: test_sky.py of the twin; `Era`, `moment`,
// `drawn` (window's) beside. The twin's exact floats: train/twin.js.

import { assert, assertEqual } from "ptyrc";
import { Twister } from "./twister.js";
import { DAY_MS, Era, NEW_MOON, SEASONS, SYNODIC, WEATHERS, at, clockOf, drawn, fields, mix, mod, moment, phaseOf, rgb, rnd, seasonOf, sunTimes, weather } from "./sky.js";

const JUNE = Date.UTC(2026, 5, 21);
const DECEMBER = Date.UTC(2026, 11, 21);
const FULL = NEW_MOON + Math.round(SYNODIC * 100.5 * DAY_MS);
const sky = (day, hour, lat = 47.0, cover = 0.0) => at(day + hour * 3600000, lat, cover);
const near = (a, b, d) => Math.abs(a - b) <= d;
const channels = (c) => [c >> 16, (c >> 8) & 255, c & 255];
const sum = (c) => channels(c).reduce((a, b) => a + b, 0);

export function test_round() {
  assertEqual([0.5, 1.5, 2.5, -0.5, -1.5, 2.4999, 2.6].map(rnd), [0, 2, 2, 0, -2, 2, 3]);
  assertEqual([mod(-1, 3), mod(5.5, 2), mod(-0.25, 1)], [2, 1.5, 0.75]);
  assertEqual(mix(rgb(0, 0, 0), rgb(1, 3, 5), 0.5), rgb(0, 2, 2));
}

export function test_sun() {
  assert(near(sky(JUNE, 12).sun, 90 - 47 + 23.44, 0.5));
  assert(near(sky(DECEMBER, 12).sun, 90 - 47 - 23.44, 0.5));
  assert(sky(JUNE, 0).sun < -15 && sky(DECEMBER, 0).sun < -60);
  assert(sky(JUNE, 6).sunX < 0.5 && sky(JUNE, 12).sunX === 0.5 && 0.5 < sky(JUNE, 18).sunX);
  assert(sky(JUNE, 6, -35).sunX > 0.5 && 0.5 > sky(JUNE, 18, -35).sunX);
  assert(sky(DECEMBER, 12, -35).sun > sky(JUNE, 12, -35).sun);
  assert(sky(JUNE, 5).sun > 0 && sky(JUNE, 19).sun > 0);
  assert(sky(DECEMBER, 7).sun < 0 && sky(DECEMBER, 17).sun < 0);
  const s = at(JUNE, 89).sun;
  assert(-90 <= s && s <= 90);
}

export function test_moon() {
  assertEqual(phaseOf(NEW_MOON), 0);
  assert(near(phaseOf(FULL), 0.5, 1e-6));
  const day = (ms) => Math.floor(ms / DAY_MS) * DAY_MS;
  const full = at(day(FULL));
  assert(full.lit > 0.95 && full.moon > 10 && 0.3 < full.moonX && full.moonX < 0.7);
  const fresh = at(day(FULL + (SYNODIC / 2) * DAY_MS) + 12 * 3600000);
  assert(fresh.lit < 0.05 && Math.abs(fresh.moonX - fresh.sunX) < 0.1);
  assert(full.moonlight > 0.5 && fresh.moonlight === 0);
}

export function test_seasons() {
  const days = { spring: 110, summer: 200, autumn: 290, winter: 20 };
  for (const [name, d] of Object.entries(days)) assertEqual(seasonOf(d), name);
  assertEqual(seasonOf(360), "winter");
  assert(seasonOf(200, -30) === "winter" && seasonOf(20, -30) === "summer");
  assert(sky(JUNE, 12).season === "summer" && sky(JUNE, 12, -30).season === "winter");
  assertEqual(Object.keys(days).sort(), [...SEASONS].sort());
}

export function test_weather() {
  const seen = new Set();
  for (let d = 730000; d < 730400; d++) {
    for (const season of SEASONS) {
      const kind = weather(d, 7, season);
      assert(WEATHERS.includes(kind) && kind === weather(d, 7, season));
      assert(!(kind === "snow" && (season === "spring" || season === "summer")));
      seen.add(kind);
    }
  }
  assertEqual(seen.size, WEATHERS.length);
  assert(Array.from({ length: 50 }, (_, d) => weather(d, 1) !== weather(d, 2)).some(Boolean));
}

export function test_light() {
  const noon = sky(JUNE, 12);
  const night = sky(JUNE, 0);
  assert(noon.light === 1 && night.light === 0);
  assert(noon.stars === 0 && night.stars > 0.6 && sky(JUNE, 0, 47, 1.0).stars === 0);
  assert(0 < sky(JUNE, 4).light && sky(JUNE, 4).light < 1);
  assertEqual(noon.ambient, 0xffffff);
  assert(sum(night.ambient) < 300);
  const grass = rgb(90, 160, 40);
  assertEqual(noon.shade(grass), grass);
  assert(sum(night.shade(grass)) < sum(grass) / 3);
  const apart = (a, b) => channels(a).reduce((n, v, i) => n + Math.abs(v - channels(b)[i]), 0);
  const horizon = noon.colour(0.5, 0.9);
  assert(apart(noon.shade(grass, 1.0), horizon) < apart(grass, horizon));
}

export function test_colours() {
  for (const day of [JUNE, DECEMBER]) {
    for (let hour = 0; hour < 48; hour += 3) {
      for (const cover of [0.0, 0.6, 1.0]) {
        const s = sky(day, hour / 2, 47, cover);
        for (const u of [0.0, 0.5, 1.0]) for (const v of [0.0, 0.5, 1.0]) assert(Number.isInteger(s.colour(u, v)) && s.colour(u, v) >= 0 && s.colour(u, v) <= 0xffffff);
      }
    }
  }
  const noon = sky(JUNE, 12);
  const zenith = channels(noon.colour(0.5, 0));
  assert(zenith[2] > 200 && 200 > zenith[0]);
  assert(sum(sky(JUNE, 0).colour(0.5, 0)) < 120);
  const dusk = sky(JUNE, 19.8);
  const west = channels(dusk.colour(dusk.sunX, 1));
  const east = channels(dusk.colour(0.0, 1));
  assert(west[0] > west[2] && west[0] >= east[0]);
  const grey = channels(sky(JUNE, 12, 47, 1.0).colour(0.5, 0.3));
  assert(Math.max(...grey) - Math.min(...grey) < 40);
  assert(sky(JUNE, 12, 47, 1.0).ambient < noon.ambient);
}

// window's: the time of a scene.
export function test_era() {
  const now = Date.UTC(2026, 2, 7, 9, 41, 12, 345);
  const era = new Era(now, 60, [8, 5], "winter", 3);
  assertEqual(fields(era.start), { year: 2026, month: 1, day: 23, hour: 8, minute: 5, second: 0 });
  assertEqual(era.when(1.5) - era.start, 90000);
  assertEqual(new Era(now).start, Date.UTC(2026, 2, 7, 9, 41, 12));
  assert(near(moment("noon", 6, 20, 0.5), 13, 1e-9) && moment("night", 6, 20, 0.9) < 24);
  const [rise, fall] = sunTimes(Date.UTC(2026, 5, 21), 47);
  assert(rise > 3 && rise < 6 && fall > 18 && fall < 21);
  assertEqual(clockOf("7:30"), [7, 30]);
  assert(clockOf("24:00") === null && clockOf("x") === null);
}

export function test_drawn() {
  const o = drawn({ random: true, line: null, season: null, weather: null, at: null, lat: 47 }, "line", ["a", "b"], new Twister(5), Date.UTC(2026, 2, 7));
  assert(["a", "b"].includes(o.line) && SEASONS.includes(o.season) && o.seasons && o.at.length === 2);
  assert(o.weather !== "snow" || o.season === "winter");
  const kept = drawn({ random: false, line: null }, "line", ["a"], new Twister(5), 0);
  assert(kept.line === null && kept.days === 0);
}
