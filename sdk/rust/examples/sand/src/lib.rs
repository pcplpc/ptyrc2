// SPDX-License-Identifier: GPL-3.0-or-later
//! Sand: a thin stream piles on the text of the frame.
//!
//! One request: a sprite whose content is the effect `sand` of the
//! driver. The simulation runs there; it reads the text under it as its
//! walls at each render and answers nothing (`plan/store-model.md`,
//! principle 2). `--hover`, `--bomb`: the driver tells the effect the
//! pointer (`pointer`, `bomb` in its params); the app is told nothing.
//! It holds no permission but `clicks-grab` (`--bomb`), and then only
//! waits. The twin of `sdk/js/examples/sand`.

use ptyrc_app::{App, Value, call, json, log, random_u32};

struct Sand;

impl App for Sand {
    fn init(args: Value) -> Sand {
        let rate = args["rate"].as_f64().filter(|r| *r > 0.0).unwrap_or(60.0);
        let cursor = args["cursor"].as_bool() == Some(true);
        let mut params = json!({"rate": rate.min(1000.0), "cursor": cursor, "seed": random_u32()});
        for (option, flag) in [("hover", "pointer"), ("bomb", "bomb")] {
            if args[option].as_bool() == Some(true) {
                params[flag] = json!(true);
            }
        }
        let put = json!({"effect": {"name": "sand", "params": params}});
        if let Err(e) = call("draw.put", put) {
            log(&format!("sand: the effect: {e}"));
            ptyrc_app::exit(1);
        }
        Sand
    }
}

ptyrc_app::app!(Sand);
