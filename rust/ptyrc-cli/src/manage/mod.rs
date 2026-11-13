// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc manage`: the apps of a session. Here: the menu only.

pub mod catalog;
pub mod menu;
mod overlay;
mod tty;
pub mod ui;

use crate::{Common, Fail};
use clap::Args;
use std::process::ExitCode;

#[derive(Args)]
pub struct Manage {
    /// The menu in a modal widget of the session (the hotkey)
    #[arg(long)]
    overlay: bool,
}

pub fn manage(common: &Common, args: Manage) -> Result<ExitCode, Fail> {
    ui::interactive(common, args.overlay)
}
