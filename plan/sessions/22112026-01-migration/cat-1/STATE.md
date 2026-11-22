# cat: state of the migration (cat-1, for cat-2)

Tier R (Rust), as the plan's row: `plugins/store-apps/cat/`, model
`life`. `cat` does not use `_mouser` (that is `window`'s cat):
nothing moved to `_shared`. It imports `_reef.mirror/pad/paint`
(rewritten in `look.rs`) and `_canvas.Braille` (`ptyrc_app::canvas`).

## Done

1. Tree: manifest (options of the twin, `count` 1..12 by the
   manifest, `hunt`/`all` need `pointer`; no `seed`, no `front`),
   `Cargo.toml`, `LICENSE.txt`, `twister.rs` (copy of `_shared`).
   `lib.rs`: `Options::of` and an `App` that draws nothing yet.
2. `look.rs`: `POSES`, `Coat`, `Skin`, `dress` (the twin's regex by
   hand), `look`, `plate_look`, `mouse_look`, `fish_look`,
   `ball_look`, `mirror`, `pad`, `paint`; `Look::to_value` (`{cells}`
   or `{frames: [{cells, ms}], loop}`: the twin's content).
3. `place.rs`: `Ledge`, `ledges`, `mass`, `perch`, `near`, `arc`,
   `wave`, `hypot`, `round` (half to even), `set_order` (CPython's
   iteration of a small int set: the twin's `{s.x0, s.x1, randint}`
   in `Cat.pick`).
4. Fixtures: `src/twin.txt` by `cat-1/twin.py` (run in
   `plugins/client-apps/python`: `uv run python
   ../../../plan/sessions/22112026-01-migration/cat-1/twin.py >
   ../../store-apps/cat/src/twin.txt`); every look, ledges, mass,
   arc, wave, set order, near: equal. `test_cat_art`, `_ledges`,
   `_arc_and_near` of `test_apps.py` carried as unit tests.
5. Cases: `count` (refused by the manifest), `all` (blank: bless it
   again once the cats draw).
6. `yarn.rs`: the twin's `Yarn` but its word, `until`, `player`,
   sprites: `knock`, `fly`, `lay`, `sink`, `wake`, `strand`, `fade`,
   `show` (returns `Drawn`: thread at z -3, ball at z -2, for the
   world to put), `step`. `Ground` is `Home.solid` and `view[1]`;
   `stale` is set where the twin sets `home.stale` (the world reads
   and clears it). Fixture: two screens, 400 steps each, uneven
   steps, a floor taken away (`wake`): state bits, points, thread,
   puts: equal.

## Left, in order

1. `world.rs`: `Plate`, `Mouse`, `Fish`, `Cat`, `Home` of the
   twin, pure: `Home` holds the rows (`Vec<Vec<char>>`), `walls`,
   `view`, `spots`, cursor, pointer, idle; sprites through a trait
   (`put(id, x, y, look, z) -> id`, `remove`, `info(id) -> (visible,
   covered)`) so that unit tests record them. Objects by index (`Vec`
   of cats; `Option<Plate>` ...), the twin's `is` by index. Draws:
   `Twister` seeded from `Host` (live) or a number (tests): the
   twin's `random.Random(seed)` draw for draw.
2. Twin-exact: a `twin.py` driving `cat.Home` with a fake pilot (a
   scripted screen, cursor, idle, hover; `now` stepped) dumping the
   puts as digests a step; the same scenario in Rust. Do it per
   feature (one cat; `--food`; `--mice`; `--fish`; `--toys`; `--hunt`).
3. Glue (`lib.rs`): `Sight` with `words: food fish toy` for the rows
   (`#` ink, words as shown); topics `idle`, `cursor`, `screen`/
   `frames` (`Sight` takes `frames`, `text`), `hover` with `--hunt`;
   the mat (`--hunt`): a blank sprite over the view, `space: view`,
   z -9; `draw.list` for `check` (visible, covered: finding 14 of the
   plan: an anchored sprite reads `visible: false` in the tick of
   its `put`: read at the next tick). Time: `now_ms / 1000`; `period`
   33 while something is due within a tick, else longer (the twin
   sleeps until `due`); a batch per `together` (`call("batch")`).
   Layer back (no `--front`: the launcher's).
4. Cases: `test_cat_play.py` (10), `test_apps.py` `test_cat`,
   `_life`, `_count`, `_food`, `_served_and_mice`, `_options` (live:
   as cases on `--screen`, `--pointer`); the 5 soaks of
   `test_cat_soak.py`: measure headless `--for 600s` first (plan (c)).
   Not the cat's: `test_mouser.py` (window's `_mouser`, carried by
   `window`), `test_hover.py`, `test_interact.py` (`_widgets`).
5. `--timing` at 200x60 (release `ptyrc-box`, loadavg under 4), a
   live run (`decks-window/live.py`, `env.sh`), README models line.

## Traps

- `Cat.pick` iterates a set: use `set_order`, not the list order.
- `rng.sample(COATS, 8)` with `-r`; `rng.sample(spots, k)`: twister's
  `sample` is CPython's (pool for small n).
- `math.hypot` of ints: `place::hypot` (sqrt of the exact sum).
- `min(seats)` / `min(ends)` / `below` compare tuples: ties go to the
  next member (index, column).
- `Yarn.x` is `round(px) >> 1`: an arithmetic shift of a rounded
  float; `g` is `(round(py) + 3) // 4 + 1` (floor division).
- `int((now - due) / TICK)` truncates; `math.floor`, `math.ceil` of
  floats in `hits`.
- `str.isspace`: `look::is_space`.
