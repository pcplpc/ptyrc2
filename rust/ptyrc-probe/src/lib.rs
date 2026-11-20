// SPDX-License-Identifier: GPL-3.0-or-later
//! What the driver reads of the machine for store apps: `probe.git`
//! (`git`), `probe.system` (`system`). Parsers with no clock and no I/O;
//! readers that block, for a worker.

pub mod git;
pub mod system;
