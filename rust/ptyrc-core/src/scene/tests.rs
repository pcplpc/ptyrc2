// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::frames::HeatClass;
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
    heat: Option<HeatView>,
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
            heat: None,
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

    fn heat(&self, id: FrameId, _now: Duration) -> Option<HeatView> {
        self.heat.clone().filter(|_| id == PANE)
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
    scene.tick_on(ms(now), base, mock);
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
    assert_eq!(s.blend, Blend::Normal);
    assert_eq!(s.layer, None);
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
        .add(1, spec(json!({"frame": PANE, "anchor": {"text": "flower", "nth": 1, "gap": 0, "slide": false}, "content": {"rows": ["1", "2", "3"]}})), ms(0))
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
        .add(
            1,
            spec(json!({"content": content, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &grid, 0)[0], "######");
    scene.remove(over);

    // The default: behind the app's text.
    let behind = scene
        .add(1, spec(json!({"content": content})), ms(0))
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
        .add(1, spec(json!({"content": {"cells": [[{"t": "a"}, {"t": "b", "fg": "green", "bg": "default"}, {"t": "c", "fg": "green"}]]}, "layer": "front"})), ms(0))
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
        .add(
            1,
            spec(json!({"content": {"rows": ["X X"]}, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [0, 1]}, "content": {"rows": ["X X"], "opaque": true}, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    scene.add(1, spec(json!({"anchor": {"at": [0, 2]}, "content": {"cells": [[{"t": "X "}, {"t": " X", "bg": "red"}]]}, "layer": "front"})), ms(0)).unwrap();
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
            spec(json!({"anchor": {"at": [1, 0]}, "content": {"rows": ["xy"]}, "layer": "front"})),
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
    scene.tick(ms(1000), &mock);
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
    scene.tick(ms(0), &mock);
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
    scene.tick(ms(0), &mock);
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
    assert_eq!(scene.list()[0].blend, Blend::Normal);
    assert_eq!(scene.list()[0].layer, -1);
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
    assert_eq!(scene.effects(), vec!["blink", "confetti", "sparkle"]);
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

#[test]
fn shift_moves_frame_rows() {
    let mut mock = Mock::new();
    mock.lines = vec!["".into(), "".into(), "a word".into()];
    let mut scene = Scene::new();
    let add = |scene: &mut Scene, v| scene.add(1, spec(v), ms(0)).unwrap();
    let fixed = add(
        &mut scene,
        json!({"frame": PANE, "anchor": {"at": [0, 2], "space": "frame"}, "content": {"rows": ["F"]}}),
    );
    let above = add(
        &mut scene,
        json!({"frame": PANE, "anchor": {"at": [1, 0], "space": "frame"}, "content": {"rows": ["A"]}}),
    );
    let hud = add(
        &mut scene,
        json!({"frame": PANE, "anchor": {"at": [0, 2], "space": "view"}, "content": {"rows": ["H"]}}),
    );
    let text = add(
        &mut scene,
        json!({"frame": PANE, "anchor": {"text": "word", "side": "over"}, "content": {"rows": ["T"]}}),
    );
    scene.tick(ms(0), &mock);
    scene.take_dirty();
    let at = |scene: &Scene, id| scene.list().iter().find(|s| s.id == id).unwrap().screen;
    assert_eq!(at(&scene, fixed), Some([10, 3]));
    assert_eq!(at(&scene, text), Some([12, 3]));

    // A line inserted above row 2: rows 2.. go down by one.
    scene.shift(PANE, 2, i64::MAX, 1);
    assert!(scene.dirty());
    mock.lines.insert(1, "".into());
    scene.tick(ms(10), &mock);
    assert_eq!(at(&scene, fixed), Some([10, 4]));
    assert_eq!(at(&scene, above), Some([11, 1]));
    assert_eq!(at(&scene, hud), Some([10, 3]));
    assert_eq!(at(&scene, text), Some([12, 4]));

    // Another frame's shift, or rows outside, change nothing.
    scene.take_dirty();
    scene.shift(RAW, 0, 100, 5);
    scene.shift(PANE, 50, 60, 5);
    assert!(!scene.dirty());
}

#[test]
fn sprite_text_is_storable() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    // A tab and an escape are drawn as (transparent) spaces; marks beyond
    // what a cell stores are dropped.
    let marks = "\u{301}".repeat(40);
    scene
        .add(
            1,
            spec(json!({"content": {"rows": [format!("a\tb\u{1b}c e{marks}")]}, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    let mut under = base();
    for col in 0..8 {
        under.put(col, 0, Cell::narrow("-", Style::default()));
    }
    scene.tick(ms(0), &mock);
    let out = scene.compose(&under, &mock);
    let row = out.row(0);
    assert_eq!(row[0].text, "a");
    assert_eq!(row[1].text, "-");
    assert_eq!(row[2].text, "b");
    assert_eq!(row[3].text, "-");
    assert_eq!(row[4].text, "c");
    assert!(row[6].text.starts_with('e'));
    assert!(row[6].text.len() <= 32 && row[6].text.len() > 1);
    for cell in row {
        assert!(!cell.text.chars().any(char::is_control));
    }
}

/// Draws the first letter under it at the cursor's cell; notes what it
/// was shown.
#[derive(Default)]
struct Echo {
    letter: String,
    cursor: Option<(u16, u16)>,
    looks: Option<Looks>,
    seen: std::sync::Arc<std::sync::Mutex<Seen>>,
}

#[derive(Default, Debug, PartialEq)]
struct Seen {
    looks: u32,
    inked: Vec<(u16, u16)>,
    origin: (i64, i64),
    size: (u16, u16),
    hot: Vec<(u16, u16)>,
    params: serde_json::Value,
}

impl Effect for Echo {
    fn render(&mut self, _t: Duration, canvas: &mut Canvas) -> bool {
        if let Some((x, y)) = self.cursor {
            canvas.put(x as i32, y as i32, &self.letter, &StyleSpec::default());
        }
        true
    }

    fn looks(&self) -> Looks {
        self.looks.unwrap_or(Looks::Cells)
    }

    fn look(&mut self, under: &Under) {
        let (w, h) = under.size;
        let cells = || (0..h).flat_map(|y| (0..w).map(move |x| (x, y)));
        let mut seen = self.seen.lock().unwrap();
        seen.looks += 1;
        seen.inked = cells().filter(|(x, y)| under.ink(*x, *y)).collect();
        seen.hot = cells()
            .filter(|(x, y)| {
                under
                    .heat(*x, *y)
                    .is_some_and(|w| w.class == HeatClass::Hot)
            })
            .collect();
        seen.origin = under.origin;
        seen.size = under.size;
        self.letter = seen
            .inked
            .first()
            .and_then(|(x, y)| under.cell(*x, *y))
            .map(|c| c.text.to_string())
            .unwrap_or_default();
        self.cursor = under.cursor;
    }

    fn tune(&mut self, params: &serde_json::Value) -> Result<(), String> {
        if params.get("bad").is_some() {
            return Err("echo: bad".into());
        }
        self.seen.lock().unwrap().params = params.clone();
        Ok(())
    }
}

/// A scene with the `echo` effect; what its last instance was shown.
fn echo_scene(looks: Looks) -> (Scene, std::sync::Arc<std::sync::Mutex<Seen>>) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Seen::default()));
    let mut scene = Scene::new();
    let shared = seen.clone();
    scene.register_effect(
        "echo",
        Box::new(move |_| {
            *shared.lock().unwrap() = Seen::default();
            Ok(Box::new(Echo {
                looks: Some(looks),
                seen: shared.clone(),
                ..Echo::default()
            }))
        }),
    );
    (scene, seen)
}

fn write(grid: &mut Grid, x: u16, y: u16, text: &str) {
    for (i, ch) in text.chars().enumerate() {
        let cell = Cell::narrow(&ch.to_string(), Style::default());
        grid.put(x + i as u16, y, cell);
    }
}

#[test]
fn an_effect_sees_the_text_and_the_cursor_under_it() {
    let mut mock = Mock::new();
    let mut grid = base();
    write(&mut grid, 12, 2, "word");
    let (mut scene, _) = echo_scene(Looks::Cells);
    let fx = json!({"frame": PANE, "layer": "front", "effect": {"name": "echo"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    // The cursor is outside the pane: nothing to draw at.
    assert_eq!(show(&mut scene, &mock, &grid, 0)[3], "");
    // Screen (13, 3) is the canvas cell (3, 2).
    mock.cursor = (13, 3);
    assert_eq!(show(&mut scene, &mock, &grid, 100)[3], "             w");
    write(&mut grid, 12, 2, "other");
    assert_eq!(show(&mut scene, &mock, &grid, 200)[3], "             o");
    // Not usable: the effect keeps what it saw.
    mock.pane().usable = false;
    show(&mut scene, &mock, &base(), 300);
    mock.pane().usable = true;
    assert_eq!(show(&mut scene, &mock, &grid, 301)[3], "             o");
}

#[test]
fn an_effect_sees_cells_not_strings() {
    // A tab and a wide glyph: columns are those of the screen.
    let mut vt = ptyrc_vt::Vt::new(20, 6);
    vt.feed("\x1b[3;11Ha\t世b\x1b[4;12H\x1b[4mu \x1b[0;8mh".as_bytes());
    let mock = Mock::new();
    let (mut scene, seen) = echo_scene(Looks::Cells);
    let fx = json!({"frame": PANE, "effect": {"name": "echo"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), vt.grid(), &mock);
    // Canvas cells of the pane (screen 10.., 1..): `a` at (0, 1); the
    // tab stop 16 is canvas 6: both halves of the wide glyph, then `b`;
    // an underlined blank is ink, hidden text is not.
    assert_eq!(
        seen.lock().unwrap().inked,
        [(0, 1), (6, 1), (7, 1), (8, 1), (1, 2), (2, 2)]
    );
    assert_eq!(seen.lock().unwrap().size, (10, 4));
}

#[test]
fn an_effect_with_a_size_sees_what_is_under_its_canvas() {
    let mut mock = Mock::new();
    mock.pane().origin = FramePos { x: 0, y: 40 };
    let mut grid = base();
    write(&mut grid, 0, 2, "0123456789abcdefghij");
    write(&mut grid, 14, 4, "z");
    let (mut scene, seen) = echo_scene(Looks::Cells);
    // A 4x3 canvas at the frame cell (3, 41): screen (13, 2).
    let fx = json!({"frame": PANE, "size": [4, 3], "anchor": {"at": [3, 41]},
        "effect": {"name": "echo"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), &grid, &mock);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.inked, [(0, 0), (1, 0), (2, 0), (3, 0), (1, 2)]);
        assert_eq!((seen.origin, seen.size), ((3, 41), (4, 3)));
    }
    // Scrolled by one row: its first row is out of the view, on the
    // chrome above the pane, and shows nothing.
    mock.pane().origin = FramePos { x: 0, y: 42 };
    write(&mut grid, 10, 0, "chrome");
    scene.tick_on(ms(60), &grid, &mock);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.inked, [(0, 2), (1, 2), (2, 2), (3, 2)]);
    assert_eq!(seen.origin, (3, 41));
}

#[test]
fn a_view_that_changes_under_a_still_sprite_is_dirty() {
    let mut mock = Mock::new();
    let mut scene = Scene::new();
    let put = json!({
        "frame": PANE,
        "anchor": {"at": [0, 0], "space": "view"},
        "content": {"rows": ["abcdef"]},
    });
    scene.add(1, spec(put), ms(0)).unwrap();
    let shown = show(&mut scene, &mock, &base(), 0);
    assert!(scene.take_dirty());
    assert_eq!(show(&mut scene, &mock, &base(), 10), shown);
    assert!(!scene.take_dirty());
    // Same top-left cell, a narrower view: clipped otherwise.
    mock.pane().rect.w = 3;
    assert_ne!(show(&mut scene, &mock, &base(), 20), shown);
    assert!(scene.take_dirty());
}

#[test]
fn a_scroll_is_shown_to_an_effect_at_once() {
    let mut mock = Mock::new();
    let (mut scene, seen) = echo_scene(Looks::Cells);
    let fx = json!({"frame": PANE, "effect": {"name": "echo"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), &base(), &mock);
    // Not due: no look, no render.
    scene.take_dirty();
    scene.tick_on(ms(10), &base(), &mock);
    assert_eq!(seen.lock().unwrap().looks, 1);
    assert!(!scene.take_dirty());
    // The app scrolls: looked at and drawn in the same tick; the pace
    // of the effect is kept.
    mock.pane().origin = FramePos { x: 0, y: 3 };
    scene.tick_on(ms(20), &base(), &mock);
    assert_eq!(seen.lock().unwrap().looks, 2);
    assert_eq!(seen.lock().unwrap().origin, (0, 3));
    // It drew what it had drawn: nothing to repaint.
    assert!(!scene.take_dirty());
    assert_eq!(scene.next_wake(), Some(ms(50)));
    // Without the screen an effect is shown nothing.
    scene.tick(ms(50), &mock);
    assert_eq!(seen.lock().unwrap().looks, 2);
}

fn heat_of(cols: u16, rows: u16, hot: &[(u16, u16)]) -> HeatView {
    let n = cols as usize * rows as usize;
    let mut heat = HeatView {
        cols,
        rows,
        safe: vec![1.0; n],
        activity: vec![0.0; n],
        occupancy: vec![0.0; n],
        still: vec![30.0; n],
        class: vec![HeatClass::Free; n],
    };
    for (x, y) in hot {
        heat.class[*y as usize * cols as usize + *x as usize] = HeatClass::Hot;
    }
    heat
}

#[test]
fn an_effect_is_shown_the_heat_when_it_asks() {
    let mut mock = Mock::new();
    mock.heat = Some(heat_of(10, 4, &[(2, 1), (9, 3)]));
    // Not asked: not given.
    let (mut scene, seen) = echo_scene(Looks::Cells);
    let fx = json!({"frame": PANE, "effect": {"name": "echo"}});
    scene.add(1, spec(fx.clone()), ms(0)).unwrap();
    scene.tick_on(ms(0), &base(), &mock);
    assert_eq!(seen.lock().unwrap().hot, []);
    // Asked: the cells of the view, in canvas cells.
    let (mut scene, seen) = echo_scene(Looks::Heat);
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), &base(), &mock);
    assert_eq!(seen.lock().unwrap().hot, [(2, 1), (9, 3)]);
    // Read again after 200 ms, not before.
    mock.heat = Some(heat_of(10, 4, &[]));
    scene.tick_on(ms(100), &base(), &mock);
    assert_eq!(seen.lock().unwrap().hot, [(2, 1), (9, 3)]);
    scene.tick_on(ms(200), &base(), &mock);
    assert_eq!(seen.lock().unwrap().hot, []);
    // A canvas smaller than the view: its own cells.
    mock.heat = Some(heat_of(10, 4, &[(2, 1), (9, 3)]));
    let (mut scene, seen) = echo_scene(Looks::Heat);
    let fx = json!({"frame": PANE, "size": [3, 2], "anchor": {"at": [1, 1]},
        "effect": {"name": "echo"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), &base(), &mock);
    assert_eq!(seen.lock().unwrap().hot, [(1, 0)]);
}

#[test]
fn effects_leave_nothing_behind() {
    let mock = Mock::new();
    for (name, params) in [
        ("confetti", json!({"duration_ms": 600})),
        ("sparkle", json!({"duration_ms": 600, "count": 8})),
    ] {
        let mut scene = Scene::new();
        scene
            .add(
                1,
                spec(json!({"effect": {"name": name, "params": params}})),
                ms(0),
            )
            .unwrap();
        let mut seen = 0;
        let mut t = 0;
        while !scene.is_empty() {
            let rows = show(&mut scene, &mock, &base(), t);
            seen += rows.iter().filter(|r| !r.is_empty()).count();
            // The scene asks to be woken later, at animation rate.
            if !scene.is_empty() {
                let wake = scene.next_wake().expect(name);
                assert!(wake > ms(t) && wake <= ms(t + 250), "{name}: {wake:?}");
            }
            t += 33;
            assert!(t < 3000, "{name} never ends");
        }
        assert!(seen > 0, "{name} drew nothing");
        assert!(scene.next_wake().is_none());
        assert_eq!(scene.compose(&base(), &mock), base(), "{name} left cells");
    }
}

#[test]
fn sparkle_twinkles_in_place() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    let p = json!({"count": 3, "life_ms": 700, "seed": 5});
    scene
        .add(
            1,
            spec(json!({"effect": {"name": "sparkle", "params": p}, "blend": "behind"})),
            ms(0),
        )
        .unwrap();
    // `behind`: text keeps its cells.
    let mut under = base();
    for row in 0..6 {
        for col in 0..20 {
            if (col + row) % 2 == 0 {
                under.put(col, row, Cell::narrow("#", Style::default()));
            }
        }
    }
    let glyphs = |rows: &[String]| -> Vec<char> {
        rows.iter()
            .flat_map(|r| r.chars())
            .filter(|c| !matches!(c, ' ' | '#'))
            .collect()
    };
    let mut kinds = std::collections::BTreeSet::new();
    for t in (0..4000).step_by(50) {
        scene.tick(ms(t), &mock);
        let out = scene.compose(&under, &mock);
        for row in 0..6 {
            for col in 0..20 {
                if (col + row) % 2 == 0 {
                    assert_eq!(out.cell(col, row).text, "#");
                }
            }
        }
        let g = glyphs(&out.text());
        assert!(g.len() <= 3);
        kinds.extend(g);
    }
    // It runs until removed, and goes through its shapes.
    assert!(!scene.is_empty());
    assert!(kinds.contains(&'✦') && kinds.contains(&'·'), "{kinds:?}");
    assert!(sparkle(&json!({"colors": []})).is_err());
}

fn styled(fg: u8, bg: Color) -> Style {
    Style {
        fg: Color::Indexed(fg),
        bg,
        ..Style::default()
    }
}

fn below(rng: &mut Rng, n: u64) -> u64 {
    rng.next_u64() % n
}

/// Tick and compose.
fn render(scene: &mut Scene, mock: &Mock, grid: &Grid, now: u64) -> Grid {
    scene.tick(ms(now), mock);
    scene.compose(grid, mock)
}

#[test]
fn layer_names_and_defaults() {
    let layer = |v: serde_json::Value| spec(json!({"content": {"rows": ["x"]}, "layer": v})).layer;
    assert_eq!(layer(json!("back")), Some(Layer(-1)));
    assert_eq!(layer(json!("front")), Some(Layer(1)));
    assert_eq!(layer(json!(-7)), Some(Layer(-7)));
    for bad in [json!(0), json!("app"), json!(1.5)] {
        let v = json!({"content": {"rows": ["x"]}, "layer": bad});
        assert!(serde_json::from_value::<SpriteSpec>(v).is_err());
    }
    assert_eq!(
        serde_json::to_value(Layer(3)).unwrap(),
        serde_json::json!(3)
    );

    // Without a layer: behind; a tint in front. The draft names of
    // `blend` still say where.
    let mut scene = Scene::new();
    for (blend, layer) in [
        (json!(null), -1),
        (json!("normal"), -1),
        (json!("tint"), 1),
        (json!("over"), 1),
        (json!("behind"), -1),
    ] {
        let mut v = json!({"content": {"rows": ["x"]}});
        if !blend.is_null() {
            v["blend"] = blend.clone();
        }
        let id = scene.add(1, spec(v), ms(0)).unwrap();
        let info = scene.list().into_iter().find(|s| s.id == id).unwrap();
        assert_eq!(info.layer, layer, "{blend}");
        assert!(matches!(info.blend, Blend::Normal | Blend::Tint));
    }
    // A layer given wins over the draft name.
    let id = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["x"]}, "blend": "over", "layer": -3})),
            ms(0),
        )
        .unwrap();
    assert_eq!(scene.list().iter().find(|s| s.id == id).unwrap().layer, -3);
}

#[test]
fn back_layer_shows_through_cells_without_ink() {
    let mock = Mock::new();
    let panel = Color::Indexed(8);
    let mut grid = Grid::from_lines(20, &["ab  cd      ", "", "", "", "", ""]);
    for x in 0..6 {
        grid.cell_mut(x, 0).style = styled(1, Color::Default);
    }
    // A grey panel: blank cells of another backdrop, one of them with text.
    for x in 6..10 {
        grid.cell_mut(x, 0).style = styled(1, panel);
    }
    grid.put(9, 0, Cell::narrow("p", styled(1, panel)));
    // Inverse video: a backdrop of the app's too.
    grid.cell_mut(10, 0).style = Style {
        attrs: Attrs(Attrs::INVERSE),
        ..styled(5, Color::Default)
    };

    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["############"], "style": {"fg": "green"}}})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.text()[0], "ab##cd###p##");
    // Text of the app: untouched.
    assert_eq!(out.cell(0, 0), grid.cell(0, 0));
    assert_eq!(out.cell(9, 0), grid.cell(9, 0));
    // The glyph takes the backdrop it is seen on.
    assert_eq!(out.cell(2, 0).style, styled(2, Color::Default));
    assert_eq!(out.cell(6, 0).style, styled(2, panel));
    let inv = out.cell(10, 0).style;
    assert!(inv.attrs.contains(Attrs::INVERSE));
    assert_eq!((inv.fg, inv.bg), (Color::Indexed(5), Color::Indexed(2)));
    assert_eq!(out.cell(11, 0).style, styled(2, Color::Default));
}

#[test]
fn back_colour_is_a_sandwich() {
    let mock = Mock::new();
    let panel = Color::Indexed(8);
    let mut grid = Grid::from_lines(20, &["ab  cd  ", "", "", "", "", ""]);
    for x in 0..4 {
        grid.cell_mut(x, 0).style = styled(1, Color::Default);
    }
    for x in 4..8 {
        grid.cell_mut(x, 0).style = styled(1, panel);
    }
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["~~~~~~~~"], "style": {"fg": "cyan", "bg": "blue"}}})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.text()[0], "ab~~cd~~");
    // The app's glyph keeps its foreground and stands on the sprite's
    // background.
    assert_eq!(out.cell(0, 0).style, styled(1, Color::Indexed(4)));
    assert_eq!(out.cell(2, 0).style, styled(6, Color::Indexed(4)));
    // The panel's colour is the app's: it wins, under text and under the
    // sprite's glyph.
    assert_eq!(out.cell(4, 0).style, styled(1, panel));
    assert_eq!(out.cell(6, 0).style, styled(6, panel));

    // A tint behind the app is a colour of background and nothing else.
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["####"], "style": {"fg": "green", "bg": "yellow", "bold": true}}, "blend": "tint", "layer": "back"})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.text()[0], "ab  cd");
    assert_eq!(out.cell(0, 0).style, styled(1, Color::Indexed(3)));
    assert_eq!(out.cell(2, 0).style, styled(1, Color::Indexed(3)));
}

#[test]
fn back_layers_stack_and_wide_glyphs_need_room() {
    let mock = Mock::new();
    let grid = Grid::from_lines(20, &["    x     ", "", "", "", "", ""]);
    let mut scene = Scene::new();
    // Far back: a field of colour. Nearer: glyphs without a colour of
    // their own, seen on the field.
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["          "], "style": {"bg": "blue"}, "opaque": true}, "layer": -2})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["a 世界b"], "style": {"fg": "red"}}})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    // 界 would cover the app's `x`: it is not drawn, by halves neither.
    assert_eq!(out.row_text(0).trim_end(), "a 世x b");
    assert_eq!((out.cell(2, 0).width, out.cell(3, 0).width), (2, 0));
    for x in 0..10 {
        assert_eq!(out.cell(x, 0).style.bg, Color::Indexed(4), "col {x}");
    }
    assert_eq!(out.cell(0, 0).style.fg, Color::Indexed(1));
    assert_eq!(out.cell(1, 0).text, " ");

    // A nearer wide glyph over the half of a farther one cuts it whole.
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["世界"]}, "layer": -2})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [1, 0]}, "content": {"rows": ["z"]}})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &base(), 0);
    assert_eq!(out.row_text(0).trim_end(), " z界");
}

#[test]
fn four_layers() {
    let mock = Mock::new();
    let grid = Grid::from_lines(20, &["", "  the app's text", "", "  more text", "", ""]);
    let mut scene = Scene::new();
    // 1. A static background.
    scene
        .add(
            1,
            spec(json!({
                "content": {"rows": ["....................", "....................",
                                     "....................", "...................."],
                            "style": {"fg": "blue"}},
                "layer": -2,
            })),
            ms(0),
        )
        .unwrap();
    // 2. An animation in front of it.
    scene
        .add(
            1,
            spec(json!({
                "anchor": {"at": [0, 1]},
                "frames": [{"rows": ["<>"], "ms": 100}, {"rows": ["><"], "ms": 100}],
                "loop": true,
                "motion": {"vx": 10.0},
                "layer": "back",
            })),
            ms(0),
        )
        .unwrap();
    // 3. The app. 4. A head-up display in front of all.
    scene
        .add(
            1,
            spec(json!({
                "anchor": {"at": [-1, 1], "space": "view"},
                "content": {"rows": ["[12:00]"], "opaque": true},
                "layer": "front",
            })),
            ms(0),
        )
        .unwrap();
    let rows = show(&mut scene, &mock, &grid, 0);
    assert_eq!(rows[0], "....................");
    assert_eq!(rows[1], "<>the.app's.t[12:00]");
    assert_eq!(rows[2], "....................");
    assert_eq!(rows[3], "..more.text.........");
    // The fish swims behind the words and comes out between them.
    let rows = show(&mut scene, &mock, &grid, 300);
    assert_eq!(rows[1], "..the.app's.t[12:00]");
    let rows = show(&mut scene, &mock, &grid, 400);
    assert_eq!(rows[1], "..the>app's.t[12:00]");
}

#[test]
fn back_layers_never_touch_ink() {
    let mock = Mock::new();
    let mut rng = Rng::new(7);
    let colours = [
        Color::Default,
        Color::Indexed(1),
        Color::Indexed(8),
        Color::Rgb(9, 9, 9),
    ];
    for round in 0..200 {
        let mut grid = base();
        for y in 0..6 {
            for x in 0..20 {
                let mut style = Style {
                    fg: colours[below(&mut rng, 4) as usize],
                    bg: colours[below(&mut rng, 4) as usize],
                    ..Style::default()
                };
                if below(&mut rng, 8) == 0 {
                    style.attrs.insert(Attrs::INVERSE);
                }
                match below(&mut rng, 6) {
                    0 | 1 => {
                        grid.put(x, y, Cell::narrow("t", style));
                    }
                    2 if x < 19 => {
                        grid.put(x, y, Cell::wide("世", style));
                    }
                    // The half of a wide glyph keeps the style of its head.
                    _ if grid.cell(x, y).width == 0 => {}
                    _ => grid.cell_mut(x, y).style = style,
                }
            }
        }
        let mut scene = Scene::new();
        let mut named = Vec::new();
        for _ in 0..4 {
            let rows: Vec<String> = (0..3)
                .map(|_| {
                    (0..8)
                        .map(|_| ["#", " ", "界", "~"][below(&mut rng, 4) as usize])
                        .collect()
                })
                .collect();
            let mut style = json!({});
            if below(&mut rng, 2) == 0 {
                style["fg"] = json!("green");
            }
            if below(&mut rng, 2) == 0 {
                style["bg"] = json!("yellow");
            }
            let mut v = json!({
                "anchor": {"at": [below(&mut rng, 18), below(&mut rng, 5)]},
                "content": {"rows": rows, "style": style, "opaque": below(&mut rng, 2) == 0},
                "layer": -1 - below(&mut rng, 3) as i32,
                "blend": if below(&mut rng, 5) == 0 { "tint" } else { "normal" },
            });
            // A sprite on a panel may replace that backdrop, no other.
            if below(&mut rng, 3) == 0 {
                let panel = [Color::Indexed(1), Color::Indexed(8)][below(&mut rng, 2) as usize];
                v["panel"] = json!(ColorSpec(panel));
                named.push(panel);
            }
            scene.add(1, spec(v), ms(0)).unwrap();
        }
        let out = render(&mut scene, &mock, &grid, 0);
        for y in 0..6 {
            for x in 0..20 {
                let (app, got) = (grid.cell(x, y), out.cell(x, y));
                if app.has_ink() {
                    assert_eq!(
                        (&got.text, got.width, got.style.fg, got.style.attrs),
                        (&app.text, app.width, app.style.fg, app.style.attrs),
                        "round {round} at {x},{y}"
                    );
                }
                // A backdrop the app chose is kept, but the one a sprite
                // names as its panel; inverse cells are kept in any case.
                let inverse = app.style.attrs.contains(Attrs::INVERSE);
                let named = !inverse && named.contains(&app.style.bg);
                if inverse || (app.backdrop() != Color::Default && !named) {
                    assert_eq!(got.backdrop(), app.backdrop(), "round {round} at {x},{y}");
                }
                // Wide pairs stay whole.
                if got.width == 2 {
                    assert_eq!(out.cell(x + 1, y).width, 0, "round {round} at {x},{y}");
                    assert_eq!(
                        out.cell(x + 1, y).style,
                        got.style,
                        "round {round} at {x},{y}"
                    );
                }
            }
        }
    }
}

#[test]
fn layer_patch_reorders() {
    let mock = Mock::new();
    let grid = Grid::from_lines(20, &["ab", "", "", "", "", ""]);
    let mut scene = Scene::new();
    let id = scene
        .add(1, spec(json!({"content": {"rows": ["###"]}})), ms(0))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &grid, 0)[0], "ab#");
    scene
        .update(
            id,
            serde_json::from_value(json!({"layer": "front"})).unwrap(),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &grid, 0)[0], "###");
    assert_eq!(scene.list()[0].layer, 1);
    // Layers order before `z`.
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["x"]}, "layer": 2, "z": -5})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &grid, 0)[0], "x##");
}

#[test]
fn a_show_waits_to_be_seen() {
    let mut mock = Mock::new();
    mock.pane().usable = false;
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"frame": PANE, "frames": [{"rows": ["1"], "ms": 100}, {"rows": ["2"], "ms": 100}]})),
            ms(0),
        )
        .unwrap();
    // The frame is not usable yet: the clock is held and nothing wakes
    // the scene for the animation.
    assert_eq!(show(&mut scene, &mock, &base(), 150)[1], "");
    assert_eq!(scene.next_wake(), None);
    assert_eq!(show(&mut scene, &mock, &base(), 5000)[1], "");
    assert!(!scene.is_empty());
    mock.pane().usable = true;
    assert_eq!(show(&mut scene, &mock, &base(), 6000)[1].trim(), "1");
    assert_eq!(scene.next_wake(), Some(ms(6100)));
    assert_eq!(show(&mut scene, &mock, &base(), 6100)[1].trim(), "2");
    assert_eq!(show(&mut scene, &mock, &base(), 6200)[1], "");
    assert!(scene.is_empty());
    // Hidden again after it started, the clock runs: `ttl_ms` aside,
    // only the first showing is waited for.
}

#[test]
fn text_anchor_slides_to_stay_whole() {
    let mut mock = Mock::new();
    mock.lines = vec!["top".into(), "".into(), "".into(), "end".into()];
    let mut scene = Scene::new();
    let mut put = |text: &str, side: &str, rows: serde_json::Value| {
        let anchor = json!({"text": text, "side": side});
        scene
            .add(
                1,
                spec(json!({"frame": PANE, "anchor": anchor, "content": {"rows": rows}})),
                ms(0),
            )
            .unwrap()
    };
    // Centred on the first and on the last row of the view: three rows
    // would lose one; they slide inside.
    put("top", "right", json!(["1", "2", "3"]));
    put("end", "right", json!(["a", "b", "c"]));
    // Centred on a word at the left edge: slides right.
    put("top", "below", json!(["vvvvv"]));
    let rows = show(&mut scene, &mock, &base(), 0);
    // Rows 1..=3 and 2..=4 in column 14; the last one drawn wins.
    assert_eq!(
        rows,
        [
            "",
            "              1",
            "          vvvvv",
            "              b",
            "              c",
            ""
        ]
    );
    // The word leaves the view: nothing is dragged along. What is left
    // of its sprites is where a strict anchor puts it, clipped.
    mock.pane().origin = FramePos { x: 0, y: 1 };
    let rows = show(&mut scene, &mock, &base(), 1);
    assert_eq!(rows[1..3], ["          vvvv3", "              a"]);
}

#[test]
fn list_on_tells_what_shows() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    let mut put = |v: serde_json::Value| scene.add(1, spec(v), ms(0)).unwrap();
    let back = put(json!({"anchor": {"at": [0, 0]}, "content": {"rows": ["abcd"]}}));
    let front =
        put(json!({"anchor": {"at": [0, 0]}, "layer": "front", "content": {"rows": ["abcd"]}}));
    let out = put(
        json!({"frame": PANE, "anchor": {"at": [0, 9], "space": "frame"}, "content": {"rows": ["x"]}}),
    );
    let mut base = base();
    base.put_ascii(0, 0, b"T", Style::default());
    scene.tick(ms(0), &mock);
    let of = |id| {
        let list = scene.list_on(&base, &mock);
        list.into_iter().find(|s| s.id == id).unwrap()
    };
    // One glyph of four under ink.
    assert!(of(back).visible);
    assert_eq!(of(back).covered, 0.25);
    assert!(of(front).visible);
    assert_eq!(of(front).covered, 0.0);
    // Below the view: a screen position, nothing shown.
    assert_eq!(of(out).screen, Some([10, 10]));
    assert!(!of(out).visible);
    scene.remove(front);
    assert_eq!(scene.compose(&base, &mock).text()[0], "Tbcd");
}

#[test]
fn forgotten_rows_and_frames_take_their_sprites() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    let mut put = |frame: FrameId, y: i64| {
        let anchor = json!({"at": [0, y], "space": "frame"});
        scene
            .add(
                1,
                spec(json!({"frame": frame, "anchor": anchor, "content": {"rows": ["x"]}})),
                ms(0),
            )
            .unwrap()
    };
    let (here, far, other) = (put(PANE, 2), put(PANE, 100_005), put(RAW, 100_005));
    let hud = scene
        .add(
            1,
            spec(json!({"frame": PANE, "content": {"rows": ["h"]}})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    scene.take_dirty();
    // An island of the pane is forgotten: only what is fixed on its rows.
    assert_eq!(scene.forget_rows(PANE, 99_800, 100_220), vec![far]);
    assert!(scene.take_dirty());
    assert!(scene.forget_rows(PANE, 99_800, 100_220).is_empty());
    let ids = |scene: &Scene| scene.list().iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(&scene), vec![here, other, hud]);
    // The frame itself: everything on it.
    assert_eq!(scene.drop_frame(PANE), vec![here, hud]);
    assert_eq!(ids(&scene), vec![other]);
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

fn host() -> HostColors {
    HostColors {
        fg: [200, 200, 200],
        bg: [20, 20, 40],
    }
}

#[test]
fn named_layers_are_ordered() {
    let order: Vec<i32> = ["wallpaper", "back", "front", "hud"]
        .iter()
        .map(|n| n.parse::<Layer>().unwrap().0)
        .collect();
    assert_eq!(order, vec![-100, -1, 1, 100]);
    assert!(order.is_sorted());
    assert_eq!(
        serde_json::from_value::<Layer>(json!("hud")).unwrap(),
        Layer::HUD
    );
    assert!("app".parse::<Layer>().unwrap_err().contains("the app's"));
    assert!(
        "sky"
            .parse::<Layer>()
            .unwrap_err()
            .contains("wallpaper, back, front, hud")
    );

    // A wallpaper is behind a back sprite added before it; a hud in
    // front of a front sprite added after it.
    let mock = Mock::new();
    let mut scene = Scene::new();
    for (layer, text) in [
        ("hud", "H"),
        ("front", "f"),
        ("back", "b"),
        ("wallpaper", "w"),
    ] {
        scene
            .add(
                1,
                spec(json!({"content": {"rows": [text]}, "layer": layer})),
                ms(0),
            )
            .unwrap();
    }
    assert_eq!(show(&mut scene, &mock, &base(), 0)[0], "H");
    let layers: Vec<i32> = scene.list().iter().map(|s| s.layer).collect();
    assert_eq!(layers, vec![-100, -1, 1, 100]);
}

#[test]
fn alpha_is_checked_and_listed() {
    let mut scene = Scene::new();
    for bad in [json!(-0.1), json!(1.5)] {
        let err = scene
            .add(
                1,
                spec(json!({"content": {"rows": ["x"]}, "alpha": bad})),
                ms(0),
            )
            .unwrap_err();
        assert_eq!(err.code(), "bad-params");
    }
    let id = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["x"]}, "alpha": 0.25})),
            ms(0),
        )
        .unwrap();
    let plain = scene
        .add(1, spec(json!({"content": {"rows": ["x"]}})), ms(0))
        .unwrap();
    let alpha = |scene: &Scene, id| scene.list().iter().find(|s| s.id == id).unwrap().alpha;
    assert_eq!(alpha(&scene, id), 0.25);
    assert_eq!(alpha(&scene, plain), 1.0);
    let patch = |v| serde_json::from_value::<SpritePatch>(v).unwrap();
    assert!(scene.update(id, patch(json!({"alpha": 2})), ms(0)).is_err());
    assert_eq!(alpha(&scene, id), 0.25);
    scene.update(id, patch(json!({"alpha": 1})), ms(0)).unwrap();
    assert_eq!(alpha(&scene, id), 1.0);
    // The host colours repaint.
    scene.take_dirty();
    scene.set_host_colors(scene.host_colors());
    assert!(!scene.take_dirty());
    scene.set_host_colors(host());
    assert!(scene.take_dirty());
}

#[test]
fn back_alpha_mixes_with_the_backdrop() {
    let mock = Mock::new();
    let panel = rgb(100, 100, 100);
    let inverse = Style {
        attrs: Attrs(Attrs::INVERSE),
        ..Style::default()
    };
    let mut grid = Grid::from_lines(20, &["ab    z ", "", "", "", "", ""]);
    for x in 4..6 {
        grid.cell_mut(x, 0).style.bg = panel;
    }
    grid.cell_mut(6, 0).style = inverse;
    let mut scene = Scene::new();
    scene.set_host_colors(host());
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["       *"], "style": {"fg": "#ffffff", "bg": "#ff0000"}, "opaque": true}, "alpha": 0.5})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.text()[0], "ab    z*");
    // Under text and on an empty cell: half red on the host's default.
    let on_host = rgb(138, 10, 20);
    assert_eq!(out.cell(0, 0).style.bg, on_host);
    assert_eq!(out.cell(0, 0).style.fg, Color::Default);
    assert_eq!(out.cell(2, 0).style.bg, on_host);
    // On a panel of the app: half red on the panel.
    assert_eq!(out.cell(4, 0).style.bg, rgb(178, 50, 50));
    // Inverse video stays the app's.
    assert_eq!(out.cell(6, 0).style, inverse);
    // The glyph fades into what it stands on.
    assert_eq!(out.cell(7, 0).style.fg, rgb(197, 133, 138));
    assert_eq!(out.cell(7, 0).style.bg, on_host);

    // Two glazes stack; an opaque colour behind is their ground on open
    // cells and stays hidden under a panel.
    let mut scene = Scene::new();
    scene.set_host_colors(host());
    for (layer, bg, alpha) in [
        (-3, "#0000ff", json!(null)),
        (-2, "#ff0000", json!(0.5)),
        (-1, "#00ff00", json!(0.5)),
    ] {
        let mut v = json!({"content": {"rows": ["      "], "style": {"bg": bg}, "opaque": true}, "layer": layer});
        if !alpha.is_null() {
            v["alpha"] = alpha;
        }
        scene.add(1, spec(v), ms(0)).unwrap();
    }
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.cell(2, 0).style.bg, rgb(64, 128, 64));
    assert_eq!(out.cell(4, 0).style.bg, rgb(89, 153, 25));

    // A palette colour goes through the xterm table; a tint behind is a
    // glaze too.
    let mut scene = Scene::new();
    scene.set_host_colors(host());
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["##"], "style": {"bg": 196}}, "blend": "tint", "layer": "back", "alpha": 0.5})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.text()[0], "ab    z");
    assert_eq!(out.cell(1, 0).style.bg, on_host);
}

#[test]
fn front_alpha_is_glass_fade_and_tint() {
    let mock = Mock::new();
    let mut grid = Grid::from_lines(20, &["abcd", "", "", "", "", ""]);
    grid.cell_mut(1, 0).style = Style {
        fg: Color::Indexed(1),
        bg: Color::Indexed(4),
        attrs: Attrs(Attrs::INVERSE),
        ..Style::default()
    };
    let mut scene = Scene::new();
    scene.set_host_colors(host());
    // Glass: blanks with a colour, over the text.
    let glass = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["  "], "style": {"bg": "#ffffff"}, "opaque": true}, "layer": "front", "alpha": 0.5})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.text()[0], "abcd");
    assert_eq!(out.cell(0, 0).style.bg, rgb(138, 138, 148));
    assert_eq!(out.cell(0, 0).style.fg, rgb(228, 228, 228));
    // An inverse cell: its colours as shown, then mixed; not inverse.
    let b = out.cell(1, 0).style;
    assert!(!b.attrs.contains(Attrs::INVERSE));
    assert_eq!(b.bg, rgb(230, 128, 128));
    assert_eq!(b.fg, rgb(128, 128, 247));
    assert_eq!(out.cell(2, 0).style, Style::default());
    scene.remove(glass);

    // A glyph replaces the text; its colour fades into the ground, and a
    // colour it does not set stays as the app wrote it.
    let id = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["X"], "style": {"fg": "#ffffff"}}, "layer": "front", "alpha": 0.25})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.text()[0], "Xbcd");
    assert_eq!(out.cell(0, 0).style.fg, rgb(79, 79, 94));
    assert_eq!(out.cell(0, 0).style.bg, Color::Default);
    scene.remove(id);

    // A tint: each colour mixed with the one it replaces.
    scene
        .add(
            1,
            spec(json!({"content": {"rows": ["#"], "style": {"fg": "#000000", "bold": true}}, "blend": "tint", "alpha": 0.5})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&grid, &mock);
    assert_eq!(out.text()[0], "abcd");
    let a = out.cell(0, 0).style;
    assert_eq!(a.fg, rgb(100, 100, 100));
    assert_eq!(a.bg, Color::Default);
    assert!(a.attrs.contains(Attrs::BOLD));
}

#[test]
fn alpha_one_changes_nothing() {
    let mock = Mock::new();
    let grid = Grid::from_lines(20, &["ab  cd", "", "", "", "", ""]);
    let compose = |alpha: serde_json::Value| {
        let mut scene = Scene::new();
        for layer in ["back", "front"] {
            let mut v = json!({"anchor": {"at": [1, 0]}, "content": {"rows": ["* *"], "style": {"fg": "red", "bg": 17}, "opaque": true}, "layer": layer, "z": 1});
            if !alpha.is_null() {
                v["alpha"] = alpha.clone();
            }
            scene.add(1, spec(v), ms(0)).unwrap();
        }
        scene.tick(ms(0), &mock);
        scene.compose(&grid, &mock)
    };
    assert_eq!(compose(json!(1.0)), compose(json!(null)));
    assert_ne!(compose(json!(0.5)), compose(json!(null)));
}

#[test]
fn a_wide_front_glyph_needs_one_backdrop() {
    let mock = Mock::new();
    let mut grid = Grid::from_lines(20, &["abcdef", "", "", "", "", ""]);
    grid.cell_mut(3, 0).style.bg = Color::Indexed(4);
    let mut scene = Scene::new();
    let mut put = |x: i64, style: serde_json::Value| {
        let v = json!({"anchor": {"at": [x, 0]}, "content": {"rows": ["界"], "style": style}, "layer": "front"});
        scene.add(1, spec(v), ms(0)).unwrap();
    };
    // On one backdrop: drawn. Across two: not drawn, unless it brings
    // its own.
    put(0, json!({}));
    put(2, json!({}));
    put(4, json!({}));
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.text()[0], "界cd界");
    assert_eq!(out.cell(3, 0).style.bg, Color::Indexed(4));
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [2, 0]}, "content": {"rows": ["界"], "style": {"bg": "red"}}, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.text()[0], "ab界ef");
}

/// Two glyphs, one whole and one at half opacity; counts its renders.
struct Fade(u32);

impl Effect for Fade {
    fn render(&mut self, _t: Duration, canvas: &mut Canvas) -> bool {
        self.0 += 1;
        let style = StyleSpec::fg(rgb(255, 255, 255));
        canvas.put(0, 0, "a", &style);
        canvas.put_faded(1, 0, "b", &style, 0.5);
        canvas.put(2, 0, &self.0.to_string(), &style);
        true
    }
}

#[test]
fn an_effect_fades_its_own_glyphs() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    scene.set_host_colors(host());
    scene.register_effect("fade", Box::new(|_| Ok(Box::new(Fade(0)))));
    for (layer, alpha) in [("back", 1.0), ("front", 1.0), ("front", 0.5)] {
        scene.clear(None);
        let fx = json!({"layer": layer, "alpha": alpha, "effect": {"name": "fade"}});
        scene.add(1, spec(fx), ms(0)).unwrap();
        scene.tick(ms(0), &mock);
        let out = scene.compose(&base(), &mock);
        assert_eq!(out.text()[0], "ab1");
        // White at `alpha` on the host's background (20, 20, 40); the
        // faded glyph at half of it.
        let on = |a: f32| {
            let mix = |bg: u8| (bg as f32 + (255.0 - bg as f32) * a).round() as u8;
            rgb(mix(20), mix(20), mix(40))
        };
        let whole = if alpha < 1.0 {
            on(alpha)
        } else {
            rgb(255, 255, 255)
        };
        assert_eq!(out.cell(0, 0).style.fg, whole, "{layer} {alpha}");
        assert_eq!(
            out.cell(1, 0).style.fg,
            on(alpha * 128.0 / 255.0),
            "{layer} {alpha}"
        );
    }
}

#[test]
fn a_running_effect_is_tuned_in_place() {
    let mock = Mock::new();
    let (mut scene, seen) = echo_scene(Looks::Cells);
    scene.register_effect("fade", Box::new(|_| Ok(Box::new(Fade(0)))));
    let id = scene
        .add(1, spec(json!({"effect": {"name": "echo"}})), ms(0))
        .unwrap();
    scene.tick_on(ms(0), &base(), &mock);
    scene.tick_on(ms(60), &base(), &mock);
    assert_eq!(seen.lock().unwrap().looks, 2);
    let patch = |v| serde_json::from_value::<SpritePatch>(v).unwrap();
    // Same name: the effect is told, and keeps what it has.
    let tune = json!({"effect": {"name": "echo", "params": {"rate": 3}}});
    scene.update(id, patch(tune), ms(70)).unwrap();
    assert_eq!(seen.lock().unwrap().params, json!({"rate": 3}));
    assert_eq!(seen.lock().unwrap().looks, 2);
    // Drawn again at once.
    scene.tick_on(ms(71), &base(), &mock);
    assert_eq!(seen.lock().unwrap().looks, 3);
    // Refused by the effect: nothing changes, not the layer either.
    let bad = json!({"layer": "front", "effect": {"name": "echo", "params": {"bad": 1}}});
    assert_eq!(
        scene.update(id, patch(bad), ms(80)).unwrap_err().code(),
        "bad-params"
    );
    assert_eq!(scene.list()[0].layer, -1);
    assert_eq!(seen.lock().unwrap().params, json!({"rate": 3}));
    // An effect that takes no change says so.
    for wrong in [
        json!({"effect": {"name": "nope"}}),
        json!({"effect": {"name": "echo"}, "content": {"rows": ["x"]}}),
    ] {
        assert!(scene.update(id, patch(wrong), ms(80)).is_err());
    }
    // Another effect starts in its place, with the sprite's fields.
    scene
        .update(id, patch(json!({"effect": {"name": "fade"}})), ms(90))
        .unwrap();
    assert_eq!(scene.list()[0].kind, "effect:fade");
    assert_eq!(show(&mut scene, &mock, &base(), 90)[0], "ab1");
    let again = json!({"effect": {"name": "fade", "params": {}}});
    let err = scene.update(id, patch(again), ms(95)).unwrap_err();
    assert!(err.to_string().contains("cannot be changed"), "{err}");
    // A content becomes an effect.
    let text = scene
        .add(
            1,
            spec(json!({"content": {"rows": ["zz"]}, "anchor": {"at": [0, 2]}})),
            ms(0),
        )
        .unwrap();
    scene
        .update(text, patch(json!({"effect": {"name": "fade"}})), ms(100))
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 100)[2], "ab1");
}

/// Notes the shifts it is told and the rows it is shown out of the view.
struct Rows(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

impl Effect for Rows {
    fn render(&mut self, _t: Duration, _canvas: &mut Canvas) -> bool {
        true
    }

    fn looks(&self) -> Looks {
        Looks::Cells
    }

    fn look(&mut self, under: &Under) {
        let show = |y: i64| match under.row(y) {
            Some(ink) => ink.iter().map(|i| if *i { '#' } else { '.' }).collect(),
            None => "none".to_string(),
        };
        let mut notes = self.0.lock().unwrap();
        notes.push(format!("{} {} {}", show(-1), show(0), show(4)));
    }

    fn shift(&mut self, from: i64, to: i64, dy: i64) {
        self.0
            .lock()
            .unwrap()
            .push(format!("shift {from} {to} {dy}"));
    }
}

/// `Mock`, remembering one row of ink under the pane.
struct Remembers(Mock);

impl Surfaces for Remembers {
    fn info(&self, id: FrameId) -> Option<SurfaceInfo> {
        self.0.info(id)
    }
    fn resolve(&self, sel: &FrameSel) -> Option<FrameId> {
        self.0.resolve(sel)
    }
    fn find(&self, id: FrameId, query: &TextQuery, known: bool) -> Vec<TextMatch> {
        self.0.find(id, query, known)
    }
    fn place(&self, id: FrameId, w: u16, h: u16, p: Prefer, t: &[Rect]) -> Option<(u16, u16)> {
        self.0.place(id, w, h, p, t)
    }
    fn ink_row(&self, id: FrameId, y: i64, x: i64, w: u16) -> Option<Vec<bool>> {
        (id == PANE && y == 4).then(|| (x..x + w as i64).map(|x| x % 2 == 0).collect())
    }
    fn cursor(&self) -> (u16, u16) {
        self.0.cursor()
    }
    fn cursor_trail(&self) -> Vec<CursorSample> {
        self.0.cursor_trail()
    }
}

#[test]
fn an_effect_is_shown_rows_out_of_the_view_and_told_a_shift() {
    let mock = Remembers(Mock::new());
    let grid = base();
    let notes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut scene = Scene::new();
    let shared = notes.clone();
    scene.register_effect(
        "rows",
        Box::new(move |_| Ok(Box::new(Rows(shared.clone())))),
    );
    let fx = json!({"frame": PANE, "effect": {"name": "rows"}});
    scene.add(1, spec(fx), ms(0)).unwrap();
    scene.tick_on(ms(0), &grid, &mock);
    // Above: not remembered. A row of the view: not given. Under the
    // view: what the frame remembers.
    assert_eq!(notes.lock().unwrap()[..], ["none none #.#.#.#.#."]);

    // A shift is told to the effects of the frame, which then look and
    // render off their period.
    scene.take_dirty();
    scene.shift(RAW, 0, 9, 1);
    assert!(!scene.dirty());
    scene.shift(PANE, 2, 9, 3);
    assert!(scene.dirty());
    scene.tick_on(ms(1), &grid, &mock);
    assert_eq!(
        notes.lock().unwrap()[1..],
        ["shift 2 9 3", "none none #.#.#.#.#."]
    );
}

#[test]
fn a_back_sprite_on_a_panel_is_clipped_to_it_and_replaces_its_colour() {
    let mock = Mock::new();
    let mut grid = base();
    let blue = Style {
        bg: Color::Indexed(4),
        ..Style::default()
    };
    // A blue panel, columns 4..10 of rows 1..3, with a word on it.
    for y in 1..3 {
        for x in 4..10 {
            grid.put(x, y, Cell::narrow(" ", blue));
        }
    }
    grid.put(5, 1, Cell::narrow("w", blue));
    let rows = ["####################"; 4];
    let v = json!({
        "anchor": {"at": [0, 0]},
        "content": {"rows": rows, "style": {"fg": "green", "bg": "red"}},
    });

    // Not on the panel: the colour stops at it, the glyphs show on blue.
    let mut scene = Scene::new();
    let id = scene.add(1, spec(v.clone()), ms(0)).unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.cell(3, 1).style.bg, Color::Indexed(1));
    assert_eq!(out.cell(4, 1).style.bg, Color::Indexed(4));
    assert_eq!(out.cell(4, 1).text, "#");

    // On the panel: nothing outside; inside, red replaces blue, under
    // the word too, which keeps its glyph.
    let patch = serde_json::from_value(json!({"panel": "blue"})).unwrap();
    scene.update(id, patch, ms(0)).unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.cell(3, 1), grid.cell(3, 1));
    assert_eq!(out.cell(4, 0), grid.cell(4, 0));
    assert_eq!(out.cell(4, 1).text, "#");
    assert_eq!(out.cell(4, 1).style.bg, Color::Indexed(1));
    assert_eq!(out.cell(5, 1).text, "w");
    assert_eq!(out.cell(5, 1).style.bg, Color::Indexed(1));
    assert_eq!(out.cell(9, 2).style.bg, Color::Indexed(1));
    assert_eq!(out.cell(10, 2), grid.cell(10, 2));
    let info = &scene.list()[0];
    assert_eq!(info.panel, Some(ColorSpec(Color::Indexed(4))));

    // In front: clipped the same way.
    let mut front = v.clone();
    front["layer"] = json!("front");
    front["panel"] = json!("blue");
    let mut scene = Scene::new();
    scene.add(1, spec(front), ms(0)).unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.cell(3, 1), grid.cell(3, 1));
    assert_eq!(out.cell(5, 1).text, "#");

    // A glaze on the panel is mixed with the colour laid there.
    let mut glaze = v.clone();
    glaze["panel"] = json!("blue");
    glaze["alpha"] = json!(0.5);
    glaze["content"] = json!({"rows": rows, "opaque": true, "style": {"bg": "#ffffff"}});
    glaze["layer"] = json!(-1);
    let mut under = v;
    under["panel"] = json!("blue");
    under["layer"] = json!(-2);
    under["content"] = json!({"rows": rows, "opaque": true, "style": {"bg": "#000000"}});
    let mut scene = Scene::new();
    scene.add(1, spec(under), ms(0)).unwrap();
    scene.add(1, spec(glaze), ms(0)).unwrap();
    let out = render(&mut scene, &mock, &grid, 0);
    assert_eq!(out.cell(6, 2).style.bg, Color::Rgb(128, 128, 128));
}

#[test]
fn a_back_glyph_fades_on_an_inverse_cell() {
    let mock = Mock::new();
    let mut grid = base();
    let mut bar = Style {
        fg: Color::Rgb(0, 0, 200),
        ..Style::default()
    };
    bar.attrs.insert(Attrs::INVERSE);
    grid.put(2, 0, Cell::narrow(" ", bar));
    let v = json!({
        "anchor": {"at": [2, 0]}, "alpha": 0.5,
        "content": {"rows": ["#"], "style": {"fg": "#ffffff"}},
    });
    let mut scene = Scene::new();
    scene.add(1, spec(v), ms(0)).unwrap();
    let got = render(&mut scene, &mock, &grid, 0).cell(2, 0).clone();
    // Still inverse on the bar's colour; the glyph half way to white.
    assert_eq!(got.text, "#");
    assert!(got.style.attrs.contains(Attrs::INVERSE));
    assert_eq!(got.style.fg, Color::Rgb(0, 0, 200));
    assert_eq!(got.style.bg, Color::Rgb(128, 128, 228));
}

#[test]
fn a_client_declares_layer_names() {
    let mut scene = Scene::new();
    scene.define_layer("stage", -50).unwrap();
    scene.define_layer("lights", 50).unwrap();
    scene.define_layer("stage", -40).unwrap();
    let names: Vec<(String, i32)> = scene.layers();
    let names: Vec<(&str, i32)> = names.iter().map(|(n, z)| (n.as_str(), *z)).collect();
    assert_eq!(
        names,
        [
            ("wallpaper", -100),
            ("stage", -40),
            ("back", -1),
            ("front", 1),
            ("lights", 50),
            ("hud", 100)
        ]
    );
    for (name, z) in [
        ("app", 3),
        ("stage", 0),
        ("back", -1),
        ("12", 4),
        ("", 1),
        ("a b", 1),
    ] {
        assert!(scene.define_layer(name, z).is_err(), "{name} {z}");
    }
    // On the wire a declared name becomes its integer; others are left.
    let mut v = json!({"content": {"rows": ["x"]}, "layer": "stage"});
    scene.name_layer(&mut v);
    assert_eq!(spec(v).layer, Some(Layer(-40)));
    let mut v = json!({"layer": "front"});
    scene.name_layer(&mut v);
    assert_eq!(v, json!({"layer": "front"}));
}

#[test]
fn hidden_owner_stands_still() {
    let mock = Mock::new();
    let frames = json!([{"rows": ["1"], "ms": 100}, {"rows": ["2"], "ms": 100}]);
    let mut scene = Scene::new();
    scene
        .add(
            1,
            spec(json!({"frames": frames, "loop": true, "ttl_ms": 1000})),
            ms(0),
        )
        .unwrap();
    scene
        .add(
            2,
            spec(json!({"anchor": {"at": [3, 0], "space": "view"},
                        "content": {"rows": ["o"]}})),
            ms(0),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 50)[0], "1  o");
    scene.take_dirty();
    scene.hide_owner(1, true, ms(50));
    assert!(scene.take_dirty() && scene.owner_hidden(1));
    // Nothing of it is drawn, nothing is due; the other goes on.
    assert_eq!(show(&mut scene, &mock, &base(), 5000)[0], "   o");
    assert_eq!(scene.next_wake(), None);
    assert_eq!(scene.list().len(), 2);
    // Added while hidden: hidden.
    scene
        .add(
            1,
            spec(json!({"anchor": {"at": [5, 0], "space": "view"},
                        "content": {"rows": ["x"]}})),
            ms(6000),
        )
        .unwrap();
    assert_eq!(show(&mut scene, &mock, &base(), 6000)[0], "   o");
    // Shown again: the clocks go on from where they were.
    scene.hide_owner(1, false, ms(10_050));
    assert_eq!(show(&mut scene, &mock, &base(), 10_149)[0], "1  o x");
    assert_eq!(show(&mut scene, &mock, &base(), 10_150)[0], "2  o x");
    assert_eq!(show(&mut scene, &mock, &base(), 10_999)[0], "2  o x");
    assert_eq!(show(&mut scene, &mock, &base(), 11_000)[0], "   o x");
}

#[test]
fn colours_and_runs_on_the_wire() {
    let color = |v: serde_json::Value| serde_json::from_value::<ColorSpec>(v);
    assert_eq!(color(json!(196)).unwrap().0, Color::Indexed(196));
    assert_eq!(color(json!("red")).unwrap().0, Color::Indexed(1));
    assert_eq!(color(json!("#0a0b0c")).unwrap().0, Color::Rgb(10, 11, 12));
    assert_eq!(
        serde_json::from_str::<ColorSpec>("\"bright-red\"")
            .unwrap()
            .0,
        Color::Indexed(9)
    );
    for (bad, message) in [
        (json!("gren"), "bad colour: gren"),
        (json!(256), "bad colour: 256"),
        (json!(-1), "bad colour: -1"),
    ] {
        assert_eq!(color(bad).unwrap_err().to_string(), message);
    }
    assert!(color(json!(true)).is_err());
    assert!(color(json!(1.5)).is_err());

    let wire = json!({"t": "ab", "fg": 196, "bg": "blue", "bold": true});
    let run: Run = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(run.t, "ab");
    assert_eq!(run.style.fg, Some(ColorSpec(Color::Indexed(196))));
    assert_eq!(run.style.bg, Some(ColorSpec(Color::Indexed(4))));
    assert!(run.style.bold && !run.style.dim);
    assert_eq!(serde_json::to_value(&run).unwrap(), wire);
    // Unknown keys are ignored, as before; the text is required.
    assert!(serde_json::from_value::<Run>(json!({"t": "a", "x": 1})).is_ok());
    assert!(serde_json::from_value::<Run>(json!({"fg": 1})).is_err());
}

/// A full-view sprite of braille runs, as an app redraws it each frame:
/// `cols x rows`, a run of 1 to 8 dots then a gap, a colour per run.
fn braille_patch(cols: usize, rows: usize, seed: u64) -> serde_json::Value {
    let mut rng = Rng::new(seed);
    let cells: Vec<serde_json::Value> = (0..rows)
        .map(|_| {
            let mut runs = Vec::new();
            let mut x = 0;
            while x < cols {
                let gap = (rng.next_u64() % 12) as usize;
                let n = 1 + (rng.next_u64() % 8) as usize;
                let n = n.min(cols - x);
                let dots: String = (0..n)
                    .map(|_| char::from_u32(0x2801 + (rng.next_u64() % 255) as u32).unwrap())
                    .collect();
                runs.push(json!({"t": dots, "fg": 16 + rng.next_u64() % 216}));
                x += n;
                let gap = gap.min(cols - x);
                if gap > 0 {
                    runs.push(json!({"t": " ".repeat(gap)}));
                    x += gap;
                }
            }
            json!(runs)
        })
        .collect();
    json!({"content": {"cells": cells}})
}

/// Cost of a sprite update and of a repaint at 200x60, best of 25
/// rounds. `cargo test --release -p ptyrc-core cost_scene -- --ignored
/// --nocapture`.
#[test]
#[ignore = "timing"]
fn cost_scene_200x60() {
    use std::time::Instant;
    let (cols, rows) = (200u16, 60u16);
    let mock = Mock {
        infos: vec![surface(RAW, Rect::new(0, 0, cols, rows), (0, 0))],
        ..Mock::new()
    };
    let lines: Vec<String> = (0..rows)
        .map(|y| format!("{y:3} words in the way, more words here"))
        .collect();
    let lines: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let base = Grid::from_lines(cols, &lines);
    let patches: Vec<serde_json::Value> = (0..8)
        .map(|i| braille_patch(cols as usize, rows as usize, i))
        .collect();
    let bytes = serde_json::to_string(&patches[0]).unwrap().len();
    let best = |f: &mut dyn FnMut()| {
        (0..25)
            .map(|_| {
                let start = Instant::now();
                for _ in 0..8 {
                    f();
                }
                start.elapsed().as_secs_f64() * 1e6 / 8.0
            })
            .fold(f64::MAX, f64::min)
    };
    for (name, layer) in [("back", -1), ("front", 1)] {
        let mut scene = Scene::new();
        let mut first = spec(patches[0].clone());
        first.layer = Some(Layer(layer));
        let id = scene.add(1, first, ms(0)).unwrap();
        let mut k = 0usize;
        let parse = best(&mut || {
            k += 1;
            let patch: SpritePatch =
                serde_json::from_value(patches[k % patches.len()].clone()).unwrap();
            std::hint::black_box(patch);
        });
        let clone = best(&mut || {
            k += 1;
            std::hint::black_box(patches[k % patches.len()].clone());
        });
        let parsed: Vec<SpritePatch> = patches
            .iter()
            .map(|p| serde_json::from_value(p.clone()).unwrap())
            .collect();
        let update = best(&mut || {
            k += 1;
            scene
                .update(id, parsed[k % parsed.len()].clone(), ms(k as u64))
                .unwrap();
        });
        let patch_clone = best(&mut || {
            k += 1;
            std::hint::black_box(parsed[k % parsed.len()].clone());
        });
        let tick = best(&mut || {
            k += 1;
            scene.tick_on(ms(k as u64), &base, &mock);
        });
        let compose = best(&mut || {
            std::hint::black_box(scene.compose(&base, &mock));
        });
        let grid_clone = best(&mut || {
            std::hint::black_box(base.clone());
        });
        // The host shows a frame; the next one is composed and repaired.
        let mut host = ptyrc_vt::Vt::new(cols, rows);
        let mut sent = 0;
        let repair = best(&mut || {
            k += 1;
            scene
                .update(id, parsed[k % parsed.len()].clone(), ms(k as u64))
                .unwrap();
            scene.tick_on(ms(k as u64), &base, &mock);
            let desired = scene.compose(&base, &mock);
            let mut out = Vec::new();
            ptyrc_vt::repair(&mut host, &desired, &mut out);
            sent = out.len();
        });
        println!("  {name:5} update to host {repair:6.1} us, {sent} B sent");
        println!(
            "  {name:5} {bytes} B  parse {:6.1} us   update {:6.1} us   tick {:5.1} us   compose {:6.1} us (grid clone {:5.1})",
            parse - clone,
            update - patch_clone,
            tick,
            compose,
            grid_clone
        );
    }
}

/// A wide glyph of the app has one style: a translucent front cell over
/// one of its halves veils both, and the host is repaired once.
#[test]
fn front_alpha_veils_a_wide_glyph_whole() {
    use ptyrc_vt::{Vt, repair};
    let mock = Mock::new();
    for (at, blend, content) in [
        (
            2,
            "normal",
            json!({"rows": [" "], "style": {"bg": "red"}, "opaque": true}),
        ),
        (
            3,
            "normal",
            json!({"rows": [" "], "style": {"bg": "red"}, "opaque": true}),
        ),
        (2, "tint", json!({"rows": ["x"], "style": {"bg": "red"}})),
        (3, "tint", json!({"rows": ["x"], "style": {"fg": "red"}})),
        (3, "tint", json!({"rows": ["界"], "style": {"bg": "red"}})),
    ] {
        let mut grid = base();
        grid.put(2, 0, Cell::wide("世", Style::default()));
        grid.put(4, 0, Cell::wide("世", Style::default()));
        let mut scene = Scene::new();
        let sprite = json!({
            "anchor": {"at": [at, 0], "space": "view"},
            "content": content,
            "layer": 1,
            "blend": blend,
            "alpha": 0.5,
        });
        scene.add(1, spec(sprite.clone()), ms(0)).unwrap();
        scene.tick_on(ms(0), &grid, &mock);
        let out = scene.compose(&grid, &mock);
        assert_eq!(out.row_text(0).trim_end(), "  世世", "{sprite}");
        for head in [2, 4] {
            assert_eq!(
                out.cell(head, 0).style,
                out.cell(head + 1, 0).style,
                "{sprite}: the halves at {head} differ"
            );
        }
        // Over the head, both halves; over the other half alone, none.
        assert_eq!(
            out.cell(2, 0).style == Style::default(),
            at == 3,
            "{sprite}"
        );
        let mut host = Vt::new(20, 6);
        host.set_grid(&grid);
        let mut bytes = Vec::new();
        repair(&mut host, &out, &mut bytes);
        bytes.clear();
        repair(&mut host, &out, &mut bytes);
        assert!(bytes.is_empty(), "{sprite}: repaired again: {bytes:?}");
    }
}

/// Whatever the sprites, a wide glyph composed has one style on its two
/// halves, a spacer follows a head, and the host is repaired at once.
#[test]
fn composed_wide_pairs_are_whole() {
    use ptyrc_vt::{Vt, repair};
    let mock = Mock::new();
    let mut rng = Rng::new(11);
    let mut pick = |n: u64| (rng.next_u64() % n) as usize;
    let colours = [Color::Default, Color::Indexed(1), Color::Rgb(9, 9, 9)];
    for round in 0..300 {
        let mut grid = base();
        for y in 0..6 {
            for x in 0..20 {
                let mut style = Style {
                    fg: colours[pick(3)],
                    bg: colours[pick(3)],
                    ..Style::default()
                };
                if pick(8) == 0 {
                    style.attrs.insert(Attrs::INVERSE);
                }
                match pick(5) {
                    0 => {
                        grid.put(x, y, Cell::narrow("t", style));
                    }
                    1 | 2 if x < 19 => {
                        grid.put(x, y, Cell::wide("世", style));
                    }
                    _ => {}
                }
            }
        }
        let mut scene = Scene::new();
        for _ in 0..4 {
            let rows: Vec<String> = (0..3)
                .map(|_| (0..8).map(|_| ["#", " ", "界", "~"][pick(4)]).collect())
                .collect();
            let mut style = json!({});
            if pick(2) == 0 {
                style["fg"] = json!("green");
            }
            if pick(2) == 0 {
                style["bg"] = json!("yellow");
            }
            let frame = [RAW, PANE][pick(2)];
            let at = [pick(22) as i64 - 3, pick(7) as i64 - 2];
            let layer = [-2, -1, 1, 2][pick(4)];
            let blend = ["normal", "tint"][pick(2)];
            let opaque = pick(2) == 0;
            let mut v = json!({
                "frame": frame,
                "anchor": {"at": at, "space": "view"},
                "content": {"rows": rows, "style": style, "opaque": opaque},
                "layer": layer,
                "blend": blend,
            });
            if pick(2) == 0 {
                let alpha = [0.0, 0.3, 0.5, 1.0][pick(4)];
                v["alpha"] = json!(alpha);
            }
            scene.add(1, spec(v), ms(0)).unwrap();
        }
        scene.tick_on(ms(0), &grid, &mock);
        let out = scene.compose(&grid, &mock);
        for y in 0..6 {
            for x in 0..20 {
                let cell = out.cell(x, y);
                match cell.width {
                    2 => {
                        assert!(x < 19, "round {round}: a head on the last column");
                        let next = out.cell(x + 1, y);
                        assert_eq!(next.width, 0, "round {round} ({x}, {y}): no spacer");
                        assert_eq!(next.style, cell.style, "round {round} ({x}, {y}): halves");
                    }
                    0 => assert!(
                        x > 0 && out.cell(x - 1, y).width == 2,
                        "round {round} ({x}, {y}): a spacer alone"
                    ),
                    _ => {}
                }
            }
        }
        let mut host = Vt::new(20, 6);
        host.set_grid(&grid);
        let mut bytes = Vec::new();
        repair(&mut host, &out, &mut bytes);
        assert!(host.grid() == &out, "round {round}: host not repaired");
        bytes.clear();
        repair(&mut host, &out, &mut bytes);
        assert!(bytes.is_empty(), "round {round}: repaired twice");
    }
}

/// Draws `n` glyphs a row apart; the same ones while `still`.
struct Dots {
    n: u16,
    still: bool,
    renders: u32,
}

impl Effect for Dots {
    fn render(&mut self, _t: Duration, canvas: &mut Canvas) -> bool {
        self.renders += 1;
        let shift = if self.still {
            0
        } else {
            self.renders as i32 % 2
        };
        for i in 0..self.n as i32 {
            canvas.put(
                i * 3 + shift,
                i % canvas.height().max(1) as i32,
                "*",
                &StyleSpec::default(),
            );
        }
        true
    }

    fn period(&self) -> Duration {
        Duration::from_millis(10)
    }
}

fn dots_scene(n: u16, still: bool) -> Scene {
    let mut scene = Scene::new();
    scene.register_effect(
        "dots",
        Box::new(move |_| {
            Ok(Box::new(Dots {
                n,
                still,
                renders: 0,
            }))
        }),
    );
    scene
        .add(1, spec(json!({"effect": {"name": "dots"}})), ms(0))
        .unwrap();
    scene
}

/// An effect that draws the picture it drew leaves the scene clean; the
/// canvas is cleared of the last picture whatever was drawn.
#[test]
fn an_effect_that_draws_the_same_is_not_dirty() {
    let mock = Mock::new();
    let grid = base();
    for (still, n) in [(true, 3), (false, 3), (true, 60), (false, 60)] {
        let mut scene = dots_scene(n, still);
        let mut seen = Vec::new();
        for k in 0..6 {
            scene.tick_on(ms(k * 10), &grid, &mock);
            let dirty = scene.take_dirty();
            assert_eq!(dirty, k == 0 || !still, "still {still}, {n} dots, tick {k}");
            let rows = scene.compose(&grid, &mock).text();
            let stars = rows
                .iter()
                .flat_map(|r| r.chars())
                .filter(|c| *c == '*')
                .count();
            seen.push((rows, stars));
        }
        // Nothing of an earlier picture stays on the canvas.
        assert!(
            seen.iter().all(|(_, stars)| *stars == seen[0].1),
            "{seen:?}"
        );
        assert_eq!(seen[0].0 == seen[1].0, still);
        assert_eq!(seen[1].0, seen[3].0);
    }
}

/// Cost of the tick of an effect that draws 50 cells of 300x80, best of
/// 25 rounds. `cargo test --release -p ptyrc-core cost_effect -- --ignored
/// --nocapture`.
#[test]
#[ignore = "timing"]
fn cost_effect_tick_300x80() {
    let mock = Mock {
        infos: vec![surface(RAW, Rect::new(0, 0, 300, 80), (0, 0))],
        ..Mock::new()
    };
    let grid = Grid::new(300, 80);
    let mut scene = dots_scene(50, false);
    let mut k = 0;
    let best = (0..25)
        .map(|_| {
            let start = std::time::Instant::now();
            for _ in 0..20 {
                k += 10;
                scene.tick_on(ms(k), &grid, &mock);
            }
            start.elapsed().as_secs_f64() * 1e6 / 20.0
        })
        .fold(f64::MAX, f64::min);
    println!("  effect tick, 50 cells of 300x80: {best:.1} us");
}

/// Sprites with a `motion` move together, 30 times a second, however
/// often the scene is ticked: a tick between two steps repaints nothing.
#[test]
fn what_moves_changes_at_the_steps_only() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    // Particles: frames and lifetimes of their own, out of phase.
    for (i, at) in [0u64, 7, 19, 23].into_iter().enumerate() {
        let sprite = json!({
            "frames": [{"rows": ["o"], "ms": 40 + 9 * i}, {"rows": ["."], "ms": 31}],
            "loop": true, "ttl_ms": 500 + 77 * i,
            "motion": {"vx": 1.0 + i as f64, "ay": 9.0},
        });
        scene.add(1, spec(sprite), ms(at)).unwrap();
    }
    scene.tick(ms(23), &mock);
    scene.take_dirty();
    let (mut now, mut ticks, mut repaints) = (ms(23), 0, 0);
    while let Some(wake) = scene.next_wake() {
        assert!(wake > now, "{wake:?} at {now:?}");
        now = wake;
        scene.tick(now, &mock);
        ticks += 1;
        repaints += scene.take_dirty() as u32;
    }
    // All gone, each at the first step past its end.
    assert!(scene.is_empty());
    assert!(now >= ms(23 + 500 + 77 * 3) && now < ms(23 + 500 + 77 * 3) + MOTION_STEP);
    assert!((22..=24).contains(&ticks), "{ticks} ticks");
    assert!(repaints <= ticks);
    // At rest a sprite keeps its own times.
    let sprite = json!({
        "frames": [{"rows": ["o"], "ms": 40}, {"rows": ["."], "ms": 31}],
        "motion": {"vx": 0.0},
    });
    scene.add(1, spec(sprite), now).unwrap();
    scene.tick(now, &mock);
    assert_eq!(scene.next_wake(), Some(now + ms(40)));
}

#[test]
fn animations_at_rest_end_their_frames_together() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    // Alone, an animation is woken at the end of each frame.
    let frames = json!([{"rows": ["o"], "ms": 40}, {"rows": ["."], "ms": 31}]);
    scene
        .add(1, spec(json!({"frames": frames, "loop": true})), ms(0))
        .unwrap();
    scene.tick(ms(0), &mock);
    for end in [40, 71, 111, 142] {
        assert_eq!(scene.next_wake(), Some(ms(end)));
        scene.tick(ms(end), &mock);
        assert!(scene.take_dirty());
    }
    // Many, out of phase: a wake half a step after the last at the soonest.
    for i in 0..40u64 {
        let now = ms(142 + i);
        scene
            .add(
                1,
                spec(json!({"frames": frames, "loop": true, "ttl_ms": 900 + i})),
                now,
            )
            .unwrap();
        scene.tick(now, &mock);
    }
    let (mut now, mut ticks) = (ms(181), 0);
    while now < ms(1181) {
        let wake = scene.next_wake().unwrap();
        assert!(wake > now, "{wake:?} at {now:?}");
        now = wake;
        scene.tick(now, &mock);
        ticks += 1;
    }
    assert!((50..=61).contains(&ticks), "{ticks} ticks in a second");
    assert_eq!(scene.list().len(), 1);
    // A tick that comes sooner ends what is due.
    let brief = json!({"content": {"rows": ["x"]}, "ttl_ms": 3});
    scene.add(1, spec(brief), now).unwrap();
    scene.tick(now + ms(3), &mock);
    assert_eq!(scene.list().len(), 1);
}

#[test]
fn motions_share_one_clock() {
    let mock = Mock::new();
    let mut scene = Scene::new();
    for (i, at) in [0u64, 7, 19].into_iter().enumerate() {
        let v = 40.0 + 20.0 * i as f64;
        let sprite = json!({"content": {"rows": ["o"]}, "motion": {"vx": v, "bounce": true}});
        scene.add(1, spec(sprite), ms(at)).unwrap();
    }
    scene.tick(ms(20), &mock);
    scene.take_dirty();
    let mut repaints = 0;
    for t in (21..=1020).step_by(3) {
        scene.tick(ms(t), &mock);
        let wake = scene.next_wake().unwrap();
        assert!(
            wake > ms(t) && wake <= ms(t) + MOTION_STEP,
            "{wake:?} at {t}"
        );
        repaints += scene.take_dirty() as u32;
    }
    assert!(
        (25..=31).contains(&repaints),
        "{repaints} repaints in a second"
    );
}

/// A row of `content.grid`: 16-byte cells, base64.
fn grid_row(cells: &[(u32, [u8; 4], [u8; 4], u16)]) -> String {
    let mut bytes = Vec::new();
    for &(v, fg, bg, attrs) in cells {
        bytes.extend(v.to_le_bytes());
        bytes.extend(fg);
        bytes.extend(bg);
        bytes.extend(attrs.to_le_bytes());
        bytes.extend([0, 0]);
    }
    let mut s = String::new();
    grid::encode(&bytes, &mut s);
    s
}

#[test]
fn grid_content_and_rows() {
    let mock = Mock::new();
    let mut base = base();
    for x in 0..20 {
        base.put(x, 0, Cell::narrow("=", Style::default()));
        base.put(x, 1, Cell::narrow("=", Style::default()));
    }
    let none = [0; 4];
    let red = [2, 1, 0, 0];
    let rgb = [3, 1, 2, 3];
    // A hole, a space without bg (a hole), a space with bg, a control
    // character (a space, no bg: a hole), a wide glyph and the cell it
    // covers, a combining mark (a space), a hidden one (a hole), a wide
    // glyph in the last column (a space).
    let row0 = grid_row(&[
        (0, red, red, 0),
        ('a' as u32, red, none, 1 | 16),
        (' ' as u32, none, none, 0),
        (' ' as u32, none, rgb, 0),
        (0x1b, red, none, 0),
        ('世' as u32, none, none, 0),
        ('x' as u32, none, none, 0),
        (0x301, none, none, 0),
        (0x202e, none, none, 0),
        (0xD800, none, rgb, 0),
        ('界' as u32, none, none, 0),
    ]);
    let row1 = grid_row(&[('b' as u32, none, none, 0)]);
    let mut scene = Scene::new();
    let id = scene
        .add(
            1,
            spec(json!({"anchor": {"at": [0, 0]}, "content": {"grid": [row0, row1]}, "layer": "front"})),
            ms(0),
        )
        .unwrap();
    scene.tick(ms(0), &mock);
    let out = scene.compose(&base, &mock);
    assert_eq!(out.row_text(0), "=a= =世== ==========");
    assert_eq!(out.row_text(1), "b===================");
    let a = out.cell(1, 0).style;
    assert_eq!(a.fg, Color::Indexed(1));
    assert!(a.attrs.contains(Attrs::BOLD | Attrs::INVERSE));
    assert_eq!(out.cell(3, 0).style.bg, Color::Rgb(1, 2, 3));
    assert_eq!(out.cell(5, 0).width, 2);

    // A row alone: the other kept; a short row leaves holes.
    let patch = |v| serde_json::from_value::<SpritePatch>(v).unwrap();
    let row = grid_row(&[('c' as u32, none, none, 0), ('d' as u32, none, none, 0)]);
    scene
        .update(id, patch(json!({"rows": {"0": row}})), ms(10))
        .unwrap();
    scene.tick(ms(10), &mock);
    let out = scene.compose(&base, &mock);
    assert_eq!(out.row_text(0), "cd==================");
    assert_eq!(out.row_text(1), "b===================");

    // Refused, nothing applied: past the last row, wider, not base64,
    // with content.
    let wide = grid_row(&[('e' as u32, none, none, 0); 12]);
    for bad in [
        json!({"rows": {"2": row1.clone()}}),
        json!({"rows": {"0": wide}}),
        json!({"rows": {"0": "AAA"}}),
        json!({"rows": {"1": row1.clone()}, "content": {"rows": ["z"]}}),
        json!({"rows": {"0": row1.clone(), "9": row1.clone()}}),
    ] {
        let err = scene.update(id, patch(bad.clone()), ms(20)).unwrap_err();
        assert_eq!(err.code(), "bad-params", "{bad}");
    }
    scene.tick(ms(20), &mock);
    assert_eq!(
        scene.compose(&base, &mock).row_text(0),
        "cd=================="
    );

    // Only a sprite showing a content takes rows.
    let anim = scene
        .add(
            1,
            spec(json!({"frames": [{"rows": ["a"]}, {"rows": ["b"]}]})),
            ms(0),
        )
        .unwrap();
    let err = scene.update(anim, patch(json!({"rows": {"0": row1}})), ms(20));
    assert_eq!(err.unwrap_err().code(), "bad-params");
    // Exclusive with rows and cells.
    let both = spec(json!({"content": {"grid": [], "rows": ["a"]}}));
    assert_eq!(scene.add(1, both, ms(0)).unwrap_err().code(), "bad-params");
}

/// A composition kept across frames (`compose_with`) equals a whole one
/// after any sequence of edits: of the scene, the app grid, the frames,
/// the cursor, the host colours, the clock.
#[test]
fn kept_compose_equals_whole() {
    let seed = std::cell::Cell::new(0x2545_f491_4f6c_dd1du64);
    let rnd = |n: u64| {
        let mut s = seed.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        seed.set(s);
        s % n
    };
    let texts = [
        "ab",
        "世x",
        "  z ",
        "e\u{301}f",
        "█▓",
        "",
        "x\u{7}y",
        "界界界",
    ];
    let colors = ["red", "#102030", "default", "blue"];
    for round in 0..24 {
        let mut mock = Mock::new();
        let mut grid = Grid::new(20, 6);
        let mut scene = Scene::new();
        let mut kept = Composer::new();
        let mut ids: Vec<SpriteId> = Vec::new();
        let mut now = 0u64;
        for step in 0..300 {
            let style = || {
                let mut s = serde_json::Map::new();
                if rnd(2) == 0 {
                    s.insert("fg".into(), colors[rnd(4) as usize].into());
                }
                if rnd(2) == 0 {
                    s.insert("bg".into(), colors[rnd(4) as usize].into());
                }
                if rnd(4) == 0 {
                    s.insert("inverse".into(), true.into());
                }
                serde_json::Value::Object(s)
            };
            let content = || match rnd(3) {
                0 => json!({"rows": [texts[rnd(8) as usize], texts[rnd(8) as usize]],
                            "opaque": rnd(2) == 0}),
                1 => json!({"cells": [[{"t": texts[rnd(8) as usize], "style": style()}],
                                      [{"t": texts[rnd(8) as usize]}]]}),
                _ => {
                    let mut rows = Vec::new();
                    for _ in 0..1 + rnd(3) {
                        let mut b = vec![0u8; (1 + rnd(5) as usize) * grid::CELL];
                        for c in b.chunks_exact_mut(grid::CELL) {
                            let v = [0, 0x41, 0x20, 0x4e16, 0x301, 7][rnd(6) as usize] as u32;
                            c[..4].copy_from_slice(&v.to_le_bytes());
                            c[4] = rnd(4) as u8;
                            c[5] = rnd(256) as u8;
                            c[8] = rnd(4) as u8;
                            c[9] = rnd(256) as u8;
                            c[12] = rnd(32) as u8;
                        }
                        let mut s = String::new();
                        grid::encode_row(&b, rnd(2) == 0, &mut s);
                        rows.push(s);
                    }
                    json!({"grid": rows, "opaque": rnd(2) == 0})
                }
            };
            let place = || {
                let mut v = serde_json::Map::new();
                let at = [rnd(24) as i64 - 3, rnd(9) as i64 - 3];
                v.insert(
                    "anchor".into(),
                    match rnd(4) {
                        0 => json!({"at": at, "space": "view"}),
                        1 => json!({"cursor": {"dx": at[0] % 3, "dy": at[1] % 2}}),
                        _ => json!({"at": at}),
                    },
                );
                v.insert(
                    "frame".into(),
                    if rnd(2) == 0 {
                        json!("raw")
                    } else {
                        json!(PANE)
                    },
                );
                v.insert("layer".into(), [-2, -1, 1, 2][rnd(4) as usize].into());
                v.insert("z".into(), (rnd(3) as i32).into());
                if rnd(3) == 0 {
                    v.insert("blend".into(), "tint".into());
                }
                if rnd(3) == 0 {
                    v.insert("alpha".into(), 0.5.into());
                }
                if rnd(6) == 0 {
                    v.insert("panel".into(), "blue".into());
                }
                v
            };
            match rnd(14) {
                0 | 1 => {
                    let mut v = place();
                    if rnd(5) == 0 {
                        // Frames of one size: only the clock tells.
                        v.insert(
                            "frames".into(),
                            json!([{"rows": ["ab", "c "], "ms": 30},
                                   {"rows": ["xy", "z "], "ms": 50}]),
                        );
                    } else if rnd(5) == 0 {
                        v.insert("frames".into(), json!([content(), content()]));
                    } else if rnd(6) == 0 {
                        v.insert("effect".into(), json!({"name": "sparkle"}));
                        v.insert("size".into(), json!([4, 2]));
                    } else {
                        v.insert("content".into(), content());
                    }
                    if rnd(5) == 0 {
                        v.insert("ttl_ms".into(), (rnd(200) + 1).into());
                    }
                    if rnd(6) == 0 {
                        v.insert(
                            "motion".into(),
                            json!({"vx": 20.0, "vy": 10.0, "bounce": true}),
                        );
                    }
                    let owner = rnd(2) + 1;
                    if let Ok(id) = scene.add(owner, spec(serde_json::Value::Object(v)), ms(now)) {
                        ids.push(id);
                    }
                }
                2 | 3 if !ids.is_empty() => {
                    let id = ids[rnd(ids.len() as u64) as usize];
                    let mut v = serde_json::Map::new();
                    match rnd(4) {
                        0 => v = place(),
                        1 => {
                            v.insert("content".into(), content());
                        }
                        _ => {
                            let mut b = vec![0u8; (1 + rnd(3) as usize) * grid::CELL];
                            for c in b.chunks_exact_mut(grid::CELL) {
                                c[0] = [0, b'q', b' '][rnd(3) as usize];
                                c[4] = 3;
                                c[5] = rnd(256) as u8;
                            }
                            let mut s = String::new();
                            grid::encode_row(&b, true, &mut s);
                            v.insert("rows".into(), json!({rnd(2).to_string(): s}));
                        }
                    }
                    let patch: SpritePatch = serde_json::from_value(v.into()).unwrap();
                    let _ = scene.update(id, patch, ms(now));
                }
                4 if !ids.is_empty() => {
                    let id = ids.swap_remove(rnd(ids.len() as u64) as usize);
                    scene.remove(id);
                }
                5 => {
                    let text = texts[rnd(8) as usize];
                    let (x, y) = (rnd(20) as u16, rnd(6) as u16);
                    let st = Style {
                        bg: [Color::Default, Color::Indexed(4), Color::Rgb(1, 2, 3)]
                            [rnd(3) as usize],
                        attrs: {
                            let mut a = Attrs::default();
                            if rnd(4) == 0 {
                                a.insert(Attrs::INVERSE);
                            }
                            a
                        },
                        ..Style::default()
                    };
                    let mut col = x;
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        let w = ptyrc_vt::char_width(ch) as u8;
                        if w > 0 && col < 20 {
                            grid.put(col, y, Cell::glyph(ch, w, st));
                            col += w as u16;
                        }
                    }
                }
                6 => grid.scroll_up(0, 6, 1, &Cell::default()),
                7 => mock.cursor = (rnd(20) as u16, rnd(6) as u16),
                8 => {
                    let pane = mock.pane();
                    match rnd(3) {
                        0 => pane.origin.y += rnd(3) as i64 - 1,
                        1 => pane.rect = Rect::new(rnd(12) as u16, rnd(3) as u16, 8, 3),
                        _ => pane.usable = !pane.usable,
                    }
                }
                9 => {
                    let owner = rnd(2) + 1;
                    let hidden = scene.owner_hidden(owner);
                    scene.hide_owner(owner, !hidden, ms(now));
                }
                10 if rnd(8) == 0 => scene.set_host_colors(HostColors {
                    fg: [rnd(256) as u8, 1, 2],
                    bg: [3, rnd(256) as u8, 5],
                }),
                11 if rnd(10) == 0 => {
                    scene.clear(Some(rnd(2) + 1));
                }
                _ => {}
            }
            now += rnd(50);
            if rnd(3) > 0 {
                scene.tick_on(ms(now), &grid, &mock);
            }
            let whole = scene.compose(&grid, &mock);
            let again = scene.compose_with(&grid, &mock, &mut kept);
            assert!(
                *again == whole,
                "round {round}, step {step}:\n{:?}\n{:?}",
                again.text(),
                whole.text()
            );
        }
    }
}

/// What changed is composed again, and no more.
#[test]
fn kept_compose_redoes_the_rows_that_changed() {
    let mock = Mock::new();
    let mut grid = Grid::new(20, 6);
    let mut scene = Scene::new();
    let mut kept = Composer::new();
    let row = |c: u8| {
        let mut b = vec![0u8; 4 * grid::CELL];
        b.chunks_exact_mut(grid::CELL).for_each(|cell| cell[0] = c);
        let mut s = String::new();
        grid::encode_row(&b, false, &mut s);
        s
    };
    let content = json!({"grid": [row(b'a'), row(b'b'), row(b'c')]});
    let id = scene
        .add(
            1,
            spec(json!({"anchor": {"at": [2, 1]}, "frame": "raw", "content": content})),
            ms(0),
        )
        .unwrap();
    let mut step = |scene: &mut Scene, grid: &Grid| {
        scene.tick_on(ms(0), grid, &mock);
        scene.compose_with(grid, &mock, &mut kept);
        assert!(*kept.grid() == scene.compose(grid, &mock));
        kept.recomposed()
    };
    assert_eq!(step(&mut scene, &grid), 6);
    assert_eq!(step(&mut scene, &grid), 0);
    let patch = serde_json::from_value(json!({"rows": {"1": row(b'x')}})).unwrap();
    scene.update(id, patch, ms(0)).unwrap();
    assert_eq!(step(&mut scene, &grid), 1);
    assert_eq!(scene.compose(&grid, &mock).row_text(2).trim_end(), "  xxxx");
    grid.put(0, 5, Cell::narrow("q", Style::default()));
    assert_eq!(step(&mut scene, &grid), 1);
    // Moved: where it was and where it is.
    let patch = serde_json::from_value(json!({"anchor": {"at": [2, 2]}})).unwrap();
    scene.update(id, patch, ms(0)).unwrap();
    assert_eq!(step(&mut scene, &grid), 4);
    scene.remove(id);
    assert_eq!(step(&mut scene, &grid), 3);
}

/// A sprite holds `MAX_CELLS` at most: width times height, the widest
/// row setting the width.
#[test]
fn sprite_cells_are_capped() {
    let mut scene = Scene::new();
    let long = "x".repeat(4096);
    let mut rows = vec![String::new(); 256];
    rows[0] = long.clone();
    scene
        .add(1, spec(json!({"content": {"rows": rows}})), ms(0))
        .unwrap();
    rows.push(String::new());
    let e = scene
        .add(1, spec(json!({"content": {"rows": rows}})), ms(0))
        .unwrap_err();
    assert!(e.to_string().contains("4096x257 is 1052672 cells"), "{e}");
    // Repeats: a few bytes a row, a large canvas.
    let mut cell = vec![0u8; grid::CELL];
    cell[0] = b'x';
    cell[14..16].copy_from_slice(&4095u16.to_le_bytes());
    let mut one = String::new();
    grid::encode(&cell, &mut one);
    let rows = vec![one; 60_000];
    let e = scene
        .add(1, spec(json!({"content": {"grid": rows}})), ms(0))
        .unwrap_err();
    assert!(e.to_string().contains("over 1048576 a sprite"), "{e}");
    let e = scene.add(
        1,
        spec(json!({"content": {"cells": [[{"t": long}], [], [], []]}, "frames": null})),
        ms(0),
    );
    assert!(e.is_ok());
    let e = scene
        .add(
            1,
            spec(json!({"effect": {"name": "sparkle"}, "size": [4096, 4096]})),
            ms(0),
        )
        .unwrap_err();
    assert!(e.to_string().starts_with("size: 4096x4096"), "{e}");
}
