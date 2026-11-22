# cat: state of the migration (cat-2; supersedes cat-1's)

Tier R (Rust), as the plan's row: `plugins/store-apps/cat/`, model
`life`. `cat` does not use `_mouser` (that is `window`'s cat):
nothing moved to `_shared`. It imports `_reef.mirror/pad/paint`
(rewritten in `look.rs`) and `_canvas.Braille` (`ptyrc_app::canvas`).

## Done

1. Tree (cat-1): manifest (options of the twin, `count` 1..12 by the
   manifest, `hunt`/`all` need `pointer`; no `seed`, no `front`),
   `Cargo.toml`, `LICENSE.txt`, `twister.rs` (copy of `_shared`).
2. `look.rs`, `place.rs`, `yarn.rs` (cat-1): every look, the ledges,
   the ball and its thread; fixture `src/twin.txt` by
   `cat-1/twin.py`.
3. `world.rs` (cat-2): `Plate`, `Mouse`, `Fish`, `Toy` (the yarn and
   what the home knows of it), `Cat`, `Home`, all of the twin's
   features. Objects by index (cats), by key (plates); `cat.fish`,
   `cat.toy` are bools (one fish, one ball at most). Draws through
   `Stage` (`put`, `set`, `remove`, `infos`, `begin`/`end` = the
   twin's `together`, `mat`, `view`). Fixture `src/world.txt` by
   `cat-2/twin.py` (run as cat-1's): the twin's `cat.Home` on a fake
   pilot, 8 scenes (one, three, food, mice, fish, toys, hunt, all;
   150 to 240 s of ticks, idle, cursor, screen, cover, hover events),
   a chained FNV digest a second of every call and of each cat's
   state and `due` (f64 bits) after each step: all equal. On a
   difference the test prints the Rust lines of that second;
   `twin.py show SCENE SECOND` prints the twin's.

4. Glue (`lib.rs`, cat-2): `Live`, the host as a `Stage`: sprites by
   handle (a `draw.put` in a batch gets its id from the batch's
   results), `together` = one `batch`, `infos` = one `draw.list`,
   the mat (`space: view`, z -9). `Sight` with `words: food fish
   toy` and `cursor`; holding `words` is holding more than `layout`:
   frame cells, the view's box from `sight.info.origin`, the cursor
   (view cells) plus the origin, `hover` taken when its `frame` is
   ours (its `sprite` is the mat). Topics: `idle` (snapshot),
   `hover` with `--hunt`, buffered to the next tick. `home.stale` on
   a new read of the view (`sight.changed`) and on `frames`, `size`,
   `move`. Time: `now_ms / 1000`; the period: what is next due,
   33 to 250 ms; 33 after an event.
5. Cases (`tests/*.args`): `cat`, `life` (idle by `--event`),
   `count`, `count13` (refused), `food` (the words), `served`
   (`--food --mice`), `play` (`--fish --toys`), `hunt` (`--pointer`),
   `all`. `dev check` clean, harness green (`PTYRC_STORE_APP=cat`).
   `test_cat_life`, `test_cat_pointer`, `_fish`, `_fish_stolen`,
   `_toy` drive the twin's `Home` on a session: their paths are in
   the scenes of `world.txt` (acts seen there: eat, hiss, bat, roll,
   beg, purr, stare, puzzled, wiggle, run, sleep, yawn).

6. Soaks: `world::tests::soaks`, the six of `test_cat_soak` on the
   pure world (the twin's `sound`, `LONG`, `SPIN`, steps a second,
   the sprites held, all gone at the end); `tests/soak`, headless,
   600 s of `-a --count 4` (5 s of wall time).
7. `--timing`, release `ptyrc-box`, 200x60, `games-snake/big.screen`,
   `-a --count 12`, loadavg 1.1: 916 ticks, mean 0.105 ms, max
   1.5 ms. Live run (`cat-2/live.py`, `cat-2/env.sh`, the driver in
   `temp/live-cat`): `--hunt --count 2`, plates, a fish, a ball and
   its thread by the words, a cat puzzled after the pointer, a clean
   stop (`cat-2/live.txt`). README models line.

## Left, in order

1. Done (close-1): `world::tests::toy_left_played_out`, `toy_move`
   (a move: a stale read; the ball kept), `toy_thread_taken`,
   `spooked_away`.
2. Not the store app's: the twin's `Home.move` (a frame replaced:
   one frame, told by `Sight`); `--seed`, `--front`.
3. The wave's end (not cat's): `_catalog.py`.

## Traps

As cat-1's: `set_order`, `hypot`, `round`, tuple `min` ties, `is_space`.
`seat`'s `min` over `(d, i, plate, right)`: equal plates compare
equal, then `right` (False first).
