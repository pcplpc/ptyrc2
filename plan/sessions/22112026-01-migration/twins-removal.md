# The removal of the Python twins (gap 30): what it consists of

For the owner: nothing below is done. Every app of
`plugins/client-apps/python` has its store tree (`plugins/store-apps/NAME/`;
`fishes --tank`: `tank`). Paths: `P` = `plugins/client-apps/python`,
`S` = `P/src/ptyrc_apps`, `T` = `P/tests`. Catalog: the tables of
`S/_catalog.py` naming the app (`APPS` is `S/__init__.py`). Taken
2026-11-22 at `ab32090`.

## Per app

| app | module | tests of its own | tests shared with others | catalog | `T/test_reads.py` | scripts | docs |
|---|---|---|---|---|---|---|---|
| `ask` | `S/ask.py` | `test_ask.py` | | APPS RANDOM | `EXTRA`, `OPTIONS` | | `P/README.md` row |
| `bricks` | `S/bricks.py`, `S/_bricks.py` | `test_bricks_look.py` | `test_bricks.py` (snake) | APPS RANDOM READS ASKS | `OPTIONS` | `demo-apps.sh` | row; `README.md`, `GALLERY.md`: `ptyrc app bricks` |
| `cat` | `S/cat.py` | `test_cat_play.py`, `test_cat_soak.py` | `test_apps.py` | APPS READS ASKS | `OPTIONS`; imports `cat` | `demo-apps.sh` | row |
| `clock` | `S/clock.py` | `test_clock.py` | `test_apps.py`, `test_timer.py` (digits) | APPS ASKS | `OPTIONS` | `demo-apps.sh`, `demo-frames.sh`, `e2e-manage.sh`, `e2e-tmux-attach.sh` | row; `P/README.md` head |
| `confetti` | `S/confetti.py` | | `test_apps.py` | APPS RANDOM READS | `EXTRA` | `demo-apps.sh` | row; `P/README.md` head |
| `fire` | `S/fire.py` | `test_fire.py` | | APPS RANDOM | | | row |
| `fishes` | `S/fishes.py`, `S/_reef.py` (cat too) | `test_tank.py`, `test_reef.py` | `test_apps.py` | APPS RANDOM READS | | `demo-apps.sh`, `demo-frames.sh`, `e2e-tmux-attach.sh` | row |
| `flock` | `S/flock.py` | `test_flock.py` | | APPS RANDOM READS ASKS | | | row |
| `flower` | `S/flower.py` | | `test_apps.py` | APPS RANDOM READS ASKS | `EXTRA` | `demo-apps.sh` | row |
| `gallery` | `S/gallery.py`, `S/pictures/` | `test_gallery.py` | | APPS RANDOM ASKS | `OPTIONS` | | row; `P/README.md` `--print` line |
| `git` | `S/git.py`, `S/_gitprobe.py` | `test_git.py` | | APPS RANDOM ASKS | | | row |
| `globe` | `S/globe.py` | `test_globe.py` | | APPS RANDOM | | | row |
| `life` | `S/life.py`, `S/_patterns.py` | `test_life.py` | | APPS RANDOM READS ASKS | `OPTIONS` | | row |
| `marks` | `S/marks.py` | | `test_apps.py`, `test_minimap.py` | APPS RANDOM READS ASKS | `EXTRA`, `OPTIONS` | `demo-apps.sh` | row |
| `minimap` | `S/minimap.py` | `test_minimap.py` | | APPS RANDOM READS | | | row |
| `monitor` | `S/monitor.py`, `S/_probe.py` | `test_monitor.py` | `test_apps.py` | APPS ASKS | | | row |
| `note` | `S/note.py` | `test_note.py` | | APPS RANDOM ASKS | `EXTRA` | | row |
| `pace` | `S/pace.py` | `test_pace.py` | | APPS RANDOM READS ASKS | `OPTIONS` | | row |
| `pond` | `S/pond.py` | | `test_pond.py` (stars) | APPS RANDOM READS ASKS | | | row |
| `rain` | `S/rain.py` | | `test_apps.py` | APPS READS | | `demo-apps.sh` | row |
| `sand` | `S/sand.py` | `test_nozzle.py` | `test_apps.py` | APPS RANDOM READS ASKS | `OPTIONS` | `demo-frames.sh` | row |
| `shore` | `S/shore.py` | `test_shore.py` | | APPS RANDOM READS ASKS | | | row |
| `snake` | `S/snake.py`, `S/_arcade.py` (life, `_bricks` too) | | `test_bricks.py`, `test_apps.py` | APPS RANDOM READS ASKS | `OPTIONS` | `demo-apps.sh` | row |
| `snow` | `S/snow.py` | `test_snow.py` | | APPS READS ASKS | `OPTIONS` | `demo-apps.sh` | row |
| `stars` | `S/stars.py` | `test_stars.py` | `test_pond.py` | APPS RANDOM READS ASKS | | | row |
| `timer` | `S/timer.py` | `test_timer.py` | | APPS RANDOM ASKS | `OPTIONS` | | row |
| `train` | `S/train.py`, `S/_line.py` | `test_train.py`, `test_line.py` | | APPS RANDOM ASKS | | | row |
| `window` | `S/window.py`, `S/_mouser.py` | `test_window.py`, `test_mouser.py` | | APPS RANDOM ASKS | | | row |

"row": its row in the table of `P/README.md`. "Shared with others": the
file goes with the last of the apps named; before, the app's tests are
cut out of it.

## With the last app

Every module of `S` is a twin or serves one: the last removal is the
package.

| what | where |
|---|---|
| helpers | `S/_cli.py`, `S/_widgets.py`, `S/_canvas.py`, `S/_sky.py`, `S/_strip.py`, `S/_catalog.py`, `S/__main__.py` (`ptyrc-app`) |
| their tests | `test_canvas.py`, `test_sky.py`, `test_strip.py`, `test_hover.py`, `test_interact.py`, `test_move.py`, `test_catalog.py`, `test_reads.py`, `test_apps.py`, `conftest.py` |
| the package | `P/` whole (`pyproject.toml`, `README.md`) |
| root (owner's) | `pyproject.toml` (`members`, `testpaths`), `uv.lock`, `justfile` (line 19), `README.md` (`ptyrc app bricks`; `ptyrc-app` into `~/.local/bin`), `GALLERY.md` |
| scripts | `demo-apps.sh`, `demo-frames.sh` (`demo_app`: `.venv/bin/ptyrc-app`; to `ptyrc app ptyrc/NAME`, their `--check` probes on the sprites the store apps draw), `lib-demo.sh` `demo_app`; `e2e-manage.sh`, `e2e-tmux-attach.sh` (`native/clock`, `native/fishes` in `permissions.toml`, `ptyrc-app` required) |
| Rust | `rust/ptyrc-cli/tests/storeapp.rs` ~2182 (skips without a native `confetti`); `ptyrc app NAME` then finds the store app alone (`manage/mod.rs` `store_app`). The launcher of a native app (`src/apps.rs`, `manage/catalog.rs` `--catalog`, the stand-ins of `tests/app.rs`, `auto.rs`, `overlay.rs`, `popup.rs`) serves any `ptyrc-app`: kept or removed, the owner's |
| docs | `plugins/store-apps/README.md`: "The native twin wins `ptyrc app NAME` while it exists"; `plan/store-model.md`, Migration; `README.md` of `P` |

`tank` has no twin of its own (`fishes --tank`, `test_tank.py`).
