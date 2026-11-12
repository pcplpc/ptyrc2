// SPDX-License-Identifier: GPL-3.0-or-later
//! Types shared by the frames and the scene: the seam between them.

use ptyrc_vt::Rect;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub type FrameId = u32;
pub type SpriteId = u64;
pub type OwnerId = u64;

/// The raw frame: the whole screen.
pub const RAW: FrameId = 0;

/// A position in a frame's own coordinate space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FramePos {
    pub x: i64,
    pub y: i64,
}

/// A frame id, or a name: `raw`, `active`, `largest`, `quietest`, `idle`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FrameSel {
    Id(FrameId),
    Name(String),
}

impl Default for FrameSel {
    fn default() -> FrameSel {
        FrameSel::Id(RAW)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameKind {
    Raw,
    Pane,
    Chrome,
}

/// `Inactive`: remembered, not on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameState {
    Candidate,
    Stable,
    Inactive,
}

/// Where a frame is on screen. `origin` is the frame position shown at the
/// top-left cell of `rect`. A frame position `p` is at screen
/// `(rect.x + p.x - origin.x, rect.y + p.y - origin.y)` when inside `rect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceInfo {
    pub id: FrameId,
    pub kind: FrameKind,
    pub state: FrameState,
    pub rect: Rect,
    pub origin: FramePos,
    /// Drawing allowed: on screen and stable.
    pub usable: bool,
}

impl SurfaceInfo {
    /// Screen cell of a frame position, if shown.
    pub fn to_screen(&self, pos: FramePos) -> Option<(u16, u16)> {
        let dx = pos.x - self.origin.x;
        let dy = pos.y - self.origin.y;
        if dx < 0 || dy < 0 || dx >= self.rect.w as i64 || dy >= self.rect.h as i64 {
            return None;
        }
        Some((self.rect.x + dx as u16, self.rect.y + dy as u16))
    }

    /// Frame position of a screen cell inside `rect`.
    pub fn to_frame(&self, col: u16, row: u16) -> Option<FramePos> {
        if !self.rect.contains(col, row) {
            return None;
        }
        Some(FramePos {
            x: self.origin.x + (col - self.rect.x) as i64,
            y: self.origin.y + (row - self.rect.y) as i64,
        })
    }
}

/// Preferred region for automatic placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Prefer {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    #[default]
    Any,
}

/// A text search: literal, or a regular expression.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextQuery {
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub ignore_case: bool,
}

/// A match. `pos` is in frame coordinates, `len` in cells, `screen` the
/// screen cell of its first glyph when shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMatch {
    pub frame: FrameId,
    pub x: i64,
    pub y: i64,
    pub len: u16,
    pub text: String,
    pub screen: Option<(u16, u16)>,
}

/// A cursor position at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorSample {
    pub at: Duration,
    pub col: u16,
    pub row: u16,
}

/// What the scene asks of the frames.
pub trait Surfaces {
    fn info(&self, id: FrameId) -> Option<SurfaceInfo>;

    fn resolve(&self, sel: &FrameSel) -> Option<FrameId>;

    /// Matches in a frame, in reading order. `known`: also search what is
    /// remembered outside the view.
    fn find(&self, id: FrameId, query: &TextQuery, known: bool) -> Vec<TextMatch>;

    /// Best `w x h` spot of the frame's view, as a cell relative to the
    /// view's top-left, not overlapping `taken` (view-relative rects).
    fn place(
        &self,
        id: FrameId,
        w: u16,
        h: u16,
        prefer: Prefer,
        taken: &[Rect],
    ) -> Option<(u16, u16)>;

    /// Heat of a frame's view at `now`.
    fn heat(&self, _id: FrameId, _now: Duration) -> Option<crate::frames::HeatView> {
        None
    }

    /// Current cursor, screen coordinates.
    fn cursor(&self) -> (u16, u16);

    /// Recent cursor positions, oldest first.
    fn cursor_trail(&self) -> Vec<CursorSample>;
}
