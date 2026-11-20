// SPDX-License-Identifier: GPL-3.0-or-later
//! The rules of an app's requests, one copy for the driver
//! (`ptyrc-server`) and the headless runner (`ptyrc-box`): what a
//! permission opens (`grants`), the answers cut to a degree (`lower`),
//! the pointer told to an effect (`nozzle`), where a store app draws
//! (`surface`). Pure: no I/O, no clock.

pub mod grants;
pub mod lower;
pub mod nozzle;
pub mod surface;
