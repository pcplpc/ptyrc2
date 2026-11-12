// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal model: bytes to `Grid`, and the repair painter. See
//! `plan/design.md`.

pub mod event;
pub mod grid;
pub mod repair;
mod sgr;
pub mod vt;

pub use event::{Cursor, VtEvent};
pub use grid::{Attrs, Cell, Color, Grid, Rect, Style};
pub use repair::{render_ansi, repair};
pub use sgr::write_sgr;
pub use vt::{Charset, Link, Modes, Vt};
