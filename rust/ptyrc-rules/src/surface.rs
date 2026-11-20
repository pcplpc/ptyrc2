// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a store app draws: `[app] surface`, on the frame it was
//! started on (`plan/store-model.md`, the surface). A draw is clipped,
//! never refused, nothing told.

use crate::lower::NOWHERE;
use ptyrc_core::FrameId;
use serde_json::Value;

/// `[app] surface` on its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Its widgets alone: a bare sprite is shown nowhere.
    Widget,
    /// Clipped to that frame.
    Frame(FrameId),
    /// Anywhere with `screen-wide`; else as `Frame`.
    Screen(FrameId),
}

impl Surface {
    /// The manifest's word on `frame`; `None`: not a surface.
    pub fn parse(name: &str, frame: FrameId) -> Option<Surface> {
        match name {
            "widget" => Some(Surface::Widget),
            "frame" => Some(Surface::Frame(frame)),
            "screen" => Some(Surface::Screen(frame)),
            _ => None,
        }
    }

    /// Its frame, when it has one.
    pub fn frame(self) -> Option<FrameId> {
        match self {
            Surface::Frame(frame) | Surface::Screen(frame) => Some(frame),
            Surface::Widget => None,
        }
    }
}

/// The frame a request of a store app is put on, whatever it named:
/// a new draw (`draw.put`, `draw.ext` without `id`), an update naming
/// a frame, a `widget.show`. `Widget`: a bare sprite on [`NOWHERE`], a
/// widget as named; `Frame`: its frame; `Screen`: as named with
/// `screen-wide`, else its frame. `None`: as named.
pub fn clip(surface: Surface, screen_wide: bool, method: &str, params: &Value) -> Option<FrameId> {
    let widget = match method {
        "draw.put" => false,
        "draw.ext" if params.get("id").is_none() => false,
        "draw.update" | "draw.ext" if params.get("frame").is_some_and(|f| !f.is_null()) => false,
        "widget.show" => true,
        _ => return None,
    };
    match surface {
        Surface::Widget if widget => None,
        Surface::Widget => Some(NOWHERE),
        Surface::Frame(frame) => Some(frame),
        Surface::Screen(_) if screen_wide => None,
        Surface::Screen(frame) => Some(frame),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_surface_clips_as_the_driver() {
        let put = json!({"frame": 3});
        let clip = |s, wide, m: &str, p: &Value| clip(s, wide, m, p);
        assert_eq!(
            clip(Surface::Widget, false, "draw.put", &put),
            Some(NOWHERE)
        );
        assert_eq!(clip(Surface::Widget, true, "draw.put", &put), Some(NOWHERE));
        assert_eq!(clip(Surface::Widget, false, "widget.show", &put), None);
        assert_eq!(clip(Surface::Frame(5), false, "draw.put", &put), Some(5));
        assert_eq!(
            clip(Surface::Frame(5), false, "widget.show", &json!({})),
            Some(5)
        );
        assert_eq!(clip(Surface::Screen(5), false, "draw.put", &put), Some(5));
        assert_eq!(clip(Surface::Screen(5), true, "draw.put", &put), None);
        // An update naming no frame stays where it is; `draw.ext` of a
        // sprite, the same.
        assert_eq!(
            clip(Surface::Widget, false, "draw.update", &json!({"id": 1})),
            None
        );
        let named = json!({"id": 1, "frame": 0});
        assert_eq!(
            clip(Surface::Widget, false, "draw.update", &named),
            Some(NOWHERE)
        );
        assert_eq!(clip(Surface::Frame(2), false, "draw.ext", &named), Some(2));
        assert_eq!(
            clip(Surface::Frame(2), false, "draw.ext", &json!({"id": 1})),
            None
        );
        assert_eq!(
            clip(Surface::Frame(2), false, "draw.ext", &json!({"name": "x"})),
            Some(2)
        );
        assert_eq!(
            clip(Surface::Frame(2), false, "draw.list", &json!({})),
            None
        );
        assert_eq!(
            Surface::parse("screen", 4).and_then(Surface::frame),
            Some(4)
        );
        assert_eq!(Surface::parse("pane", 4), None);
    }
}
