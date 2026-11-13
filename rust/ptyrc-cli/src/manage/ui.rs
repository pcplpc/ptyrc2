// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu of `ptyrc manage`. A stand-in: the list, no menu.

use crate::{Common, Fail};
use std::process::ExitCode;

/// The menu on this terminal, or (`overlay`) in a modal widget of the
/// session.
pub fn interactive(common: &Common, overlay: bool) -> Result<ExitCode, Fail> {
    if overlay {
        return Err(Fail {
            status: 5,
            message: "manage --overlay: not built".into(),
            hint: None,
        });
    }
    super::ls(common)
}
