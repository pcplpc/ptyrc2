// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal model: bytes to `Grid`, and the repair painter. See
//! `plan/design.md`.

pub mod grid;

pub use grid::{Attrs, Cell, Color, Grid, Rect, Style};
