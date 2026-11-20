# sdk/rust

The Rust tier of store apps. `ptyrc-app/README.md` is the API; `ptyrc
dev new --rust` copies it into the app as `vendor/ptyrc-app/`, so it
names nothing outside an app. What only the repository holds is here.

| | |
|---|---|
| `ptyrc-app/` | the guest crate: ABI 1 (`rust/ptyrc-box/README.md`), built as `rust/ptyrc-store/README.md`, Build, says |
| `examples/confetti/` | pops of colour, the twin of the Python `confetti` without `--fancy`, `--random`, `--fireworks`, `--sparkle`; `one-shot` exits once the last pop is played |
| `examples/sand/` | the effect `sand` of the driver, one `draw.put`, no permission; where the effect is missing (the test host), a pile of its own |
| `examples/snake/` | the text is the wall (`screen.shape`); `grab`: the arrows steer it (`input.grab`); each denied, it plays on |
| `examples/clockface/` | a stopwatch in a widget, the deck; a click pauses, a right click resets |
| `examples/tally/` | storage, the clock, the result |
| `tests/` | the examples built as the store builds them, every import of `ptyrc`, run headless; the host functions of both SDKs (`tests/tests/host.rs`) |

`sand`, `snake`, `clockface`, `tally` are twins of `sdk/js/examples/`:
the same cases, `tests/CASE.args` and `tests/CASE.txt` byte for byte
(`rust/ptyrc-cli/tests/dev.rs`). `./test.sh`: `ptyrc dev test .
--cases`, then `cargo test`; `--update` blesses.

```
cd sdk/rust/tests && cargo test
```

## By hand

What `ptyrc dev check`, `test`, `run` and the install do:

```
rustc --edition 2024 --target wasm32-unknown-unknown -C opt-level=s -C panic=abort \
  -C codegen-units=1 --crate-type rlib --crate-name ptyrc_app -o out/libptyrc_app.rlib \
  ptyrc-app/src/lib.rs
rustc --edition 2024 --target wasm32-unknown-unknown -C opt-level=s -C panic=abort \
  -C codegen-units=1 -C lto -C strip=symbols --crate-type cdylib --crate-name app \
  --extern ptyrc_app=out/libptyrc_app.rlib -L dependency=out -o out/app.wasm src/lib.rs
ptyrc-box out/app.wasm '{"opt": 1}'
```

`ptyrc-box` runs in a session (`PTYRC_SOCKET`); headless, through the
library:

```rust
// dev-dependencies: ptyrc-box, ptyrc-store (rust/), serde_json
use ptyrc_box::headless::{Options, run};
use ptyrc_store::{DirTree, build, check};
let tree = DirTree::new("myapp");
let checked = check(&tree)?;
let toolchain = build::Toolchain::find()?;
let wasm = build::build(&tree, &checked, Path::new("work"), &toolchain,
    &build::Options::default())?.wasm;
let report = run(&wasm, &Options { size: (80, 24), duration_ms: 1000,
    at: vec![0, 500], args: serde_json::json!({}), seed: 1,
    ..Options::default() })?;
print!("{report}");   // the screens, the requests refused, the limits met, the end
assert_eq!(report.end, None);
```
