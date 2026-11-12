// SPDX-License-Identifier: GPL-3.0-or-later
//! What is linked into the binary: plugin crates and the engine.

use ptyrc_core::plugin::TuiSupport;
use ptyrc_server::{Engine, PassThrough};

/// Per-app presets, in order of precedence.
pub fn tui_support() -> Vec<Box<dyn TuiSupport>> {
    Vec::new()
}

/// The engine the driver runs.
pub fn engine() -> Box<dyn Engine> {
    let _ = tui_support();
    Box::new(PassThrough)
}
