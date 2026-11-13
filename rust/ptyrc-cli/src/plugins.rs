// SPDX-License-Identifier: GPL-3.0-or-later
//! What is linked into the binary: plugin crates and the engine.

use ptyrc_core::plugin::{DrawExt, TuiSupport};
use ptyrc_server::{Engine, PassThrough, ScreenEngine};

/// Per-app presets, in order of precedence.
pub fn tui_support() -> Vec<Box<dyn TuiSupport>> {
    vec![
        Box::new(ptyrc_tui_tmux::Tmux),
        Box::new(ptyrc_tui_vim::Vim),
        Box::new(ptyrc_tui_ranger::Ranger),
        Box::new(ptyrc_tui_htop::Htop),
        Box::new(ptyrc_tui_glances::Glances),
        Box::new(ptyrc_tui_agent::AgentTui),
    ]
}

/// Draw extensions.
pub fn draw_ext() -> Vec<Box<dyn DrawExt>> {
    vec![
        Box::new(ptyrc_draw_shapes::Shapes),
        Box::new(ptyrc_draw_bigtext::BigText),
        Box::new(ptyrc_draw_sand::SandExt),
        Box::new(ptyrc_draw_rain::RainExt),
        Box::new(ptyrc_draw_snow::SnowExt),
        Box::new(ptyrc_draw_borders::Borders),
    ]
}

/// The engine the driver runs. `transparent`: plain pass-through, no
/// screen model. `presets`: with the per-app presets.
pub fn engine(transparent: bool, presets: bool) -> Box<dyn Engine> {
    if transparent {
        Box::new(PassThrough)
    } else {
        let supports = if presets { tui_support() } else { Vec::new() };
        Box::new(ScreenEngine::new(supports, draw_ext()))
    }
}
