// SPDX-License-Identifier: GPL-3.0-or-later
// The sky of a place at a time (the twin's `_sky`), and the time of a
// scene (the twin's `window.Era`, `drawn`): `train`, `window`.
//
// Pure: the time is an argument, a local naive time in ms: the wall
// time of the zone written as UTC (`naive(unixMs, offsetS)`), as the
// twin's naive `datetime`. `at(when, lat, cover)` gives a `Sky`:
//
// - `sun`, `moon`: altitudes in degrees; `sunX`, `moonX`: across a
//   view to the south (north under the equator), 0 left, 1 right.
// - `phase`: of the moon, 0 new, 0.5 full; `lit`: its lit share.
// - `light`: 0 at night to 1 by day; `stars`: how well they show.
// - `season`, by the day and the hemisphere.
// - `colour(u, v)`: the sky at `u` across, `v` down (0 zenith, 1
//   horizon). `shade(c, depth)`: a colour of the land under this light,
//   hazed by `depth`. `cover`, 0..1: clouds.
//
// Colours are numbers 0xrrggbb. `mix` and `rnd` round half to even, as
// the twin's `round`: the same pixels for the same time.

export const SEASONS = ["spring", "summer", "autumn", "winter"];
export const WEATHERS = ["clear", "clouds", "rain", "fog", "snow"];
export const SYNODIC = 29.530588853; // days from a new moon to the next
export const NEW_MOON = Date.UTC(2000, 0, 6, 18, 14); // naive ms
const TILT = 23.44;
const SPAN = 110.0;
const D2R = Math.PI / 180;
const R2D = 180 / Math.PI;
export const DAY_MS = 86400000;
/** `toordinal()` of 1970-01-01. */
const EPOCH_ORDINAL = 719163;

/** Python's `round(x)`: half to even. */
export function rnd(x) {
  const f = Math.floor(x);
  const d = x - f;
  if (d < 0.5) return f;
  if (d > 0.5) return f + 1;
  return f % 2 === 0 ? f : f + 1;
}

/** Python's `a % b`: the sign of `b`. */
export function mod(a, b) {
  const m = a % b;
  return m !== 0 && m < 0 !== b < 0 ? m + b : m;
}

export const rgb = (r, g, b) => (r << 16) | (g << 8) | b;
export const red = (c) => c >> 16;
export const green = (c) => (c >> 8) & 255;
export const blue = (c) => c & 255;

/** From `a` to `b`: `t` 0 is `a`, 1 is `b`; clamped. */
export function mix(a, b, t) {
  t = t < 0 ? 0 : t > 1 ? 1 : t;
  const ar = a >> 16;
  const ag = (a >> 8) & 255;
  const ab = a & 255;
  return (
    (rnd(ar + ((b >> 16) - ar) * t) << 16) |
    (rnd(ag + (((b >> 8) & 255) - ag) * t) << 8) |
    rnd(ab + ((b & 255) - ab) * t)
  );
}

// ---- the naive time ----

/** The wall time of `unixMs` at the offset `offsetS`, as naive ms. */
export const naive = (unixMs, offsetS) => unixMs + offsetS * 1000;

/** `{year, month, day, hour, minute, second}` of naive ms. */
export function fields(ms) {
  const d = new Date(Math.floor(ms));
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: d.getUTCSeconds(),
  };
}

/** `toordinal()` of the day of naive ms. */
export const ordinal = (ms) => Math.floor(ms / DAY_MS) + EPOCH_ORDINAL;

/** `tm_yday`: the day of the year, from 1. */
export function yday(ms) {
  const y = new Date(Math.floor(ms)).getUTCFullYear();
  return Math.floor(ms / DAY_MS) - Math.floor(Date.UTC(y, 0, 1) / DAY_MS) + 1;
}

/** The naive ms of the minute of `ms`. */
export const minute = (ms) => Math.floor(ms / 60000) * 60000;

// ---- the sky ----

const KEYS = [
  [-18.0, rgb(0, 0, 50), rgb(0, 6, 112)],
  [-9.0, rgb(0, 0, 95), rgb(80, 20, 130)],
  [-4.0, rgb(10, 10, 135), rgb(215, 95, 95)],
  [0.0, rgb(70, 90, 190), rgb(255, 140, 90)],
  [5.0, rgb(80, 135, 215), rgb(255, 205, 140)],
  [14.0, rgb(70, 140, 230), rgb(175, 215, 245)],
  [40.0, rgb(60, 130, 225), rgb(150, 205, 245)],
];
const DAWN = rgb(250, 150, 150);
const DUSK = rgb(255, 130, 60);
const GLOW = rgb(255, 200, 110);
const LIGHT = [
  [-18.0, rgb(42, 52, 104)],
  [-6.0, rgb(70, 80, 130)],
  [0.0, rgb(115, 125, 140)],
  [6.0, rgb(225, 205, 185)],
  [16.0, rgb(255, 255, 255)],
];

/** 0 under `lo`, 1 over `hi`, smooth between. */
export function step(lo, hi, x) {
  const t = Math.min(1.0, Math.max(0.0, (x - lo) / (hi - lo)));
  return t * t * (3 - 2 * t);
}

function keyed(keys, x) {
  if (x <= keys[0][0]) return [0, 0.0];
  for (let i = 0; i < keys.length - 1; i++) {
    if (x < keys[i + 1][0]) return [i, (x - keys[i][0]) / (keys[i + 1][0] - keys[i][0])];
  }
  return [keys.length - 2, 1.0];
}

/** The phase of the moon at naive ms `when`: 0 new, 0.5 full. */
export function phaseOf(when) {
  return mod(((when - NEW_MOON) * 1000) / 1e6 / 86400 / SYNODIC, 1.0);
}

/** The season of the day `day` of the year (1: 1 January). */
export function seasonOf(day, lat = 47.0) {
  if (lat < 0) day = (day + 183) % 365;
  if (day >= 355) return "winter";
  if (day >= 266) return "autumn";
  if (day >= 172) return "summer";
  return day >= 80 ? "spring" : "winter";
}

/** The weather of the day `day` (an ordinal), one of `WEATHERS`. */
export function weather(day, seed = 0, season = "summer") {
  let n = (Math.imul(day, 0x9e3779b1) ^ Math.imul(seed, 0x85ebca6b)) >>> 0;
  n = (n ^ (n >>> 15)) >>> 0;
  n = Math.imul(n, 0x2c1b3c6d) >>> 0;
  n = (n ^ (n >>> 12)) >>> 0;
  const draw = n % 100;
  if (draw < 40) return "clear";
  if (draw < 68) return "clouds";
  if (draw < 84) return season === "winter" ? "snow" : "rain";
  if (draw < 94) return "fog";
  return (season === "winter" || season === "autumn") && draw < 97 ? "snow" : "rain";
}

function altitude(lat, dec, angle) {
  const la = lat * D2R;
  const de = dec * D2R;
  const ha = angle * D2R;
  const s = Math.sin(la) * Math.sin(de) + Math.cos(la) * Math.cos(de) * Math.cos(ha);
  return Math.asin(Math.min(1.0, Math.max(-1.0, s))) * R2D;
}

function across(angle, lat) {
  const x = 0.5 + angle / (2 * SPAN);
  return lat >= 0 ? x : 1 - x;
}

export class Sky {
  constructor(o) {
    this.hour = o.hour; // of the day, 0..24
    this.day = o.day; // of the year, from 1
    this.lat = o.lat;
    this.sun = o.sun;
    this.sunX = o.sunX;
    this.moon = o.moon;
    this.moonX = o.moonX;
    this.phase = o.phase;
    this.cover = o.cover ?? 0.0;
    this.key = [this.hour, this.day, this.lat, this.sun, this.sunX, this.moon, this.moonX, this.phase, this.cover].join();
    this.lit = (1 - Math.cos(2 * Math.PI * this.phase)) / 2;
    this.light = step(-6.0, 8.0, this.sun);
    this.moonlight = this.lit * step(-2.0, 12.0, this.moon) * (1 - this.light);
    this.stars = (1 - step(-13.0, -3.0, this.sun)) * (1 - this.cover) * (1 - 0.35 * this.moonlight);
    this.season = seasonOf(this.day, this.lat);
    this.shades = new Map(); // `c,depth` -> shaded
    this.columns = new Map(); // `u` -> `column(u)`
    this._ambient = null;
    this._haze = null;
  }

  /** What `clear` takes of `u` alone: `[zenith, horizon, glow]`, kept
   * a column. */
  column(u) {
    let out = this.columns.get(u);
    if (out !== undefined) return out;
    const [i, t] = keyed(KEYS, this.sun);
    const low = this.hour < 12 ? DAWN : DUSK;
    const ends = [];
    for (const [alt, zenith, horizon0] of [KEYS[i], KEYS[i + 1]]) {
      let horizon = horizon0;
      if (-6.0 < alt && alt < 3.0) {
        const warm = Math.exp(-(((u - this.sunX) / 0.55) ** 2));
        horizon = mix(horizon, low, 0.25 + 0.5 * warm);
      }
      ends.push([zenith, horizon]);
    }
    let glow = null;
    if (-8.0 < this.sun && this.sun < 30.0) {
      const near = Math.exp(-(((u - this.sunX) / 0.22) ** 2));
      const height = step(-8.0, 1.0, this.sun) * (1 - step(8.0, 30.0, this.sun));
      glow = 0.45 * near * height;
    }
    out = [mix(ends[0][0], ends[1][0], t), mix(ends[0][1], ends[1][1], t), glow];
    if (this.columns.size >= 1024) this.columns.clear();
    this.columns.set(u, out);
    return out;
  }

  clear(u, v) {
    const [zenith, horizon, glow] = this.column(u);
    let c = mix(zenith, horizon, Math.min(1.0, Math.max(0.0, v)) ** 1.6);
    if (glow !== null) c = mix(c, GLOW, glow * Math.max(0.0, v) ** 2);
    if (this.moonlight) c = mix(c, rgb(70, 90, 160), 0.22 * this.moonlight);
    return c;
  }

  /** The sky at `u` across and `v` down to the horizon. */
  colour(u, v) {
    let c = this.clear(u, v);
    if (this.cover) {
      let grey = mix(rgb(14, 16, 30), rgb(146, 150, 158), this.light);
      grey = mix(grey, c, 0.12);
      const lower = mix(grey, 0xffffff, 0.12 * v * this.light);
      c = mix(c, lower, this.cover);
    }
    return c;
  }

  /** The light on the land: white at noon. */
  get ambient() {
    if (this._ambient !== null) return this._ambient;
    const [i, t] = keyed(LIGHT, this.sun);
    let c = mix(LIGHT[i][1], LIGHT[i + 1][1], t);
    if (this.moonlight) c = mix(c, rgb(110, 125, 175), 0.5 * this.moonlight);
    if (this.cover) {
      const dull = Math.floor((red(c) + green(c) + blue(c)) / 3);
      const d = Math.trunc(dull * 0.8);
      c = mix(c, rgb(d, d, d), 0.7 * this.cover);
    }
    return (this._ambient = c);
  }

  /** `c` under this light, hazed by `depth`. */
  shade(c, depth = 0.0) {
    const k = c * 8 + depth;
    let out = this.shades.get(k);
    if (out !== undefined) return out;
    const a = this.ambient;
    const lit = rgb(Math.floor((red(c) * red(a)) / 255), Math.floor((green(c) * green(a)) / 255), Math.floor((blue(c) * blue(a)) / 255));
    const haze = 0.5 * depth * (0.25 + 0.75 * this.light);
    if (this._haze === null) this._haze = this.colour(0.5, 0.9);
    out = mix(lit, this._haze, haze);
    if (this.shades.size >= 4096) this.shades.clear();
    this.shades.set(k, out);
    return out;
  }
}

const SKIES = new Map();

/** The sky at the naive time `when` (ms), at the latitude `lat`. The
 * same arguments, the same `Sky`: what it memoises is kept. */
export function at(when, lat = 47.0, cover = 0.0) {
  const key = `${when},${lat},${cover}`;
  let sky = SKIES.get(key);
  if (sky) return sky;
  const f = fields(when);
  const hour = f.hour + f.minute / 60 + f.second / 3600;
  const day = yday(when);
  const phase = phaseOf(when);
  lat = Math.min(66.0, Math.max(-66.0, lat));
  const lean = (d) => -TILT * Math.cos((2 * Math.PI * (d + 10)) / 365);
  const angle = 15 * (hour - 12);
  const trail = mod(angle - 360 * phase + 180, 360) - 180;
  sky = new Sky({
    hour,
    day,
    lat,
    sun: altitude(lat, lean(day), angle),
    sunX: across(angle, lat),
    moon: altitude(lat, lean(day + 365 * phase), trail),
    moonX: across(trail, lat),
    phase,
    cover: Math.min(1.0, Math.max(0.0, cover)),
  });
  if (SKIES.size >= 64) SKIES.clear();
  SKIES.set(key, sky);
  return sky;
}

// ---- the time of a scene ----

/** The middle of each season, `[month, day]`. */
export const MIDDLE = { spring: [4, 20], summer: [7, 20], autumn: [10, 20], winter: [1, 20] };
/** `-r`: the moments of a day, and how often each is drawn. */
export const MOMENTS = { dawn: 2, morning: 2, noon: 2, afternoon: 2, golden: 3, dusk: 3, evening: 3, night: 1 };
/** `-r`: how often a weather is drawn; no snow but in winter. */
export const DRAWN_SKIES = { clear: 34, clouds: 28, rain: 14, fog: 10, snow: 14 };

/** The time of a scene: `when(t)`, the naive time after `t` seconds,
 * `speed` seconds of the scene a second. It starts at `start` (naive
 * ms): at `at` (`[hour, minute]`), in the middle of `season`, `days`
 * later. */
export class Era {
  constructor(start, speed = 1.0, at = null, season = null, days = 0) {
    const f = fields(start);
    let s = f.second;
    if (at !== null) {
      [f.hour, f.minute] = at;
      s = 0;
    }
    if (season !== null) [f.month, f.day] = MIDDLE[season];
    this.start = Date.UTC(f.year, f.month - 1, f.day, f.hour, f.minute, s) + days * DAY_MS;
    this.speed = speed;
  }

  /** Naive ms, to the microsecond as the twin's `timedelta`. */
  when(t) {
    return this.start + rnd(t * this.speed * 1e6) / 1000;
  }
}

/** The hours the sun rises and sets at, the day of naive ms `day`. */
export function sunTimes(day, lat) {
  const up = [];
  for (let m = 0; m < 288; m++) if (at(hourOf(day, m / 12), lat).sun > 0) up.push(m / 12);
  return up.length ? [up[0], up[up.length - 1]] : [12.0, 12.0];
}

function hourOf(day, hour) {
  const minutes = mod(rnd(mod(hour, 24) * 60), 1440);
  const f = fields(day);
  return Date.UTC(f.year, f.month - 1, f.day, Math.floor(minutes / 60), minutes % 60, 0);
}

/** The hour of the moment `name` of `MOMENTS`, the sun rising at `rise`
 * and setting at `fall`; `draw`, 0..1: where in it. */
export function moment(name, rise, fall, draw) {
  const noon = (rise + fall) / 2;
  const [a, b] = {
    dawn: [rise - 0.4, rise + 0.3],
    morning: [rise + 0.8, noon - 1.5],
    noon: [noon - 1.5, noon + 1.5],
    afternoon: [noon + 1.5, fall - 1.2],
    golden: [fall - 1.0, fall - 0.2],
    dusk: [fall - 0.1, fall + 0.6],
    evening: [fall + 0.8, Math.max(fall + 1.0, 23.0)],
    night: [fall + 4.0, rise + 23.0],
  }[name];
  return mod(a + Math.max(0.0, b - a) * draw, 24);
}

/** `-r`: draw what `o` leaves open, from `rng` (a `Twister` of the
 * seed): `o[key]` among `among`, the season and a day of it, the
 * weather, the time (`o.at`, `[hour, minute]`). `o.days` is set; the
 * season shows (`o.seasons`). `now`: naive ms of the start. */
export function drawn(o, key, among, rng, now) {
  o.days = 0;
  if (!o.random) return o;
  if (o[key] == null || o[key] === "random") o[key] = rng.choice(among);
  if (o.season == null) {
    o.season = rng.choice(SEASONS);
    o.days = rng.randrange(-28, 29);
  }
  o.seasons = true;
  if (o.weather == null) {
    const names = Object.keys(DRAWN_SKIES).filter((k) => k !== "snow" || o.season === "winter");
    o.weather = rng.weighted(names, names.map((k) => DRAWN_SKIES[k]));
  }
  if (o.at == null) {
    const day = new Era(now, 1.0, null, o.season, o.days).start;
    const names = Object.keys(MOMENTS);
    const name = rng.weighted(names, names.map((k) => MOMENTS[k]));
    const [rise, fall] = sunTimes(day, o.lat);
    const hour = moment(name, rise, fall, rng.random());
    o.at = [Math.trunc(hour), Math.trunc(mod(hour, 1) * 60)];
  }
  return o;
}

/** `HH:MM` as `[hour, minute]`, or null. */
export function clockOf(text) {
  const m = /^\s*([+-]?\d+)\s*:\s*([+-]?\d+)\s*$/.exec(String(text));
  if (!m) return null;
  const h = Number(m[1]);
  const mi = Number(m[2]);
  return h >= 0 && h < 24 && mi >= 0 && mi < 60 ? [h, mi] : null;
}
