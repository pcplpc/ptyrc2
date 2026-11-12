// SPDX-License-Identifier: GPL-3.0-or-later
//! What is linked into the binary: plugin crates and the engine.

use ptyrc_core::plugin::{DrawExt, TuiSupport};
use ptyrc_server::{Engine, PassThrough, ScreenEngine};

/// Per-app presets, in order of precedence.
pub fn tui_support() -> Vec<Box<dyn TuiSupport>> {
    vec![
        Box::new(ptyrc_tui_tmux::Tmux),
        Box::new(ptyrc_tui_vim::Vim),
        Box::new(ptyrc_tui_agent::AgentTui),
    ]
}

/// Draw extensions.
pub fn draw_ext() -> Vec<Box<dyn DrawExt>> {
    vec![
        Box::new(ptyrc_draw_shapes::Shapes),
        Box::new(ptyrc_draw_bigtext::BigText),
    ]
}

/// The engine the driver runs. `transparent`: plain pass-through, no
/// screen model.
pub fn engine(transparent: bool) -> Box<dyn Engine> {
    if transparent {
        Box::new(PassThrough)
    } else {
        Box::new(ScreenEngine::new(tui_support(), draw_ext()))
    }
}
