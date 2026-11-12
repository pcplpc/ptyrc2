// SPDX-License-Identifier: GPL-3.0-or-later
//! Plugin traits. Implementations live in `plugins/`.

use crate::scene::{EffectFactory, SpriteSpec};
use ptyrc_vt::{Grid, Rect};

/// A pane as a plugin sees it: a rectangle of the screen.
#[derive(Clone, Copy, Debug)]
pub struct PaneView<'a> {
    pub grid: &'a Grid,
    pub rect: Rect,
    /// Screen cursor, if inside `rect`.
    pub cursor: Option<(u16, u16)>,
    pub title: &'a str,
    pub alt_screen: bool,
}

impl PaneView<'_> {
    /// Text of a row of the pane (`row` relative to `rect`), spacers
    /// skipped.
    pub fn row_text(&self, row: u16) -> String {
        let y = self.rect.y + row;
        (self.rect.x..self.rect.right())
            .map(|x| self.grid.cell(x, y))
            .filter(|c| c.width != 0)
            .map(|c| c.text.as_str())
            .collect()
    }
}

/// Static margins inside a pane: rows and columns that do not scroll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub top: u16,
    pub bottom: u16,
    pub left: u16,
    pub right: u16,
}

/// Absolute position of the view's top-left in the frame, when the app
/// shows it (line numbers, ruler).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewHint {
    pub row: Option<i64>,
    pub col: Option<i64>,
}

/// Overrides of the frame heuristics. `None`: keep the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tuning {
    pub settle_ms: Option<u32>,
    pub commit_ms: Option<u32>,
}

/// Per-app knowledge. Consulted for a pane when `detect` is at least 0.5.
pub trait TuiSupport: Send {
    fn name(&self) -> &'static str;

    /// Confidence, 0 to 1, that the pane shows this app.
    fn detect(&self, pane: &PaneView) -> f32;

    fn chrome(&self, _pane: &PaneView) -> Option<Chrome> {
        None
    }

    fn view_hint(&self, _pane: &PaneView) -> Option<ViewHint> {
        None
    }

    /// Identity of what the pane shows (buffer name, window index).
    fn frame_key(&self, _pane: &PaneView) -> Option<String> {
        None
    }

    /// Panes of the area, replacing the generic segmentation.
    fn segment(&self, _area: &PaneView) -> Option<Vec<Rect>> {
        None
    }

    fn tuning(&self) -> Tuning {
        Tuning::default()
    }
}

/// A function of a draw extension, for `draw.exts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FnDoc {
    pub name: &'static str,
    /// Parameters, as a one-line JSON sketch.
    pub params: &'static str,
    pub doc: &'static str,
}

/// Extra draw functions: each turns its `params` into a sprite, which the
/// caller may then place (frame, anchor, z...).
pub trait DrawExt: Send {
    fn name(&self) -> &'static str;

    fn functions(&self) -> Vec<FnDoc>;

    fn draw(&self, function: &str, params: &serde_json::Value) -> Result<SpriteSpec, String>;

    /// Native effects, registered in the scene under these names.
    fn effects(&self) -> Vec<(&'static str, EffectFactory)> {
        vec![]
    }
}
