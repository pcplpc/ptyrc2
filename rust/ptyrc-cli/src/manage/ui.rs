// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Common, Fail};
use std::process::ExitCode;
pub fn interactive(_common: &Common, _overlay: bool) -> Result<ExitCode, Fail> {
    Ok(ExitCode::SUCCESS)
}
