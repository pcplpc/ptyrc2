// SPDX-License-Identifier: GPL-3.0-or-later
//! The runner of store apps: one WebAssembly guest, ABI 1, its limits,
//! its canvases. See `README.md` and `plan/store-model.md`.

pub mod abi;
pub mod core;
mod engine;
pub mod error;
pub mod grants;
pub mod headless;
pub mod live;
pub mod runner;
pub mod script;

pub use crate::core::{CallError, Host, Limits};
pub use crate::error::Error;
pub use crate::runner::Runner;
