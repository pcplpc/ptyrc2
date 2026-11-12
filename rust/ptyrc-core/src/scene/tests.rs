// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::types::{CursorSample, FrameKind, FrameState, Prefer, TextMatch};
use ptyrc_vt::Color;
use serde_json::json;
use std::cell::RefCell;

const PANE: FrameId = 7;

/// Frames as the tests want them. `lines` is the content of `PANE` in
/// frame coordinates, row 0 first.
struct Mock {
    infos: Vec<SurfaceInfo>,
    lines: Vec<String>,
    cursor: (u16, u16),
    spot: Option<(u16, u16)>,
    taken_seen: RefCell<Vec<Vec<Rect>>>,
}

fn surface(id: FrameId, rect: Rect, origin: (i64, i64)) -> SurfaceInfo {
    SurfaceInfo {
        id,
        kind: if id == RAW {
            FrameKind::Raw
        } else {
            FrameKind::Pane
        },
        state: FrameState::Stable,
        rect,
        origin: FramePos {
            x: origin.0,
            y: origin.1,
        },
        usable: true,
    }
}

impl Mock {
    /// A 20x6 screen; `PANE` is its right half, rows 1..5.
    fn new() -> Mock {
        Mock {
            infos: vec![
                surface(RAW, Rect::new(0, 0, 20, 6), (0, 0)),
                surface(PANE, Rect::new(10, 1, 10, 4), (0, 0)),
            ],
            lines: Vec::new(),
            cursor: (0, 0),
            spot: Some((0, 0)),
            taken_seen: RefCell::new(Vec::new()),
        }
    }

    fn pane(&mut self) -> &mut SurfaceInfo {
        &mut self.infos[1]
    }
}

impl Surfaces for Mock {
    fn info(&self, id: FrameId) -> Option<SurfaceInfo> {
        self.infos.iter().find(|i| i.id == id).copied()
    }

    fn resolve(&self, sel: &FrameSel) -> Option<FrameId> {
        match sel {
            FrameSel::Id(id) => self.info(*id).map(|i| i.id),
            FrameSel::Name(n) if n == "raw" => Some(RAW),
            FrameSel::Name(n) if n == "active" => Some(PANE),
            FrameSel::Name(_) => None,
        }
    }

    fn find(&self, id: FrameId, query: &TextQuery, _known: bool) -> Vec<TextMatch> {
        let mut out = Vec::new();
        if id != PANE {
            return out;
        }
        for (y, line) in self.lines.iter().enumerate() {
            for (x, _) in line.match_indices(&query.pattern) {
                out.push(TextMatch {
                    frame: id,
                    x: x as i64,
                    y: y as i64,
                    len: query.pattern.len() as u16,
                    text: query.pattern.clone(),
                    screen: None,
                });
            }
        }
        out
    }

    fn place(
        &self,
        _id: FrameId,
        _w: u16,
        _h: u16,
        _prefer: Prefer,
        taken: &[Rect],
    ) -> Option<(u16, u16)> {
        self.taken_seen.borrow_mut().push(taken.to_vec());
        self.spot
    }

    fn cursor(&self) -> (u16, u16) {
        self.cursor
    }

    fn cursor_trail(&self) -> Vec<CursorSample> {
        Vec::new()
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn spec(v: serde_json::Value) -> SpriteSpec {
    serde_json::from_value(v).unwrap()
}

fn base() -> Grid {
    Grid::new(20, 6)
}

/// Tick at `now`, compose over `base`, return the text rows.
fn show(scene: &mut Scene, mock: &Mock, base: &Grid, now: u64) -> Vec<String> {
    scene.tick(ms(now), mock);
    scene.compose(base, mock).text()
}

#[test]
fn minimal_spec_defaults() {
    let s = spec(json!({"content": {"rows": ["hi"]}}));
    assert_eq!(s.frame, FrameSel::Id(RAW));
    assert_eq!(
        s.anchor,
        Anchor::At {
            at: [0, 0],
            space: Space::View
        }
    );
    assert_eq!(s.blend, Blend::Over);
    assert_eq!(s.looping, LoopSpec::Flag(false));
    assert!(!s.persist);
    let mut scene = Scene::new();
    scene.add(1, s, ms(0)).unwrap();
    assert_eq!(show(&mut scene, &Mock::new(), &base(), 0)[0], "hi");
}

#[test]
fn design_json_round_trips() {
    let full = json!({
        "frame": 0,
        "anchor": {"at": [3, 4], "space": "frame"},
        "content": {"rows": ["text", "..."], "style": {"fg": "red"}},
        "loop": true,
        "blend": "over",
        "z": 0,
        "ttl_ms": null,
        "persist": false,
        "motion": {"vx": 0.0, "vy": 0.0, "ax": 0.0, "ay": 0.0, "bounce": false}
    });
    let s = spec(full.clone());
    assert_eq!(serde_json::to_value(&s).unwrap(), full);

    for anchor in [
        json!({"at": [1, 2], "space": "view"}),
        json!({"text": "flower", "nth": 0, "side": "right", "gap": 1}),
        json!({"text": "f.*r", "regex": true, "nth": 2, "side": "above"}),
        json!({"auto": {"prefer": "top-right"}}),
        json!({"cursor": {"dx": 1, "dy": 0}}),
    ] {
        let a: Anchor = serde_json::from_value(anchor.clone()).unwrap();
        assert_eq!(serde_json::to_value(&a).unwrap(), anchor);
    }
    assert_eq!(
        serde_json::from_value::<Anchor>(json!({"at": [1, 2]})).unwrap(),
        Anchor::At {
            at: [1, 2],
            space: Space::Frame
        }
    );
    assert!(matches!(
        spec(json!({"frame": "active", "effect": {"name": "confetti"}})).frame,
        FrameSel::Name(_)
    ));

    let anim = spec(json!({"frames": [{"rows": ["a"], "ms": 40}, {"rows": ["b"]}], "loop": 3}));
    let frames = anim.frames.unwrap();
    assert_eq!((frames[0].ms, frames[1].ms), (40, 100));
    assert_eq!(anim.looping, LoopSpec::Count(3));

    let cells = spec(
        json!({"content": {"cells": [[{"t": "ab", "fg": 200, "bold": true}, {"t": "c", "bg": "#102030"}]]}}),
    );
    let run = &cells.content.unwrap().cells.unwrap()[0];
    assert_eq!(run[0].style.fg, Some(ColorSpec(Color::Indexed(200))));
    assert!(run[0].style.bold);
    assert_eq!(
        run[1].style.bg,
        Some(ColorSpec(Color::Rgb(0x10, 0x20, 0x30)))
    );

    let patch: SpritePatch =
        serde_json::from_value(json!({"anchor": {"at": [1, 1]}, "z": 4})).unwrap();
    assert_eq!(patch.z, Some(4));
    assert!(patch.content.is_none());
}

#[test]
fn colours_parse_and_print() {
    let parse = |s: &str| s.parse::<ColorSpec>().map(|c| c.0);
    assert_eq!(parse("red"), Ok(Color::Indexed(1)));
    assert_eq!(parse("bright-yellow"), Ok(Color::Indexed(11)));
    assert_eq!(parse("brightyellow"), Ok(Color::Indexed(11)));
    assert_eq!(parse("default"), Ok(Color::Default));
    assert_eq!(parse("#FF8000"), Ok(Color::Rgb(255, 128, 0)));
    assert!(parse("#12").is_err());
    assert!(parse("mauve").is_err());
    assert!(serde_json::from_value::<ColorSpec>(json!(256)).is_err());
    for c in [
        Color::Default,
        Color::Indexed(3),
        Color::Indexed(12),
        Color::Indexed(99),
        Color::Rgb(1, 2, 3),
    ] {
        let v = serde_json::to_value(ColorSpec(c)).unwrap();
        assert_eq!(serde_json::from_value::<ColorSpec>(v).unwrap().0, c);
    }
}

#[test]
fn bad_specs_are_refused() {
    let mut scene = Scene::new();
    let mut add = |v| scene.add(1, spec(v), ms(0)).map_err(|e| e.code());
    assert_eq!(add(json!({})), Err("bad-params"));
    assert_eq!(add(json!({"content": {}})), Err("bad-params"));
    assert_eq!(
        add(json!({"content": {"rows": ["a"], "cells": []}})),
        Err("bad-params")
    );
    assert_eq!(add(json!({"frames": []})), Err("bad-params"));
    assert_eq!(
        add(json!({"content": {"rows": ["a"]}, "frames": [{"rows": ["a"]}]})),
        Err("bad-params")
    );
    assert_eq!(
        add(json!({"content": {"rows": ["a"]}, "effect": {"name": "confetti"}})),
        Err("bad-params")
    );
    assert_eq!(add(json!({"effect": {"name": "nope"}})), Err("bad-params"));
    assert_eq!(
        add(json!({"effect": {"name": "confetti", "params": {"colors": []}}})),
        Err("bad-params")
    );
    assert_eq!(
        add(json!({"frame": "nowhere", "content": {"rows": ["a"]}})),
        Err("no-frame")
    );
    assert_eq!(
        add(json!({"anchor": {"text": "(", "regex": true}, "content": {"rows": ["a"]}})),
        Err("bad-params")
    );
    assert!(scene.is_empty());
    assert_eq!(
        scene
            .update(9, SpritePatch::default(), ms(0))
            .map_err(|e| e.code()),
        Err("no-sprite")
    );
    assert!(!scene.remove(9));
}

#[test]
fn view_anchor_and_negative_coordinates() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    scene.add(1, spec(json!({"frame": PANE, "anchor": {"at": [1, 0], "space": "view"}, "content": {"rows": ["ab"]}})), ms(0)).unwrap();
    scene.add(1, spec(json!({"frame": PANE, "anchor": {"at": [-1, -1], "space": "view"}, "content": {"rows": ["xyz"]}})), ms(0)).unwrap();
    scene.add(1, spec(json!({"frame": PANE, "anchor": {"at": [-3, 1], "space": "view"}, "content": {"rows": ["q"]}})), ms(0)).unwrap();
    let rows = show(&mut scene, &mock, &base(), 0);
    assert_eq!(rows[1], "           ab");
    assert_eq!(rows[4], "                 xyz");
    assert_eq!(rows[2], "                 q");
}

#[test]
fn frame_anchor_scrolls_and_clips() {
    let mut mock = Mock::new();
    let mut scene = Scene::new();
    let id = scene.add(1, spec(json!({"frame": PANE, "anchor": {"at": [8, 2]}, "content": {"rows": ["abc", "def"]}})), ms(0)).unwrap();
    // Clipped on the right edge of the view.
    let rows = show(&mut scene, &mock, &base(), 0);
    assert_eq!(rows[3], "                  ab");
    assert_eq!(rows[4], "                  de");
    assert_eq!(scene.list()[0].screen, Some([18, 3]));
    // The view scrolls down by 2 and right by 3; compose alone follows.
    mock.pane().origin = FramePos { x: 3, y: 2 };
    let rows = scene.compose(&base(), &mock).text();
    assert_eq!(rows[1], "               abc");
    assert_eq!(rows[2], "               def");
    // Scrolled past: only the second row is left, on the top row of the view.
    mock.pane().origin = FramePos { x: 3, y: 3 };
    let rows = show(&mut scene, &mock, &base(), 10);
    assert_eq!(rows[0], "");
    assert_eq!(rows[1], "               def");
    // Out of the view: nothing drawn, still alive.
    mock.pane().origin = FramePos { x: 3, y: 50 };
    assert_eq!(show(&mut scene, &mock, &base(), 20), vec![""; 6]);
    assert_eq!(scene.list()[0].id, id);
}

#[test]
fn unusable_frame_hides() {
    let mut mock = Mock::new();
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"frame": PANE, "content": {"rows": ["ab"]}})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"frame": 99, "content": {"rows": ["zz"]}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0)[1], "          ab");
    scene.take_dirty();
    mock.pane().usable = false;
    assert_eq!(show(&mut scene, &mock, &base(), 10), vec![""; 6]);
    assert!(scene.take_dirty());
    assert_eq!(scene.list()[0].screen, None);
    mock.pane().usable = true;
    assert_eq!(show(&mut scene, &mock, &base(), 20)[1], "          ab");
}

#[test]
fn frame_name_resolves_once() {
    let mut mock = Mock::new();
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"frame": "largest", "content": {"rows": ["ab"]}})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"frame": "active", "content": {"rows": ["cd"]}})),
            ms(0),
        )
        .unwrap();
    assert!(scene.next_wake().is_some());
    let rows = show(&mut scene, &mock, &base(), 0);
    assert_eq!(rows[1], "          cd");
    assert_eq!(scene.list()[0].frame, None);
    assert_eq!(scene.list()[1].frame, Some(PANE));
    mock.infos.push(surface(8, Rect::new(0, 0, 5, 5), (0, 0)));
    assert_eq!(scene.list()[1].frame, Some(PANE));
}

#[test]
fn text_anchor_sides_follow_and_hide() {
    let mut mock = Mock::new();
    mock.lines = vec!["".into(), "a flower".into(), "".into(), "flower".into()];
    let mut scene = Scene::new();
    let put = |scene: &mut Scene, side: &str, rows: serde_json::Value| {
        scene
            .add(1, spec(json!({"frame": PANE, "anchor": {"text": "flower", "side": side}, "content": {"rows": rows}})), ms(0))
            .unwrap()
    };
    let right = put(&mut scene, "right", json!(["*"]));
    let rows = show(&mut scene, &mock, &base(), 0);
    // Match at frame (2, 1), 6 cells: right of it with a gap of 1.
    assert_eq!(rows[2], "                   *");
    scene.remove(right);

    let left = put(&mut scene, "left", json!(["<"]));
    assert_eq!(show(&mut scene, &mock, &base(), 1)[2], "          <");
    scene.remove(left);

    let above = put(&mut scene, "above", json!(["^^"]));
    assert_eq!(show(&mut scene, &mock, &base(), 2)[1], "              ^^");
    scene.remove(above);

    let below = put(&mut scene, "below", json!(["vv"]));
    assert_eq!(show(&mut scene, &mock, &base(), 3)[3], "              vv");
    scene.remove(below);

    let over = put(&mut scene, "over", json!(["FLOWER"]));
    assert_eq!(show(&mut scene, &mock, &base(), 4)[2], "            FLOWER");
    // The view scrolls: the sprite stays on its word.
    mock.pane().origin = FramePos { x: 0, y: 1 };
    assert_eq!(
        scene.compose(&base(), &mock).text()[1],
        "            FLOWER"
    );
    // A line is inserted above: the word moves in the frame, so does the sprite.
    mock.lines.insert(0, "new".into());
    assert_eq!(show(&mut scene, &mock, &base(), 5)[2], "            FLOWER");
    // No match: hidden, not removed.
    mock.lines.clear();
    assert_eq!(show(&mut scene, &mock, &base(), 6), vec![""; 6]);
    assert_eq!(scene.list().len(), 1);
    mock.lines = vec!["flower".into()];
    mock.pane().origin = FramePos { x: 0, y: 0 };
    assert_eq!(show(&mut scene, &mock, &base(), 7)[1], "          FLOWER");
    scene.remove(over);

    // nth picks the second match; a tall sprite is centred on the row.
    mock.lines = vec!["".into(), "a flower".into(), "".into(), "flower".into()];
    scene
        .add(1, spec(json!({"frame": PANE, "anchor": {"text": "flower", "nth": 1, "gap": 0}, "content": {"rows": ["1", "2", "3"]}})), ms(0))
        .unwrap();
    let rows = show(&mut scene, &mock, &base(), 8);
    // Rows 3 and 4; the third is below the view.
    assert_eq!(
        (rows[2].as_str(), rows[3].as_str(), rows[4].as_str()),
        ("", "                1", "                2")
    );
    assert_eq!(rows[5], "");
}

#[test]
fn auto_placement_is_stable() {
    let mut mock = Mock::new();
    mock.spot = Some((7, 0));
    let mut scene = Scene::new();
    scene.add(1, spec(json!({"frame": PANE, "anchor": {"auto": {"prefer": "top-right"}}, "content": {"rows": ["ab"]}})), ms(0)).unwrap();
    assert_eq!(
        show(&mut scene, &mock, &base(), 0)[1],
        "                 ab"
    );
    assert_eq!(scene.next_wake(), Some(ms(500)));
    // A better spot shows up: not before the next check, then not before
    // it has held for the replace delay.
    mock.spot = Some((0, 3));
    for t in [100, 500, 1000, 2000, 2400] {
        assert_eq!(
            show(&mut scene, &mock, &base(), t)[1],
            "                 ab",
            "at {t}"
        );
    }
    assert_eq!(mock.taken_seen.borrow().len(), 4);
    // It flaps back: the delay starts again.
    mock.spot = Some((7, 0));
    show(&mut scene, &mock, &base(), 2900);
    mock.spot = Some((0, 3));
    for t in [3400, 4400, 5300] {
        assert_eq!(
            show(&mut scene, &mock, &base(), t)[1],
            "                 ab",
            "at {t}"
        );
    }
    assert_eq!(show(&mut scene, &mock, &base(), 5900)[4], "          ab");

    // A second auto sprite is told about the first.
    scene
        .add(
            2,
            spec(json!({"frame": PANE, "anchor": {"auto": {}}, "content": {"rows": ["cd"]}})),
            ms(5900),
        )
        .unwrap();
    mock.spot = Some((4, 1));
    show(&mut scene, &mock, &base(), 5950);
    assert_eq!(
        mock.taken_seen.borrow().last().unwrap(),
        &vec![Rect::new(0, 3, 2, 1)]
    );

    // The view shrinks under the spot: a new one is taken at once.
    mock.pane().rect = Rect::new(10, 1, 10, 2);
    mock.spot = Some((1, 1));
    let rows = show(&mut scene, &mock, &base(), 5960);
    assert_eq!(rows[2], "           ab cd");

    // No spot: hidden.
    let mut scene = Scene::new();
    mock.spot = None;
    scene
        .add(
            1,
            spec(json!({"frame": PANE, "anchor": {"auto": {}}, "content": {"rows": ["ab"]}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0), vec![""; 6]);
}

#[test]
fn cursor_anchor_follows() {
    let mut mock = Mock::new();
    mock.cursor = (3, 2);
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"anchor": {"cursor": {"dx": 1, "dy": -1}}, "content": {"rows": ["<>"]}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0)[1], "    <>");
    scene.take_dirty();
    mock.cursor = (10, 5);
    assert_eq!(show(&mut scene, &mock, &base(), 10)[4], "           <>");
    assert!(scene.take_dirty());
    scene.tick(ms(20), &mock);
    assert!(!scene.dirty());
}

fn red() -> Style {
    Style {
        fg: Color::Indexed(1),
        bg: Color::Indexed(4),
        ..Style::default()
    }
}

#[test]
fn blend_modes() {
    let mock = Mock::new();
    let mut grid = Grid::from_lines(20, &["ab  cd", "", "", "", "", ""]);
    for x in 0..6 {
        grid.cell_mut(x, 0).style = red();
    }
    let mut scene = Scene::new();
    let content = json!({"rows": ["######"], "style": {"fg": "green", "bold": true}});
    let over = scene
        .add(1, spec(json!({"content": content})), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &grid, 0)[0], "######");
    scene.remove(over);

    let behind = scene
        .add(
            1,
            spec(json!({"content": content, "blend": "behind"})),
            ms(0),
        )
        .unwrap();
    let out = {
        scene.tick(ms(0), &mock);
        scene.compose(&grid, &mock)
    };
    assert_eq!(out.text()[0], "ab##cd");
    assert_eq!(out.cell(0, 0).style, red());
    assert_eq!(out.cell(2, 0).style.fg, Color::Indexed(2));
    scene.remove(behind);

    scene.add(1, spec(json!({"content": {"rows": ["###"], "style": {"bg": "yellow", "underline": true}}, "blend": "tint"})), ms(0)).unwrap();
    let out = {
        scene.tick(ms(0), &mock);
        scene.compose(&grid, &mock)
    };
    assert_eq!(out.text()[0], "ab  cd");
    let s = out.cell(1, 0).style;
    assert_eq!((s.fg, s.bg), (Color::Indexed(1), Color::Indexed(3)));
    assert!(s.attrs.contains(Attrs::UNDERLINE));
    assert_eq!(out.cell(3, 0).style, red());
}

#[test]
fn colours_inherit_from_below() {
    let mock = Mock::new();
    let mut grid = base();
    grid.cell_mut(0, 0).style = red();
    grid.cell_mut(1, 0).style = red();
    grid.cell_mut(2, 0).style = Style {
        attrs: Attrs(Attrs::INVERSE),
        ..red()
    };
    let mut scene = Scene::new();
    scene
        .add(1, spec(json!({"content": {"cells": [[{"t": "a"}, {"t": "b", "fg": "green", "bg": "default"}, {"t": "c", "fg": "green"}]]}})), ms(0))
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.cell(0, 0).style, red());
    assert_eq!(
        out.cell(1, 0).style,
        Style {
            fg: Color::Indexed(2),
            ..Style::default()
        }
    );
    // Over an inverse cell the visible background (its fg) is kept.
    let c = out.cell(2, 0).style;
    assert!(c.attrs.contains(Attrs::INVERSE));
    assert_eq!((c.fg, c.bg), (Color::Indexed(1), Color::Indexed(2)));
    // Links and underline colour of the cell below do not leak.
    assert_eq!(c.link, 0);
}

#[test]
fn spaces_are_holes_unless_opaque() {
    let mock = Mock::new();
    let grid = Grid::from_lines(20, &["abcdef", "abcdef", "abcdef", "", "", ""]);
    let mut scene = Scene::new();
    scene
        .add(1, spec(json!({"content": {"rows": ["X X"]}})), ms(0))
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [0, 1]}, "content": {"rows": ["X X"], "opaque": true}})),
            ms(0),
        )
        .unwrap();
    scene.add(1, spec(json!({"anchor": {"at": [0, 2]}, "content": {"cells": [[{"t": "X "}, {"t": " X", "bg": "red"}]]}})), ms(0)).unwrap();
    let rows = show(&mut scene, &mock, &grid, 0);
    assert_eq!(rows[0], "XbXdef");
    assert_eq!(rows[1], "X Xdef");
    assert_eq!(rows[2], "Xb Xef");
}

#[test]
fn wide_glyphs() {
    let mock = Mock::new();
    let mut grid = base();
    grid.put(0, 0, Cell::wide("世", Style::default()));
    grid.put(2, 0, Cell::wide("界", Style::default()));
    let mut scene = Scene::new();
    // A narrow sprite cell on the spacer of the first, the head of the second.
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [1, 0]}, "content": {"rows": ["xy"]}})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.row_text(0).trim_end(), " xy");
    assert_eq!(out.cell(0, 0).width, 1);
    assert_eq!(out.cell(3, 0).width, 1);

    // A wide sprite glyph takes two cells; cut by the view edge it is dropped.
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"frame": PANE, "anchor": {"at": [7, 0]}, "content": {"rows": ["a世界"]}})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&base(), &mock);
    assert_eq!(out.row_text(1).trim_end(), "                 a世");
    assert_eq!((out.cell(18, 1).width, out.cell(19, 1).width), (2, 0));
    scene
        .update(
            1,
            serde_json::from_value(json!({"anchor": {"at": [8, 0]}})).unwrap(),
            ms(1),
        )
        .unwrap();
    scene.tick(ms(1), &mock);
    let out = scene.compose(&base(), &mock);
    assert_eq!(out.row_text(1).trim_end(), "                  a");
    // Cut by the left edge too.
    scene
        .update(
            1,
            serde_json::from_value(json!({"anchor": {"at": [-2, 0]}})).unwrap(),
            ms(2),
        )
        .unwrap();
    scene.tick(ms(2), &mock);
    let out = scene.compose(&base(), &mock);
    assert_eq!(out.row_text(1).trim_end(), "           界");

    // A combining mark joins its base.
    let mut scene = Scene::new();
    scene
        .add(1, spec(json!({"content": {"rows": ["e\u{301}x"]}})), ms(0))
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&base(), &mock);
    assert_eq!(out.cell(0, 0).text.as_str(), "e\u{301}");
    assert_eq!(out.cell(1, 0).text.as_str(), "x");
}

#[test]
fn z_then_insertion_order() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    let a = scene
        .add(1, spec(json!({"content": {"rows": ["a"]}, "z": 1})), ms(0))
        .unwrap();
    scene
        .add(1, spec(json!({"content": {"rows": ["b"]}})), ms(0))
        .unwrap();
    let c = scene
        .add(1, spec(json!({"content": {"rows": ["c"]}})), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0)[0], "a");
    scene
        .update(a, serde_json::from_value(json!({"z": -1})).unwrap(), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0)[0], "c");
    scene.remove(c);
    assert_eq!(show(&mut scene, &mock, &base(), 0)[0], "b");
}

#[test]
fn animation_timing_and_loops() {
    let mock = Mock::new();
    let frames = json!([{"rows": ["1"], "ms": 100}, {"rows": ["2"], "ms": 50}]);
    let mut scene = Scene::new();
    // Once.
    scene
        .add(1, spec(json!({"frames": frames})), ms(1000))
        .unwrap();
    assert_eq!(scene.next_wake(), Some(ms(1100)));
    assert_eq!(show(&mut scene, &mock, &base(), 1099)[0], "1");
    assert_eq!(show(&mut scene, &mock, &base(), 1100)[0], "2");
    assert_eq!(scene.next_wake(), Some(ms(1150)));
    assert_eq!(show(&mut scene, &mock, &base(), 1149)[0], "2");
    assert_eq!(show(&mut scene, &mock, &base(), 1150)[0], "");
    assert!(scene.is_empty());
    assert_eq!(scene.next_wake(), None);

    // Twice, with a late tick that skips frames.
    scene
        .add(1, spec(json!({"frames": frames, "loop": 2})), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 160)[0], "1");
    assert_eq!(show(&mut scene, &mock, &base(), 260)[0], "2");
    assert_eq!(show(&mut scene, &mock, &base(), 299)[0], "2");
    scene.take_dirty();
    scene.tick(ms(300), &mock);
    assert!(scene.is_empty() && scene.take_dirty());

    // Forever.
    scene
        .add(1, spec(json!({"frames": frames, "loop": true})), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 150 * 1000 + 120)[0], "2");
    assert!(!scene.is_empty());

    // A static content never wakes.
    let mut scene = Scene::new();
    scene
        .add(1, spec(json!({"content": {"rows": ["s"]}})), ms(0))
        .unwrap();
    scene.tick(ms(5), &mock);
    assert_eq!(scene.next_wake(), None);
    scene.take_dirty();
    scene.tick(ms(500), &mock);
    assert!(!scene.dirty());
}

#[test]
fn motion_and_bounce() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["o"]}, "motion": {"vx": 10.0}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(scene.next_wake(), Some(ms(33)));
    assert_eq!(show(&mut scene, &mock, &base(), 500)[0], "     o");
    assert_eq!(scene.next_wake(), Some(ms(533)));
    // Without bounce it leaves the view.
    assert_eq!(show(&mut scene, &mock, &base(), 5000)[0], "");

    let mut scene = Scene::new();
    scene
        .add(1, spec(json!({"frame": PANE, "content": {"rows": ["ab"]}, "motion": {"vx": 10.0, "vy": 2.0, "bounce": true}})), ms(0))
        .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for t in (0..=4000).step_by(50) {
        scene.tick(ms(t), &mock);
        let [x, y] = scene.list()[0].screen.unwrap();
        assert!(
            (10..=18).contains(&x) && (1..=4).contains(&y),
            "left the view at {t}: {x},{y}"
        );
        seen.insert(x);
    }
    assert!(seen.contains(&10) && seen.contains(&18));

    // Gravity.
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["o"]}, "motion": {"ay": 8.0}})),
            ms(0),
        )
        .unwrap();
    for t in (0..=1000).step_by(10) {
        scene.tick(ms(t), &mock);
    }
    assert_eq!(scene.list()[0].screen, Some([0, 4]));
}

#[test]
fn ttl_and_patch() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    let id = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["a"]}, "ttl_ms": 100})),
            ms(0),
        )
        .unwrap();
    assert_eq!(scene.next_wake(), Some(ms(100)));
    scene
        .update(
            id,
            serde_json::from_value(
                json!({"content": {"rows": ["bb"]}, "ttl_ms": 300, "blend": "behind"}),
            )
            .unwrap(),
            ms(50),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 349)[0], "bb");
    assert_eq!(scene.list()[0].blend, Blend::Behind);
    assert_eq!(scene.list()[0].size, [2, 1]);
    // A failed update changes nothing.
    assert!(
        scene
            .update(
                id,
                serde_json::from_value(json!({"z": 5, "frames": []})).unwrap(),
                ms(60)
            )
            .is_err()
    );
    assert_eq!(scene.list()[0].z, 0);
    assert_eq!(show(&mut scene, &mock, &base(), 350)[0], "");
    assert!(scene.is_empty());
}

#[test]
fn owners_and_persist() {
    let mut scene = Scene::new();
    let add = |scene: &mut Scene, owner, persist| {
        scene
            .add(
                owner,
                spec(json!({"content": {"rows": ["a"]}, "persist": persist})),
                ms(0),
            )
            .unwrap()
    };
    let a = add(&mut scene, 1, false);
    let b = add(&mut scene, 1, true);
    let c = add(&mut scene, 2, false);
    scene.take_dirty();
    scene.drop_owner(1);
    assert!(scene.take_dirty());
    assert_eq!(
        scene.list().iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![b, c]
    );
    assert!(a < b);
    assert_eq!(scene.clear(Some(1)), 1);
    assert_eq!(scene.clear(None), 1);
    assert_eq!(scene.clear(None), 0);
    assert!(
        scene.is_empty()
            && !{
                scene.take_dirty();
                scene.dirty()
            }
    );
}

struct Blink(u32);

impl Effect for Blink {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool {
        self.0 += 1;
        canvas.put(
            canvas.width() as i32 - 1,
            0,
            if self.0 % 2 == 1 { "#" } else { "." },
            &StyleSpec::default(),
        );
        t < Duration::from_millis(100)
    }

    fn period(&self) -> Duration {
        Duration::from_millis(30)
    }
}

#[test]
fn effect_lifecycle() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    scene.register_effect("blink", Box::new(|_| Ok(Box::new(Blink(0)))));
    assert_eq!(scene.effects(), vec!["blink", "confetti"]);
    scene
        .add(
            1,
            spec(json!({"frame": PANE, "effect": {"name": "blink"}})),
            ms(1000),
        )
        .unwrap();
    assert_eq!(scene.next_wake(), Some(ms(1000)));
    // The canvas is the view: last column of the pane.
    assert_eq!(
        show(&mut scene, &mock, &base(), 1000)[1],
        "                   #"
    );
    assert_eq!(scene.list()[0].kind, "effect:blink");
    assert_eq!(scene.list()[0].size, [10, 4]);
    // Not rendered again before its period.
    assert_eq!(
        show(&mut scene, &mock, &base(), 1029)[1],
        "                   #"
    );
    assert_eq!(
        show(&mut scene, &mock, &base(), 1030)[1],
        "                   ."
    );
    assert_eq!(scene.next_wake(), Some(ms(1060)));
    show(&mut scene, &mock, &base(), 1060);
    show(&mut scene, &mock, &base(), 1090);
    assert_eq!(show(&mut scene, &mock, &base(), 1120), vec![""; 6]);
    assert!(scene.is_empty());

    // A fixed canvas.
    scene
        .add(
            1,
            spec(json!({"effect": {"name": "blink"}, "size": [3, 1], "anchor": {"at": [1, 1]}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 0)[1], "   #");
}

#[test]
fn confetti_is_deterministic_and_ends() {
    let mock = Mock::new();
    let run = |seed: u64| {
        let mut scene = Scene::new();
        scene
            .add(1, spec(json!({"effect": {"name": "confetti", "params": {"seed": seed, "count": 30, "duration_ms": 800}}})), ms(0))
            .unwrap();
        let mut shots = Vec::new();
        let mut t = 0;
        while !scene.is_empty() {
            shots.push(show(&mut scene, &mock, &base(), t));
            t += 40;
            assert!(t < 5000);
        }
        (shots, t)
    };
    let (a, end) = run(7);
    assert_eq!(end, 840);
    assert_eq!(a, run(7).0);
    assert_ne!(a, run(8).0);
    // Something is drawn, and it moves.
    let count = |rows: &Vec<String>| {
        rows.iter()
            .map(|r| r.chars().filter(|c| *c != ' ').count())
            .sum::<usize>()
    };
    assert!(count(&a[4]) > 5, "{:?}", a[4]);
    assert_ne!(a[4], a[8]);
    // Default params.
    assert!(confetti(&serde_json::Value::Null).is_ok());
    assert!(confetti(&json!({"count": "many"})).is_err());
}
