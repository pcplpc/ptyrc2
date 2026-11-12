// SPDX-License-Identifier: GPL-3.0-or-later
//! Cursor and events: what the emulator reports besides the grid.

use crate::grid::Rect;

/// Cursor of a `Vt`. `pending_wrap`: a glyph was written in the last
/// column and the next one wraps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub col: u16,
    pub row: u16,
    pub visible: bool,
    pub pending_wrap: bool,
}

/// What happened during `Vt::feed`, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtEvent {
    /// Content of `region` moved by `dy` rows (negative: up). `scrollback`:
    /// rows left the top of the main screen, into the host scrollback.
    Scroll {
        region: Rect,
        dy: i16,
        scrollback: bool,
    },
    /// Entered (`true`) or left the alternate screen.
    AltScreen(bool),
    /// The whole screen was erased.
    Clear,
    Title,
    Bell,
}
