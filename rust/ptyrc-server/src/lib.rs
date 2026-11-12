// SPDX-License-Identifier: GPL-3.0-or-later
//! The driver: owns the pty of the wrapped app, sits between it and the
//! host terminal, serves the protocol. See `plan/design.md`.
//!
//! The driver is transparent plumbing. What is drawn and what the protocol
//! methods answer is the `Engine`'s business.

mod conn;
pub mod driver;
pub mod engine;
mod hints;
pub mod host;
pub mod keys;
pub mod pty;
mod rpc;
pub mod screen;
#[cfg(test)]
mod screen_tests;
pub mod start;

pub use driver::{Config, run};
pub use engine::{ConnId, Ctx, Engine, PassThrough, Reply, SessionInfo, Token};
pub use screen::ScreenEngine;
pub use start::{RunError, RunErrorKind};
